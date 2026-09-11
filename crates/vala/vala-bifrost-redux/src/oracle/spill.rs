//! Oracle-owned process and query spill runtime lifecycle.
//!
//! The process owner limits cleanup to its private child prefix, while each
//! admitted query receives a fresh `DataFusion` disk manager bounded by the
//! spill share already retained by admission.

use std::path::Path;
use std::sync::Arc;

use datafusion::execution::disk_manager::{DiskManagerBuilder, DiskManagerMode};
use datafusion::execution::memory_pool::MemoryPool;
use datafusion::execution::runtime_env::{RuntimeEnv, RuntimeEnvBuilder};
use wyrd_spec::vala::BifrostError;

const ORACLE_RUNTIME_PREFIX: &str = "oracle-runtime-";

/// Process-lifetime owner of Oracle's pod-local disposable spill directory.
///
/// The owner creates exactly one prefixed child beneath the supplied pod root.
/// Dropping it removes that active child through `TempDir`; the root and
/// unrelated siblings are never owned or removed.
pub struct OracleSpillRuntime {
    /// Active process child used as the parent for query-local `DataFusion` files.
    spill_dir: tempfile::TempDir,
    /// Aggregate pod spill ceiling retained for diagnostics and invariant checks.
    pod_limit_bytes: u64,
}

impl OracleSpillRuntime {
    /// Creates the process spill owner after removing stale owned children.
    ///
    /// Cleanup is prefix-scoped to directories named `oracle-runtime-*` directly
    /// beneath `root`. Files and unrelated directories are preserved.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the ceiling is zero or the root,
    /// stale-child inspection/removal, or active-child creation fails.
    pub fn new(root: &Path, pod_limit_bytes: u64) -> Result<Self, BifrostError> {
        if pod_limit_bytes == 0 {
            return Err(BifrostError::Internal {
                detail: "Oracle spill limit must be positive".to_owned(),
            });
        }
        std::fs::create_dir_all(root).map_err(|error| spill_io_error(&error))?;
        for entry in std::fs::read_dir(root).map_err(|error| spill_io_error(&error))? {
            let entry = entry.map_err(|error| spill_io_error(&error))?;
            let file_type = entry.file_type().map_err(|error| spill_io_error(&error))?;
            if file_type.is_dir()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(ORACLE_RUNTIME_PREFIX)
            {
                std::fs::remove_dir_all(entry.path()).map_err(|error| spill_io_error(&error))?;
            }
        }
        let spill_dir = tempfile::Builder::new()
            .prefix(ORACLE_RUNTIME_PREFIX)
            .tempdir_in(root)
            .map_err(|error| spill_io_error(&error))?;
        Ok(Self {
            spill_dir,
            pod_limit_bytes,
        })
    }

    /// Builds one query-owned runtime over the supplied shared memory pool.
    ///
    /// A zero query share installs a disabled disk manager. A nonzero share uses
    /// the active process child and enforces that exact byte ceiling; it may not
    /// exceed the aggregate pod ceiling retained by this owner.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when a query share exceeds the pod
    /// ceiling, or [`BifrostError::QueryExecutionFailed`] when `DataFusion`
    /// cannot construct the bounded runtime.
    pub(crate) fn build_query_runtime(
        &self,
        memory_pool: Arc<dyn MemoryPool>,
        query_limit_bytes: u64,
    ) -> Result<Arc<RuntimeEnv>, BifrostError> {
        if query_limit_bytes > self.pod_limit_bytes {
            return Err(BifrostError::Internal {
                detail: "Oracle query spill share exceeds the pod spill limit".to_owned(),
            });
        }
        let disk_manager = if query_limit_bytes == 0 {
            DiskManagerBuilder::default().with_mode(DiskManagerMode::Disabled)
        } else {
            DiskManagerBuilder::default()
                .with_mode(DiskManagerMode::Directories(vec![
                    self.spill_dir.path().to_path_buf(),
                ]))
                .with_max_temp_directory_size(query_limit_bytes)
        };
        RuntimeEnvBuilder::new()
            .with_memory_pool(memory_pool)
            .with_disk_manager_builder(disk_manager)
            .build()
            .map(Arc::new)
            .map_err(|_| BifrostError::QueryExecutionFailed)
    }

