use super::tcp_stream::TcpServerStream;
use crate::Config;
use anyhow::Result;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_native_tls::{TlsConnector, TlsStream};

pub struct TlsServerStream {
    inner: TlsStream<TcpStream>,
}

impl TlsServerStream {
    pub async fn connect(config: &Config, port: u16) -> Result<Self> {
        let connector = TlsConnector::from(native_tls::TlsConnector::builder().build()?);

        let tcp_stream = TcpServerStream::connect(config, port).await?.inner();

        let transport_stream = connector.connect(config.relay_host(), tcp_stream).await?;

        Ok(Self {
            inner: transport_stream,
        })
    }
}

impl AsyncRead for TlsServerStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for TlsServerStream {
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

impl super::AsyncIO for TlsServerStream {}
