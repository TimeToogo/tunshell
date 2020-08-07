use crate::TunnelStream;
use anyhow::Result;
use async_trait::async_trait;
use tunshell_shared::{PeerJoinedPayload};

pub mod tcp;
pub mod udp;
pub mod udp_adaptor;

#[async_trait]
pub trait P2PConnection: TunnelStream {
    fn new(peer_info: PeerJoinedPayload) -> Self
    where
        Self: Sized;

    async fn bind(&mut self) -> Result<u16>;

    async fn connect(&mut self, peer_port: u16, master_side: bool) -> Result<()>;
}
