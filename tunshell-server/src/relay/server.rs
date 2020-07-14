use super::{config::Config, connection::Connection as ClientConnection};
use crate::db::{Session, SessionStore};
use anyhow::{Error, Result};
use chrono::Utc;
use futures::Future;
use log::*;
use std::net::Ipv4Addr;
use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Mutex},
    task::Poll,
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use tunshell_shared::ServerMessage;

type Connection = ClientConnection<TlsStream<TcpStream>>;

pub(super) struct Server {
    config: Config,
    sessions: SessionStore,
    connections: Connections,
}

struct Connections {
    pending: Vec<JoinHandle<Result<Connection>>>,
    waiting: HashMap<String, Connection>,
    paired: Vec<JoinHandle<(Connection, Connection)>>,
}

struct TlsListener {
    tcp: TcpListener,
    tls: TlsAcceptor,
}

impl Connections {
    fn new() -> Self {
        Self {
            pending: vec![],
            waiting: HashMap::new(),
            paired: vec![],
        }
    }

    async fn next_waiting(&mut self) -> Result<Connection> {
        futures::future::poll_fn(|cx| {
            for pending in &mut self.pending {
                let poll = Pin::new(pending).poll(cx);

                if let Poll::Ready(result) = poll {
                    return Poll::Ready(result.unwrap_or_else(|err| Err(Error::from(err))));
                }
            }

            Poll::Pending
        })
        .await
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

    pub(super) async fn start(&mut self) -> Result<()> {
        let mut tls_listener = TlsListener::bind(&self.config).await?;

        loop {
            tokio::select! {
                stream = tls_listener.accept() => self.handle_new_connection(stream),
                next = self.connections.next_waiting() => {}
            }
        }
    }

    fn handle_new_connection(&mut self, stream: Result<TlsStream<TcpStream>>) {
        if let Err(err) = stream {
            warn!("error while establishing connection: {:?}", err);
            return;
        }

        self.connections
            .pending
            .push(self.negotiate_key(stream.unwrap()));
    }

    fn negotiate_key(&self, stream: TlsStream<TcpStream>) -> JoinHandle<Result<Connection>> {
        let mut sessions = self.sessions.clone();

        tokio::spawn(async move {
            let peer_addr = stream.get_ref().0.peer_addr()?;
            let mut connection = Connection::new(stream);

            let key = connection
                .wait_for_key(Duration::from_millis(3000))
                .await?
                .key;

            let session = sessions.find_by_key(key.as_ref()).await?;

            if let None = session {
                connection.write(ServerMessage::KeyRejected).await?;
                return Err(Error::msg("client did not supply valid key"));
            }

            let mut session = session.unwrap();

            if !is_session_valid_to_join(&session, key.as_ref()) {
                connection.write(ServerMessage::KeyRejected).await?;
                return Err(Error::msg("session is not valid to join"));
            }

            let participant = session.participant_mut(key.as_ref()).unwrap();
            participant.set_joined(peer_addr.ip());
            sessions.save(&session).await?;

            Ok(connection)
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use lazy_static::lazy_static;
    use rustls::ClientConfig;
    use std::net::SocketAddr;
    use tokio::runtime::Runtime;
    use tokio_rustls::{client::TlsStream, TlsConnector};
    use tokio_util::compat::*;
    use tunshell_shared::{ClientMessage, MessageStream};

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

    async fn init_client_server_pair() -> ClientConnection {
        let port = init_port_number();

        let mut server_config = Config::from_env().unwrap();
        server_config.port = port;

        let sessions = SessionStore::new(db::connect().await.unwrap());

        let mut server = Server::new(server_config.clone(), sessions);

        tokio::spawn(async move { server.start().await });

        // Give server time to bind
        tokio::time::delay_for(Duration::from_millis(10)).await;

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

        let ca_file = std::fs::File::open(std::env::var("TLS_CA_CERT").unwrap()).unwrap();
        let mut ca_file = std::io::BufReader::new(ca_file);
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

    #[test]
    fn test_valid_key() {
        Runtime::new().unwrap().block_on(async {
            let mut client = init_client_server_pair().await;
        });
    }
}
