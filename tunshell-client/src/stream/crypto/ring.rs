use crate::stream::aes_stream::EncryptedMessage;
use anyhow::{Error, Result};
use log::*;
use ring::aead::*;
use ring::pbkdf2::*;
use ring::rand::{SecureRandom, SystemRandom};

pub struct Key(LessSafeKey, Vec<u8>);

impl Clone for Key {
    fn clone(&self) -> Self {
        Self(
            LessSafeKey::new(UnboundKey::new(&self.0.algorithm(), self.1.as_slice()).unwrap()),
            self.1.clone(),
        )
    }
}

pub(in crate::stream) async fn derive_key(salt: &[u8], key: &[u8]) -> Result<Key> {
    let mut derived_key = [0u8; 32];
    derive(
        PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(1000).unwrap(),
        salt,
        key,
        &mut derived_key,
    );

    Ok(Key(
        LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &derived_key).unwrap()),
        derived_key.to_vec(),
    ))
}

pub(in crate::stream) async fn encrypt(
    plaintext: Vec<u8>,
    key: Key,
) -> Result<(EncryptedMessage, Key)> {
    let mut nonce = [0u8; 12];
    let rand = SystemRandom::new();
    rand.fill(&mut nonce).unwrap();

    let len = plaintext.len();
    let mut ciphertext = plaintext;
    key.0
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce.to_owned()),
            Aad::empty(),
            &mut ciphertext,
        )
        .map_err(|_| Error::msg("failed to encrypt message"))?;

    debug!("encrypted {} bytes", len);
    Ok((
        EncryptedMessage {
            nonce: nonce.to_vec(),
            ciphertext,
        },
        key,
    ))
}

pub(in crate::stream) async fn decrypt(
    message: EncryptedMessage,
    key: Key,
) -> Result<(Vec<u8>, Key)> {
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&message.nonce[..12]);
    let mut plaintext = message.ciphertext.clone();

    let plaintext = key
        .0
        .open_in_place(
            Nonce::assume_unique_for_key(nonce.to_owned()),
            Aad::empty(),
            &mut plaintext,
        )
        .map_err(|_| Error::msg("failed to decrypt message"))?;

    debug!("decrypted {} bytes", plaintext.len());
    Ok((plaintext.to_vec(), key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_derive_key() {
        Runtime::new().unwrap().block_on(async {
            let key = derive_key(&[1, 2, 3], &[4, 5, 6]).await.unwrap();

            assert_eq!(key.0.algorithm(), &AES_256_GCM);
        });
    }
}
