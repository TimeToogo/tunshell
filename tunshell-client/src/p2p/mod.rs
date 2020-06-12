use crate::TunnelStream;
use anyhow::Result;
use async_trait::async_trait;
use tunshell_shared::{AttemptDirectConnectPayload, PeerJoinedPayload};

mod tcp;

pub use tcp::*;

#[async_trait]
pub trait P2PConnection: TunnelStream {
    async fn connect(
        peer_info: &PeerJoinedPayload,
        connection_info: &AttemptDirectConnectPayload,
    ) -> Result<Self>
    where
        Self: Sized;
}
