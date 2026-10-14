//! Tenant-bound protection of submitted provider keys.
//!
//! A tenant administrator submits a provider key once; the server seals it
//! under the deployment sealing keyring, the same [`SealingKeyring`] that
//! protects every other stored secret, and stores only the resulting versioned
//! envelope in the tenant's row-level-secured credential row. No key material
//! reaches Postgres, rotation and rewrap follow the deployment keyring, and the
//! sealed payload binds the tenant, credential name, and provider so an
//! envelope copied to another identity cannot be opened.

use std::fmt::{Debug, Formatter, Result as FmtResult};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use wyrd_crypt::SealingKeyring;
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::{ProviderCredentialName, ProviderId};
use zeroize::Zeroizing;

use crate::credential::CredentialError;

/// Identity a sealed payload is bound to.
///
/// Sealing writes it into the authenticated payload and opening compares it,
/// so an envelope moved to another tenant, credential name, or provider fails
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

/// Payload sealed under the deployment keyring: the binding plus the provider
/// key.
///
/// This is the one shape in the process that holds an unsealed provider key,
/// so the key is wrapped and [`Debug`] is written by hand: no derive, tracing
/// field, or enclosing `#[derive(Debug)]` can print it. The serialized form is
/// unchanged — `secret` is still a plain JSON string — because committed
/// envelopes must keep opening.
#[derive(Serialize, serde::Deserialize)]
struct SealedPayload {
    /// Tenant the credential belongs to.
    tenant: DataTenantId,
    /// Credential name within that tenant.
    name: ProviderCredentialName,
    /// Provider identity of the credential.
    provider: ProviderId,
    /// Submitted provider key.
    #[serde(serialize_with = "expose_secret", deserialize_with = "wrap_secret")]
    secret: SecretString,
}

impl Debug for SealedPayload {
    /// Names the binding and elides the provider key.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("SealedPayload")
            .field("tenant", &self.tenant)
            .field("name", &self.name)
            .field("provider", &self.provider)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// Writes the wrapped provider key into the plaintext that is about to be
/// sealed, keeping the serialized shape a plain JSON string.
///
/// # Errors
/// Returns the serializer's error when the string cannot be written.
fn expose_secret<S: Serializer>(secret: &SecretString, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(secret.expose_secret())
}

/// Reads a provider key out of an opened payload straight into its wrapper,
/// so the plaintext is never held in an unwrapped field.
///
/// # Errors
/// Returns the deserializer's error when the field is not a string.
fn wrap_secret<'de, D: Deserializer<'de>>(deserializer: D) -> Result<SecretString, D::Error> {
    String::deserialize(deserializer).map(SecretString::from)
}

impl ManagedSecretBinding<'_> {
    /// Seals `secret` for this binding under the keyring's write key.
    ///
    /// The serialized payload holds the provider key in the clear until it is
    /// encrypted, so it is owned by [`Zeroizing`] and erased on drop. The
    /// result is the keyring's versioned envelope, so boot rewrap reseals it
    /// like any other stored secret.
    ///
    /// # Errors
    /// Returns [`CredentialError::Unavailable`] when the payload cannot be
    /// encoded or encrypted.
    pub fn seal(&self, keyring: &SealingKeyring, secret: &str) -> Result<Vec<u8>, CredentialError> {
        let payload = SealedPayload {
            tenant: self.tenant,
            name: self.name.clone(),
            provider: self.provider.clone(),
            secret: SecretString::from(secret.to_owned()),
        };
        let plaintext =
            Zeroizing::new(serde_json::to_vec(&payload).map_err(|_| CredentialError::Unavailable)?);
        keyring
            .seal(&plaintext)
            .map_err(|_| CredentialError::Unavailable)
    }

    /// Opens `sealed` for this binding, returning the provider key.
    ///
    /// The envelope names the keyring key that sealed it, so a value sealed
    /// before a rotation opens through a retained key. A key the keyring no
    /// longer holds, a tampered envelope, or a payload bound to a different
    /// identity all fail closed without falling back to any other source. The
    /// decrypted payload is owned by [`Zeroizing`] and erased on drop.
    ///
    /// # Errors
    /// Returns [`CredentialError::Unavailable`] when no held key opens the
    /// envelope, the payload does not decode, or it is bound to another
    /// tenant, name, or provider.
    pub fn open(
        &self,
        keyring: &SealingKeyring,
        sealed: &[u8],
    ) -> Result<SecretString, CredentialError> {
        let plaintext = Zeroizing::new(
            keyring
                .open(sealed)
                .map_err(|_| CredentialError::Unavailable)?,
        );
        let payload: SealedPayload =
            serde_json::from_slice(&plaintext).map_err(|_| CredentialError::Unavailable)?;
        let bound = payload.tenant == self.tenant
            && &payload.name == self.name
            && &payload.provider == self.provider;
        if !bound {
            return Err(CredentialError::Unavailable);
        }
        Ok(payload.secret)
    }
}

#[cfg(test)]
mod tests {
    use super::{ManagedSecretBinding, SealedPayload};
    use crate::credential::CredentialError;
    use secrecy::{ExposeSecret, SecretString};
    use wyrd_crypt::{SealingKeyring, SecretKey};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::ids::{ProviderCredentialName, ProviderId};

    /// Keyring whose write key holds repeated `seed` bytes.
    fn keyring(seed: u8) -> SealingKeyring {
        SealingKeyring::new(SecretKey::from_bytes([seed; 32]))
    }

