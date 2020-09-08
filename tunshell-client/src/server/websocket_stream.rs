use super::tls_stream::TlsServerStream;
use crate::Config;
use anyhow::Result;
use async_tungstenite::{client_async, tungstenite::Message, WebSocketStream};
use futures::{stream::StreamExt, SinkExt};
use log::*;
use std::{
    cmp, io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::compat::*;

pub struct WebsocketServerStream {
    inner: WebSocketStream<Compat<TlsServerStream>>,

    read_buff: Vec<u8>,
    write_buff: Option<Message>,
}

impl WebsocketServerStream {
    pub async fn connect(config: &Config) -> Result<Self> {
        let tls_stream = TlsServerStream::connect(config, config.relay_ws_port()).await?;

        let url = format!(
            "wss://{}:{}/ws",
            config.relay_host(),
            config.relay_ws_port()
        );
        let (websocket_stream, _) = client_async(url, tls_stream.compat()).await?;

        Ok(Self {
            inner: websocket_stream,
            read_buff: vec![],
            write_buff: None,
        })
    }
}

impl AsyncRead for WebsocketServerStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        while self.read_buff.is_empty() {
            let msg = match self.inner.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(msg))) => msg,
                Poll::Ready(Some(Err(err))) => {
                    error!("error while reading from websocket: {}", err);
                    return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
                }
                Poll::Ready(None) => return Poll::Ready(Ok(0)),
                Poll::Pending => return Poll::Pending,
            };

            if msg.is_binary() {
                self.read_buff.extend_from_slice(msg.into_data().as_slice());
            }
        }

        let len = cmp::min(buf.len(), self.read_buff.len());
        buf[..len].copy_from_slice(&self.read_buff[..len]);
        self.read_buff.drain(..len);

        Poll::Ready(Ok(len))
    }
}

impl AsyncWrite for WebsocketServerStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, io::Error>> {
        assert!(buf.len() > 0);

        let mut written = 0;
        let max_size = self
            .inner
            .get_config()
            .max_frame_size
            .unwrap_or(1024 * 1024);

        loop {
            if self.write_buff.is_none() {
                written = cmp::min(max_size, buf.len());
                self.write_buff
                    .replace(Message::binary(buf[..written].to_vec()));
            }

            if let Poll::Pending = self.inner.poll_ready_unpin(cx) {
                return Poll::Pending;
            }

            let msg = self.write_buff.take().unwrap();
            if let Err(err) = self.inner.start_send_unpin(msg) {
                error!("error while writing to websocket: {}", err);
                return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
            }

            if written > 0 {
                return Poll::Ready(Ok(written));
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        if self.write_buff.is_none() {
            return Poll::Ready(Ok(()));
        }

        if let Poll::Pending = self.inner.poll_ready_unpin(cx) {
            return Poll::Pending;
        }

        let msg = self.write_buff.take().unwrap();
        if let Err(err) = self.inner.start_send_unpin(msg) {
            error!("error while writing to websocket: {}", err);
            return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
        }

        if let Poll::Pending = self.inner.poll_flush_unpin(cx) {
            return Poll::Pending;
        }

        return Poll::Ready(Ok(()));
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        self.inner.poll_close_unpin(cx).map_err(|err| {
            error!("error while closing to websocket: {}", err);
            io::Error::from(io::ErrorKind::BrokenPipe)
        })
    }
}

impl super::AsyncIO for WebsocketServerStream {}
