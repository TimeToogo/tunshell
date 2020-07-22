use super::*;
use crate::db;
use crate::db::SessionStore;
use futures::StreamExt;
use std::time::Duration;
use tokio::{
    runtime::Runtime,
    time::{delay_for, timeout},
};
use tokio_util::compat::*;
use tunshell_shared::{ClientMessage, KeyType, PeerJoinedPayload, RelayPayload, ServerMessage};

mod utils;

pub(crate) use utils::*;

#[test]
fn test_init_server() {
    env_logger::init();
    Runtime::new().unwrap().block_on(async {
        let server = init_server(Config::from_env().unwrap()).await;

        delay_for(Duration::from_millis(100)).await;

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 0);
        assert_eq!(server.connections.paired.0.len(), 0);
    });
}

#[test]
fn test_connect_to_server_without_sending_key_timeout() {
    Runtime::new().unwrap().block_on(async {
        let mut config = Config::from_env().unwrap();
        config.client_key_timeout = Duration::from_millis(100);

        let server = init_server(config).await;
        let mut con = create_client_connection_to_server(&server).await;

        // Wait for timeout
        delay_for(Duration::from_millis(200)).await;

        let message = con.next().await.unwrap().unwrap();

        assert_eq!(message, ServerMessage::Close);

        delay_for(Duration::from_millis(10)).await;

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 0);
        assert_eq!(server.connections.paired.0.len(), 0);
    });
}

#[test]
fn test_connect_with_valid_host_key() {
    Runtime::new().unwrap().block_on(async {
        let server = init_server(Config::from_env().unwrap()).await;
        let mut con = create_client_connection_to_server(&server).await;

        let mock_session = create_mock_session().await;

        send_key_to_server(&mut con, &mock_session.host.key).await;

        assert_next_message_is_key_accepted(&mut con, KeyType::Host).await;

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 1);
        assert_eq!(
            server
                .connections
                .waiting
                .0
                .contains_key(&mock_session.host.key),
            true
        );
        assert_eq!(server.connections.paired.0.len(), 0);
    });
}

#[test]
fn test_close_connection_while_waiting_for_peer() {
    Runtime::new().unwrap().block_on(async {
        let server = init_server(Config::from_env().unwrap()).await;
        {
            let mut con = create_client_connection_to_server(&server).await;

            let mock_session = create_mock_session().await;

            send_key_to_server(&mut con, &mock_session.host.key).await;

            assert_next_message_is_key_accepted(&mut con, KeyType::Host).await;

            // Connection should drop here
        }

        delay_for(Duration::from_millis(100)).await;

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        // Connection should be removed from waiting hash map after being closed
        assert_eq!(server.connections.waiting.0.len(), 0);
        assert_eq!(server.connections.paired.0.len(), 0);
    });
}

#[test]
fn test_connect_with_valid_client_key() {
    Runtime::new().unwrap().block_on(async {
        let server = init_server(Config::from_env().unwrap()).await;
        let mut con = create_client_connection_to_server(&server).await;

        let mock_session = create_mock_session().await;

        send_key_to_server(&mut con, &mock_session.client.key).await;

        assert_next_message_is_key_accepted(&mut con, KeyType::Client).await;

        delay_for(Duration::from_millis(50)).await;

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 1);
        assert_eq!(
            server
                .connections
                .waiting
                .0
                .contains_key(&mock_session.client.key),
            true
        );
        assert_eq!(server.connections.paired.0.len(), 0);
    });
}

#[test]
fn test_connect_with_invalid_key() {
    Runtime::new().unwrap().block_on(async {
        let server = init_server(Config::from_env().unwrap()).await;
        let mut con = create_client_connection_to_server(&server).await;

        send_key_to_server(&mut con, "some-invalid-key").await;

        let response = con.next().await.unwrap().unwrap();

        assert_eq!(response, ServerMessage::KeyRejected);

        delay_for(Duration::from_millis(10)).await;

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 0);
        assert_eq!(server.connections.paired.0.len(), 0);
    });
}

