use crate::p2p::{P2PConnection, DIRECT_CONNECT_TIMEOUT};
use crate::TunnelStream;
use anyhow::{Error, Result};
use async_trait::async_trait;
use futures::future::pending;
use futures::TryFutureExt;
use log::*;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::delay_for;
use tunshell_shared::{AttemptDirectConnectPayload, PeerJoinedPayload};

pub struct TcpConnection {
    peer_info: PeerJoinedPayload,
    connection_info: AttemptDirectConnectPayload,
    listener: Option<TcpListener>,
    socket: Option<TcpStream>,
}

impl AsyncRead for TcpConnection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &mut [u8],
    ) -> Poll<std::result::Result<usize, std::io::Error>> {
        Pin::new(&mut self.socket.as_mut().unwrap()).poll_read(cx, buff)
    }
}

impl AsyncWrite for TcpConnection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &[u8],
    ) -> Poll<std::result::Result<usize, std::io::Error>> {
        Pin::new(&mut self.socket.as_mut().unwrap()).poll_write(cx, buff)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.socket.as_mut().unwrap()).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.socket.as_mut().unwrap()).poll_shutdown(cx)
    }
}

impl TunnelStream for TcpConnection {}

#[async_trait]
impl P2PConnection for TcpConnection {
    fn new(peer_info: PeerJoinedPayload, connection_info: AttemptDirectConnectPayload) -> Self {
        Self {
            peer_info,
            connection_info,
            listener: None,
            socket: None,
        }
    }

    async fn bind(&mut self) -> Result<()> {
        let listener = TcpListener::bind(SocketAddr::from((
            [0, 0, 0, 0],
            self.connection_info.self_listen_port,
        )))
        .await?;

        self.listener.replace(listener);

        Ok(())
    }

    async fn connect(&mut self, _master_side: bool) -> Result<()> {
        assert!(self.listener.is_some());

        info!(
            "Attempting to connect to {} via TCP",
            self.peer_info.peer_ip_address
        );

        let peer_addr = (
            self.peer_info.peer_ip_address.as_str(),
            self.connection_info.peer_listen_port,
        )
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();

        let connect_future =
            TcpStream::connect(peer_addr).or_else(|_| pending::<std::io::Result<TcpStream>>());

        let listen_future = self
            .listener
            .as_mut()
            .unwrap()
            .accept()
            .or_else(|_| pending::<std::io::Result<(TcpStream, SocketAddr)>>());

        let result = tokio::select! {
            result = connect_future => result.map(|socket| (socket, peer_addr)),
            result = listen_future => result,
            _ = delay_for(Duration::from_millis(DIRECT_CONNECT_TIMEOUT as u64)) => {
                info!("timed out while attempting TCP connection");
                return Err(Error::msg("TCP connection timed out"));
            }
        };

        if let Ok((socket, peer_addr)) = result {
            let connected_ip = self.peer_info.peer_ip_address.parse::<IpAddr>().unwrap();

            if peer_addr.ip() == connected_ip {
                self.socket.replace(socket);
                return Ok(());
            } else {
                error!("received connection for unknown ip address: {}", peer_addr);
            }
        }

        Err(Error::msg("Direct TCP connection failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use futures::TryFutureExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::runtime::Runtime;

    #[test]
    fn test_connect_via_connect() {
        Runtime::new().unwrap().block_on(async {
            let mut listener = TcpListener::bind("0.0.0.0:22335".to_owned())
                .await
                .expect("failed listen for connection");

            let mut connection1 = TcpConnection::new(
                PeerJoinedPayload {
                    peer_ip_address: "127.0.0.1".to_owned(),
                    peer_key: "test".to_owned(),
                },
                AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22335,
                    self_listen_port: 22334,
                },
            );

            connection1.bind().await.expect("failed to bind");
            connection1.connect(false).await.expect("failed to connect");

            let (mut socket, _) = listener
                .accept()
                .await
                .expect("failed to accept connection");

            connection1
                .write("hello".as_bytes())
                .await
                .expect("failed to write to socket");

            let mut buff = [0; 1024];
            let read = socket
                .read(&mut buff)
                .await
                .expect("failed to read from socket");

            assert_eq!(String::from_utf8(buff[..read].to_vec()).unwrap(), "hello");

            socket
                .write("hi".as_bytes())
                .await
                .expect("failed to write to socket");

            let read = connection1
                .read(&mut buff)
                .await
                .expect("failed to read from socket");

            assert_eq!(String::from_utf8(buff[..read].to_vec()).unwrap(), "hi");
        });
    }

    #[test]
    fn test_connect_via_listener() {
        Runtime::new().unwrap().block_on(async {
            let peer_info = PeerJoinedPayload {
                peer_ip_address: "127.0.0.1".to_owned(),
                peer_key: "test".to_owned(),
            };
            let mut connection1 = TcpConnection::new(
                peer_info.clone(),
                AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22444,
                    self_listen_port: 22445,
                },
            );

            connection1.bind().await.expect("failed to bind");

            let socket = delay_for(Duration::from_millis(100))
                .then(|_| TcpStream::connect("127.0.0.1:22445"))
                .or_else(|err| futures::future::err(Error::new(err)));

            let (_, mut socket) =
                futures::try_join!(connection1.connect(false), socket).expect("failed to connect");

            socket.write("hello".as_bytes()).await.unwrap();

            let mut buff = [0; 1024];
            let read = connection1.read(&mut buff).await.unwrap();

            assert_eq!(String::from_utf8(buff[..read].to_vec()).unwrap(), "hello");

            connection1.write("hi".as_bytes()).await.unwrap();

            let read = socket.read(&mut buff).await.unwrap();

            assert_eq!(String::from_utf8(buff[..read].to_vec()).unwrap(), "hi");
        });
    }

    #[test]
    fn test_connect_timeout() {
        Runtime::new().unwrap().block_on(async {
            let peer_info = PeerJoinedPayload {
                peer_ip_address: "127.0.0.1".to_owned(),
                peer_key: "test".to_owned(),
            };
            let mut connection1 = TcpConnection::new(
                peer_info.clone(),
                AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22554,
                    self_listen_port: 22555,
                },
            );

            connection1.bind().await.expect("failed to bind");

            let (_, result) = futures::join!(
                delay_for(Duration::from_millis((DIRECT_CONNECT_TIMEOUT + 100) as u64)),
                connection1.connect(false)
            );

            assert!(result.is_err());
        });
    }

