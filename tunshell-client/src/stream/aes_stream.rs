use super::TunnelStream;
use anyhow::{Error, Result};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use futures::Stream;
use io::prelude::*;
use log::*;
use ring::aead::*;
use ring::pbkdf2::*;
use ring::rand::{SecureRandom, SystemRandom};
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
struct EncryptedMessage {
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
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

        Ok(RawMessage::new(self.type_id(), cursor.into_inner()))
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
    key: LessSafeKey,
    read_buff: Vec<u8>,
    write_buff: Option<EncryptedMessage>,
}

impl<S: futures::AsyncRead + futures::AsyncWrite + Unpin + Send> AesStream<S> {
    pub fn new(inner: S, salt: &[u8], key: &[u8]) -> Self {
        Self {
            inner: AesMessageStream::new(inner),
            key: derive_key(salt, key),
            read_buff: vec![],
            write_buff: None,
        }
    }
}

impl<S: futures::AsyncRead + futures::AsyncWrite + Unpin + Send> AsyncRead for AesStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buff: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        while self.read_buff.len() == 0 {
            let message = match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(message))) => message,
                Poll::Ready(Some(Err(err))) => {
                    warn!("encountered error while reading from inner stream: {}", err);
                    return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
                }
                Poll::Ready(None) => return Poll::Ready(Ok(0)),
                Poll::Pending => return Poll::Pending,
            };

            let decrypted = match decrypt(message, &self.key) {
                Ok(data) => data,
                Err(err) => {
                    warn!("failed to decrypt message: {}", err);
                    return Poll::Ready(Err(io::Error::from(io::ErrorKind::InvalidData)));
                }
            };

            self.read_buff.extend_from_slice(decrypted.as_slice());
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
        assert!(buff.len() > 0);

        let mut written = 0;
        loop {
            let message = match self.write_buff.take() {
                Some(message) => message,
                None => {
                    written += buff.len();
                    encrypt(buff, &self.key)
                }
            };

            match Pin::new(&mut self.inner).poll_write(cx, &message) {
                Poll::Ready(Ok(_)) => {
                    if written > 0 {
                        return Poll::Ready(Ok(written));
                    } else {
                        self.write_buff.replace(message);
                    }
                }
                Poll::Ready(Err(err)) => {
                    warn!("encountered error while writing to inner stream: {}", err);
                    return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
                }
                Poll::Pending => {
                    self.write_buff.replace(message);

                    return if written > 0 {
                        Poll::Ready(Ok(written))
                    } else {
                        Poll::Pending
                    };
                }
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        if let Some(message) = self.write_buff.take() {
            match Pin::new(&mut self.inner).poll_write(cx, &message) {
                Poll::Ready(Ok(_)) => {}
                Poll::Ready(Err(err)) => {
                    warn!("encountered error while writing to inner stream: {}", err);
                    return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
                }
                Poll::Pending => {
                    self.write_buff.replace(message);
                    return Poll::Pending;
                }
            }
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

fn derive_key(salt: &[u8], key: &[u8]) -> LessSafeKey {
    let mut derived_key = [0u8; 32];
    derive(
        PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(1000).unwrap(),
        salt,
        key,
        &mut derived_key,
    );

    LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &derived_key).unwrap())
}

fn encrypt(plaintext: &[u8], key: &LessSafeKey) -> EncryptedMessage {
    let mut nonce = [0u8; 12];
    let rand = SystemRandom::new();
    rand.fill(&mut nonce).unwrap();

    let mut ciphertext = plaintext.to_vec();
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce.to_owned()),
        Aad::empty(),
        &mut ciphertext,
    )
    .unwrap();

    EncryptedMessage {
        nonce: nonce.to_vec(),
        ciphertext,
    }
}

fn decrypt(message: EncryptedMessage, key: &LessSafeKey) -> Result<Vec<u8>> {
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&message.nonce[..12]);
    let mut plaintext = message.ciphertext.clone();

    let plaintext = key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce.to_owned()),
            Aad::empty(),
            &mut plaintext,
        )
        .map_err(|_| Error::msg("failed to decrypt message"))?;

    Ok(plaintext.to_vec())
}

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
    fn test_derive_key() {
        let key = derive_key(&[1, 2, 3], &[4, 5, 6]);

        assert_eq!(key.algorithm(), &AES_256_GCM);
    }

    #[test]
    fn test_encrypt() {
        let key = derive_key(&[1, 2, 3], &[4, 5, 6]);

        let plaintext = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        let message = encrypt(plaintext, &key);

        assert_eq!(message.nonce.len(), 12);
        assert_eq!(message.ciphertext != plaintext, true);
    }

    #[test]
    fn test_decrypt() {
        let key = derive_key(&[1, 2, 3], &[4, 5, 6]);

        let message = EncryptedMessage {
            nonce: vec![189, 25, 240, 240, 29, 63, 100, 148, 64, 109, 40, 111],
            ciphertext: vec![
                136, 184, 73, 200, 170, 217, 131, 111, 107, 84, 98, 187, 208, 126, 250, 121, 133,
                200, 18, 46, 89, 115, 196, 239, 53, 171,
            ],
        };

        let plaintext = decrypt(message, &key).unwrap();

        assert_eq!(plaintext, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn test_decrypt_invalid() {
        let key = derive_key(&[1, 2, 3], &[4, 5, 6]);

        let message = EncryptedMessage {
            nonce: vec![189, 25, 240, 240, 29, 63, 100, 148, 64, 109, 40, 111],
            ciphertext: vec![2, 54, 65, 47, 54, 3, 35],
        };

        decrypt(message, &key).expect_err("decrypt should fail on garbage data");
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

            let mut stream = AesStream::new(cursor, &[1, 2, 3], &[4, 5, 6]);

            let mut buff = [0u8; 1024];
            let read = stream.read(&mut buff).await.unwrap();

            assert_eq!(&buff[..read], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        })
    }

    #[test]
    fn test_write_then_read_aes_stream() {
        Runtime::new().unwrap().block_on(async {
            let cursor = Cursor::new(vec![]);

            let mut stream = AesStream::new(cursor, &[1, 2, 3], &[4, 5, 6]);

            let wrote = stream.write("hello world".as_bytes()).await.unwrap();
            stream.flush().await.unwrap();

            assert_eq!(wrote, 11);

            let mut cursor = stream.inner.inner().clone();
            cursor.set_position(0);
            let mut stream = AesStream::new(cursor, &[1, 2, 3], &[4, 5, 6]);

            let mut buff = [0u8; 1024];
            let read = stream.read(&mut buff).await.unwrap();

            assert_eq!(
                String::from_utf8(buff[..read].to_vec()).unwrap(),
                "hello world".to_owned()
            );
        })
    }
}
