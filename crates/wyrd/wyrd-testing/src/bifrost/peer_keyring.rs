//! Test material for the independent Bifrost peer ticket keyring.
//!
//! Wyrd never generates peer ticket keys in production: a deployment supplies
//! one active Ed25519 signing key plus a published verifying manifest, and
//! rotates them on its own schedule. A journey has to prove that schedule
//! works — that a retired key still verifies until its published instant and
//! not after, and that a key outside the manifest never verifies at all — so
//! this module mints exactly that shape in memory and writes the same two
//! files a deployment would mount.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{
    SigningKey,
    pkcs8::{EncodePrivateKey, EncodePublicKey, spki::der::pem::LineEnding},
};
use rand::RngCore;

/// Failure while minting or writing test peer ticket key material.
#[derive(Debug, thiserror::Error)]
pub enum PeerKeyringMaterialError {
    /// A key could not be encoded as PKCS#8 or SPKI PEM.
    #[error("peer ticket key encoding failed: {0}")]
    Encode(String),
    /// The verifying manifest could not be serialized.
    #[error("peer ticket manifest serialization failed: {0}")]
    Manifest(String),
    /// A file could not be written under the requested directory.
    #[error("peer ticket material could not be written: {0}")]
    Write(String),
}

/// One generated ticket key together with the identifier it is published under.
///
/// The signing half is retained so a test can mint a ticket with a retired or
/// unpublished key and prove how the server treats it; production only ever
/// holds the active signing key.
pub struct TestPeerTicketKey {
    /// Identifier this key is published under in the manifest.
    key_id: String,
    /// Signing half retained for test minting.
    signing: SigningKey,
    /// Instant after which verification must stop, absent for the active key.
    verify_until: Option<DateTime<Utc>>,
}

impl std::fmt::Debug for TestPeerTicketKey {
    /// Formats the published identity only; the signing key never reaches logs.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TestPeerTicketKey")
            .field("key_id", &self.key_id)
            .field("verify_until", &self.verify_until)
            .finish_non_exhaustive()
    }
}

impl TestPeerTicketKey {
    /// Generates one key published under `key_id` with the given retirement.
    fn generate(key_id: &str, verify_until: Option<DateTime<Utc>>) -> Self {
        Self {
            key_id: key_id.to_owned(),
            signing: {
                // ed25519-dalek's own generator takes a `rand_core` 0.6 RNG,
                // which is not the workspace `rand` major; seeding from raw
                // bytes keeps one RNG in the workspace.
                let mut seed = [0_u8; 32];
                rand::rng().fill_bytes(&mut seed);
                SigningKey::from_bytes(&seed)
            },
            verify_until,
        }
    }

    /// Returns the identifier a ticket signed by this key must carry.
    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Returns the signing half so a test can mint a ticket with this key.
    #[must_use]
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing
    }

    /// Returns this key's PKCS#8 private PEM.
    ///
    /// # Errors
    ///
    /// Returns [`PeerKeyringMaterialError::Encode`] when PKCS#8 encoding fails.
    pub fn private_key_pem(&self) -> Result<String, PeerKeyringMaterialError> {
        self.signing
            .to_pkcs8_pem(LineEnding::LF)
            .map(|pem| pem.as_str().to_owned())
            .map_err(|error| PeerKeyringMaterialError::Encode(error.to_string()))
    }

    /// Returns this key's SPKI public PEM as published in the manifest.
    ///
    /// # Errors
    ///
    /// Returns [`PeerKeyringMaterialError::Encode`] when SPKI encoding fails.
    pub fn public_key_pem(&self) -> Result<String, PeerKeyringMaterialError> {
        self.signing
            .verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .map_err(|error| PeerKeyringMaterialError::Encode(error.to_string()))
    }
}

/// On-disk paths of one replica's peer ticket keyring.
#[derive(Clone, Debug)]
pub struct TestPeerKeyringPaths {
    /// Identifier of the key every ticket this node issues is signed with.
    pub active_key_id: String,
    /// PKCS#8 PEM path of the active signing key.
    pub signing_key_path: PathBuf,
    /// Versioned JSON manifest path listing every accepted verifying key.
    pub verifying_keyring_path: PathBuf,
}

/// A complete rotation-shaped keyring shared by every replica in one topology.
///
/// The three published keys are the three states a rotation passes through:
/// the active key that signs, a retired key still inside its verification
/// window, and a retired key past it. A fourth key is generated and
/// deliberately left out of the manifest so a test can prove that a
/// well-formed signature under an unpublished identifier is refused.
pub struct TestPeerKeyring {
    /// Key every replica signs with.
    active: TestPeerTicketKey,
    /// Retired key still accepted for verification.
    retired_valid: TestPeerTicketKey,
    /// Retired key whose verification window has closed.
    retired_expired: TestPeerTicketKey,
    /// Key that appears in no manifest.
    unpublished: TestPeerTicketKey,
}

