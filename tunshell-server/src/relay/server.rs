use super::{config::Config, connection::Connection as ClientConnection};
use crate::db::{Session, SessionStore};
use anyhow::{Context as AnyhowContext, Error, Result};
use chrono::Utc;
use futures::{Future, FutureExt, StreamExt};
use log::*;
use rand::Rng;
use std::net::{Ipv4Addr, SocketAddr};
use std::{
    collections::HashMap,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::timeout,
};
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use tunshell_shared::{
    AttemptDirectConnectPayload, ClientMessage, KeyAcceptedPayload, PeerJoinedPayload,
    ServerMessage,
};

pub(super) struct Server {
    config: Config,
    sessions: SessionStore,
    connections: Connections,
}

type ConnectionStream = ClientConnection<TlsStream<TcpStream>>;

struct Connection {
    stream: ConnectionStream,
    key: String,
    connected_at: Instant,
    remote_addr: SocketAddr,
}

struct AcceptedConnection {
    con: Connection,
    session: Session,
}

struct PairedConnection {
    task: JoinHandle<Result<(Connection, Connection)>>,
    paired_at: Instant,
}

struct Connections {
    new: NewConnections,
    waiting: WaitingConnections,
    paired: PairedConnections,
}

struct NewConnections(Vec<JoinHandle<Result<AcceptedConnection>>>);
struct WaitingConnections(HashMap<String, Connection>);
struct PairedConnections(Vec<PairedConnection>);

struct TlsListener {
    tcp: TcpListener,
    tls: TlsAcceptor,
}

impl Connections {
    fn new() -> Self {
        Self {
            new: NewConnections(vec![]),
            waiting: WaitingConnections(HashMap::new()),
            paired: PairedConnections(vec![]),
        }
    }
}

impl Future for NewConnections {
    type Output = Result<AcceptedConnection>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Poll::Ready((i, result)) = poll_all_remove_ready(self.0.iter_mut(), cx) {
            self.0.swap_remove(i);
            return Poll::Ready(result);
        }

        Poll::Pending
    }
}

impl Future for PairedConnections {
    type Output = Result<(Connection, Connection)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let poll = poll_all_remove_ready(self.0.iter_mut().map(|i| &mut i.task), cx);

        if let Poll::Ready((i, result)) = poll {
            self.0.swap_remove(i);
            return Poll::Ready(result);
        }

        Poll::Pending
    }
}

/// If a connection received a message or closed while it is waiting
/// we remove this connection from the pool
impl Future for WaitingConnections {
    type Output = (Connection, Result<ClientMessage>);

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut received = None;

        for (key, con) in &mut self.0 {
            let result = con.stream.stream_mut().poll_next_unpin(cx);

            if let Poll::Ready(message) = result {
                received = Some((key.to_owned(), message));
                break;
            }
        }

        if let None = received {
            return Poll::Pending;
        }

        let (key, message) = received.unwrap();
        let con = self.0.remove(&key).unwrap();

        Poll::Ready((
            con,
            message.unwrap_or_else(|| Err(Error::msg("connection closed by client"))),
        ))
    }
}

fn poll_all_remove_ready<'a, T>(
    futures: impl Iterator<Item = &'a mut JoinHandle<Result<T>>>,
    cx: &mut Context<'_>,
) -> Poll<(usize, Result<T>)>
where
    T: 'a,
{
    let mut i = 0;
    for fut in futures {
        let poll = fut.poll_unpin(cx);

        if let Poll::Ready(result) = poll {
            return Poll::Ready((i, result.unwrap_or_else(|err| Err(Error::from(err)))));
        }

        i += 1;
    }

    Poll::Pending
}

impl TlsListener {
    async fn bind(config: &Config) -> Result<Self> {
        Ok(Self {
            tcp: TcpListener::bind((Ipv4Addr::new(0, 0, 0, 0), config.port)).await?,
            tls: TlsAcceptor::from(Arc::clone(&config.tls_config)),
        })
    }

    async fn accept(&mut self) -> Result<TlsStream<TcpStream>> {
        let (socket, addr) = self.tcp.accept().await?;
        debug!("received connection from {}", addr);

        Ok(self.tls.accept(socket).await?)
    }
}

