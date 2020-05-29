use crate::config::Config;
use anyhow::Error;
use anyhow::Result;
use dmp_shared::*;
use futures::stream::StreamExt;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::{client::TlsStream, rustls::ClientConfig, TlsConnector};
use tokio_util::compat::*;
use webpki::DNSNameRef;

type ClientMessageStream =
    MessageStream<ClientMessage, ServerMessage, Compat<TlsStream<TcpStream>>>;

pub struct Client<'a> {
    config: &'a Config,
}

impl<'a> Client<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self { config }
    }

    pub async fn start_session(&mut self) -> Result<()> {
        println!("Connecting to relay server...");
        let relay_socket: TlsStream<TcpStream> = self.connect_to_relay().await?;

        let mut message_stream = ClientMessageStream::new(relay_socket.compat());

        let key_type = self.send_key(&mut message_stream).await?;

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

    async fn send_key(&self, message_stream: &mut ClientMessageStream) -> Result<KeyType> {
        message_stream
            .write(&ClientMessage::Key(KeyPayload {
                key: self.config.client_key().to_owned(),
            }))
            .await?;

        match message_stream.next().await {
            Some(Ok(ServerMessage::KeyAccepted(payload))) => Ok(payload.key_type),
            Some(Ok(ServerMessage::KeyRejected)) => {
                eprintln!("The session key has expired or is invalid");
                Err(Error::msg("test"))
            }
            _ => {
                eprintln!("Unexpected response returned by server");
                Err(Error::msg("test"))
            }
        }
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

        let result = Runtime::new()
            .unwrap()
            .block_on(client.connect_to_relay());

        result.unwrap();
    }
}
