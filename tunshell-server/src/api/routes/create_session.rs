use crate::db::{Participant, Session, SessionStore};
use log::*;
use serde::Serialize;
use uuid::Uuid;
use warp::{http::response::Builder, hyper::Body, Rejection, Reply};

#[derive(Serialize)]
struct ResponsePayload<'a> {
    host_key: &'a str,
    client_key: &'a str,
}

pub(crate) async fn create_session(db: mongodb::Client) -> Result<Box<dyn Reply>, Rejection> {
    let mut store = SessionStore::new(db);

    debug!("creating new session");
    let session = Session::new(
        Participant::waiting(Uuid::new_v4().to_string()),
        Participant::waiting(Uuid::new_v4().to_string()),
    );

    if let Err(err) = store.save(&session).await {
        error!("error while saving session: {}", err);

        return Ok(Box::new(
            Builder::new()
                .status(500)
                .body(Body::from("error occurred while saving session"))
                .unwrap(),
        ));
    }

    Ok(Box::new(warp::reply::json(&ResponsePayload {
        host_key: &session.host.key,
        client_key: &session.client.key,
    })))
}
