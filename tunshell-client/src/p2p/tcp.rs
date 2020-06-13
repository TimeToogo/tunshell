use crate::P2PConnection;
use crate::TunnelStream;
use anyhow::{Error, Result};
use async_trait::async_trait;
use futures::TryFutureExt;
use log::*;
use std::pin::Pin;
use thrussh::Tcp;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tunshell_shared::{AttemptDirectConnectPayload, PeerJoinedPayload};

pub struct TcpConnection {
    socket: TcpStream,
}

impl AsyncRead for TcpConnection {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buff: &mut [u8],
    ) -> std::task::Poll<std::result::Result<usize, std::io::Error>> {
        Pin::new(&mut self.socket).poll_read(cx, buff)
    }
}

impl AsyncWrite for TcpConnection {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buff: &[u8],
    ) -> std::task::Poll<std::result::Result<usize, std::io::Error>> {
        Pin::new(&mut self.socket).poll_write(cx, buff)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.socket).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.socket).poll_shutdown(cx)
    }
}

impl Tcp for TcpConnection {}

impl TunnelStream for TcpConnection {}

#[async_trait]
impl P2PConnection for TcpConnection {
    async fn connect(
        peer_info: &PeerJoinedPayload,
        connection_info: &AttemptDirectConnectPayload,
    ) -> Result<Self> {
        info!(
            "Attempting to connect to {} via TCP",
            peer_info.peer_ip_address
        );

        let mut listener = TcpListener::bind(
            "0.0.0.0:".to_owned() + &connection_info.self_listen_port.to_string(),
        )
        .await?;

        let peer_addr =
            peer_info.peer_ip_address.clone() + ":" + &connection_info.peer_listen_port.to_string();
        let connect_future = TcpStream::connect(peer_addr)
            .or_else(|_| futures::future::pending::<std::io::Result<TcpStream>>());
        let listen_future = listener.accept().or_else(|_| {
            futures::future::pending::<std::io::Result<(TcpStream, std::net::SocketAddr)>>()
        });

        tokio::select! {
            socket = connect_future => if let Ok(socket) = socket {
                return Ok(TcpConnection { socket });
            } else {
                println!("{:?}", socket)
            },
            socket = listen_future => if let Ok((socket, _)) = socket {
                return Ok(TcpConnection { socket })
            },
            _ = tokio::time::delay_for(std::time::Duration::from_secs(3)) => {
                info!("timed out while attempting TCP connection");
            }
        };

        return Err(Error::msg("Direct TCP connection failed"));
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

            let mut connection1 = TcpConnection::connect(
                &PeerJoinedPayload {
                    peer_ip_address: "127.0.0.1".to_owned(),
                    peer_key: "test".to_owned(),
                },
                &AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22335,
                    self_listen_port: 22334,
                },
            )
            .await
            .expect("failed to connect");

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
            let connection1 = TcpConnection::connect(
                &peer_info,
                &AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22444,
                    self_listen_port: 22445,
                },
            );

            let socket = tokio::time::delay_for(std::time::Duration::from_millis(100))
                .then(|_| TcpStream::connect("127.0.0.1:22445"))
                .or_else(|err| futures::future::err(Error::new(err)));

            let (mut connection1, mut socket) =
                futures::try_join!(connection1, socket).expect("failed to connect");

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
            let connection1 = TcpConnection::connect(
                &peer_info,
                &AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22554,
                    self_listen_port: 22555,
                },
            );

            let (_, result) = futures::join!(
                tokio::time::delay_for(std::time::Duration::from_secs(5)),
                connection1
            );

            assert!(result.is_err());
        });
    }
}
