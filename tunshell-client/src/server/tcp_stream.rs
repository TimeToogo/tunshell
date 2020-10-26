use crate::Config;
use anyhow::{bail, Context as AnyhowContext, Result};
use std::{time::Duration, net::ToSocketAddrs};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub struct TcpServerStream {
    inner: TcpStream
}

impl TcpServerStream {
    pub fn inner(self) -> TcpStream {
        self.inner
    }

    pub async fn connect(config: &Config, port: u16) -> Result<Self> {
        let network_stream = if let Ok(http_proxy) = std::env::var("HTTP_PROXY") {
            log::info!("Connecting to relay server via http proxy {}", http_proxy);

            connect_via_http_proxy(config, port, http_proxy).await?
        } else {
            log::info!("Connecting to relay server over TCP");
            let relay_addr = (config.relay_host(), port)
                .to_socket_addrs()?
                .next()
                .unwrap();

            TcpStream::connect(relay_addr).await?
        };

        if let Err(err) = network_stream.set_keepalive(Some(Duration::from_secs(30))) {
            log::warn!("failed to set tcp keepalive: {}", err);
        }

        Ok(Self {
            inner: network_stream,
        })
    }
}

async fn connect_via_http_proxy(
    config: &Config,
    port: u16,
    http_proxy: String,
) -> Result<TcpStream> {
    let proxy_addr = http_proxy.to_socket_addrs()?.next().unwrap();
    let mut proxy_stream = TcpStream::connect(proxy_addr).await?;

    proxy_stream
        .write_all(format!("CONNECT {}:{} HTTP/1.1\n\n", config.relay_host(), port).as_bytes())
        .await?;
    let mut read_buff = [0u8; 1024];

    let read = match proxy_stream.read(&mut read_buff).await? {
        0 => bail!("Failed to read response from http proxy"),
        read @ _ => read,
    };

    let response =
        String::from_utf8(read_buff[..read].to_vec()).context("failed to parse proxy response")?;
    if !response.contains("HTTP/1.1 200") && !response.contains("HTTP/1.0 200") {
        bail!(format!(
            "invalid response returned from http proxy: {}",
            response
        ));
    }

    Ok(proxy_stream)
}