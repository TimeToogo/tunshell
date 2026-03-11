use std::{collections::HashMap, fmt::Display, net::{SocketAddr, ToSocketAddrs}};

use anyhow::{Error, Result};
use futures::{
    future::{BoxFuture, FutureExt},
    stream::{FuturesUnordered, StreamExt},
};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    net::{
        udp::{RecvHalf as UdpRecvHalf, SendHalf as UdpSendHalf},
        TcpListener, TcpStream, UdpSocket,
    },
    sync::mpsc::{Receiver, Sender},
};

use crate::shell::{proto::ConId, NetworkMessage};

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Copy)]
pub enum NetworkPeerBindingDirection {
    LocalToRemote,
    RemoteToLocal,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Copy)]
pub enum NetworkPeerProtocol {
    Tcp,
    Udp,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct NetworkPeerBinding {
    pub direction: NetworkPeerBindingDirection,
    pub protocol: NetworkPeerProtocol,
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
}

impl NetworkPeerBinding {
    pub fn invert(&self) -> Self {
        Self {
            direction: match self.direction {
                NetworkPeerBindingDirection::LocalToRemote => {
                    NetworkPeerBindingDirection::RemoteToLocal
                }
                NetworkPeerBindingDirection::RemoteToLocal => {
                    NetworkPeerBindingDirection::LocalToRemote
                }
            },
            protocol: self.protocol,
            local_addr: self.remote_addr.clone(),
            local_port: self.remote_port,
            remote_addr: self.local_addr.clone(),
            remote_port: self.local_port,
        }
    }
}

impl Display for NetworkPeerBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{} {} {}:{} ({})",
            self.local_addr,
            self.local_port,
            match self.direction {
                NetworkPeerBindingDirection::LocalToRemote => "->",
                NetworkPeerBindingDirection::RemoteToLocal => "<-",
            },
            self.remote_addr,
            self.remote_port,
            match self.protocol {
                NetworkPeerProtocol::Tcp => "TCP",
                NetworkPeerProtocol::Udp => "UDP",
            }
        )
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct NetworkPeerConfig {
    pub bindings: Vec<NetworkPeerBinding>,
}

impl NetworkPeerConfig {
    pub fn new(bindings: Vec<NetworkPeerBinding>) -> Self {
        Self { bindings }
    }

    pub fn invert(&self) -> Self {
        Self {
            bindings: self
                .bindings
                .iter()
                .map(|binding| binding.invert())
                .collect(),
        }
    }
}

type TcpAcceptEvent = (
    usize,
    NetworkPeerBinding,
    TcpListener,
    std::io::Result<(TcpStream, SocketAddr)>,
);
type TcpAcceptFuture = BoxFuture<'static, TcpAcceptEvent>;

type TcpReadEvent = (ConId, ReadHalf<TcpStream>, std::io::Result<Vec<u8>>);
type TcpReadFuture = BoxFuture<'static, TcpReadEvent>;

type UdpReadEvent = (ConId, UdpRecvHalf, std::io::Result<(Vec<u8>, SocketAddr)>);
type UdpReadFuture = BoxFuture<'static, UdpReadEvent>;

struct UdpBindingState {
    receiver: Option<UdpRecvHalf>,
    sender: UdpSendHalf,
    target_addr: SocketAddr,
    bound_locally: bool,
    last_peer_addr: Option<SocketAddr>,
}

pub struct NetworkPeer {
    config: NetworkPeerConfig,
    rx: Receiver<NetworkMessage>,
    tx: Sender<NetworkMessage>,
    tcp_listeners: Vec<(NetworkPeerBinding, TcpListener)>,
    tcp_pending: HashMap<ConId, TcpStream>,
    tcp_connections: HashMap<ConId, WriteHalf<TcpStream>>,
    udp_bindings: HashMap<ConId, UdpBindingState>,
}

