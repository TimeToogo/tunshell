use reqwest;
use std::collections::HashMap;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::delay_for;
use tunshell_client as client;
use tunshell_server as server;

#[test]
fn test_valid_connection() {
    env_logger::init();
    Runtime::new().unwrap().block_on(async {
        let mut config = server::relay::Config::from_env().unwrap();
        config.tls_port = 20001;
        config.api_port = 20002;

        tokio::task::spawn(server::start(config));

        delay_for(Duration::from_millis(1000)).await;

        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let response = http
            .post("https://localhost:20002/api/sessions")
            .send()
            .await
            .unwrap()
            .json::<HashMap<String, String>>()
            .await
            .unwrap();

        let target_key = response.get("peer1_key").unwrap();
        let client_key = response.get("peer2_key").unwrap();

        let mut target_config = client::Config::new(
            client::ClientMode::Target,
            target_key,
            "localhost.tunshell.com",
            20001,
            "mock_encryption_key",
        );
        target_config.set_dangerous_disable_relay_server_verification(true);
        let mut target = client::Client::new(target_config, client::HostShell::new().unwrap());

        let mut client_config = client::Config::new(
            client::ClientMode::Target,
            client_key,
            "localhost.tunshell.com",
            20001,
            "mock_encryption_key",
        );
        client_config.set_dangerous_disable_relay_server_verification(true);
        let mut client = client::Client::new(client_config, client::HostShell::new().unwrap());

        let result = tokio::try_join!(
            tokio::task::spawn_blocking(move || futures::executor::block_on(async move {
                client.start_session().await
            })),
            tokio::task::spawn_blocking(move || futures::executor::block_on(async move {
                target.start_session().await
            }))
        );

        let (res1, res2) = result.unwrap();
        assert_eq!(res1.unwrap(), 0);
        assert_eq!(res2.unwrap(), 0);
    })
}
