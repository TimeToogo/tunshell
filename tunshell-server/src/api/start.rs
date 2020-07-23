use super::{config::Config, cors::cors, routes};
use crate::db;
use anyhow::Result;
use log::*;
use std::sync::Arc;
use warp::Filter;

pub async fn start() -> Result<()> {
    let config = Config::from_env()?;
    info!("starting api server on port {}", config.port);

    let db_client = Arc::new(db::connect().await?);

    let router = warp::any()
        .and({
            warp::header::exact("host", "lets.tunshell.com").and(
                // GET lets.tunshell.com/{file_name}
                warp::path::param()
                    .and(warp::get())
                    .and_then(move |file_name: String| {
                        routes::client_install_script(file_name)
                    }),
            )
        })
        .or({
            warp::path("api").and(
                // POST /api/sessions
                warp::path("sessions")
                    .and(warp::post())
                    .and_then(move || routes::create_session(db_client.as_ref().clone())),
            )
        })
        .with(cors());

    warp::serve(router).run(([0, 0, 0, 0], config.port)).await;

    Ok(())
}
