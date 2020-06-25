use super::SequenceNumber;
use anyhow::Result;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use log::*;
use std::hash::Hasher;
use std::io::Cursor;
use thiserror::Error;
use twox_hash;

/// Total packed size of the packet header in bytes
pub(super) const MAX_PACKET_SIZE: usize = 576;
pub(super) const UDP_MESSAGE_HEADER_SIZE: usize = 4 + 4 + 4 + 2 + 4;

#[derive(Debug, PartialEq, Clone)]
pub(super) struct UdpPacket {
    /// The index (relative to the start of the connection) of the first bytes in the packet
    pub(super) sequence_number: SequenceNumber,

    /// The number of acknowledged bytes by the sender during the connection
    pub(super) ack_number: SequenceNumber,

    /// The amount of available space at the sender
    pub(super) window: u32,

    /// The number of bytes in the packet payload
    pub(super) length: u16,

    // A checksum of the packet data (excluding the checksum field) using twox_hash
    pub(super) checksum: u32,

    // The payload of the packet
    pub(super) payload: Vec<u8>,
}

#[derive(Error, Debug)]
pub(super) enum UdpPacketParseError {
    #[error("received packet is too small: {0}")]
    BufferTooSmall(usize),
    #[error("received packet payload length mismatch, expected {0} != actual {1}")]
    PayloadLengthMismatch(usize, usize),
}

impl UdpPacket {
    pub(super) fn parse(data: &[u8]) -> Result<UdpPacket, UdpPacketParseError> {
        if data.len() < UDP_MESSAGE_HEADER_SIZE {
            return Err(UdpPacketParseError::BufferTooSmall(data.len()));
        }

        let mut cursor = Cursor::new(data);

        let sequence_number = SequenceNumber(cursor.read_u32::<BigEndian>().unwrap());
        let ack_number = SequenceNumber(cursor.read_u32::<BigEndian>().unwrap());
        let window = cursor.read_u32::<BigEndian>().unwrap();
        let length = cursor.read_u16::<BigEndian>().unwrap();
        let checksum = cursor.read_u32::<BigEndian>().unwrap();

        let payload_start = cursor.position() as usize;
        let payload_end = payload_start + length as usize;
        let mut data = cursor.into_inner();

        if data.len() < payload_end {
            return Err(UdpPacketParseError::PayloadLengthMismatch(
                length as usize,
                data.len(),
            ));
        }

        let payload = data[payload_start..payload_end].to_vec();

        Ok(UdpPacket {
            sequence_number,
            ack_number,
            window,
            length,
            checksum,
            payload,
        })
    }

    pub(super) fn create(
        sequence_number: SequenceNumber,
        ack_number: SequenceNumber,
        window: u32,
        buff: &[u8],
    ) -> UdpPacket {
        let length = std::cmp::min(buff.len(), MAX_PACKET_SIZE - UDP_MESSAGE_HEADER_SIZE) as u16;

        let payload = buff[..(length as usize)].to_vec();

        let mut message = UdpPacket {
            sequence_number,
            ack_number,
            window,
            length,
            checksum: 0,
            payload,
        };

        message.checksum = message.calculate_checksum();

        return message;
    }

    pub(super) fn to_vec(&self) -> Vec<u8> {
        use std::io::Write;

        let buff = Vec::with_capacity(UDP_MESSAGE_HEADER_SIZE + self.payload.len());

        let mut cursor = Cursor::new(buff);
        cursor
            .write_u32::<BigEndian>(self.sequence_number.0)
            .unwrap();
        cursor.write_u32::<BigEndian>(self.ack_number.0).unwrap();
        cursor.write_u32::<BigEndian>(self.window).unwrap();
        cursor.write_u16::<BigEndian>(self.length).unwrap();
        cursor.write_u32::<BigEndian>(self.checksum).unwrap();
        cursor.write_all(self.payload.as_slice()).unwrap();

        cursor.into_inner()
    }

    pub(super) fn len(&self) -> usize {
        UDP_MESSAGE_HEADER_SIZE + self.payload.len()
    }

    pub(super) fn is_checksum_valid(&self) -> bool {
        self.checksum == self.calculate_checksum()
    }

    pub(super) fn calculate_checksum(&self) -> u32 {
        let mut hasher = twox_hash::XxHash32::default();

        hasher.write_u32(self.sequence_number.0);
        hasher.write_u32(self.ack_number.0);
        hasher.write_u32(self.window);
        hasher.write_u16(self.length);
        hasher.write(self.payload.as_slice());

        hasher.finish() as u32
    }

    pub(super) fn end_sequence_number(&self) -> SequenceNumber {
        self.sequence_number + SequenceNumber(self.length as u32)
    }

