//! Tenant-bound protection of submitted provider keys.
//!
//! A tenant administrator submits a provider key once; the server seals it
//! through the deployment [`WyrdKms`], the same facade that protects every
//! other stored secret, and stores only the resulting envelope in the
//! tenant's row-level-secured credential row. No key material reaches
//! Postgres, and the envelope authenticates the tenant, credential name, and
//! provider so a value copied to another identity cannot be opened.

use secrecy::SecretString;
use wyrd_kms::{KmsError, SecretContext, SecretPurpose, SecretScope, WyrdKms};
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::{ProviderCredentialName, ProviderId};

use crate::credential::CredentialError;

/// Identity a managed secret's envelope is bound to.
///
/// Sealing and opening authenticate it as the envelope's context, so an
/// envelope moved to another tenant, credential name, or provider fails
/// closed rather than resolving the wrong tenant's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedSecretBinding<'a> {
    /// Verified tenant owning the credential.
    pub tenant: DataTenantId,
    /// Tenant-unique credential name.
    pub name: &'a ProviderCredentialName,
    /// Provider the credential authenticates to.
    pub provider: &'a ProviderId,
}

impl ManagedSecretBinding<'_> {
    /// Seals `secret` for this binding under the active wrapping key.
    ///
    /// # Errors
    /// Returns [`CredentialError::Unavailable`] when encryption or the
    /// wrapping backend fails.
    pub async fn seal(&self, kms: &WyrdKms, secret: &str) -> Result<Vec<u8>, CredentialError> {
        let authority = [self.provider.as_str()];
        kms.seal(&self.context(&authority), secret.as_bytes())
            .await
            .map_err(|error| {
                tracing::warn!(%error, "managed secret could not be sealed");
                CredentialError::Unavailable
            })
    }

    /// Opens `sealed` for this binding, returning the provider key.
    ///
    /// A disallowed key, a tampered envelope, a backend failure, or an
    /// envelope bound to a different identity all fail closed without
    /// falling back to any other source. The caller tells a retryable
    /// [`KmsError::Unavailable`] from a permanent refusal.
    ///
    /// # Errors
    /// Returns the facade's [`KmsError`], or [`KmsError::Authentication`]
    /// for an opened value that is not UTF-8, which no write path seals.
    pub async fn open(&self, kms: &WyrdKms, sealed: &[u8]) -> Result<SecretString, KmsError> {
        let authority = [self.provider.as_str()];
        let plaintext = kms.open(&self.context(&authority), sealed).await?;
        std::str::from_utf8(&plaintext)
            .map(|secret| SecretString::from(secret.to_owned()))
            .map_err(|_| KmsError::Authentication)
    }

    /// Renews `sealed`'s encryption for this binding: the same provider key
    /// resealed under a fresh DEK and nonce wrapped by the active key.
    ///
    /// # Errors
    /// Returns the facade's [`KmsError`] when `sealed` does not open for this
    /// binding or the reseal fails; no envelope is returned then.
    pub async fn renew(&self, kms: &WyrdKms, sealed: &[u8]) -> Result<Vec<u8>, KmsError> {
        let authority = [self.provider.as_str()];
        kms.renew(&self.context(&authority), sealed).await
    }

    /// The envelope context: tenant scope, Gateway purpose, the credential
    /// name as owner, and `authority` (the provider).
    fn context<'a>(&'a self, authority: &'a [&'a str]) -> SecretContext<'a> {
        SecretContext {
            scope: SecretScope::Tenant(self.tenant),
            purpose: SecretPurpose::GatewayProviderCredential,
            owner: self.name.as_str(),
            authority,
            version: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ManagedSecretBinding;
    use secrecy::ExposeSecret;
    use wyrd_kms::{KmsConfig, KmsError, WyrdKms};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::ids::{ProviderCredentialName, ProviderId};

    /// An ephemeral memory facade with its own random key.
    ///
    /// # Panics
    /// Panics when the memory configuration stops validating.
    fn kms() -> WyrdKms {
        WyrdKms::new(&KmsConfig::memory("test"), false).expect("memory facade")
    }

    /// Opens `sealed` for `binding`, exposing the plaintext for comparison.
    ///
    /// # Errors
    /// Returns the [`KmsError`] [`ManagedSecretBinding::open`] does.
    async fn open(
        binding: ManagedSecretBinding<'_>,
        kms: &WyrdKms,
        sealed: &[u8],
    ) -> Result<String, KmsError> {
        binding
            .open(kms, sealed)
            .await
            .map(|secret| secret.expose_secret().to_owned())
    }

    /// A sealed value opens only for its own tenant, name, and provider under
    /// the facade that sealed it; every other identity, facade, or tampered
    /// envelope fails closed.
    ///
    /// # Panics
    /// Panics when sealing fails, when the ciphertext discloses the plaintext,
    /// or when any foreign binding, other facade, or tampered envelope opens.
    #[tokio::test]
    async fn sealed_values_open_only_for_their_own_binding() {
        let keys = kms();
        let name = ProviderCredentialName::new("openai-key").expect("name");
        let provider = ProviderId::new("openai").expect("provider");
        let binding = ManagedSecretBinding {
            tenant: DataTenantId::new_v7(),
            name: &name,
            provider: &provider,
        };

        let sealed = binding.seal(&keys, "sk-live-SECRET").await.expect("seals");
        assert!(
            !String::from_utf8_lossy(&sealed).contains("sk-live"),
            "ciphertext discloses the plaintext"
        );
        assert_eq!(
            open(binding, &keys, &sealed).await,
            Ok("sk-live-SECRET".to_owned())
        );

        let renamed = ProviderCredentialName::new("other-key").expect("name");
        let elsewhere = ProviderId::new("anthropic").expect("provider");
        for (foreign, reason) in [
            (
                ManagedSecretBinding {
                    tenant: DataTenantId::new_v7(),
                    ..binding
                },
                "an envelope copied to another tenant must not open",
            ),
            (
                ManagedSecretBinding {
                    name: &renamed,
                    ..binding
                },
                "an envelope copied to another credential name must not open",
            ),
            (
                ManagedSecretBinding {
                    provider: &elsewhere,
                    ..binding
                },
                "an envelope copied to another provider must not open",
            ),
        ] {
            assert_eq!(
                open(foreign, &keys, &sealed).await,
                Err(KmsError::Authentication),
                "{reason}"
            );
        }
        assert_eq!(
            open(binding, &kms(), &sealed).await,
            Err(KmsError::Authentication),
            "another deployment key must not open the envelope"
        );
        let mut tampered = sealed.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xff;
        assert_eq!(
            open(binding, &keys, &tampered).await,
            Err(KmsError::Authentication),
            "authentication must reject a tampered envelope"
        );
    }
}
