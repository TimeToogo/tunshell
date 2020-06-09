use crate::stream::TunnelStream;
use dmp_shared::{ClientMessage, MessageStream, RelayPayload, ServerMessage};
use futures::stream::Stream;
use log::debug;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::pin::Pin;
use std::result::Result as StdResult;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use thrussh::Tcp;
use tokio::io::{AsyncRead, AsyncWrite};

pub struct RelayStream<S: futures::AsyncRead + futures::AsyncWrite + Unpin> {
    message_stream: Arc<Mutex<MessageStream<ClientMessage, ServerMessage, S>>>,

    read_buff: Vec<u8>,
    sent_close: bool,
    closed: bool,
}

impl<S: futures::AsyncRead + futures::AsyncWrite + Send + Unpin> RelayStream<S> {
    pub fn new(message_stream: Arc<Mutex<MessageStream<ClientMessage, ServerMessage, S>>>) -> Self {
        RelayStream {
            message_stream,
            read_buff: vec![],
            sent_close: false,
            closed: false,
        }
    }
}

impl<S: futures::AsyncRead + futures::AsyncWrite + Send + Unpin> AsyncRead for RelayStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        mut cx: &mut Context<'_>,
        buff: &mut [u8],
    ) -> Poll<StdResult<usize, IoError>> {
        if self.closed {
            debug!("poll_read: stream closed, 0 bytes returned");
            return Poll::Ready(Ok(0));
        }

        if self.read_buff.len() == 0 {
            debug!("poll_read: poll_next underlying message stream");
            let read_result =
                Pin::new(&mut *self.message_stream.lock().unwrap()).poll_next(&mut cx);
            debug!("poll_read: read result: {:?}", read_result);

            match read_result {
                Poll::Ready(Some(Ok(ServerMessage::Relay(payload)))) => {
                    debug!("poll_read: relay message returned {:?}", payload);
                    self.read_buff.extend(payload.data)
                }
                Poll::Ready(Some(Ok(ServerMessage::PeerLeft))) => {
                    debug!("poll_read: peer left");
                    self.closed = true
                }
                Poll::Ready(Some(Ok(ServerMessage::Close))) => {
                    debug!("poll_read: closed");
                    self.closed = true
                }
                Poll::Ready(Some(Ok(message))) => {
                    debug!(
                        "Unexpected message received from server during relay: {:?}",
                        message
                    );
                    return Poll::Ready(Err(IoError::from(IoErrorKind::InvalidData)));
                }
                Poll::Ready(Some(Err(err))) => {
                    debug!(
                        "Error returned from underlying message stream while reading relay message: {:?}",
                        err
                    );
                    return Poll::Ready(Err(IoError::from(IoErrorKind::Other)));
                }
                Poll::Ready(None) => {
                    debug!("Underlying message stream ended unexpectedly during relay");
                    return Poll::Ready(Err(IoError::from(IoErrorKind::UnexpectedEof)));
                }
                Poll::Pending => return Poll::Pending,
            };
        }

        let read = std::cmp::min(buff.len(), self.read_buff.len());
        buff[..read].copy_from_slice(&self.read_buff[..read]);
        self.read_buff.drain(..read);

        debug!("poll_read: returned {} bytes", read);
        Poll::Ready(Ok(read))
    }
}

