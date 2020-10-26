cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        use tokio::io::{AsyncRead, AsyncWrite};
        
        pub trait AsyncIO : AsyncRead + AsyncWrite + Send + Unpin {}

        pub mod tcp_stream;

        cfg_if::cfg_if! {
            if #[cfg(openssl)] {
                mod openssl_tls_stream;
                pub mod tls_stream {
                    pub use super::openssl_tls_stream::*;
                }
            } else {
                mod ring_tls_stream;
                pub mod tls_stream {
                    pub use super::ring_tls_stream::*;
                }
            }
        }

        pub mod websocket_stream;

        mod dual_stream;
        pub use dual_stream::*;
    } else {
        mod websys_websocket_stream;
        pub use websys_websocket_stream::*;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientMode, Config};
    use tokio::runtime::Runtime;

    #[test]
    fn test_connect_to_relay_server() {
        let config = Config::new(
            ClientMode::Target,
            "test",
            "au.relay.tunshell.com",
            5000,
            443,
            "test",
            true,
            false
        );

        let result = Runtime::new()
            .unwrap()
            .block_on(ServerStream::connect(&config));

        result.unwrap();
    }
}
