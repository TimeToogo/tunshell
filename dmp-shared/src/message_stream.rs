use crate::message::{ClientMessage, KeyPayload, Message, RawMessage, ServerMessage};
use anyhow::{Error, Result};
use futures::prelude::*;
use futures::stream::Stream;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct MessageStream<M: Message<M> + Unpin, S: AsyncRead + AsyncWrite + Unpin + Sync + Send> {
    inner: S,
    buff: Vec<u8>,
    has_error: bool,

    // For unused type param M
    phantom: PhantomData<M>,
}

pub trait MessageStreamTrait<M: Message<M>> {}

impl<M: Message<M> + Unpin, S: AsyncRead + AsyncWrite + Unpin + Sync + Send> MessageStream<M, S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            buff: vec![],
            has_error: false,
            phantom: PhantomData,
        }
    }

    fn poll_inner_stream(
        self: &mut Pin<&mut Self>,
        mut cx: &mut Context<'_>,
    ) -> Poll<Result<usize, std::io::Error>> {
        let mut raw_buff = [0u8; 1024];
        let inner = Pin::new(&mut self.inner);

        match inner.poll_read(&mut cx, &mut raw_buff) {
            Poll::Ready(Ok(0)) => Poll::Ready(Ok(0)),
            Poll::Ready(Ok(read)) => {
                self.as_mut().buff.extend_from_slice(&raw_buff[..read]);

                Poll::Ready(Ok(read))
            }
            Poll::Ready(Err(err)) => {
                self.has_error = true;

                Poll::Ready(Err(err))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn parse_buffer(&self) -> (u8, usize, usize) {
        if self.buff.len() < 3 {
            (0, 0, 0)
        } else {
            (
                self.buff[0],
                (self.buff[1] as usize) << 8 | (self.buff[2] as usize),
                self.buff.len() - 3,
            )
        }
    }
}

impl<M: Message<M> + Unpin + std::fmt::Debug, S: AsyncRead + AsyncWrite + Unpin + Sync + Send>
    Stream for MessageStream<M, S>
{
    type Item = Result<M>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let (mut type_id, mut message_length, mut bytes_available) = self.parse_buffer();

            while self.buff.len() < 3 || bytes_available < message_length {
                match self.poll_inner_stream(cx) {
                    Poll::Ready(Ok(0)) => return Poll::Ready(None),
                    Poll::Ready(Ok(_read)) => (),
                    Poll::Ready(Err(err)) => return Poll::Ready(Some(Err(Error::new(err)))),
                    Poll::Pending => return Poll::Pending,
                }

                let parsed_buff = self.parse_buffer();
                type_id = parsed_buff.0;
                message_length = parsed_buff.1;
                bytes_available = parsed_buff.2;
            }

            let raw_message = RawMessage::new(
                type_id,
                self.buff
                    .iter()
                    .cloned()
                    .skip(3)
                    .take(message_length)
                    .collect(),
            );

            self.buff = self.buff.iter().cloned().skip(3 + message_length).collect();

            return Poll::Ready(Some(M::deserialise(&raw_message)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor;
    use futures::io::Cursor;

    #[test]
    fn test_read_server_close_message() {
        let mock_stream = Cursor::new(ServerMessage::Close.serialise().unwrap().to_vec());
        let stream = MessageStream::<ServerMessage, Cursor<Vec<u8>>>::new(mock_stream);

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
        let stream = MessageStream::<ClientMessage, Cursor<Vec<u8>>>::new(mock_stream);

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
}