    /// Opens `sealed` for `binding`, exposing the plaintext so a result can be
    /// compared directly against the expected value or failure.
    ///
    /// # Errors
    /// Returns [`CredentialError::Unavailable`], as
    /// [`ManagedSecretBinding::open`] does, when no key in `keyring` opens the
    /// envelope, the payload does not decode, or the payload is bound to
    /// another tenant, name, or provider.
    fn open(
        binding: ManagedSecretBinding<'_>,
        keyring: &SealingKeyring,
        sealed: &[u8],
    ) -> Result<String, CredentialError> {
        binding
            .open(keyring, sealed)
            .map(|secret| secret.expose_secret().to_owned())
    }

    /// Credential name every fixture seals under.
    ///
    /// # Panics
    ///
    /// Panics when the literal stops being a valid credential name.
    fn name() -> ProviderCredentialName {
        ProviderCredentialName::new("openai-key").expect("name")
    }

    /// Provider every fixture seals under.
    ///
    /// # Panics
    ///
    /// Panics when the literal stops being a valid provider id.
    fn provider() -> ProviderId {
        ProviderId::new("openai").expect("provider")
    }

    /// The one struct that holds an unsealed provider key never prints it,
    /// and wrapping it left the sealed representation openable.
    ///
    /// # Panics
    ///
    /// Panics when the debug rendering discloses the key, when sealing fails,
    /// or when the envelope does not round-trip to the original value.
    #[test]
    fn a_sealed_payload_redacts_its_provider_key_and_still_round_trips() {
        const SENTINEL: &str = "sk-live-sentinel-value";
        let tenant = DataTenantId::new_v7();
        let (name, provider) = (name(), provider());
        let payload = SealedPayload {
            tenant,
            name: name.clone(),
            provider: provider.clone(),
            secret: SecretString::from(SENTINEL),
        };
        let rendered = format!("{payload:?}");
        assert!(!rendered.contains(SENTINEL), "{rendered}");
        assert!(rendered.contains("<redacted>"), "{rendered}");

        let keys = keyring(3);
        let binding = ManagedSecretBinding {
            tenant,
            name: &name,
            provider: &provider,
        };
        let sealed = binding.seal(&keys, SENTINEL).expect("seals");
        assert_eq!(open(binding, &keys, &sealed), Ok(SENTINEL.to_owned()));
    }

    /// A sealed value opens only for its own tenant, name, and provider under
    /// a key the deployment keyring holds; every other identity or key fails
    /// closed even though every tenant shares the deployment keyring.
    ///
    /// # Panics
    ///
    /// Panics when sealing fails, when the ciphertext discloses the plaintext,
    /// or when any foreign tenant, name, provider, unheld key, or tampered
    /// envelope opens.
    #[test]
    fn sealed_values_open_only_for_their_own_binding() {
        let keys = keyring(2);
        let tenant = DataTenantId::new_v7();
        let (name, provider) = (name(), provider());
        let binding = ManagedSecretBinding {
            tenant,
            name: &name,
            provider: &provider,
        };

        let sealed = binding.seal(&keys, "sk-live-SECRET").expect("seals");
        assert!(
            !String::from_utf8_lossy(&sealed).contains("sk-live"),
            "ciphertext discloses the plaintext"
        );
        assert_eq!(
            open(binding, &keys, &sealed),
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
                open(foreign, &keys, &sealed),
                Err(CredentialError::Unavailable),
                "{reason}"
            );
        }
        assert_eq!(
            open(binding, &keyring(9), &sealed),
            Err(CredentialError::Unavailable),
            "a keyring not holding the sealing key must not open the envelope"
        );
        let mut tampered = sealed.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xff;
        assert_eq!(
            open(binding, &keys, &tampered),
            Err(CredentialError::Unavailable),
            "authentication must reject a tampered envelope"
        );
    }

    /// Deployment sealing-key rotation covers managed secrets: a retained key
    /// keeps a committed envelope readable, the keyring's own rewrap moves it
    /// to the new write key with its binding intact, and retiring the old key
    /// fails closed only for an envelope that was never rewrapped.
    ///
    /// # Panics
    ///
    /// Panics when sealing or rewrap fails, when a retained key stops opening
    /// a committed envelope, when a rewrapped envelope loses its binding, or
    /// when a retired key's envelope still opens.
    #[test]
    fn deployment_rotation_rewraps_managed_secrets_under_the_new_write_key() {
        let (name, provider) = (name(), provider());
        let binding = ManagedSecretBinding {
            tenant: DataTenantId::new_v7(),
            name: &name,
            provider: &provider,
        };
        let committed = binding.seal(&keyring(1), "sk-old").expect("seals");

        let rotated = keyring(2).with_retained(SecretKey::from_bytes([1; 32]));
        assert_eq!(
            open(binding, &rotated, &committed),
            Ok("sk-old".to_owned()),
            "a retained key still opens a committed envelope"
        );
        let rewrapped = rotated
            .rewrap(&committed)
            .expect("rewraps")
            .expect("an old-key envelope needs rewrap");

        let retired = keyring(2);
        assert_eq!(
            open(binding, &retired, &rewrapped),
            Ok("sk-old".to_owned()),
            "the rewrapped envelope opens under the new key alone"
        );
        assert_eq!(
            open(
                ManagedSecretBinding {
                    tenant: DataTenantId::new_v7(),
                    ..binding
                },
                &retired,
                &rewrapped
            ),
            Err(CredentialError::Unavailable),
            "rewrap keeps the tenant binding"
        );
        assert_eq!(
            open(binding, &retired, &committed),
            Err(CredentialError::Unavailable),
            "retiring a key still referenced by an envelope fails closed"
        );
    }
}
