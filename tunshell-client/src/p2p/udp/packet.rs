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
pub(super) const UDP_PACKET_HEADER_SIZE: usize = 1 + 4 + 4 + 4 + 2 + 4;

#[derive(Debug, PartialEq, Copy, Clone)]
pub(super) enum UdpPacketType {
    Open,
    Data,
    Close,
}

#[derive(Debug, PartialEq, Clone)]
pub(super) struct UdpPacket {
    /// The type of the packet
    pub(super) packet_type: UdpPacketType,

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
    #[error("received packet has invalid type: {0}")]
    InvalidType(u8),
    #[error("received packet is too small: {0}")]
    BufferTooSmall(usize),
    #[error("received packet payload length mismatch, expected {0} != actual {1}")]
    PayloadLengthMismatch(usize, usize),
}

impl UdpPacketType {
    pub(super) fn parse(byte: u8) -> Result<Self, UdpPacketParseError> {
        match byte {
            0 => Ok(UdpPacketType::Open),
            1 => Ok(UdpPacketType::Data),
            2 => Ok(UdpPacketType::Close),
            _ => Err(UdpPacketParseError::InvalidType(byte)),
        }
    }

    pub(super) fn to_byte(&self) -> u8 {
        match self {
            Self::Open => 0,
            Self::Data => 1,
            Self::Close => 2,
        }
    }
}

impl UdpPacket {
    pub(super) fn parse(data: &[u8]) -> Result<UdpPacket, UdpPacketParseError> {
        if data.len() < UDP_PACKET_HEADER_SIZE {
            return Err(UdpPacketParseError::BufferTooSmall(data.len()));
        }

        let mut cursor = Cursor::new(data);

        let packet_type = UdpPacketType::parse(cursor.read_u8().unwrap())?;
        let sequence_number = SequenceNumber(cursor.read_u32::<BigEndian>().unwrap());
        let ack_number = SequenceNumber(cursor.read_u32::<BigEndian>().unwrap());
        let window = cursor.read_u32::<BigEndian>().unwrap();
        let length = cursor.read_u16::<BigEndian>().unwrap();
        let checksum = cursor.read_u32::<BigEndian>().unwrap();

        let payload_start = cursor.position() as usize;
        let payload_end = payload_start + length as usize;
        let data = cursor.into_inner();

        if data.len() < payload_end {
            return Err(UdpPacketParseError::PayloadLengthMismatch(
                length as usize,
                data.len(),
            ));
        }

        let payload = data[payload_start..payload_end].to_vec();

        Ok(UdpPacket {
            packet_type,
            sequence_number,
            ack_number,
            window,
            length,
            checksum,
            payload,
        })
    }

    pub(super) fn create(
        packet_type: UdpPacketType,
        sequence_number: SequenceNumber,
        ack_number: SequenceNumber,
        window: u32,
        buff: &[u8],
    ) -> UdpPacket {
        let length = std::cmp::min(buff.len(), MAX_PACKET_SIZE - UDP_PACKET_HEADER_SIZE) as u16;

        let payload = buff[..(length as usize)].to_vec();

        let mut packet = UdpPacket {
            packet_type,
            sequence_number,
            ack_number,
            window,
            length,
            checksum: 0,
            payload,
        };

        packet.checksum = packet.calculate_checksum();

        return packet;
    }

    pub(super) fn open(
        sequence_number: SequenceNumber,
        ack_number: SequenceNumber,
        window: u32,
    ) -> UdpPacket {
        Self::create(
            UdpPacketType::Open,
            sequence_number,
            ack_number,
            window,
            &[],
        )
    }

    pub(super) fn data(
        sequence_number: SequenceNumber,
        ack_number: SequenceNumber,
        window: u32,
        buff: &[u8],
    ) -> UdpPacket {
        Self::create(
            UdpPacketType::Data,
            sequence_number,
            ack_number,
            window,
            buff,
        )
    }

    pub(super) fn close(sequence_number: SequenceNumber, ack_number: SequenceNumber) -> UdpPacket {
        Self::create(UdpPacketType::Close, sequence_number, ack_number, 0, &[])
    }

