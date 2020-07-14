use anyhow::{Error, Result};
use rustls::{internal::pemfile, Certificate, NoClientAuth, PrivateKey, ServerConfig};
use std::fs;
use std::io;
use std::{env, sync::Arc};

#[derive(Clone)]
pub(super) struct Config {
    pub(super) port: u16,
    pub(super) tls_config: Arc<ServerConfig>,
}

impl Config {
    pub(super) fn from_env() -> Result<Config> {
        let port = env::var("TUNSHELL_RELAY_PORT")?;
        let port = port.parse::<u16>()?;

        let mut tls_config = ServerConfig::new(NoClientAuth::new());
        tls_config.set_single_cert(Self::parse_tls_cert()?, Self::parse_tls_private_key()?)?;
        let tls_config = Arc::new(tls_config);

        Ok(Config { port, tls_config })
    }

    pub(super) fn parse_tls_cert() -> Result<Vec<Certificate>> {
        let file = fs::File::open(env::var("TLS_RELAY_CERT")?)?;
        let mut reader = io::BufReader::new(file);

        pemfile::certs(&mut reader).map_err(|_| Error::msg("failed to parse tls cert file"))
    }

    pub(super) fn parse_tls_private_key() -> Result<PrivateKey> {
        let file = fs::File::open(env::var("TLS_RELAY_PRIVATE_KEY")?)?;
        let mut reader = io::BufReader::new(file);

        let keys = pemfile::pkcs8_private_keys(&mut reader)
            .map_err(|_| Error::msg("failed to parse tls private key file"))?;

        Ok(keys.into_iter().next().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env() {
        env::remove_var("TUNSHELL_RELAY_PORT");
        env::remove_var("TLS_RELAY_CERT");
        env::remove_var("TLS_RELAY_PRIVATE_KEY");

        assert!(Config::from_env().is_err());

        env::set_var("TUNSHELL_RELAY_PORT", "1234");
        env::set_var("TLS_RELAY_CERT", "certs/development.cert");
        env::set_var("TLS_RELAY_PRIVATE_KEY", "certs/development.key");

        let config = Config::from_env().unwrap();

        assert_eq!(config.port, 1234);
    }
}
