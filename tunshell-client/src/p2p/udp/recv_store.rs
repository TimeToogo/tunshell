use super::UdpPacket;

/// Stores incoming packets and reassembles them into
/// a continuous bytes buffer
#[derive(Debug)]
pub(super) struct RecvStore {
    /// The index of the next byte to be sent.
    /// This number will wrap back to 0 after exceeding u32::MAX
    sequence_number: u32,

    /// The highest sequence number of successfully received bytes.
    /// The packets must be in order, without gaps to be acknowledged.
    ack_number: u32,

    /// Incoming packets which are not able to be reassembled yet
    /// due to gaps in the sequence.
    packets: Vec<UdpPacket>,

    /// Storage for the reassembled byte stream.
    reassembled_buffer: Vec<u8>,
}

impl RecvStore {
    pub(super) fn new() -> Self {
        Self {
            sequence_number: 0,
            ack_number: 0,
            packets: vec![],
            reassembled_buffer: vec![],
        }
    }
}
