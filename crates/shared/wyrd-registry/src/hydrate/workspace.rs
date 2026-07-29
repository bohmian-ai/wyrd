//! Staging lifecycle and rollback-safe bundle publication.

use std::{
    fs,
    path::{Path, PathBuf},
};

use wyrd_spec::error::WyrdError;

use crate::error::RegistryEngineError;

/// Owns local paths and publication invariants for one hydration operation.
pub(super) struct HydrationWorkspace {
    /// Temporary directory containing the fully materialized bundle.
    staging: PathBuf,
    /// Final directory exposed after successful publication.
    destination: PathBuf,
}

impl HydrationWorkspace {
    /// Validates the destination and creates an isolated sibling staging directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the destination exists as a non-directory, its metadata cannot be
    /// inspected, or the parent and staging directories cannot be created.
    pub(super) fn prepare(destination: &Path) -> Result<Self, WyrdError> {
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(RegistryEngineError::from)
            .map_err(WyrdError::from)?;
        match fs::metadata(destination) {
            Ok(metadata) if !metadata.is_dir() => {
                return Err(WyrdError::RegistryInvalidCardSpec {
                    message: "hydration destination is not a directory".to_owned(),
                    details: serde_json::json!({ "destination": destination }),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(WyrdError::from(RegistryEngineError::from(error))),
        }
        let staging = parent.join(format!(
            ".{}.staging-{}",
            destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("wyrd-state"),
            uuid::Uuid::now_v7()
        ));
        fs::create_dir(&staging)
            .map_err(RegistryEngineError::from)
            .map_err(WyrdError::from)?;
        Ok(Self {
            staging,
            destination: destination.to_path_buf(),
        })
    }

    /// Returns the isolated directory used for bundle materialization.
    pub(super) fn staging(&self) -> &Path {
        &self.staging
    }

    /// Returns the final published bundle location.
    pub(super) fn destination(&self) -> &Path {
        &self.destination
    }

    /// Removes staging best-effort after a failed write or publication.
    pub(super) fn abort(&self) {
        let _ = fs::remove_dir_all(&self.staging);
    }

    /// Promotes staging while restoring the previous bundle on failure.
    ///
    /// A new destination is renamed directly into place. An existing destination is first moved
    /// to a unique sibling backup. The backup is removed only after promotion succeeds; cleanup
    /// failure is logged because the requested bundle is already published.
    ///
    /// # Errors
    ///
    /// Returns an error when promotion fails or the previous destination cannot be restored.
    pub(super) fn publish(&self) -> Result<(), WyrdError> {
        if fs::metadata(&self.destination).is_err() {
            fs::rename(&self.staging, &self.destination)
                .map_err(RegistryEngineError::from)
                .map_err(WyrdError::from)?;
            return Ok(());
        }
        let backup = self.backup_path();
        fs::rename(&self.destination, &backup)
            .map_err(RegistryEngineError::from)
            .map_err(WyrdError::from)?;
        if let Err(error) = promote(&self.staging, &self.destination) {
            return self.restore_after_failure(&backup, error);
        }
        cleanup_backup(&backup, &self.destination);
        Ok(())
    }

    /// Builds a unique backup path beside the destination.
    fn backup_path(&self) -> PathBuf {
        let parent = self.destination.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!(
            ".{}.previous-{}",
            self.destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("wyrd-state"),
            uuid::Uuid::now_v7()
        ))
    }

    /// Restores a previous destination after failed promotion.
    ///
    /// # Errors
    ///
    /// Always returns the publication error when restoration succeeds. Returns an internal error
    /// containing both failures when restoration also fails.
    fn restore_after_failure(
        &self,
        backup: &Path,
        promotion_error: std::io::Error,
    ) -> Result<(), WyrdError> {
        let publication_error = WyrdError::from(RegistryEngineError::from(promotion_error));
        if let Err(error) = restore(backup, &self.destination) {
            let restoration_error = RegistryEngineError::from(error);
            tracing::error!(
                error = %restoration_error,
                destination = %self.destination.display(),
                backup = %backup.display(),
                "hydration publication failed and rollback failed"
            );
            return Err(WyrdError::Internal {
                message: "hydration publication and rollback failed".to_owned(),
                details: serde_json::json!({
                    "publication_error": publication_error.to_string(),
                    "rollback_error": restoration_error.to_string(),
                    "destination": self.destination,
                    "backup": backup,
                }),
            });
        }
        Err(publication_error)
    }
}

/// Renames staging into the final destination, with injectable test failure.
///
/// # Errors
///
/// Returns the underlying filesystem error when promotion fails.
fn promote(staging: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if take_publish_fault(FAIL_PROMOTION) {
        return Err(std::io::Error::other(
            "injected hydration promotion failure",
        ));
    }
    fs::rename(staging, destination)
}

/// Restores the previous bundle, with injectable test failure.
///
/// # Errors
///
/// Returns the underlying filesystem error when restoration fails.
fn restore(backup: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if take_publish_fault(FAIL_RESTORE) {
        return Err(std::io::Error::other(
            "injected hydration restoration failure",
        ));
    }
    fs::rename(backup, destination)
}

/// Removes a previous bundle best-effort after successful publication.
fn cleanup_backup(backup: &Path, destination: &Path) {
    #[cfg(test)]
    let cleanup = if take_publish_fault(FAIL_CLEANUP) {
        Err(std::io::Error::other(
            "injected hydration backup cleanup failure",
        ))
    } else {
        fs::remove_dir_all(backup)
    };
    #[cfg(not(test))]
    let cleanup = fs::remove_dir_all(backup);
    if let Err(error) = cleanup {
        tracing::warn!(
            error = %error,
            backup = %backup.display(),
            destination = %destination.display(),
            "published hydration bundle but could not remove the previous bundle"
        );
    }
}

/// Test fault bit that forces staged bundle promotion to fail.
#[cfg(test)]
const FAIL_PROMOTION: u8 = 1;
/// Test fault bit that forces previous bundle restoration to fail.
#[cfg(test)]
const FAIL_RESTORE: u8 = 2;
/// Test fault bit that forces backup cleanup to fail.
#[cfg(test)]
const FAIL_CLEANUP: u8 = 4;
/// One-shot publication fault set shared by local unit tests.
#[cfg(test)]
static PUBLISH_FAULTS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Consumes one configured publication fault and reports whether it was active.
#[cfg(test)]
fn take_publish_fault(fault: u8) -> bool {
    use std::sync::atomic::Ordering;

    PUBLISH_FAULTS.fetch_and(!fault, Ordering::SeqCst) & fault != 0
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use tempfile::TempDir;

    use super::{FAIL_CLEANUP, FAIL_PROMOTION, FAIL_RESTORE, HydrationWorkspace, PUBLISH_FAULTS};

    /// Serializes tests that mutate the process-wide publication fault flags.
    static PUBLISH_FAULT_LOCK: Mutex<()> = Mutex::new(());

    /// Configures one-shot publication faults for rollback tests.
    fn inject_publish_faults(faults: u8) {
        PUBLISH_FAULTS.store(faults, std::sync::atomic::Ordering::SeqCst);
    }

    /// Writes a marker used to distinguish old and newly published bundles.
    fn write_bundle_marker(directory: &std::path::Path, marker: &str) {
        std::fs::create_dir_all(directory).expect("bundle directory creates");
        std::fs::write(directory.join("marker"), marker).expect("bundle marker writes");
    }

    /// Constructs a workspace around test-controlled staging and destination paths.
    fn test_workspace(
        staging: std::path::PathBuf,
        destination: std::path::PathBuf,
    ) -> HydrationWorkspace {
        HydrationWorkspace {
            staging,
            destination,
        }
    }

    /// Missing staging fails replacement and restores the previous bundle.
    #[test]
    fn failed_replacement_restores_the_previous_bundle() {
        let _publish_fault_guard = PUBLISH_FAULT_LOCK
            .lock()
            .expect("publication fault lock remains available");
        inject_publish_faults(0);
        let temp = TempDir::new().expect("tempdir creates");
        let destination = temp.path().join("bundle");
        let staging = temp.path().join("missing-staging");
        write_bundle_marker(&destination, "previous");

        let error = test_workspace(staging, destination.clone())
            .publish()
            .expect_err("missing staging must fail replacement");

        assert!(
            error
                .to_string()
                .contains("local artifact materialization failed")
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("marker")).expect("previous bundle reads"),
            "previous"
        );
    }

    /// Promotion and restoration failures surface as an unrecoverable publication error.
    #[test]
    fn rollback_failure_is_reported_as_unrecoverable() {
        let _publish_fault_guard = PUBLISH_FAULT_LOCK
            .lock()
            .expect("publication fault lock remains available");
        inject_publish_faults(0);
        let temp = TempDir::new().expect("tempdir creates");
        let destination = temp.path().join("bundle");
        let staging = temp.path().join("staging");
        write_bundle_marker(&destination, "previous");
        write_bundle_marker(&staging, "new");
        inject_publish_faults(FAIL_PROMOTION | FAIL_RESTORE);

        let error = test_workspace(staging, destination.clone())
            .publish()
            .expect_err("injected promotion must fail");

        assert!(
            error
                .to_string()
                .contains("hydration publication and rollback failed")
        );
        assert!(!destination.exists());
        assert!(
            temp.path()
                .read_dir()
                .expect("publication parent reads")
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().contains(".previous-"))
        );
    }

    /// Cleanup failure does not hide a bundle that was already published.
    #[test]
    fn cleanup_failure_does_not_hide_a_published_bundle() {
        let _publish_fault_guard = PUBLISH_FAULT_LOCK
            .lock()
            .expect("publication fault lock remains available");
        inject_publish_faults(0);
        let temp = TempDir::new().expect("tempdir creates");
        let destination = temp.path().join("bundle");
        let staging = temp.path().join("staging");
        write_bundle_marker(&destination, "previous");
        write_bundle_marker(&staging, "new");
        inject_publish_faults(FAIL_CLEANUP);

        test_workspace(staging, destination.clone())
            .publish()
            .expect("cleanup failure is best effort");

        assert_eq!(
            std::fs::read_to_string(destination.join("marker")).expect("new bundle reads"),
            "new"
        );
    }
}
