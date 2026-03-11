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

// Older getrandom releases call open64 on Linux. Musl dropped that exported
// symbol, so provide a minimal compatibility entrypoint for musl targets.
#[cfg(all(target_os = "linux", target_env = "musl"))]
#[no_mangle]
pub unsafe extern "C" fn open64(path: *const libc::c_char, oflag: libc::c_int) -> libc::c_int {
    libc::open(path, oflag)
}
