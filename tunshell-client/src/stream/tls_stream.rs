use crate::TunnelStream;
use anyhow::Result;
use log::*;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::{
    rustls::{
        NoClientAuth, Certificate, ClientConfig, RootCertStore,
        ServerCertVerified, ServerCertVerifier, ServerConfig, TLSError,
    },
    TlsAcceptor, TlsConnector, TlsStream,
};
use webpki::DNSNameRef;

impl<IO: AsyncRead + AsyncWrite + Send + Unpin> TunnelStream for TlsStream<IO> {}

// TODO: Re-architect relay server to be a CA and generate certs for each session
pub async fn establish_tls_connection(
    inner_stream: Box<dyn TunnelStream>,
    master_side: bool,
) -> Result<TlsStream<Box<dyn TunnelStream>>> {
    info!("establishing tls connection");

    if master_side {
        let config =
            ServerConfig::new(NoClientAuth::new());
            config.set_single_cert(cert_chain, key_der)

        let acceptor = TlsAcceptor::from(Arc::new(config));

        let stream = acceptor.accept(inner_stream).await?;

        Ok(TlsStream::from(stream))
    } else {
        let mut config = ClientConfig::new();
        config
            .dangerous()
            .set_certificate_verifier(Arc::from(NullCertVerifier {}));

        let connector = TlsConnector::from(Arc::new(config));

        let stream = connector
            .connect(
                DNSNameRef::try_from_ascii("todo".as_bytes()).unwrap(),
                inner_stream,
            )
            .await?;

        Ok(TlsStream::from(stream))
    }
}

struct NullCertVerifier {}

impl ServerCertVerifier for NullCertVerifier {
    fn verify_server_cert(
        &self,
        _roots: &RootCertStore,
        _presented_certs: &[Certificate],
        _dns_name: DNSNameRef,
        _ocsp_response: &[u8],
    ) -> Result<ServerCertVerified, TLSError> {
        // TODO: this is bad (see first comment)
        Ok(ServerCertVerified::assertion())
    }
}
