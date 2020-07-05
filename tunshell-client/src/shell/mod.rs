mod client;
mod proto;
mod server;

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

pub use client::*;
pub use server::*;
