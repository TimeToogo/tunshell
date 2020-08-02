use super::{config::Config, cors::cors, routes};
use crate::db;
use anyhow::Result;
use log::*;
use std::sync::Arc;
use warp::{filters::BoxedFilter, Filter, Reply};

pub async fn register() -> Result<BoxedFilter<(impl Reply + 'static,)>> {
    let config = Config::from_env()?;
    info!("starting api server on port {}", config.port);

    let db_client = Arc::new(db::connect().await?);

    let routes = warp::any()
        .and({
            warp::path("api").and(
                // POST /api/sessions
                warp::path("sessions")
                    .and(warp::post())
                    .and_then(move || routes::create_session(db_client.as_ref().clone())),
            )
        })
        .with(cors());

    Ok(routes.boxed())
}
