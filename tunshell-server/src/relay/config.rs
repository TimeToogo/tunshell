use anyhow::Result;
use std::env;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Config {
    pub(super) port: u16,
}

impl Config {
    pub(super) fn from_env() -> Result<Config> {
        let port = env::var("TUNSHELL_RELAY_PORT")?;
        let port = port.parse::<u16>()?;

        Ok(Config { port })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env() {
        env::remove_var("TUNSHELL_RELAY_PORT");

        Config::from_env().unwrap_err();

        env::set_var("TUNSHELL_RELAY_PORT", "invalid");

        Config::from_env().unwrap_err();

        env::set_var("TUNSHELL_RELAY_PORT", "1234");

        assert_eq!(Config::from_env().unwrap(), Config { port: 1234 })
    }
}
