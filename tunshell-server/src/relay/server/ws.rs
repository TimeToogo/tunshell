use super::{super::config::Config, IoStream};
use anyhow::{Error, Result};
use futures::{Sink, Stream};
use log::*;
use mpsc::{Receiver, Sender};
use std::{
    cmp, io,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
    task::JoinHandle,
};
use warp::{
    ws::{Message, WebSocket},
    Filter,
};

pub(super) struct WebSocketStream {
    peer_addr: SocketAddr,
    ws: Arc<Mutex<WebSocket>>,
    recv_buff: Vec<u8>,
}

impl WebSocketStream {
    fn new(peer_addr: SocketAddr, ws: WebSocket) -> Self {
        Self {
            peer_addr,
            ws: Arc::new(Mutex::new(ws)),
            recv_buff: vec![],
        }
    }
}

impl AsyncRead for WebSocketStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        while self.recv_buff.len() == 0 {
            let message = {
                let mut ws = self.ws.lock().unwrap();

                let poll = Pin::new(&mut *ws).poll_next(cx);

                let message = match poll {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(None) => {
                        return Poll::Ready(Err(io::Error::from(io::ErrorKind::NotConnected)))
                    }
                    Poll::Ready(Some(Err(err))) => {
                        return Poll::Ready(Err(warp_err_to_io_err(err)))
                    }
                    Poll::Ready(Some(Ok(res))) => res,
                };

                message
            };

            if message.is_binary() {
                debug!("received {} bytes from websocket", message.as_bytes().len());
                self.recv_buff.extend_from_slice(message.as_bytes());
            } else {
                warn!("received non-binary message from websocket: {:?}", message);
            }
        }

        let len = cmp::min(buf.len(), self.recv_buff.len());
        &buf[..len].copy_from_slice(&self.recv_buff[..len]);
        self.recv_buff.drain(..len);

        Poll::Ready(Ok(len))
    }
}

impl AsyncWrite for WebSocketStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let mut ws = self.ws.lock().unwrap();

        let poll = Pin::new(&mut *ws)
            .poll_ready(cx)
            .map_err(warp_err_to_io_err);

        match poll {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
            Poll::Ready(Ok(_)) => {}
        };

        let len = buf.len();
        return Poll::Ready(
            Pin::new(&mut *ws)
                .start_send(Message::binary(buf.to_vec()))
                .map(|_| {
                    debug!("wrote {} bytes from websocket", len);
                    len
                })
                .map_err(warp_err_to_io_err),
        );
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let mut ws = self.ws.lock().unwrap();
        Pin::new(&mut *ws)
            .poll_flush(cx)
            .map_err(warp_err_to_io_err)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let mut ws = self.ws.lock().unwrap();
        Pin::new(&mut *ws)
            .poll_close(cx)
            .map_err(warp_err_to_io_err)
    }
}

fn warp_err_to_io_err(err: warp::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err)
}

impl IoStream for WebSocketStream {
    fn get_peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.peer_addr)
    }
}

pub(super) struct WebSocketListener {
    _listener: JoinHandle<()>,
    con_rx: Receiver<WebSocketStream>,
    terminate_tx: Sender<()>,
}

impl WebSocketListener {
    pub(super) async fn bind(config: &Config) -> Result<Self> {
        let (terminate_tx, terminate_rx) = mpsc::channel(1);
        let (_listener, con_rx) = Self::listen_for_connections(config.clone(), terminate_rx);

        Ok(Self {
            _listener,
            con_rx,
            terminate_tx,
        })
    }

