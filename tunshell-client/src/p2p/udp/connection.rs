use super::{
    negotiate_connection, SendEvent, UdpConnectionConfig, UdpConnectionOrchestrator,
    UdpConnectionState, UdpConnectionVars,
};
use anyhow::{Error, Result};
use log::*;
use std::io;
use std::mem;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

/// This struct is the entry point into the reliable UDP connection API
pub struct UdpConnection {
    state: State,
}

#[derive(Clone)]
pub struct UdpConnectParams {
    pub ip: IpAddr,
    pub suggested_port: u16,
    pub master_side: bool,
}

#[allow(dead_code)]
struct Running {
    con: Arc<Mutex<UdpConnectionVars>>,
    orchestrator: UdpConnectionOrchestrator,
    sender: UnboundedSender<SendEvent>,
}

enum State {
    New(UdpConnectionConfig),
    Binding,
    Bound(UdpConnectionConfig, UdpSocket),
    Connecting,
    Running(Running),
    Stopping,
    Disconnecting(Running),
    Disconnected,
}

impl UdpConnection {
    /// Initialises a new UDP connection
    pub fn new(config: UdpConnectionConfig) -> Self {
        Self {
            state: State::New(config),
        }
    }

    /// Whether the connection has been activated
    pub fn is_new(&self) -> bool {
        match &self.state {
            State::New(_) => true,
            _ => false,
        }
    }

    /// Whether the connection is bound
    pub fn is_bound(&self) -> bool {
        match &self.state {
            State::Bound(_, _) => true,
            _ => false,
        }
    }

    /// Whether the connection is in a valid connected state.
    pub fn is_connected(&self) -> bool {
        let running = match &self.state {
            State::Running(running) => running,
            _ => return false,
        };

        let con = running.con.lock().unwrap();
        con.state == UdpConnectionState::Connected
    }

    /// Whether the connection has been disconnected
    #[allow(dead_code)]
    pub fn is_disconnected(&self) -> bool {
        let running = match &self.state {
            State::Disconnected => return true,
            State::Running(running) => running,
            _ => return false,
        };

        let con = running.con.lock().unwrap();
        return !con.is_connected();
    }

    /// Bind the connection to the UDP socket
    pub async fn bind(&mut self) -> Result<u16> {
        if !self.is_new() {
            return Err(Error::msg("Connection must be in NEW state"));
        }

        match self.do_bind().await {
            Ok(port) => Ok(port),
            Err((config, err)) => {
                debug!("UDP bind failed: {}", err);
                self.state = State::New(config);
                Err(err)
            }
        }
    }

    async fn do_bind(&mut self) -> Result<u16, (UdpConnectionConfig, Error)> {
        assert!(self.is_new());

        let config = match mem::replace(&mut self.state, State::Binding) {
            State::New(config) => config,
            _ => unreachable!(),
        };

        let socket = UdpSocket::bind(config.bind_addr())
            .await
            .map_err(|err| (config.clone(), Error::from(err)))?;
        let port = socket
            .local_addr()
            .map_err(|err| (config.clone(), Error::from(err)))?
            .port();

        self.state = State::Bound(config, socket);

        Ok(port)
    }

    /// Attempts to make a connection with the peer as specified in the UdpConnectParams.
    /// This does not guarantee that the connection will be made on the suggested port.
    pub async fn connect(&mut self, params: UdpConnectParams) -> Result<()> {
        if !self.is_new() & !self.is_bound() {
            return Err(Error::msg("Connection must be in NEW or BOUND state"));
        }

        if !self.is_bound() {
            self.bind().await.map(|_| ())?
        }

        match self.do_connect(params).await {
            Ok(_) => Ok(()),
            Err((config, err)) => {
                debug!("UDP connection failed: {}", err);
                self.state = State::New(config);
                Err(err)
            }
        }
    }

