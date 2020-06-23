use super::{CongestionControl, PacketResender, RecvStore, RttEstimator, UdpConnectionConfig};
use log::*;
use std::time::Duration;

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
    /// The current state of the connection
    state: UdpConnectionState,

    congestion: CongestionControl,

    recv: RecvStore,

    rtt: RttEstimator,

    resender: PacketResender,
}

impl UdpConnectionVars {
    pub(super) fn new(config: &UdpConnectionConfig) -> Self {
        debug!("connection state initialised to NEW");
        Self {
            state: UdpConnectionState::New,
            congestion: CongestionControl::new(config),
            recv: RecvStore::new(),
            rtt: RttEstimator::new(),
            resender: PacketResender::new(),
        }
    }

    pub(super) fn state(&self) -> UdpConnectionState {
        self.state
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

    pub(super) fn set_state_connected(&mut self) {
        assert!(
            self.state == UdpConnectionState::SentSync
                || self.state == UdpConnectionState::WaitingForSync
        );
        debug!("connection state set to CONNECTED");
        self.state = UdpConnectionState::Connected;
    }

    pub(super) fn set_state_disconnected(&mut self) {
        assert!(self.state == UdpConnectionState::Connected);
        debug!("connection state set to DISCONNECTED");
        self.state = UdpConnectionState::Disconnected;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_vars() {
        let config = UdpConnectionConfig::default();
        let mut vars = UdpConnectionVars::new(&config);

        assert_eq!(vars.state, UdpConnectionState::New);
    }

    #[test]
    fn test_state_transition_master_side() {
        let mut vars = UdpConnectionVars::new(&UdpConnectionConfig::default());

        assert_eq!(vars.state, UdpConnectionState::New);

        vars.set_state_sent_hello();

        assert_eq!(vars.state, UdpConnectionState::SentHello);

        vars.set_state_sent_sync();

        assert_eq!(vars.state, UdpConnectionState::SentSync);

        vars.set_state_connected();

        assert_eq!(vars.state, UdpConnectionState::Connected);

        vars.set_state_disconnected();

        assert_eq!(vars.state, UdpConnectionState::Disconnected);
    }

    #[test]
    fn test_state_transition_client_side() {
        let mut vars = UdpConnectionVars::new(&UdpConnectionConfig::default());

        assert_eq!(vars.state, UdpConnectionState::New);

        vars.set_state_sent_hello();

        assert_eq!(vars.state, UdpConnectionState::SentHello);

        vars.set_state_waiting_for_sync();

        assert_eq!(vars.state, UdpConnectionState::WaitingForSync);

        vars.set_state_connected();

        assert_eq!(vars.state, UdpConnectionState::Connected);

        vars.set_state_disconnected();

        assert_eq!(vars.state, UdpConnectionState::Disconnected);
    }
}
