use super::UdpPacket;

pub(super) enum SendEvent {
    Send(UdpPacket),
    Resend(UdpPacket),
}

pub(super) struct SendOrchestrator {}
