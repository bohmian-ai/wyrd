//! The platform derivation key behind signup and sign-in state.
//!
//! Signup and sign-in flow cookies, email proofs and the keyed pseudonyms
//! that admission windows and creator accounting store all derive their keys
//! from one random 32-byte platform key. It is generated once, stored only as
//! a `WyrdKms` envelope in `platform.derivation_key`, and opened at most once
//! per process. Stored-secret maintenance rewraps that envelope when the
//! wrapping key rotates; the key itself never changes, so pseudonyms recorded
//! before a rotation still match after it. Each use derives its own subkey
//! under a distinct fixed label, so no two uses share key material.
//!
//! Without `[kms]`, or while the key cannot be read or opened, every use
//! refuses with [`WyrdError::AuthVerifyUnavailable`]; nothing falls back to
//! plain state.

use std::sync::Arc;

use serde_json::json;
use tokio::sync::OnceCell;
use wyrd_crypt::{EncryptedPayload, SecretKey};
use wyrd_kms::{SecretContext, SecretPurpose, SecretScope, WyrdKms};
use wyrd_spec::error::WyrdError;
use wyrd_sql::OperatorPool;
use wyrd_sql::queries::platform::social::{derivation_key, offer_derivation_key};

use crate::error::store_error;

/// The envelope context of the one platform derivation key, which
/// stored-secret maintenance rebuilds from the same fixed owner.
const CONTEXT: SecretContext<'static> = SecretContext {
    scope: SecretScope::Platform,
    purpose: SecretPurpose::PlatformDerivationKey,
    owner: "platform_derivation_key",
    authority: &[],
    version: None,
};

/// Byte length of an AES-GCM nonce in a sealed value.
const NONCE_LEN: usize = 12;

/// The deployment's platform derivation key, loaded on first use.
pub struct PlatformKey {
    /// Operator pool; `platform.*` admits only operator sessions.
    operator: OperatorPool,
    /// Stored-secret facade that wraps the key; `None` refuses every use.
    kms: Option<Arc<WyrdKms>>,
    /// The opened key, once a load succeeded; a failed load is retried.
    key: OnceCell<Derivation>,
}

impl PlatformKey {
    /// Build the owner over the operator pool and stored-secret facade.
    #[must_use]
    pub fn new(operator: OperatorPool, kms: Option<Arc<WyrdKms>>) -> Self {
        Self {
            operator,
            kms,
            key: OnceCell::new(),
        }
    }

    /// The keyed pseudonym of `value` within `scope`, which a store keeps
    /// instead of the value.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the key is
    /// unavailable.
    pub async fn pseudonym(&self, scope: &str, value: &[u8]) -> Result<[u8; 32], WyrdError> {
        Ok(self.key().await?.pseudonym(scope, value))
    }

    /// Seal `plaintext` for use `label` as `nonce ‖ ciphertext`.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the key is
    /// unavailable and [`WyrdError::Internal`] when encryption fails.
    pub async fn seal(&self, label: &str, plaintext: &[u8]) -> Result<Vec<u8>, WyrdError> {
        self.key().await?.seal(label, plaintext)
    }

    /// Open a value [`Self::seal`] sealed for `label`; `None` for anything
    /// else, so a tampered or foreign value proves nothing.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the key is
    /// unavailable.
    pub async fn open(&self, label: &str, sealed: &[u8]) -> Result<Option<Vec<u8>>, WyrdError> {
        Ok(self.key().await?.open(label, sealed))
    }

    /// The opened key, loading or creating it on first use.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] without `[kms]` or when
    /// the store or wrapping backend fails.
    async fn key(&self) -> Result<&Derivation, WyrdError> {
        self.key.get_or_try_init(|| self.load()).await
    }

    /// Read the stored key, or store a fresh one when none exists, and open
    /// whichever envelope the store holds afterwards.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] without `[kms]` or when
    /// the store or wrapping backend fails.
    async fn load(&self) -> Result<Derivation, WyrdError> {
        let kms = self
            .kms
            .as_deref()
            .ok_or_else(|| unavailable("kms_unconfigured"))?;
        let envelope =
            if let Some(envelope) = derivation_key(&self.operator).await.map_err(store_error)? {
                envelope
            } else {
                let fresh = SecretKey::generate();
                let offered = kms.seal(&CONTEXT, fresh.expose()).await.map_err(|error| {
                    tracing::warn!(%error, "platform derivation key could not be sealed");
                    unavailable("derivation_key_unavailable")
                })?;
                offer_derivation_key(&self.operator, &offered)
                    .await
                    .map_err(store_error)?
            };
        let opened = kms.open(&CONTEXT, &envelope).await.map_err(|error| {
            tracing::warn!(%error, "platform derivation key could not be opened");
            unavailable("derivation_key_unavailable")
        })?;
        let bytes: [u8; 32] = opened
            .as_slice()
            .try_into()
            .map_err(|_| unavailable("derivation_key_unavailable"))?;
        Ok(Derivation(SecretKey::from_bytes(bytes)))
    }
}

