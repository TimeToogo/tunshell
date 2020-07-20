use super::udp::{UdpConnectParams, UdpConnection, UdpConnectionConfig};
use crate::p2p::{P2PConnection, DIRECT_CONNECT_TIMEOUT};
use crate::TunnelStream;
use anyhow::{Error, Result};
use async_trait::async_trait;
use log::*;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::delay_for;
use tunshell_shared::{AttemptDirectConnectPayload, PeerJoinedPayload};

pub struct UdpConnectionAdaptor {
    peer_info: PeerJoinedPayload,
    connection_info: AttemptDirectConnectPayload,
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
    fn new(peer_info: PeerJoinedPayload, connection_info: AttemptDirectConnectPayload) -> Self {
        let config = UdpConnectionConfig::default()
            .with_bind_addr(SocketAddr::from((
                [0, 0, 0, 0],
                connection_info.self_listen_port,
            )))
            .with_connect_timeout(Duration::from_millis(DIRECT_CONNECT_TIMEOUT as u64));

        Self {
            peer_info,
            connection_info,
            con: UdpConnection::new(config),
        }
    }

    async fn bind(&mut self) -> Result<()> {
        self.con.bind().await
    }

    async fn connect(&mut self, master_side: bool) -> Result<()> {
        let connect_future = self.con.connect(UdpConnectParams {
            ip: self.peer_info.peer_ip_address.parse::<IpAddr>().unwrap(),
            suggested_port: self.connection_info.peer_listen_port,
            master_side,
        });

        let result = tokio::select! {
            result = connect_future => result,
            _ = delay_for(Duration::from_millis(DIRECT_CONNECT_TIMEOUT as u64)) => {
                info!("timed out while attempting UDP connection");
                return Err(Error::msg("UDP connection timed out"));
            }
        };

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::runtime::Runtime;

    #[test]
    fn test_connect_timeout() {
        Runtime::new().unwrap().block_on(async {
            let peer_info = PeerJoinedPayload {
                peer_ip_address: "127.0.0.1".to_owned(),
                peer_key: "test".to_owned(),
            };
            let mut connection1 = UdpConnectionAdaptor::new(
                peer_info.clone(),
                AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22554,
                    self_listen_port: 22555,
                    session_salt: vec![1, 2, 3],
                    session_key: vec![4, 5, 6],
                },
            );

            connection1.bind().await.expect("failed to bind");

            let (_, result) = futures::join!(
                delay_for(Duration::from_millis((DIRECT_CONNECT_TIMEOUT + 100) as u64)),
                connection1.connect(true)
            );

            assert!(result.is_err());
        });
    }

    #[test]
    fn test_connect_simultaneous_open() {
        Runtime::new().unwrap().block_on(async {
            let peer_info = PeerJoinedPayload {
                peer_ip_address: "127.0.0.1".to_owned(),
                peer_key: "test".to_owned(),
            };

            let mut connection1 = UdpConnectionAdaptor::new(
                peer_info.clone(),
                AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22664,
                    self_listen_port: 22665,
                    session_salt: vec![1, 2, 3],
                    session_key: vec![4, 5, 6],
                },
            );

            let mut connection2 = UdpConnectionAdaptor::new(
                peer_info.clone(),
                AttemptDirectConnectPayload {
                    connect_at: 1,
                    peer_listen_port: 22665,
                    self_listen_port: 22664,
                    session_salt: vec![1, 2, 3],
                    session_key: vec![4, 5, 6],
                },
            );

            connection1.bind().await.expect("failed to bind");
            connection2.bind().await.expect("failed to bind");

            futures::try_join!(connection1.connect(true), connection2.connect(false))
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
