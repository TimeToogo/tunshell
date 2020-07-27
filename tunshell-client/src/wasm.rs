use crate::{Config, Client, ClientMode};
use wasm_bindgen::prelude::*;
use std::panic;

#[wasm_bindgen]
pub async fn tunshell_init_client(client_key: String, encryption_salt: String, encryption_key: String) -> bool {
    panic::set_hook(Box::new(console_error_panic_hook::hook));

    let config = Config::new(
        ClientMode::Local,
        &client_key,
        "relay.tunshell.com",
        5001,
        &encryption_salt,
        &encryption_key,
    );

    Client::new(config).start_session().await.is_ok()
}
