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

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        #[global_allocator]
        static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

        mod wasm;
        pub use wasm::*;
    }
}