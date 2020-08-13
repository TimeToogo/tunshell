use anyhow::Error;
use env_logger;
use log::error;
use std::process::exit;
use tokio::signal;
use tunshell_client::{Client, Config, HostShell};

#[tokio::main]
async fn main() -> () {
    env_logger::init();

    let config = Config::new_from_env();

    let mut client = Client::new(config, HostShell::new().unwrap());
    let session = client.start_session();

    let result = tokio::select! {
        result = session => result,
        _ = signal::ctrl_c() => Err(Error::msg("interrupt received, terminating")),
    };

    match result {
        Ok(code) => exit(code as i32),
        Err(err) => {
            error!("Error occurred: {:?}", err);
            exit(1)
        }
    }
}