impl Server {
    pub(super) fn new(config: Config, sessions: SessionStore) -> Self {
        Self {
            config,
            sessions,
            connections: Connections::new(),
        }
    }

    pub(super) async fn start(&mut self, terminate_rx: Option<mpsc::Receiver<()>>) -> Result<()> {
        let mut tls_listener = TlsListener::bind(&self.config).await?;

        // If the terminate channel is not supplied we create a default channel
        // that is never invoked
        let (_tx, mut terminate_rx) = match terminate_rx {
            Some(rx) => (None, rx),
            None => {
                let (tx, rx) = mpsc::channel(1);
                (Some(tx), rx)
            }
        };

        let mut next_clean_at =
            tokio::time::Instant::now() + self.config.expired_connection_clean_interval;

        loop {
            tokio::select! {
                stream = tls_listener.accept() => { self.handle_new_connection(stream); },
                accepted = &mut self.connections.new => { self.handle_accepted_connection(accepted); },
                closed = &mut self.connections.waiting => { self.handle_closed_waiting_connection(closed); },
                finished = &mut self.connections.paired => { self.handle_finished_connection(finished); }
                _ = tokio::time::delay_until(next_clean_at) => {
                    self.clean_expired_connections();
                    next_clean_at += self.config.expired_connection_clean_interval;
                }
                _ = terminate_rx.recv() => break
            }
        }

        Ok(())
    }

    fn handle_new_connection(&mut self, stream: Result<TlsStream<TcpStream>>) {
        if let Err(err) = stream {
            warn!("error while establishing connection: {:?}", err);
            return;
        }

        self.connections
            .new
            .0
            .push(self.negotiate_key(stream.unwrap()));
    }

    fn negotiate_key(
        &self,
        stream: TlsStream<TcpStream>,
    ) -> JoinHandle<Result<AcceptedConnection>> {
        debug!("negotiating key");
        let mut sessions = self.sessions.clone();
        let key_timeout = self.config.client_key_timeout;

        tokio::spawn(async move {
            let remote_addr = stream.get_ref().0.peer_addr()?;
            let mut connection = ConnectionStream::new(stream);

            let key = connection.wait_for_key(key_timeout).await;

            if let Err(err) = key {
                warn!("error while receiving key: {}", err);
                return Err(err);
            }

            let key = key.unwrap().key;
            let session = sessions.find_by_key(key.as_ref()).await?;

            if session.is_none() {
                debug!("key rejected, could not find session");
                connection.write(ServerMessage::KeyRejected).await?;
                return Err(Error::msg("client did not supply valid key"));
            }

            let session = session.unwrap();

            if !is_session_valid_to_join(&session, key.as_ref()) {
                debug!("key rejected, session is not valid to join");
                connection.write(ServerMessage::KeyRejected).await?;
                return Err(Error::msg("session is not valid to join"));
            }

            // TODO: verify if joined status should be persisted to mongo
            // let participant = session.participant_mut(key.as_ref()).unwrap();
            // participant.set_joined(remote_addr.ip());
            // sessions.save(&session).await?;

            connection
                .write(ServerMessage::KeyAccepted(KeyAcceptedPayload {
                    key_type: session.key_type(key.as_ref()).unwrap(),
                }))
                .await?;

            debug!("key accepted");
            let connection = Connection {
                stream: connection,
                key,
                connected_at: Instant::now(),
                remote_addr,
            };

            Ok(AcceptedConnection {
                con: connection,
                session,
            })
        })
    }