    async fn do_connect(
        &mut self,
        params: UdpConnectParams,
    ) -> Result<(), (UdpConnectionConfig, Error)> {
        assert!(self.is_bound());

        let (config, mut socket) = match mem::replace(&mut self.state, State::Connecting) {
            State::Bound(config, socket) => (config, socket),
            _ => unreachable!(),
        };

        let mut con = UdpConnectionVars::new(config);

        let result = negotiate_connection(
            &mut con,
            &mut socket,
            params.ip,
            params.suggested_port,
            params.master_side,
        )
        .await;

        let (send_tx, send_rx) = unbounded_channel();

        match result {
            Ok(_) => con.set_state_connected(send_tx.clone()),
            Err(err) => {
                con.set_state_connect_failed();
                return Err((con.config().clone(), err));
            }
        }

        let con = Arc::new(Mutex::new(con));
        let mut orchestrator = UdpConnectionOrchestrator::new(socket, Arc::clone(&con), send_rx);
        orchestrator.start_orchestration_loop();

        self.state = State::Running(Running {
            con,
            orchestrator,
            sender: send_tx,
        });

        Ok(())
    }

    /// Closes the connection
    #[allow(dead_code)]
    pub async fn close(&mut self) -> Result<()> {
        if !self.is_connected() {
            return Err(Error::msg("Connection must be in CONNECTED state"));
        }

        self.shutdown().await.map_err(|err| Error::from(err))
    }
}

impl AsyncRead for UdpConnection {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        if !self.is_connected() {
            warn!("attempted to poll connection which is not connected");
            return Poll::Ready(Err(io::Error::from(io::ErrorKind::NotConnected)));
        }

        let running = match &self.state {
            State::Running(running) => running,
            _ => unreachable!(),
        };

        let mut con = running.con.lock().unwrap();

        if con.recv_available_bytes() == 0 {
            con.recv_wakers.push(cx.waker().clone());
            return Poll::Pending;
        } else {
            let data = con.recv_drain_bytes(buff.len());
            &buff[..data.len()].copy_from_slice(&data[..]);
            return Poll::Ready(Ok(data.len()));
        }
    }
}

