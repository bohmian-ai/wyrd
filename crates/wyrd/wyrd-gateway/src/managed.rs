//! Tenant-isolated protection of submitted provider keys.
//!
//! A tenant administrator submits a provider key once; the server seals it
//! under that tenant's own versioned keyring and stores only the resulting
//! envelope. No key material reaches Postgres, no key is shared between
//! tenants, and the sealed payload binds the tenant, credential name, and
//! provider so an envelope copied to another identity cannot be opened.

use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter, Result as FmtResult};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use wyrd_crypt::{EncryptedPayload, SecretKey};
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::{ProviderCredentialName, ProviderId};
use zeroize::Zeroizing;

use crate::credential::CredentialError;

/// Sealed managed secret exactly as it is persisted and admitted.
///
/// The envelope is opaque: it discloses no plaintext, digest, or prefix of the
/// provider key, only which retained key version can open it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedSecretEnvelope {
    /// Keyring version that sealed the payload and must open it.
    pub key_version: String,
    /// AES-GCM nonce.
    pub nonce: Vec<u8>,
    /// Sealed payload bytes.
    pub ciphertext: Vec<u8>,
}

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

/// Payload sealed under the tenant key: the binding plus the provider key.
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

/// One tenant's versioned managed-secret keyring.
///
/// `active` seals new and replaced values; every retained version stays
/// available so credentials committed under an older key keep resolving until
/// their administrator resubmits them.
#[derive(Debug)]
pub struct TenantKeyring {
    /// Version name that seals new values.
    active: String,
    /// Every distributed version, including retained older ones.
    versions: BTreeMap<String, SecretKey>,
}

impl TenantKeyring {
    /// Builds a keyring whose `active` version must be present in `versions`.
    ///
    /// # Errors
    /// Returns [`CredentialError::Unconfigured`] when `active` names no
    /// distributed version, which is an incomplete keyring.
    pub fn new(
        active: String,
        versions: BTreeMap<String, SecretKey>,
    ) -> Result<Self, CredentialError> {
        if !versions.contains_key(&active) {
            return Err(CredentialError::Unconfigured);
        }
        Ok(Self { active, versions })
    }
}

/// Every configured tenant keyring, keyed by the tenant that owns it.
///
/// This is the whole managed-secret protection capability: configuration
/// builds it once, administration seals through it, and the credential
/// resolver opens through it. There is no other key selection or envelope
/// path.
#[derive(Debug, Default)]
pub struct ManagedSecretKeys(
    /// Keyring per configured tenant. A tenant absent from this map has no
    /// managed-secret protection, so sealing and opening fail closed rather
    /// than falling back to another tenant's key.
    BTreeMap<DataTenantId, TenantKeyring>,
);

impl ManagedSecretKeys {
    /// Builds the process-wide map of independently configured tenant
    /// keyrings.
    #[must_use]
    pub fn new(keyrings: BTreeMap<DataTenantId, TenantKeyring>) -> Self {
        Self(keyrings)
    }

    /// Seals `secret` for `binding` under the tenant's active key version.
    ///
    /// The serialized payload holds the provider key in the clear until it is
    /// encrypted, so it is owned by [`Zeroizing`] and erased on drop.
    ///
    /// # Errors
    /// Returns [`CredentialError::Unconfigured`] when the binding's tenant has
    /// no keyring, and [`CredentialError::Unavailable`] when the payload
    /// cannot be encoded or encrypted.
    pub fn seal(
        &self,
        binding: ManagedSecretBinding<'_>,
        secret: &str,
    ) -> Result<ManagedSecretEnvelope, CredentialError> {
        let keyring = self
            .0
            .get(&binding.tenant)
            .ok_or(CredentialError::Unconfigured)?;
        let key = keyring
            .versions
            .get(&keyring.active)
            .ok_or(CredentialError::Unconfigured)?;
        let payload = SealedPayload {
            tenant: binding.tenant,
            name: binding.name.clone(),
            provider: binding.provider.clone(),
            secret: SecretString::from(secret.to_owned()),
        };
        let plaintext =
            Zeroizing::new(serde_json::to_vec(&payload).map_err(|_| CredentialError::Unavailable)?);
        let sealed =
            wyrd_crypt::encrypt(key, &plaintext).map_err(|_| CredentialError::Unavailable)?;
        Ok(ManagedSecretEnvelope {
            key_version: keyring.active.clone(),
            nonce: sealed.nonce.to_vec(),
            ciphertext: sealed.ciphertext,
        })
    }

