use super::super::EncryptedMessage;
use anyhow::{Error, Result};

pub struct Key {
    buff: Vec<u8>
}

pub(in crate::stream) fn derive_key(salt: &[u8], key: &[u8]) -> Key {
    todo!()
}

pub(in crate::stream) fn encrypt(plaintext: &[u8], key: &Key) -> EncryptedMessage {
    todo!()
}

pub(in crate::stream) fn decrypt(message: EncryptedMessage, key: &Key) -> Result<Vec<u8>> {
    todo!()
}
