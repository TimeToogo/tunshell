use anyhow::Error;
use env_logger;
use log::error;
use std::{process::exit, sync::atomic::Ordering};
use tokio::signal;
use tunshell_client::{Client, Config, HostShell, STOP_ON_SIGINT};

#[tokio::main]
async fn main() -> () {
    env_logger::init();

    let config = Config::new_from_env();

    let mut client = Client::new(config, HostShell::new().unwrap());
    let mut session = tokio::task::spawn(async move { client.start_session().await });

    let result = loop {
        tokio::select! {
            result = &mut session => break result.unwrap_or_else(|_| Err(Error::msg("failed to join task"))),
            _ = signal::ctrl_c() => {
                if STOP_ON_SIGINT.load(Ordering::SeqCst) {
                    break Err(Error::msg("interrupt received, terminating"));
                }
            }
        };
    };

    match result {
        Ok(code) => exit(code as i32),
        Err(err) => {
            error!("Error occurred: {:?}", err);
            exit(1)
        }
    }
}
