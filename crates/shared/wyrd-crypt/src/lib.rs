//! Encryption helpers for local artifact material and envelope-encrypted secrets.
//!
//! [`seal`], [`open`], and [`rewrap`] implement envelope encryption: each
//! secret version gets a fresh random 256-bit data-encryption key (DEK) that
//! encrypts the secret, and the DEK is itself encrypted ("wrapped") under an
//! externally held key-encryption key (KEK). Both layers use AES-256-GCM and
//! authenticate the caller's canonical context under distinct domain tags, so
//! a ciphertext or wrapped key moved to another context fails to open.

#![deny(missing_docs)]

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use zeroize::{Zeroize, Zeroizing};

/// Domain tag authenticated with a secret encrypted under its DEK.
const SECRET_DOMAIN: &str = "wyrd.envelope.secret.v1";

/// Domain tag authenticated with a DEK wrapped under a KEK.
const DEK_DOMAIN: &str = "wyrd.envelope.dek.v1";

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
    encrypt_aad(key, plaintext, &[])
}

/// Decrypt bytes with AES-256-GCM.
///
/// # Errors
/// Returns an error when decryption fails.
pub fn decrypt(key: &SecretKey, payload: &EncryptedPayload) -> Result<Vec<u8>, CryptError> {
    decrypt_aad(key, payload, &[])
}

/// Encrypt bytes with AES-256-GCM under a fresh OS-random nonce, authenticating `aad`.
///
/// # Errors
/// Returns [`CryptError::InvalidKey`] or [`CryptError::Encrypt`].
pub fn encrypt_aad(
    key: &SecretKey,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<EncryptedPayload, CryptError> {
    let cipher = Aes256Gcm::new_from_slice(key.expose()).map_err(|_| CryptError::InvalidKey)?;
    let mut nonce = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad })
        .map_err(|_| CryptError::Encrypt)?;
    Ok(EncryptedPayload { nonce, ciphertext })
}

/// Decrypt AES-256-GCM bytes whose associated data must equal `aad`.
///
/// # Errors
/// Returns [`CryptError::Decrypt`] for a wrong key, tampered ciphertext, or
/// different associated data.
pub fn decrypt_aad(
    key: &SecretKey,
    payload: &EncryptedPayload,
    aad: &[u8],
) -> Result<Vec<u8>, CryptError> {
    let cipher = Aes256Gcm::new_from_slice(key.expose()).map_err(|_| CryptError::InvalidKey)?;
    cipher
        .decrypt(
            Nonce::from_slice(&payload.nonce),
            Payload {
                msg: payload.ciphertext.as_slice(),
                aad,
            },
        )
        .map_err(|_| CryptError::Decrypt)
}

/// One envelope-encrypted secret: the secret under its DEK, and the wrapped DEK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// The secret encrypted under the DEK.
    pub secret: EncryptedPayload,
    /// The DEK encrypted under the KEK.
    pub wrapped_dek: EncryptedPayload,
}

/// Envelope-encrypt `plaintext` under a fresh DEK wrapped by `kek`.
///
/// `context` is the caller's canonical identity of this secret version; it is
/// length-prefix encoded and authenticated by both layers under distinct
/// domain tags. The DEK is zeroized on return.
///
/// # Errors
/// Returns [`CryptError::Encrypt`] when either layer fails.
pub fn seal(kek: &SecretKey, plaintext: &[u8], context: &[&str]) -> Result<Envelope, CryptError> {
    let mut dek = [0_u8; 32];
    OsRng.fill_bytes(&mut dek);
    let dek = SecretKey(dek);
    Ok(Envelope {
        secret: encrypt_aad(&dek, plaintext, &canonical_aad(SECRET_DOMAIN, context))?,
        wrapped_dek: encrypt_aad(kek, dek.expose(), &canonical_aad(DEK_DOMAIN, context))?,
    })
}

