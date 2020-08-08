use super::{crypto::*, TunnelStream};
use anyhow::{Context as AnyhowContext, Result};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use futures::{future::BoxFuture, FutureExt, Stream};
use io::prelude::*;
use log::*;
use std::cmp;
use std::io;
use std::{
    io::Cursor,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tunshell_shared::{Message, MessageStream, RawMessage};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct EncryptedMessage {
    pub(super) nonce: Vec<u8>,
    pub(super) ciphertext: Vec<u8>,
}

impl Message for EncryptedMessage {
    fn type_id(&self) -> u8 {
        return 0;
    }

    fn serialise(&self) -> Result<RawMessage> {
        let mut cursor = Cursor::new(vec![]);

        cursor.write_u8(self.nonce.len() as u8).unwrap();
        cursor.write(self.nonce.as_slice()).unwrap();

        cursor
            .write_u16::<BigEndian>(self.ciphertext.len() as u16)
            .unwrap();
        cursor.write(self.ciphertext.as_slice()).unwrap();

        RawMessage::new(self.type_id(), cursor.into_inner())
    }

    fn deserialise(raw_message: &RawMessage) -> Result<Self> {
        let mut cursor = Cursor::new(raw_message.data());

        let nonce_length = cursor.read_u8()?;
        let mut nonce = vec![0u8; nonce_length as usize];
        cursor.read_exact(nonce.as_mut_slice())?;

        let ciphertext_length = cursor.read_u16::<BigEndian>()?;
        let mut ciphertext = vec![0u8; ciphertext_length as usize];
        cursor.read_exact(ciphertext.as_mut_slice())?;

        Ok(Self { nonce, ciphertext })
    }
}

type AesMessageStream<S> = MessageStream<EncryptedMessage, EncryptedMessage, S>;

pub struct AesStream<S: futures::AsyncRead + futures::AsyncWrite + Unpin + Send> {
    inner: AesMessageStream<S>,
    decrypt: CryptoState<Vec<u8>>,
    encrypt: CryptoState<EncryptedMessage>,
    read_buff: Vec<u8>,
    write_buff: Option<EncryptedMessage>,
}

enum CryptoState<R> {
    Swapping,
    Pending(Key),
    Crypting(BoxFuture<'static, Result<(R, Key)>>),
}

impl<S: futures::AsyncRead + futures::AsyncWrite + Unpin + Send> AesStream<S> {
    pub async fn new(inner: S, salt: &[u8], key: &[u8]) -> Result<Self> {
        let key = derive_key(salt, key)
            .await
            .with_context(|| "failed to derive encryption key")?;

        Ok(Self {
            inner: AesMessageStream::new(inner),
            encrypt: CryptoState::Pending(key.clone()),
            decrypt: CryptoState::Pending(key),
            read_buff: vec![],
            write_buff: None,
        })
    }
}

impl<S: futures::AsyncRead + futures::AsyncWrite + Unpin + Send> AsyncRead for AesStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        debug!("reading from aes stream");

        while self.read_buff.len() == 0 {
            match std::mem::replace(&mut self.decrypt, CryptoState::Swapping) {
                CryptoState::Crypting(mut fut) => {
                    let result = match fut.as_mut().poll(cx) {
                        Poll::Ready(i) => i,
                        Poll::Pending => {
                            self.decrypt = CryptoState::Crypting(fut);
                            return Poll::Pending;
                        }
                    };

                    if let Err(err) = result {
                        warn!("failed to decrypt message: {}", err);
                        return Poll::Ready(Err(io::Error::from(io::ErrorKind::InvalidData)));
                    }

                    let (plaintext, key) = result.unwrap();
                    self.read_buff.extend_from_slice(plaintext.as_slice());
                    self.decrypt = CryptoState::Pending(key);
                }
                CryptoState::Pending(key) => {
                    let message = match Pin::new(&mut self.inner).poll_next(cx) {
                        Poll::Ready(Some(Ok(message))) => message,
                        Poll::Ready(Some(Err(err))) => {
                            warn!("encountered error while reading from inner stream: {}", err);
                            return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
                        }
                        Poll::Ready(None) => return Poll::Ready(Ok(0)),
                        Poll::Pending => {
                            self.decrypt = CryptoState::Pending(key);
                            return Poll::Pending;
                        }
                    };

                    self.decrypt = CryptoState::Crypting(decrypt(message, key).boxed());
                }
                CryptoState::Swapping => unreachable!(),
            }
        }

        let read = cmp::min(buff.len(), self.read_buff.len());

        buff[..read].copy_from_slice(&self.read_buff[..read]);
        self.read_buff.drain(..read);

        Poll::Ready(Ok(read))
    }
}

