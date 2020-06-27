use super::{SendEvent, SequenceNumber, UdpConnectionVars, UdpPacket, MAX_PACKET_SIZE};
use anyhow::Result;
use log::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub(super) enum UdpRecvError {
    #[error(
        "received packet sequence number {0} was dropped because the receiving buffers are full"
    )]
    OutOfBuffer(SequenceNumber),
    #[error(
        "received packet sequence number [{0}, {1}] is outside of the current window [{2}, {3}]"
    )]
    OutOfWindow(
        SequenceNumber,
        SequenceNumber,
        SequenceNumber,
        SequenceNumber,
    ),
    #[error("received payload [{0}, {1}] overlaps with previously received packet [{2}, {3}]")]
    OverlapsOtherMessage(
        SequenceNumber,
        SequenceNumber,
        SequenceNumber,
        SequenceNumber,
    ),
}

impl UdpConnectionVars {
    pub(super) fn recv_process_packet(&mut self, packet: UdpPacket) -> Result<u32, UdpRecvError> {
        if packet.len() as u32 > self.calculate_recv_window() {
            return Err(UdpRecvError::OutOfBuffer(packet.sequence_number));
        }

        if packet.sequence_number <= self.recv_window_start()
            || packet.end_sequence_number() > self.recv_window_end()
        {
            return Err(UdpRecvError::OutOfWindow(
                packet.sequence_number,
                packet.end_sequence_number(),
                self.recv_window_start(),
                self.recv_window_start(),
            ));
        }

        if let Some(other) = self.recv_packets.iter().find(|i| packet.overlaps(i)) {
            return Err(UdpRecvError::OverlapsOtherMessage(
                packet.sequence_number,
                packet.end_sequence_number(),
                other.sequence_number,
                other.end_sequence_number(),
            ));
        }

        // We first insert in packet into the recv_packets vector
        // making sure to keep the packets in order of ascending sequence number.
        let insert_at = self
            .recv_packets
            .iter()
            .take_while(|i| i.sequence_number < packet.sequence_number)
            .count();

        self.recv_packets.insert(insert_at, packet);
        Ok(self.recv_reassemble_packets())
    }

    fn recv_window_start(&self) -> SequenceNumber {
        self.ack_number
    }

    fn recv_window_end(&self) -> SequenceNumber {
        self.ack_number + SequenceNumber(self.config().recv_window())
    }

    fn recv_reassemble_packets(&mut self) -> u32 {
        // After receiving a packet we attempt to reassemble the
        // payloads into the original bytes stream
        let mut reassembled_packets = 0;
        let mut reassembled_bytes = 0;
        let mut advanced_sequence_numbers = 0;

        for packet in self.recv_packets.iter() {
            // Each packet will consume one sequence number, this will ensure that even
            // empty packets are reliably delivered.
            // Hence the next sequence number we expect is the current ack number + 1
            let expected_sequence_number = self.ack_number + SequenceNumber(1);

            if expected_sequence_number != packet.sequence_number {
                info!(
                    "received out of order packets, expected next sequence number {} but received {}",
                    expected_sequence_number, packet.sequence_number
                );
                break;
            }

            self.reassembled_buffer
                .extend_from_slice(&packet.payload[..]);
            reassembled_bytes += packet.payload.len();
            reassembled_packets += 1;
            self.ack_number = packet.end_sequence_number();
            advanced_sequence_numbers += 1 + packet.payload.len() as u32;
            info!("successfully received {} bytes", packet.length);
        }

        self.recv_packets.drain(..reassembled_packets);

        // If new bytes are available in the reassembled_buffer
        // we wake the task wakers which are waiting for new data
        if reassembled_bytes > 0 {
            for waker in self.recv_wakers.drain(..) {
                waker.wake();
            }
        }

        // If we have acknowledged any new packets or bytes
        // we trigger an ack-update event to send through an acknowledgement
        // of the received packets
        if advanced_sequence_numbers > 0 {
            self.event_sender()
                .send(SendEvent::AckUpdate);
        }

        advanced_sequence_numbers
    }

    pub(super) fn recv_available_bytes(&self) -> usize {
        self.reassembled_buffer.len()
    }

    pub(super) fn recv_drain_bytes(&mut self, len: usize) -> Vec<u8> {
        let original_window = self.calculate_recv_window();

        let bytes = std::cmp::min(self.recv_available_bytes(), len);
        let drained = self.reassembled_buffer.drain(..bytes).collect::<Vec<u8>>();

        let new_window = self.calculate_recv_window();

        // When the recv buffer permits a new inbound packet we send the peer
        // a window update
        if original_window < MAX_PACKET_SIZE as u32 && new_window >= MAX_PACKET_SIZE as u32 {
            let result = self
                .event_sender()
                .send(SendEvent::WindowUpdate);

            if let Err(err) = result {
                error!("failed to send window update event: {}", err);
            }
        }

        drained
    }