/// Open an envelope sealed by [`seal`] with the same `kek` and `context`.
///
/// # Errors
/// Returns [`CryptError::Decrypt`] for a wrong KEK, different context, or
/// tampered material, and [`CryptError::InvalidKey`] for a malformed DEK.
pub fn open(
    kek: &SecretKey,
    envelope: &Envelope,
    context: &[&str],
) -> Result<Zeroizing<Vec<u8>>, CryptError> {
    let dek = unwrap_dek(kek, &envelope.wrapped_dek, context)?;
    decrypt_aad(&dek, &envelope.secret, &canonical_aad(SECRET_DOMAIN, context)).map(Zeroizing::new)
}

/// Re-wrap a DEK from `old_kek` to `new_kek` without decrypting the secret.
///
/// Only the DEK exists in memory, and it is zeroized on return.
///
/// # Errors
/// Returns [`CryptError::Decrypt`] when `old_kek` or `context` does not open
/// the wrapped DEK, and [`CryptError::Encrypt`] when re-wrapping fails.
pub fn rewrap(
    old_kek: &SecretKey,
    new_kek: &SecretKey,
    wrapped_dek: &EncryptedPayload,
    context: &[&str],
) -> Result<EncryptedPayload, CryptError> {
    let dek = unwrap_dek(old_kek, wrapped_dek, context)?;
    encrypt_aad(new_kek, dek.expose(), &canonical_aad(DEK_DOMAIN, context))
}

/// Decrypt a wrapped DEK into a zeroizing key.
///
/// # Errors
/// Returns [`CryptError::Decrypt`] or [`CryptError::InvalidKey`].
fn unwrap_dek(
    kek: &SecretKey,
    wrapped_dek: &EncryptedPayload,
    context: &[&str],
) -> Result<SecretKey, CryptError> {
    let bytes = Zeroizing::new(decrypt_aad(
        kek,
        wrapped_dek,
        &canonical_aad(DEK_DOMAIN, context),
    )?);
    let dek: [u8; 32] = bytes.as_slice().try_into().map_err(|_| CryptError::InvalidKey)?;
    Ok(SecretKey(dek))
}

/// Encode `domain` and `context` unambiguously: each part as a u32 big-endian
/// length followed by its bytes.
fn canonical_aad(domain: &str, context: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in std::iter::once(&domain).chain(context) {
        let len = u32::try_from(part.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(part.as_bytes());
    }
    out
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
    use super::{SecretKey, decrypt, derive_key, encrypt, open, rewrap, seal};

    /// Envelopes open only with the same KEK and context, and a rewrap moves
    /// the DEK to a new KEK without touching the secret ciphertext.
    #[test]
    fn envelope_binds_kek_and_context_and_rewraps() {
        let kek = SecretKey::from_bytes([1_u8; 32]);
        let next = SecretKey::from_bytes([2_u8; 32]);
        let context = ["tenant", "connection", "slack", "ops", "1"];
        let envelope = seal(&kek, b"xoxb-token", &context).expect("seals");
        assert_ne!(envelope.secret.ciphertext, b"xoxb-token");
        assert_eq!(
            open(&kek, &envelope, &context).expect("opens").as_slice(),
            b"xoxb-token"
        );
        assert!(open(&next, &envelope, &context).is_err(), "wrong KEK");
        for tampered in [
            ["other", "connection", "slack", "ops", "1"],
            ["tenant", "connection", "slack", "ops", "2"],
            ["tenantc", "onnection", "slack", "ops", "1"],
        ] {
            assert!(open(&kek, &envelope, &tampered).is_err(), "{tampered:?}");
        }
        let mut flipped = envelope.clone();
        flipped.secret.ciphertext[0] ^= 1;
        assert!(open(&kek, &flipped, &context).is_err(), "tampered ciphertext");

        let rewrapped = super::Envelope {
            secret: envelope.secret.clone(),
            wrapped_dek: rewrap(&kek, &next, &envelope.wrapped_dek, &context).expect("rewraps"),
        };
        assert_eq!(
            open(&next, &rewrapped, &context).expect("opens").as_slice(),
            b"xoxb-token"
        );
        assert!(open(&kek, &rewrapped, &context).is_err(), "old KEK retired");
    }

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
