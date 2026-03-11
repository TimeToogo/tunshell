use std::{
    collections::HashMap,
    fmt::Display,
    io,
    net::{SocketAddr, ToSocketAddrs},
};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SocketAddrPreference {
    Any,
    V4,
    V6,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkPeerRole {
    Client,
    Server,
}

struct UdpBindingState {
    receiver: Option<UdpRecvHalf>,
    sender: UdpSendHalf,
    target_addr: SocketAddr,
    bound_locally: bool,
    last_peer_addr: Option<SocketAddr>,
    pending_payloads: Vec<Vec<u8>>,
}

pub struct NetworkPeer {
    config: NetworkPeerConfig,
    rx: Receiver<NetworkMessage>,
    tx: Sender<NetworkMessage>,
    tcp_listeners: Vec<(NetworkPeerBinding, TcpListener)>,
    tcp_pending: HashMap<ConId, TcpStream>,
    tcp_pending_writes: HashMap<ConId, Vec<Vec<u8>>>,
    tcp_connections: HashMap<ConId, WriteHalf<TcpStream>>,
    udp_bindings: HashMap<ConId, UdpBindingState>,
    next_tcp_con_id: ConId,
}

impl NetworkPeer {
    pub async fn new(
        config: NetworkPeerConfig,
        role: NetworkPeerRole,
    ) -> (Self, Receiver<NetworkMessage>, Sender<NetworkMessage>) {
        let (recv_tx, recv_rx) = tokio::sync::mpsc::channel(100);
        let (send_tx, send_rx) = tokio::sync::mpsc::channel(100);
        let peer = Self {
            config,
            rx: recv_rx,
            tx: send_tx,
            tcp_listeners: Vec::new(),
            tcp_pending: HashMap::new(),
            tcp_pending_writes: HashMap::new(),
            tcp_connections: HashMap::new(),
            udp_bindings: HashMap::new(),
            next_tcp_con_id: match role {
                NetworkPeerRole::Client => 1,
                NetworkPeerRole::Server => 2,
            },
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
                        debug!("received network message: {:?}", message);
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
                let target_addr = resolve_socket_addr(
                    match binding.direction {
                        NetworkPeerBindingDirection::LocalToRemote => &binding.remote_addr,
                        NetworkPeerBindingDirection::RemoteToLocal => &binding.local_addr,
                    },
                    match binding.direction {
                        NetworkPeerBindingDirection::LocalToRemote => binding.remote_port,
                        NetworkPeerBindingDirection::RemoteToLocal => binding.local_port,
                    },
                    udp_addr_preference(binding),
                )?;
                let bind_addr = match binding.direction {
                    NetworkPeerBindingDirection::LocalToRemote => {
                        udp_bind_addr(&binding.local_addr, binding.local_port, target_addr)
                    }
                    NetworkPeerBindingDirection::RemoteToLocal => {
                        udp_unspecified_bind_addr(target_addr).to_owned()
                    }
                };

                let socket = UdpSocket::bind(bind_addr).await?;
                let (receiver, sender) = socket.split();

                self.udp_bindings.insert(
                    con_id,
                    UdpBindingState {
                        receiver: Some(receiver),
                        sender,
                        target_addr,
                        bound_locally: binding.direction == NetworkPeerBindingDirection::LocalToRemote,
                        last_peer_addr: None,
                        pending_payloads: Vec::new(),
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
                match connect_socket_addr(&addr, port).await {
                    Ok(stream) => {
                        self.activate_tcp_connection(con_id, stream, tcp_reads).await?;
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
                    None => self.activate_tcp_connection(con_id, stream, tcp_reads).await?,
                    Some(err) => {
                        self.tcp_pending_writes.remove(&con_id);
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
                } else if self.tcp_pending.contains_key(&con_id) {
                    self.tcp_pending_writes
                        .entry(con_id)
                        .or_insert_with(Vec::new)
                        .push(payload);
                } else {
                    self.notice(format!("tcp connection {} not found", con_id))
                        .await?;
                }
            }
            NetworkMessage::TcpClose(con_id) => {
                self.tcp_pending.remove(&con_id);
                self.tcp_pending_writes.remove(&con_id);
                self.tcp_connections.remove(&con_id);
            }
            NetworkMessage::UdpSend(con_id, payload) | NetworkMessage::UdpRecv(con_id, payload) => {
                self.handle_udp_payload(con_id, payload).await?;
            }
            NetworkMessage::Notice(_notice) => {

            }
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
                    let pending_payloads = std::mem::take(&mut binding.pending_payloads);
                    for pending_payload in pending_payloads {
                        binding
                            .sender
                            .send_to(pending_payload.as_slice(), &peer_addr)
                            .await?;
                    }
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
                    debug!("sending UDP payload to {}:{}", peer_addr.ip(), peer_addr.port());
                    binding.sender.send_to(payload.as_slice(), &peer_addr).await?;
                }
                None => {
                    binding.pending_payloads.push(payload);
                }
            }
        } else {
            debug!("sending UDP payload to {}:{}", binding.target_addr.ip(), binding.target_addr.port());
            binding
                .sender
                .send_to(payload.as_slice(), &binding.target_addr)
                .await?;
        }

        Ok(())
    }

    async fn activate_tcp_connection(
        &mut self,
        con_id: ConId,
        stream: TcpStream,
        tcp_reads: &mut FuturesUnordered<TcpReadFuture>,
    ) -> Result<()> {
        let (reader, writer) = tokio::io::split(stream);
        self.tcp_connections.insert(con_id, writer);

        if let Some(pending_writes) = self.tcp_pending_writes.remove(&con_id) {
            let writer = self
                .tcp_connections
                .get_mut(&con_id)
                .expect("tcp connection should exist before flushing pending writes");
            for payload in pending_writes {
                writer.write_all(payload.as_slice()).await?;
            }
        }

        tcp_reads.push(read_tcp_once(con_id, reader));
        Ok(())
    }

    fn next_tcp_con_id(&mut self) -> ConId {
        let mut con_id = self.next_tcp_con_id;
        while self.tcp_pending.contains_key(&con_id) || self.tcp_connections.contains_key(&con_id)
        {
            con_id += 2;
        }

        self.next_tcp_con_id = con_id + 2;
        con_id
    }
}

fn resolve_socket_addr(addr: &str, port: u16, preference: SocketAddrPreference) -> Result<SocketAddr> {
    let mut addrs = (addr, port).to_socket_addrs()?;
    let first_addr = addrs
        .next()
        .ok_or_else(|| Error::msg(format!("failed to resolve socket address {}:{}", addr, port)))?;

    if preference == SocketAddrPreference::Any || socket_addr_matches_preference(first_addr, preference) {
        return Ok(first_addr);
    }

    addrs
        .find(|addr| socket_addr_matches_preference(*addr, preference))
        .ok_or_else(|| Error::msg(format!("failed to resolve socket address {}:{}", addr, port)))
}

fn resolve_socket_addrs_for_connect(addr: &str, port: u16) -> Result<Vec<SocketAddr>> {
    let mut addrs: Vec<_> = (addr, port).to_socket_addrs()?.collect();
    if addrs.is_empty() {
        return Err(Error::msg(format!(
            "failed to resolve socket address {}:{}",
            addr, port
        )));
    }

    addrs.sort_by_key(|addr| addr.is_ipv4());
    Ok(addrs)
}

fn socket_addr_matches_preference(addr: SocketAddr, preference: SocketAddrPreference) -> bool {
    match preference {
        SocketAddrPreference::Any => true,
        SocketAddrPreference::V4 => addr.is_ipv4(),
        SocketAddrPreference::V6 => addr.is_ipv6(),
    }
}

fn udp_addr_preference(binding: &NetworkPeerBinding) -> SocketAddrPreference {
    match binding.local_addr.as_str() {
        "localhost" => SocketAddrPreference::Any,
        addr if addr.contains(':') => SocketAddrPreference::V6,
        addr if addr.parse::<std::net::Ipv4Addr>().is_ok() || addr == "0.0.0.0" => {
            SocketAddrPreference::V4
        }
        _ => SocketAddrPreference::Any,
    }
}

fn udp_bind_addr(addr: &str, port: u16, target_addr: SocketAddr) -> String {
    if addr == "localhost" {
        match target_addr {
            SocketAddr::V4(_) => format!("127.0.0.1:{}", port),
            SocketAddr::V6(_) => format!("[::1]:{}", port),
        }
    } else {
        format!("{}:{}", addr, port)
    }
}

fn udp_unspecified_bind_addr(target_addr: SocketAddr) -> &'static str {
    match target_addr {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    }
}

async fn connect_socket_addr(addr: &str, port: u16) -> io::Result<TcpStream> {
    let addrs = resolve_socket_addrs_for_connect(addr, port)
        .map_err(|err| io::Error::new(io::ErrorKind::AddrNotAvailable, err.to_string()))?;

    let mut last_err = None;
    for addr in addrs {
        match TcpStream::connect(addr).await {
            Ok(stream) => return Ok(stream),
            Err(err) => last_err = Some(err),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            format!("failed to resolve socket address {}:{}", addr, port),
        )
    }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        runtime::Runtime,
        time::{delay_for, Duration},
    };

    fn reserve_tcp_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }

    fn reserve_udp_port() -> u16 {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        port
    }

    async fn bridge_network_messages(
        mut from_a: Receiver<NetworkMessage>,
        to_b: Sender<NetworkMessage>,
        mut from_b: Receiver<NetworkMessage>,
        to_a: Sender<NetworkMessage>,
    ) {
        let mut to_b = to_b;
        let mut to_a = to_a;

        loop {
            tokio::select! {
                message = from_a.recv() => match message {
                    Some(message) => {
                        let _ = to_b.send(message).await;
                    }
                    None => break,
                },
                message = from_b.recv() => match message {
                    Some(message) => {
                        let _ = to_a.send(message).await;
                    }
                    None => break,
                }
            }
        }
    }

    #[test]
    fn test_tcp_network_peer_round_trip() {
        Runtime::new().unwrap().block_on(async {
            let local_port = reserve_tcp_port();
            let remote_port = reserve_tcp_port();

            let binding = NetworkPeerBinding {
                direction: NetworkPeerBindingDirection::LocalToRemote,
                protocol: NetworkPeerProtocol::Tcp,
                local_addr: "127.0.0.1".to_owned(),
                local_port,
                remote_addr: "127.0.0.1".to_owned(),
                remote_port,
            };
            let config = NetworkPeerConfig::new(vec![binding]);

            let (local_peer, local_rx, local_tx) =
                NetworkPeer::new(config.clone(), NetworkPeerRole::Client).await;
            let (remote_peer, remote_rx, remote_tx) =
                NetworkPeer::new(config.invert(), NetworkPeerRole::Server).await;

            tokio::spawn(local_peer.run());
            tokio::spawn(remote_peer.run());
            tokio::spawn(bridge_network_messages(local_rx, remote_tx, remote_rx, local_tx));

            let service_handle = tokio::spawn(async move {
                let mut listener = TcpListener::bind(("127.0.0.1", remote_port)).await.unwrap();
                let (mut socket, _) = listener.accept().await.unwrap();

                let mut buff = [0u8; 128];
                let read = socket.read(&mut buff).await.unwrap();
                assert_eq!(&buff[..read], b"hello");

                socket.write_all(b"world").await.unwrap();
            });

            delay_for(Duration::from_millis(50)).await;

            let mut client = TcpStream::connect(("127.0.0.1", local_port)).await.unwrap();
            client.write_all(b"hello").await.unwrap();

            let mut buff = [0u8; 128];
            let read = client.read(&mut buff).await.unwrap();
            assert_eq!(&buff[..read], b"world");

            service_handle.await.unwrap();

        });
    }

    #[test]
    fn test_tcp_con_ids_are_partitioned_by_role() {
        Runtime::new().unwrap().block_on(async {
            let (mut client_peer, _, _) =
                NetworkPeer::new(NetworkPeerConfig::default(), NetworkPeerRole::Client).await;
            let (mut server_peer, _, _) =
                NetworkPeer::new(NetworkPeerConfig::default(), NetworkPeerRole::Server).await;

            assert_eq!(client_peer.next_tcp_con_id(), 1);
            assert_eq!(client_peer.next_tcp_con_id(), 3);
            assert_eq!(server_peer.next_tcp_con_id(), 2);
            assert_eq!(server_peer.next_tcp_con_id(), 4);
        });
    }

    #[test]
    fn test_udp_network_peer_round_trip() {
        Runtime::new().unwrap().block_on(async {
            let local_port = reserve_udp_port();
            let remote_port = reserve_udp_port();

            let binding = NetworkPeerBinding {
                direction: NetworkPeerBindingDirection::LocalToRemote,
                protocol: NetworkPeerProtocol::Udp,
                local_addr: "127.0.0.1".to_owned(),
                local_port,
                remote_addr: "127.0.0.1".to_owned(),
                remote_port,
            };
            let config = NetworkPeerConfig::new(vec![binding]);

            let (local_peer, local_rx, local_tx) =
                NetworkPeer::new(config.clone(), NetworkPeerRole::Client).await;
            let (remote_peer, remote_rx, remote_tx) =
                NetworkPeer::new(config.invert(), NetworkPeerRole::Server).await;

            tokio::spawn(local_peer.run());
            tokio::spawn(remote_peer.run());
            tokio::spawn(bridge_network_messages(local_rx, remote_tx, remote_rx, local_tx));

            let service_handle = tokio::spawn(async move {
                let mut socket = UdpSocket::bind(("127.0.0.1", remote_port)).await.unwrap();
                let mut buff = [0u8; 128];
                let (read, addr) = socket.recv_from(&mut buff).await.unwrap();
                assert_eq!(&buff[..read], b"ping");
                socket.send_to(b"pong", &addr).await.unwrap();
            });

            delay_for(Duration::from_millis(50)).await;

            let mut socket = UdpSocket::bind(("127.0.0.1", 0)).await.unwrap();
            socket.send_to(b"ping", &SocketAddr::from(([127, 0, 0, 1], local_port))).await.unwrap();

            let mut buff = [0u8; 128];
            let (read, _) = socket.recv_from(&mut buff).await.unwrap();
            assert_eq!(&buff[..read], b"pong");

            service_handle.await.unwrap();

        });
    }

    #[test]
    fn test_resolve_socket_addrs_for_connect_prefers_ipv6_then_ipv4() {
        let addrs = resolve_socket_addrs_for_connect("localhost", 5050).unwrap();

        let first_ipv4 = addrs.iter().position(|addr| addr.is_ipv4());
        let last_ipv6 = addrs.iter().rposition(|addr| addr.is_ipv6());

        if let (Some(first_ipv4), Some(last_ipv6)) = (first_ipv4, last_ipv6) {
            assert!(last_ipv6 < first_ipv4);
        }
    }

    #[test]
    fn test_udp_bind_addr_uses_ipv4_for_localhost_v4_target() {
        assert_eq!(
            udp_bind_addr("localhost", 5050, SocketAddr::from(([127, 0, 0, 1], 8080))),
            "127.0.0.1:5050"
        );
    }

    #[test]
    fn test_udp_bind_addr_uses_ipv6_for_localhost_v6_target() {
        assert_eq!(
            udp_bind_addr("localhost", 5050, SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 8080))),
            "[::1]:5050"
        );
        assert_eq!(udp_unspecified_bind_addr(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 8080))), "[::]:0");
    }

}
