cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        mod tls_server_stream;
        pub use tls_server_stream::*;
    } else {
        mod websocket_stream;
        pub use websocket_stream::*;
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
            "relay.tunshell.com",
            5000,
            "test",
        );

        let result = Runtime::new()
            .unwrap()
            .block_on(ServerStream::connect(&config));

        result.unwrap();
    }
}