impl NetworkPeer {
    pub async fn new(
        config: NetworkPeerConfig,
    ) -> (Self, Receiver<NetworkMessage>, Sender<NetworkMessage>) {
        let (recv_tx, recv_rx) = tokio::sync::mpsc::channel(100);
        let (send_tx, send_rx) = tokio::sync::mpsc::channel(100);
        let peer = Self {
            config,
            rx: recv_rx,
            tx: send_tx,
            tcp_listeners: Vec::new(),
            tcp_pending: HashMap::new(),
            tcp_connections: HashMap::new(),
            udp_bindings: HashMap::new(),
        };

        (peer, send_rx, recv_tx)
    }

    pub async fn run(mut self) -> Result<()> {
        for (binding_idx, binding) in self.config.bindings.clone().into_iter().enumerate() {
            if let Err(err) = self.setup_binding(binding_idx as ConId + 1, &binding).await {
                info!("error setting up network peer binding {}: {}", binding, err);
                self.notice(format!(
                    "error setting up network peer binding {}: {}",
                    binding, err
                ))
                .await?;
            }
        }

        let mut tcp_accepts = FuturesUnordered::new();
        for (listener_idx, (binding, listener)) in std::mem::take(&mut self.tcp_listeners)
            .into_iter()
            .enumerate()
        {
            tcp_accepts.push(accept_once(listener_idx, binding, listener));
        }

        let mut tcp_reads = FuturesUnordered::new();
        let mut udp_reads = FuturesUnordered::new();
        for (&con_id, binding) in self.udp_bindings.iter_mut() {
            let receiver = binding
                .receiver
                .take()
                .expect("udp receiver should be present when network peer starts");
            udp_reads.push(recv_udp_once(con_id, receiver));
        }

        loop {
            tokio::select! {
                message = self.rx.recv() => match message {
                    None => {
                        debug!("network peer channel closed");
                        break;
                    }
                    Some(message) => {
                        info!("received network message: {:?}", message);
                        if let Err(err) = self.handle_message(message, &mut tcp_reads).await {
                            info!("error handling network message: {}", err);
                        }
                    }
                },
                accept = tcp_accepts.next(), if !tcp_accepts.is_empty() => {
                    if let Some((listener_idx, binding, listener, result)) = accept {
                        tcp_accepts.push(accept_once(listener_idx, binding.clone(), listener));

                        match result {
                            Ok((stream, addr)) => {
                                let con_id = self.next_tcp_con_id();
                                self.tcp_pending.insert(con_id, stream);
                                info!(
                                    "accepted new TCP connection from {} on listener {}, assigned ConId {}",
                                    addr, listener_idx, con_id
                                );
                                self.tx
                                    .send(NetworkMessage::TcpConnect(
                                        con_id,
                                        binding.remote_addr.clone(),
                                        binding.remote_port,
                                    ))
                                    .await?;
                            }
                            Err(err) => {
                                info!("error accepting TCP connection on listener {}: {}", listener_idx, err);
                            }
                        }
                    }
                },
                read = tcp_reads.next(), if !tcp_reads.is_empty() => {
                    if let Some((con_id, reader, result)) = read {
                        self.handle_tcp_read(con_id, reader, result, &mut tcp_reads).await?;
                    }
                },
                read = udp_reads.next(), if !udp_reads.is_empty() => {
                    if let Some((con_id, receiver, result)) = read {
                        udp_reads.push(recv_udp_once(con_id, receiver));
                        self.handle_udp_read(con_id, result).await?;
                    }
                }
            }
        }

        Ok(())
    }

