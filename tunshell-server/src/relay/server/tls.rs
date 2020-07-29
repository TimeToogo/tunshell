use super::{super::config::Config, IoStream};
use anyhow::{Error, Result};
use log::*;
use mpsc::{Receiver, Sender};
use std::net::{Ipv4Addr, SocketAddr};
use std::{sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::JoinHandle,
    time::timeout,
};
use tokio_rustls::{server::TlsStream, TlsAcceptor};

pub(super) struct TlsListener {
    _listener: JoinHandle<()>,
    con_rx: Receiver<Result<TlsStream<TcpStream>>>,
    terminate_tx: Sender<()>,
}

impl IoStream for TlsStream<TcpStream> {
    fn get_peer_addr(&self) -> Result<SocketAddr> {
        self.get_ref().0.peer_addr().map_err(Error::from)
    }
}

impl TlsListener {
    pub(super) async fn bind(config: &Config) -> Result<Self> {
        let tcp = TcpListener::bind((Ipv4Addr::new(0, 0, 0, 0), config.tls_port)).await?;
        let tls = TlsAcceptor::from(Arc::clone(&config.tls_config));

        let (terminate_tx, terminate_rx) = mpsc::channel(1);

        let (_listener, con_rx) = Self::listen_for_connections(tcp, tls, terminate_rx);

        Ok(Self {
            _listener,
            con_rx,
            terminate_tx,
        })
    }

    fn listen_for_connections(
        mut tcp: TcpListener,
        mut tls: TlsAcceptor,
        mut terminate_rx: Receiver<()>,
    ) -> (JoinHandle<()>, Receiver<Result<TlsStream<TcpStream>>>) {
        let (mut con_tx, con_rx) = mpsc::channel(128);

        let task = tokio::spawn(async move {
            loop {
                let incoming = tokio::select! {
                    con = accept_tls_connection(&mut tcp, &mut tls) => con,
                    _ = terminate_rx.recv() => break
                };

                if let Err(_) = con_tx.send(incoming).await {
                    break;
                }
            }
        });

        (task, con_rx)
    }

    pub(crate) async fn accept(&mut self) -> Result<TlsStream<TcpStream>> {
        self.con_rx
            .recv()
            .await
            .unwrap_or_else(|| Err(Error::msg("channel closed")))
    }
}

impl Drop for TlsListener {
    fn drop(&mut self) {
        self.terminate_tx
            .try_send(())
            .unwrap_or_else(|err| warn!("failed to send terminate message: {}", err));
    }
}

async fn accept_tls_connection(
    tcp: &mut TcpListener,
    tls: &mut TlsAcceptor,
) -> Result<TlsStream<TcpStream>> {
    let (socket, addr) = tcp.accept().await?;
    debug!("received connection from {}", addr);

    let result = timeout(Duration::from_secs(5), tls.accept(socket)).await;

    if let Err(err) = result {
        warn!("timed out while establishing tls connection with {}", addr);
        return Err(Error::from(err));
    }

    let result = result.unwrap();

    if let Err(err) = result {
        warn!(
            "error occurred while establishing TLS connection: {:?} [{}]",
            err, addr
        );
        return Err(Error::from(err));
    }

    debug!("established TLS connection with {}", addr);
    Ok(result.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::server::tests::insecure_tls_config;
    use lazy_static::lazy_static;
    use std::{net::SocketAddr, sync::Mutex};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        runtime::Runtime,
    };
    use tokio_rustls::{client, TlsConnector};

    lazy_static! {
        static ref TCP_PORT_NUMBER: Mutex<u16> = Mutex::from(45555);
    }

    fn init_port_number() -> u16 {
        let mut port = TCP_PORT_NUMBER.lock().unwrap();

        *port += 1;

        *port - 1
    }

    async fn init_server(config: &mut Config) -> TlsListener {
        config.tls_port = init_port_number();
        let server = TlsListener::bind(&config).await;

        server.unwrap()
    }

    async fn init_connection(port: u16) -> client::TlsStream<TcpStream> {
        let client = TcpStream::connect(SocketAddr::from((Ipv4Addr::new(127, 0, 0, 1), port)))
            .await
            .unwrap();

        let client_config = insecure_tls_config();

        let client = TlsConnector::from(Arc::new(client_config))
            .connect(
                webpki::DNSNameRef::try_from_ascii("localhost".as_bytes()).unwrap(),
                client,
            )
            .await
            .unwrap();

        client
    }

    #[test]
    fn test_connect_to_listener() {
        Runtime::new().unwrap().block_on(async {
            let mut config = Config::from_env().unwrap();
            let mut listener = init_server(&mut config).await;
            let mut client_con = init_connection(config.tls_port).await;

            let mut server_con = listener.accept().await.unwrap();

            assert_eq!(
                client_con.get_ref().0.peer_addr().unwrap(),
                server_con.get_ref().0.local_addr().unwrap()
            );
            assert_eq!(
                client_con.get_ref().0.local_addr().unwrap(),
                server_con.get_ref().0.peer_addr().unwrap()
            );

            client_con.write(&[1, 2, 3]).await.unwrap();

            let mut buff = [0u8; 1024];
            let read = server_con.read(&mut buff).await.unwrap();

            assert_eq!(&buff[..read], &[1, 2, 3]);
        });
    }

    #[test]
    fn test_simultaneous_connections_to_listener() {
        Runtime::new().unwrap().block_on(async {
            let mut config = Config::from_env().unwrap();
            let mut listener = init_server(&mut config).await;
            let (client_con1, client_con2) = futures::join!(
                init_connection(config.tls_port),
                init_connection(config.tls_port)
            );

            let server_con1 = listener.accept().await.unwrap();
            let server_con2 = listener.accept().await.unwrap();

            assert_eq!(
                client_con1.get_ref().0.peer_addr().unwrap(),
                server_con1.get_ref().0.local_addr().unwrap()
            );
            assert_eq!(
                client_con2.get_ref().0.local_addr().unwrap(),
                server_con2.get_ref().0.peer_addr().unwrap()
            );
        });
    }
}