    fn handle_accepted_connection(&mut self, accepted: Result<AcceptedConnection>) {
        let accepted = match accepted {
            Ok(accepted) => accepted,
            Err(err) => {
                error!("error while accepting connection: {}", err);
                return;
            }
        };

        // Ensure no race condition where connection can be joined twice
        if self.connections.waiting.0.contains_key(&accepted.con.key) {
            warn!("connection was joined twice");
            return;
        }

        let peer = accepted
            .session
            .other_participant(&accepted.con.key)
            .unwrap();

        if self.connections.waiting.0.contains_key(&peer.key) {
            // Peer is waiting, we can pair the connections
            let peer = self.connections.waiting.0.remove(&peer.key).unwrap();
            self.connections.paired.0.push(pair_connections(
                accepted.con,
                peer,
                self.config.paired_connection_expiry,
            ));
        } else {
            // Put connection into hash map, waiting for peer to join
            self.connections
                .waiting
                .0
                .insert(accepted.con.key.clone(), accepted.con);
        }
    }

    fn handle_closed_waiting_connection(&self, closed: (Connection, Result<ClientMessage>)) {
        match closed {
            (_, Ok(msg)) => error!(
                "received unexpected message from client while waiting for peer: {:?}",
                msg
            ),
            (_, Err(err)) => error!(
                "connection error from client while waiting for peer: {:?}",
                err
            ),
        }
    }

    fn handle_finished_connection(&self, paired: Result<(Connection, Connection)>) {
        if let Err(err) = paired {
            error!("error occurred during paired connection: {}", err);
        }
    }

    fn clean_expired_connections(&mut self) {
        debug!("cleaning expired connections");

        let expiry = self.config.waiting_connection_expiry;
        self.connections
            .waiting
            .0
            .retain(|_, con| Instant::now() - con.connected_at < expiry);

        let expiry = self.config.paired_connection_expiry;
        self.connections
            .paired
            .0
            .retain(|con| Instant::now() - con.paired_at < expiry);
    }
}

fn is_session_valid_to_join(session: &Session, key: &str) -> bool {
    // Ensure session is not older than a day
    if Utc::now() - session.created_at > chrono::Duration::days(1) {
        return false;
    }

    let participant = session.participant(key);

    if participant.is_none() {
        return false;
    }

    let participant = participant.unwrap();

    // Ensure session is not being joined twice
    if participant.is_joined() {
        return false;
    }

    return true;
}

fn pair_connections(
    mut con1: Connection,
    mut con2: Connection,
    timeout_dur: Duration,
) -> PairedConnection {
    debug!("pairing connections");

    let task = async move {
        tokio::try_join!(
            con1.stream
                .write(ServerMessage::PeerJoined(PeerJoinedPayload {
                    peer_ip_address: con2.remote_addr.ip().to_string(),
                    peer_key: con2.key.clone(),
                })),
            con2.stream
                .write(ServerMessage::PeerJoined(PeerJoinedPayload {
                    peer_ip_address: con1.remote_addr.ip().to_string(),
                    peer_key: con1.key.clone(),
                })),
        )
        .context("sending peer joined message")?;

        let direct_connection = attempt_direct_connection(&mut con1, &mut con2).await;

        if let Err(err) = direct_connection {
            return Err(err.context("establishing direct connection"));
        }

        if direct_connection.unwrap() {
            // In the case of a direct connection between the peers the
            // relay server does not have to do much, since the clients
            // will stream between themselves.
            // The next message must indicate the connection is over so
            // wait until a message is received.
            debug!("direct connection established");

            let message: Result<ClientMessage> = tokio::select! {
                message = con1.stream.next() => message,
                message = con2.stream.next() => message
            };

            match message {
                Ok(ClientMessage::Close) => {}
                Ok(message) => {
                    return Err(Error::msg(format!(
                        "received unexpected message from client during direct connection {:?}",
                        message
                    )))
                }
                Err(err) => {
                    return Err(Error::msg(format!(
                        "error received from client stream {}",
                        err
                    )))
                }
            }
        } else {
            // If the direct connection fails the relay server becomes responsible
            // for proxying data between the two peers
            debug!("starting relay");
            tokio::try_join!(
                con1.stream.write(ServerMessage::StartRelayMode),
                con2.stream.write(ServerMessage::StartRelayMode)
            )
            .context("sending relay mode message")?;

            relay_loop(&mut con1, &mut con2).await?;
        }

        Ok((con1, con2))
    };

    let task = timeout(timeout_dur, task)
        .map(|i| i.unwrap_or_else(|_| Err(Error::msg("direct connection timed out"))));

    let task = tokio::spawn(task);

    PairedConnection {
        task,
        paired_at: Instant::now(),
    }
}

