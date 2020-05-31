use crate::message::*;
use anyhow::{Error, Result};
use futures::prelude::*;
use futures::stream::Stream;
use log::debug;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct MessageStream<I: Message<I>, O: Message<O>, S: AsyncRead + AsyncWrite + Unpin> {
    inner: S,
    read_buff: Vec<u8>,

    write_buff: Vec<u8>,
    stream_error: bool,

    // For unused type param I, O
    phantom_i: PhantomData<I>,
    phantom_o: PhantomData<O>,
}

impl<I: Message<I>, O: Message<O>, S: AsyncRead + AsyncWrite + Unpin> MessageStream<I, O, S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            read_buff: vec![],
            write_buff: vec![],
            stream_error: false,
            phantom_i: PhantomData,
            phantom_o: PhantomData,
        }
    }

    pub fn inner(&self) -> &S {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    fn poll_read_inner_stream(
        self: &mut Pin<&mut Self>,
        mut cx: &mut Context<'_>,
    ) -> Poll<Result<usize, std::io::Error>> {
        let mut raw_buff = [0u8; 1024];
        let inner = Pin::new(&mut self.inner);

        match inner.poll_read(&mut cx, &mut raw_buff) {
            Poll::Ready(Ok(0)) => Poll::Ready(Ok(0)),
            Poll::Ready(Ok(read)) => {
                self.as_mut().read_buff.extend_from_slice(&raw_buff[..read]);

                Poll::Ready(Ok(read))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn parse_buffer(&self) -> (u8, usize, usize) {
        if self.read_buff.len() < 3 {
            (0, 0, 0)
        } else {
            (
                self.read_buff[0],
                (self.read_buff[1] as usize) << 8 | (self.read_buff[2] as usize),
                self.read_buff.len() - 3,
            )
        }
    }
}

impl<I: Message<I>, O: Message<O>, S: AsyncRead + AsyncWrite + Unpin> Stream
    for MessageStream<I, O, S>
{
    type Item = Result<O>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.stream_error {
            return Poll::Ready(None);
        }

        loop {
            let (mut type_id, mut message_length, mut bytes_available) = self.parse_buffer();

            while self.read_buff.len() < 3 || bytes_available < message_length {
                match self.poll_read_inner_stream(cx) {
                    Poll::Ready(Ok(0)) => {
                        // If the stream ends on the end of a message boundary, return success
                        if self.read_buff.len() == 0 {
                            return Poll::Ready(None);
                        }

                        // Else the stream ended with a partial message, return error
                        self.stream_error = true;
                        return Poll::Ready(Some(Err(Error::msg(
                            "Inner stream failed to return complete message",
                        ))));
                    }
                    Poll::Ready(Ok(_read)) => (),
                    Poll::Ready(Err(err)) => {
                        self.stream_error = true;

                        return Poll::Ready(Some(Err(Error::new(err))));
                    }
                    Poll::Pending => return Poll::Pending,
                }

                let parsed_buff = self.parse_buffer();
                type_id = parsed_buff.0;
                message_length = parsed_buff.1;
                bytes_available = parsed_buff.2;
            }

            let raw_message = RawMessage::new(
                type_id,
                self.read_buff
                    .iter()
                    .cloned()
                    .skip(3)
                    .take(message_length)
                    .collect(),
            );

            self.read_buff = self
                .read_buff
                .iter()
                .cloned()
                .skip(3 + message_length)
                .collect();

            match O::deserialise(&raw_message) {
                Ok(message) => {
                    debug!("Received message {:?}", message);
                    return Poll::Ready(Some(Ok(message)));
                }
                Err(err) => {
                    debug!("Error while deserialised received message {:?}", err);
                    return Poll::Ready(Some(Err(err)));
                }
            }
        }
    }
}

impl<I: Message<I>, O: Message<O>, S: AsyncRead + AsyncWrite + Unpin> MessageStream<I, O, S> {
    pub fn poll_write(
        mut self: Pin<&mut Self>,
        mut cx: &mut Context<'_>,
        message: &I,
    ) -> Poll<Result<()>> {
        if self.stream_error {
            return Poll::Ready(Err(Error::msg(
                "Stream called after underlying stream error",
            )));
        }

        debug!("Sending message: {:?}", message);
        let serialised = message.serialise()?.to_vec();
        self.write_buff.extend(serialised);

        let buff = self.write_buff.clone();
        let mut written = 0;

        while written < buff.len() {
            let write_result = Pin::new(&mut self.inner).poll_write(&mut cx, &buff[written..]);

            match write_result {
                Poll::Ready(Ok(wrote)) => written += wrote,
                Poll::Ready(Err(err)) => {
                    self.stream_error = true;

                    return Poll::Ready(Err(Error::new(err)));
                }
                Poll::Pending => {
                    self.write_buff = self
                        .write_buff
                        .iter()
                        .skip(written)
                        .cloned()
                        .collect::<Vec<u8>>();

                    return Poll::Pending;
                }
            }
        }

        self.write_buff = vec![];

        Poll::Ready(Ok(()))
    }

    pub async fn write(&mut self, message: &I) -> Result<()> {
        let serialised = message.serialise()?.to_vec();
        let mut written = 0;

        while written < serialised.len() {
            match self.inner.write(&serialised[written..]).await {
                Ok(wrote) => written += wrote,
                Err(err) => return Err(Error::new(err)),
            }
        }

        debug!("Sent message: {:?}", message);

        Ok(())
    }

    pub async fn write_all(&mut self, messages: &[I]) -> Vec<Result<()>> {
        let mut results = Vec::with_capacity(messages.len());

        for message in messages {
            results.push(self.write(message).await);
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor;
    use futures::io::Cursor;

    #[test]
    fn test_read_empty_stream() {
        let mock_stream = Cursor::new(vec![]);
        let stream =
            MessageStream::<ClientMessage, ServerMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let results: Vec<Result<ServerMessage>> =
            executor::block_on(async move { stream.collect().await });

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_read_server_close_message() {
        let mock_stream = Cursor::new(ServerMessage::Close.serialise().unwrap().to_vec());
        let stream =
            MessageStream::<ClientMessage, ServerMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let results: Vec<Result<ServerMessage>> =
            executor::block_on(async move { stream.collect().await });

        assert_eq!(results.len(), 1);
        assert_eq!(*results[0].as_ref().unwrap(), ServerMessage::Close);
    }

    #[test]
    fn test_read_client_key_message() {
        let mock_stream = Cursor::new(
            ClientMessage::Key(KeyPayload {
                key: "key".to_owned(),
            })
            .serialise()
            .unwrap()
            .to_vec(),
        );
        let stream =
            MessageStream::<ServerMessage, ClientMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let results: Vec<Result<ClientMessage>> =
            executor::block_on(async move { stream.collect().await });

        assert_eq!(results.len(), 1);
        assert_eq!(
            *results[0].as_ref().unwrap(),
            ClientMessage::Key(KeyPayload {
                key: "key".to_owned(),
            })
        );
    }

    #[test]
    fn test_read_multiple_messages() {
        let messages = vec![
            ClientMessage::Key(KeyPayload {
                key: "key".to_owned(),
            }),
            ClientMessage::Time(TimePayload {
                client_time: 123456,
            }),
            ClientMessage::DirectConnectSucceeded,
        ];
        let mock_stream = Cursor::new(
            messages
                .iter()
                .flat_map(|m| m.serialise().unwrap().to_vec())
                .collect(),
        );
        let stream =
            MessageStream::<ServerMessage, ClientMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let results: Vec<Result<ClientMessage>> =
            executor::block_on(async move { stream.collect().await });

        assert_eq!(
            results
                .into_iter()
                .map(|m| m.unwrap())
                .collect::<Vec<ClientMessage>>(),
            messages
        );
    }

    #[test]
    fn test_read_incomplete_message() {
        let mock_stream = Cursor::new(vec![255, 255, 255]);
        let stream =
            MessageStream::<ServerMessage, ClientMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let results: Vec<Result<ClientMessage>> =
            executor::block_on(async move { stream.collect().await });

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].as_ref().unwrap_err().to_string(),
            "Inner stream failed to return complete message"
        );
    }

    #[test]
    fn test_read_invalid_message() {
        let mock_stream = Cursor::new(vec![255, 0, 1, 1]);
        let stream =
            MessageStream::<ServerMessage, ClientMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let results: Vec<Result<ClientMessage>> =
            executor::block_on(async move { stream.collect().await });

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].as_ref().unwrap_err().to_string(),
            "Failed to parse client message: RawMessage { type_id: 255, length: 1, data: [1] }"
        );
    }

    #[test]
    fn test_write_client_close_message() {
        let message = ClientMessage::Close;
        let mock_stream = Cursor::new(vec![]);
        let mut stream =
            MessageStream::<ClientMessage, ServerMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let result = executor::block_on(async { stream.write(&message).await });

        assert_eq!(result.unwrap(), ());
        assert_eq!(stream.inner.into_inner(), vec![0, 0, 0]);
    }

    #[test]
    fn test_write_multiple_server_messages() {
        let messages = vec![
            ServerMessage::KeyAccepted(KeyAcceptedPayload {
                key_type: KeyType::Client,
            }),
            ServerMessage::StartRelayMode,
            ServerMessage::Close,
        ];
        let mock_stream = Cursor::new(vec![]);
        let mut stream =
            MessageStream::<ServerMessage, ClientMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let results = executor::block_on(async { stream.write_all(&messages).await });

        assert_eq!(
            results.into_iter().map(|i| i.unwrap()).collect::<Vec<()>>(),
            vec![(), (), ()]
        );
        assert_eq!(
            stream.inner.into_inner(),
            messages
                .iter()
                .flat_map(|i| i.serialise().unwrap().to_vec())
                .collect::<Vec<u8>>()
        );
    }

    #[test]
    fn test_poll_write_close_message() {
        use futures_test::task::noop_context;

        let message = ClientMessage::Close;
        let mock_stream = Cursor::new(vec![]);
        let mut stream =
            MessageStream::<ClientMessage, ServerMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let result = Pin::new(&mut stream).poll_write(&mut noop_context(), &message);

        assert!(result.is_ready());
        assert_eq!(stream.inner.into_inner(), vec![0, 0, 0]);
    }
}
