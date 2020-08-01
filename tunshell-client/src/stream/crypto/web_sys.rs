use super::super::EncryptedMessage;
use anyhow::{Error, Result};
use futures::Future;
use js_sys::{Array, JsString, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AesDerivedKeyParams, AesGcmParams, Crypto, CryptoKey, Pbkdf2Params};

#[derive(Clone)]
pub struct Key(CryptoKey);

// Our wasm target runs in a single-threaded environment so the non-Send
// restriction would require large amounts of conditional compilation and code
// duplication. Will revisit if multi-threaded wasm envs are targetted.
unsafe impl Send for Key {}
struct SendJsFuture(JsFuture);
unsafe impl Send for SendJsFuture {}
impl Future for SendJsFuture {
    type Output = std::result::Result<JsValue, JsValue>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::pin::Pin::new(&mut self.0).poll(cx)
    }
}

fn crypto() -> Result<Crypto> {
    Ok(web_sys::window()
        .ok_or_else(|| Error::msg("could not get 'window' reference"))?
        .crypto()
        .map_err(|_| Error::msg("could not get 'window.crypto' reference"))?)
}

async fn import_key(key: &[u8]) -> Result<CryptoKey> {
    let key_material = {
        let promise = crypto()?
            .subtle()
            .import_key_with_str(
                "raw",
                &Uint8Array::from(key),
                "PBKDF2",
                false,
                &Array::of2(&JsString::from("deriveBits"), &JsString::from("deriveKey")),
            )
            .map_err(|err| Error::msg(format!("failed to import key: {:?}", err)))?;

        JsFuture::from(promise)
    };

    let key_material = CryptoKey::unchecked_from_js(
        key_material
            .await
            .map_err(|err| Error::msg(format!("failed to import key: {:?}", err)))?,
    );

    Ok(key_material)
}

pub(in crate::stream) async fn derive_key(salt: &[u8], key: &[u8]) -> Result<Key> {
    let crypto = crypto()?.subtle();

    let key_material = import_key(key).await?;

    let derived_key = crypto
        .derive_key_with_object_and_object(
            &Pbkdf2Params::new(
                "PBKDF2",
                &JsString::from("SHA-256"),
                1000,
                &Uint8Array::from(salt),
            ),
            &key_material,
            &AesDerivedKeyParams::new("AES-GCM", 256),
            true,
            &Array::of2(&JsString::from("encrypt"), &JsString::from("decrypt")),
        )
        .map_err(|err| Error::msg(format!("failed to derive key: {:?}", err)))?;

    let derived_key = CryptoKey::unchecked_from_js(
        JsFuture::from(derived_key)
            .await
            .map_err(|err| Error::msg(format!("failed to derive key: {:?}", err)))?,
    );

    Ok(Key(derived_key))
}

fn generate_nonce(len: usize) -> Result<Vec<u8>> {
    let crypto = crypto()?;
    let mut buff = vec![0u8; len];

    crypto
        .get_random_values_with_u8_array(&mut buff[..len])
        .map_err(|err| Error::msg(format!("failed to generate nonce: {:?}", err)))?;

    Ok(buff)
}

pub(in crate::stream) async fn encrypt(
    mut plaintext: Vec<u8>,
    key: Key,
) -> Result<(EncryptedMessage, Key)> {
    let iv = generate_nonce(12)?;

    let encrypted = SendJsFuture({
        let iv: Uint8Array = iv.as_slice().into();

        let promise = crypto()?
            .subtle()
            .encrypt_with_object_and_u8_array(
                &AesGcmParams::new("AES-GCM", &iv),
                &key.0,
                &mut plaintext[..],
            )
            .map_err(|err| Error::msg(format!("failed to encrypt data: {:?}", err)))?;

        JsFuture::from(promise)
    });

    let encrypted = Uint8Array::new(
        &encrypted
            .await
            .map_err(|err| Error::msg(format!("failed to encrypt data: {:?}", err)))?,
    );

    Ok((
        EncryptedMessage {
            ciphertext: encrypted.to_vec(),
            nonce: iv,
        },
        key,
    ))
}

pub(in crate::stream) async fn decrypt(
    mut message: EncryptedMessage,
    key: Key,
) -> Result<(Vec<u8>, Key)> {
    let decrypted = SendJsFuture({
        let iv: Uint8Array = message.nonce.as_slice().into();

        let promise = crypto()?
            .subtle()
            .decrypt_with_object_and_u8_array(
                &AesGcmParams::new("AES-GCM", &iv),
                &key.0,
                &mut message.ciphertext[..],
            )
            .map_err(|err| Error::msg(format!("failed to encrypt data: {:?}", err)))?;

        JsFuture::from(promise)
    });

    let decrypted = Uint8Array::new(
        &decrypted
            .await
            .map_err(|err| Error::msg(format!("failed to decrypt data: {:?}", err)))?,
    );

    Ok((decrypted.to_vec(), key))
}
