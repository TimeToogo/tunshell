use super::{SequenceNumber, UdpConnectionVars, UdpPacket};
use anyhow::Result;
use log::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub(super) enum UdpRecvError {
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
        let mut advanced_sequence_numbers = 0;

        for packet in self.recv_packets.iter() {
            // Each packet will consume one sequence number, this ensure that even
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
            reassembled_packets += 1;
            self.ack_number = packet.end_sequence_number();
            advanced_sequence_numbers += 1 + packet.payload.len() as u32;
            info!("successfully received {} bytes", packet.length);
        }

        self.recv_packets.drain(..reassembled_packets);

        advanced_sequence_numbers
    }

    pub(super) fn recv_available_bytes(&self) -> usize {
        self.reassembled_buffer.len()
    }

    pub(super) fn recv_drain_bytes(&mut self, len: usize) -> Vec<u8> {
        let bytes = std::cmp::min(self.recv_available_bytes(), len);

        self.reassembled_buffer.drain(..bytes).collect::<Vec<u8>>()
    }
}

#[cfg(test)]
mod tests {
    use super::super::UdpConnectionConfig;
    use super::*;

    #[test]
    fn test_recv_process_packet_empty_packet() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

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
    }

    #[test]
    fn test_recv_process_packet_single_packet() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

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
    }

    #[test]
    fn test_recv_process_packet_packet_before_window() {
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
    }

    #[test]
    fn test_recv_process_packet_packet_after_window() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

        con.ack_number = SequenceNumber(10);

        let result = con.recv_process_packet(UdpPacket::create(
            SequenceNumber(5000000),
            SequenceNumber(0),
            0,
            &[],
        ));

        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn test_recv_process_packet_overlap_existing() {
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
    }

    #[test]
    fn test_recv_process_packet_out_of_order_packet() {
        let mut con = UdpConnectionVars::new(UdpConnectionConfig::default());

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
}
