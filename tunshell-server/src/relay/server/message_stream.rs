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

type MessageStreamInner<IO> = MessageStream<ServerMessage, ClientMessage, IO>;

pub(super) struct ClientMessageStream<IO: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static> {
    stream: Option<MessageStreamInner<Compat<IO>>>,
    closed: bool,
}

impl<IO: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static> ClientMessageStream<IO> {
    pub(super) fn new(stream: IO) -> Self {
        Self {
            stream: Some(MessageStreamInner::new(stream.compat())),
            closed: false,
        }
    }

    #[allow(dead_code)]
    pub(super) fn stream(&self) -> &MessageStreamInner<Compat<IO>> {
        self.stream.as_ref().unwrap()
    }

    pub(super) fn stream_mut(&mut self) -> &mut MessageStreamInner<Compat<IO>> {
        self.stream.as_mut().unwrap()
    }

    #[allow(dead_code)]
    pub(super) fn inner(&self) -> &IO {
        self.stream().inner().get_ref()
    }

    pub(super) async fn next(&mut self) -> Result<ClientMessage> {
        match self.stream_mut().next().await {
            Some(result @ Ok(ClientMessage::Close)) => {
                self.closed = true;
                result
            }
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

impl<IO: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static> Drop for ClientMessageStream<IO> {
    fn drop(&mut self) {
        if self.closed {
            return;
        }

        // Attempt to send a close message to the client
        // when the ClientMessageStream is being dropped without being closed
        let stream = self.stream.take().unwrap();
        try_send_close(stream);
    }
}

fn try_send_close<IO: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static>(
    mut stream: MessageStreamInner<Compat<IO>>,
) {
    // In the case of the client closing the ClientMessageStream early
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
