//! Stable Bifrost node identity coupled to the Scribe durable volume.
//!
//! A Wyrd runtime node incarnation is `(NodeId, ClusterRole, fencing_token)`.
//! The fence is re-issued by membership on every incarnation, but the `NodeId`
//! is not free to change for a Scribe-bearing target: Scribe's durable WAL,
//! its published stream identities, and the tail routes an Oracle resolves
//! against them are all keyed by the node that owns the volume. A Scribe that
//! came back under a fresh identity would be a new node holding another node's
//! durable state, which is exactly the ambiguity the recovery contract refuses.
//!
//! So the identity lives with the volume. It is written once, next to the WAL,
//! and reloaded by every later incarnation that mounts the same root. An
//! Oracle-only target owns no durable volume and may generate a new identity
//! per process.
//!
//! This identity is deliberately independent of the peer certificate. The
//! certificate admits the private transport; it never encodes or is compared
//! to a `NodeId`.

use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use vala_bifrost_redux::scribe::stream_identity::NodeId;

/// File name the stable identity is stored under inside the Scribe root.
const IDENTITY_FILE_NAME: &str = "node-identity.json";

/// Only identity-document version this build accepts.
///
/// An unknown version fails boot closed rather than being reinterpreted: a
/// future format could carry fields whose absence silently changes meaning.
const IDENTITY_VERSION: u32 = 1;

/// Domain separator mixed into the checksum.
///
/// Keeps this digest from colliding with any other SHA-256 Wyrd computes over
/// a bare UUID, so a digest lifted from elsewhere cannot validate here.
const CHECKSUM_DOMAIN: &[u8] = b"wyrd.bifrost.node-identity.v1";

/// Why a stable node identity could not be established.
#[derive(Debug, thiserror::Error)]
pub enum NodeIdentityError {
    /// The identity root or file could not be read, written, or synced.
    #[error("Scribe node identity I/O failed at {path}: {source}")]
    Io {
        /// Path the failing operation targeted.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// The stored document is not parseable as an identity document.
    #[error("Scribe node identity at {path} is malformed: {detail}")]
    Malformed {
        /// Path of the unreadable document.
        path: PathBuf,
        /// Non-secret parse or validation detail.
        detail: String,
    },
    /// The stored document declares a version this build cannot interpret.
    #[error("Scribe node identity at {path} declares unsupported version {version}")]
    UnsupportedVersion {
        /// Path of the rejected document.
        path: PathBuf,
        /// Version the document declared.
        version: u32,
    },
    /// The stored checksum does not cover the stored identity.
    #[error("Scribe node identity at {path} failed its checksum")]
    ChecksumMismatch {
        /// Path of the corrupt document.
        path: PathBuf,
    },
}

/// On-disk representation of one node's stable identity.
///
/// Serialized as JSON so an operator can read it during an incident without a
/// Wyrd binary, and checksummed so a truncated or hand-edited file is refused
/// rather than silently adopted.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NodeIdentityDocument {
    /// Format version of this document.
    version: u32,
    /// Stable node identity this volume owns.
    node_id: uuid::Uuid,
    /// Hex SHA-256 over the domain separator, version, and identity.
    checksum: String,
}

impl NodeIdentityDocument {
    /// Builds a document carrying the checksum for `node_id`.
    fn new(node_id: uuid::Uuid) -> Self {
        Self {
            version: IDENTITY_VERSION,
            node_id,
            checksum: checksum_of(IDENTITY_VERSION, node_id),
        }
    }

    /// Validates version and checksum, returning the identity it carries.
    ///
    /// # Errors
    ///
    /// Returns [`NodeIdentityError::UnsupportedVersion`] for a version this
    /// build does not interpret, and [`NodeIdentityError::ChecksumMismatch`]
    /// when the stored digest does not cover the stored identity.
    fn verify(&self, path: &Path) -> Result<NodeId, NodeIdentityError> {
        if self.version != IDENTITY_VERSION {
            return Err(NodeIdentityError::UnsupportedVersion {
                path: path.to_path_buf(),
                version: self.version,
            });
        }
        if self.checksum != checksum_of(self.version, self.node_id) {
            return Err(NodeIdentityError::ChecksumMismatch {
                path: path.to_path_buf(),
            });
        }
        Ok(NodeId::new(self.node_id))
    }
}

