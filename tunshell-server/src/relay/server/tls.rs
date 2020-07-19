use super::super::config::Config;
use anyhow::Result;
use log::*;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{server::TlsStream, TlsAcceptor};

pub(super) struct TlsListener {
    tcp: TcpListener,
    tls: TlsAcceptor,
}

impl TlsListener {
    pub(super) async fn bind(config: &Config) -> Result<Self> {
        Ok(Self {
            tcp: TcpListener::bind((Ipv4Addr::new(0, 0, 0, 0), config.port)).await?,
            tls: TlsAcceptor::from(Arc::clone(&config.tls_config)),
        })
    }

    pub(super) async fn accept(&mut self) -> Result<TlsStream<TcpStream>> {
        let (socket, addr) = self.tcp.accept().await?;
        debug!("received connection from {}", addr);

        Ok(self.tls.accept(socket).await?)
    }
}