impl std::fmt::Debug for TestPeerKeyring {
    /// Formats the published identities only; no signing key reaches logs.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TestPeerKeyring")
            .field("active", &self.active)
            .field("retired_valid", &self.retired_valid)
            .field("retired_expired", &self.retired_expired)
            .finish_non_exhaustive()
    }
}

impl Default for TestPeerKeyring {
    /// Generates a fresh keyring; every topology holds its own.
    fn default() -> Self {
        Self::generate()
    }
}

impl TestPeerKeyring {
    /// Generates one keyring covering every rotation state.
    ///
    /// The retirement instants are relative to generation time so the material
    /// is meaningful whenever the test runs, rather than pinned to a fixture
    /// date that eventually expires everything.
    #[must_use]
    pub fn generate() -> Self {
        let now = Utc::now();
        Self {
            active: TestPeerTicketKey::generate("wyrd-peer-active", None),
            retired_valid: TestPeerTicketKey::generate(
                "wyrd-peer-retired-valid",
                Some(now + Duration::hours(1)),
            ),
            retired_expired: TestPeerTicketKey::generate(
                "wyrd-peer-retired-expired",
                Some(now - Duration::hours(1)),
            ),
            unpublished: TestPeerTicketKey::generate("wyrd-peer-unpublished", None),
        }
    }

    /// Returns the key every replica in this topology signs tickets with.
    #[must_use]
    pub fn active(&self) -> &TestPeerTicketKey {
        &self.active
    }

    /// Returns the retired key still inside its verification window.
    #[must_use]
    pub fn retired_valid(&self) -> &TestPeerTicketKey {
        &self.retired_valid
    }

    /// Returns the retired key whose verification window has closed.
    #[must_use]
    pub fn retired_expired(&self) -> &TestPeerTicketKey {
        &self.retired_expired
    }

    /// Returns the key deliberately absent from the published manifest.
    #[must_use]
    pub fn unpublished(&self) -> &TestPeerTicketKey {
        &self.unpublished
    }

    /// Renders the versioned verifying manifest every replica loads.
    ///
    /// # Errors
    ///
    /// Returns [`PeerKeyringMaterialError::Encode`] when a public key cannot be
    /// encoded or [`PeerKeyringMaterialError::Manifest`] when serialization
    /// fails.
    pub fn manifest_json(&self) -> Result<String, PeerKeyringMaterialError> {
        let mut keys = Vec::with_capacity(3);
        for key in [&self.active, &self.retired_valid, &self.retired_expired] {
            let mut entry = serde_json::Map::new();
            entry.insert("keyId".to_owned(), key.key_id.clone().into());
            entry.insert("publicKeyPem".to_owned(), key.public_key_pem()?.into());
            if let Some(verify_until) = key.verify_until {
                entry.insert(
                    "verifyUntil".to_owned(),
                    verify_until.to_rfc3339().to_string().into(),
                );
            }
            keys.push(serde_json::Value::Object(entry));
        }
        let manifest = serde_json::json!({ "version": 1, "keys": keys });
        serde_json::to_string_pretty(&manifest)
            .map_err(|error| PeerKeyringMaterialError::Manifest(error.to_string()))
    }

    /// Writes this replica's active signing key and shared manifest.
    ///
    /// `label` names the private key file so co-located in-process replicas can
    /// share one directory; the manifest is shared by construction because
    /// every replica in a topology trusts the same published keys.
    ///
    /// # Errors
    ///
    /// Returns [`PeerKeyringMaterialError::Encode`] or
    /// [`PeerKeyringMaterialError::Manifest`] when material cannot be rendered,
    /// and [`PeerKeyringMaterialError::Write`] when a file cannot be written.
    pub fn materialize(
        &self,
        directory: &Path,
        label: &str,
    ) -> Result<TestPeerKeyringPaths, PeerKeyringMaterialError> {
        let signing_key_path = directory.join(format!("{label}-peer-ticket-key.pem"));
        let verifying_keyring_path = directory.join("bifrost-peer-ticket-keyring.json");
        let write = |path: &Path, contents: &str| -> Result<(), PeerKeyringMaterialError> {
            std::fs::write(path, contents)
                .map_err(|error| PeerKeyringMaterialError::Write(error.to_string()))
        };
        write(&signing_key_path, &self.active.private_key_pem()?)?;
        write(&verifying_keyring_path, &self.manifest_json()?)?;
        Ok(TestPeerKeyringPaths {
            active_key_id: self.active.key_id.clone(),
            signing_key_path,
            verifying_keyring_path,
        })
    }
}