impl<S: futures::AsyncRead + futures::AsyncWrite + Unpin + Send> AsyncWrite for AesStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        if buff.len() > 0 {
            debug!("sending {} bytes", buff.len());
        }

        let mut poll = Poll::Pending;

        while self.write_buff.is_none() {
            match std::mem::replace(&mut self.encrypt, CryptoState::Swapping) {
                CryptoState::Pending(key) => {
                    self.encrypt = CryptoState::Crypting(encrypt(buff.to_vec(), key).boxed());
                    poll = Poll::Ready(Ok(buff.len()));
                }
                CryptoState::Crypting(mut fut) => {
                    let result = match fut.as_mut().poll(cx) {
                        Poll::Ready(i) => i,
                        Poll::Pending => {
                            self.encrypt = CryptoState::Crypting(fut);
                            return poll;
                        }
                    };

                    if let Err(err) = result {
                        warn!("failed to encrypt message: {}", err);
                        return Poll::Ready(Err(io::Error::from(io::ErrorKind::Other)));
                    }

                    let (encrypted, key) = result.unwrap();
                    debug!("encrypted into {} bytes", encrypted.ciphertext.len());
                    self.write_buff.replace(encrypted);
                    self.encrypt = CryptoState::Pending(key);
                }
                CryptoState::Swapping => unreachable!(),
            }
        }

        let message = self.write_buff.take().unwrap();
        match Pin::new(&mut self.inner).poll_write(cx, &message) {
            Poll::Ready(Ok(_)) => {
                return if buff.len() == 0 {
                    Poll::Ready(Ok(0))
                } else {
                    return poll;
                }
            }
            Poll::Ready(Err(err)) => {
                warn!("encountered error while writing to inner stream: {}", err);
                return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
            }
            Poll::Pending => {
                self.write_buff.replace(message);
                return poll;
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        debug!("flushing aes stream");

        let poll = if self.write_buff.is_some() {
            Some(Pin::new(&mut *self.as_mut()).poll_write(cx, &[]))
        } else if let CryptoState::Crypting(_) = &self.encrypt {
            Some(Pin::new(&mut *self.as_mut()).poll_write(cx, &[]))
        } else {
            None
        };

        if let Some(Poll::Pending) = poll {
            return Poll::Pending;
        }

        Pin::new(self.inner.inner_mut()).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(self.inner.inner_mut()).poll_close(cx)
    }
}

impl<S: futures::AsyncRead + futures::AsyncWrite + Unpin + Send> TunnelStream for AesStream<S> {}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::Cursor;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::runtime::Runtime;

    #[test]
    fn test_serialise_encrypted_message() {
        let message = EncryptedMessage {
            nonce: vec![1, 2, 3],
            ciphertext: vec![5, 6, 7, 8, 9, 10],
        };

        let serialised = message.serialise().unwrap();

        assert_eq!(
            serialised.to_vec(),
            vec![0, 0, 12, 3, 1, 2, 3, 0, 6, 5, 6, 7, 8, 9, 10]
        );

        let parsed = EncryptedMessage::deserialise(&serialised).unwrap();

        assert_eq!(parsed, message);
    }

    #[test]
    fn test_encrypt() {
        Runtime::new().unwrap().block_on(async {
            let key = derive_key(&[1, 2, 3], &[4, 5, 6]).await.unwrap();

            let plaintext = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

            let (message, _) = encrypt(plaintext.to_vec(), key.clone()).await.unwrap();

            assert_eq!(message.nonce.len(), 12);
            assert_eq!(message.ciphertext != plaintext, true);
        });
    }

    #[test]
    fn test_decrypt() {
        Runtime::new().unwrap().block_on(async {
            let key = derive_key(&[1, 2, 3], &[4, 5, 6]).await.unwrap();

            let message = EncryptedMessage {
                nonce: vec![189, 25, 240, 240, 29, 63, 100, 148, 64, 109, 40, 111],
                ciphertext: vec![
                    136, 184, 73, 200, 170, 217, 131, 111, 107, 84, 98, 187, 208, 126, 250, 121,
                    133, 200, 18, 46, 89, 115, 196, 239, 53, 171,
                ],
            };

            let (plaintext, _) = decrypt(message, key.clone()).await.unwrap();

            assert_eq!(plaintext, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        });
    }

    #[test]
    fn test_decrypt_invalid() {
        Runtime::new().unwrap().block_on(async {
            let key = derive_key(&[1, 2, 3], &[4, 5, 6]).await.unwrap();

            let message = EncryptedMessage {
                nonce: vec![189, 25, 240, 240, 29, 63, 100, 148, 64, 109, 40, 111],
                ciphertext: vec![2, 54, 65, 47, 54, 3, 35],
            };

            decrypt(message, key)
                .await
                .map(|i| i.0)
                .expect_err("decrypt should fail on garbage data");
        });
    }

    #[test]
    fn test_read_from_aes_stream() {
        Runtime::new().unwrap().block_on(async {
            let message = EncryptedMessage {
                nonce: vec![189, 25, 240, 240, 29, 63, 100, 148, 64, 109, 40, 111],
                ciphertext: vec![
                    136, 184, 73, 200, 170, 217, 131, 111, 107, 84, 98, 187, 208, 126, 250, 121,
                    133, 200, 18, 46, 89, 115, 196, 239, 53, 171,
                ],
            };

            let cursor = Cursor::new(message.serialise().unwrap().to_vec());

            let mut stream = AesStream::new(cursor, &[1, 2, 3], &[4, 5, 6])
                .await
                .unwrap();

            let mut buff = [0u8; 1024];
            let read = stream.read(&mut buff).await.unwrap();

            assert_eq!(&buff[..read], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        });
    }

    #[test]
    fn test_write_then_read_aes_stream() {
        env_logger::init();
        Runtime::new().unwrap().block_on(async {
            let cursor = Cursor::new(vec![]);

            let mut stream = AesStream::new(cursor, &[1, 2, 3], &[4, 5, 6])
                .await
                .unwrap();

            let wrote = stream.write("hello world".as_bytes()).await.unwrap();
            stream.flush().await.unwrap();

            assert_eq!(wrote, 11);

            let mut cursor = stream.inner.inner().clone();
            cursor.set_position(0);
            let mut stream = AesStream::new(cursor, &[1, 2, 3], &[4, 5, 6])
                .await
                .unwrap();

            let mut buff = [0u8; 1024];
            let read = stream.read(&mut buff).await.unwrap();

            assert_eq!(
                String::from_utf8(buff[..read].to_vec()).unwrap(),
                "hello world".to_owned()
            );
        })
    }
}
