use tokio::io::{AsyncRead, AsyncWrite};

mod relay_stream;
mod tls_stream;
mod aes_stream;

pub use relay_stream::*;
pub use tls_stream::*;
pub use aes_stream::*;

pub trait TunnelStream: AsyncRead + AsyncWrite + Send + Unpin {}