    fn listen_for_connections(
        config: Config,
        mut terminate_rx: Receiver<()>,
    ) -> (JoinHandle<()>, Receiver<WebSocketStream>) {
        let (con_tx, con_rx) = mpsc::channel(128);

        let routes = warp::any()
            .and(warp::ws()) //
            .and(warp::addr::remote())
            .map(move |ws: warp::ws::Ws, addr: Option<SocketAddr>| {
                let mut con_tx = con_tx.clone();

                ws.on_upgrade(move |websocket| async move {
                    if addr.is_none() {
                        warn!("could not get remote ip address from websocket client");
                        return;
                    }

                    let con = WebSocketStream::new(addr.unwrap(), websocket);

                    if let Err(err) = con_tx.send(con).await {
                        error!("failed to send websocket: {}", err);
                    }
                })
            });

        let server = warp::serve(routes)
            .tls()
            .cert_path(config.tls_cert_path)
            .key_path(config.tls_key_path)
            .run(([127, 0, 0, 1], config.ws_port));

        let task = tokio::spawn(async move {
            tokio::select! {
                _ = server => warn!("websocket server ended"),
                _ = terminate_rx.recv() => debug!("websocket server stopped")
            }
        });

        (task, con_rx)
    }

    pub(crate) async fn accept(&mut self) -> Result<WebSocketStream> {
        self.con_rx
            .recv()
            .await
            .ok_or_else(|| Error::msg("channel closed"))
    }
}

impl Drop for WebSocketListener {
    fn drop(&mut self) {
        self.terminate_tx
            .try_send(())
            .unwrap_or_else(|err| warn!("failed to send terminate message: {}", err));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::server::tests::insecure_tls_config;
    use async_tungstenite::{
        async_tls::client_async_tls_with_connector, WebSocketStream as ClientWebSocketStream,
    };
    use futures::{SinkExt, StreamExt};
    use lazy_static::lazy_static;
    use std::{
        net::{Ipv4Addr, SocketAddr},
        sync::Mutex,
        time::Duration,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
        runtime::Runtime,
        time::delay_for,
    };
    use tokio_util::compat::*;
    use tungstenite::protocol::Message as ClientMessage;

    lazy_static! {
        static ref TCP_PORT_NUMBER: Mutex<u16> = Mutex::from(55555);
    }

    fn init_port_number() -> u16 {
        let mut port = TCP_PORT_NUMBER.lock().unwrap();

        *port += 1;

        *port - 1
    }

    async fn init_server(config: &mut Config) -> WebSocketListener {
        config.ws_port = init_port_number();
        let server = WebSocketListener::bind(&config).await;

        delay_for(Duration::from_millis(100)).await;

        server.unwrap()
    }

    async fn init_connection(
        port: u16,
    ) -> (
        SocketAddr,
        ClientWebSocketStream<
            async_tungstenite::stream::Stream<
                tokio_util::compat::Compat<TcpStream>,
                async_tls::client::TlsStream<tokio_util::compat::Compat<TcpStream>>,
            >,
        >,
    ) {
        let client_config = insecure_tls_config();

        let addr = SocketAddr::from((Ipv4Addr::new(127, 0, 0, 1), port));
        let tcp = TcpStream::connect(addr).await.unwrap();
        let local_addr = tcp.local_addr().unwrap();

        let url = format!("wss://localhost:{}", port.to_string().as_str());

        let (ws, _) =
            client_async_tls_with_connector(url.as_str(), tcp.compat(), Some(client_config.into()))
                .await
                .unwrap();

        (local_addr, ws)
    }

    #[test]
    fn test_connect_to_listener() {
        Runtime::new().unwrap().block_on(async {
            let mut config = Config::from_env().unwrap();
            let mut listener = init_server(&mut config).await;
            let (addr, mut client_con) = init_connection(config.ws_port).await;

            let mut server_con = listener.accept().await.unwrap();

            assert_eq!(addr, server_con.get_peer_addr().unwrap());

            client_con
                .send(ClientMessage::binary(vec![1, 2, 3]))
                .await
                .unwrap();

            let mut buff = [0u8; 1024];
            let read = server_con.read(&mut buff).await.unwrap();

            assert_eq!(&buff[..read], &[1, 2, 3]);

            server_con.write_all(&[4, 5, 6]).await.unwrap();

            let message = client_con.next().await.unwrap().unwrap();

            assert_eq!(message, ClientMessage::binary(vec![4, 5, 6]));
        });
    }
}
