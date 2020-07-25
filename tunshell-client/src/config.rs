use std::env;

pub struct Config {
    session_key: String,
    encryption_salt: String,
    encryption_key: String,
    relay_host: String,
    relay_port: u16,
}

impl Config {
    pub fn new_from_env() -> Self {
        let mut args = env::args().into_iter();

        args.next().expect("first argument must be set");

        let session_key = args.next().expect("second argument argument must be set");
        let encryption_salt = args.next().expect("third argument argument must be set");
        let encryption_key = args.next().expect("fourth argument argument must be set");

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
            session_key,
            relay_host: "relay.tunshell.com".to_owned(),
            relay_port: 5000,
            encryption_salt,
            encryption_key,
        }
    }

    pub fn new(
        client_key: &str,
        relay_host: &str,
        relay_port: u16,
        encryption_salt: &str,
        encryption_key: &str,
    ) -> Self {
        Self {
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
