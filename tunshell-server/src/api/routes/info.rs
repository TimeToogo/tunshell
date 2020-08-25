use log::*;
use serde::{Deserialize, Serialize};
use warp::{reject, Rejection, Reply};
use std::env;

#[derive(Serialize, Deserialize, Debug)]
struct ResponsePayload<'a> {
    domain_name: &'a str,
}

#[derive(Debug)]
struct InternalError;

impl reject::Reject for InternalError {}

pub(crate) async fn get_info() -> Result<Box<dyn Reply>, Rejection> {
    debug!("retrieving server info new session");
    let domain_name = match env::var("DOMAIN_NAME") {
        Ok(domain_name) => domain_name,
        Err(err) => {
            error!("error while retrieving domain env var: {}", err);
            
            return Err(reject::custom(InternalError));
        }
    };

    Ok(Box::new(warp::reply::json(&ResponsePayload {
        domain_name: &domain_name,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::TryStreamExt;
    use serde_json;
    use tokio::runtime::Runtime;

    #[test]
    fn test_get_info() {
        Runtime::new().unwrap().block_on(async {
            let result = get_info().await.unwrap();

            let body = result
                .into_response()
                .into_body()
                .try_fold(Vec::new(), |mut data, chunk| async move {
                    data.extend_from_slice(&chunk);
                    Ok(data)
                })
                .await
                .unwrap();

            let response = serde_json::from_slice::<ResponsePayload<'_>>(body.as_slice()).unwrap();

            assert_eq!(response.domain_name, env::var("DOMAIN_NAME").unwrap());
            debug!("response: {:?}", response);
        });
    }
}
