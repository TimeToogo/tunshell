use crate::{Config, Client, ClientMode, HostShell};
use wasm_bindgen::prelude::*;
use std::panic;
use log::*;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen] 
pub struct BrowserConfig {
    client_key: String,
    encryption_key: String,
    term: TerminalEmulator
}

#[wasm_bindgen]
impl BrowserConfig {
    #[wasm_bindgen(constructor)]
    pub fn new(client_key: String, encryption_key: String, term: TerminalEmulator) -> Self{
        Self {
            client_key,
            encryption_key,
            term
        }
    }
}


#[wasm_bindgen(typescript_custom_section)]
const TERMINAL_EMULATOR: &'static str = r#"
interface TerminalEmulator {
    onData: (cb: (data: string) => void) => void;
    onResize: (cb: (cols: number, rows: number) => void) => void;
    write: (data: UInt8Array) => void;
    size: () => Uint16Array;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "TerminalEmulator")]
    pub type TerminalEmulator;

    #[wasm_bindgen(method, js_name = onData)]
    pub fn on_data(
        this: &TerminalEmulator,
        listener: &Closure<dyn FnMut(String)>,
    );

    #[wasm_bindgen(method, js_name = onResize)]
    pub fn on_resize(
        this: &TerminalEmulator,
        listener: &Closure<dyn FnMut(u16, u16)>,
    );

    #[wasm_bindgen(method, js_name = write)]
    pub fn write(
        this: &TerminalEmulator,
        data: &[u8],
    );

    #[wasm_bindgen(method, js_name = getSize)]
    pub fn size(
        this: &TerminalEmulator
    ) -> Vec<u16>;
}

#[wasm_bindgen]
pub fn tunshell_init_client(config: BrowserConfig) {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Debug).expect("failed to set log level");

    let host_shell = HostShell::new(config.term).unwrap();
    let config = Config::new(
        ClientMode::Local,
        &config.client_key,
        "relay.tunshell.com",
        5001,
        &config.encryption_key,
    );

    wasm_bindgen_futures::spawn_local(async move {
        let res = Client::new(config, host_shell).start_session().await;

        if let Err(err) = res {
            error!("Error occurred during session: {:?}", err);
        }
    });
}
