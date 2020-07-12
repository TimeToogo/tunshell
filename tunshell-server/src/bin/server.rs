use log::*;
use tokio;
use tunshell_server::{api, relay};
use env_logger;

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("starting tunshell server");

    let result = tokio::select! {
        result = api::start() => {
            info!("api server stopped");
            result
        },
        result = relay::start() => {
            info!("tls relay stopped");
            result
        }
    };

    if let Err(err) = result {
        error!("error occurred: {}", err);
    }

    info!("tunshell server exiting");
}
