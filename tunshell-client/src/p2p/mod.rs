use crate::TunnelStream;
use anyhow::Result;
use async_trait::async_trait;
use tunshell_shared::{AttemptDirectConnectPayload, PeerJoinedPayload};

pub mod tcp;
pub mod udp;
pub mod udp_adaptor;

pub(super) const DIRECT_CONNECT_TIMEOUT: u32 = 3000; // ms

#[async_trait]
pub trait P2PConnection: TunnelStream {
    fn new(peer_info: PeerJoinedPayload, connection_info: AttemptDirectConnectPayload) -> Self
    where
        Self: Sized;

    async fn bind(&mut self) -> Result<()>;

    async fn connect(&mut self, master_side: bool) -> Result<()>;
}
