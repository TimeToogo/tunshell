use super::ClientMessageStream;
use crate::db::Session;
use anyhow::{Error, Result};
use futures::{Future, FutureExt, StreamExt};
use std::net::SocketAddr;
use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};
use tokio::{net::TcpStream, task::JoinHandle};
use tokio_rustls::server::TlsStream;
use tunshell_shared::ClientMessage;

type ConnectionStream = ClientMessageStream<TlsStream<TcpStream>>;

pub(super) struct Connection {
    pub(super) stream: ConnectionStream,
    pub(super) key: String,
    pub(super) connected_at: Instant,
    pub(super) remote_addr: SocketAddr,
}

pub(super) struct AcceptedConnection {
    pub(super) con: Connection,
    pub(super) session: Session,
}

pub(super) struct PairedConnection {
    pub(super) task: JoinHandle<Result<(Connection, Connection)>>,
    pub(super) paired_at: Instant,
}

pub(super) struct Connections {
    pub(super) new: NewConnections,
    pub(super) waiting: WaitingConnections,
    pub(super) paired: PairedConnections,
}

pub(super) struct NewConnections(pub(super) Vec<JoinHandle<Result<AcceptedConnection>>>);
pub(super) struct WaitingConnections(pub(super) HashMap<String, Connection>);
pub(super) struct PairedConnections(pub(super) Vec<PairedConnection>);

impl Connections {
    pub(super) fn new() -> Self {
        Self {
            new: NewConnections(vec![]),
            waiting: WaitingConnections(HashMap::new()),
            paired: PairedConnections(vec![]),
        }
    }
}

impl Future for NewConnections {
    type Output = Result<AcceptedConnection>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Poll::Ready((idx, result)) = poll_all_remove_ready(self.0.iter_mut().enumerate(), cx)
        {
            self.0.swap_remove(idx);
            return Poll::Ready(result);
        }

        Poll::Pending
    }
}

impl Future for PairedConnections {
    type Output = Result<(Connection, Connection)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let poll = poll_all_remove_ready(
            self.0.iter_mut().enumerate().map(|(k, v)| (k, &mut v.task)),
            cx,
        );

        if let Poll::Ready((idx, result)) = poll {
            self.0.swap_remove(idx);
            return Poll::Ready(result);
        }

        Poll::Pending
    }
}

/// If a connection received a message or closed while it is waiting
/// we remove this connection from the pool
impl Future for WaitingConnections {
    type Output = (Connection, Result<ClientMessage>);

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut received = None;

        for (key, con) in &mut self.0 {
            let result = con.stream.stream_mut().poll_next_unpin(cx);

            if let Poll::Ready(message) = result {
                received = Some((key.to_owned(), message));
                break;
            }
        }

        if let None = received {
            return Poll::Pending;
        }

        let (key, message) = received.unwrap();
        let con = self.0.remove(&key).unwrap();

        Poll::Ready((
            con,
            message.unwrap_or_else(|| Err(Error::msg("connection closed by client"))),
        ))
    }
}

fn poll_all_remove_ready<'a, T>(
    futures: impl Iterator<Item = (usize, &'a mut JoinHandle<Result<T>>)>,
    cx: &mut Context<'_>,
) -> Poll<(usize, Result<T>)>
where
    T: 'a,
{
    for (k, fut) in futures {
        let poll = fut.poll_unpin(cx);

        if let Poll::Ready(result) = poll {
            return Poll::Ready((k, result.unwrap_or_else(|err| Err(Error::from(err)))));
        }
    }

    Poll::Pending
}