    async fn setup_binding(&mut self, con_id: ConId, binding: &NetworkPeerBinding) -> Result<()> {
        info!("setting up network peer binding: {}", binding);

        match (binding.direction, binding.protocol) {
            (NetworkPeerBindingDirection::LocalToRemote, NetworkPeerProtocol::Tcp) => {
                let listener =
                    TcpListener::bind(format!("{}:{}", binding.local_addr, binding.local_port))
                        .await?;
                self.tcp_listeners.push((binding.clone(), listener));
            }
            (NetworkPeerBindingDirection::LocalToRemote, NetworkPeerProtocol::Udp)
            | (NetworkPeerBindingDirection::RemoteToLocal, NetworkPeerProtocol::Udp) => {
                let bind_addr = match binding.direction {
                    NetworkPeerBindingDirection::LocalToRemote => {
                        format!("{}:{}", binding.local_addr, binding.local_port)
                    }
                    NetworkPeerBindingDirection::RemoteToLocal => "0.0.0.0:0".to_owned(),
                };

                let socket = UdpSocket::bind(bind_addr).await?;
                let (receiver, sender) = socket.split();
                let target_addr = resolve_socket_addr(
                    match binding.direction {
                        NetworkPeerBindingDirection::LocalToRemote => &binding.remote_addr,
                        NetworkPeerBindingDirection::RemoteToLocal => &binding.local_addr,
                    },
                    match binding.direction {
                        NetworkPeerBindingDirection::LocalToRemote => binding.remote_port,
                        NetworkPeerBindingDirection::RemoteToLocal => binding.local_port,
                    },
                )?;

                self.udp_bindings.insert(
                    con_id,
                    UdpBindingState {
                        receiver: Some(receiver),
                        sender,
                        target_addr,
                        bound_locally: binding.direction == NetworkPeerBindingDirection::LocalToRemote,
                        last_peer_addr: None,
                    },
                );
            }
            (NetworkPeerBindingDirection::RemoteToLocal, NetworkPeerProtocol::Tcp) => {}
        }

        Ok(())
    }

    async fn notice(&mut self, notice: String) -> Result<()> {
        self.tx.send(NetworkMessage::Notice(notice)).await?;
        Ok(())
    }

    async fn handle_message(
        &mut self,
        message: NetworkMessage,
        tcp_reads: &mut FuturesUnordered<TcpReadFuture>,
    ) -> Result<()> {
        match message {
            NetworkMessage::TcpConnect(con_id, addr, port) => {
                match TcpStream::connect(resolve_socket_addr(&addr, port)?).await {
                    Ok(stream) => {
                        self.activate_tcp_connection(con_id, stream, tcp_reads);
                        self.tx
                            .send(NetworkMessage::TcpConnectResult(con_id, None))
                            .await?;
                    }
                    Err(err) => {
                        self.tx
                            .send(NetworkMessage::TcpConnectResult(
                                con_id,
                                Some(err.to_string()),
                            ))
                            .await?;
                    }
                }
            }
            NetworkMessage::TcpConnectResult(con_id, err) => {
                let stream = match self.tcp_pending.remove(&con_id) {
                    Some(stream) => stream,
                    None => return Ok(()),
                };

                match err {
                    None => self.activate_tcp_connection(con_id, stream, tcp_reads),
                    Some(err) => {
                        self.notice(format!(
                            "failed to establish remote tcp connection {}: {}",
                            con_id, err
                        ))
                        .await?;
                    }
                }
            }
            NetworkMessage::TcpSend(con_id, payload) | NetworkMessage::TcpRecv(con_id, payload) => {
                if let Some(stream) = self.tcp_connections.get_mut(&con_id) {
                    stream.write_all(payload.as_slice()).await?;
                } else {
                    self.notice(format!("tcp connection {} not found", con_id))
                        .await?;
                }
            }
            NetworkMessage::TcpClose(con_id) => {
                self.tcp_pending.remove(&con_id);
                self.tcp_connections.remove(&con_id);
            }
            NetworkMessage::UdpSend(con_id, payload) | NetworkMessage::UdpRecv(con_id, payload) => {
                self.handle_udp_payload(con_id, payload).await?;
            }
            NetworkMessage::Notice(_) => {}
        }

        Ok(())
    }

