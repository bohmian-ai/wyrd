//! Independent rotatable signing material for private peer purpose tickets.
//!
//! Peer tickets are authority to act inside the cluster, and a user or API
//! token is authority to ask the cluster for something. They are different
//! powers, so they are signed by different keys: this keyring is loaded from
//! its own configured files and never from the north-south workload/JWT
//! signing key. A token minted by the public issuer therefore cannot be
//! presented as peer authority, and a leaked peer key does not mint user
//! sessions.
//!
//! Rotation is the reason verification and issuance are separate here. One
//! process issues under exactly one active key, but accepts tickets under every
//! key the manifest still lists, each until its own `verifyUntil` instant. A
//! rollout can therefore publish the new key to every replica's manifest first,
//! switch issuance second, and let the retired key age out third, without a
//! window in which a legitimately minted ticket is refused.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use chrono::{DateTime, Utc};
use ed25519_dalek::pkcs8::DecodePrivateKey as _;
use ed25519_dalek::pkcs8::spki::DecodePublicKey as _;
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use secrecy::{ExposeSecret as _, SecretString};
use sha2::Digest as _;
use vala_bifrost_redux::oracle::peer::PeerSecurityError;

use crate::config::PeerTicketKeyringConfig;

/// Manifest schema version this build accepts.
///
/// The version is checked before any key is read, so a future manifest shape is
/// refused as a configuration error rather than silently half-parsed into a
/// keyring that is missing keys.
const SUPPORTED_MANIFEST_VERSION: u32 = 1;

/// One verification key the manifest publishes.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PeerTicketManifestKey {
    /// Identifier stamped into every ticket signed by the matching private key.
    key_id: String,
    /// SPKI PEM public half of the key.
    public_key_pem: String,
    /// Instant after which this key no longer verifies anything.
    ///
    /// Absent means the key has no scheduled retirement, which is the normal
    /// state of the currently active key.
    #[serde(default)]
    verify_until: Option<DateTime<Utc>>,
}

/// Versioned manifest of every key this plane still accepts.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PeerTicketManifest {
    /// Schema version; see [`SUPPORTED_MANIFEST_VERSION`].
    version: u32,
    /// Every accepted verification key, including the active one.
    keys: Vec<PeerTicketManifestKey>,
}

/// One accepted verification key and the instant it stops verifying.
struct PeerVerifyingKey {
    /// Public half used to authenticate a presented signature.
    key: VerifyingKey,
    /// Instant after which this key is refused; `None` never retires.
    verify_until: Option<DateTime<Utc>>,
}

/// Why peer-ticket key material could not be loaded.
///
/// Every variant is a boot failure. A process that cannot load its peer keyring
/// must not serve the peer plane, because the alternative is falling back to
/// some other key — which is exactly the sharing this keyring exists to prevent.
#[derive(Debug, thiserror::Error)]
pub enum PeerKeyringError {
    /// A configured path is absent from the configuration.
    #[error("Bifrost peer ticket keyring is not fully configured")]
    Incomplete,
    /// A configured file could not be read.
    #[error("Bifrost peer ticket keyring file could not be read")]
    Unreadable,
    /// The manifest is malformed or carries an unsupported version.
    #[error("Bifrost peer ticket keyring manifest is invalid")]
    Manifest,
    /// A key in the manifest, or the signing key itself, is malformed.
    #[error("Bifrost peer ticket key material is invalid")]
    Key,
    /// The configured active key id is absent from the manifest.
    #[error("Bifrost peer ticket active key is not published in the keyring")]
    ActiveKeyUnpublished,
    /// The manifest's active entry does not match the loaded private key.
    #[error("Bifrost peer ticket active key does not match its published public key")]
    ActiveKeyMismatch,
    /// The manifest schedules the active key's own retirement.
    #[error("Bifrost peer ticket active key is published as retired")]
    ActiveKeyRetired,
}

/// The independent signing and verification material for peer purpose tickets.
///
/// One process signs with exactly one active key and verifies against every
/// unretired key in its manifest. Nothing here is shared with the workload
/// issuer: the material is loaded from this plane's own configured paths.
pub struct PeerTicketKeyring {
    /// Identifier stamped into every ticket this process issues.
    active_key_id: String,
    /// Private half retained only by this keyring.
    signing: SigningKey,
    /// Every accepted verification key, by identifier.
    verifying: HashMap<String, PeerVerifyingKey>,
}

