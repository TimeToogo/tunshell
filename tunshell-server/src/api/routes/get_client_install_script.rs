use log::*;
use std::env;
use tokio::fs;
use warp::{http::Response, hyper::Body, reply, Rejection, Reply};

enum ScriptType {
    Unix,
    Windows,
}

impl ScriptType {
    fn from_ext(ext: &str) -> Option<Self> {
        match ext {
            "sh" => Some(Self::Unix),
            "cmd" => Some(Self::Windows),
            _ => None,
        }
    }
}

pub(crate) async fn get_client_install_script(
    file_name: String,
) -> Result<Box<dyn Reply>, Rejection> {
    if !file_name.contains('.') {
        return Ok(bad_file_name());
    }

    let mut parts = file_name.split('.');
    let key = parts.next().unwrap();
    let script_type = ScriptType::from_ext(parts.next().unwrap());

    if script_type.is_none() {
        return Ok(bad_file_name());
    }

    let script = match script_type.unwrap() {
        ScriptType::Unix => {
            fs::read_to_string(env::var("STATIC_DIR").unwrap() + "/init-client.sh").await
        }
        ScriptType::Windows => {
            fs::read_to_string(env::var("STATIC_DIR").unwrap() + "/init-client.cmd").await
        }
    };

    if let Err(err) = script {
        error!("error while reading script: {}", err);
        return Ok(Box::new(reply::with_status(
            reply::reply(),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        )));
    }

    let script = script.unwrap().replace("__KEY__", key);

    Ok(Box::new(
        Response::builder()
            .status(200)
            .header("content-type", "application/octet-stream")
            .body(Body::from(script))
            .unwrap(),
    ))
}

fn bad_file_name() -> Box<dyn Reply> {
    Box::new(reply::with_status(
        reply::json(&serde_json::json!({"error": "invalid file name"})),
        warp::http::StatusCode::BAD_REQUEST,
    ))
}

#[cfg(all(test, integration))]
mod tests {
    use super::*;
    use futures::TryStreamExt;
    use tokio::runtime::Runtime;

    #[test]
    fn test_get_client_install_script_unix() {
        Runtime::new().unwrap().block_on(async {
            let response = get_client_install_script("test-key.sh".to_owned())
                .await
                .unwrap();

            let body = response
                .into_response()
                .into_body()
                .try_fold(Vec::new(), |mut data, chunk| async move {
                    data.extend_from_slice(&chunk);
                    Ok(data)
                })
                .await
                .unwrap();

            let response = String::from_utf8(body).unwrap();

            assert!(response.contains("=== TUNSHELL SHELL SCRIPT ==="));
            assert!(response.contains("test-key"));
        });
    }

    #[test]
    fn test_get_client_install_script_windows() {
        Runtime::new().unwrap().block_on(async {
            let response = get_client_install_script("test-key.cmd".to_owned())
                .await
                .unwrap();

            let body = response
                .into_response()
                .into_body()
                .try_fold(Vec::new(), |mut data, chunk| async move {
                    data.extend_from_slice(&chunk);
                    Ok(data)
                })
                .await
                .unwrap();

            let response = String::from_utf8(body).unwrap();

            assert!(response.contains("=== TUNSHELL CMD SCRIPT ==="));
            assert!(response.contains("test-key"));
        });
    }

    #[test]
    fn test_get_client_install_script_invalid() {
        Runtime::new().unwrap().block_on(async {
            let response = get_client_install_script("some-invalid-name".to_owned())
                .await
                .unwrap();

            assert_eq!(response.into_response().status(), 400);
        });
    }
}
