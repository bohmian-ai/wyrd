//! The one local Bifrost data root shared by every selected role.
//!
//! `WYRD_BIFROST_DATA_DIR` names a single directory. The root itself is the
//! Scribe WAL base, and therefore also holds the stable node identity written
//! beside the WAL. Every other managed local path is a fixed child of it:
//! Scribe staged members, Scribe output scratch, Oracle spill, and Forge
//! spill.
//!
//! Preparation creates every managed path, takes an exclusive advisory lock on
//! the root, clears Forge spill left by a previous owner, and write-probes each
//! managed directory before any role is constructed, so a second process
//! mounting the same root, or an unwritable child, fails boot instead of
//! surfacing after work was acknowledged.

use std::fs::{File, TryLockError};
use std::io::Error as IoError;
use std::path::{Path, PathBuf};

use vala_bifrost_redux::resources::BifrostVolumeRoots;

/// Directory used when `WYRD_BIFROST_DATA_DIR` is unset.
pub const DEFAULT_BIFROST_DATA_DIR: &str = ".wyrd/bifrost";

/// Lock file held open for the life of the owning process.
const LOCK_FILE_NAME: &str = ".lock";

/// Write-usability probe created and removed in each managed directory.
///
/// The name is only ever used while the root lock is held, so a probe left by a
/// crashed owner is this process's to overwrite rather than foreign content.
const PROBE_FILE_NAME: &str = ".wyrd-write-probe";

/// Why the configured Bifrost data root cannot back this process.
#[derive(Debug, thiserror::Error)]
pub enum BifrostDataRootError {
    /// A managed path could not be created or written, or the lock file could not be opened or locked.
    #[error("Bifrost data root {path} is unusable: {source}")]
    Unusable {
        /// Managed path the failing operation targeted.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: IoError,
    },
    /// Another live process already owns this root.
    #[error(
        "Bifrost data root {path} is already owned by another process; give each replica its own WYRD_BIFROST_DATA_DIR volume"
    )]
    InUse {
        /// Root whose lock is held elsewhere.
        path: PathBuf,
    },
}

/// Prepared, exclusively owned Bifrost data root.
///
/// Dropping the value releases the lock, which is what lets a restarted
/// process reclaim the same root and replay its WAL under the same identity.
#[derive(Debug)]
pub struct BifrostDataRoot {
    /// Volume-governed paths derived from the root.
    roots: BifrostVolumeRoots,
    /// Oracle spill directory; bounded per query by `DataFusion`, not governed.
    oracle_spill: PathBuf,
    /// Forge rewrite spill directory; emptied at every boot under the lock.
    forge_spill: PathBuf,
    /// Open lock file; the advisory lock lives exactly as long as this handle.
    _lock: File,
}

impl BifrostDataRoot {
    /// Creates every managed path below `root`, locks the root exclusively, and
    /// proves each managed directory writable.
    ///
    /// Directories are created before locking so a fresh root needs no manual
    /// provisioning. Creation is idempotent; existing WAL, staged, and identity
    /// contents are never removed here. Forge spill is disposable attempt
    /// scratch that no restart can resume, so once the lock proves no other
    /// owner is spilling into it, every entry left by a previous owner is
    /// removed. Then one probe file is created, written, synced, and removed in
    /// every managed directory, so a read-only child fails boot before any
    /// role can acknowledge work.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostDataRootError::Unusable`] when a managed directory or
    /// the lock file cannot be created or locked, stale Forge spill cannot be
    /// removed, or a directory rejects the write probe, and
    /// [`BifrostDataRootError::InUse`] when another process holds the root.
    pub fn prepare(root: &Path) -> Result<Self, BifrostDataRootError> {
        let roots = BifrostVolumeRoots {
            wal: root.to_path_buf(),
            scribe_stage: root.join("scribe-stage"),
            scribe_output_scratch: root.join("scribe-output-scratch"),
        };
        let oracle_spill = root.join("oracle-spill");
        let forge_spill = root.join("forge-spill");
        for path in [
            &roots.wal,
            &roots.scribe_stage,
            &roots.scribe_output_scratch,
            &oracle_spill,
            &forge_spill,
        ] {
            std::fs::create_dir_all(path).map_err(|source| BifrostDataRootError::Unusable {
                path: path.clone(),
                source,
            })?;
        }
        let lock_path = root.join(LOCK_FILE_NAME);
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|source| BifrostDataRootError::Unusable {
                path: lock_path.clone(),
                source,
            })?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(BifrostDataRootError::InUse {
                    path: root.to_path_buf(),
                });
            }
            Err(TryLockError::Error(source)) => {
                return Err(BifrostDataRootError::Unusable {
                    path: lock_path,
                    source,
                });
            }
        }
        clear_directory(&forge_spill).map_err(|source| BifrostDataRootError::Unusable {
            path: forge_spill.clone(),
            source,
        })?;
        for dir in [
            &roots.wal,
            &roots.scribe_stage,
            &roots.scribe_output_scratch,
            &oracle_spill,
            &forge_spill,
        ] {
            let probe = dir.join(PROBE_FILE_NAME);
            probe_write(&probe).map_err(|source| BifrostDataRootError::Unusable {
                path: probe,
                source,
            })?;
        }
        Ok(Self {
            roots,
            oracle_spill,
            forge_spill,
            _lock: lock,
        })
    }

    /// Scribe WAL base, which also holds the stable node identity.
    #[must_use]
    pub fn wal(&self) -> &Path {
        &self.roots.wal
    }

    /// Oracle query spill directory.
    #[must_use]
    pub fn oracle_spill(&self) -> &Path {
        &self.oracle_spill
    }

    /// Forge rewrite spill directory, empty when preparation returned.
    #[must_use]
    pub fn forge_spill(&self) -> &Path {
        &self.forge_spill
    }

    /// Managed paths in the shape the Bifrost resource detector registers.
    #[must_use]
    pub fn volume_roots(&self) -> BifrostVolumeRoots {
        self.roots.clone()
    }
}