impl fmt::Debug for PeerTicketKeyring {
    /// Reports the public identifiers only; private material never renders.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PeerTicketKeyring")
            .field("active_key_id", &self.active_key_id)
            .field("accepted_key_count", &self.verifying.len())
            .finish_non_exhaustive()
    }
}

impl PeerTicketKeyring {
    /// Loads the keyring from this process's configured peer-ticket files.
    ///
    /// # Errors
    ///
    /// Returns [`PeerKeyringError::Incomplete`] when any input is unset,
    /// [`PeerKeyringError::Unreadable`] when a file cannot be read, and the
    /// manifest, key, or active-key variants when the material is malformed or
    /// the active key is unpublished, mismatched, or already retired.
    pub fn load(config: &PeerTicketKeyringConfig) -> Result<Self, PeerKeyringError> {
        let (active_key_id, signing_key_path, manifest_path) = match (
            config.active_key_id.as_ref(),
            config.signing_key_path.as_ref(),
            config.verifying_keyring_path.as_ref(),
        ) {
            (Some(key_id), Some(signing), Some(manifest)) if !key_id.trim().is_empty() => {
                (key_id, signing, manifest)
            }
            _ => return Err(PeerKeyringError::Incomplete),
        };
        Self::from_parts(
            active_key_id,
            &SecretString::from(read_file(signing_key_path)?),
            &read_file(manifest_path)?,
        )
    }

    /// Builds the keyring from already-read key material.
    ///
    /// Separated from [`Self::load`] so the parsing, matching, and retirement
    /// rules can be proved without a filesystem, and so a harness can compose a
    /// keyring it generated in memory.
    ///
    /// # Errors
    ///
    /// Returns the manifest, key, or active-key variants described on
    /// [`PeerKeyringError`].
    pub fn from_parts(
        active_key_id: &str,
        signing_key_pem: &SecretString,
        manifest_json: &str,
    ) -> Result<Self, PeerKeyringError> {
        let signing = SigningKey::from_pkcs8_pem(signing_key_pem.expose_secret())
            .map_err(|_| PeerKeyringError::Key)?;
        let manifest: PeerTicketManifest =
            serde_json::from_str(manifest_json).map_err(|_| PeerKeyringError::Manifest)?;
        if manifest.version != SUPPORTED_MANIFEST_VERSION || manifest.keys.is_empty() {
            return Err(PeerKeyringError::Manifest);
        }
        let mut verifying = HashMap::with_capacity(manifest.keys.len());
        for entry in manifest.keys {
            let key = VerifyingKey::from_public_key_pem(&entry.public_key_pem)
                .map_err(|_| PeerKeyringError::Key)?;
            verifying.insert(
                entry.key_id,
                PeerVerifyingKey {
                    key,
                    verify_until: entry.verify_until,
                },
            );
        }
        let published = verifying
            .get(active_key_id)
            .ok_or(PeerKeyringError::ActiveKeyUnpublished)?;
        if published.key != signing.verifying_key() {
            return Err(PeerKeyringError::ActiveKeyMismatch);
        }
        if published.verify_until.is_some() {
            return Err(PeerKeyringError::ActiveKeyRetired);
        }
        Ok(Self {
            active_key_id: active_key_id.to_owned(),
            signing,
            verifying,
        })
    }

    /// Builds a single-key keyring from one PKCS#8 signing key.
    ///
    /// The identifier is derived as the lowercase SHA-256 digest of the raw
    /// public key, which is a property of the key itself rather than of any
    /// manifest, so a caller with only key material — a fixture, or a plane
    /// that has no rotation story of its own yet — composes the same identity
    /// the published manifest would give it. A keyring built this way accepts
    /// exactly one key and therefore cannot rotate.
    ///
    /// # Errors
    ///
    /// Returns [`PeerKeyringError::Key`] when the PEM is not a valid PKCS#8
    /// Ed25519 private key.
    pub fn from_signing_key_pem(pem: &SecretString) -> Result<Self, PeerKeyringError> {
        let signing =
            SigningKey::from_pkcs8_pem(pem.expose_secret()).map_err(|_| PeerKeyringError::Key)?;
        let verifying_key = signing.verifying_key();
        let key_id = hex::encode(sha2::Sha256::digest(verifying_key.to_bytes()));
        let mut verifying = HashMap::with_capacity(1);
        verifying.insert(
            key_id.clone(),
            PeerVerifyingKey {
                key: verifying_key,
                verify_until: None,
            },
        );
        Ok(Self {
            active_key_id: key_id,
            signing,
            verifying,
        })
    }