    // #[test]
    // fn test_connect_simultaneous_open() {
    //     Runtime::new().unwrap().block_on(async {
    //         let peer_info = PeerJoinedPayload {
    //             peer_ip_address: "127.0.0.1".to_owned(),
    //             peer_key: "test".to_owned(),
    //         };

    //         let mut connection1 = TcpConnection::new(
    //             peer_info.clone(),
    //             AttemptDirectConnectPayload {
    //                 connect_at: 1,
    //                 peer_listen_port: 22664,
    //                 self_listen_port: 22665,
    //             },
    //         );

    //         let mut connection2 = TcpConnection::new(
    //             peer_info.clone(),
    //             AttemptDirectConnectPayload {
    //                 connect_at: 1,
    //                 peer_listen_port: 22665,
    //                 self_listen_port: 22664,
    //             },
    //         );

    //         connection1.bind().await.expect("failed to bind");
    //         connection2.bind().await.expect("failed to bind");

    //         futures::try_join!(connection1.connect(false), connection2.connect(false))
    //             .expect("failed to connect");

    //         connection1.write("hello from 1".as_bytes()).await.unwrap();
    //         connection1.flush().await.unwrap();

    //         delay_for(Duration::from_millis(50)).await;

    //         let mut buff = [0; 1024];
    //         let read = connection2.read(&mut buff).await.unwrap();

    //         assert_eq!(
    //             String::from_utf8(buff[..read].to_vec()).unwrap(),
    //             "hello from 1"
    //         );

    //         connection2.write("hello from 2".as_bytes()).await.unwrap();
    //         connection2.flush().await.unwrap();

    //         delay_for(Duration::from_millis(50)).await;

    //         let mut buff = [0; 1024];
    //         let read = connection1.read(&mut buff).await.unwrap();

    //         assert_eq!(
    //             String::from_utf8(buff[..read].to_vec()).unwrap(),
    //             "hello from 2"
    //         );
    //     });
    // }
}
