use super::{config::Config, server::Server};
use crate::db;
use anyhow::Result;
use log::*;
use warp::{filters::BoxedFilter, Reply};

pub async fn start(config: Config, routes: BoxedFilter<(impl Reply + 'static,)>) -> Result<()> {
    let sessions = db::SessionStore::new(db::connect().await?);

    info!(
        "starting relay server on ports (tls: {}, api: {})",
        config.tls_port, config.api_port
    );

    Server::new(config, sessions, routes).start(None).await
}