async fn attempt_direct_connection(con1: &mut Connection, con2: &mut Connection) -> Result<bool> {
    // TODO: Improve port selection
    let (port1, port2) = {
        let mut rng = rand::thread_rng();
        (rng.gen_range(20000, 40000), rng.gen_range(20000, 40000))
    };

    tokio::try_join!(
        con1.stream.write(ServerMessage::AttemptDirectConnect(
            AttemptDirectConnectPayload {
                connect_at: 0,
                self_listen_port: port1,
                peer_listen_port: port2
            }
        )),
        con2.stream.write(ServerMessage::AttemptDirectConnect(
            AttemptDirectConnectPayload {
                connect_at: 0,
                self_listen_port: port2,
                peer_listen_port: port1
            }
        ))
    )
    .context("sending direct connection command")?;

    let (result1, result2) = tokio::try_join!(con1.stream.next(), con2.stream.next())
        .context("waiting for direct connection response")?;

    let result = match (result1, result2) {
        (ClientMessage::DirectConnectSucceeded, ClientMessage::DirectConnectSucceeded) => true,
        (ClientMessage::DirectConnectFailed, ClientMessage::DirectConnectFailed) => false,
        msgs @ _ => {
            return Err(Error::msg(format!(
                "unexpected message while attempting direct connection: {:?}",
                msgs
            )))
        }
    };

    Ok(result)
}

async fn relay_loop(con1: &mut Connection, con2: &mut Connection) -> Result<()> {
    enum ProxyResult {
        Continue,
        Closed,
    }

    async fn proxy_payload(
        message: Result<ClientMessage>,
        dest: &mut Connection,
    ) -> Result<ProxyResult> {
        let payload = match message {
            Ok(ClientMessage::Relay(payload)) => payload,
            Ok(ClientMessage::Close) => return Ok(ProxyResult::Closed),
            Ok(msg) => {
                return Err(Error::msg(format!(
                    "received unexpected message from client during relay: {:?}",
                    msg
                )))
            }
            Err(err) => return Err(err),
        };

        dest.stream.write(ServerMessage::Relay(payload)).await?;
        Ok(ProxyResult::Continue)
    }

    loop {
        let result = tokio::select! {
            message = con1.stream.next() => proxy_payload(message, con2).await?,
            message = con2.stream.next() => proxy_payload(message, con1).await?,
        };

        if let ProxyResult::Closed = result {
            break;
        }
    }

    Ok(())
}

// TODO: refactor into multiple files

