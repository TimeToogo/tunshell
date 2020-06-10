use std::env;

pub struct Config {
    client_key: String,
    relay_host: String,
    relay_port: u16,
}

impl Config {
    pub fn new_from_env() -> Self {
        let client_key = env::var("TUNSHELL_KEY").expect("TUNSHELL_KEY environment variable must be set");

        Self {
            client_key,
            relay_host: "relay.tunshell.com".to_owned(),
            relay_port: 5000,
        }
    }

    pub fn new(client_key: &str, relay_host: &str, relay_port: u16) -> Self {
        Self {
            client_key: client_key.to_owned(),
            relay_host: relay_host.to_owned(),
            relay_port,
        }
    }

    pub fn client_key(&self) -> &str {
        &self.client_key[..]
    }

    pub fn relay_host(&self) -> &str {
        &self.relay_host[..]
    }

    pub fn relay_port(&self) -> u16 {
        self.relay_port
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_from_env() {
        // Force tests to run sequentially
        // since they mock env vars

        test_new_from_env_with_var();

        let result = std::panic::catch_unwind(|| {
            test_should_panic_without_key_env_var();
        });
        assert_eq!(result.is_err(), true);
    }

    fn test_new_from_env_with_var() {
        env::set_var("TUNSHELL_KEY", "Example key");

        let config = Config::new_from_env();

        assert_eq!(config.client_key(), "Example key");
        assert!(config.relay_host().len() > 0);
        assert!(config.relay_port() > 0);
    }

    fn test_should_panic_without_key_env_var() {
        env::remove_var("TUNSHELL_KEY");

        Config::new_from_env();
    }
}
