//! File and explicit development-memory wrapping keys.
//!
//! Both hold 32-byte KEKs in process memory and wrap a DEK with AES-256-GCM,
//! authenticating the canonical wrap context. The wrapped blob is
//! `nonce (12) | ciphertext`; the envelope's key reference names the key.

use std::collections::HashMap;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use wyrd_crypt::{EncryptedPayload, SecretKey};
use zeroize::Zeroizing;

use crate::KmsError;

/// Local KEKs by logical reference.
pub(crate) struct LocalKeys {
    /// Every configured active and retained key.
    keys: HashMap<String, SecretKey>,
}

impl LocalKeys {
    /// Hold exactly `keys`.
    pub(crate) fn new(keys: HashMap<String, SecretKey>) -> Self {
        Self { keys }
    }

    /// Read each named key from its owner-only base64 file `<dir>/<name>`.
    ///
    /// Only the named files are read, so a key file left in the directory
    /// grants nothing unless configured.
    ///
    /// # Errors
    /// Returns [`KmsError::Config`] on `key_dir` when a file is missing,
    /// readable beyond its owner, oversized, not base64, or not 32 bytes.
    pub(crate) fn read<'a>(
        dir: &Path,
        names: impl Iterator<Item = &'a String>,
    ) -> Result<Self, KmsError> {
        let mut keys = HashMap::new();
        for name in names {
            let text = Zeroizing::new(wyrd_utils::secret::read_secret_file(&dir.join(name)).map_err(
                |_| KmsError::Config {
                    setting: "key_dir",
                    reason: "names a key file that is missing, unreadable, or not owner-only",
                },
            )?);
            let bytes = Zeroizing::new(STANDARD.decode(text.trim()).map_err(|_| {
                KmsError::Config {
                    setting: "key_dir",
                    reason: "names a key file that is not base64",
                }
            })?);
            let key: [u8; 32] = bytes.as_slice().try_into().map_err(|_| KmsError::Config {
                setting: "key_dir",
                reason: "names a key file that is not a 32-byte key",
            })?;
            keys.insert(name.clone(), SecretKey::from_bytes(key));
        }
        Ok(Self { keys })
    }

    /// The key named `key_ref`.
    ///
    /// # Errors
    /// Returns [`KmsError::KeyNotAllowed`] when it is not held.
    fn key(&self, key_ref: &str) -> Result<&SecretKey, KmsError> {
        self.keys.get(key_ref).ok_or(KmsError::KeyNotAllowed)
    }

    /// Wrap `dek` under `key_ref`, authenticating `aad`.
    ///
    /// # Errors
    /// Returns [`KmsError::KeyNotAllowed`] or [`KmsError::Encrypt`].
    pub(crate) fn wrap(
        &self,
        key_ref: &str,
        dek: &SecretKey,
        aad: &[u8],
    ) -> Result<Vec<u8>, KmsError> {
        let payload = wyrd_crypt::encrypt_aad(self.key(key_ref)?, dek.expose(), aad)
            .map_err(|_| KmsError::Encrypt)?;
        let mut out = payload.nonce.to_vec();
        out.extend_from_slice(&payload.ciphertext);
        Ok(out)
    }

    /// Unwrap a blob produced by [`Self::wrap`].
    ///
    /// # Errors
    /// Returns [`KmsError::KeyNotAllowed`], [`KmsError::UnknownFormat`] for a
    /// truncated blob, or [`KmsError::Authentication`].
    pub(crate) fn unwrap(
        &self,
        key_ref: &str,
        wrapped: &[u8],
        aad: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, KmsError> {
        let key = self.key(key_ref)?;
        let (nonce, ciphertext) = wrapped
            .split_at_checked(12)
            .ok_or(KmsError::UnknownFormat)?;
        let mut fixed = [0_u8; 12];
        fixed.copy_from_slice(nonce);
        let payload = EncryptedPayload {
            nonce: fixed,
            ciphertext: ciphertext.to_vec(),
        };
        wyrd_crypt::decrypt_aad(key, &payload, aad)
            .map(Zeroizing::new)
            .map_err(|_| KmsError::Authentication)
    }
}
