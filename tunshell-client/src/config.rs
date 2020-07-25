use std::env;

pub struct Config {
    session_key: String,
    relay_host: String,
    relay_port: u16,
}

impl Config {
    pub fn new_from_env() -> Self {
        let mut args = env::args().into_iter();

        args.next().expect("first argument must be set");

        let session_key = args.next().expect("second argument argument must be set");

        Self {
            session_key,
            relay_host: "relay.tunshell.com".to_owned(),
            relay_port: 5000,
        }
    }

    pub fn new(client_key: &str, relay_host: &str, relay_port: u16) -> Self {
        Self {
            session_key: client_key.to_owned(),
            relay_host: relay_host.to_owned(),
            relay_port,
        }
    }

    pub fn session_key(&self) -> &str {
        &self.session_key[..]
    }

    pub fn relay_host(&self) -> &str {
        &self.relay_host[..]
    }

    pub fn relay_port(&self) -> u16 {
        self.relay_port
    }
}
