use anyhow::{Error, Result};
use futures::StreamExt;
use log::*;
use std::time::Duration;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::timeout,
};
use tokio_util::compat::*;
use tunshell_shared::{ClientMessage, KeyPayload, MessageStream, ServerMessage};

type ClientMessageStream<IO> = MessageStream<ServerMessage, ClientMessage, IO>;

pub(super) struct Connection<IO: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static> {
    stream: Option<ClientMessageStream<Compat<IO>>>,
}

impl<IO: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static> Connection<IO> {
    pub(super) fn new(stream: IO) -> Self {
        Self {
            stream: Some(ClientMessageStream::new(stream.compat())),
        }
    }

    pub(super) fn stream(&self) -> &ClientMessageStream<Compat<IO>> {
        self.stream.as_ref().unwrap()
    }

    pub(super) fn stream_mut(&mut self) -> &mut ClientMessageStream<Compat<IO>> {
        self.stream.as_mut().unwrap()
    }

    pub(super) fn inner(&self) -> &IO {
        self.stream().inner().get_ref()
    }

    pub(super) async fn next(&mut self) -> Result<ClientMessage> {
        match self.stream_mut().next().await {
            Some(result) => result,
            None => Err(Error::msg("no messages are left in stream")),
        }
    }

    pub(super) async fn wait_for_key(&mut self, timeout_duration: Duration) -> Result<KeyPayload> {
        let message = timeout(timeout_duration, self.next()).await??;

        match message {
            ClientMessage::Key(key) => Ok(key),
            message @ _ => Err(Error::msg(format!(
                "unexpected message received from client, expecting key, got {:?}",
                message
            ))),
        }
    }

    pub(super) async fn write(&mut self, message: ServerMessage) -> Result<()> {
        self.stream_mut().write(&message).await
    }
}

impl<IO: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static> Drop for Connection<IO> {
    fn drop(&mut self) {
        // Always attempt to send a close message to the client
        // when the connection is being closed
        let stream = self.stream.take().unwrap();
        try_send_close(stream);
    }
}

fn try_send_close<IO: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static>(
    mut stream: ClientMessageStream<Compat<IO>>,
) {
    // In the case of the client closing the connection early
    // there is no need to send a close message
    if stream.is_closed() {
        return;
    }

    tokio::task::spawn(async move {
        debug!("sending close");
        stream
            .write(&ServerMessage::Close)
            .await
            .unwrap_or_else(|err| warn!("error while sending close: {}", err));
    });
}
