use super::config::Config;
use crate::db::SessionStore;
use anyhow::{Error, Result};
use log::*;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::{net::TcpStream, task::JoinHandle};
use tokio_rustls::server::TlsStream;
use tunshell_shared::{ClientMessage, KeyAcceptedPayload, ServerMessage};

mod connection;
mod message_stream;
mod relay;
mod session_validation;
mod tls;

pub(self) use connection::*;
pub(self) use message_stream::*;
pub(self) use relay::*;
pub(self) use session_validation::*;
pub(self) use tls::*;

// #[cfg(all(test, integration))]
#[cfg(test)]
mod tests;

pub(super) struct Server {
    config: Config,
    sessions: SessionStore,
    connections: Connections,
}

impl Server {
    pub(super) fn new(config: Config, sessions: SessionStore) -> Self {
        Self {
            config,
            sessions,
            connections: Connections::new(),
        }
    }

    pub(super) async fn start(&mut self, terminate_rx: Option<mpsc::Receiver<()>>) -> Result<()> {
        let mut tls_listener = TlsListener::bind(&self.config).await?;

        // If the terminate channel is not supplied we create a default channel
        // that is never invoked
        let (_tx, mut terminate_rx) = match terminate_rx {
            Some(rx) => (None, rx),
            None => {
                let (tx, rx) = mpsc::channel(1);
                (Some(tx), rx)
            }
        };

        let mut next_clean_at =
            tokio::time::Instant::now() + self.config.expired_connection_clean_interval;

        loop {
            tokio::select! {
                stream = tls_listener.accept() => { self.handle_new_connection(stream); },
                accepted = &mut self.connections.new => { self.handle_accepted_connection(accepted); },
                closed = &mut self.connections.waiting => { self.handle_closed_waiting_connection(closed); },
                finished = &mut self.connections.paired => { self.handle_finished_connection(finished); }
                _ = tokio::time::delay_until(next_clean_at) => {
                    self.clean_expired_connections();
                    next_clean_at += self.config.expired_connection_clean_interval;
                }
                _ = terminate_rx.recv() => break
            }
        }

        Ok(())
    }

    fn handle_new_connection(&mut self, stream: Result<TlsStream<TcpStream>>) {
        if let Err(err) = stream {
            warn!("error while establishing connection: {:?}", err);
            return;
        }

        self.connections
            .new
            .0
            .push(self.negotiate_key(stream.unwrap()));
    }

    fn negotiate_key(
        &self,
        stream: TlsStream<TcpStream>,
    ) -> JoinHandle<Result<AcceptedConnection>> {
        debug!("negotiating key");
        let mut sessions = self.sessions.clone();
        let key_timeout = self.config.client_key_timeout;

        tokio::spawn(async move {
            let remote_addr = stream.get_ref().0.peer_addr()?;
            let mut connection = ClientMessageStream::new(stream);

            let key = connection.wait_for_key(key_timeout).await;

            if let Err(err) = key {
                warn!("error while receiving key: {}", err);
                return Err(err);
            }

            let key = key.unwrap().key;
            let session = sessions.find_by_key(key.as_ref()).await?;

            if session.is_none() {
                debug!("key rejected, could not find session");
                connection.write(ServerMessage::KeyRejected).await?;
                return Err(Error::msg("client did not supply valid key"));
            }

            let session = session.unwrap();

            if !is_session_valid_to_join(&session, key.as_ref()) {
                debug!("key rejected, session is not valid to join");
                connection.write(ServerMessage::KeyRejected).await?;
                return Err(Error::msg("session is not valid to join"));
            }

            connection
                .write(ServerMessage::KeyAccepted(KeyAcceptedPayload {
                    key_type: session.key_type(key.as_ref()).unwrap(),
                }))
                .await?;

            debug!("key accepted");
            let connection = Connection {
                stream: connection,
                key,
                connected_at: Instant::now(),
                remote_addr,
            };

            Ok(AcceptedConnection {
                con: connection,
                session,
            })
        })
    }

    fn handle_accepted_connection(&mut self, accepted: Result<AcceptedConnection>) {
        let mut accepted = match accepted {
            Ok(accepted) => accepted,
            Err(err) => {
                error!("error while accepting connection: {}", err);
                return;
            }
        };

        // Ensure no race condition where connection can be joined twice
        if self.connections.waiting.0.contains_key(&accepted.con.key) {
            warn!("connection was joined twice");
            tokio::spawn(async move {
                accepted
                    .con
                    .stream
                    .write(ServerMessage::AlreadyJoined)
                    .await
            });
            return;
        }

        let peer = accepted
            .session
            .other_participant(&accepted.con.key)
            .unwrap();

        if self.connections.waiting.0.contains_key(&peer.key) {
            // Peer is waiting, we can pair the connections
            let peer = self.connections.waiting.0.remove(&peer.key).unwrap();
            self.connections.paired.0.push(pair_connections(
                accepted.con,
                peer,
                self.config.paired_connection_expiry,
            ));
        } else {
            // Put connection into hash map, waiting for peer to join
            self.connections
                .waiting
                .0
                .insert(accepted.con.key.clone(), accepted.con);
        }
    }

    fn handle_closed_waiting_connection(&self, closed: (Connection, Result<ClientMessage>)) {
        match closed {
            (_, Ok(msg)) => error!(
                "received unexpected message from client while waiting for peer: {:?}",
                msg
            ),
            (_, Err(err)) => error!(
                "connection error from client while waiting for peer: {:?}",
                err
            ),
        }
    }

    fn handle_finished_connection(&self, paired: Result<(Connection, Connection)>) {
        if let Err(err) = paired {
            error!("error occurred during paired connection: {}", err);
        }
    }

    fn clean_expired_connections(&mut self) {
        debug!("cleaning expired connections");

        let expiry = self.config.waiting_connection_expiry;
        self.connections
            .waiting
            .0
            .retain(|_, con| Instant::now() - con.connected_at < expiry);

        let expiry = self.config.paired_connection_expiry;
        self.connections
            .paired
            .0
            .retain(|con| Instant::now() - con.paired_at < expiry);
    }
}
