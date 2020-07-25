use anyhow::Error;
use std::{convert::TryFrom, env};

pub struct Config {
    mode: ClientMode,
    session_key: String,
    encryption_salt: String,
    encryption_key: String,
    relay_host: String,
    relay_port: u16,
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
        let encryption_salt = args.next().expect("encryption salt arg (4) must be set");
        let encryption_key = args.next().expect("encryption key arg (5) must be set");

        if session_key.len() < 10 {
            panic!("session key is too short")
        }

        if encryption_salt.len() < 5 {
            panic!("encryption salt is too short")
        }

        if encryption_key.len() < 10 {
            panic!("encryption key is too short")
        }

        Self {
            mode,
            session_key,
            relay_host: "relay.tunshell.com".to_owned(),
            relay_port: 5000,
            encryption_salt,
            encryption_key,
        }
    }

    pub fn new(
        mode: ClientMode,
        client_key: &str,
        relay_host: &str,
        relay_port: u16,
        encryption_salt: &str,
        encryption_key: &str,
    ) -> Self {
        Self {
            mode,
            session_key: client_key.to_owned(),
            relay_host: relay_host.to_owned(),
            relay_port,
            encryption_salt: encryption_salt.to_owned(),
            encryption_key: encryption_key.to_owned(),
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

    pub fn relay_port(&self) -> u16 {
        self.relay_port
    }

    pub fn encryption_salt(&self) -> &str {
        &self.encryption_salt[..]
    }

    pub fn encryption_key(&self) -> &str {
        &self.encryption_key[..]
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
