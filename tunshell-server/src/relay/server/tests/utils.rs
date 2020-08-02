use super::*;
use crate::db;
use crate::db::SessionStore;
use anyhow::{Error, Result};
use db::{Participant, Session};
use futures::StreamExt;
use lazy_static::lazy_static;
use rustls::ClientConfig;
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use tokio::{net::TcpStream, task::JoinHandle};
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use tunshell_shared::{ClientMessage, KeyPayload, MessageStream, ServerMessage};
use warp::Filter;

lazy_static! {
    static ref TCP_PORT_NUMBER: Mutex<u16> = Mutex::from(35555);
}

type ClientConnection = MessageStream<ClientMessage, ServerMessage, Compat<TlsStream<TcpStream>>>;

pub(super) fn init_port_numbers() -> (u16, u16) {
    let mut port = TCP_PORT_NUMBER.lock().unwrap();

    *port += 2;

    (*port - 1, *port - 2)
}

pub(super) struct TerminableServer {
    tls_port: u16,
    _api_port: u16,
    running: JoinHandle<Server<warp::http::StatusCode>>,
    terminate: mpsc::Sender<()>,
}

impl TerminableServer {
    pub(super) async fn stop(mut self) -> Result<Server<warp::http::StatusCode>> {
        self.terminate.send(()).await?;
        self.running.await.map_err(Error::from)
    }
}

pub(super) async fn init_server(mut server_config: Config) -> TerminableServer {
    let (tls_port, api_port) = init_port_numbers();
    server_config.tls_port = tls_port;
    server_config.api_port = api_port;

    let sessions = SessionStore::new(db::connect().await.unwrap());

    let mut server = Server::new(
        server_config.clone(),
        sessions,
        warp::path("unused")
            .map(|| warp::http::StatusCode::OK)
            .boxed(),
    );
    let (tx, rx) = mpsc::channel(1);

    let running = tokio::spawn(async move {
        server.start(Some(rx)).await.unwrap();
        server
    });

    // Give server time to bind
    loop {
        let socket = TcpStream::connect(SocketAddr::from((
            Ipv4Addr::new(127, 0, 0, 1),
            server_config.tls_port,
        )))
        .await;

        if let Ok(_) = socket {
            break;
        }

        tokio::time::delay_for(Duration::from_millis(100)).await;
    }

    TerminableServer {
        tls_port: server_config.tls_port,
        _api_port: server_config.api_port,
        running,
        terminate: tx,
    }
}

pub(super) async fn create_client_connection_to_server(
    server: &TerminableServer,
) -> ClientConnection {
    let client = TcpStream::connect(SocketAddr::from((
        Ipv4Addr::new(127, 0, 0, 1),
        server.tls_port,
    )))
    .await
    .unwrap();

    let client = TlsConnector::from(Arc::new(insecure_tls_config()))
        .connect(
            webpki::DNSNameRef::try_from_ascii("localhost".as_bytes()).unwrap(),
            client,
        )
        .await
        .unwrap();

    ClientConnection::new(client.compat())
}

pub(crate) struct NullCertVerifier {}

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

pub(crate) fn insecure_tls_config() -> ClientConfig {
    let mut client_config = ClientConfig::default();

    let conf = Config::from_env().unwrap();
    client_config
        .set_single_client_cert(
            Config::parse_tls_cert(conf.tls_cert_path).unwrap(),
            Config::parse_tls_private_key(conf.tls_key_path).unwrap(),
        )
        .unwrap();

    client_config
        .dangerous()
        .set_certificate_verifier(Arc::new(NullCertVerifier {}));

    client_config
}

pub(super) async fn create_mock_session() -> Session {
    let mock_session = Session::new(Participant::default(), Participant::default());

    let db = db::connect().await.unwrap();
    SessionStore::new(db).save(&mock_session).await.unwrap();

    mock_session
}

pub(super) async fn send_key_to_server(con: &mut ClientConnection, key: &str) {
    con.write(&ClientMessage::Key(KeyPayload {
        key: key.to_owned(),
    }))
    .await
    .unwrap();
}

pub(super) async fn assert_next_message_is_key_accepted(con: &mut ClientConnection) {
    assert_eq!(
        con.next().await.unwrap().unwrap(),
        ServerMessage::KeyAccepted
    );
}

pub(super) async fn assert_next_message_is_peer_joined(
    con: &mut ClientConnection,
    peer_ip_address: &str,
    peer_key: &str,
) -> PeerJoinedPayload {
    let message = con.next().await.unwrap().unwrap();
    let payload = match &message {
        ServerMessage::PeerJoined(i) => i.clone(),
        msg @ _ => panic!(
            "expected PeerJoined payload from connection but received: {:?}",
            msg
        ),
    };

    assert_eq!(
        message,
        ServerMessage::PeerJoined(PeerJoinedPayload {
            peer_ip_address: peer_ip_address.to_owned(),
            peer_key: peer_key.to_owned(),
            session_nonce: payload.session_nonce.clone()
        })
    );

    payload
}
