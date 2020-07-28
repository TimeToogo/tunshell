use crate::{Config, Client, ClientMode};
use wasm_bindgen::prelude::*;
use std::panic;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
pub fn tunshell_init_client(client_key: String, encryption_salt: String, encryption_key: String) {
    panic::set_hook(Box::new(console_error_panic_hook::hook));

    let config = Config::new(
        ClientMode::Local,
        &client_key,
        "relay.tunshell.com",
        5001,
        &encryption_salt,
        &encryption_key,
    );

    wasm_bindgen_futures::spawn_local(async move {
        Client::new(config).start_session().await;
    });
}
