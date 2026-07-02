//! Encryption helpers for local artifact material.

#![deny(missing_docs)]

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use zeroize::Zeroize;

/// Secret key bytes with redacted debug output.
pub struct SecretKey([u8; 32]);

impl SecretKey {
    /// Construct a [`SecretKey`] directly from a 32-byte AES-256 key.
    ///
    /// The bytes are used as-is — no KDF is applied. Use this when the caller
    /// already holds a uniform deployment sealing key (e.g. a config-provided
    /// secret) and must not re-hash it through Argon2.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> SecretKey {
        SecretKey(bytes)
    }

    /// Borrow raw key bytes.
    #[must_use]
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(***)")
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Encrypted payload with nonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedPayload {
    /// AES-GCM nonce.
    pub nonce: [u8; 12],
    /// Ciphertext bytes.
    pub ciphertext: Vec<u8>,
}

/// Derive a deterministic key from a secret and salt.
///
/// # Errors
/// Returns an error when key derivation fails.
pub fn derive_key(secret: &[u8], salt: &str) -> Result<SecretKey, CryptError> {
    let mut out = [0_u8; 32];
    Argon2::default()
        .hash_password_into(secret, salt.as_bytes(), &mut out)
        .map_err(|source| CryptError::KeyDerive {
            reason: source.to_string(),
        })?;
    Ok(SecretKey(out))
}

/// Encrypt bytes with AES-256-GCM.
///
/// # Errors
/// Returns an error when encryption fails.
pub fn encrypt(key: &SecretKey, plaintext: &[u8]) -> Result<EncryptedPayload, CryptError> {
    let cipher = Aes256Gcm::new_from_slice(key.expose()).map_err(|_| CryptError::InvalidKey)?;
    let mut nonce = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|_| CryptError::Encrypt)?;
    Ok(EncryptedPayload { nonce, ciphertext })
}

/// Decrypt bytes with AES-256-GCM.
///
/// # Errors
/// Returns an error when decryption fails.
pub fn decrypt(key: &SecretKey, payload: &EncryptedPayload) -> Result<Vec<u8>, CryptError> {
    let cipher = Aes256Gcm::new_from_slice(key.expose()).map_err(|_| CryptError::InvalidKey)?;
    cipher
        .decrypt(
            Nonce::from_slice(&payload.nonce),
            payload.ciphertext.as_slice(),
        )
        .map_err(|_| CryptError::Decrypt)
}

/// Cryptography helper errors.
#[derive(Debug, thiserror::Error)]
pub enum CryptError {
    /// Invalid key material.
    #[error("invalid key")]
    InvalidKey,
    /// Key derivation failed.
    #[error("key derivation failed: {reason}")]
    KeyDerive {
        /// Error source.
        reason: String,
    },
    /// Encryption failed.
    #[error("encryption failed")]
    Encrypt,
    /// Decryption failed.
    #[error("decryption failed")]
    Decrypt,
}

#[cfg(test)]
mod tests {
    use super::{SecretKey, decrypt, derive_key, encrypt};

    #[test]
    fn from_bytes_round_trip_and_redacted_debug() {
        let raw: [u8; 32] = [0x42_u8; 32];
        let key = SecretKey::from_bytes(raw);
        let payload = match encrypt(&key, b"sealing-key-payload") {
            Ok(payload) => payload,
            Err(error) => panic!("{error}"),
        };
        let plaintext = match decrypt(&key, &payload) {
            Ok(plaintext) => plaintext,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(plaintext, b"sealing-key-payload");
        assert_eq!(format!("{key:?}"), "SecretKey(***)");
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = match derive_key(b"correct horse battery staple", "workspace-salt") {
            Ok(key) => key,
            Err(error) => panic!("{error}"),
        };
        let payload = match encrypt(&key, b"payload") {
            Ok(payload) => payload,
            Err(error) => panic!("{error}"),
        };
        let plaintext = match decrypt(&key, &payload) {
            Ok(plaintext) => plaintext,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(plaintext, b"payload");
        assert_eq!(format!("{key:?}"), "SecretKey(***)");
    }
}
