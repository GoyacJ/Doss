use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::prelude::*;
use sha2::{Digest, Sha256};

use crate::core::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub(crate) struct FieldCipher {
    key: [u8; 32],
}

impl FieldCipher {
    pub(crate) fn from_seed(seed: &str) -> Self {
        let digest = Sha256::digest(seed.as_bytes());
        let mut key = [0_u8; 32];
        key.copy_from_slice(&digest[..32]);
        Self { key }
    }

    pub(crate) fn encrypt(&self, plaintext: &str) -> AppResult<String> {
        if plaintext.is_empty() {
            return Ok(String::new());
        }

        let nonce_bytes: [u8; 12] = rand::random();
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|_| AppError::Crypto)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let encrypted = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| AppError::Crypto)?;

        let nonce_text = BASE64_STANDARD.encode(nonce_bytes);
        let encrypted_text = BASE64_STANDARD.encode(encrypted);
        Ok(format!("{nonce_text}:{encrypted_text}"))
    }

    pub(crate) fn decrypt(&self, ciphertext: &str) -> AppResult<String> {
        if ciphertext.is_empty() {
            return Ok(String::new());
        }

        let mut parts = ciphertext.split(':');
        let nonce_part = parts.next().ok_or(AppError::Crypto)?;
        let data_part = parts.next().ok_or(AppError::Crypto)?;
        if parts.next().is_some() {
            return Err(AppError::Crypto);
        }

        let nonce_vec = BASE64_STANDARD.decode(nonce_part)?;
        let data = BASE64_STANDARD.decode(data_part)?;
        if nonce_vec.len() != 12 {
            return Err(AppError::Crypto);
        }

        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|_| AppError::Crypto)?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce_vec), data.as_ref())
            .map_err(|_| AppError::Crypto)?;
        String::from_utf8(plaintext).map_err(|_| AppError::Crypto)
    }
}