    pub(super) fn calculate_recv_window(&self) -> u32 {
        let buffered_packet_bytes: u32 = self
            .recv_packets
            .iter()
            .map(|i| i.len() as u32)
            .sum::<u32>()
            + self.reassembled_buffer.len() as u32;
        let recv_window = self.config().recv_window();

        if buffered_packet_bytes > recv_window {
            0
        } else {
            recv_window - buffered_packet_bytes
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::UdpConnectionConfig;
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::task::Poll;
    use std::time::Duration;
    use tokio::runtime::Runtime;

    #[test]
    fn test_recv_process_packet_empty_packet() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);

            let advanced = con
                .recv_process_packet(UdpPacket::create(
                    SequenceNumber(1),
                    SequenceNumber(0),
                    0,
                    &[],
                ))
                .unwrap();

            assert_eq!(advanced, 1);
            assert_eq!(con.recv_packets, Vec::<UdpPacket>::new());
            assert_eq!(con.reassembled_buffer, Vec::<u8>::new());
            assert_eq!(con.ack_number, SequenceNumber(1));
        });
    }

    #[test]
    fn test_recv_process_packet_single_packet() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);

            let advanced = con
                .recv_process_packet(UdpPacket::create(
                    SequenceNumber(1),
                    SequenceNumber(0),
                    0,
                    &[1, 2, 3, 4, 5],
                ))
                .unwrap();

            assert_eq!(advanced, 6);
            assert_eq!(con.recv_packets, Vec::<UdpPacket>::new());
            assert_eq!(con.reassembled_buffer, vec![1, 2, 3, 4, 5]);
            assert_eq!(con.ack_number, SequenceNumber(6));
        });
    }

    #[test]
    fn test_recv_process_packet_emits_ack_update() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);

            con
                .recv_process_packet(UdpPacket::create(
                    SequenceNumber(1),
                    SequenceNumber(0),
                    0,
                    &[1, 2, 3, 4, 5],
                ))
                .unwrap();

            let event = tokio::select! {
                event = rx.recv() => event.unwrap(),
                _ = tokio::time::delay_for(Duration::from_millis(10)) => panic!("should received event from channel")
            };

            assert_eq!(event, SendEvent::AckUpdate);
        });
    }

    #[test]
    fn test_recv_process_packet_packet_before_window() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

            con.ack_number = SequenceNumber(10);

            let result = con.recv_process_packet(UdpPacket::create(
                SequenceNumber(9),
                SequenceNumber(0),
                0,
                &[],
            ));

            assert_eq!(result.is_err(), true);

            let result = con.recv_process_packet(UdpPacket::create(
                SequenceNumber(10),
                SequenceNumber(0),
                0,
                &[],
            ));

            assert_eq!(result.is_err(), true);
        });
    }

    #[test]
    fn test_recv_process_packet_packet_after_window() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

            con.ack_number = SequenceNumber(10);

            let result = con.recv_process_packet(UdpPacket::create(
                SequenceNumber(5000000),
                SequenceNumber(0),
                0,
                &[],
            ));

            assert_eq!(result.is_err(), true);
        });
    }

    #[test]
    fn test_recv_process_packet_overlap_existing() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

            con.ack_number = SequenceNumber(10);
            con.recv_packets.push(UdpPacket::create(
                SequenceNumber(1),
                SequenceNumber(0),
                0,
                &[1, 2, 3, 4, 5],
            ));

            let result = con.recv_process_packet(UdpPacket::create(
                SequenceNumber(5),
                SequenceNumber(0),
                0,
                &[],
            ));

            assert_eq!(result.is_err(), true);
        });
    }

    #[test]
    fn test_recv_process_packet_out_of_order_packet() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);

            let advanced = con
                .recv_process_packet(UdpPacket::create(
                    SequenceNumber(7),
                    SequenceNumber(0),
                    0,
                    &[6, 7, 8, 9, 10],
                ))
                .unwrap();

            assert_eq!(advanced, 0);
            assert_eq!(
                con.recv_packets,
                vec![UdpPacket::create(
                    SequenceNumber(7),
                    SequenceNumber(0),
                    0,
                    &[6, 7, 8, 9, 10],
                )]
            );
            assert_eq!(con.reassembled_buffer, Vec::<u8>::new());
            assert_eq!(con.ack_number, SequenceNumber(0));

            let advanced = con
                .recv_process_packet(UdpPacket::create(
                    SequenceNumber(1),
                    SequenceNumber(0),
                    0,
                    &[1, 2, 3, 4, 5],
                ))
                .unwrap();

            assert_eq!(advanced, 12);
            assert_eq!(con.recv_packets, Vec::<UdpPacket>::new());
            assert_eq!(con.reassembled_buffer, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
            assert_eq!(con.ack_number, SequenceNumber(12));
        });
    }

    #[test]
    fn test_recv_process_packet_calls_recv_wakers() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();
            con.event_sender = Some(tx);
            
            let con = Arc::new(Mutex::new(con));

            // Create a task which will wait until buffer becomes available.
            let recv_task = {
                let con = Arc::clone(&con);

                tokio::spawn(async move {
                    futures::future::poll_fn(move |cx| {
                        let mut con = con.lock().unwrap();
                        if con.reassembled_buffer.len() == 0 {
                            con.recv_wakers.push(cx.waker().clone());
                            return Poll::Pending;
                        }

                        return Poll::Ready(con.reassembled_buffer.clone());
                    })
                    .await
                })
            };

            // Yield to executor to allow recv_task to poll
            tokio::time::delay_for(Duration::from_millis(10)).await;

            // Polling the future should add the waker to recv_wakers
            {
                let con = con.lock().unwrap();
                assert_eq!(con.recv_wakers.len(), 1);
            }

            // Send buff
            {
                let mut con = con.lock().unwrap();
                con.recv_process_packet(UdpPacket::create(
                    SequenceNumber(1),
                    SequenceNumber(0),
                    0,
                    &[1, 2, 3, 4, 5],
                ))
                .unwrap();

                assert_eq!(con.recv_packets, Vec::<UdpPacket>::new());
                assert_eq!(con.reassembled_buffer, vec![1, 2, 3, 4, 5]);
                assert_eq!(con.ack_number, SequenceNumber(6));
                assert_eq!(con.recv_wakers.len(), 0);
            }

            let result = tokio::select! {
                result = recv_task => result,
                _ = tokio::time::delay_for(Duration::from_millis(10)) => panic!("recv_task should resolve")
            };

            assert_eq!(result.unwrap(), vec![1, 2, 3, 4, 5]);
        });
    }

    #[test]
    fn test_recv_available_bytes() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.reassembled_buffer = vec![1, 2, 3];

        assert_eq!(con.recv_available_bytes(), 3);
    }

    #[test]
    fn test_recv_drain_bytes() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.reassembled_buffer = vec![1, 2, 3];

        assert_eq!(con.recv_drain_bytes(1), vec![1]);
        assert_eq!(con.recv_drain_bytes(2), vec![2, 3]);
        assert_eq!(con.recv_drain_bytes(1), Vec::<u8>::new());

        con.reassembled_buffer = vec![1, 2, 3];

        assert_eq!(con.recv_drain_bytes(10), vec![1, 2, 3]);
    }

    #[test]
    fn test_recv_drain_bytes_sending_window_update() {
        Runtime::new().unwrap().block_on(async {
            let mut con = UdpConnectionVars::new(
                UdpConnectionConfig::default().with_recv_window(MAX_PACKET_SIZE as u32),
            );
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

            con.event_sender = Some(tx);
            con.reassembled_buffer = (1..MAX_PACKET_SIZE).map(|_| 0u8).collect();

            con.recv_drain_bytes(1024);

            let event = tokio::select! {
                event = rx.recv() => event.unwrap(),
                _ = tokio::time::delay_for(Duration::from_millis(10)) => panic!("should received event from channel")
            };

            assert_eq!(event, SendEvent::WindowUpdate);
        });
    }

    #[test]
    fn test_calculate_recv_window() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default().with_recv_window(1000));

        assert_eq!(con.calculate_recv_window(), 1000);

        let recv_packet =
            UdpPacket::create(SequenceNumber(0), SequenceNumber(0), 0, &[1, 2, 3, 4, 5]);

        con.recv_packets.push(recv_packet.clone());

        assert_eq!(con.calculate_recv_window(), 1000 - recv_packet.len() as u32);

        con.reassembled_buffer.extend_from_slice(&[1, 2, 3, 4, 5]);

        assert_eq!(
            con.calculate_recv_window(),
            1000 - 5 - recv_packet.len() as u32
        );
    }
}
