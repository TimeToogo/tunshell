use super::{config::Config, routes::create_session};
use crate::db;
use anyhow::Result;
use log::*;
use std::sync::Arc;
use warp::Filter;

pub async fn start() -> Result<()> {
    let config = Config::from_env()?;
    info!("starting api server on port {}", config.port);

    let db_client = Arc::new(db::connect().await?);

    let router = warp::any().and({
        warp::path("api").and(
            // POST /api/sessions
            warp::path("sessions")
                .and(warp::post())
                .and_then(move || create_session(db_client.as_ref().clone())),
        )
    });

    warp::serve(router).run(([0, 0, 0, 0], config.port)).await;

    Ok(())
}
