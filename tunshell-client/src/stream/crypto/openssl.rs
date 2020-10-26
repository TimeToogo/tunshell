use crate::stream::aes_stream::EncryptedMessage;
use anyhow::{bail, Context, Result};
use log::*;
use openssl::rand::rand_bytes;
use openssl::symm;
use openssl::{hash::MessageDigest, pkcs5::pbkdf2_hmac};

pub struct Key(Vec<u8>);

impl Clone for Key {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

pub(in crate::stream) async fn derive_key(salt: &[u8], key: &[u8]) -> Result<Key> {
    let mut derived_key = [0u8; 32];

    pbkdf2_hmac(
        key,
        salt,
        1000,
        MessageDigest::sha256(),
        &mut derived_key[..],
    )
    .context("failed to derive key")?;

    Ok(Key(derived_key.to_vec()))
}

pub(in crate::stream) async fn encrypt(
    plaintext: Vec<u8>,
    key: Key,
) -> Result<(EncryptedMessage, Key)> {
    let mut nonce = [0u8; 12];
    rand_bytes(&mut nonce[..]).context("failed to generate random bytes")?;

    let mut tag = [0u8; 16];
    let mut ciphertext = symm::encrypt_aead(
        symm::Cipher::aes_256_gcm(),
        key.0.as_slice(),
        Some(&nonce[..]),
        &[],
        plaintext.as_slice(),
        &mut tag[..],
    )
    .context("failed to init openssl crypto")?;

    // append tag to end of ciphertext (as per ring)
    ciphertext.extend_from_slice(&tag[..]);

    debug!("encrypted {} bytes", plaintext.len());
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
    // Separate out tag from ciphertext
    if message.ciphertext.len() < 16 {
        bail!("encrypted message cannot be less than 16 bytes in length");
    }

    let ciphertext = &message.ciphertext[..message.ciphertext.len() - 16];
    let tag = &message.ciphertext[message.ciphertext.len() - 16..];

    let plaintext = symm::decrypt_aead(
        symm::Cipher::aes_256_gcm(),
        key.0.as_slice(),
        Some(message.nonce.as_slice()),
        &[],
        ciphertext,
        tag,
    )
    .context("failed to decrypt data")?;

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

            assert_eq!(key.0.len(), 32);
        });
    }
}
