use super::MAX_PACKET_SIZE;
use std::time::Duration;

const DEFAULT_KEEP_ALIVE_INTERVAL: u64 = 15000; // ms
const DEFAULT_INITIAL_TRANSIT_WINDOW: u32 = 102400; // bytes
const DEFAULT_RECV_WINDOW: u32 = 102400; // bytes

#[derive(Debug, Clone)]
pub struct UdpConnectionConfig {
    /// How often to send a keep-alive packet    
    keep_alive_interval: Duration,

    /// Duration to wait for a packet before assuming the connection has dropped.
    recv_timeout: Duration,

    /// The initial amount of bytes permitted to be unacknowledged at the start of the connection.
    initial_transit_window: u32,

    /// The amount of bytes permitted in the reassembled byte buffer
    recv_window: u32,
}

impl UdpConnectionConfig {
    pub fn default() -> Self {
        Self {
            keep_alive_interval: Duration::from_millis(DEFAULT_KEEP_ALIVE_INTERVAL),
            recv_timeout: Duration::from_millis(DEFAULT_KEEP_ALIVE_INTERVAL * 2),
            initial_transit_window: DEFAULT_INITIAL_TRANSIT_WINDOW,
            recv_window: DEFAULT_RECV_WINDOW,
        }
    }

    pub fn keep_alive_interval(&self) -> Duration {
        self.keep_alive_interval
    }

    pub fn with_keep_alive_interval(mut self, value: Duration) -> Self {
        self.keep_alive_interval = value;

        self
    }

    pub fn recv_timeout(&self) -> Duration {
        self.recv_timeout
    }

    pub fn with_recv_timeout(mut self, value: Duration) -> Self {
        self.recv_timeout = value;

        self
    }

    pub fn initial_transit_window(&self) -> u32 {
        self.initial_transit_window
    }

    pub fn with_initial_transit_window(mut self, value: u32) -> Self {
        self.initial_transit_window = value;

        self
    }

    pub fn recv_window(&self) -> u32 {
        self.recv_window
    }

    pub fn with_recv_window(mut self, value: u32) -> Self {
        assert!(value >= MAX_PACKET_SIZE as u32);
        self.recv_window = value;

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = UdpConnectionConfig::default();

        assert_eq!(
            config.keep_alive_interval.as_millis() as u64,
            DEFAULT_KEEP_ALIVE_INTERVAL
        );
        assert_eq!(
            config.recv_timeout.as_millis() as u64,
            DEFAULT_KEEP_ALIVE_INTERVAL * 2
        );
        assert_eq!(
            config.initial_transit_window,
            DEFAULT_INITIAL_TRANSIT_WINDOW
        );
        assert_eq!(config.recv_window, DEFAULT_RECV_WINDOW);
    }
}
