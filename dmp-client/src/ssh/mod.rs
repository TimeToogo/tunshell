mod client;
mod server;

pub struct SshCredentials {
    username: String,
    password: String,
}

impl SshCredentials {
    pub fn new(username: &str, password: &str) -> Self {
        Self {
            username: username.to_owned(),
            password: password.to_owned(),
        }
    }
}

pub use client::*;
pub use server::*;
