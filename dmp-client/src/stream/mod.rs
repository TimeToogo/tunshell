use thrussh::Tcp;
use tokio::io::{AsyncRead, AsyncWrite};

mod relay_stream;

pub use relay_stream::*;

pub trait TunnelStream: AsyncRead + AsyncWrite + Tcp + Unpin {}
