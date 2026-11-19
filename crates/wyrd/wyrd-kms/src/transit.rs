//! Vault Transit wrapping through the shared screened `wyrd-vault` client.
//!
//! The configured reference is a Transit key name. Transit versions its
//! output (`vault:v<n>:…`) and keeps old versions for decryption; Wyrd stores
//! that string as the opaque wrapped DEK. The canonical wrap context is
//! Transit `associated_data`, so the key must be an AEAD type such as
//! `aes256-gcm96`. The token is re-read from its file on every call so an
//! agent can renew it in place.

use std::path::PathBuf;

use secrecy::SecretString;
use url::Url;
use wyrd_vault::{VaultError, VaultTransit};
use zeroize::Zeroizing;

use crate::config::{VaultSettings, VaultToken};
use crate::remote::unavailable;
use crate::{KmsBackendKind, KmsError, ProviderFailure};

/// This backend's kind.
const KIND: KmsBackendKind = KmsBackendKind::VaultTransit;

/// A reusable Transit client and its token source.
pub(crate) struct TransitKms {
    /// Screened, bounded Transit client.
    transit: VaultTransit,
    /// Where the token comes from.
    token: VaultToken,
}

impl TransitKms {
    /// Build over the validated Vault address and settings.
    ///
    /// # Errors
    /// Returns [`KmsError::Config`] when the client cannot be built.
    pub(crate) fn new(address: Url, settings: VaultSettings) -> Result<Self, KmsError> {
        Ok(Self {
            transit: VaultTransit::new(address, &settings.mount, settings.namespace, None)
                .map_err(|_| KmsError::Config {
                    setting: "endpoint",
                    reason: "could not build the Vault client",
                })?,
            token: settings.token,
        })
    }

    /// The current token, re-reading the owner-only file off the executor.
    ///
    /// # Errors
    /// Returns [`ProviderFailure::Credential`] when the file cannot be read.
    async fn token(&self) -> Result<SecretString, KmsError> {
        let credential = || unavailable(KIND, ProviderFailure::Credential);
        match &self.token {
            VaultToken::Inline(token) => Ok(token.clone()),
            VaultToken::File(path) => {
                let path: PathBuf = path.clone();
                let text = tokio::task::spawn_blocking(move || {
                    wyrd_utils::secret::read_secret_file(&path).map(Zeroizing::new)
                })
                .await
                .map_err(|_| credential())?
                .map_err(|_| credential())?;
                Ok(SecretString::from(text.trim()))
            }
        }
    }

    /// Encrypt `dek` under Transit key `key`, authenticating `aad`.
    ///
    /// # Errors
    /// Returns [`KmsError::Unavailable`] for a token, transport, or refusal
    /// failure.
    pub(crate) async fn wrap(
        &self,
        key: &str,
        dek: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, KmsError> {
        self.transit
            .encrypt(&self.token().await?, key, dek, aad)
            .await
            .map(String::into_bytes)
            .map_err(map)
    }

    /// Decrypt Transit ciphertext `wrapped` under `key`, requiring `aad`.
    ///
    /// # Errors
    /// Returns [`KmsError::UnknownFormat`] for a non-UTF-8 blob and
    /// [`KmsError::Unavailable`] otherwise; Transit answers a context
    /// mismatch and a permission refusal alike, so both are refused.
    pub(crate) async fn unwrap(
        &self,
        key: &str,
        wrapped: &[u8],
        aad: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, KmsError> {
        let ciphertext = std::str::from_utf8(wrapped).map_err(|_| KmsError::UnknownFormat)?;
        self.transit
            .decrypt(&self.token().await?, key, ciphertext, aad)
            .await
            .map_err(map)
    }
}

/// Map a closed Vault class onto this backend's failure.
fn map(error: VaultError) -> KmsError {
    unavailable(
        KIND,
        match error {
            VaultError::Missing | VaultError::Refused => ProviderFailure::Refused,
            VaultError::Unavailable => ProviderFailure::Transport,
        },
    )
}
