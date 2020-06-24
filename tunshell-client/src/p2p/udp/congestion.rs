use super::{UdpConnectionVars, UdpPacket};
use std::task::{Context, Poll, Waker};

impl UdpConnectionVars {
    pub fn poll_can_send(&mut self, cx: &Context, packet: &UdpPacket) -> Poll<()> {
        if self.can_send(packet) {
            Poll::Ready(())
        } else {
            self.window_wakers.push(cx.waker().clone());
            Poll::Pending
        }
    }

    pub fn can_send(&self, packet: &UdpPacket) -> bool {
        todo!()
    }

    pub(super) fn handle_recv_packet(&mut self, packet: &UdpPacket) {
        todo!()
    }

    pub(super) fn handle_send_packet(&mut self, packet: &UdpPacket) {
        todo!()
    }
}