    pub(super) fn to_vec(&self) -> Vec<u8> {
        use std::io::Write;

        let buff = Vec::with_capacity(UDP_PACKET_HEADER_SIZE + self.payload.len());

        let mut cursor = Cursor::new(buff);
        cursor.write_u8(self.packet_type.to_byte()).unwrap();
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
        UDP_PACKET_HEADER_SIZE + self.payload.len()
    }

    pub(super) fn is_checksum_valid(&self) -> bool {
        self.checksum == self.calculate_checksum()
    }

    pub(super) fn calculate_checksum(&self) -> u32 {
        let mut hasher = twox_hash::XxHash32::default();

        hasher.write_u8(self.packet_type.to_byte());
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
    fn test_udp_packet_type_parse() {
        assert_eq!(UdpPacketType::parse(0).unwrap(), UdpPacketType::Open);
        assert_eq!(UdpPacketType::parse(1).unwrap(), UdpPacketType::Data);
        assert_eq!(UdpPacketType::parse(2).unwrap(), UdpPacketType::Close);
        assert_eq!(UdpPacketType::parse(3).is_err(), true);
    }

    #[test]
    fn test_udp_packet_type_to_byte() {
        assert_eq!(UdpPacketType::Open.to_byte(), 0);
        assert_eq!(UdpPacketType::Data.to_byte(), 1);
        assert_eq!(UdpPacketType::Close.to_byte(), 2);
    }

    #[test]
    fn test_parse_udp_packet() {
        let raw_data = [
            1u8, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 4, 0, 0, 0, 5, 1, 2, 3, 4, 5, 6,
        ];

        let packet = UdpPacket::parse(&raw_data).unwrap();

        assert_eq!(packet.packet_type, UdpPacketType::Data);
        assert_eq!(packet.sequence_number, SequenceNumber(1));
        assert_eq!(packet.ack_number, SequenceNumber(2));
        assert_eq!(packet.window, 3);
        assert_eq!(packet.length, 4);
        assert_eq!(packet.checksum, 5);
        assert_eq!(packet.payload, vec![1, 2, 3, 4]);
        assert_eq!(packet.len(), 23);
    }

    #[test]
    fn test_parse_udp_packet_too_short() {
        let raw_data = [1, 2, 3, 4];

        match UdpPacket::parse(&raw_data) {
            Err(UdpPacketParseError::BufferTooSmall(_)) => {}
            Err(err) => panic!("incorrect error type: {:?}", err),
            Ok(_) => panic!("must not return Ok"),
        }
    }

    #[test]
    fn test_parse_udp_packet_not_enough_payload() {
        let raw_data = [0u8, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 4, 0, 0, 0, 5, 1];

        match UdpPacket::parse(&raw_data) {
            Err(UdpPacketParseError::PayloadLengthMismatch(_, _)) => {}
            Err(err) => panic!("incorrect error type: {:?}", err),
            Ok(_) => panic!("must not return Ok"),
        }
    }

    #[test]
    fn test_udp_packet_calculate_hash() {
        let packet = UdpPacket {
            packet_type: UdpPacketType::Data,
            sequence_number: SequenceNumber(0),
            ack_number: SequenceNumber(1),
            window: 2,
            length: 3,
            checksum: 0,
            payload: vec![1, 2, 3, 4],
        };

        assert_eq!(packet.calculate_checksum(), 2061921599);
    }

    #[test]
    fn test_udp_packet_create() {
        let packet = UdpPacket::create(
            UdpPacketType::Data,
            SequenceNumber(100),
            SequenceNumber(200),
            300,
            &[1, 2, 3, 4, 5],
        );

        assert_eq!(packet.sequence_number, SequenceNumber(100));
        assert_eq!(packet.ack_number, SequenceNumber(200));
        assert_eq!(packet.window, 300);
        assert_eq!(packet.length, 5);
        assert!(packet.is_checksum_valid());
        assert_eq!(packet.payload, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_udp_packet_open() {
        let packet = UdpPacket::open(SequenceNumber(100), SequenceNumber(200), 300);

        assert_eq!(packet.sequence_number, SequenceNumber(100));
        assert_eq!(packet.ack_number, SequenceNumber(200));
        assert_eq!(packet.window, 300);
        assert_eq!(packet.length, 0);
        assert!(packet.is_checksum_valid());
        assert_eq!(packet.payload, Vec::<u8>::new());
    }

    #[test]
    fn test_udp_packet_data() {
        let packet = UdpPacket::data(
            SequenceNumber(100),
            SequenceNumber(200),
            300,
            &[1, 2, 3, 4, 5],
        );

        assert_eq!(packet.sequence_number, SequenceNumber(100));
        assert_eq!(packet.ack_number, SequenceNumber(200));
        assert_eq!(packet.window, 300);
        assert_eq!(packet.length, 5);
        assert!(packet.is_checksum_valid());
        assert_eq!(packet.payload, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_udp_packet_close() {
        let packet = UdpPacket::close(SequenceNumber(100), SequenceNumber(200));

        assert_eq!(packet.sequence_number, SequenceNumber(100));
        assert_eq!(packet.ack_number, SequenceNumber(200));
        assert_eq!(packet.window, 0);
        assert_eq!(packet.length, 0);
        assert!(packet.is_checksum_valid());
        assert_eq!(packet.payload, Vec::<u8>::new());
    }

    #[test]
    fn test_udp_packet_to_vec() {
        let packet = UdpPacket {
            packet_type: UdpPacketType::Data,
            sequence_number: SequenceNumber(0),
            ack_number: SequenceNumber(1),
            window: 2,
            length: 3,
            checksum: 4,
            payload: vec![1, 2, 3],
        };

        let result = packet.to_vec();

        assert_eq!(
            result,
            vec![1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 3, 0, 0, 0, 4, 1, 2, 3]
        );
    }

    #[test]
    fn test_udp_packet_to_vec_then_parse() {
        let packet = UdpPacket {
            packet_type: UdpPacketType::Data,
            sequence_number: SequenceNumber(12345765),
            ack_number: SequenceNumber(46547747),
            window: 67653435,
            length: 23,
            checksum: 44365478,
            payload: vec![
                1, 2, 3, 54, 5, 6, 54, 65, 6, 5, 7, 65, 76, 87, 86, 7, 8, 7, 98, 7, 89, 79, 2,
            ],
        };

        let parsed_packet = UdpPacket::parse(packet.to_vec().as_slice()).unwrap();

        assert_eq!(parsed_packet, packet);
    }

    #[test]
    fn test_udp_packet_end_sequence_number() {
        let packet = UdpPacket {
            packet_type: UdpPacketType::Data,
            sequence_number: SequenceNumber(100),
            ack_number: SequenceNumber(0),
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![],
        };

        assert_eq!(packet.end_sequence_number(), SequenceNumber(110));
    }

    #[test]
    fn test_udp_packet_overlap() {
        let packet1 = UdpPacket {
            packet_type: UdpPacketType::Data,
            sequence_number: SequenceNumber(100),
            ack_number: SequenceNumber(0),
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![1],
        };
        let packet2 = UdpPacket {
            packet_type: UdpPacketType::Data,
            sequence_number: SequenceNumber(111),
            ack_number: SequenceNumber(0),
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![1],
        };
        let packet3 = UdpPacket {
            packet_type: UdpPacketType::Data,
            sequence_number: SequenceNumber(105),
            ack_number: SequenceNumber(0),
            window: 0,
            length: 10,
            checksum: 0,
            payload: vec![1],
        };

        assert_eq!(packet1.overlaps(&packet1), true);
        assert_eq!(packet1.overlaps(&packet2), false);
        assert_eq!(packet2.overlaps(&packet1), false);
        assert_eq!(packet1.overlaps(&packet3), true);
        assert_eq!(packet3.overlaps(&packet1), true);
        assert_eq!(packet2.overlaps(&packet3), true);
        assert_eq!(packet3.overlaps(&packet2), true);
    }
}