#[test]
fn test_connect_and_joined_twice_with_same_key() {
    Runtime::new().unwrap().block_on(async {
        let server = init_server(Config::from_env().unwrap()).await;
        let mut con1 = create_client_connection_to_server(&server).await;
        let mut con2 = create_client_connection_to_server(&server).await;

        let mock_session = create_mock_session().await;

        send_key_to_server(&mut con1, &mock_session.client.key).await;
        assert_next_message_is_key_accepted(&mut con1, KeyType::Client).await;

        send_key_to_server(&mut con2, &mock_session.client.key).await;
        assert_next_message_is_key_accepted(&mut con2, KeyType::Client).await;

        assert_eq!(
            con2.next().await.unwrap().unwrap(),
            ServerMessage::AlreadyJoined
        );
        assert_eq!(con2.next().await.unwrap().unwrap(), ServerMessage::Close);

        delay_for(Duration::from_millis(50)).await;

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 1);
        assert_eq!(server.connections.paired.0.len(), 0);
    });
}

#[test]
fn test_connect_with_to_expired_session() {
    Runtime::new().unwrap().block_on(async {
        let server = init_server(Config::from_env().unwrap()).await;
        let mut con = create_client_connection_to_server(&server).await;

        let mut mock_session = create_mock_session().await;
        mock_session.created_at = chrono::Utc::now() - chrono::Duration::days(1);
        SessionStore::new(db::connect().await.unwrap())
            .save(&mock_session)
            .await
            .unwrap();

        send_key_to_server(&mut con, &mock_session.host.key).await;

        let response = con.next().await.unwrap().unwrap();

        assert_eq!(response, ServerMessage::KeyRejected);

        delay_for(Duration::from_millis(10)).await;

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 0);
        assert_eq!(server.connections.paired.0.len(), 0);
    });
}

#[test]
fn test_clean_expired_waiting_connections() {
    Runtime::new().unwrap().block_on(async {
        let mut config = Config::from_env().unwrap();
        config.expired_connection_clean_interval = Duration::from_millis(200);
        config.waiting_connection_expiry = Duration::from_millis(400);
        let server = init_server(config).await;
        let mut con = create_client_connection_to_server(&server).await;

        let mock_session = create_mock_session().await;

        send_key_to_server(&mut con, &mock_session.client.key).await;

        assert_next_message_is_key_accepted(&mut con, KeyType::Client).await;

        // Wait for connection to be cleaned up by gc timeout
        delay_for(Duration::from_millis(1000)).await;

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 0);
        assert_eq!(server.connections.paired.0.len(), 0);

        let response = con.next().await.unwrap().unwrap();

        assert_eq!(response, ServerMessage::Close);

        assert!(con.next().await.is_none());
    });
}

#[test]
fn test_paired_connection() {
    Runtime::new().unwrap().block_on(async {
        let config = Config::from_env().unwrap();
        let server = init_server(config).await;

        let mut con_host = create_client_connection_to_server(&server).await;
        let mut con_client = create_client_connection_to_server(&server).await;

        let mock_session = create_mock_session().await;

        send_key_to_server(&mut con_host, &mock_session.host.key).await;
        assert_next_message_is_key_accepted(&mut con_host, KeyType::Host).await;

        send_key_to_server(&mut con_client, &mock_session.client.key).await;
        assert_next_message_is_key_accepted(&mut con_client, KeyType::Client).await;

        delay_for(Duration::from_millis(10)).await;
        
        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 0);
        assert_eq!(server.connections.paired.0.len(), 1);
    });
}

