use super::{SendEvent, UdpConnectionConfig, UdpPacket, SequenceNumber};
use log::*;
use std::collections::HashMap;
use std::task::Waker;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, PartialEq, Copy, Clone)]
pub(super) enum UdpConnectionState {
    New,
    SentHello,
    SentSync,
    WaitingForSync,
    ConnectFailed,
    Connected,
    Disconnected,
}

#[derive(Debug)]
pub(super) struct UdpConnectionVars {
    /// The configuration variables of the connection
    config: UdpConnectionConfig,

    /// The current state of the connection
    pub(super) state: UdpConnectionState,

    /// The amount of bytes available in the peer's buffers to receive packets.
    /// This will be the window value of the packet received with the highest sequence number.
    pub(super) peer_window: u32,

    /// The number of bytes permitted to be in-flight and not yet acknowledged by the peer.
    /// If this number falls near zero, packets will not be permitted to be sent outbound.
    pub(super) transit_window: u32,

    /// Task wakers which are waiting for the window to grow  allowing for another packet to be sent.
    /// The u32 in the tuple represents the packet's length associated to the waker.
    pub(super) window_wakers: Vec<(Waker, u32)>,

    /// The index of the next byte to be sent.
    /// This number will wrap back to 0 after exceeding u32::MAX
    pub(super) sequence_number: SequenceNumber,

    /// The highest sequence number of successfully received bytes.
    /// The packets must be in order, without gaps to be acknowledged.
    pub(super) ack_number: SequenceNumber,

    /// Incoming packets which are not able to be reassembled yet
    /// due to gaps in the sequence.
    pub(super) recv_packets: Vec<UdpPacket>,

    /// Storage for the reassembled byte stream.
    pub(super) reassembled_buffer: Vec<u8>,

    /// Task wakers which a waiting on new received buffer becoming available
    pub (super) recv_wakers: Vec<Waker>,

    /// The current estimated round trip time of the connection.
    pub(super) rtt_estimate: Duration,

    /// Stores when packets were sent.
    /// The key is the sequence number of the packet the value is when it was sent.
    pub(super) send_times: HashMap<SequenceNumber, Instant>,

    /// The highest acknowledged sequence number received from the peer.
    pub(super) peer_ack_number: SequenceNumber,

    /// Stores a map of sent packets which have not been acknowledged yet.
    /// Indexed by their sequence number.
    pub(super) sent_packets: HashMap<SequenceNumber, UdpPacket>,

    /// Channel for signaling that new packets should be sent or resent.
    pub(super) event_sender: Option<UnboundedSender<SendEvent>>,
}

impl UdpConnectionVars {
    pub(super) fn new(config: UdpConnectionConfig) -> Self {
        debug!("connection state initialised to NEW");
        let transit_window = config.initial_transit_window();
        Self {
            config,
            state: UdpConnectionState::New,
            peer_window: 0,
            transit_window,
            window_wakers: vec![],
            sequence_number: SequenceNumber(0),
            ack_number: SequenceNumber(0),
            recv_packets: vec![],
            reassembled_buffer: vec![],
            recv_wakers: vec![],
            rtt_estimate: Duration::from_millis(0),
            send_times: HashMap::new(),
            peer_ack_number: SequenceNumber(0),
            sent_packets: HashMap::new(),
            event_sender: None,
        }
    }

    pub(super) fn config(&self) -> &UdpConnectionConfig {
        &self.config
    }

    pub(super) fn state(&self) -> UdpConnectionState {
        self.state
    }

    pub(super) fn is_connected(&self) -> bool {
        self.state == UdpConnectionState::Connected
    }

    pub(super) fn set_state_sent_hello(&mut self) {
        assert!(self.state == UdpConnectionState::New);
        debug!("connection state set to SENT_HELLO");
        self.state = UdpConnectionState::SentHello;
    }

    pub(super) fn set_state_sent_sync(&mut self) {
        assert!(self.state == UdpConnectionState::SentHello);
        debug!("connection state set to SENT_SYNC");
        self.state = UdpConnectionState::SentSync;
    }

    pub(super) fn set_state_waiting_for_sync(&mut self) {
        assert!(self.state == UdpConnectionState::SentHello);
        debug!("connection state set to WAITING_FOR_SYNC");
        self.state = UdpConnectionState::WaitingForSync;
    }

    pub(super) fn set_state_connect_failed(&mut self) {
        assert!(
            self.state == UdpConnectionState::SentHello
                || self.state == UdpConnectionState::SentSync
                || self.state == UdpConnectionState::WaitingForSync
        );
        debug!("connection state set to CONNECT_FAILED");
        self.state = UdpConnectionState::ConnectFailed;
    }

    pub(super) fn set_state_connected(&mut self, event_sender: UnboundedSender<SendEvent>) {
        assert!(
            self.state == UdpConnectionState::SentSync
                || self.state == UdpConnectionState::WaitingForSync
        );
        debug!("connection state set to CONNECTED");
        self.state = UdpConnectionState::Connected;
        self.event_sender.replace(event_sender);
    }

    pub(super) fn set_state_disconnected(&mut self) {
        assert!(self.state == UdpConnectionState::Connected);
        debug!("connection state set to DISCONNECTED");
        self.state = UdpConnectionState::Disconnected;
    }

    pub(super) fn event_sender(&self) -> UnboundedSender<SendEvent> {
        self.event_sender.as_ref().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_vars() {
        let mut vars = UdpConnectionVars::new(UdpConnectionConfig::default());

        assert_eq!(vars.state, UdpConnectionState::New);
    }

    #[test]
    fn test_state_transition_master_side() {
        let mut vars = UdpConnectionVars::new(UdpConnectionConfig::default());

        assert_eq!(vars.state, UdpConnectionState::New);

        vars.set_state_sent_hello();

        assert_eq!(vars.state, UdpConnectionState::SentHello);

        vars.set_state_sent_sync();

        assert_eq!(vars.state, UdpConnectionState::SentSync);

        vars.set_state_connected(tokio::sync::mpsc::unbounded_channel().0);

        assert_eq!(vars.state, UdpConnectionState::Connected);

        vars.set_state_disconnected();

        assert_eq!(vars.state, UdpConnectionState::Disconnected);
    }

    #[test]
    fn test_state_transition_client_side() {
        let mut vars = UdpConnectionVars::new(UdpConnectionConfig::default());

        assert_eq!(vars.state, UdpConnectionState::New);

        vars.set_state_sent_hello();

        assert_eq!(vars.state, UdpConnectionState::SentHello);

        vars.set_state_waiting_for_sync();

        assert_eq!(vars.state, UdpConnectionState::WaitingForSync);

        vars.set_state_connected(tokio::sync::mpsc::unbounded_channel().0);

        assert_eq!(vars.state, UdpConnectionState::Connected);

        vars.set_state_disconnected();

        assert_eq!(vars.state, UdpConnectionState::Disconnected);
    }
}
