use super::{config::Config, connection::Connection as ClientConnection};
use crate::db::{Session, SessionStore};
use anyhow::{Error, Result};
use chrono::Utc;
use futures::{Future, FutureExt};
use log::*;
use std::net::Ipv4Addr;
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
};
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use tunshell_shared::{KeyAcceptedPayload, ServerMessage};

const CLIENT_KEY_TIMEOUT_MS: u64 = 3000;
const CLEAN_EXPIRED_CONNECTION_INTERVAL_MS: u64 = 60000;

pub(super) struct Server {
    config: Config,
    sessions: SessionStore,
    connections: Connections,
}

type ConnectionStream = ClientConnection<TlsStream<TcpStream>>;

struct Connection {
    stream: ConnectionStream,
    connected_at: Instant,
}

struct AcceptedConnection {
    con: Connection,
    session: Session,
    key: String,
}

struct Connections {
    new: NewConnections,
    waiting: WaitingConnections,
    paired: PairedConnections,
}

struct NewConnections(Vec<JoinHandle<Result<AcceptedConnection>>>);
struct WaitingConnections(HashMap<String, Connection>);
struct PairedConnections(Vec<JoinHandle<(Connection, Connection)>>);

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

        let (_, mut terminate_rx) = match terminate_rx {
            Some(rx) => (None, rx),
            None => {
                let (tx, rx) = mpsc::channel(0);
                (Some(tx), rx)
            }
        };

        let clean_interval = Duration::from_millis(CLEAN_EXPIRED_CONNECTION_INTERVAL_MS);
        let mut next_clean_at = tokio::time::Instant::now() + clean_interval;

        loop {
            tokio::select! {
                stream = tls_listener.accept() => { self.handle_new_connection(stream); },
                accepted = &mut self.connections.new => { self.handle_accepted_connection(accepted); },
                _ = tokio::time::delay_until(next_clean_at) => { self.clean_expired_connections(); next_clean_at += clean_interval; }
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

        tokio::spawn(async move {
            let peer_addr = stream.get_ref().0.peer_addr()?;
            let mut connection = ConnectionStream::new(stream);

            let key = connection
                .wait_for_key(Duration::from_millis(CLIENT_KEY_TIMEOUT_MS))
                .await;

            if let Err(err) = key {
                warn!("error while receiving key: {}", err);
                connection.try_send_close();
                return Err(err);
            }

            let key = key.unwrap().key;
            let session = sessions.find_by_key(key.as_ref()).await?;

            if let None = session {
                debug!("key rejected, could not find session");
                connection.write(ServerMessage::KeyRejected).await?;
                return Err(Error::msg("client did not supply valid key"));
            }

            let mut session = session.unwrap();

            if !is_session_valid_to_join(&session, key.as_ref()) {
                debug!("key rejected, session is not valid to join");
                connection.write(ServerMessage::KeyRejected).await?;
                return Err(Error::msg("session is not valid to join"));
            }

            let participant = session.participant_mut(key.as_ref()).unwrap();
            participant.set_joined(peer_addr.ip());
            sessions.save(&session).await?;

            connection
                .write(ServerMessage::KeyAccepted(KeyAcceptedPayload {
                    key_type: session.key_type(key.as_ref()).unwrap(),
                }))
                .await?;

            debug!("key accepted");
            let connection = Connection {
                stream: connection,
                connected_at: Instant::now(),
            };

            Ok(AcceptedConnection {
                con: connection,
                session,
                key,
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
        if self.connections.waiting.0.contains_key(&accepted.key) {
            accepted.con.stream.try_send_close();
            warn!("connection was joined twice");
            return;
        }

        let peer = accepted.session.other_participant(&accepted.key).unwrap();

        if self.connections.waiting.0.contains_key(&peer.key) {
            // Peer is waiting, we can pair the connections
            let peer = self.connections.waiting.0.remove(&peer.key).unwrap();
            self.pair_connections(accepted.con, peer);
        } else {
            // Put connection into hash map, waiting for peer to join
            self.connections
                .waiting
                .0
                .insert(accepted.key, accepted.con);
        }
    }

    fn pair_connections(&mut self, con1: Connection, con2: Connection) {
        debug!("pairing connections");
        //  let c = con1.stream.inner();
        //  c.get_ref().0.peer_addr()
        todo!()
    }

    fn clean_expired_connections(&mut self) {
        debug!("cleaning expired connections");
        todo!()
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

// #[cfg(all(test, integration))]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use db::Participant;
    use futures::StreamExt;
    use lazy_static::lazy_static;
    use rustls::ClientConfig;
    use std::{net::SocketAddr, sync::Mutex};
    use tokio::{runtime::Runtime, time::delay_for};
    use tokio_rustls::{client::TlsStream, TlsConnector};
    use tokio_util::compat::*;
    use tunshell_shared::{ClientMessage, KeyAcceptedPayload, KeyPayload, KeyType, MessageStream};

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

    async fn init_client_server_pair() -> (ClientConnection, TerminableServer) {
        let port = init_port_number();

        let mut server_config = Config::from_env().unwrap();
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
            let (_client, server) = init_client_server_pair().await;

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
            let (mut client, server) = init_client_server_pair().await;

            // Wait for timeout
            delay_for(Duration::from_millis(CLIENT_KEY_TIMEOUT_MS + 100)).await;

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
            let (mut client, server) = init_client_server_pair().await;

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
            let (mut client, server) = init_client_server_pair().await;

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
            let (mut client, server) = init_client_server_pair().await;

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
}
