use crate::Config;
use anyhow::{bail, Context as AnyhowContext, Result};
use std::{
    io,
    net::ToSocketAddrs,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{client::TlsStream, rustls::ClientConfig, TlsConnector};
use webpki::DNSNameRef;

pub struct TlsServerStream {
    inner: TlsStream<TcpStream>,
}

impl TlsServerStream {
    pub async fn connect(config: &Config, port: u16) -> Result<Self> {
        let mut tls_config = ClientConfig::default();
        tls_config
            .root_store
            .add_server_trust_anchors(&webpki_roots::TLS_SERVER_ROOTS);

        if config.dangerous_disable_relay_server_verification() {
            use tokio_rustls::rustls;

            struct NullCertVerifier {}

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

            log::warn!("disabling TLS verification");
            tls_config
                .dangerous()
                .set_certificate_verifier(Arc::new(NullCertVerifier {}));
        }

        let connector = TlsConnector::from(Arc::new(tls_config));

        let relay_dns_name = DNSNameRef::try_from_ascii_str(config.relay_host())?;

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

        network_stream.set_keepalive(Some(Duration::from_secs(30)))?;
        let transport_stream = connector.connect(relay_dns_name, network_stream).await?;

        Ok(Self {
            inner: transport_stream,
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
