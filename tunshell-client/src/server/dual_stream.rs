use super::tls_stream::TlsServerStream;
use super::websocket_stream::WebsocketServerStream;
use crate::Config;
use anyhow::Result;
use log::*;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::timeout,
};

pub struct ServerStream {
    inner: Box<dyn super::AsyncIO>,
}

impl ServerStream {
    pub async fn connect(config: &Config) -> Result<Self> {
        let connection = timeout(
            config.server_connection_timeout(),
            TlsServerStream::connect(config, config.relay_tls_port()),
        )
        .await;

        let connection: Box<dyn super::AsyncIO> = match connection {
            Ok(Ok(con)) => Box::new(con),
            err @ _ => {
                error!(
                    "Failed to connect via TLS, falling back to websocket: {}",
                    match err {
                        Err(err) => err.to_string(),
                        Ok(Err(err)) => err.to_string(),
                        _ => unreachable!(),
                    }
                );

                let ws = timeout(
                    config.server_connection_timeout(),
                    WebsocketServerStream::connect(config),
                )
                .await??;

                Box::new(ws)
            }
        };

        Ok(Self { inner: connection })
    }
}

impl AsyncRead for ServerStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for ServerStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
