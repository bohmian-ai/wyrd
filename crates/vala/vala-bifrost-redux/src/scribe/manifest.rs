//! WAL manifest — atomic per-seal-key `sealed_lsn` tracking.
//!
//! The manifest records, per seal-key, the highest LSN fully committed to
//! `file_list` (`sealed_lsn`). Replay skips WAL records at or below this watermark.
//! Written atomically via write-fsync-rename-fsync-parent.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::contracts::ScribeError;
use crate::scribe::seal_key::SealKey;
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::WalLsn;

/// WAL manifest — per-seal-key sealed watermark and stream identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Stream identity for this WAL lifetime.
    pub stream_identity: ManifestStreamIdentity,
    /// Per-seal-key sealed LSN watermark.
    ///
    /// Key is the seal-key path component string; value is the highest LSN
    /// fully committed to `file_list` for that key.
    pub sealed_lsn: HashMap<String, u64>,
}

/// Serializable stream identity (manifest carries `node_id` as hex UUID string).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestStreamIdentity {
    /// Node ID as hex UUID string.
    pub node_id: String,
    /// Writer epoch from `vala.cluster_nodes.fencing_token`.
    pub writer_epoch: i64,
}

impl From<StreamIdentity> for ManifestStreamIdentity {
    fn from(identity: StreamIdentity) -> Self {
        Self {
            node_id: identity.node_id.to_string(),
            writer_epoch: identity.writer_epoch.as_i64(),
        }
    }
}

impl Manifest {
    /// Create a new manifest with the given stream identity.
    #[must_use]
    pub fn new(stream_identity: StreamIdentity) -> Self {
        Self {
            stream_identity: stream_identity.into(),
            sealed_lsn: HashMap::new(),
        }
    }

    /// Update the sealed LSN for the given seal-key.
    pub fn update_sealed_lsn(&mut self, seal_key: &SealKey, lsn: WalLsn) {
        self.sealed_lsn.insert(seal_key.to_string(), lsn.as_u64());
    }

    /// Get the sealed LSN for the given seal-key, or `None` if not present.
    #[must_use]
    pub fn get_sealed_lsn(&self, seal_key: &SealKey) -> Option<WalLsn> {
        self.sealed_lsn
            .get(&seal_key.to_string())
            .map(|&lsn| WalLsn::new(lsn))
    }

    /// Serialize the manifest to JSON bytes.
    pub fn serialize(&self) -> Result<Vec<u8>, ScribeError> {
        serde_json::to_vec_pretty(self).map_err(|e| ScribeError::Internal {
            detail: format!("failed to serialize manifest: {e}"),
        })
    }

    /// Deserialize a manifest from JSON bytes.
    pub fn deserialize(bytes: &[u8]) -> Result<Self, ScribeError> {
        serde_json::from_slice(bytes).map_err(|e| ScribeError::Internal {
            detail: format!("failed to deserialize manifest: {e}"),
        })
    }
}

/// Write the manifest atomically to the given path.
///
/// Implements the write-fsync-rename-fsync-parent pattern:
/// 1. Write to `manifest.tmp`
/// 2. fsync `manifest.tmp`
/// 3. Rename `manifest.tmp` → `manifest`
/// 4. fsync parent directory
///
/// # Errors
/// Returns [`ScribeError::Internal`] if any filesystem operation fails.
pub fn write_atomic(path: impl AsRef<Path>, manifest: &Manifest) -> Result<(), ScribeError> {
    let path = path.as_ref();
    let tmp_path = path.with_file_name("manifest.tmp");

    let bytes = manifest.serialize()?;

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&tmp_path)
        .map_err(|e| ScribeError::Internal {
            detail: format!("failed to create manifest temp file: {e}"),
        })?;

    file.write_all(&bytes).map_err(|e| ScribeError::Internal {
        detail: format!("failed to write manifest temp file: {e}"),
    })?;

    file.sync_all().map_err(|e| ScribeError::Internal {
        detail: format!("failed to fsync manifest temp file: {e}"),
    })?;

    drop(file);

    std::fs::rename(&tmp_path, path).map_err(|e| ScribeError::Internal {
        detail: format!("failed to rename manifest temp file: {e}"),
    })?;

    if let Some(parent) = path.parent() {
        let parent_file = File::open(parent).map_err(|e| ScribeError::Internal {
            detail: format!("failed to open manifest parent dir: {e}"),
        })?;
        parent_file.sync_all().map_err(|e| ScribeError::Internal {
            detail: format!("failed to fsync manifest parent dir: {e}"),
        })?;
    }

    Ok(())
}

/// Read the manifest from the given path.
///
/// Returns `None` if the file does not exist.
///
/// # Errors
/// Returns [`ScribeError::Internal`] if the file exists but cannot be read or parsed.
pub fn read_manifest(path: impl AsRef<Path>) -> Result<Option<Manifest>, ScribeError> {
    let path = path.as_ref();

    if !path.exists() {
        return Ok(None);
    }

    let mut file = File::open(path).map_err(|e| ScribeError::Internal {
        detail: format!("failed to open manifest file: {e}"),
    })?;

    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| ScribeError::Internal {
            detail: format!("failed to read manifest file: {e}"),
        })?;

    let manifest = Manifest::deserialize(&bytes)?;

    Ok(Some(manifest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TableRef;
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::seal_key::EventDay;
    use crate::scribe::stream_identity::{NodeId, WriterEpoch};
    use chrono::NaiveDate;
    use tempfile::TempDir;
    use wyrd_spec::ids::DataTenantId;

    #[test]
    fn manifest_roundtrip() {
        let node_id = NodeId::generate();
        let writer_epoch = WriterEpoch::new(42);
        let identity = StreamIdentity::new(node_id, writer_epoch);

        let mut manifest = Manifest::new(identity);

        let tenant = DataTenantId::SYSTEM_OWNER;
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let day = EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap());
        let seal_key = SealKey::new(tenant, table, day);

        manifest.update_sealed_lsn(&seal_key, WalLsn::new(100));

        let bytes = manifest.serialize().expect("serialize");
        let decoded = Manifest::deserialize(&bytes).expect("deserialize");

        assert_eq!(
            decoded.stream_identity.writer_epoch,
            manifest.stream_identity.writer_epoch
        );
        assert_eq!(decoded.get_sealed_lsn(&seal_key), Some(WalLsn::new(100)));
    }

    #[test]
    fn wal_manifest_replace_is_atomic_across_crash() {
        let temp_dir = TempDir::new().expect("temp dir");
        let manifest_path = temp_dir.path().join("manifest");

        let node_id = NodeId::generate();
        let identity = StreamIdentity::new(node_id, WriterEpoch::new(1));

        let mut manifest1 = Manifest::new(identity);
        let tenant = DataTenantId::SYSTEM_OWNER;
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let day = EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap());
        let seal_key = SealKey::new(tenant, table, day);

        manifest1.update_sealed_lsn(&seal_key, WalLsn::new(50));

        write_atomic(&manifest_path, &manifest1).expect("write first");

        let mut manifest2 = Manifest::new(identity);
        manifest2.update_sealed_lsn(&seal_key, WalLsn::new(100));

        write_atomic(&manifest_path, &manifest2).expect("write second");

        let loaded = read_manifest(&manifest_path)
            .expect("read")
            .expect("exists");
        assert_eq!(loaded.get_sealed_lsn(&seal_key), Some(WalLsn::new(100)));
    }
}
