//! Encryption helpers for local artifact material.

#![deny(missing_docs)]

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use sha2::{Digest, Sha256};
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

/// Magic prefix marking a versioned sealed value produced by [`SealingKeyring`].
const SEALED_MAGIC: &[u8; 4] = b"wsk1";
/// Byte length of a sealing key identifier.
const KEY_ID_LEN: usize = 8;
/// Byte length of an AES-GCM nonce.
const NONCE_LEN: usize = 12;

/// Stable, non-secret identifier of one sealing key.
///
/// The identifier is the first eight bytes of a domain-separated SHA-256
/// fingerprint of the key, so every replica holding the same key derives the
/// same identifier without configuring one. It is safe to log and to store
/// beside ciphertext; it reveals nothing usable about the key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SealingKeyId([u8; KEY_ID_LEN]);

impl SealingKeyId {
    /// Derive the identifier of `key` from its domain-separated fingerprint.
    #[must_use]
    pub fn of(key: &SecretKey) -> Self {
        let digest = Sha256::new()
            .chain_update(b"wyrd-sealing-key-id/v1")
            .chain_update(key.expose())
            .finalize();
        let mut id = [0_u8; KEY_ID_LEN];
        id.copy_from_slice(&digest[..KEY_ID_LEN]);
        Self(id)
    }
}

impl std::fmt::Debug for SealingKeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for SealingKeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Deployment sealing keys for secrets stored at rest: one write key plus any
/// retained keys still needed to open older ciphertext.
///
/// [`Self::seal`] always writes a versioned value, `"wsk1" ‖ key_id(8) ‖
/// nonce(12) ‖ ciphertext`, under the write key. [`Self::open`] selects the key
/// named by the stored identifier; a value without the versioned prefix is the
/// earlier unversioned `nonce ‖ ciphertext` form and is tried against every
/// held key. Rotation therefore works as: add the new key as retained on every
/// replica, switch the write key while keeping the old one retained, rewrap
/// every stored value (see [`Self::needs_rewrap`]), and retire the old key only
/// once no stored value references it.
pub struct SealingKeyring {
    /// Key every new value is sealed under, with its identifier.
    write: (SealingKeyId, SecretKey),
    /// Keys kept only to open values sealed before the latest rotation.
    retained: Vec<(SealingKeyId, SecretKey)>,
}

impl SealingKeyring {
    /// Build a keyring whose write key is `write` and which retains no older key.
    #[must_use]
    pub fn new(write: SecretKey) -> Self {
        Self {
            write: (SealingKeyId::of(&write), write),
            retained: Vec::new(),
        }
    }

    /// Retain `key` so values sealed under it can still be opened and rewrapped.
    ///
    /// Retaining the current write key, or the same key twice, is a no-op.
    #[must_use]
    pub fn with_retained(mut self, key: SecretKey) -> Self {
        let id = SealingKeyId::of(&key);
        if id != self.write.0 && self.retained.iter().all(|(held, _)| *held != id) {
            self.retained.push((id, key));
        }
        self
    }

    /// Identifier of the key new values are sealed under.
    #[must_use]
    pub fn write_key_id(&self) -> SealingKeyId {
        self.write.0
    }

    /// Identifiers of the retained (read-only) keys, in configuration order.
    #[must_use]
    pub fn retained_key_ids(&self) -> Vec<SealingKeyId> {
        self.retained.iter().map(|(id, _)| *id).collect()
    }

    /// Seal `plaintext` under the write key into the versioned stored form.
    ///
    /// # Errors
    /// Returns [`CryptError::Encrypt`] when AES-GCM encryption fails.
    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptError> {
        let payload = encrypt(&self.write.1, plaintext)?;
        let mut out = Vec::with_capacity(
            SEALED_MAGIC.len() + KEY_ID_LEN + NONCE_LEN + payload.ciphertext.len(),
        );
        out.extend_from_slice(SEALED_MAGIC);
        out.extend_from_slice(&self.write.0.0);
        out.extend_from_slice(&payload.nonce);
        out.extend_from_slice(&payload.ciphertext);
        Ok(out)
    }

    /// Open a stored value sealed by this keyring or its unversioned predecessor.
    ///
    /// # Errors
    /// Returns [`CryptError::Decrypt`] when the value is malformed, names a key
    /// this keyring does not hold, or fails authentication under every
    /// candidate key.
    pub fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, CryptError> {
        if let Some((id, payload)) = Self::split_versioned(sealed)
            && let Some(key) = self.key(id)
        {
            return decrypt(key, &payload);
        }
        let payload = Self::split_legacy(sealed).ok_or(CryptError::Decrypt)?;
        self.keys()
            .find_map(|key| decrypt(key, &payload).ok())
            .ok_or(CryptError::Decrypt)
    }

    /// Whether `sealed` must be rewrapped to reference only the write key.
    ///
    /// True for unversioned values and for values sealed under any key other
    /// than the current write key.
    #[must_use]
    pub fn needs_rewrap(&self, sealed: &[u8]) -> bool {
        Self::split_versioned(sealed).is_none_or(|(id, _)| id != self.write.0)
    }

    /// Open `sealed` and reseal it under the write key when it is not already.
    ///
    /// Returns `Ok(None)` when the value already references the write key.
    ///
    /// # Errors
    /// Returns [`CryptError::Decrypt`] when no held key opens the value and
    /// [`CryptError::Encrypt`] when resealing fails.
    pub fn rewrap(&self, sealed: &[u8]) -> Result<Option<Vec<u8>>, CryptError> {
        if !self.needs_rewrap(sealed) {
            return Ok(None);
        }
        let mut plaintext = self.open(sealed)?;
        let resealed = self.seal(&plaintext);
        plaintext.zeroize();
        resealed.map(Some)
    }

