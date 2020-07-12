use super::config::Config;
use anyhow::Result;
use log::*;

pub async fn start() -> Result<()> {
    let config = Config::from_env()?;
    info!("starting relay server on port {}", config.port);

    futures::future::pending().await
}
