use super::{UdpConnectionVars, UdpPacket, MAX_PACKET_SIZE};
use log::*;
use std::cmp;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

impl UdpConnectionVars {
    fn poll_can_send(&mut self, cx: &Context, packet: &UdpPacket) -> Poll<()> {
        if self.can_send(packet) {
            Poll::Ready(())
        } else {
            debug!("connection is congested, waiting before sending packet {} ({} bytes permitted)", packet.sequence_number, self.bytes_permitted_to_be_sent());
            self.window_wakers
                .push((cx.waker().clone(), packet.len() as u32));
            Poll::Pending
        }
    }

    pub(super) fn can_send(&self, packet: &UdpPacket) -> bool {
        self.can_send_bytes(packet.len() as u32)
    }

    pub(super) fn can_send_bytes(&self, amount: u32) -> bool {
        amount <= self.bytes_permitted_to_be_sent()
    }

    fn bytes_permitted_to_be_sent(&self) -> u32 {
        let bytes_in_transit = self.sent_packets.values().map(|i| i.len()).sum::<usize>() as u32;

        let bytes_left_in_transit = if bytes_in_transit < self.transit_window {
            self.transit_window - bytes_in_transit
        } else {
            0
        };

        let bytes_left_in_peer_window = if bytes_in_transit < self.peer_window {
            self.peer_window - bytes_in_transit
        } else {
            0
        };

        cmp::min(bytes_left_in_transit, bytes_left_in_peer_window)
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
        let mut permitted_bytes = self.bytes_permitted_to_be_sent();
        let mut to_wake = 0;

        // Determine the number of packets we can now send form those are waiting.
        for (_, packet_len) in &self.window_wakers {
            if *packet_len < permitted_bytes {
                permitted_bytes -= *packet_len;
                to_wake += 1;
            }
        }

        for (waker, _) in self.window_wakers.drain(..to_wake) {
            waker.wake();
        }
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
    fn test_bytes_permitted_to_be_sent() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.peer_window = 10;
        con.transit_window = 20;

        assert_eq!(con.bytes_permitted_to_be_sent(), 10);

        con.peer_window = 30;
        con.transit_window = 20;

        assert_eq!(con.bytes_permitted_to_be_sent(), 20);

        let sent_packet =
            UdpPacket::data(SequenceNumber(0), SequenceNumber(0), 0, &[1, 2, 3, 4, 5]);
        con.sent_packets
            .insert(SequenceNumber(0), sent_packet.clone());
        con.peer_window = 50;
        con.transit_window = 100;

        assert_eq!(
            con.bytes_permitted_to_be_sent(),
            50 - sent_packet.len() as u32
        );

        con.peer_window = 100;
        con.transit_window = 50;

        assert_eq!(
            con.bytes_permitted_to_be_sent(),
            50 - sent_packet.len() as u32
        );

        con.peer_window = 0;
        con.transit_window = 0;

        assert_eq!(con.bytes_permitted_to_be_sent(), 0);
    }

    #[test]
    fn test_can_send() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        let packet = UdpPacket::data(SequenceNumber(0), SequenceNumber(0), 0, &[1, 2, 3, 4, 5]);

        con.transit_window = 10;
        con.peer_window = 10;

        assert_eq!(con.can_send(&packet), false);

        con.transit_window = packet.len() as u32;
        con.peer_window = packet.len() as u32;

        assert_eq!(con.can_send(&packet), true);
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
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

            con.transit_window = MAX_PACKET_SIZE as u32;
            con.peer_window = 0;

            let con = Arc::from(Mutex::from(con));

            let packet = UdpPacket::data(SequenceNumber(0), SequenceNumber(0), 0, &[]);
            
            let wait_for_send = tokio::spawn(wait_until_can_send(Arc::clone(&con), packet.clone()));

            tokio::time::delay_for(Duration::from_millis(10)).await;

            {
                let con = con.lock().unwrap();
                
                assert_eq!(con.window_wakers.len(), 1);
                assert_eq!(con.window_wakers[0].1, packet.len() as u32);
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