impl std::fmt::Debug for PlatformKey {
    /// Redacting debug: prints only whether the key has been loaded.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformKey")
            .field("loaded", &self.key.initialized())
            .finish_non_exhaustive()
    }
}

/// The opened platform key and the per-use derivations made from it.
#[derive(Debug)]
struct Derivation(SecretKey);

impl Derivation {
    /// The subkey for use `label`, an HMAC-SHA-256 of the label under the
    /// platform key.
    fn subkey(&self, label: &str) -> SecretKey {
        SecretKey::from_bytes(wyrd_crypt::hmac_sha256(
            self.0.expose(),
            &wyrd_crypt::canonical_aad("wyrd.derive.v1", &[label]),
        ))
    }

    /// The HMAC-SHA-256 pseudonym of `value` within `scope`; the scope is
    /// length-delimited, so one value in two scopes gives unrelated
    /// pseudonyms.
    fn pseudonym(&self, scope: &str, value: &[u8]) -> [u8; 32] {
        let mut message = wyrd_crypt::canonical_aad("wyrd.pseudonym.v1", &[scope]);
        message.extend_from_slice(value);
        wyrd_crypt::hmac_sha256(self.subkey("pseudonym").expose(), &message)
    }

    /// Seal `plaintext` under `label`'s subkey, authenticating the label.
    ///
    /// # Errors
    /// Returns [`WyrdError::Internal`] when encryption fails.
    fn seal(&self, label: &str, plaintext: &[u8]) -> Result<Vec<u8>, WyrdError> {
        let payload = wyrd_crypt::encrypt_aad(
            &self.subkey(label),
            plaintext,
            &wyrd_crypt::canonical_aad("wyrd.sealed.v1", &[label]),
        )
        .map_err(|error| WyrdError::Internal {
            message: format!("sealing failed: {error}"),
            details: json!({}),
        })?;
        let mut out = Vec::with_capacity(NONCE_LEN + payload.ciphertext.len());
        out.extend_from_slice(&payload.nonce);
        out.extend_from_slice(&payload.ciphertext);
        Ok(out)
    }

    /// Open a value [`Self::seal`] sealed under `label`.
    fn open(&self, label: &str, sealed: &[u8]) -> Option<Vec<u8>> {
        let (nonce, ciphertext) = sealed.split_first_chunk::<NONCE_LEN>()?;
        wyrd_crypt::decrypt_aad(
            &self.subkey(label),
            &EncryptedPayload {
                nonce: *nonce,
                ciphertext: ciphertext.to_vec(),
            },
            &wyrd_crypt::canonical_aad("wyrd.sealed.v1", &[label]),
        )
        .ok()
    }
}

/// Signup and sign-in state refused because the derivation key is
/// unavailable for `reason`.
fn unavailable(reason: &str) -> WyrdError {
    WyrdError::AuthVerifyUnavailable {
        message: "signup and social sign-in are unavailable right now; try again".to_owned(),
        details: json!({ "reason": reason }),
    }
}

#[cfg(test)]
mod tests {
    use super::{Derivation, SecretKey};

    /// A pseudonym is stable per key and value, and separated by scope and
    /// key.
    #[test]
    fn pseudonyms_separate_scope_and_key() {
        let key = Derivation(SecretKey::from_bytes([7; 32]));
        let other = Derivation(SecretKey::from_bytes([8; 32]));
        let address = key.pseudonym("source", b"198.51.100.7");
        assert_eq!(address, key.pseudonym("source", b"198.51.100.7"));
        assert_ne!(address, key.pseudonym("start", b"198.51.100.7"));
        assert_ne!(address, key.pseudonym("sourc", b"e198.51.100.7"));
        assert_ne!(address, other.pseudonym("source", b"198.51.100.7"));
    }

    /// A sealed value opens only under its own label and key.
    #[test]
    fn sealed_values_open_only_for_their_label() {
        let key = Derivation(SecretKey::from_bytes([7; 32]));
        let sealed = key.seal("signup-envelope", b"state").expect("seals");
        assert_eq!(
            key.open("signup-envelope", &sealed).as_deref(),
            Some(&b"state"[..])
        );
        assert_eq!(key.open("other", &sealed), None);
        assert_eq!(
            Derivation(SecretKey::from_bytes([8; 32])).open("signup-envelope", &sealed),
            None
        );
        assert_eq!(key.open("signup-envelope", &sealed[..8]), None);
    }
}
