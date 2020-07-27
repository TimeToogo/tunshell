use tokio::io::{AsyncRead, AsyncWrite};

mod crypto;
mod aes_stream;
mod relay_stream;

pub use aes_stream::*;
pub use relay_stream::*;

pub trait TunnelStream: AsyncRead + AsyncWrite + Send + Unpin {}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::Cursor;
    use tokio_util::compat::Compat;

    impl TunnelStream for Compat<Cursor<Vec<u8>>> {}
}
