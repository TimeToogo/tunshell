use super::UdpPacket;

#[derive(Debug, PartialEq)]
pub(super) enum SendEvent {
    Send(UdpPacket),
    Resend(UdpPacket),
}

pub(super) struct SendOrchestrator {}
