use crate::shell::network::{
    NetworkPeerBinding, NetworkPeerBindingDirection, NetworkPeerConfig, NetworkPeerProtocol,
};
use anyhow::Error;
use std::{convert::TryFrom, env, time::Duration};

const DEFAULT_SERVER_CONNECT_TIMEOUT: u64 = 10000; // ms
const DEFAULT_DIRECT_CONNECT_TIMEOUT: u64 = 3000; // ms
const DEFAULT_RELAY_HOST: &str = "relay.tunshell.com";
const DEFAULT_RELAY_TLS_PORT: u16 = 5000;
const DEFAULT_RELAY_WS_PORT: u16 = 443;
const DEFAULT_BIND_HOST: &str = "localhost";

pub struct Config {
    mode: ClientMode,
    session_key: String,
    encryption_key: String,
    relay_host: String,
    relay_tls_port: u16,
    relay_ws_port: u16,
    server_connection_timeout: Duration,
    direct_connection_timeout: Duration,
    echo_stdout: bool,
    enable_direct_connection: bool,
    dangerous_disable_relay_server_verification: bool,
    network_peer_config: NetworkPeerConfig,
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
        let parsed_args = parse_remaining_args(args).expect("failed to parse args");

        if session_key.len() < 10 {
            panic!("session key is too short")
        }

        if encryption_key.len() < 10 {
            panic!("encryption key is too short")
        }

        Self {
            mode,
            session_key,
            relay_host: parsed_args.relay_host,
            relay_tls_port: parsed_args.relay_tls_port,
            relay_ws_port: parsed_args.relay_ws_port,
            encryption_key,
            server_connection_timeout: Duration::from_millis(DEFAULT_SERVER_CONNECT_TIMEOUT),
            direct_connection_timeout: Duration::from_millis(DEFAULT_DIRECT_CONNECT_TIMEOUT),
            echo_stdout: parsed_args.echo_stdout,
            enable_direct_connection: true,
            dangerous_disable_relay_server_verification: false,
            network_peer_config: parsed_args.network_peer_config,
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
        echo_stdout: bool,
        network_peer_config: NetworkPeerConfig,
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
            echo_stdout,
            dangerous_disable_relay_server_verification: false,
            network_peer_config,
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

    pub fn echo_stdout(&self) -> bool {
        self.echo_stdout
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

    pub fn network_peer_config(&self) -> &NetworkPeerConfig {
        &self.network_peer_config
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

struct ParsedArgs {
    relay_host: String,
    relay_tls_port: u16,
    relay_ws_port: u16,
    echo_stdout: bool,
    network_peer_config: NetworkPeerConfig,
}

fn parse_remaining_args<I>(args: I) -> Result<ParsedArgs, Error>
where
    I: IntoIterator<Item = String>,
{
    let mut echo_stdout = false;
    let mut bindings = vec![];
    let mut relay_host = None;
    let mut relay_tls_port = None;
    let mut relay_ws_port = None;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--echo" => echo_stdout = true,
            "--bind" => {
                let spec = args
                    .next()
                    .ok_or_else(|| Error::msg("missing value for --bind"))?;
                bindings.push(parse_network_binding(&spec)?);
            }
            _ if arg.starts_with("--bind=") => {
                bindings.push(parse_network_binding(&arg["--bind=".len()..])?);
            }
            _ if arg.starts_with("--") => {
                return Err(Error::msg(format!("Unknown argument: {}", arg)))
            }
            _ => {
                if relay_host.is_none() {
                    relay_host = Some(arg);
                } else if relay_tls_port.is_none() {
                    relay_tls_port = Some(parse_u16_arg(&arg, "relay TLS port")?);
                } else if relay_ws_port.is_none() {
                    relay_ws_port = Some(parse_u16_arg(&arg, "relay WebSocket port")?);
                } else {
                    return Err(Error::msg(format!("Unexpected argument: {}", arg)));
                }
            }
        }
    }

    Ok(ParsedArgs {
        relay_host: relay_host.unwrap_or_else(|| DEFAULT_RELAY_HOST.to_owned()),
        relay_tls_port: relay_tls_port.unwrap_or(DEFAULT_RELAY_TLS_PORT),
        relay_ws_port: relay_ws_port.unwrap_or(DEFAULT_RELAY_WS_PORT),
        echo_stdout,
        network_peer_config: NetworkPeerConfig::new(bindings),
    })
}

fn parse_network_binding(value: &str) -> Result<NetworkPeerBinding, Error> {
    let (binding, protocol) = if let Some((binding, protocol)) = value.rsplit_once('/') {
        let protocol = match protocol {
            "tcp" => NetworkPeerProtocol::Tcp,
            "udp" => NetworkPeerProtocol::Udp,
            _ => {
                return Err(Error::msg(format!(
                    "invalid network binding protocol: {}",
                    protocol
                )))
            }
        };

        (binding, protocol)
    } else {
        (value, NetworkPeerProtocol::Tcp)
    };

    let (local, remote, direction) = if let Some((local, remote)) = binding.split_once("->") {
        (local, remote, NetworkPeerBindingDirection::LocalToRemote)
    } else if let Some((local, remote)) = binding.split_once("<-") {
        (local, remote, NetworkPeerBindingDirection::RemoteToLocal)
    } else {
        return Err(Error::msg(format!(
            "network binding is missing direction arrow: {}",
            value
        )));
    };

    if local.contains("->")
        || local.contains("<-")
        || remote.contains("->")
        || remote.contains("<-")
    {
        return Err(Error::msg(format!(
            "network binding contains multiple direction arrows: {}",
            value
        )));
    }

    let (local_addr, local_port) = parse_network_endpoint(local)?;
    let (remote_addr, remote_port) = parse_network_endpoint(remote)?;

    Ok(NetworkPeerBinding {
        direction,
        protocol,
        local_addr,
        local_port,
        remote_addr,
        remote_port,
    })
}

fn parse_network_endpoint(value: &str) -> Result<(String, u16), Error> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::msg("network binding endpoint cannot be empty"));
    }

    let (addr, port) = match value.rsplit_once(':') {
        Some((addr, port)) if !addr.is_empty() => (addr.to_owned(), parse_port(port)?),
        _ => (DEFAULT_BIND_HOST.to_owned(), parse_port(value)?),
    };

    Ok((addr, port))
}

