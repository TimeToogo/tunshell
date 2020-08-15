use super::{UdpConnectionVars, UdpPacket, MAX_PACKET_SIZE, SequenceNumber, UdpPacketType};
use log::*;
use std::cmp;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

impl UdpConnectionVars {
    fn poll_can_send(&mut self, cx: &Context, packet: &UdpPacket) -> Poll<()> {
        if packet.packet_type == UdpPacketType::Close {
            return Poll::Ready(());
        }

        if !self.can_send_sequence_number(packet.end_sequence_number()) {
            debug!(
                "packet is outside of window, waiting before sending packet [{}, {}] (peer ack: {}, peer window: {})",
                packet.sequence_number,
                packet.end_sequence_number(),
                self.peer_ack_number,
                self.peer_window
            );
            
            self.window_wakers
                .push((cx.waker().clone(), packet.end_sequence_number()));
            return Poll::Pending;
        }

        Poll::Ready(())
    }

    pub(super) fn can_send_sequence_number(&self, sequence_number: SequenceNumber) -> bool {
        sequence_number < self.max_send_sequence_number()
    }

    pub(super) fn max_send_sequence_number(&self ) -> SequenceNumber {
        self.peer_ack_number + SequenceNumber(cmp::min(self.peer_window, self.transit_window))
    }

    pub(super) fn is_congested(&self) -> bool {
        self.sequence_number >= self.max_send_sequence_number()
    }

    pub(super) fn wait_until_decongested(&mut self, waker: Waker) {
        self.window_wakers.push((waker, self.sequence_number + SequenceNumber(1)));
    }

    pub(super) fn increase_transit_window_after_send(&mut self) {
        // Each time a new packet successfully sends we gradually increase the transit window

        let packets_per_window = self.transit_window / MAX_PACKET_SIZE as u32;

        self.transit_window += MAX_PACKET_SIZE as u32 / packets_per_window;
        info!("transit window increased to {}", self.transit_window);
    }

    pub(super) fn decrease_transit_window_after_drop(&mut self) {
        // If a packet drops we decrease the transit window by 25%
        self.transit_window = cmp::max(MAX_PACKET_SIZE as u32, self.transit_window * 3 / 4);
        info!("transit window decreased to {}", self.transit_window);
    }

    pub(super) fn update_peer_window(&mut self, window: u32) {
        let original_window = self.peer_window;
        self.peer_window = window;
        if self.peer_window > original_window {
            // If the window has increased we will trigger pending send events that can be fired
            self.wake_pending_send_events();
        }
    }

    pub(super) fn wake_pending_send_events(&mut self) {
        let max_sequence_number = self.max_send_sequence_number();

        // Determine the number of packets we can now send form those are waiting.
        let (ready_wakers, pending_wakers) = self.window_wakers
            .drain(..)
            .partition(|(_, end_sequence_number)| end_sequence_number < &max_sequence_number);

        for (waker, _) in ready_wakers as Vec<(Waker, SequenceNumber)> {
            waker.wake();
        }

        self.window_wakers = pending_wakers;
    }
}

pub(super) async fn wait_until_can_send(
    con: Arc<Mutex<UdpConnectionVars>>,
    packet: UdpPacket,
) -> UdpPacket {
    futures::future::poll_fn(|cx| {
        let mut con = con.lock().unwrap();
        con.poll_can_send(cx, &packet)
    })
    .await;

    packet
}

#[cfg(test)]
mod tests {
    use super::super::{SequenceNumber, UdpConnectionConfig};
    use super::*;
    use std::time::Duration;
    use tokio::runtime::Runtime;

    #[test]
    fn test_can_send_sequence_number() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.peer_ack_number = SequenceNumber(100);
        con.peer_window = 10;

