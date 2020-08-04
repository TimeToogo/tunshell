use anyhow::Result;
use log::*;

pub mod api;
pub mod relay;
pub mod db;

pub async fn start(relay_config: relay::Config) -> Result<()> {
    info!("starting tunshell server");

    let routes = match api::register().await {
        Ok(r) => r,
        Err(err) => {
            error!("error while registering api routes: {}", err);
            return Err(err);
        }
    };

    let result = relay::start(relay_config, routes).await;
    info!("tls relay stopped");

    if let Err(err) = result {
        error!("error occurred: {}", err);
        return Err(err);
    }

    info!("tunshell server exiting");
    Ok(())
}
