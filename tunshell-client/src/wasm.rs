use crate::{Config, Client, ClientMode, HostShell};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use js_sys::{Uint8Array, Promise};
use wasm_bindgen_futures::JsFuture;
use std::panic;
use log::*;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen] 
pub struct BrowserConfig {
    client_key: String,
    encryption_key: String,
    relay_server: String,
    term: TerminalEmulator,
    terminate: Promise
}

#[wasm_bindgen]
impl BrowserConfig {
    #[wasm_bindgen(constructor)]
    pub fn new(client_key: String, encryption_key: String, relay_server: String, term: TerminalEmulator, terminate: Promise) -> Self{
        Self {
            client_key,
            encryption_key,
            relay_server,
            term,
            terminate
        }
    }
}


#[wasm_bindgen(typescript_custom_section)]
const TERMINAL_EMULATOR: &'static str = r#"
interface TerminalEmulator {
    data: () => Promise<string>;
    resize: () => Promise<UInt16Array>;
    write: (data: UInt8Array) => Promise<void>;
    size: () => Uint16Array;
    clone: () => TerminalEmulator;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "TerminalEmulator")]
    pub type TerminalEmulator;

    #[wasm_bindgen(method, js_name = data)]
    pub async fn data(
        this: &TerminalEmulator,
    ) -> JsValue;

    #[wasm_bindgen(method, js_name = resize)]
    pub async fn resize(
        this: &TerminalEmulator,
    ) -> JsValue;

    #[wasm_bindgen(method, js_name = write)]
    pub async fn write(
        this: &TerminalEmulator,
        data: Uint8Array,
    );

    #[wasm_bindgen(method, js_name = size)]
    pub fn size(
        this: &TerminalEmulator
    ) -> JsValue;

    #[wasm_bindgen(method, js_name = clone)]
    pub fn clone(
        this: &TerminalEmulator
    ) -> TerminalEmulator;
}

#[wasm_bindgen]
pub async fn tunshell_init_client(config: BrowserConfig) {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    
    if let Err(err) = console_log::init_with_level(log::Level::Debug) {
        warn!("failed to set log level: {}", err);
    }

    let host_shell = HostShell::new(config.term).unwrap();
    let terminate = config.terminate;
    let config = Config::new(
        ClientMode::Local,
        &config.client_key,
        &config.relay_server,
        443,
        &config.encryption_key,
        false
    );


    let mut client = Client::new(config, host_shell);
    let terminate = JsFuture::from(terminate);

    tokio::select! {
        res = client.start_session() => if let Err(err) = res {
            client.println(&format!("\r\nError occurred during session: {:?}", err)).await;
        },
        _ = terminate => info!("terminating client...")
    }
}