    /// Returns the identifier stamped into every ticket this process issues.
    #[must_use]
    pub fn active_key_id(&self) -> &str {
        &self.active_key_id
    }

    /// Signs one domain-separated input under the active key.
    #[must_use]
    pub fn sign(&self, input: &[u8]) -> Vec<u8> {
        self.signing.sign(input).to_bytes().to_vec()
    }

    /// Verifies one presented signature against the named accepted key.
    ///
    /// The key is resolved by the identifier the ticket carries rather than
    /// tried against every key, so a retired or unknown identifier is a refusal
    /// and never a slow path through the whole keyring.
    ///
    /// # Errors
    ///
    /// Returns [`PeerSecurityError::UnknownKey`] when the identifier is not
    /// published or its key has passed `verifyUntil`, and
    /// [`PeerSecurityError::InvalidSignature`] when the signature does not
    /// authenticate the input under that key.
    pub fn verify(
        &self,
        key_id: &str,
        input: &[u8],
        signature: &[u8],
        now: DateTime<Utc>,
    ) -> Result<(), PeerSecurityError> {
        let accepted = self
            .verifying
            .get(key_id)
            .ok_or(PeerSecurityError::UnknownKey)?;
        if accepted.verify_until.is_some_and(|until| now > until) {
            return Err(PeerSecurityError::UnknownKey);
        }
        let bytes: [u8; 64] = signature
            .try_into()
            .map_err(|_| PeerSecurityError::InvalidSignature)?;
        accepted
            .key
            .verify(input, &Signature::from_bytes(&bytes))
            .map_err(|_| PeerSecurityError::InvalidSignature)
    }
}