        assert_eq!(con.can_send_sequence_number(SequenceNumber(111)), false);
        assert_eq!(con.can_send_sequence_number(SequenceNumber(109)), true);
        assert_eq!(con.can_send_sequence_number(SequenceNumber(100)), true);
    }

    #[test]
    fn test_can_send_sequence_number_transit_window() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.peer_ack_number = SequenceNumber(100);
        con.transit_window = 5;
        con.peer_window = 10;

        assert_eq!(con.can_send_sequence_number(SequenceNumber(111)), false);
        assert_eq!(con.can_send_sequence_number(SequenceNumber(109)), false);
        assert_eq!(con.can_send_sequence_number(SequenceNumber(104)), true);
        assert_eq!(con.can_send_sequence_number(SequenceNumber(100)), true);
    }

    #[test]
    fn test_is_congested() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.sequence_number = SequenceNumber(100);
        con.peer_ack_number = SequenceNumber(100);
        con.peer_window = 10;

        assert_eq!(con.is_congested(), false);

        con.sequence_number = SequenceNumber(110);

        assert_eq!(con.is_congested(), true);
    }

    #[test]
    fn test_increase_transit_window_after_send() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.transit_window = MAX_PACKET_SIZE as u32;

        con.increase_transit_window_after_send();

        assert_eq!(con.transit_window, MAX_PACKET_SIZE as u32 * 2);

        con.increase_transit_window_after_send();

        assert_eq!(con.transit_window, MAX_PACKET_SIZE as u32 * 5 / 2);

        con.increase_transit_window_after_send();

        assert_eq!(con.transit_window, MAX_PACKET_SIZE as u32 * 3);

        con.increase_transit_window_after_send();
        con.increase_transit_window_after_send();
        con.increase_transit_window_after_send();

        assert_eq!(con.transit_window, MAX_PACKET_SIZE as u32 * 4);
    }

    #[test]
    fn test_decrease_transit_window_after_drop() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.transit_window = MAX_PACKET_SIZE as u32 * 10;

        con.decrease_transit_window_after_drop();

        assert_eq!(con.transit_window, MAX_PACKET_SIZE as u32 * 10 * 15 / 20);

        con.decrease_transit_window_after_drop();

        assert_eq!(
            con.transit_window,
            MAX_PACKET_SIZE as u32 * 10 * 15 / 20 * 15 / 20
        );

        con.transit_window = MAX_PACKET_SIZE as u32;
        con.decrease_transit_window_after_drop();

        assert_eq!(con.transit_window, MAX_PACKET_SIZE as u32);
    }

    #[test]
    fn test_update_peer_window() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.peer_window = 10;
        con.update_peer_window(20);

        assert_eq!(con.peer_window, 20);

        con.peer_window = 20;
        con.update_peer_window(10);

        assert_eq!(con.peer_window, 10);
    }

    #[test]
    fn test_wait_until_can_send_with_sufficient_window() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

            con.transit_window = MAX_PACKET_SIZE as u32;
            con.peer_window = MAX_PACKET_SIZE as u32;

            let con = Arc::from(Mutex::from(con));

            let packet = UdpPacket::data(SequenceNumber(0), SequenceNumber(0), 0, &[]);
            
            let packet = tokio::select! {
                packet = wait_until_can_send(Arc::clone(&con), packet) => packet,
                _ = tokio::time::delay_for(Duration::from_millis(1)) => panic!("should return from future immediately if there is sufficient window")
            };

            let con = con.lock().unwrap();

            assert_eq!(packet.sequence_number, SequenceNumber(0));
            assert_eq!(con.window_wakers.len(), 0);
        });
    }

    #[test]
    fn test_wait_until_can_send_without_sufficient_window() {
        // TODO: fix flaky test
        if std::env::var("CI").is_ok() {
            return;
        }
        
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

            con.transit_window = MAX_PACKET_SIZE as u32;
            con.peer_window = 0;

            let con = Arc::from(Mutex::from(con));

            let packet = UdpPacket::data(SequenceNumber(0), SequenceNumber(0), 0, &[]);
            
            let wait_for_send = tokio::spawn(wait_until_can_send(Arc::clone(&con), packet.clone()));

            tokio::time::delay_for(Duration::from_millis(100)).await;

            {
                let con = con.lock().unwrap();
                
                assert_eq!(con.window_wakers.len(), 1);
                assert_eq!(con.window_wakers[0].1, packet.end_sequence_number());
            }

            // Updating the peer window should trigger the task wakers
            {
                let mut con = con.lock().unwrap();
                con.update_peer_window(MAX_PACKET_SIZE as u32);
                
                assert_eq!(con.window_wakers.len(), 0);
            }

            // Task should now complete      
            let packet = tokio::select! {
                packet = wait_for_send => packet.unwrap(),
                _ = tokio::time::delay_for(Duration::from_millis(1)) => panic!("should return from future immediately if there is sufficient window")
            };

            let con = con.lock().unwrap();
            
            assert_eq!(packet.sequence_number, SequenceNumber(0));
            assert_eq!(con.window_wakers.len(), 0);
        });
    }
}