impl AsyncWrite for UdpConnection {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &[u8],
    ) -> Poll<io::Result<usize>> {
        assert!(buff.len() > 0);

        if !self.is_connected() {
            warn!("attempted to poll connection which is not connected");
            return Poll::Ready(Err(io::Error::from(io::ErrorKind::NotConnected)));
        }

        let running = match &self.state {
            State::Running(running) => running,
            _ => unreachable!(),
        };

        // Create the packet to be sent
        let packet = {
            let mut con = running.con.lock().unwrap();

            // We first check if the connection is currently congested
            // and wait until the connection decongests if so
            if con.is_congested() {
                debug!(
                    "connection is congested, waiting until window grows before continuing sending"
                );
                con.wait_until_decongested(cx.waker().clone());
                return Poll::Pending;
            }

            con.create_data_packet(buff)
        };

        let bytes_sent = packet.payload.len();
        let result = running.sender.send(SendEvent::Send(packet));

        match result {
            Ok(_) => Poll::Ready(Ok(bytes_sent)),
            Err(err) => {
                warn!("failed to send event to orchestration sender: {}", err);
                Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Flushing is handled by the orchestrator
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let State::Disconnecting(running) = &self.state {
            if !self.is_connected() {
                self.state = State::Disconnected;
                return Poll::Ready(Ok(()));
            } else {
                let mut con = running.con.lock().unwrap();
                con.close_wakers.push(cx.waker().clone());
                return Poll::Pending;
            }
        }

        let running = match mem::replace(&mut self.state, State::Stopping) {
            State::Running(running) => running,
            _ => unreachable!(),
        };

        let result = running.sender.send(SendEvent::Close);

        match result {
            Ok(_) => {}
            Err(err) => {
                warn!("failed to send event to orchestration sender: {}", err);
                return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
            }
        };

        {
            let mut con = running.con.lock().unwrap();
            con.close_wakers.push(cx.waker().clone());
        }

        self.state = State::Disconnecting(running);
        return Poll::Pending;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazy_static::lazy_static;
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::runtime::Runtime;
    use tokio::time::delay_for;

    lazy_static! {
        static ref UDP_PORT_NUMBER: Mutex<u16> = Mutex::from(27660);
    }

    fn init_udp_port_number_pairs() -> (u16, u16) {
        let mut port = UDP_PORT_NUMBER.lock().unwrap();

        *port += 2;

        (*port - 2, *port - 1)
    }

    async fn init_connection_pair() -> (UdpConnection, UdpConnection) {
        let (port1, port2) = init_udp_port_number_pairs();

        let config1 = UdpConnectionConfig::default()
            .with_connect_timeout(Duration::from_millis(500))
            .with_bind_addr(SocketAddr::from(([0, 0, 0, 0], port1)));

        let config2 = UdpConnectionConfig::default()
            .with_connect_timeout(Duration::from_millis(500))
            .with_bind_addr(SocketAddr::from(([0, 0, 0, 0], port2)));

        let mut connection1 = UdpConnection::new(config1);
        let mut connection2 = UdpConnection::new(config2);

        let (result1, result2) = tokio::join!(
            connection1.connect(UdpConnectParams {
                ip: "127.0.0.1".parse().unwrap(),
                suggested_port: port2,
                master_side: true
            }),
            connection2.connect(UdpConnectParams {
                ip: "127.0.0.1".parse().unwrap(),
                suggested_port: port1,
                master_side: false
            })
        );

        result1.unwrap();
        result2.unwrap();

        (connection1, connection2)
    }

    #[test]
    fn test_new_connection() {
        let connection = UdpConnection::new(UdpConnectionConfig::default());

        assert_eq!(connection.is_new(), true);
        assert_eq!(connection.is_bound(), false);
        assert_eq!(connection.is_connected(), false);
        assert_eq!(connection.is_disconnected(), false);
    }

    #[test]
    fn test_bind() {
        Runtime::new().unwrap().block_on(async {
            let config = UdpConnectionConfig::default();
            let mut connection = UdpConnection::new(config);

            let result = connection.bind().await;

            result.unwrap();
            assert_eq!(connection.is_new(), false);
            assert_eq!(connection.is_bound(), true);
            assert_eq!(connection.is_connected(), false);
            assert_eq!(connection.is_disconnected(), false);
        });
    }

    #[test]
    fn test_connect_without_peer_should_fail() {
        Runtime::new().unwrap().block_on(async {
            let config =
                UdpConnectionConfig::default().with_connect_timeout(Duration::from_millis(50));
            let mut connection = UdpConnection::new(config);

            let result = connection
                .connect(UdpConnectParams {
                    ip: "127.0.0.1".parse().unwrap(),
                    suggested_port: 1234,
                    master_side: true,
                })
                .await;

            assert_eq!(result.is_err(), true);
            assert_eq!(connection.is_new(), true);
            assert_eq!(connection.is_bound(), false);
            assert_eq!(connection.is_connected(), false);
            assert_eq!(connection.is_disconnected(), false);
        });
    }

    #[test]
    fn test_connect_with_peer() {
        Runtime::new().unwrap().block_on(async {
            init_connection_pair().await;
        });
    }

    #[test]
    fn test_connect_write_then_read() {
        Runtime::new().unwrap().block_on(async {
            let (mut con1, mut con2) = init_connection_pair().await;

            con1.write(&[1u8, 2, 3, 4, 5]).await.unwrap();

            let mut buff = [0u8; 1024];
            let read = con2.read(&mut buff).await.unwrap();
            assert_eq!(&buff[..read], [1u8, 2, 3, 4, 5]);

            con2.write(&[6u8, 7, 8, 9, 10]).await.unwrap();

            let mut buff = [0u8; 1024];
            let read = con1.read(&mut buff).await.unwrap();
            assert_eq!(&buff[..read], [6u8, 7, 8, 9, 10]);
        });
    }

    #[test]
    fn test_connect_write_then_close_one_side() {
        // TODO: fix flaky test
        if std::env::var("CI").is_ok() {
            return; 
        }

        Runtime::new().unwrap().block_on(async {
            let (mut con1, mut con2) = init_connection_pair().await;

            con1.write(&[1u8, 2, 3, 4, 5]).await.unwrap();

            con2.close().await.unwrap();

            assert_eq!(con2.is_new(), false);
            assert_eq!(con2.is_connected(), false);
            assert_eq!(con2.is_disconnected(), true);

            // Wait for close packet to be sent and process
            delay_for(Duration::from_millis(500)).await;

            assert_eq!(con1.is_new(), false);
            assert_eq!(con1.is_connected(), false);
            assert_eq!(con1.is_disconnected(), true);

            // Write / recv should fail
            let result = con1.write(&[1u8, 2, 3, 4, 5]).await;
            assert_eq!(result.is_err(), true);

            let mut buff = [0u8; 1024];
            let result = con1.write(&mut buff).await;
            assert_eq!(result.is_err(), true);
        });
    }
}