/// Reads one configured keyring file into a string.
///
/// # Errors
///
/// Returns [`PeerKeyringError::Unreadable`] for any IO failure. The path is
/// deliberately not included in the error: a boot failure message is not a
/// place to disclose where a private key lives.
fn read_file(path: &Path) -> Result<String, PeerKeyringError> {
    std::fs::read_to_string(path).map_err(|error| {
        tracing::error!(
            ?error,
            "Bifrost peer ticket keyring file could not be read"
        );
        PeerKeyringError::Unreadable
    })
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::pkcs8::EncodePrivateKey as _;
    use ed25519_dalek::pkcs8::spki::EncodePublicKey as _;

    use super::*;

    /// Deterministic signing key for one manifest entry.
    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    /// Renders one key's PKCS#8 private PEM.
    fn signing_pem(key: &SigningKey) -> SecretString {
        SecretString::from(
            key.to_pkcs8_pem(ed25519_dalek::pkcs8::spki::der::pem::LineEnding::LF)
                .expect("a generated key encodes as PKCS#8")
                .to_string(),
        )
    }

    /// Renders one key's SPKI public PEM.
    fn public_pem(key: &SigningKey) -> String {
        key.verifying_key()
            .to_public_key_pem(ed25519_dalek::pkcs8::spki::der::pem::LineEnding::LF)
            .expect("a generated key encodes as SPKI")
    }

    /// Builds a manifest from `(key id, key, verifyUntil)` entries.
    fn manifest(entries: &[(&str, &SigningKey, Option<&str>)]) -> String {
        let keys: Vec<String> = entries
            .iter()
            .map(|(key_id, key, until)| {
                let retirement = until.map_or_else(
                    || "null".to_owned(),
                    |value| format!("\"{value}\""),
                );
                format!(
                    "{{\"keyId\":\"{key_id}\",\"publicKeyPem\":{},\"verifyUntil\":{retirement}}}",
                    serde_json::to_string(&public_pem(key)).expect("a PEM renders as JSON")
                )
            })
            .collect();
        format!("{{\"version\":1,\"keys\":[{}]}}", keys.join(","))
    }

    /// The instant every rotation case is judged at.
    fn now() -> DateTime<Utc> {
        "2026-06-01T00:00:00Z"
            .parse()
            .expect("a static instant parses")
    }

    /// A rotation accepts the retired key until its published instant.
    ///
    /// This is the whole reason issuance and verification are separate: during
    /// a rollout the cluster holds tickets minted under both keys, and refusing
    /// the old one early would fail work that was legitimately authorized.
    #[test]
    fn a_retired_key_verifies_until_its_published_instant_and_not_after() {
        let active = key(1);
        let retired = key(2);
        let expired = key(3);
        let keyring = PeerTicketKeyring::from_parts(
            "active",
            &signing_pem(&active),
            &manifest(&[
                ("active", &active, None),
                ("retiring", &retired, Some("2026-07-01T00:00:00Z")),
                ("expired", &expired, Some("2026-05-01T00:00:00Z")),
            ]),
        )
        .expect("a well-formed keyring loads");

        let input = b"wyrd.peer.test";
        assert!(
            keyring
                .verify("active", input, &keyring.sign(input), now())
                .is_ok(),
            "the active key verifies what it signed"
        );
        assert!(
            keyring
                .verify(
                    "retiring",
                    input,
                    &retired.sign(input).to_bytes(),
                    now()
                )
                .is_ok(),
            "a key still inside its verification window is accepted"
        );
        assert!(
            matches!(
                keyring.verify("expired", input, &expired.sign(input).to_bytes(), now()),
                Err(PeerSecurityError::UnknownKey)
            ),
            "a key past its verification window is refused as unknown"
        );
        assert!(
            matches!(
                keyring.verify("never-published", input, &keyring.sign(input), now()),
                Err(PeerSecurityError::UnknownKey)
            ),
            "an unpublished identifier is refused"
        );
    }

    /// A signature from a key outside the keyring never verifies.
    ///
    /// The workload/JWT issuer is exactly such a key. Presenting one of its
    /// signatures under a published identifier must fail on the signature, and
    /// under its own identifier must fail as unknown — a public token can never
    /// become peer authority by either route.
    #[test]
    fn a_foreign_key_signature_is_refused_under_any_identifier() {
        let active = key(1);
        let foreign = key(9);
        let keyring = PeerTicketKeyring::from_parts(
            "active",
            &signing_pem(&active),
            &manifest(&[("active", &active, None)]),
        )
        .expect("a well-formed keyring loads");

        let input = b"wyrd.peer.test";
        assert!(
            matches!(
                keyring.verify("active", input, &foreign.sign(input).to_bytes(), now()),
                Err(PeerSecurityError::InvalidSignature)
            ),
            "a foreign signature under a published identifier fails verification"
        );
        assert!(
            matches!(
                keyring.verify("foreign", input, &foreign.sign(input).to_bytes(), now()),
                Err(PeerSecurityError::UnknownKey)
            ),
            "a foreign identifier is not in the keyring at all"
        );
    }

    /// The active key must be published, matching, and not itself retired.
    ///
    /// Each of these is a way for a rollout to produce a process that issues
    /// tickets nobody accepts, which is silent until work starts failing. They
    /// are refused at load instead.
    #[test]
    fn an_inconsistent_active_key_refuses_to_load() {
        let active = key(1);
        let other = key(2);

        assert!(
            matches!(
                PeerTicketKeyring::from_parts(
                    "active",
                    &signing_pem(&active),
                    &manifest(&[("other", &other, None)]),
                ),
                Err(PeerKeyringError::ActiveKeyUnpublished)
            ),
            "an unpublished active key id is refused"
        );
        assert!(
            matches!(
                PeerTicketKeyring::from_parts(
                    "active",
                    &signing_pem(&active),
                    &manifest(&[("active", &other, None)]),
                ),
                Err(PeerKeyringError::ActiveKeyMismatch)
            ),
            "a published public key that is not this process's own is refused"
        );
        assert!(
            matches!(
                PeerTicketKeyring::from_parts(
                    "active",
                    &signing_pem(&active),
                    &manifest(&[("active", &active, Some("2026-07-01T00:00:00Z"))]),
                ),
                Err(PeerKeyringError::ActiveKeyRetired)
            ),
            "issuing under a key scheduled for retirement is refused"
        );
        assert!(
            matches!(
                PeerTicketKeyring::from_parts(
                    "active",
                    &signing_pem(&active),
                    "{\"version\":2,\"keys\":[]}",
                ),
                Err(PeerKeyringError::Manifest)
            ),
            "an unsupported manifest version is refused before any key is read"
        );
    }
}
