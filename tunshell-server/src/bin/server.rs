use anyhow::Result;
use env_logger;
use log::*;
use tokio;
use tunshell_server::{api, relay};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    info!("starting tunshell server");

    let routes = match api::register().await {
        Ok(r) => r,
        Err(err) => {
            error!("error while registering api routes: {}", err);
            return Err(err);
        }
    };

    let result = relay::start(routes).await;
    info!("tls relay stopped");

    if let Err(err) = result {
        error!("error occurred: {}", err);
        return Err(err);
    }

    info!("tunshell server exiting");
    Ok(())
}
