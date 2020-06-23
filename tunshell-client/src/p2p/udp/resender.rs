use super::{SendEvent, UdpPacket};
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

/// Responsible for resending dropped packets
#[derive(Debug)]
pub(super) struct PacketResender {
    /// The highest acknowledged sequence number received from the peer.
    peer_ack_number: u32,

    /// Stores a map of sent packets which have not been acknowledged yet.
    /// Indexed by their sequence number.
    sent_packets: HashMap<u32, UdpPacket>,

    /// The packet sender.
    packet_sender: Sender<SendEvent>,
}

impl PacketResender {
    pub(super) fn new() -> Self {
        Self {
            peer_ack_number: 0,
            sent_packets: HashMap::new(),
            packet_sender: tokio::sync::mpsc::channel(1).0 // TODO: get from param
        }
    }
}