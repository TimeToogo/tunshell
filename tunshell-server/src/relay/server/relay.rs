use super::{Connection, PairedConnection};
use anyhow::{Context as AnyhowContext, Error, Result};
use futures::FutureExt;
use log::*;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tunshell_shared::{ClientMessage, PeerJoinedPayload, PortBindings, ServerMessage};

pub(super) fn pair_connections(
    mut con1: Connection,
    mut con2: Connection,
    timeout_dur: Duration,
) -> PairedConnection {
    debug!("pairing connections");

    let session_nonce = generate_secure_nonce();

    let task = async move {
        tokio::try_join!(
            con1.stream
                .write(ServerMessage::PeerJoined(PeerJoinedPayload {
                    peer_ip_address: con2.remote_addr.ip().to_string(),
                    peer_key: con2.key.clone(),
                    session_nonce: session_nonce.clone()
                })),
            con2.stream
                .write(ServerMessage::PeerJoined(PeerJoinedPayload {
                    peer_ip_address: con1.remote_addr.ip().to_string(),
                    peer_key: con1.key.clone(),
                    session_nonce
                })),
        )
        .context("sending peer joined message")?;

        let direct_connection = attempt_direct_connection(&mut con1, &mut con2).await;

        if let Err(err) = direct_connection {
            warn!("error during attempting direct connection: {}", err);
            return Err(err.context("establishing direct connection"));
        }

        if direct_connection.unwrap() {
            // In the case of a direct connection between the peers the
            // relay server does not have to do much, since the clients
            // will stream between themselves.
            // The next message must indicate the connection is over so
            // we wait until a message is received.
            debug!("direct connection established");

            let message: Result<ClientMessage> = tokio::select! {
                message = con1.stream.next() => message,
                message = con2.stream.next() => message
            };

            match message {
                Ok(ClientMessage::Close) => {}
                Ok(message) => {
                    return Err(Error::msg(format!(
                        "received unexpected message from client during direct connection {:?}",
                        message
                    )))
                }
                Err(err) => {
                    return Err(Error::msg(format!(
                        "error received from client stream {}",
                        err
                    )))
                }
            }
        } else {
            // If the direct connection fails the relay server becomes responsible
            // for proxying data between the two peers
            debug!("starting relay");
            tokio::try_join!(
                con1.stream.write(ServerMessage::StartRelayMode),
                con2.stream.write(ServerMessage::StartRelayMode)
            )
            .context("sending relay mode message")?;

            relay_loop(&mut con1, &mut con2).await?;
        }

        Ok((con1, con2))
    };

    let task = timeout(timeout_dur, task)
        .map(|i| i.unwrap_or_else(|_| Err(Error::msg("direct connection timed out"))));

    let task = tokio::spawn(task);

    PairedConnection {
        task,
        paired_at: Instant::now(),
    }
}

// 22 alphanurmeric chars ~= 131 bits of entropy
fn generate_secure_nonce() -> String {
    thread_rng().sample_iter(&Alphanumeric).take(22).collect()
}

async fn attempt_direct_connection(con1: &mut Connection, con2: &mut Connection) -> Result<bool> {
    tokio::try_join!(
        con1.stream.write(ServerMessage::BindForDirectConnect),
        con2.stream.write(ServerMessage::BindForDirectConnect)
    )
    .context("sending bind for direct connection command")?;

    let (result1, result2) = tokio::try_join!(con1.stream.next(), con2.stream.next())
        .context("waiting for direct connection binding response")?;

    let (ports1, ports2) = match (result1, result2) {
        (ClientMessage::DirectConnectBound(ports1), ClientMessage::DirectConnectBound(ports2)) => {
            (ports1, ports2)
        }
        (ClientMessage::DirectConnectFailed, ClientMessage::DirectConnectBound(_)) => return Ok(false),
        (ClientMessage::DirectConnectBound(_), ClientMessage::DirectConnectFailed) => return Ok(false),
        (ClientMessage::DirectConnectFailed, ClientMessage::DirectConnectFailed) => return Ok(false),
        msgs @ _ => {
            return Err(Error::msg(format!(
                "unexpected message while attempting to bind for direct connection: {:?}",
                msgs
            )))
        }
    };

    let supports_matching_protocols = match (&ports1, &ports2) {
        (
            PortBindings {
                tcp_port: Some(_), ..
            },
            PortBindings {
                tcp_port: Some(_), ..
            },
        ) => true,
        (
            PortBindings {
                udp_port: Some(_), ..
            },
            PortBindings {
                udp_port: Some(_), ..
            },
        ) => true,
        _ => false,
    };

    if !supports_matching_protocols {
        debug!("cannot attempt direct connection due to mismatch of port binding protocols");
        return Ok(false);
    }

    tokio::try_join!(
        con1.stream
            .write(ServerMessage::AttemptDirectConnect(ports2)),
        con2.stream
            .write(ServerMessage::AttemptDirectConnect(ports1))
    )
    .context("sending direct connection command")?;

    let (result1, result2) = tokio::try_join!(con1.stream.next(), con2.stream.next())
        .context("waiting for direct connection response")?;

    let result = match (result1, result2) {
        (ClientMessage::DirectConnectSucceeded, ClientMessage::DirectConnectSucceeded) => true,
        (ClientMessage::DirectConnectFailed, ClientMessage::DirectConnectFailed) => false,
        msgs @ _ => {
            return Err(Error::msg(format!(
                "unexpected message while attempting direct connection: {:?}",
                msgs
            )))
        }
    };

    Ok(result)
}

async fn relay_loop(con1: &mut Connection, con2: &mut Connection) -> Result<()> {
    enum ProxyResult {
        Continue,
        Closed,
    }

    async fn proxy_payload(
        message: Result<ClientMessage>,
        dest: &mut Connection,
    ) -> Result<ProxyResult> {
        let payload = match message {
            Ok(ClientMessage::Relay(payload)) => payload,
            Ok(ClientMessage::Close) => return Ok(ProxyResult::Closed),
            Ok(msg) => {
                return Err(Error::msg(format!(
                    "received unexpected message from client during relay: {:?}",
                    msg
                )))
            }
            Err(err) => return Err(err),
        };

        dest.stream.write(ServerMessage::Relay(payload)).await?;
        Ok(ProxyResult::Continue)
    }

    loop {
        let result = tokio::select! {
            message = con1.stream.next() => proxy_payload(message, con2).await?,
            message = con2.stream.next() => proxy_payload(message, con1).await?,
        };

        if let ProxyResult::Closed = result {
            break;
        }
    }

    Ok(())
}