    /// Returns the active process child for crate tests and test-support inspection.
    #[must_use]
    #[cfg(any(test, feature = "test-support"))]
    pub fn spill_path(&self) -> &Path {
        self.spill_dir.path()
    }
}

/// Projects a local scratch filesystem failure into the private boot error.
fn spill_io_error(error: &std::io::Error) -> BifrostError {
    BifrostError::Internal {
        detail: format!("Oracle spill directory operation failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use datafusion::execution::memory_pool::GreedyMemoryPool;

    use super::*;

    /// A zero pod ceiling cannot create an unbounded Oracle spill owner.
    #[test]
    fn oracle_spill_runtime_rejects_zero_limit() {
        let root = tempfile::tempdir().expect("test spill root must exist");
        let result = OracleSpillRuntime::new(root.path(), 0);
        assert!(matches!(result, Err(BifrostError::Internal { .. })));
        assert_eq!(
            std::fs::read_dir(root.path())
                .expect("test root must remain readable")
                .count(),
            0
        );
    }

    /// Startup removes stale owned directories but preserves unrelated siblings.
    #[test]
    fn oracle_spill_runtime_cleans_only_owned_children() {
        let root = tempfile::tempdir().expect("test spill root must exist");
        let stale = root.path().join("oracle-runtime-stale");
        let unrelated = root.path().join("keep-me");
        std::fs::create_dir_all(&stale).expect("stale owned child must be created");
        std::fs::create_dir_all(&unrelated).expect("unrelated child must be created");
        let active = {
            let runtime = OracleSpillRuntime::new(root.path(), 1_024)
                .expect("bounded spill owner must be created");
            assert!(!stale.exists());
            assert!(unrelated.exists());
            runtime.spill_path().to_path_buf()
        };
        assert!(!active.exists());
        assert!(root.path().exists());
        assert!(unrelated.exists());
    }

    /// A zero query share installs a disk manager that refuses temp files.
    #[test]
    fn oracle_zero_spill_share_disables_temp_files() {
        let root = tempfile::tempdir().expect("test spill root must exist");
        let runtime = OracleSpillRuntime::new(root.path(), 1_024)
            .expect("bounded spill owner must be created");
        let query = runtime
            .build_query_runtime(Arc::new(GreedyMemoryPool::new(1_024)), 0)
            .expect("memory-only runtime must be created");
        assert!(matches!(
            query.disk_manager.create_tmp_file("disabled"),
            Err(datafusion::error::DataFusionError::ResourcesExhausted(_))
        ));
    }

    /// A query runtime rejects spill growth beyond its exact admitted share.
    ///
    /// `DataFusion` 55 enforces the temp-directory quota inside the spill
    /// writer's `Write::write`, so growth past the admitted share surfaces as a
    /// write error naming the limit rather than a post-hoc usage refresh.
    #[test]
    fn oracle_query_runtime_enforces_exact_disk_share() {
        let root = tempfile::tempdir().expect("test spill root must exist");
        let runtime = OracleSpillRuntime::new(root.path(), 1_024)
            .expect("bounded spill owner must be created");
        let query = runtime
            .build_query_runtime(Arc::new(GreedyMemoryPool::new(1_024)), 8)
            .expect("bounded query runtime must be created");
        let file = query
            .disk_manager
            .create_tmp_file("quota")
            .expect("first temporary file must be created");
        let mut writer = file
            .open_writer()
            .expect("admitted spill file must open a writer");
        writer
            .write_all(&[0; 8])
            .expect("write at the exact quota must succeed");
        assert_eq!(file.size(), Some(8));
        let over_quota = writer
            .write(&[0])
            .expect_err("write past the admitted share must be refused");
        assert!(
            over_quota
                .to_string()
                .contains("exceeded the allowable limit"),
            "refusal must name the exceeded disk limit, got: {over_quota}"
        );
        assert_eq!(
            file.size(),
            Some(8),
            "a refused write must not grow the accounted file"
        );
        drop(writer);
        drop(file);
        drop(query);
        assert_eq!(
            std::fs::read_dir(runtime.spill_path())
                .expect("active spill directory must remain readable")
                .count(),
            0
        );
    }
}
