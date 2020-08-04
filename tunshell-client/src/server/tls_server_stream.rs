use crate::Config;
use anyhow::Result;
use std::{
    io,
    net::ToSocketAddrs,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_rustls::{client::TlsStream, rustls::ClientConfig, TlsConnector};
use webpki::DNSNameRef;

pub struct ServerStream {
    inner: TlsStream<TcpStream>,
}

impl ServerStream {
    pub async fn connect(config: &Config) -> Result<Self> {
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

        let relay_addr = (config.relay_host(), config.relay_port())
            .to_socket_addrs()?
            .next()
            .unwrap();

        let network_stream = TcpStream::connect(relay_addr).await?;
        network_stream.set_keepalive(Some(Duration::from_secs(30)))?;
        let transport_stream = connector.connect(relay_dns_name, network_stream).await?;

        Ok(Self {
            inner: transport_stream,
        })
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