/// Removes every entry beneath `dir`, keeping `dir` itself.
///
/// # Errors
///
/// Returns the first filesystem error listing or removing an entry.
fn clear_directory(dir: &Path) -> Result<(), IoError> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Creates, writes, syncs, and removes `probe`, removing it on failure too.
///
/// # Errors
///
/// Returns the first filesystem error; a failed cleanup after a successful
/// write is reported as well, since it means the directory cannot remove files.
fn probe_write(probe: &Path) -> Result<(), IoError> {
    let written = File::create(probe).and_then(|mut file| {
        std::io::Write::write_all(&mut file, b"wyrd")?;
        file.sync_all()
    });
    let removed = std::fs::remove_file(probe);
    written?;
    removed
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// Every managed path is created beneath the one root.
    ///
    /// # Panics
    ///
    /// Panics when preparation fails or a derived path escapes the root.
    #[test]
    fn prepare_creates_every_managed_path_below_the_root() {
        let base = tempfile::tempdir().expect("root fixture");
        let root = base.path().join("bifrost");
        let prepared = BifrostDataRoot::prepare(&root).expect("fresh root prepares");
        let roots = prepared.volume_roots();
        assert_eq!(prepared.wal(), root);
        assert_eq!(prepared.oracle_spill(), root.join("oracle-spill"));
        assert_eq!(prepared.forge_spill(), root.join("forge-spill"));
        for path in [
            &roots.wal,
            &roots.scribe_stage,
            &roots.scribe_output_scratch,
            prepared.oracle_spill(),
            prepared.forge_spill(),
        ] {
            assert!(
                path.starts_with(&root) && path.is_dir(),
                "{}",
                path.display()
            );
        }
    }

    /// Stale Forge spill is cleared under the root lock while every other
    /// managed child keeps its contents.
    ///
    /// # Panics
    ///
    /// Panics when preparation fails, stale spill survives, or a WAL,
    /// staged, or Oracle spill entry is removed.
    #[test]
    fn prepare_clears_stale_forge_spill() {
        let base = tempfile::tempdir().expect("root fixture");
        let root = base.path();
        let stale_dir = root.join("forge-spill").join("datafusion-stale");
        std::fs::create_dir_all(&stale_dir).expect("stale spill directory");
        std::fs::write(stale_dir.join("run.arrow"), b"spill").expect("stale spill file");
        std::fs::write(root.join("forge-spill").join("loose"), b"spill").expect("loose spill");
        let wal = root.join("retained.wal");
        std::fs::write(&wal, b"wal").expect("retained WAL");
        std::fs::create_dir_all(root.join("oracle-spill")).expect("oracle spill");
        let oracle = root.join("oracle-spill").join("retained");
        std::fs::write(&oracle, b"oracle").expect("oracle spill file");

        let prepared = BifrostDataRoot::prepare(root).expect("root prepares");
        assert_eq!(
            std::fs::read_dir(prepared.forge_spill())
                .expect("forge spill readable")
                .count(),
            0,
            "stale Forge spill is cleared"
        );
        assert!(wal.exists(), "WAL contents are never cleared");
        assert!(oracle.exists(), "only Forge spill is cleared");
    }

    /// A root that cannot be a directory fails with a typed error.
    ///
    /// # Panics
    ///
    /// Panics when a file-backed root prepares or reports the wrong error.
    #[test]
    fn prepare_rejects_an_uncreatable_root() {
        let base = tempfile::tempdir().expect("root fixture");
        let file = base.path().join("occupied");
        std::fs::write(&file, b"not a directory").expect("blocking file");
        let error = BifrostDataRoot::prepare(&file).expect_err("file cannot be a data root");
        assert!(matches!(error, BifrostDataRootError::Unusable { .. }));
    }

    /// A second owner of one root is refused until the first releases it.
    ///
    /// # Panics
    ///
    /// Panics when two owners coexist or a released root cannot be reclaimed.
    #[test]
    fn prepare_refuses_a_root_owned_by_another_replica() {
        let base = tempfile::tempdir().expect("root fixture");
        let first = BifrostDataRoot::prepare(base.path()).expect("first owner");
        let error = BifrostDataRoot::prepare(base.path()).expect_err("second owner refused");
        assert!(matches!(error, BifrostDataRootError::InUse { .. }));
        drop(first);
        BifrostDataRoot::prepare(base.path()).expect("restart reclaims the released root");
    }

    /// An existing read-only managed child fails preparation before composition.
    ///
    /// # Panics
    ///
    /// Panics when a read-only stage directory prepares or reports the wrong error.
    #[cfg(unix)]
    #[test]
    fn prepare_rejects_an_unwritable_managed_child() {
        let base = tempfile::tempdir().expect("root fixture");
        let stage = base.path().join("scribe-stage");
        std::fs::create_dir(&stage).expect("stage fixture");
        std::fs::set_permissions(&stage, std::fs::Permissions::from_mode(0o555))
            .expect("read-only stage");
        let result = BifrostDataRoot::prepare(base.path());
        std::fs::set_permissions(&stage, std::fs::Permissions::from_mode(0o755))
            .expect("restore stage permissions");
        let error = result.expect_err("read-only stage cannot back a data root");
        assert!(
            matches!(&error, BifrostDataRootError::Unusable { path, .. } if path.starts_with(&stage)),
            "{error}"
        );
        assert_eq!(
            std::fs::read_dir(&stage).expect("stage readable").count(),
            0,
            "the probe must leave no file behind"
        );
    }
}
