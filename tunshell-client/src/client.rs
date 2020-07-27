use crate::{AesStream, ClientMode, Config, RelayStream, ServerStream, ShellKey, TunnelStream};
use anyhow::{Error, Result};
use futures::stream::StreamExt;
use log::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_util::compat::*;
use tunshell_shared::*;

pub type ClientMessageStream = MessageStream<ClientMessage, ServerMessage, Compat<ServerStream>>;

pub struct Client {
    config: Config,
}

impl Client {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn start_session(&mut self) -> Result<u8> {
        println!("Connecting to relay server...");
        let relay_socket = ServerStream::connect(&self.config).await?;

        let mut message_stream = ClientMessageStream::new(relay_socket.compat());

        self.send_key(&mut message_stream).await?;

        println!("Waiting for peer to join...");
        let mut peer_info = self.wait_for_peer_to_join(&mut message_stream).await?;
        println!("{} joined the session", peer_info.peer_ip_address);

        println!("Negotiating connection...");
        let message_stream = Arc::new(Mutex::new(message_stream));
        let peer_socket = self
            .negotiate_peer_connection(&message_stream, &mut peer_info, self.config.is_target())
            .await?;

        let exit_code = match self.config.mode() {
            ClientMode::Target => self.start_shell_server(peer_socket).await?,
            ClientMode::Local => self.start_shell_client(peer_socket).await?
        };

        Ok(exit_code)
    }

    async fn send_key(&self, message_stream: &mut ClientMessageStream) -> Result<()> {
        message_stream
            .write(&ClientMessage::Key(KeyPayload {
                key: self.config.session_key().to_owned(),
            }))
            .await?;

        match message_stream.next().await {
            Some(Ok(ServerMessage::KeyAccepted)) => Ok(()),
            Some(Ok(ServerMessage::KeyRejected)) => {
                Err(Error::msg("The session key has expired or is invalid"))
            }
            Some(Ok(message)) => Err(Error::msg(format!(
                "Unexpected response returned by server: {:?}",
                message
            ))),
            Some(Err(err)) => return Err(err),
            None => return Err(Error::msg("Connection closed unexpectedly")),
        }
    }

    async fn wait_for_peer_to_join(
        &mut self,
        message_stream: &mut ClientMessageStream,
    ) -> Result<PeerJoinedPayload> {
        match message_stream.next().await {
            Some(Ok(ServerMessage::AlreadyJoined)) => {
                return Err(Error::msg(
                    "Connection has already been joined by another host",
                ))
            }
            Some(Ok(ServerMessage::PeerJoined(payload))) => Ok(payload),
            _ => Err(Error::msg("Unexpected response returned by server")),
        }
    }

    async fn negotiate_peer_connection(
        &mut self,
        message_stream: &Arc<Mutex<ClientMessageStream>>,
        peer_info: &mut PeerJoinedPayload,
        master_side: bool,
    ) -> Result<Box<dyn TunnelStream>> {
        let stream = loop {
            let mut msg_stream = message_stream.lock().unwrap();
            match msg_stream.next().await {
                Some(Ok(ServerMessage::AttemptDirectConnect(payload))) => {
                    match self
                        .attempt_direct_connection(
                            &mut msg_stream,
                            peer_info,
                            &payload,
                            master_side,
                        )
                        .await?
                    {
                        Some(direct_stream) => {
                            println!("Direct connection to peer established");
                            break direct_stream;
                        }
                        None => {
                            msg_stream
                                .write(&ClientMessage::DirectConnectFailed)
                                .await?
                        }
                    }
                }
                Some(Ok(ServerMessage::StartRelayMode)) => {
                    break Box::new(RelayStream::new(Arc::clone(message_stream)))
                }
                Some(Ok(message)) => {
                    return Err(Error::msg(format!(
                        "Unexpected response returned by server: {:?}",
                        message
                    )))
                }
                Some(Err(err)) => return Err(err),
                None => return Err(Error::msg("Connection closed unexpectedly")),
            }
        };

        let stream = AesStream::new(
            stream.compat(),
            self.config.encryption_salt().as_bytes(),
            self.config.encryption_key().as_bytes(),
        );

        println!("Falling back to relayed connection");
        Ok(Box::new(stream))
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn attempt_direct_connection(
        &mut self,
        _message_stream: &mut ClientMessageStream,
        peer_info: &mut PeerJoinedPayload,
        connection_info: &AttemptDirectConnectPayload,
        master_side: bool,
    ) -> Result<Option<Box<dyn TunnelStream>>> {
        use crate::p2p::{self, P2PConnection};

        println!(
            "Attempting direct connection to {}",
            peer_info.peer_ip_address
        );

        // Initialise and bind sockets
        let mut tcp = p2p::tcp::TcpConnection::new(peer_info.clone(), connection_info.clone());
        let mut udp =
            p2p::udp_adaptor::UdpConnectionAdaptor::new(peer_info.clone(), connection_info.clone());

        tokio::try_join!(tcp.bind(), udp.bind())?;

        let current_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let sleep_duration = if connection_info.connect_at < current_timestamp {
            0
        } else {
            connection_info.connect_at - current_timestamp
        };

        std::thread::sleep(Duration::from_millis(sleep_duration));

        let stream: Option<Box<dyn TunnelStream>> = tokio::select! {
            result = tcp.connect(master_side) => match result {
                Ok(_) => Some(Box::new(tcp)),
                Err(err) => {
                    warn!("Error while establishing TCP connection: {}", err);
                    None
                }
            },
            result = udp.connect(master_side) => match result {
                Ok(_) => Some(Box::new(udp)),
                Err(err) => {
                    warn!("Error while establishing UDP connection: {}", err);
                    None
                }
            }
        };

        let stream = match stream {
            Some(stream) => stream,
            None => return Ok(None),
        };

        Ok(Some(stream))
    }

    #[cfg(target_arch = "wasm32")]
    async fn attempt_direct_connection(
        &mut self,
        _message_stream: &mut ClientMessageStream,
        peer_info: &mut PeerJoinedPayload,
        connection_info: &AttemptDirectConnectPayload,
        master_side: bool,
    ) -> Result<Option<Box<dyn TunnelStream>>> {
        Err(Error::msg("not supported"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn start_shell_server(&self, peer_socket: Box<dyn TunnelStream>) -> Result<u8> {
        crate::ShellServer::new()?
            .run(peer_socket, ShellKey::new(self.config.encryption_key()))
            .await
            .and_then(|_| Ok(0))
    }

    #[cfg(target_arch = "wasm32")]
    async fn start_shell_server(&self, peer_socket: Box<dyn TunnelStream>) -> Result<u8> {
        unreachable!()
    }

    async fn start_shell_client(&self, peer_socket: Box<dyn TunnelStream>) -> Result<u8> {
        crate::ShellClient::new()?
            .connect(peer_socket, ShellKey::new(self.config.encryption_key()))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_send_invalid_key() {
        let config = Config::new(
            ClientMode::Target,
            "invalid_key",
            "relay.tunshell.com",
            5000,
            "test",
            "test",
        );
        let mut client = Client::new(config);

        let result = Runtime::new().unwrap().block_on(client.start_session());

        match result {
            Ok(_) => panic!("should not return ok"),
            Err(err) => assert_eq!(err.to_string(), "The session key has expired or is invalid"),
        }
    }
}
