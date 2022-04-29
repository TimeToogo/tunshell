mod client;

pub use client::*;

mod config;
pub use config::*;

mod server;
pub use server::*;

#[cfg(not(target_arch = "wasm32"))]
mod p2p;

mod shell;
pub use shell::*;

mod stream;
pub use stream::*;

pub mod util;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        mod wasm;
        pub use wasm::*;
    }
}

cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        use std::sync::atomic::AtomicBool;
        pub static STOP_ON_SIGINT: AtomicBool = AtomicBool::new(true);
    }
}