#[test]
fn test_direct_connection() {
    Runtime::new().unwrap().block_on(async {
        let config = Config::from_env().unwrap();
        let server = init_server(config).await;

        let mut con_host = create_client_connection_to_server(&server).await;
        let mut con_client = create_client_connection_to_server(&server).await;

        let mock_session = create_mock_session().await;

        send_key_to_server(&mut con_host, &mock_session.host.key).await;
        assert_next_message_is_key_accepted(&mut con_host, KeyType::Host).await;

        send_key_to_server(&mut con_client, &mock_session.client.key).await;
        assert_next_message_is_key_accepted(&mut con_client, KeyType::Client).await;

        assert_eq!(
            con_host.next().await.unwrap().unwrap(),
            ServerMessage::PeerJoined(PeerJoinedPayload {
                peer_key: mock_session.client.key.to_owned(),
                peer_ip_address: "127.0.0.1".to_owned()
            })
        );

        assert_eq!(
            con_client.next().await.unwrap().unwrap(),
            ServerMessage::PeerJoined(PeerJoinedPayload {
                peer_key: mock_session.host.key.to_owned(),
                peer_ip_address: "127.0.0.1".to_owned()
            })
        );

        let message_host = con_host.next().await.unwrap().unwrap();
        let message_client = con_client.next().await.unwrap().unwrap();

        match (message_host, message_client) {
            (ServerMessage::AttemptDirectConnect(_), ServerMessage::AttemptDirectConnect(_)) => {}
            msgs @ _ => panic!(
                "expected to receive attempt direct connect messages but received: {:?}",
                msgs
            ),
        }

        con_host
            .write(&ClientMessage::DirectConnectSucceeded)
            .await
            .unwrap();
        con_client
            .write(&ClientMessage::DirectConnectSucceeded)
            .await
            .unwrap();

        timeout(
            Duration::from_millis(100),
            futures::future::select(con_host.next(), con_client.next()),
        )
        .await
        .err()
        .expect("after direct connection succeeds no message should be sent by the server");

        con_host.write(&ClientMessage::Close).await.unwrap();
        con_client.write(&ClientMessage::Close).await.unwrap();

        // Wait for socket to be closed
        delay_for(Duration::from_millis(1100)).await;

        con_host
            .next()
            .await
            .map(|i| i.expect_err("socket should be closed after close message"));
        con_client
            .next()
            .await
            .map(|i| i.expect_err("socket should be closed after close message"));

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 0);
        assert_eq!(server.connections.paired.0.len(), 0);
    });
}

#[test]
fn test_relayed_connection() {
    Runtime::new().unwrap().block_on(async {
        let config = Config::from_env().unwrap();
        let server = init_server(config).await;

        let mut con_host = create_client_connection_to_server(&server).await;
        let mut con_client = create_client_connection_to_server(&server).await;

        let mock_session = create_mock_session().await;

        send_key_to_server(&mut con_host, &mock_session.host.key).await;
        assert_next_message_is_key_accepted(&mut con_host, KeyType::Host).await;

        send_key_to_server(&mut con_client, &mock_session.client.key).await;
        assert_next_message_is_key_accepted(&mut con_client, KeyType::Client).await;

        assert_eq!(
            con_host.next().await.unwrap().unwrap(),
            ServerMessage::PeerJoined(PeerJoinedPayload {
                peer_key: mock_session.client.key.to_owned(),
                peer_ip_address: "127.0.0.1".to_owned()
            })
        );

        assert_eq!(
            con_client.next().await.unwrap().unwrap(),
            ServerMessage::PeerJoined(PeerJoinedPayload {
                peer_key: mock_session.host.key.to_owned(),
                peer_ip_address: "127.0.0.1".to_owned()
            })
        );

        let message_host = con_host.next().await.unwrap().unwrap();
        let message_client = con_client.next().await.unwrap().unwrap();

        match (message_host, message_client) {
            (ServerMessage::AttemptDirectConnect(_), ServerMessage::AttemptDirectConnect(_)) => {}
            msgs @ _ => panic!(
                "expected to receive attempt direct connect messages but received: {:?}",
                msgs
            ),
        }

        con_host
            .write(&ClientMessage::DirectConnectFailed)
            .await
            .unwrap();
        con_client
            .write(&ClientMessage::DirectConnectFailed)
            .await
            .unwrap();

        assert_eq!(
            con_host.next().await.unwrap().unwrap(),
            ServerMessage::StartRelayMode
        );
        assert_eq!(
            con_client.next().await.unwrap().unwrap(),
            ServerMessage::StartRelayMode
        );

        con_host
            .write(&ClientMessage::Relay(RelayPayload {
                data: "hello from host".as_bytes().to_vec(),
            }))
            .await
            .unwrap();
        con_client
            .write(&ClientMessage::Relay(RelayPayload {
                data: "hello from client".as_bytes().to_vec(),
            }))
            .await
            .unwrap();

        assert_eq!(
            con_host.next().await.unwrap().unwrap(),
            ServerMessage::Relay(RelayPayload {
                data: "hello from client".as_bytes().to_vec(),
            })
        );
        assert_eq!(
            con_client.next().await.unwrap().unwrap(),
            ServerMessage::Relay(RelayPayload {
                data: "hello from host".as_bytes().to_vec(),
            })
        );

        con_host.write(&ClientMessage::Close).await.unwrap();
        con_client.write(&ClientMessage::Close).await.unwrap();

        // Wait for socket to be closed
        delay_for(Duration::from_millis(1100)).await;
        
        con_host
            .next()
            .await
            .map(|i| i.expect_err("socket should be closed after close message"));
        con_client
            .next()
            .await
            .map(|i| i.expect_err("socket should be closed after close message"));

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 0);
        assert_eq!(server.connections.paired.0.len(), 0);
    });
}

