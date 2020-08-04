use super::{cors::cors, routes};
use crate::db;
use anyhow::Result;
use log::*;
use std::sync::Arc;
use warp::{filters::BoxedFilter, Filter, Reply};

pub async fn register() -> Result<BoxedFilter<(impl Reply + 'static,)>> {
    info!("registering api server routes");

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