impl<S: futures::AsyncRead + futures::AsyncWrite + Send + Unpin> AsyncWrite for RelayStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        mut cx: &mut Context<'_>,
        buff: &[u8],
    ) -> Poll<StdResult<usize, IoError>> {
        if self.closed {
            return Poll::Ready(Err(IoError::from(IoErrorKind::BrokenPipe)));
        }

        // Ensure the write serialises to message under 16KB limit
        let len = std::cmp::min(buff.len(), 10240);
        let write_result = Pin::new(&mut *self.message_stream.lock().unwrap()).poll_write(
            &mut cx,
            &ClientMessage::Relay(RelayPayload {
                data: Vec::from(&buff[..len]),
            }),
        );

        match write_result {
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(len)),
            Poll::Ready(Err(err)) => {
                self.closed = true;
                debug!(
                    "Error returned from underlying message stream while sending relay message: {}",
                    err
                );
                return Poll::Ready(Err(IoError::from(IoErrorKind::Other)));
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, mut cx: &mut Context<'_>) -> Poll<StdResult<(), IoError>> {
        Pin::new(self.message_stream.lock().unwrap().inner_mut()).poll_flush(&mut cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        mut cx: &mut Context<'_>,
    ) -> Poll<StdResult<(), IoError>> {
        if !self.sent_close {
            self.sent_close = true;

            let write_result = Pin::new(&mut *self.message_stream.lock().unwrap())
                .poll_write(&mut cx, &ClientMessage::Close);

            match write_result {
                Poll::Ready(Ok(_)) => (),
                Poll::Ready(Err(_)) => (),
                Poll::Pending => return Poll::Pending,
            };
        }

        let close_result =
            Pin::new(self.message_stream.lock().unwrap().inner_mut()).poll_close(&mut cx);

        if let Poll::Ready(_) = close_result {
            self.closed = true;
        }

        close_result
    }
}

impl<S: futures::AsyncRead + futures::AsyncWrite + Send + Unpin> Tcp for RelayStream<S> {}

impl<S: futures::AsyncRead + futures::AsyncWrite + Send + Unpin> TunnelStream for RelayStream<S> {}

#[cfg(test)]
mod tests {
    use super::*;
    use dmp_shared::Message;
    use futures::io::Cursor;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::runtime::Runtime;

    #[test]
    fn test_relay_stream_read_valid_messages() {
        let messages = vec![
            ServerMessage::Relay(RelayPayload {
                data: vec![1, 2, 3, 4, 5],
            }),
            ServerMessage::Relay(RelayPayload {
                data: vec![6, 7, 8, 9, 0],
            }),
            ServerMessage::Relay(RelayPayload {
                data: vec![1, 1, 1],
            }),
            ServerMessage::Close,
        ];

        let mock_stream = Cursor::new(
            messages
                .iter()
                .flat_map(|m| m.serialise().unwrap().to_vec())
                .collect(),
        );

        let message_stream =
            MessageStream::<ClientMessage, ServerMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let mut relay_stream = RelayStream::new(Arc::new(Mutex::new(message_stream)));

        let mut buff = vec![];
        let result = Runtime::new()
            .unwrap()
            .block_on(relay_stream.read_to_end(&mut buff));

        assert_eq!(result.unwrap(), 13usize);
        assert_eq!(
            buff.iter().cloned().take(13).collect::<Vec<u8>>(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 1, 1]
        )
    }

    #[test]
    fn test_relay_stream_read_ends_unexpectedly() {
        let messages = vec![ServerMessage::Relay(RelayPayload {
            data: vec![1, 2, 3, 4, 5],
        })];

        let mock_stream = Cursor::new(
            messages
                .iter()
                .flat_map(|m| m.serialise().unwrap().to_vec())
                .collect(),
        );

        let message_stream =
            MessageStream::<ClientMessage, ServerMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let mut relay_stream = RelayStream::new(Arc::new(Mutex::new(message_stream)));

        let mut buff = vec![];
        let result = Runtime::new()
            .unwrap()
            .block_on(relay_stream.read_to_end(&mut buff));

        assert_eq!(
            result.as_ref().unwrap_err().kind(),
            IoErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn test_relay_stream_read_invalid_message() {
        let messages = vec![
            ServerMessage::Relay(RelayPayload {
                data: vec![1, 2, 3, 4, 5],
            }),
            ServerMessage::TimePlease,
        ];

        let mock_stream = Cursor::new(
            messages
                .iter()
                .flat_map(|m| m.serialise().unwrap().to_vec())
                .collect(),
        );

        let message_stream =
            MessageStream::<ClientMessage, ServerMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let mut relay_stream = RelayStream::new(Arc::new(Mutex::new(message_stream)));

        let mut buff = vec![];
        let result = Runtime::new()
            .unwrap()
            .block_on(relay_stream.read_to_end(&mut buff));

        assert_eq!(
            result.as_ref().unwrap_err().kind(),
            IoErrorKind::InvalidData
        );
    }

    #[test]
    fn test_relay_stream_write_single_chunk() {
        let mock_stream = Cursor::new(vec![]);

        let message_stream =
            MessageStream::<ClientMessage, ServerMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let message_stream = Arc::new(Mutex::new(message_stream));
        let mut relay_stream = RelayStream::new(message_stream.clone());

        let buff = vec![1, 2, 3, 4, 5];
        let result = Runtime::new().unwrap().block_on(relay_stream.write(&buff));

        assert_eq!(result.unwrap(), 5);
        assert_eq!(
            message_stream.lock().unwrap().inner().get_ref(),
            &ClientMessage::Relay(RelayPayload {
                data: vec![1, 2, 3, 4, 5]
            })
            .serialise()
            .unwrap()
            .to_vec()
        );
    }

    #[test]
    fn test_relay_stream_close() {
        let mock_stream = Cursor::new(vec![]);

        let message_stream =
            MessageStream::<ClientMessage, ServerMessage, Cursor<Vec<u8>>>::new(mock_stream);

        let message_stream = Arc::new(Mutex::new(message_stream));
        let mut relay_stream = RelayStream::new(message_stream.clone());

        let result = Runtime::new().unwrap().block_on(relay_stream.shutdown());

        assert!(result.is_ok());
        assert_eq!(
            message_stream.lock().unwrap().inner().get_ref(),
            &ClientMessage::Close.serialise().unwrap().to_vec()
        );
    }
}