    async fn handle_tcp_read(
        &mut self,
        con_id: ConId,
        reader: ReadHalf<TcpStream>,
        result: std::io::Result<Vec<u8>>,
        tcp_reads: &mut FuturesUnordered<TcpReadFuture>,
    ) -> Result<()> {
        match result {
            Ok(payload) if payload.is_empty() => {
                self.tcp_connections.remove(&con_id);
                self.tx.send(NetworkMessage::TcpClose(con_id)).await?;
            }
            Ok(payload) => {
                tcp_reads.push(read_tcp_once(con_id, reader));
                self.tx.send(NetworkMessage::TcpSend(con_id, payload)).await?;
            }
            Err(err) => {
                info!("error reading from tcp connection {}: {}", con_id, err);
                self.tcp_connections.remove(&con_id);
                self.tx.send(NetworkMessage::TcpClose(con_id)).await?;
            }
        }

        Ok(())
    }

    async fn handle_udp_read(
        &mut self,
        con_id: ConId,
        result: std::io::Result<(Vec<u8>, SocketAddr)>,
    ) -> Result<()> {
        let binding = match self.udp_bindings.get_mut(&con_id) {
            Some(binding) => binding,
            None => return Ok(()),
        };

        match result {
            Ok((payload, peer_addr)) => {
                if binding.bound_locally {
                    binding.last_peer_addr = Some(peer_addr);
                }

                self.tx.send(NetworkMessage::UdpSend(con_id, payload)).await?;
            }
            Err(err) => {
                info!("error reading from udp binding {}: {}", con_id, err);
            }
        }

        Ok(())
    }

    async fn handle_udp_payload(&mut self, con_id: ConId, payload: Vec<u8>) -> Result<()> {
        let binding = match self.udp_bindings.get_mut(&con_id) {
            Some(binding) => binding,
            None => {
                self.notice(format!("udp binding {} not found", con_id)).await?;
                return Ok(());
            }
        };

        if binding.bound_locally {
            match binding.last_peer_addr {
                Some(peer_addr) => {
                    binding.sender.send_to(payload.as_slice(), &peer_addr).await?;
                }
                None => {
                    self.notice(format!("udp binding {} has no local peer yet", con_id))
                        .await?;
                }
            }
        } else {
            binding
                .sender
                .send_to(payload.as_slice(), &binding.target_addr)
                .await?;
        }

        Ok(())
    }

    fn activate_tcp_connection(
        &mut self,
        con_id: ConId,
        stream: TcpStream,
        tcp_reads: &mut FuturesUnordered<TcpReadFuture>,
    ) {
        let (reader, writer) = tokio::io::split(stream);
        self.tcp_connections.insert(con_id, writer);
        tcp_reads.push(read_tcp_once(con_id, reader));
    }

    fn next_tcp_con_id(&self) -> ConId {
        let mut con_id = self.config.bindings.len() as ConId + self.tcp_pending.len() as ConId + 1;
        while self.tcp_pending.contains_key(&con_id) || self.tcp_connections.contains_key(&con_id)
        {
            con_id += 1;
        }

        con_id
    }
}

fn resolve_socket_addr(addr: &str, port: u16) -> Result<SocketAddr> {
    (addr, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| Error::msg(format!("failed to resolve socket address {}:{}", addr, port)))
}

fn accept_once(listener_idx: usize, binding: NetworkPeerBinding, mut listener: TcpListener) -> TcpAcceptFuture {
    async move {
        let result = listener.accept().await;
        (listener_idx, binding, listener, result)
    }
    .boxed()
}

fn read_tcp_once(con_id: ConId, mut reader: ReadHalf<TcpStream>) -> TcpReadFuture {
    async move {
        let mut buff = [0u8; 4096];
        let result = reader.read(&mut buff).await.map(|read| buff[..read].to_vec());
        (con_id, reader, result)
    }
    .boxed()
}

fn recv_udp_once(con_id: ConId, mut receiver: UdpRecvHalf) -> UdpReadFuture {
    async move {
        let mut buff = vec![0u8; 65535];
        let result = receiver
            .recv_from(buff.as_mut_slice())
            .await
            .map(|(read, peer_addr)| {
                buff.truncate(read);
                (buff, peer_addr)
            });
        (con_id, receiver, result)
    }
    .boxed()
}
