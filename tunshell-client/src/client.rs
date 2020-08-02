use crate::{
    AesStream, ClientMode, Config, HostShell, RelayStream, ServerStream, ShellKey, TunnelStream,
};
use anyhow::{Error, Result};
use futures::stream::StreamExt;
use log::*;
use std::sync::{Arc, Mutex};
use tokio_util::compat::*;
use tunshell_shared::*;

pub type ClientMessageStream = MessageStream<ClientMessage, ServerMessage, Compat<ServerStream>>;

pub struct Client {
    config: Config,
    host_shell: Option<HostShell>,
}

impl Client {
    pub fn new(config: Config, host_shell: HostShell) -> Self {
        Self {
            config,
            host_shell: Some(host_shell),
        }
    }

    pub async fn println(&mut self, line: &str) {
        self.host_shell.as_mut().unwrap().println(line).await;
    }

    pub async fn start_session(&mut self) -> Result<u8> {
        self.println("Connecting to relay server...").await;
        let relay_socket = ServerStream::connect(&self.config).await?;

        let mut message_stream = ClientMessageStream::new(relay_socket.compat());

        self.send_key(&mut message_stream).await?;

        self.println("Waiting for peer to join...").await;
        let mut peer_info = self.wait_for_peer_to_join(&mut message_stream).await?;
        self.println(&format!("{} joined the session", peer_info.peer_ip_address))
            .await;

        self.println("Negotiating connection...").await;
        let peer_socket = self
            .negotiate_peer_connection(message_stream, &mut peer_info, self.config.is_target())
            .await?;

        let exit_code = match self.config.mode() {
            ClientMode::Target => self.start_shell_server(peer_socket).await?,
            ClientMode::Local => self.start_shell_client(peer_socket).await?,
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
        mut message_stream: ClientMessageStream,
        peer_info: &mut PeerJoinedPayload,
        master_side: bool,
    ) -> Result<Box<dyn TunnelStream>> {
        let stream = loop {
            match message_stream.next().await {
                Some(Ok(ServerMessage::AttemptDirectConnect(payload))) => {
                    match self
                        .attempt_direct_connection(
                            &mut message_stream,
                            peer_info,
                            &payload,
                            master_side,
                        )
                        .await?
                    {
                        Some(direct_stream) => {
                            self.println("Direct connection to peer established").await;
                            break direct_stream;
                        }
                        None => {
                            message_stream
                                .write(&ClientMessage::DirectConnectFailed)
                                .await?
                        }
                    }
                }
                Some(Ok(ServerMessage::StartRelayMode)) => {
                    break Box::new(RelayStream::new(Arc::new(Mutex::new(message_stream))))
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

        assert!(peer_info.session_nonce.len() > 10);
        let stream = AesStream::new(
            stream.compat(),
            peer_info.session_nonce.as_bytes(),
            self.config.encryption_key().as_bytes(),
        )
        .await?;

        self.println("Falling back to relayed connection").await;
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
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        self.println(
            format!(
                "Attempting direct connection to {}",
                peer_info.peer_ip_address
            )
            .as_str(),
        )
        .await;

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
        _peer_info: &mut PeerJoinedPayload,
        _connection_info: &AttemptDirectConnectPayload,
        _master_side: bool,
    ) -> Result<Option<Box<dyn TunnelStream>>> {
        // Does not support direct connection when running in browser
        Ok(None)
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn start_shell_server(&self, peer_socket: Box<dyn TunnelStream>) -> Result<u8> {
        crate::ShellServer::new()?
            .run(peer_socket, ShellKey::new(self.config.encryption_key()))
            .await
            .and_then(|_| Ok(0))
    }

    #[cfg(target_arch = "wasm32")]
    async fn start_shell_server(&self, _peer_socket: Box<dyn TunnelStream>) -> Result<u8> {
        unreachable!()
    }

    async fn start_shell_client(&mut self, peer_socket: Box<dyn TunnelStream>) -> Result<u8> {
        let mut client = crate::ShellClient::new(self.host_shell.take().unwrap())?;
        let result = client
            .connect(peer_socket, ShellKey::new(self.config.encryption_key()))
            .await;

        self.host_shell.replace(client.host_shell);
        result
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
        );
        let mut client = Client::new(config, HostShell::new().unwrap());

        let result = Runtime::new().unwrap().block_on(client.start_session());

        match result {
            Ok(_) => panic!("should not return ok"),
            Err(err) => assert_eq!(err.to_string(), "The session key has expired or is invalid"),
        }
    }
}