// #[cfg(all(test, integration))]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use db::Participant;
    use futures::StreamExt;
    use lazy_static::lazy_static;
    use rustls::ClientConfig;
    use std::{net::SocketAddr, sync::Mutex, time::Duration};
    use tokio::{
        runtime::Runtime,
        time::{delay_for, timeout},
    };
    use tokio_rustls::{client::TlsStream, TlsConnector};
    use tokio_util::compat::*;
    use tunshell_shared::{KeyAcceptedPayload, KeyPayload, KeyType, MessageStream, RelayPayload};

    lazy_static! {
        static ref TCP_PORT_NUMBER: Mutex<u16> = Mutex::from(35555);
    }

    type ClientConnection =
        MessageStream<ClientMessage, ServerMessage, Compat<TlsStream<TcpStream>>>;

    fn init_port_number() -> u16 {
        let mut port = TCP_PORT_NUMBER.lock().unwrap();

        *port += 1;

        *port - 1
    }

    struct TerminableServer {
        port: u16,
        running: JoinHandle<Server>,
        terminate: mpsc::Sender<()>,
    }

    impl TerminableServer {
        async fn stop(mut self) -> Result<Server> {
            self.terminate.send(()).await?;
            self.running.await.map_err(Error::from)
        }
    }

    async fn init_server(mut server_config: Config) -> TerminableServer {
        server_config.port = init_port_number();

        let sessions = SessionStore::new(db::connect().await.unwrap());

        let mut server = Server::new(server_config.clone(), sessions);
        let (tx, rx) = mpsc::channel(1);

        let running = tokio::spawn(async move {
            server.start(Some(rx)).await.unwrap();
            server
        });

        // Give server time to bind
        loop {
            let socket = TcpStream::connect(SocketAddr::from((
                Ipv4Addr::new(127, 0, 0, 1),
                server_config.port,
            )))
            .await;

            if let Ok(_) = socket {
                break;
            }

            tokio::time::delay_for(Duration::from_millis(100)).await;
        }

        TerminableServer {
            port: server_config.port,
            running,
            terminate: tx,
        }
    }

    async fn create_client_connection_to_server(server: &TerminableServer) -> ClientConnection {
        let client =
            TcpStream::connect(SocketAddr::from((Ipv4Addr::new(127, 0, 0, 1), server.port)))
                .await
                .unwrap();

        let mut client_config = ClientConfig::default();

        client_config
            .set_single_client_cert(
                Config::parse_tls_cert().unwrap(),
                Config::parse_tls_private_key().unwrap(),
            )
            .unwrap();

        client_config
            .dangerous()
            .set_certificate_verifier(Arc::new(NullCertVerifier {}));

        let client = TlsConnector::from(Arc::new(client_config))
            .connect(
                webpki::DNSNameRef::try_from_ascii("localhost".as_bytes()).unwrap(),
                client,
            )
            .await
            .unwrap();

        ClientConnection::new(client.compat())
    }

    struct NullCertVerifier {}

    impl rustls::ServerCertVerifier for NullCertVerifier {
        fn verify_server_cert(
            &self,
            _roots: &rustls::RootCertStore,
            _presented_certs: &[rustls::Certificate],
            _dns_name: webpki::DNSNameRef,
            _ocsp_response: &[u8],
        ) -> Result<rustls::ServerCertVerified, rustls::TLSError> {
            Ok(rustls::ServerCertVerified::assertion())
        }
    }

    async fn create_mock_session() -> Session {
        let mock_session = Session::new(
            Participant::waiting(uuid::Uuid::new_v4().to_string()),
            Participant::waiting(uuid::Uuid::new_v4().to_string()),
        );

        let db = db::connect().await.unwrap();
        SessionStore::new(db).save(&mock_session).await.unwrap();

        mock_session
    }

    async fn send_key_to_server(con: &mut ClientConnection, key: &str) {
        con.write(&ClientMessage::Key(KeyPayload {
            key: key.to_owned(),
        }))
        .await
        .unwrap();
    }

    async fn assert_next_message_is_key_accepted(con: &mut ClientConnection, key_type: KeyType) {
        assert_eq!(
            con.next().await.unwrap().unwrap(),
            ServerMessage::KeyAccepted(KeyAcceptedPayload { key_type })
        );
    }

    #[test]
    fn test_init_server() {
        Runtime::new().unwrap().block_on(async {
            let server = init_server(Config::from_env().unwrap()).await;

            delay_for(Duration::from_millis(100)).await;

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 0);
        });
    }

    #[test]
    fn test_connect_to_server_without_sending_key_timeout() {
        Runtime::new().unwrap().block_on(async {
            let mut config = Config::from_env().unwrap();
            config.client_key_timeout = Duration::from_millis(100);

            let server = init_server(config).await;
            let mut con = create_client_connection_to_server(&server).await;

            // Wait for timeout
            delay_for(Duration::from_millis(200)).await;

            let message = con.next().await.unwrap().unwrap();

            assert_eq!(message, ServerMessage::Close);

            delay_for(Duration::from_millis(10)).await;

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 0);
        });
    }

    #[test]
    fn test_connect_with_valid_host_key() {
        Runtime::new().unwrap().block_on(async {
            let server = init_server(Config::from_env().unwrap()).await;
            let mut con = create_client_connection_to_server(&server).await;

            let mock_session = create_mock_session().await;

            send_key_to_server(&mut con, &mock_session.host.key).await;

            assert_next_message_is_key_accepted(&mut con, KeyType::Host).await;

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 1);
            assert_eq!(
                server
                    .connections
                    .waiting
                    .0
                    .contains_key(&mock_session.host.key),
                true
            );
            assert_eq!(server.connections.paired.0.len(), 0);
        });
    }

    #[test]
    fn test_close_connection_while_waiting_for_peer() {
        Runtime::new().unwrap().block_on(async {
            let server = init_server(Config::from_env().unwrap()).await;
            {
                let mut con = create_client_connection_to_server(&server).await;

                let mock_session = create_mock_session().await;

                send_key_to_server(&mut con, &mock_session.host.key).await;

                assert_next_message_is_key_accepted(&mut con, KeyType::Host).await;

                // Connection should drop here
            }

            delay_for(Duration::from_millis(100)).await;

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            // Connection should be removed from waiting hash map after being closed
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 0);
        });
    }

    #[test]
    fn test_connect_with_valid_client_key() {
        Runtime::new().unwrap().block_on(async {
            let server = init_server(Config::from_env().unwrap()).await;
            let mut con = create_client_connection_to_server(&server).await;

            let mock_session = create_mock_session().await;

            send_key_to_server(&mut con, &mock_session.client.key).await;

            assert_next_message_is_key_accepted(&mut con, KeyType::Client).await;

            delay_for(Duration::from_millis(50)).await;

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 1);
            assert_eq!(
                server
                    .connections
                    .waiting
                    .0
                    .contains_key(&mock_session.client.key),
                true
            );
            assert_eq!(server.connections.paired.0.len(), 0);
        });
    }

    #[test]
    fn test_connect_with_invalid_key() {
        Runtime::new().unwrap().block_on(async {
            let server = init_server(Config::from_env().unwrap()).await;
            let mut con = create_client_connection_to_server(&server).await;

            send_key_to_server(&mut con, "some-invalid-key").await;

            let response = con.next().await.unwrap().unwrap();

            assert_eq!(response, ServerMessage::KeyRejected);

            delay_for(Duration::from_millis(10)).await;

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 0);
        });
    }

    #[test]
    fn test_connect_with_to_expired_session() {
        Runtime::new().unwrap().block_on(async {
            let server = init_server(Config::from_env().unwrap()).await;
            let mut con = create_client_connection_to_server(&server).await;

            let mut mock_session = create_mock_session().await;
            mock_session.created_at = chrono::Utc::now() - chrono::Duration::days(1);
            SessionStore::new(db::connect().await.unwrap())
                .save(&mock_session)
                .await
                .unwrap();

            send_key_to_server(&mut con, &mock_session.host.key).await;

            let response = con.next().await.unwrap().unwrap();

            assert_eq!(response, ServerMessage::KeyRejected);

            delay_for(Duration::from_millis(10)).await;

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 0);
        });
    }

    #[test]
    fn test_clean_expired_waiting_connections() {
        Runtime::new().unwrap().block_on(async {
            let mut config = Config::from_env().unwrap();
            config.expired_connection_clean_interval = Duration::from_millis(200);
            config.waiting_connection_expiry = Duration::from_millis(400);
            let server = init_server(config).await;
            let mut con = create_client_connection_to_server(&server).await;

            let mock_session = create_mock_session().await;

            send_key_to_server(&mut con, &mock_session.client.key).await;

            assert_next_message_is_key_accepted(&mut con, KeyType::Client).await;

            // Wait for connection to be cleaned up by gc timeout
            delay_for(Duration::from_millis(1000)).await;

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 0);

            let response = con.next().await.unwrap().unwrap();

            assert_eq!(response, ServerMessage::Close);

            assert!(con.next().await.is_none());
        });
    }

    #[test]
    fn test_paired_connection() {
        Runtime::new().unwrap().block_on(async {
            let config = Config::from_env().unwrap();
            let server = init_server(config).await;

            let mut con_host = create_client_connection_to_server(&server).await;
            let mut con_client = create_client_connection_to_server(&server).await;

            let mock_session = create_mock_session().await;

            send_key_to_server(&mut con_host, &mock_session.host.key).await;
            assert_next_message_is_key_accepted(&mut con_host, KeyType::Host).await;

            send_key_to_server(&mut con_client, &mock_session.client.key).await;
            assert_next_message_is_key_accepted(&mut con_client, KeyType::Client).await;

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 1);
        });
    }

    #[test]
    fn test_direct_connection() {
        Runtime::new().unwrap().block_on(async {
            let config = Config::from_env().unwrap();
            let server = init_server(config).await;

            let mut con_host = create_client_connection_to_server(&server).await;
            let mut con_client = create_client_connection_to_server(&server).await;

            let mock_session = create_mock_session().await;

            send_key_to_server(&mut con_host, &mock_session.host.key).await;
            assert_next_message_is_key_accepted(&mut con_host, KeyType::Host).await;

            send_key_to_server(&mut con_client, &mock_session.client.key).await;
            assert_next_message_is_key_accepted(&mut con_client, KeyType::Client).await;

            assert_eq!(
                con_host.next().await.unwrap().unwrap(),
                ServerMessage::PeerJoined(PeerJoinedPayload {
                    peer_key: mock_session.client.key.to_owned(),
                    peer_ip_address: "127.0.0.1".to_owned()
                })
            );

            assert_eq!(
                con_client.next().await.unwrap().unwrap(),
                ServerMessage::PeerJoined(PeerJoinedPayload {
                    peer_key: mock_session.host.key.to_owned(),
                    peer_ip_address: "127.0.0.1".to_owned()
                })
            );

            let message_host = con_host.next().await.unwrap().unwrap();
            let message_client = con_client.next().await.unwrap().unwrap();

            match (message_host, message_client) {
                (
                    ServerMessage::AttemptDirectConnect(_),
                    ServerMessage::AttemptDirectConnect(_),
                ) => {}
                msgs @ _ => panic!(
                    "expected to receive attempt direct connect messages but received: {:?}",
                    msgs
                ),
            }

            con_host
                .write(&ClientMessage::DirectConnectSucceeded)
                .await
                .unwrap();
            con_client
                .write(&ClientMessage::DirectConnectSucceeded)
                .await
                .unwrap();

            timeout(
                Duration::from_millis(100),
                futures::future::select(con_host.next(), con_client.next()),
            )
            .await
            .err()
            .expect("after direct connection succeeds no message should be sent by the server");

            con_host.write(&ClientMessage::Close).await.unwrap();
            con_client.write(&ClientMessage::Close).await.unwrap();

            con_host
                .next()
                .await
                .map(|i| i.expect_err("socket should be closed after close message"));
            con_client
                .next()
                .await
                .map(|i| i.expect_err("socket should be closed after close message"));

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 0);
        });
    }

    #[test]
    fn test_relayed_connection() {
        Runtime::new().unwrap().block_on(async {
            let config = Config::from_env().unwrap();
            let server = init_server(config).await;

            let mut con_host = create_client_connection_to_server(&server).await;
            let mut con_client = create_client_connection_to_server(&server).await;

            let mock_session = create_mock_session().await;

            send_key_to_server(&mut con_host, &mock_session.host.key).await;
            assert_next_message_is_key_accepted(&mut con_host, KeyType::Host).await;

            send_key_to_server(&mut con_client, &mock_session.client.key).await;
            assert_next_message_is_key_accepted(&mut con_client, KeyType::Client).await;

            assert_eq!(
                con_host.next().await.unwrap().unwrap(),
                ServerMessage::PeerJoined(PeerJoinedPayload {
                    peer_key: mock_session.client.key.to_owned(),
                    peer_ip_address: "127.0.0.1".to_owned()
                })
            );

            assert_eq!(
                con_client.next().await.unwrap().unwrap(),
                ServerMessage::PeerJoined(PeerJoinedPayload {
                    peer_key: mock_session.host.key.to_owned(),
                    peer_ip_address: "127.0.0.1".to_owned()
                })
            );

            let message_host = con_host.next().await.unwrap().unwrap();
            let message_client = con_client.next().await.unwrap().unwrap();

            match (message_host, message_client) {
                (
                    ServerMessage::AttemptDirectConnect(_),
                    ServerMessage::AttemptDirectConnect(_),
                ) => {}
                msgs @ _ => panic!(
                    "expected to receive attempt direct connect messages but received: {:?}",
                    msgs
                ),
            }

            con_host
                .write(&ClientMessage::DirectConnectFailed)
                .await
                .unwrap();
            con_client
                .write(&ClientMessage::DirectConnectFailed)
                .await
                .unwrap();

            assert_eq!(
                con_host.next().await.unwrap().unwrap(),
                ServerMessage::StartRelayMode
            );
            assert_eq!(
                con_client.next().await.unwrap().unwrap(),
                ServerMessage::StartRelayMode
            );

            con_host
                .write(&ClientMessage::Relay(RelayPayload {
                    data: "hello from host".as_bytes().to_vec(),
                }))
                .await
                .unwrap();
            con_client
                .write(&ClientMessage::Relay(RelayPayload {
                    data: "hello from client".as_bytes().to_vec(),
                }))
                .await
                .unwrap();

            assert_eq!(
                con_host.next().await.unwrap().unwrap(),
                ServerMessage::Relay(RelayPayload {
                    data: "hello from client".as_bytes().to_vec(),
                })
            );
            assert_eq!(
                con_client.next().await.unwrap().unwrap(),
                ServerMessage::Relay(RelayPayload {
                    data: "hello from host".as_bytes().to_vec(),
                })
            );

            con_host.write(&ClientMessage::Close).await.unwrap();
            con_client.write(&ClientMessage::Close).await.unwrap();

            con_host
                .next()
                .await
                .map(|i| i.expect_err("socket should be closed after close message"));
            con_client
                .next()
                .await
                .map(|i| i.expect_err("socket should be closed after close message"));

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 0);
        });
    }

    #[test]
    fn test_clean_up_paired_connection() {
        Runtime::new().unwrap().block_on(async {
            let mut config = Config::from_env().unwrap();
            config.expired_connection_clean_interval = Duration::from_millis(100);
            config.paired_connection_expiry = Duration::from_millis(100);
            let server = init_server(config).await;

            let mut con_host = create_client_connection_to_server(&server).await;
            let mut con_client = create_client_connection_to_server(&server).await;

            let mock_session = create_mock_session().await;

            send_key_to_server(&mut con_host, &mock_session.host.key).await;
            assert_next_message_is_key_accepted(&mut con_host, KeyType::Host).await;

            send_key_to_server(&mut con_client, &mock_session.client.key).await;
            assert_next_message_is_key_accepted(&mut con_client, KeyType::Client).await;

            assert_eq!(
                con_host.next().await.unwrap().unwrap(),
                ServerMessage::PeerJoined(PeerJoinedPayload {
                    peer_key: mock_session.client.key.to_owned(),
                    peer_ip_address: "127.0.0.1".to_owned()
                })
            );

            assert_eq!(
                con_client.next().await.unwrap().unwrap(),
                ServerMessage::PeerJoined(PeerJoinedPayload {
                    peer_key: mock_session.host.key.to_owned(),
                    peer_ip_address: "127.0.0.1".to_owned()
                })
            );

            let message_host = con_host.next().await.unwrap().unwrap();
            let message_client = con_client.next().await.unwrap().unwrap();

            match (message_host, message_client) {
                (
                    ServerMessage::AttemptDirectConnect(_),
                    ServerMessage::AttemptDirectConnect(_),
                ) => {}
                msgs @ _ => panic!(
                    "expected to receive attempt direct connect messages but received: {:?}",
                    msgs
                ),
            }

            con_host
                .write(&ClientMessage::DirectConnectSucceeded)
                .await
                .unwrap();
            con_client
                .write(&ClientMessage::DirectConnectSucceeded)
                .await
                .unwrap();

            // Wait for connection to expire
            delay_for(Duration::from_millis(500)).await;

            assert_eq!(
                con_host.next().await.unwrap().unwrap(),
                ServerMessage::Close
            );
            assert_eq!(
                con_client.next().await.unwrap().unwrap(),
                ServerMessage::Close
            );

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 0);
        });
    }
}
