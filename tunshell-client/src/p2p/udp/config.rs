use super::MAX_PACKET_SIZE;
use std::net::SocketAddr;
use std::time::Duration;

const DEFAULT_CONNECT_TIMEOUT: u64 = 3000; // ms
const DEFAULT_KEEP_ALIVE_INTERVAL: u64 = 15000; // ms
const DEFAULT_INITIAL_TRANSIT_WINDOW: u32 = 102400; // bytes
const DEFAULT_RECV_WINDOW: u32 = 102400; // bytes
const DEFAULT_PACKET_RESEND_LIMIT: u8 = 10;

#[derive(Debug, Clone)]
pub struct UdpConnectionConfig {
    /// How long to allow for the connection to be negotiated
    connect_timeout: Duration,

    /// The address on which to bind on
    bind_addr: SocketAddr,

    /// How often to send a keep-alive packet    
    keep_alive_interval: Duration,

    /// Duration to wait for a packet before assuming the connection has dropped.
    recv_timeout: Duration,

    /// The initial amount of bytes permitted to be unacknowledged at the start of the connection.
    initial_transit_window: u32,

    /// The amount of bytes permitted in the reassembled byte buffer
    recv_window: u32,

    /// The amount of resend the connection will tolerate for a single packet
    packet_resend_limit: u8,
}

#[allow(dead_code)]
impl UdpConnectionConfig {
    pub fn default() -> Self {
        Self {
            connect_timeout: Duration::from_millis(DEFAULT_CONNECT_TIMEOUT),
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 0)),
            keep_alive_interval: Duration::from_millis(DEFAULT_KEEP_ALIVE_INTERVAL),
            recv_timeout: Duration::from_millis(DEFAULT_KEEP_ALIVE_INTERVAL * 2),
            initial_transit_window: DEFAULT_INITIAL_TRANSIT_WINDOW,
            recv_window: DEFAULT_RECV_WINDOW,
            packet_resend_limit: DEFAULT_PACKET_RESEND_LIMIT,
        }
    }

    pub fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    pub fn with_connect_timeout(mut self, value: Duration) -> Self {
        self.connect_timeout = value;

        self
    }

    pub fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    pub fn with_bind_addr(mut self, value: SocketAddr) -> Self {
        self.bind_addr = value;

        self
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

    pub fn packet_resend_limit(&self) -> u8 {
        self.packet_resend_limit
    }

    pub fn with_packet_resend_limit(mut self, value: u8) -> Self {
        self.packet_resend_limit = value;

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
            config.connect_timeout.as_millis() as u64,
            DEFAULT_CONNECT_TIMEOUT
        );
        assert_eq!(config.bind_addr, SocketAddr::from(([0, 0, 0, 0], 0)));
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
        assert_eq!(config.packet_resend_limit, DEFAULT_PACKET_RESEND_LIMIT);
    }
}
