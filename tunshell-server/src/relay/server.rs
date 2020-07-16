use super::{config::Config, connection::Connection as ClientConnection};
use crate::db::{Session, SessionStore};
use anyhow::{Context as AnyhowContext, Error, Result};
use chrono::Utc;
use futures::{Future, FutureExt};
use log::*;
use rand::Rng;
use std::net::{Ipv4Addr, SocketAddr};
use std::{
    collections::HashMap,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};
use tokio::sync::mpsc;
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
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
        for i in 0..self.0.len() {
            let poll = self.0[i].poll_unpin(cx);

            if let Poll::Ready(result) = poll {
                self.0.swap_remove(i);
                return Poll::Ready(result.unwrap_or_else(|err| Err(Error::from(err))));
            }
        }

        Poll::Pending
    }
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

            if let None = session {
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
            self.connections
                .paired
                .0
                .push(pair_connections(accepted.con, peer));
        } else {
            // Put connection into hash map, waiting for peer to join
            self.connections
                .waiting
                .0
                .insert(accepted.con.key.clone(), accepted.con);
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

    if let None = participant {
        return false;
    }

    let participant = participant.unwrap();

    // Ensure session is not being joined twice
    if participant.is_joined() {
        return false;
    }

    return true;
}

fn pair_connections(mut con1: Connection, mut con2: Connection) -> PairedConnection {
    debug!("pairing connections");

    let task = tokio::spawn(async move {
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
    });

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
        (ClientMessage::DirectConnectFailed, ClientMessage::DirectConnectFailed) => true,
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

// TODO: add additional tests
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
    use tokio::{runtime::Runtime, time::delay_for};
    use tokio_rustls::{client::TlsStream, TlsConnector};
    use tokio_util::compat::*;
    use tunshell_shared::{KeyAcceptedPayload, KeyPayload, KeyType, MessageStream};

    lazy_static! {
        static ref TCP_PORT_NUMBER: Mutex<u16> = Mutex::from(25555);
    }

    type ClientConnection =
        MessageStream<ClientMessage, ServerMessage, Compat<TlsStream<TcpStream>>>;

    fn init_port_number() -> u16 {
        let mut port = TCP_PORT_NUMBER.lock().unwrap();

        *port += 1;

        *port - 1
    }

    struct TerminableServer {
        running: JoinHandle<Server>,
        terminate: mpsc::Sender<()>,
    }

    impl TerminableServer {
        async fn stop(mut self) -> Result<Server> {
            self.terminate.send(()).await?;
            self.running.await.map_err(Error::from)
        }
    }

    async fn init_client_server_pair(
        mut server_config: Config,
    ) -> (ClientConnection, TerminableServer) {
        let port = init_port_number();

        server_config.port = port;

        let sessions = SessionStore::new(db::connect().await.unwrap());

        let mut server = Server::new(server_config.clone(), sessions);
        let (tx, rx) = mpsc::channel(1);

        let running = tokio::spawn(async move {
            server.start(Some(rx)).await.unwrap();
            server
        });

        // Give server time to bind
        tokio::time::delay_for(Duration::from_millis(100)).await;

        let client = TcpStream::connect(SocketAddr::from((Ipv4Addr::new(127, 0, 0, 1), port)))
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

        (
            ClientConnection::new(client.compat()),
            TerminableServer {
                running,
                terminate: tx,
            },
        )
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

    #[test]
    fn test_connect_to_server_test() {
        Runtime::new().unwrap().block_on(async {
            let (_client, server) = init_client_server_pair(Config::from_env().unwrap()).await;

            delay_for(Duration::from_millis(100)).await;

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 1);
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 0);
        });
    }

    #[test]
    fn test_connect_to_server_without_sending_key_timeout() {
        Runtime::new().unwrap().block_on(async {
            let mut config = Config::from_env().unwrap();
            config.client_key_timeout = Duration::from_millis(100);
            let (mut client, server) = init_client_server_pair(config).await;

            // Wait for timeout
            delay_for(Duration::from_millis(200)).await;

            let message = client.next().await.unwrap().unwrap();

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
            let (mut client, server) = init_client_server_pair(Config::from_env().unwrap()).await;

            let mock_session = Session::new(
                Participant::waiting(uuid::Uuid::new_v4().to_string()),
                Participant::waiting(uuid::Uuid::new_v4().to_string()),
            );

            let db = db::connect().await.unwrap();
            SessionStore::new(db).save(&mock_session).await.unwrap();

            client
                .write(&ClientMessage::Key(KeyPayload {
                    key: mock_session.host.key.clone(),
                }))
                .await
                .unwrap();

            let response = client.next().await.unwrap().unwrap();

            assert_eq!(
                response,
                ServerMessage::KeyAccepted(KeyAcceptedPayload {
                    key_type: KeyType::Host
                })
            );

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
    fn test_connect_with_valid_client_key() {
        Runtime::new().unwrap().block_on(async {
            let (mut client, server) = init_client_server_pair(Config::from_env().unwrap()).await;

            let mock_session = Session::new(
                Participant::waiting(uuid::Uuid::new_v4().to_string()),
                Participant::waiting(uuid::Uuid::new_v4().to_string()),
            );

            let db = db::connect().await.unwrap();
            SessionStore::new(db).save(&mock_session).await.unwrap();

            client
                .write(&ClientMessage::Key(KeyPayload {
                    key: mock_session.client.key.clone(),
                }))
                .await
                .unwrap();

            let response = client.next().await.unwrap().unwrap();

            assert_eq!(
                response,
                ServerMessage::KeyAccepted(KeyAcceptedPayload {
                    key_type: KeyType::Client
                })
            );

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
            let (mut client, server) = init_client_server_pair(Config::from_env().unwrap()).await;

            client
                .write(&ClientMessage::Key(KeyPayload {
                    key: "some-invalid-key".to_owned(),
                }))
                .await
                .unwrap();

            let response = client.next().await.unwrap().unwrap();

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
            config.expired_connection_clean_interval = Duration::from_millis(500);
            config.waiting_connection_expiry = Duration::from_millis(450);
            let (mut client, server) = init_client_server_pair(config).await;

            let mock_session = Session::new(
                Participant::waiting(uuid::Uuid::new_v4().to_string()),
                Participant::waiting(uuid::Uuid::new_v4().to_string()),
            );

            let db = db::connect().await.unwrap();
            SessionStore::new(db).save(&mock_session).await.unwrap();

            client
                .write(&ClientMessage::Key(KeyPayload {
                    key: mock_session.client.key.clone(),
                }))
                .await
                .unwrap();

            let response = client.next().await.unwrap().unwrap();

            assert_eq!(
                response,
                ServerMessage::KeyAccepted(KeyAcceptedPayload {
                    key_type: KeyType::Client
                })
            );

            // Wait for connection to be cleaned up by gc timeout
            delay_for(Duration::from_millis(600)).await;

            let server = server.stop().await.unwrap();

            assert_eq!(server.connections.new.0.len(), 0);
            assert_eq!(server.connections.waiting.0.len(), 0);
            assert_eq!(server.connections.paired.0.len(), 0);

            let response = client.next().await.unwrap().unwrap();

            assert_eq!(response, ServerMessage::Close);

            assert!(client.next().await.is_none());
        });
    }
}
