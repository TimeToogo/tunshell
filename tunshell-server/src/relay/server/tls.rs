use super::super::config::Config;
use anyhow::{Error, Result};
use log::*;
use std::net::Ipv4Addr;
use std::{sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    time::timeout,
};
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

        let result = timeout(Duration::from_secs(5), self.tls.accept(socket)).await;

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
}
