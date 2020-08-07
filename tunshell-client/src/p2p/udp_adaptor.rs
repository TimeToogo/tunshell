use super::udp::{UdpConnectParams, UdpConnection, UdpConnectionConfig};
use crate::p2p::P2PConnection;
use crate::TunnelStream;
use anyhow::Result;
use async_trait::async_trait;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tunshell_shared::PeerJoinedPayload;

pub struct UdpConnectionAdaptor {
    peer_info: PeerJoinedPayload,
    con: UdpConnection,
}

impl AsyncRead for UdpConnectionAdaptor {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &mut [u8],
    ) -> Poll<std::result::Result<usize, std::io::Error>> {
        Pin::new(&mut self.con).poll_read(cx, buff)
    }
}

impl AsyncWrite for UdpConnectionAdaptor {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &[u8],
    ) -> Poll<std::result::Result<usize, std::io::Error>> {
        Pin::new(&mut self.con).poll_write(cx, buff)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.con).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.con).poll_shutdown(cx)
    }
}

impl TunnelStream for UdpConnectionAdaptor {}

#[async_trait]
impl P2PConnection for UdpConnectionAdaptor {
    fn new(peer_info: PeerJoinedPayload) -> Self {
        let config =
            UdpConnectionConfig::default().with_bind_addr(SocketAddr::from(([0, 0, 0, 0], 0)));

        Self {
            peer_info,
            con: UdpConnection::new(config),
        }
    }

    async fn bind(&mut self) -> Result<u16> {
        self.con.bind().await
    }

    async fn connect(&mut self, peer_port: u16, master_side: bool) -> Result<()> {
        let connect_future = self.con.connect(UdpConnectParams {
            ip: self.peer_info.peer_ip_address.parse::<IpAddr>().unwrap(),
            suggested_port: peer_port,
            master_side,
        });

        connect_future.await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::{time::delay_for, runtime::Runtime};
    use std::time::Duration;

    #[test]
    fn test_connect_simultaneous_open() {
        Runtime::new().unwrap().block_on(async {
            let peer_info = PeerJoinedPayload {
                peer_ip_address: "127.0.0.1".to_owned(),
                peer_key: "test".to_owned(),
                session_nonce: "nonce".to_owned(),
            };

            let mut connection1 = UdpConnectionAdaptor::new(peer_info.clone());

            let mut connection2 = UdpConnectionAdaptor::new(peer_info.clone());

            let port1 = connection1.bind().await.expect("failed to bind");
            let port2 = connection2.bind().await.expect("failed to bind");

            futures::try_join!(
                connection1.connect(port2, true),
                connection2.connect(port1, false)
            )
            .expect("failed to connect");

            connection1.write("hello from 1".as_bytes()).await.unwrap();
            connection1.flush().await.unwrap();

            delay_for(Duration::from_millis(50)).await;

            let mut buff = [0; 1024];
            let read = connection2.read(&mut buff).await.unwrap();

            assert_eq!(
                String::from_utf8(buff[..read].to_vec()).unwrap(),
                "hello from 1"
            );

            connection2.write("hello from 2".as_bytes()).await.unwrap();
            connection2.flush().await.unwrap();

            delay_for(Duration::from_millis(50)).await;
            let mut buff = [0; 1024];
            let read = connection1.read(&mut buff).await.unwrap();

            assert_eq!(
                String::from_utf8(buff[..read].to_vec()).unwrap(),
                "hello from 2"
            );
        });
    }
}