    /// Opens `envelope` for `binding`, returning the provider key.
    ///
    /// The recorded key version selects the key; a retired version, another
    /// tenant's keyring, a tampered envelope, or a payload bound to a
    /// different identity all fail closed without falling back to any other
    /// key or source. The decrypted payload holds the provider key in the
    /// clear until deserialization moves it into a [`SecretString`], so it is
    /// owned by [`Zeroizing`] and erased on drop.
    ///
    /// # Errors
    /// Returns [`CredentialError::Unconfigured`] when the tenant keyring or
    /// the recorded version is absent, and [`CredentialError::Unavailable`]
    /// when the nonce is malformed, authentication fails, the payload does not
    /// decode, or it is bound to another tenant, name, or provider.
    pub fn open(
        &self,
        binding: ManagedSecretBinding<'_>,
        envelope: &ManagedSecretEnvelope,
    ) -> Result<SecretString, CredentialError> {
        let key = self
            .0
            .get(&binding.tenant)
            .and_then(|keyring| keyring.versions.get(&envelope.key_version))
            .ok_or(CredentialError::Unconfigured)?;
        let nonce: [u8; 12] = envelope
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_| CredentialError::Unavailable)?;
        let plaintext = Zeroizing::new(
            wyrd_crypt::decrypt(
                key,
                &EncryptedPayload {
                    nonce,
                    ciphertext: envelope.ciphertext.clone(),
                },
            )
            .map_err(|_| CredentialError::Unavailable)?,
        );
        let payload: SealedPayload =
            serde_json::from_slice(&plaintext).map_err(|_| CredentialError::Unavailable)?;
        let bound = payload.tenant == binding.tenant
            && &payload.name == binding.name
            && &payload.provider == binding.provider;
        if !bound {
            return Err(CredentialError::Unavailable);
        }
        Ok(payload.secret)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ManagedSecretBinding, ManagedSecretEnvelope, ManagedSecretKeys, SealedPayload,
        TenantKeyring,
    };
    use crate::credential::CredentialError;
    use secrecy::{ExposeSecret, SecretString};
    use std::collections::BTreeMap;
    use wyrd_crypt::SecretKey;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::ids::{ProviderCredentialName, ProviderId};

    /// Keyring whose `active` version holds a key of repeated `seed` bytes,
    /// plus any extra `(version, seed)` pairs retained beside it.
    ///
    /// # Panics
    ///
    /// Panics when `active` is absent from `versions`, which would make the
    /// fixture itself incomplete.
    fn keyring(active: &str, versions: &[(&str, u8)]) -> TenantKeyring {
        let versions = versions
            .iter()
            .map(|(name, seed)| ((*name).to_owned(), SecretKey::from_bytes([*seed; 32])))
            .collect::<BTreeMap<_, _>>();
        TenantKeyring::new(active.to_owned(), versions).expect("keyring is complete")
    }

    /// Opens `envelope` for `binding`, exposing the plaintext so a result can
    /// be compared directly against the expected value or failure.
    fn open(
        keys: &ManagedSecretKeys,
        binding: ManagedSecretBinding<'_>,
        envelope: &ManagedSecretEnvelope,
    ) -> Result<String, CredentialError> {
        keys.open(binding, envelope)
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

        let keys = ManagedSecretKeys::new(BTreeMap::from([(tenant, keyring("v1", &[("v1", 3)]))]));
        let binding = ManagedSecretBinding {
            tenant,
            name: &name,
            provider: &provider,
        };
        let envelope = keys.seal(binding, SENTINEL).expect("seals");
        assert_eq!(open(&keys, binding, &envelope), Ok(SENTINEL.to_owned()));
    }

    /// An incomplete keyring whose active version was never distributed is
    /// refused at construction instead of failing at the first write.
    ///
    /// # Panics
    ///
    /// Panics when construction succeeds or reports anything other than
    /// `CredentialError::Unconfigured`.
    #[test]
    fn an_active_version_absent_from_the_keyring_is_refused() {
        let error = TenantKeyring::new("v2".to_owned(), BTreeMap::new()).expect_err("incomplete");
        assert_eq!(error, CredentialError::Unconfigured);
    }

    /// A sealed value opens only for its own tenant, name, provider, and a
    /// retained key version; every other identity or key fails closed.
    ///
    /// # Panics
    ///
    /// Panics when sealing fails, when the ciphertext discloses the plaintext,
    /// or when any foreign tenant, name, provider, retired version, tampered
    /// envelope, or unconfigured keyring fails to produce its expected error.
    #[test]
    fn sealed_values_open_only_for_their_own_binding_and_keyring() {
        let tenant = DataTenantId::new_v7();
        let other_tenant = DataTenantId::new_v7();
        let keys = ManagedSecretKeys::new(BTreeMap::from([
            (tenant, keyring("v2", &[("v1", 1), ("v2", 2)])),
            (other_tenant, keyring("v2", &[("v2", 9)])),
        ]));
        let (name, provider) = (name(), provider());
        let binding = ManagedSecretBinding {
            tenant,
            name: &name,
            provider: &provider,
        };

        let envelope = keys.seal(binding, "sk-live-SECRET").expect("seals");
        assert_eq!(envelope.key_version, "v2", "new values use the active key");
        assert!(
            !String::from_utf8_lossy(&envelope.ciphertext).contains("sk-live"),
            "ciphertext discloses the plaintext"
        );
        assert_eq!(
            open(&keys, binding, &envelope),
            Ok("sk-live-SECRET".to_owned())
        );

        let foreign = ManagedSecretBinding {
            tenant: other_tenant,
            name: &name,
            provider: &provider,
        };
        assert_eq!(
            open(&keys, foreign, &envelope),
            Err(CredentialError::Unavailable),
            "another tenant's key of the same version must not open a copied envelope"
        );
        let renamed = ProviderCredentialName::new("other-key").expect("name");
        assert_eq!(
            open(
                &keys,
                ManagedSecretBinding {
                    tenant,
                    name: &renamed,
                    provider: &provider
                },
                &envelope
            ),
            Err(CredentialError::Unavailable),
            "an envelope copied to another credential name must not open"
        );
        let elsewhere = ProviderId::new("anthropic").expect("provider");
        assert_eq!(
            open(
                &keys,
                ManagedSecretBinding {
                    tenant,
                    name: &name,
                    provider: &elsewhere
                },
                &envelope
            ),
            Err(CredentialError::Unavailable),
            "an envelope copied to another provider must not open"
        );
        assert_eq!(
            open(
                &keys,
                binding,
                &ManagedSecretEnvelope {
                    key_version: "retired".to_owned(),
                    ..envelope.clone()
                }
            ),
            Err(CredentialError::Unconfigured),
            "a retired key version must not fall back to another version"
        );
        let mut tampered = envelope.clone();
        tampered.ciphertext[0] ^= 0xff;
        assert_eq!(
            open(&keys, binding, &tampered),
            Err(CredentialError::Unavailable),
            "authentication must reject a tampered envelope"
        );

        let unconfigured = ManagedSecretKeys::default();
        assert_eq!(
            unconfigured.seal(binding, "sk-live-SECRET"),
            Err(CredentialError::Unconfigured)
        );
        assert_eq!(
            open(&unconfigured, binding, &envelope),
            Err(CredentialError::Unconfigured)
        );
    }

    /// Rotating `active` keeps values committed under a retained version
    /// readable while new values take the new key.
    ///
    /// # Panics
    ///
    /// Panics when sealing fails, when a retained version stops opening a
    /// committed envelope, when a new value does not take the rotated key
    /// version, or when a removed version stops failing closed.
    #[test]
    fn rotation_retains_older_versions_until_values_are_resubmitted() {
        let tenant = DataTenantId::new_v7();
        let (name, provider) = (name(), provider());
        let binding = ManagedSecretBinding {
            tenant,
            name: &name,
            provider: &provider,
        };
        let before =
            ManagedSecretKeys::new(BTreeMap::from([(tenant, keyring("v1", &[("v1", 1)]))]));
        let committed = before.seal(binding, "sk-old").expect("seals");

        let after = ManagedSecretKeys::new(BTreeMap::from([(
            tenant,
            keyring("v2", &[("v1", 1), ("v2", 2)]),
        )]));
        assert_eq!(
            open(&after, binding, &committed),
            Ok("sk-old".to_owned()),
            "a retained version still opens a committed envelope"
        );
        assert_eq!(
            after.seal(binding, "sk-new").expect("seals").key_version,
            "v2"
        );

        let retired =
            ManagedSecretKeys::new(BTreeMap::from([(tenant, keyring("v2", &[("v2", 2)]))]));
        assert_eq!(
            open(&retired, binding, &committed),
            Err(CredentialError::Unconfigured),
            "removing a version still referenced by an envelope fails closed"
        );
    }
}
