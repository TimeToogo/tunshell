use crate::stream::aes_stream::EncryptedMessage;
use anyhow::{Error, Result};
use log::*;
use ring::aead::*;
use ring::pbkdf2::*;
use ring::rand::{SecureRandom, SystemRandom};

pub struct Key(LessSafeKey);

pub(in crate::stream) fn derive_key(salt: &[u8], key: &[u8]) -> Key {
    let mut derived_key = [0u8; 32];
    derive(
        PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(1000).unwrap(),
        salt,
        key,
        &mut derived_key,
    );

    Key(LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, &derived_key).unwrap(),
    ))
}

pub(in crate::stream) fn encrypt(plaintext: &[u8], key: &Key) -> EncryptedMessage {
    let mut nonce = [0u8; 12];
    let rand = SystemRandom::new();
    rand.fill(&mut nonce).unwrap();

    let mut ciphertext = plaintext.to_vec();
    key.0
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce.to_owned()),
            Aad::empty(),
            &mut ciphertext,
        )
        .unwrap();

    debug!("encrypted {} bytes", plaintext.len());
    EncryptedMessage {
        nonce: nonce.to_vec(),
        ciphertext,
    }
}

pub(in crate::stream) fn decrypt(message: EncryptedMessage, key: &Key) -> Result<Vec<u8>> {
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
    Ok(plaintext.to_vec())
}
