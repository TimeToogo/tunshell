use crate::config::Config;
use anyhow::Result;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::{client::TlsStream, rustls::ClientConfig, TlsConnector};
use webpki::DNSNameRef;

pub struct Client<'a> {
    config: &'a Config,
}

impl<'a> Client<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self { config }
    }

    pub async fn start_session(&mut self) -> Result<()> {
        let relay_socket: TlsStream<TcpStream> = self.connect_to_relay().await?;

        Ok(())
    }

    async fn connect_to_relay(&mut self) -> Result<TlsStream<TcpStream>> {
        let mut config = ClientConfig::default();
        config
            .root_store
            .add_server_trust_anchors(&webpki_roots::TLS_SERVER_ROOTS);
        
        let connector = TlsConnector::from(Arc::new(config));

        let relay_dns_name = DNSNameRef::try_from_ascii_str(self.config.relay_host())?;
        let relay_addr = (self.config.relay_host(), self.config.relay_port())
            .to_socket_addrs()?
            .next()
            .unwrap();

        let network_stream = TcpStream::connect(&relay_addr).await?;
        let transport_stream = connector.connect(relay_dns_name, network_stream).await?;

        Ok(transport_stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_connect_to_local() {
        let config = Config::new("test", "relay1.debugmypipeline.com", 5000);
        let mut client = Client::new(&config);

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(client.connect_to_relay());

        result.unwrap();
    }
}
