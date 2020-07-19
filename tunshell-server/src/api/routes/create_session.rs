use crate::db::{Participant, Session, SessionStore};
use log::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use warp::{http::Response, hyper::Body, Rejection, Reply};

#[derive(Serialize, Deserialize, Debug)]
struct ResponsePayload<'a> {
    host_key: &'a str,
    client_key: &'a str,
}

pub(crate) async fn create_session(db: mongodb::Client) -> Result<Box<dyn Reply>, Rejection> {
    let mut store = SessionStore::new(db);

    debug!("creating new session");
    let session = Session::new(
        Participant::new(Uuid::new_v4().to_string()),
        Participant::new(Uuid::new_v4().to_string()),
    );

    let result = store.save(&session).await;

    if let Err(err) = result {
        error!("error while saving session: {}", err);

        return Ok(Box::new(
            Response::builder()
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

#[cfg(all(test, integration))]
mod tests {
    use super::*;
    use crate::db;
    use futures::TryStreamExt;
    use serde_json;
    use tokio::runtime::Runtime;

    #[test]
    fn test_create_session() {
        Runtime::new().unwrap().block_on(async {
            let client = db::connect().await.unwrap();

            let session = create_session(client).await.unwrap();

            let body = session
                .into_response()
                .into_body()
                .try_fold(Vec::new(), |mut data, chunk| async move {
                    data.extend_from_slice(&chunk);
                    Ok(data)
                })
                .await
                .unwrap();

            let response = serde_json::from_slice::<ResponsePayload<'_>>(body.as_slice()).unwrap();

            assert_ne!(response.host_key, "");
            assert_ne!(response.client_key, "");

            debug!("response: {:?}", response);
        });
    }
}