    pub(super) fn overlaps(&self, other: &Self) -> bool {
        self.sequence_number.0 <= other.end_sequence_number().0
            && other.sequence_number.0 <= self.end_sequence_number().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_udp_message() {
        let raw_data = [
            0u8, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 4, 0, 0, 0, 5, 1, 2, 3, 4, 5, 6,
        ];

        let message = UdpPacket::parse(&raw_data).unwrap();

        assert_eq!(message.sequence_number, SequenceNumber(1));
        assert_eq!(message.ack_number, SequenceNumber(2));
        assert_eq!(message.window, 3);
        assert_eq!(message.length, 4);
        assert_eq!(message.checksum, 5);
        assert_eq!(message.payload, vec![1, 2, 3, 4]);
        assert_eq!(message.len(), 22);
    }

    #[test]
    fn test_parse_udp_message_too_short() {
        let raw_data = [1, 2, 3, 4];

        match UdpPacket::parse(&raw_data) {
            Err(UdpPacketParseError::BufferTooSmall(_)) => {}
            Err(err) => panic!("incorrect error type: {:?}", err),
            Ok(_) => panic!("must not return Ok"),
        }
    }

    #[test]
    fn test_parse_udp_message_not_enough_payload() {
        let raw_data = [0u8, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 4, 0, 0, 0, 5, 1];

        match UdpPacket::parse(&raw_data) {
            Err(UdpPacketParseError::PayloadLengthMismatch(_, _)) => {}
            Err(err) => panic!("incorrect error type: {:?}", err),
            Ok(_) => panic!("must not return Ok"),
        }
    }

    #[test]
    fn test_udp_message_calculate_hash() {
        let message = UdpPacket {
            sequence_number: SequenceNumber(0),
            ack_number: SequenceNumber(1),
            window: 2,
            length: 3,
            checksum: 0,
            payload: vec![1, 2, 3, 4],
        };

        assert_eq!(message.calculate_checksum(), 3014949120);
    }

    #[test]
    fn test_udp_message_create() {
        let message = UdpPacket::create(
            SequenceNumber(100),
            SequenceNumber(200),
            300,
            &[1, 2, 3, 4, 5],
        );

        assert_eq!(message.sequence_number, SequenceNumber(100));
        assert_eq!(message.ack_number, SequenceNumber(200));
        assert_eq!(message.window, 300);
        assert_eq!(message.length, 5);
        assert!(message.is_checksum_valid());
        assert_eq!(message.payload, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_udp_message_to_vec() {
        let message = UdpPacket {
            sequence_number: SequenceNumber(0),
            ack_number: SequenceNumber(1),
            window: 2,
            length: 3,
            checksum: 4,
            payload: vec![1, 2, 3],
        };

        let result = message.to_vec();

        assert_eq!(
            result,
            vec![0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 3, 0, 0, 0, 4, 1, 2, 3]
        );
    }

    #[test]
    fn test_udp_message_to_vec_then_parse() {
        let message = UdpPacket {
            sequence_number: SequenceNumber(12345765),
            ack_number: SequenceNumber(46547747),
            window: 67653435,
            length: 23,
            checksum: 44365478,
            payload: vec![
                1, 2, 3, 54, 5, 6, 54, 65, 6, 5, 7, 65, 76, 87, 86, 7, 8, 7, 98, 7, 89, 79, 2,
            ],
        };

        let parsed_message = UdpPacket::parse(message.to_vec().as_slice()).unwrap();

        assert_eq!(parsed_message, message);
    }

    #[test]
    fn test_udp_message_end_sequence_number() {
        let message = UdpPacket {
            sequence_number: SequenceNumber(100),
            ack_number: SequenceNumber(0),
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![],
        };

        assert_eq!(message.end_sequence_number(), SequenceNumber(110));
    }

    #[test]
    fn test_udp_message_overlap() {
        let message1 = UdpPacket {
            sequence_number: SequenceNumber(100),
            ack_number: SequenceNumber(0),
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![],
        };
        let message2 = UdpPacket {
            sequence_number: SequenceNumber(111),
            ack_number: SequenceNumber(0),
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![],
        };
        let message3 = UdpPacket {
            sequence_number: SequenceNumber(105),
            ack_number: SequenceNumber(0),
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![],
        };

        assert_eq!(message1.overlaps(&message1), true);
        assert_eq!(message1.overlaps(&message2), false);
        assert_eq!(message2.overlaps(&message1), false);
        assert_eq!(message1.overlaps(&message3), true);
        assert_eq!(message3.overlaps(&message1), true);
        assert_eq!(message2.overlaps(&message3), true);
        assert_eq!(message3.overlaps(&message2), true);
    }
}