    /// Key named by `id`, if this keyring holds it.
    fn key(&self, id: SealingKeyId) -> Option<&SecretKey> {
        self.keys_with_ids()
            .find(|(held, _)| *held == id)
            .map(|(_, key)| key)
    }

    /// Every held key, write key first.
    fn keys(&self) -> impl Iterator<Item = &SecretKey> {
        self.keys_with_ids().map(|(_, key)| key)
    }

    /// Every held key with its identifier, write key first.
    fn keys_with_ids(&self) -> impl Iterator<Item = (SealingKeyId, &SecretKey)> {
        std::iter::once((self.write.0, &self.write.1))
            .chain(self.retained.iter().map(|(id, key)| (*id, key)))
    }

    /// Split a versioned stored value into its key identifier and payload.
    fn split_versioned(sealed: &[u8]) -> Option<(SealingKeyId, EncryptedPayload)> {
        let rest = sealed.strip_prefix(SEALED_MAGIC.as_slice())?;
        if rest.len() < KEY_ID_LEN + NONCE_LEN {
            return None;
        }
        let (id_bytes, rest) = rest.split_at(KEY_ID_LEN);
        let mut id = [0_u8; KEY_ID_LEN];
        id.copy_from_slice(id_bytes);
        Some((SealingKeyId(id), Self::split_legacy(rest)?))
    }

    /// Split an unversioned `nonce ‖ ciphertext` value.
    fn split_legacy(sealed: &[u8]) -> Option<EncryptedPayload> {
        if sealed.len() < NONCE_LEN {
            return None;
        }
        let (nonce_bytes, ciphertext) = sealed.split_at(NONCE_LEN);
        let mut nonce = [0_u8; NONCE_LEN];
        nonce.copy_from_slice(nonce_bytes);
        Some(EncryptedPayload {
            nonce,
            ciphertext: ciphertext.to_vec(),
        })
    }
}

impl std::fmt::Debug for SealingKeyring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealingKeyring")
            .field("write_key_id", &self.write.0)
            .field("retained_key_ids", &self.retained_key_ids())
            .finish()
    }
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
    use super::{SealingKeyId, SealingKeyring, SecretKey, decrypt, derive_key, encrypt};

    /// Seal `plaintext` in the unversioned `nonce ‖ ciphertext` form that
    /// predates the keyring, as rows written before rotation support hold.
    fn legacy_seal(key: &SecretKey, plaintext: &[u8]) -> Vec<u8> {
        let payload = encrypt(key, plaintext).expect("legacy seal encrypts");
        let mut out = payload.nonce.to_vec();
        out.extend_from_slice(&payload.ciphertext);
        out
    }

    /// Rotation walks old key -> retained -> rewrapped -> old key retired, and
    /// every stage keeps the stored secret readable.
    #[test]
    fn keyring_rotation_rewraps_and_retires_the_old_key() {
        let old = SealingKeyring::new(SecretKey::from_bytes([1_u8; 32]));
        let sealed_old = old.seal(b"client-secret").expect("seal");
        let legacy = legacy_seal(&SecretKey::from_bytes([1_u8; 32]), b"legacy-secret");

        let rotated = SealingKeyring::new(SecretKey::from_bytes([2_u8; 32]))
            .with_retained(SecretKey::from_bytes([1_u8; 32]));
        assert_eq!(
            rotated.open(&sealed_old).expect("retained opens"),
            b"client-secret"
        );
        assert_eq!(
            rotated.open(&legacy).expect("legacy opens"),
            b"legacy-secret"
        );
        assert!(rotated.needs_rewrap(&sealed_old));
        assert!(rotated.needs_rewrap(&legacy));

        let rewrapped = rotated
            .rewrap(&sealed_old)
            .expect("rewrap")
            .expect("changed");
        let rewrapped_legacy = rotated.rewrap(&legacy).expect("rewrap").expect("changed");
        assert!(!rotated.needs_rewrap(&rewrapped));
        assert_eq!(rotated.rewrap(&rewrapped).expect("rewrap"), None);

        let retired = SealingKeyring::new(SecretKey::from_bytes([2_u8; 32]));
        assert_eq!(
            retired.open(&rewrapped).expect("new key opens"),
            b"client-secret"
        );
        assert_eq!(
            retired.open(&rewrapped_legacy).expect("new key opens"),
            b"legacy-secret"
        );
        assert!(
            retired.open(&sealed_old).is_err(),
            "retired key no longer opens"
        );
    }

    /// Key identifiers are deterministic, key-specific, and the keyring Debug
    /// output names identifiers only.
    #[test]
    fn keyring_ids_are_stable_and_debug_is_redacted() {
        let a = SealingKeyId::of(&SecretKey::from_bytes([5_u8; 32]));
        assert_eq!(a, SealingKeyId::of(&SecretKey::from_bytes([5_u8; 32])));
        assert_ne!(a, SealingKeyId::of(&SecretKey::from_bytes([6_u8; 32])));
        assert_eq!(a.to_string().len(), 16);
        let ring = SealingKeyring::new(SecretKey::from_bytes([5_u8; 32]))
            .with_retained(SecretKey::from_bytes([5_u8; 32]));
        assert!(
            ring.retained_key_ids().is_empty(),
            "write key is not retained twice"
        );
        assert!(format!("{ring:?}").contains(&a.to_string()));
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
