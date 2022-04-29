mod client;
pub use client::*;

cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        mod server;
        pub(crate) use server::*;
    }
}

mod proto;
use proto::*;

pub struct ShellKey {
    key: String,
}

impl ShellKey {
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

pub(super) fn remote_pty_supported() -> bool {
    cfg!(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))
}
