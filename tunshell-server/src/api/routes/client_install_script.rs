use log::*;
use std::{env, path::Path};
use tokio::fs;
use warp::{http::Response, hyper::Body, reply, Rejection, Reply};

pub(crate) async fn client_install_script(file_name: String) -> Result<Box<dyn Reply>, Rejection> {
    if !file_name.contains('.') {
        return Ok(bad_file_name());
    }

    let mut parts = file_name.split('.');
    let key = parts.next().unwrap();
    let ext = parts.next().unwrap();

    let script_path =
        env::var("STATIC_DIR").expect("STATIC_DIR env var not present") + "/init-client." + ext;
    let script_path = Path::new::<str>(script_path.as_ref());

    if !script_path.is_file() {
        return Ok(bad_file_name());
    }

    let script = fs::read_to_string(script_path).await;

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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::TryStreamExt;
    use tokio::runtime::Runtime;

    async fn mock_request(file_name: &str) -> String {
        let response = client_install_script(file_name.to_owned()).await.unwrap();

        let body = response
            .into_response()
            .into_body()
            .try_fold(Vec::new(), |mut data, chunk| async move {
                data.extend_from_slice(&chunk);
                Ok(data)
            })
            .await
            .unwrap();

        let body = String::from_utf8(body).unwrap();

        body
    }

    #[test]
    fn test_client_install_script_sh() {
        Runtime::new().unwrap().block_on(async {
            let response = mock_request("test-key.sh").await;

            assert!(response.contains("=== TUNSHELL SHELL SCRIPT ==="));
            assert!(response.contains("test-key"));
        });
    }

    #[test]
    fn test_client_install_script_cmd() {
        Runtime::new().unwrap().block_on(async {
            let response = mock_request("test-key.cmd").await;

            assert!(response.contains("=== TUNSHELL CMD SCRIPT ==="));
            assert!(response.contains("test-key"));
        });
    }


    #[test]
    fn test_client_install_script_ps1() {
        Runtime::new().unwrap().block_on(async {
            let response = mock_request("test-key.ps1").await;

            assert!(response.contains("=== TUNSHELL PS SCRIPT ==="));
            assert!(response.contains("test-key"));
        });
    }

    #[test]
    fn test_client_install_script_node() {
        Runtime::new().unwrap().block_on(async {
            let response = mock_request("test-key.js").await;

            assert!(response.contains("=== TUNSHELL NODE SCRIPT ==="));
            assert!(response.contains("test-key"));
        });
    }

    #[test]
    fn test_client_install_script_invalid() {
        Runtime::new().unwrap().block_on(async {
            let response = client_install_script("some-invalid-name".to_owned())
                .await
                .unwrap();

            assert_eq!(response.into_response().status(), 400);
        });
    }
}
