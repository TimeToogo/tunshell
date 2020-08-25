use super::{cors::cors, routes};
use crate::db;
use anyhow::Result;
use db::SessionStore;
use log::*;
use warp::{filters::BoxedFilter, Filter, Reply};

pub async fn register() -> Result<BoxedFilter<(impl Reply + 'static,)>> {
    info!("registering api server routes");

    let store = SessionStore::new(db::connect().await?);

    let routes = warp::any()
        .and({
            warp::path("api").and(
                warp::any().and(
                    // POST /api/sessions
                    warp::path("sessions")
                        .and(warp::post())
                        .and_then(move || routes::create_session(store.clone())),
                ).or(
                    // GET /api/info
                    warp::path("info")
                        .and(warp::get())
                        .and_then(move || routes::get_info()),
                )
            )
        })
        .with(cors());

    Ok(routes.boxed())
}
