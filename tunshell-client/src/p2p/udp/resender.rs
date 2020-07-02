use super::{SendEvent, SequenceNumber, UdpConnectionVars, UdpPacket};
use log::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

impl UdpConnectionVars {
    pub(super) fn update_peer_ack_number(&mut self, ack_number: SequenceNumber) {
        if ack_number > self.peer_ack_number {
            self.peer_ack_number = ack_number;
            self.clear_acknowledged_packets();
        }
    }

    fn clear_acknowledged_packets(&mut self) {
        let peer_ack_number = self.peer_ack_number;

        self.sent_packets
            .retain(|sequence_number, _| *sequence_number > peer_ack_number);
        self.wake_pending_send_events();
    }

    fn normalised_rtt_estimate(&self) -> Duration {
        std::cmp::max(Duration::from_millis(100), self.rtt_estimate)
    }

    fn update_resending_packet(&self, packet: &mut UdpPacket) {
        // Ensure that the packet contains the latest
        // ack and window information before resending
        packet.ack_number = self.ack_number;
        packet.window = self.calculate_recv_window();
        packet.checksum = packet.calculate_checksum();
    }
}

pub(super) fn schedule_resend_if_dropped(
    con: Arc<Mutex<UdpConnectionVars>>,
    sent_packet: UdpPacket,
) {
    let packet_key = sent_packet.sequence_number;
    let resend_count = sent_packet.resend_count;

    let (rtt_estimate, position_delay) = {
        let mut con = con.lock().unwrap();

        con.sent_packets.insert(packet_key, sent_packet);
        let position_delay = Duration::from_millis(con.sent_packets.len() as u64);

        (con.normalised_rtt_estimate(), position_delay)
    };

    tokio::spawn(async move {
        let resend_delay = (rtt_estimate * 2 * (resend_count + 1) as u32) + position_delay;
        tokio::time::delay_for(resend_delay).await;

        // It's important we only scope the lock of the connection to this block.
        // Since we may need to send an event to the orchestration loop which could attempt
        // to lock the connection for other purposes, this avoids a potential deadlock.
        let (packet, event_sender) = {
            if con.is_poisoned() {
                warn!("cannot resend packet: lock poisoned");
                return;
            }

            let mut con = loop {
                match con.try_lock() {
                    Ok(con) => break con,
                    Err(_) => {}
                };

                tokio::time::delay_for(position_delay).await;
            };

            if !con.is_connected() {
                return;
            }

            let sent_packet = con.sent_packets.remove(&packet_key);
            (sent_packet, con.event_sender())
        };

        if let Some(mut packet) = packet {
            // Packet has still not been acknowledged after 2*RTT
            // likely that is was dropped so we resend it
            packet.resend_count += 1;

            {
                let mut con = con.lock().unwrap();

                if packet.resend_count >= con.config().packet_resend_limit() {
                    warn!(
                        "packet [{}, {}] was resent too many times ({}), closing connection.",
                        packet.sequence_number,
                        packet.end_sequence_number(),
                        resend_count
                    );

                    con.event_sender()
                        .send(SendEvent::Close)
                        .unwrap_or_else(|err| warn!("failed to send close message: {}", err));
                    return;
                }

                con.update_resending_packet(&mut packet);
                con.decrease_transit_window_after_drop();

                warn!(
                    "resending dropped packet: [{}, {}] (peer ack: {})",
                    packet.sequence_number,
                    packet.end_sequence_number(),
                    con.peer_ack_number
                );
            }

            let result = event_sender.send(SendEvent::Resend(packet));

            if result.is_err() {
                warn!("failed to resend packet: {:?}", result);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::{SequenceNumber, UdpConnectionConfig, UdpConnectionState};
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_update_peer_ack_number_old_packet() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.peer_ack_number = SequenceNumber(50);

        con.update_peer_ack_number(SequenceNumber(40));

        assert_eq!(con.peer_ack_number, SequenceNumber(50));
    }

    #[test]
    fn test_update_peer_ack_number_new_packet() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.peer_ack_number = SequenceNumber(50);

        con.update_peer_ack_number(SequenceNumber(100));

        assert_eq!(con.peer_ack_number, SequenceNumber(100));
    }

    #[test]
    fn test_update_peer_ack_number_new_packet_with_wrapping() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
        con.state = UdpConnectionState::Connected;

        con.peer_ack_number = SequenceNumber(u32::MAX);

        con.update_peer_ack_number(SequenceNumber(50));

        assert_eq!(con.peer_ack_number, SequenceNumber(50));
    }

    #[test]
    fn test_update_peer_ack_number_removes_acknowledged_packets() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
        con.state = UdpConnectionState::Connected;

        con.sent_packets.insert(
            SequenceNumber(10),
            UdpPacket::data(SequenceNumber(10), SequenceNumber(0), 0, &[]),
        );
        con.sent_packets.insert(
            SequenceNumber(30),
            UdpPacket::data(SequenceNumber(30), SequenceNumber(0), 0, &[]),
        );
        con.peer_ack_number = SequenceNumber(0);

        con.update_peer_ack_number(SequenceNumber(20));

        assert_eq!(con.peer_ack_number, SequenceNumber(20));
        assert_eq!(con.sent_packets.contains_key(&SequenceNumber(10)), false);
        assert_eq!(con.sent_packets.contains_key(&SequenceNumber(30)), true);
    }

    #[test]
    fn test_normalised_rtt_estimate() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
        con.rtt_estimate = Duration::from_millis(200);

        assert_eq!(con.normalised_rtt_estimate(), Duration::from_millis(200));

        con.rtt_estimate = Duration::from_millis(50);

        assert_eq!(con.normalised_rtt_estimate(), Duration::from_millis(100));
    }

    #[test]
    fn test_update_resending_packet() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default().with_recv_window(1000));
        con.ack_number = SequenceNumber(100);

        let mut packet = UdpPacket::data(SequenceNumber(0), SequenceNumber(0), 0, &[1]);

        con.update_resending_packet(&mut packet);

        assert_eq!(
            packet,
            UdpPacket::data(SequenceNumber(0), SequenceNumber(100), 1000, &[1])
        );
    }

    #[test]
    fn test_schedule_resend_does_not_resend_if_packet_is_acknowledged() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
            con.state = UdpConnectionState::Connected;
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);
            con.rtt_estimate = Duration::from_millis(10);

            let con = Arc::from(Mutex::from(con));

            let sent_packet = UdpPacket::data(SequenceNumber(10), SequenceNumber(0), 0, &[]);

            schedule_resend_if_dropped(Arc::clone(&con), sent_packet.clone());

            // Should store packet in sent_packets map
            {
                let con = con.lock().unwrap();

                assert_eq!(
                    con.sent_packets.get(&SequenceNumber(10)),
                    Some(&sent_packet)
                );
            }

            // Acknowledge sent packet
            {
                let mut con = con.lock().unwrap();

                con.update_peer_ack_number(SequenceNumber(10));
            }

            // Wait for resend task to fire
            tokio::time::delay_for(Duration::from_millis(25)).await;
            // Should not have removed from sent_packets
            let result = rx.try_recv();
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_schedule_resend_does_resend_if_packet_is_not_acknowledged() {
        Runtime::new().unwrap().block_on(async {
            let mut con =
                UdpConnectionVars::new(UdpConnectionConfig::default().with_recv_window(1000));
            con.state = UdpConnectionState::Connected;
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);
            con.rtt_estimate = Duration::from_millis(100);

            let con = Arc::from(Mutex::from(con));

            let sent_packet = UdpPacket::data(SequenceNumber(10), SequenceNumber(0), 1000, &[]);

            schedule_resend_if_dropped(Arc::clone(&con), sent_packet.clone());

            // Should store packet in sent_packets map
            {
                let con = con.lock().unwrap();

                assert_eq!(
                    con.sent_packets.get(&SequenceNumber(10)),
                    Some(&sent_packet)
                );
            }

            // Mock ack update to ensure that the packet is updated when reset
            {
                let mut con = con.lock().unwrap();

                con.ack_number = SequenceNumber(50);
            }

            // Wait for resend task to fire
            tokio::time::delay_for(Duration::from_millis(250)).await;

            // Should remove packet from sent_packets map
            {
                let con = con.lock().unwrap();

                assert_eq!(con.sent_packets.get(&SequenceNumber(10)), None);
            }

            // Should send resend event with updated packet
            let result = rx.try_recv();

            let mut expected_packet =
                UdpPacket::data(SequenceNumber(10), SequenceNumber(50), 1000, &[]);
            expected_packet.resend_count = 1;

            assert_eq!(
                result.expect("should have sent Resend event"),
                SendEvent::Resend(expected_packet)
            );
        });
    }

    #[test]
    fn test_schedule_resend_does_resend_if_packet_connection_is_dropped() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
            con.state = UdpConnectionState::Connected;
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);
            con.rtt_estimate = Duration::from_millis(100);

            let con = Arc::from(Mutex::from(con));

            let sent_packet = UdpPacket::data(SequenceNumber(10), SequenceNumber(0), 0, &[]);

            schedule_resend_if_dropped(Arc::clone(&con), sent_packet.clone());

            // Should store packet in sent_packets map
            {
                let mut con = con.lock().unwrap();

                con.state = UdpConnectionState::Disconnected;
            }

            // Wait for resend task to fire
            tokio::time::delay_for(Duration::from_millis(250)).await;

            // Should not remove packet from sent_packets map
            {
                let con = con.lock().unwrap();

                assert_eq!(
                    con.sent_packets.get(&SequenceNumber(10)),
                    Some(&sent_packet)
                );
            }

            // Should not send resend event
            let result = rx.try_recv();
            assert_eq!(result.is_err(), true);
        });
    }

    #[test]
    fn test_schedule_resend_closes_connection_if_exceeds_resend_limit() {
        Runtime::new().unwrap().block_on(async {
            let mut con =
                UdpConnectionVars::new(UdpConnectionConfig::default().with_packet_resend_limit(3));
            con.state = UdpConnectionState::Connected;
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);
            con.rtt_estimate = Duration::from_millis(100);

            let con = Arc::from(Mutex::from(con));

            let mut sent_packet = UdpPacket::data(SequenceNumber(10), SequenceNumber(0), 0, &[]);
            sent_packet.resend_count = 3;

            schedule_resend_if_dropped(Arc::clone(&con), sent_packet.clone());

            // Wait for resend task to fire
            tokio::time::delay_for(Duration::from_millis(1010)).await;

            // Should send close event
            let result = rx.try_recv();
            
            match result {
                Ok(SendEvent::Close) => {},
                result @ _ => panic!("resend should have triggered close event, got {:?}", result)
            }
        });
    }
}
