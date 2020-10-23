#[cfg(not(target_arch = "wasm32"))]
use crate::{p2p, ShellServerConfig};
use crate::{
    AesStream, ClientMode, Config, HostShell, RelayStream, ServerStream, ShellKey, TunnelStream,
};
use anyhow::{Error, Result};
use futures::{future, stream::StreamExt, FutureExt};
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
        let (peer_socket, message_stream) = self
            .negotiate_peer_connection(message_stream, &mut peer_info, self.config.is_target())
            .await?;

        let exit_code = match self.config.mode() {
            ClientMode::Target => self.start_shell_server(peer_socket).await?,
            ClientMode::Local => self.start_shell_client(peer_socket).await?,
        };

        let message_stream = if let Ok(message_stream) = Arc::try_unwrap(message_stream) {
            message_stream.into_inner().ok()
        } else {
            None
        };

        if let Some(mut message_stream) = message_stream {
            debug!("sending close message to server");
            message_stream.write(&ClientMessage::Close).await?;
        } else {
            warn!("failed to take ownership of message stream");
        }

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
            result @ _ => Err(self.handle_unexpected_message(result)),
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
            result @ _ => Err(self.handle_unexpected_message(result)),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn negotiate_peer_connection(
        &mut self,
        mut message_stream: ClientMessageStream,
        peer_info: &mut PeerJoinedPayload,
        master_side: bool,
    ) -> Result<(Box<dyn TunnelStream>, Arc<Mutex<ClientMessageStream>>)> {
        let mut bound = None;

        let (stream, message_stream) = loop {
            match message_stream.next().await {
                Some(Ok(ServerMessage::BindForDirectConnect)) => {
                    match self.bind_direct_connection(peer_info).await {
                        Some((tcp, udp, ports)) => {
                            message_stream
                                .write(&ClientMessage::DirectConnectBound(ports))
                                .await?;

                            bound = Some((tcp, udp));
                        }
                        None => {
                            message_stream
                                .write(&ClientMessage::DirectConnectFailed)
                                .await?
                        }
                    }
                }
                Some(Ok(ServerMessage::AttemptDirectConnect(ports))) => {
                    if bound.is_none() {
                        return Err(Error::msg(
                            "server attempted direct connection before binding",
                        ));
                    }

                    let bound = std::mem::replace(&mut bound, None).unwrap();
                    match self
                        .attempt_direct_connection(bound.0, bound.1, ports, master_side)
                        .await?
                    {
                        Some(direct_stream) => {
                            self.println("Direct connection to peer established").await;
                            message_stream
                                .write(&ClientMessage::DirectConnectSucceeded)
                                .await?;
                            let message_stream = Arc::new(Mutex::new(message_stream));
                            break (direct_stream, message_stream);
                        }
                        None => {
                            message_stream
                                .write(&ClientMessage::DirectConnectFailed)
                                .await?
                        }
                    }
                }
                Some(Ok(ServerMessage::StartRelayMode)) => {
                    self.println("Falling back to relayed connection").await;
                    let message_stream = Arc::new(Mutex::new(message_stream));
                    let relay_stream = Box::new(RelayStream::new(Arc::clone(&message_stream)));
                    break (relay_stream, message_stream);
                }
                result @ _ => return Err(self.handle_unexpected_message(result)),
            }
        };

        assert!(peer_info.session_nonce.len() > 10);

        let stream = AesStream::new(
            stream.compat(),
            peer_info.session_nonce.as_bytes(),
            self.config.encryption_key().as_bytes(),
        )
        .await?;

        Ok((Box::new(stream), message_stream))
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn bind_direct_connection(
        &mut self,
        peer_info: &mut PeerJoinedPayload,
    ) -> Option<(
        Option<p2p::tcp::TcpConnection>,
        Option<p2p::udp_adaptor::UdpConnectionAdaptor>,
        PortBindings,
    )> {
        use crate::p2p::P2PConnection;

        if !self.config.enable_direct_connection() {
            debug!("direct connections are disabled");
            return None;
        }

        self.println(
            format!(
                "Attempting direct connection to {}",
                peer_info.peer_ip_address
            )
            .as_str(),
        )
        .await;

        debug!("binding ports for direct connection");

        let mut tcp = p2p::tcp::TcpConnection::new(peer_info.clone());
        let mut udp = p2p::udp_adaptor::UdpConnectionAdaptor::new(peer_info.clone());

        let (tcp_port, udp_port) = future::join(tcp.bind(), udp.bind()).await;

        let ports = PortBindings {
            tcp_port: tcp_port.ok(),
            udp_port: udp_port.ok(),
        };

        debug!("direct connection ports: {:?}", ports);

        let tcp = ports.tcp_port.map(move |_| tcp);
        let udp = ports.udp_port.map(move |_| udp);

        Some((tcp, udp, ports))
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn attempt_direct_connection(
        &mut self,
        mut tcp: Option<p2p::tcp::TcpConnection>,
        mut udp: Option<p2p::udp_adaptor::UdpConnectionAdaptor>,
        peer_ports: PortBindings,
        master_side: bool,
    ) -> Result<Option<Box<dyn TunnelStream>>> {
        use crate::p2p::P2PConnection;

        let tcp_connection = match (&mut tcp, &peer_ports) {
            (
                Some(tcp),
                PortBindings {
                    tcp_port: Some(peer_port),
                    ..
                },
            ) => tcp.connect(*peer_port, master_side).boxed(),
            _ => future::pending().boxed(),
        };

        let udp_connection = match (&mut udp, &peer_ports) {
            (
                Some(udp),
                PortBindings {
                    udp_port: Some(peer_port),
                    ..
                },
            ) => udp.connect(*peer_port, master_side).boxed(),
            _ => future::pending().boxed(),
        };

        let timeout = self.config.direct_connection_timeout();

        let attempt_connection = future::join(
            tokio::time::timeout(timeout, tcp_connection),
            tokio::time::timeout(timeout, udp_connection),
        );

        let stream: Option<Box<dyn TunnelStream>> = match attempt_connection.await {
            (Ok(Ok(_)), _) => {
                info!("direct TCP connection established with peer");
                Some(Box::new(tcp.unwrap()))
            }
            (_, Ok(Ok(_))) => {
                info!("direct UDP connection established with peer");
                Some(Box::new(udp.unwrap()))
            }
            _ => None,
        };

        Ok(stream)
    }

    #[cfg(target_arch = "wasm32")]
    async fn negotiate_peer_connection(
        &mut self,
        mut message_stream: ClientMessageStream,
        peer_info: &mut PeerJoinedPayload,
        _master_side: bool,
    ) -> Result<(Box<dyn TunnelStream>, Arc<Mutex<ClientMessageStream>>)> {
        let (stream, message_stream) = loop {
            match message_stream.next().await {
                Some(Ok(ServerMessage::BindForDirectConnect)) => {
                    message_stream
                        .write(&ClientMessage::DirectConnectFailed)
                        .await?
                }
                Some(Ok(ServerMessage::StartRelayMode)) => {
                    self.println("Falling back to relayed connection").await;
                    let message_stream = Arc::new(Mutex::new(message_stream));
                    let relay_stream = Box::new(RelayStream::new(Arc::clone(&message_stream)));
                    break (relay_stream, message_stream);
                }
                result @ _ => return Err(self.handle_unexpected_message(result)),
            }
        };

        assert!(peer_info.session_nonce.len() > 10);

        let stream = AesStream::new(
            stream.compat(),
            peer_info.session_nonce.as_bytes(),
            self.config.encryption_key().as_bytes(),
        )
        .await?;

        Ok((Box::new(stream), message_stream))
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn start_shell_server(&self, peer_socket: Box<dyn TunnelStream>) -> Result<u8> {
        crate::ShellServer::new(ShellServerConfig {
            echo_stdout: self.config.echo_stdout(),
        })?
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

    fn handle_unexpected_message(&self, message: Option<Result<ServerMessage>>) -> Error {
        match message {
            Some(Ok(message)) => {
                return Error::msg(format!(
                    "Unexpected response returned by server: {:?}",
                    message
                ))
            }
            Some(Err(err)) => return err,
            None => return Error::msg("Connection closed unexpectedly"),
        }
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
            "au.relay.tunshell.com",
            5000,
            443,
            "test",
            true,
            false,
        );
        let mut client = Client::new(config, HostShell::new().unwrap());

        let result = Runtime::new().unwrap().block_on(client.start_session());

        match result {
            Ok(_) => panic!("should not return ok"),
            Err(err) => assert_eq!(err.to_string(), "The session key has expired or is invalid"),
        }
    }
}