#[test]
fn test_clean_up_paired_connection() {
    Runtime::new().unwrap().block_on(async {
        let mut config = Config::from_env().unwrap();
        config.expired_connection_clean_interval = Duration::from_millis(100);
        config.paired_connection_expiry = Duration::from_millis(100);
        let server = init_server(config).await;

        let mut con_host = create_client_connection_to_server(&server).await;
        let mut con_client = create_client_connection_to_server(&server).await;

        let mock_session = create_mock_session().await;

        send_key_to_server(&mut con_host, &mock_session.host.key).await;
        assert_next_message_is_key_accepted(&mut con_host, KeyType::Host).await;

        send_key_to_server(&mut con_client, &mock_session.client.key).await;
        assert_next_message_is_key_accepted(&mut con_client, KeyType::Client).await;

        assert_eq!(
            con_host.next().await.unwrap().unwrap(),
            ServerMessage::PeerJoined(PeerJoinedPayload {
                peer_key: mock_session.client.key.to_owned(),
                peer_ip_address: "127.0.0.1".to_owned()
            })
        );

        assert_eq!(
            con_client.next().await.unwrap().unwrap(),
            ServerMessage::PeerJoined(PeerJoinedPayload {
                peer_key: mock_session.host.key.to_owned(),
                peer_ip_address: "127.0.0.1".to_owned()
            })
        );

        let message_host = con_host.next().await.unwrap().unwrap();
        let message_client = con_client.next().await.unwrap().unwrap();

        match (message_host, message_client) {
            (ServerMessage::AttemptDirectConnect(_), ServerMessage::AttemptDirectConnect(_)) => {}
            msgs @ _ => panic!(
                "expected to receive attempt direct connect messages but received: {:?}",
                msgs
            ),
        }

        con_host
            .write(&ClientMessage::DirectConnectSucceeded)
            .await
            .unwrap();
        con_client
            .write(&ClientMessage::DirectConnectSucceeded)
            .await
            .unwrap();

        // Wait for connection to expire
        delay_for(Duration::from_millis(500)).await;

        assert_eq!(
            con_host.next().await.unwrap().unwrap(),
            ServerMessage::Close
        );
        assert_eq!(
            con_client.next().await.unwrap().unwrap(),
            ServerMessage::Close
        );

        let server = server.stop().await.unwrap();

        assert_eq!(server.connections.new.0.len(), 0);
        assert_eq!(server.connections.waiting.0.len(), 0);
        assert_eq!(server.connections.paired.0.len(), 0);
    });
}
