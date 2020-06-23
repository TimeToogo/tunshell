use super::{UdpConnectionConfig, UdpPacket};
use std::task::{Poll, Waker};

#[derive(Debug)]
pub(super) struct CongestionControl {
    /// The amount of bytes available in the peer's buffers to receive packets.
    /// This will be the window value of the packet received with the highest sequence number.
    peer_window: u32,

    /// The number of bytes permitted to be in-flight and not yet acknowledged by the peer.
    /// If this number falls near zero, packets will not be permitted to be sent outbound.
    transit_window: u32,

    /// Task wakers which are waiting for the window to grow
    /// allowing for another packet to be sent.
    window_wakers: Vec<Waker>,
}

impl CongestionControl {
    pub(super) fn new(config: &UdpConnectionConfig) -> Self {
        Self {
            peer_window: 0,
            transit_window: config.initial_transit_window(),
            window_wakers: vec![],
        }
    }

    pub fn until_can_send(self, packet: UdpPacket) -> impl futures::future::Future {
        let mut this = Some(self);
        let mut packet = Some(packet);

        futures::future::poll_fn(move |cx| {
            if this.as_mut().unwrap().can_send(&packet.as_ref().unwrap()) {
                let this = this.take().unwrap();
                let packet = packet.take().unwrap();
                Poll::Ready((this, packet))
            } else {
                let this = this.as_mut().unwrap();
                this.window_wakers.push(cx.waker().clone());
                Poll::Pending
            }
        })
    }

    pub fn can_send(&mut self, packet: &UdpPacket) -> bool {
        todo!()
    }

    pub(super) fn handle_recv_packet(&mut self, packet: &UdpPacket) {
        todo!()
    }

    pub(super) fn handle_send_packet(&mut self, packet: &UdpPacket) {
        todo!()
    }
}