fn parse_port(value: &str) -> Result<u16, Error> {
    value
        .parse::<u16>()
        .map_err(|_| Error::msg(format!("invalid network binding port: {}", value)))
}

fn parse_u16_arg(value: &str, name: &str) -> Result<u16, Error> {
    value
        .parse::<u16>()
        .map_err(|_| Error::msg(format!("invalid {}: {}", name, value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_network_binding_full_tcp() {
        let binding = parse_network_binding("0.0.0.0:8080->remote.service:1234").unwrap();

        assert_eq!(
            binding,
            NetworkPeerBinding {
                direction: NetworkPeerBindingDirection::LocalToRemote,
                protocol: NetworkPeerProtocol::Tcp,
                local_addr: "0.0.0.0".to_owned(),
                local_port: 8080,
                remote_addr: "remote.service".to_owned(),
                remote_port: 1234,
            }
        );
    }

    #[test]
    fn test_parse_network_binding_remote_to_local_with_default_remote_host() {
        let binding = parse_network_binding("localhost:9090<-9090").unwrap();

        assert_eq!(
            binding,
            NetworkPeerBinding {
                direction: NetworkPeerBindingDirection::RemoteToLocal,
                protocol: NetworkPeerProtocol::Tcp,
                local_addr: "localhost".to_owned(),
                local_port: 9090,
                remote_addr: "localhost".to_owned(),
                remote_port: 9090,
            }
        );
    }

    #[test]
    fn test_parse_network_binding_udp_with_default_local_host() {
        let binding = parse_network_binding("5050->localhost:5555/udp").unwrap();

        assert_eq!(
            binding,
            NetworkPeerBinding {
                direction: NetworkPeerBindingDirection::LocalToRemote,
                protocol: NetworkPeerProtocol::Udp,
                local_addr: "localhost".to_owned(),
                local_port: 5050,
                remote_addr: "localhost".to_owned(),
                remote_port: 5555,
            }
        );
    }

    #[test]
    fn test_parse_remaining_args_with_bindings() {
        let parsed_args = parse_remaining_args(vec![
            "--echo".to_owned(),
            "--bind".to_owned(),
            "5050->localhost:5555/udp".to_owned(),
            "--bind=localhost:9090<-9090".to_owned(),
        ])
        .unwrap();

        assert_eq!(parsed_args.relay_host, DEFAULT_RELAY_HOST);
        assert_eq!(parsed_args.relay_tls_port, DEFAULT_RELAY_TLS_PORT);
        assert_eq!(parsed_args.relay_ws_port, DEFAULT_RELAY_WS_PORT);
        assert!(parsed_args.echo_stdout);
        assert_eq!(parsed_args.network_peer_config.bindings.len(), 2);
    }

    #[test]
    fn test_parse_remaining_args_with_mixed_positionals_and_flags() {
        let parsed_args = parse_remaining_args(vec![
            "relay.example.com".to_owned(),
            "--bind=5050->localhost:5555/udp".to_owned(),
            "5443".to_owned(),
            "--echo".to_owned(),
        ])
        .unwrap();

        assert_eq!(parsed_args.relay_host, "relay.example.com");
        assert_eq!(parsed_args.relay_tls_port, 5443);
        assert_eq!(parsed_args.relay_ws_port, DEFAULT_RELAY_WS_PORT);
        assert!(parsed_args.echo_stdout);
        assert_eq!(parsed_args.network_peer_config.bindings.len(), 1);
    }

    #[test]
    fn test_parse_remaining_args_with_bind_before_missing_positionals() {
        let parsed_args = parse_remaining_args(vec![
            "--bind".to_owned(),
            "localhost:9090<-9090".to_owned(),
            "relay.example.com".to_owned(),
        ])
        .unwrap();

        assert_eq!(parsed_args.relay_host, "relay.example.com");
        assert_eq!(parsed_args.relay_tls_port, DEFAULT_RELAY_TLS_PORT);
        assert_eq!(parsed_args.relay_ws_port, DEFAULT_RELAY_WS_PORT);
        assert_eq!(parsed_args.network_peer_config.bindings.len(), 1);
    }
}
