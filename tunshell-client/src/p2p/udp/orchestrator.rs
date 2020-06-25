use super::{UdpPacket, UdpConnectionVars, SequenceNumber};
use std::sync::{Arc, Mutex};

#[derive(Debug, PartialEq)]
pub(super) enum SendEvent {
    Send(UdpPacket),
    Resend(UdpPacket),
    AckUpdate(SequenceNumber),
    WindowUpdate(u32),
}

pub(super) struct SendOrchestrator {}

fn handle_recv_packet(con: Arc<Mutex<UdpConnectionVars>>, packet: &UdpPacket) {
    todo!();
}

fn handle_send_packet(con: Arc<Mutex<UdpConnectionVars>>, packet: &UdpPacket) {
    todo!();
}