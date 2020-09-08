use anyhow::Error;
use std::{convert::TryFrom, env, time::Duration};

const DEFAULT_SERVER_CONNECT_TIMEOUT: u64 = 10000; // ms
const DEFAULT_DIRECT_CONNECT_TIMEOUT: u64 = 3000; // ms

pub struct Config {
    mode: ClientMode,
    session_key: String,
    encryption_key: String,
    relay_host: String,
    relay_tls_port: u16,
    relay_ws_port: u16,
    server_connection_timeout: Duration,
    direct_connection_timeout: Duration,
    enable_direct_connection: bool,
    dangerous_disable_relay_server_verification: bool,
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum ClientMode {
    Local,
    Target,
}

impl Config {
    pub fn new_from_env() -> Self {
        let mut args = env::args().into_iter();

        args.next().expect("first argument must be set");

        let mode =
            ClientMode::try_from(args.next().expect("mode arg (2) must be set").as_str()).unwrap();
        let session_key = args.next().expect("session key arg (3) must be set");
        let encryption_key = args.next().expect("encryption key arg (4) must be set");
        let relay_host = args.next().unwrap_or("relay.tunshell.com".to_owned());
        let relay_tls_port = args
            .next()
            .unwrap_or("5000".to_owned())
            .parse::<u16>()
            .expect("could not parse arg (6) as TLS port");
        let relay_ws_port = args
            .next()
            .unwrap_or("443".to_owned())
            .parse::<u16>()
            .expect("could not parse arg (7) as WebSocket port");

        if session_key.len() < 10 {
            panic!("session key is too short")
        }

        if encryption_key.len() < 10 {
            panic!("encryption key is too short")
        }

        Self {
            mode,
            session_key,
            relay_host,
            relay_tls_port,
            relay_ws_port,
            encryption_key,
            server_connection_timeout: Duration::from_millis(DEFAULT_SERVER_CONNECT_TIMEOUT),
            direct_connection_timeout: Duration::from_millis(DEFAULT_DIRECT_CONNECT_TIMEOUT),
            enable_direct_connection: true,
            dangerous_disable_relay_server_verification: false,
        }
    }

    pub fn new(
        mode: ClientMode,
        client_key: &str,
        relay_host: &str,
        relay_tls_port: u16,
        relay_ws_port: u16,
        encryption_key: &str,
        enable_direct_connection: bool,
    ) -> Self {
        Self {
            mode,
            session_key: client_key.to_owned(),
            relay_host: relay_host.to_owned(),
            relay_tls_port,
            relay_ws_port,
            encryption_key: encryption_key.to_owned(),
            server_connection_timeout: Duration::from_millis(DEFAULT_SERVER_CONNECT_TIMEOUT),
            direct_connection_timeout: Duration::from_millis(DEFAULT_DIRECT_CONNECT_TIMEOUT),
            enable_direct_connection,
            dangerous_disable_relay_server_verification: false,
        }
    }

    pub fn session_key(&self) -> &str {
        &self.session_key[..]
    }

    pub fn mode(&self) -> ClientMode {
        self.mode
    }

    pub fn is_target(&self) -> bool {
        self.mode == ClientMode::Target
    }

    pub fn relay_host(&self) -> &str {
        &self.relay_host[..]
    }

    pub fn relay_tls_port(&self) -> u16 {
        self.relay_tls_port
    }

    pub fn relay_ws_port(&self) -> u16 {
        self.relay_ws_port
    }

    pub fn encryption_key(&self) -> &str {
        &self.encryption_key[..]
    }

    pub fn server_connection_timeout(&self) -> Duration {
        self.server_connection_timeout
    }

    pub fn direct_connection_timeout(&self) -> Duration {
        self.direct_connection_timeout
    }

    pub fn enable_direct_connection(&self) -> bool {
        self.enable_direct_connection
    }

    pub fn dangerous_disable_relay_server_verification(&self) -> bool {
        self.dangerous_disable_relay_server_verification
    }

    pub fn set_dangerous_disable_relay_server_verification(&mut self, flag: bool) {
        log::warn!("disabling TLS cert verification for relay server");
        self.dangerous_disable_relay_server_verification = flag;
    }
}

impl TryFrom<&str> for ClientMode {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let value = match value {
            "T" => Self::Target,
            "L" => Self::Local,
            _ => {
                return Err(Error::msg(format!(
                    "invalid client mode parameter: {}",
                    value
                )))
            }
        };

        Ok(value)
    }
}