/// Computes the domain-separated checksum for one identity document.
fn checksum_of(version: u32, node_id: uuid::Uuid) -> String {
    let mut hasher = Sha256::new();
    hasher.update(CHECKSUM_DOMAIN);
    hasher.update(version.to_be_bytes());
    hasher.update(node_id.as_bytes());
    hex::encode(hasher.finalize())
}

/// Reads and creates the stable identity owned by one Scribe durable root.
///
/// The store owns the path rather than the identity: it is consulted once at
/// boot and dropped. Creation is crash-safe — the document is written to a
/// temporary sibling, fsynced, atomically renamed into place, and the parent
/// directory is fsynced — so a power loss during first boot leaves either no
/// identity or a complete one, never a half-written file a later boot would
/// have to guess about.
#[derive(Debug)]
pub struct ScribeNodeIdentityStore {
    /// Durable root the identity document lives in.
    root: PathBuf,
}

impl ScribeNodeIdentityStore {
    /// Binds a store to one Scribe durable root.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Returns the document path this store reads and writes.
    fn path(&self) -> PathBuf {
        self.root.join(IDENTITY_FILE_NAME)
    }

    /// Loads the stable identity for this volume, creating it on first boot.
    ///
    /// Repeated calls against the same intact root return the same identity,
    /// which is the whole contract: a restarted Scribe reclaims its own durable
    /// state rather than presenting as a new node.
    ///
    /// # Errors
    ///
    /// Returns [`NodeIdentityError::Io`] when the root cannot be created or the
    /// document cannot be read, written, or synced;
    /// [`NodeIdentityError::Malformed`] when the stored bytes do not parse;
    /// [`NodeIdentityError::UnsupportedVersion`] for an uninterpretable
    /// version; and [`NodeIdentityError::ChecksumMismatch`] for a corrupt
    /// document. Every failure is fail-closed: boot stops rather than adopting
    /// an ambiguous identity.
    pub fn load_or_create(&self) -> Result<NodeId, NodeIdentityError> {
        let path = self.path();
        match std::fs::read(&path) {
            Ok(bytes) => {
                let document: NodeIdentityDocument =
                    serde_json::from_slice(&bytes).map_err(|error| {
                        NodeIdentityError::Malformed {
                            path: path.clone(),
                            detail: error.to_string(),
                        }
                    })?;
                document.verify(&path)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => self.create(&path),
            Err(source) => Err(NodeIdentityError::Io { path, source }),
        }
    }

    /// Durably creates a fresh identity document at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`NodeIdentityError::Io`] when the root cannot be created or any
    /// write, fsync, or rename fails, and [`NodeIdentityError::Malformed`] when
    /// the freshly built document cannot be serialized.
    fn create(&self, path: &Path) -> Result<NodeId, NodeIdentityError> {
        std::fs::create_dir_all(&self.root).map_err(|source| NodeIdentityError::Io {
            path: self.root.clone(),
            source,
        })?;
        let node_id = uuid::Uuid::new_v4();
        let document = NodeIdentityDocument::new(node_id);
        let bytes =
            serde_json::to_vec_pretty(&document).map_err(|error| NodeIdentityError::Malformed {
                path: path.to_path_buf(),
                detail: error.to_string(),
            })?;
        // A sibling in the same directory, so the rename below stays within one
        // filesystem and is therefore atomic.
        let staging = self
            .root
            .join(format!("{IDENTITY_FILE_NAME}.{node_id}.tmp"));
        let mut file = File::create(&staging).map_err(|source| NodeIdentityError::Io {
            path: staging.clone(),
            source,
        })?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|source| NodeIdentityError::Io {
                path: staging.clone(),
                source,
            })?;
        drop(file);
        std::fs::rename(&staging, path).map_err(|source| NodeIdentityError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        // Without this the rename itself can be lost on power failure even
        // though the file contents were durable.
        File::open(&self.root)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| NodeIdentityError::Io {
                path: self.root.clone(),
                source,
            })?;
        Ok(NodeId::new(node_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a store over a fresh temporary root.
    fn store(root: &tempfile::TempDir) -> ScribeNodeIdentityStore {
        ScribeNodeIdentityStore::new(root.path().join("scribe-wal"))
    }

    /// A volume that already carries an identity hands back the same one.
    ///
    /// This is the property the whole module exists for: a Scribe restart over
    /// the same durable root must reclaim its own WAL and published stream
    /// identities rather than present as a new node.
    #[test]
    fn a_reused_volume_returns_the_identity_it_already_owns() {
        let root = tempfile::tempdir().expect("temporary root");
        let store = store(&root);
        let first = store.load_or_create().expect("first boot");
        let second = store.load_or_create().expect("restart");
        assert_eq!(first, second);
    }

    /// Two distinct volumes never share an identity.
    #[test]
    fn distinct_volumes_own_distinct_identities() {
        let first_root = tempfile::tempdir().expect("first root");
        let second_root = tempfile::tempdir().expect("second root");
        let first = store(&first_root).load_or_create().expect("first volume");
        let second = store(&second_root).load_or_create().expect("second volume");
        assert_ne!(first, second);
    }

    /// A truncated or hand-edited document fails boot instead of being adopted.
    #[test]
    fn a_corrupt_identity_document_fails_closed() {
        let root = tempfile::tempdir().expect("temporary root");
        let store = store(&root);
        store.load_or_create().expect("first boot");
        std::fs::write(store.path(), b"{\"version\":1,").expect("corrupt the document");
        assert!(matches!(
            store.load_or_create(),
            Err(NodeIdentityError::Malformed { .. })
        ));
    }

    /// An identity whose checksum does not cover it is refused.
    #[test]
    fn a_rewritten_identity_fails_its_checksum() {
        let root = tempfile::tempdir().expect("temporary root");
        let store = store(&root);
        let original = store.load_or_create().expect("first boot");
        let bytes = std::fs::read(store.path()).expect("read document");
        let mut document: NodeIdentityDocument =
            serde_json::from_slice(&bytes).expect("parse document");
        document.node_id = uuid::Uuid::new_v4();
        assert_ne!(document.node_id, original.as_uuid());
        std::fs::write(
            store.path(),
            serde_json::to_vec(&document).expect("serialize"),
        )
        .expect("rewrite document");
        assert!(matches!(
            store.load_or_create(),
            Err(NodeIdentityError::ChecksumMismatch { .. })
        ));
    }

    /// A document from an unknown format version is refused, not reinterpreted.
    #[test]
    fn an_unsupported_version_fails_closed() {
        let root = tempfile::tempdir().expect("temporary root");
        let store = store(&root);
        store.load_or_create().expect("first boot");
        let node_id = uuid::Uuid::new_v4();
        let document = NodeIdentityDocument {
            version: IDENTITY_VERSION + 1,
            node_id,
            checksum: checksum_of(IDENTITY_VERSION + 1, node_id),
        };
        std::fs::write(
            store.path(),
            serde_json::to_vec(&document).expect("serialize"),
        )
        .expect("rewrite document");
        assert!(matches!(
            store.load_or_create(),
            Err(NodeIdentityError::UnsupportedVersion { .. })
        ));
    }

    /// Creation leaves no staging file behind for a later boot to trip over.
    #[test]
    fn creation_leaves_only_the_committed_document() {
        let root = tempfile::tempdir().expect("temporary root");
        let store = store(&root);
        store.load_or_create().expect("first boot");
        let entries = std::fs::read_dir(store.root)
            .expect("list root")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec![std::ffi::OsString::from(IDENTITY_FILE_NAME)]);
    }
}
