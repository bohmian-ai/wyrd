//! Encryption primitives, secret buffers, and HMAC helpers.

#![deny(missing_docs)]

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use hmac::Mac as _;
use sha2::Sha256;
use zeroize::{Zeroize, Zeroizing};

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

    /// Generate a fresh AES-256 key from the operating system's
    /// cryptographic random source.
    ///
    /// Envelope owners call this once per stored value to obtain an
    /// independent data-encryption key; the bytes are zeroized on drop.
    #[must_use]
    pub fn generate() -> SecretKey {
        let mut bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut bytes);
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

/// Owned secret bytes — a plaintext key, or a request or answer body that
/// carries one — with redacted debug output, zeroized on drop.
///
/// Growth never reallocates in place: appending past the capacity moves the
/// bytes into a larger buffer and zeroizes the old one, so no unprotected
/// copy is left behind. Writing JSON into it with `serde_json::to_writer`
/// therefore keeps the whole serialized body protected.
#[derive(Default)]
pub struct SecretBytes(Zeroizing<Vec<u8>>);

impl SecretBytes {
    /// An empty buffer that holds `capacity` bytes before it grows.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Zeroizing::new(Vec::with_capacity(capacity)))
    }

    /// Append `bytes`, moving into a buffer of at least twice the capacity
    /// when they do not fit.
    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        let needed = self.0.len() + bytes.len();
        if needed > self.0.capacity() {
            let mut grown = Zeroizing::new(Vec::with_capacity(needed.max(self.0.capacity() * 2)));
            grown.extend_from_slice(&self.0);
            self.0 = grown;
        }
        self.0.extend_from_slice(bytes);
    }
}

impl From<Zeroizing<Vec<u8>>> for SecretBytes {
    fn from(bytes: Zeroizing<Vec<u8>>) -> Self {
        Self(bytes)
    }
}

impl std::ops::Deref for SecretBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for SecretBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl std::io::Write for SecretBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl std::fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretBytes([REDACTED])")
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
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
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

/// Encode `domain` and `context` unambiguously: each part as a u32 big-endian
/// length followed by its bytes.
///
/// Envelope owners authenticate this encoding as AES-GCM associated data, or
/// hand it to a remote wrapping provider as its authenticated context, so the
/// same identity always produces the same bytes and no two part lists collide.
#[must_use]
pub fn canonical_aad(domain: &str, context: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in std::iter::once(&domain).chain(context) {
        let len = u32::try_from(part.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(part.as_bytes());
    }
    out
}

/// HMAC-SHA256 (RFC 2104) of `message` under `key`.
///
/// Authenticates provider deliveries signed with a shared secret, such as a
/// GitHub webhook's `X-Hub-Signature-256`. Any key length is accepted, as
/// the RFC allows.
#[must_use]
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    hmac_of(key, message).finalize().into_bytes().into()
}

/// Whether `tag` is the full HMAC-SHA256 of `message` under `key`, compared
/// in constant time so a forger learns nothing from how long refusal takes.
/// A truncated tag is refused.
#[must_use]
pub fn verify_hmac_sha256(key: &[u8], message: &[u8], tag: &[u8]) -> bool {
    hmac_of(key, message).verify_slice(tag).is_ok()
}

/// The keyed HMAC-SHA256 state after absorbing `message`.
///
/// # Panics
/// Never: HMAC accepts a key of any length.
fn hmac_of(key: &[u8], message: &[u8]) -> hmac::Hmac<Sha256> {
    let mut mac = <hmac::Hmac<Sha256> as hmac::Mac>::new_from_slice(key)
        .expect("invariant: HMAC accepts a key of any length");
    mac.update(message);
    mac
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
    use super::{
        SecretBytes, SecretKey, decrypt, derive_key, encrypt, hmac_sha256, verify_hmac_sha256,
    };

    /// Secret bytes keep their content across growth, which moves them into
    /// a fresh larger buffer instead of reallocating in place, accept JSON
    /// through `Write`, and never print their content.
    ///
    /// # Panics
    /// Panics when content, growth, or redaction differs.
    #[test]
    fn secret_bytes_grow_by_moving_and_redact_debug() {
        let mut bytes = SecretBytes::with_capacity(4);
        bytes.extend_from_slice(b"abcd");
        let before = bytes.as_ptr();
        bytes.extend_from_slice(b"e");
        assert_eq!(&bytes[..], b"abcde");
        assert_ne!(bytes.as_ptr(), before, "grown into a new buffer");

        let mut json = SecretBytes::default();
        std::io::Write::write_all(&mut json, br#"{"plaintext":"c2stbGl2ZQ=="}"#).expect("writes");
        assert_eq!(&json[..], br#"{"plaintext":"c2stbGl2ZQ=="}"#);
        assert_eq!(format!("{json:?}"), "SecretBytes([REDACTED])");
    }

    /// HMAC-SHA256 matches the RFC 4231 vectors for a short key (case 2) and
    /// a key longer than one block (case 6), and verification refuses a
    /// changed message or a truncated tag.
    ///
    /// # Panics
    /// Panics when a computed tag differs from the published one.
    #[test]
    fn hmac_sha256_matches_rfc_4231() {
        let short = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&short),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        let long = hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First",
        );
        assert_eq!(
            hex(&long),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
        assert!(verify_hmac_sha256(
            b"Jefe",
            b"what do ya want for nothing?",
            &short
        ));
        assert!(!verify_hmac_sha256(
            b"Jefe",
            b"what do ya want for nothing!",
            &short
        ));
        assert!(!verify_hmac_sha256(
            b"Jefe",
            b"what do ya want for nothing?",
            &short[..31]
        ));
        let mut wrong = short;
        wrong[0] ^= 1;
        assert!(!verify_hmac_sha256(
            b"Jefe",
            b"what do ya want for nothing?",
            &wrong
        ));
    }

    /// Lowercase hex of `bytes`.
    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        bytes.iter().fold(String::new(), |mut out, byte| {
            write!(out, "{byte:02x}").expect("writing to a String cannot fail");
            out
        })
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
