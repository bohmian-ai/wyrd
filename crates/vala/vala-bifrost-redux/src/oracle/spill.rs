//! Oracle-owned process and query spill runtime lifecycle.
//!
//! The process owner limits cleanup to its private child prefix, while each
//! admitted query receives a fresh `DataFusion` disk manager bounded by its
//! per-query spill limit. Spill is a limit on bytes actually written, never a
//! reservation: an unspilled query holds no disk.

use std::path::Path;
use std::sync::Arc;

use datafusion::execution::disk_manager::{DiskManagerBuilder, DiskManagerMode};
use datafusion::execution::memory_pool::MemoryPool;
use datafusion::execution::runtime_env::{RuntimeEnv, RuntimeEnvBuilder};

use crate::resources::BifrostResourceError;

const ORACLE_RUNTIME_PREFIX: &str = "oracle-runtime-";

/// Read-buffer budget one spill file is assumed to need while it is merged.
///
/// A merge holds one non-spillable read buffer per spill file it reads. The
/// widest measured case, 8192-row batches of 6 KiB keys, needed about 50 MB.
const SPILL_MERGE_FILE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// Most spill files one sort merge phase may read, derived from its query.
///
/// `DataFusion`'s default is unbounded: a merge greedily reserves a read
/// buffer per spill file until the pool refuses, so a few merging partitions
/// can hold the whole query limit and starve their sibling sorters (measured
/// at the 4 GiB pod floor: four merges at about 288 MB each). Here every
/// partition's merges together may use at most half the query's memory limit,
/// at [`SPILL_MERGE_FILE_BUDGET_BYTES`] per file, never below the two files a
/// merge needs. The cap therefore widens with the pod: larger limits merge in
/// fewer passes, and the floor stays safe.
fn spill_merge_fan_in(memory_limit_bytes: usize, target_partitions: usize) -> usize {
    (memory_limit_bytes / 2 / target_partitions.max(1) / SPILL_MERGE_FILE_BUDGET_BYTES).max(2)
}

/// Process-lifetime owner of Oracle's pod-local disposable spill directory.
///
/// The owner creates exactly one prefixed child beneath the supplied pod root.
/// Dropping it removes that active child through `TempDir`; the root and
/// unrelated siblings are never owned or removed. Resource composition creates
/// the only instance, so a second owner can never clear the first one's child.
#[derive(Debug)]
pub struct OracleSpillRuntime {
    /// Active process child used as the parent for query-local `DataFusion` files.
    spill_dir: tempfile::TempDir,
}

impl OracleSpillRuntime {
    /// Creates the process spill owner after removing stale owned children.
    ///
    /// Cleanup is prefix-scoped to directories named `oracle-runtime-*` directly
    /// beneath `root`. Files and unrelated directories are preserved.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Unavailable`] when the root,
    /// stale-child inspection/removal, or active-child creation fails.
    pub fn new(root: &Path) -> Result<Self, BifrostResourceError> {
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
        Ok(Self { spill_dir })
    }

    /// Returns the active process child for crate tests and test-support inspection.
    #[must_use]
    #[cfg(any(test, feature = "test-support"))]
    pub fn spill_path(&self) -> &Path {
        self.spill_dir.path()
    }
}

/// Builds one query-owned runtime over the supplied shared memory pool.
///
/// This is the only construction of an Oracle query runtime: the resource
/// capability that issues a grant calls it once, and the grant carries the
/// result. With no spill owner, or a zero `spill_limit_bytes`, the runtime has
/// a disabled disk manager, so an operator that would spill fails with a typed
/// resource error instead of writing ungoverned files. Otherwise it spills
/// under the owner's active child up to exactly `spill_limit_bytes`, and each
/// sort merge phase's fan-in is capped from the query's memory limit and
/// partition count; see [`spill_merge_fan_in`].
///
/// # Errors
///
/// Returns [`BifrostResourceError::Unavailable`] when `DataFusion` cannot
/// construct the bounded runtime.
pub(crate) fn build_query_runtime(
    spill: Option<&OracleSpillRuntime>,
    memory_pool: Arc<dyn MemoryPool>,
    memory_limit_bytes: usize,
    target_partitions: usize,
    spill_limit_bytes: u64,
) -> Result<Arc<RuntimeEnv>, BifrostResourceError> {
    let disk_manager = match spill {
        Some(spill) if spill_limit_bytes > 0 => DiskManagerBuilder::default()
            .with_mode(DiskManagerMode::Directories(vec![
                spill.spill_dir.path().to_path_buf(),
            ]))
            .with_max_temp_directory_size(spill_limit_bytes)
            .with_max_spill_merge_fan_in(spill_merge_fan_in(memory_limit_bytes, target_partitions)),
        _ => DiskManagerBuilder::default().with_mode(DiskManagerMode::Disabled),
    };
    RuntimeEnvBuilder::new()
        .with_memory_pool(memory_pool)
        .with_disk_manager_builder(disk_manager)
        .build()
        .map(Arc::new)
        .map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("Oracle query runtime construction failed: {error}"),
        })
}

/// Projects a local scratch filesystem failure into the resource error.
fn spill_io_error(error: &std::io::Error) -> BifrostResourceError {
    BifrostResourceError::Unavailable {
        detail: format!("Oracle spill directory operation failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use datafusion::execution::memory_pool::GreedyMemoryPool;

    use super::*;

    /// Startup removes stale owned directories but preserves unrelated siblings.
    #[test]
    fn oracle_spill_runtime_cleans_only_owned_children() {
        let root = tempfile::tempdir().expect("test spill root must exist");
        let stale = root.path().join("oracle-runtime-stale");
        let unrelated = root.path().join("keep-me");
        std::fs::create_dir_all(&stale).expect("stale owned child must be created");
        std::fs::create_dir_all(&unrelated).expect("unrelated child must be created");
        let active = {
            let runtime =
                OracleSpillRuntime::new(root.path()).expect("bounded spill owner must be created");
            assert!(!stale.exists());
            assert!(unrelated.exists());
            runtime.spill_path().to_path_buf()
        };
        assert!(!active.exists());
        assert!(root.path().exists());
        assert!(unrelated.exists());
    }

    /// Merge fan-in widens with the query limit and narrows with partitions.
    ///
    /// # Panics
    ///
    /// Panics when a derived cap differs from the half-limit budget rule.
    #[test]
    fn spill_merge_fan_in_scales_with_the_query() {
        const GIB: usize = 1024 * 1024 * 1024;
        // 4 GiB pod floor: 1.5 GiB limit.
        assert_eq!(spill_merge_fan_in(3 * GIB / 2, 4), 3);
        assert_eq!(spill_merge_fan_in(3 * GIB / 2, 16), 2);
        // 32 GiB pod: 15.5 GiB limit.
        assert_eq!(spill_merge_fan_in(31 * GIB / 2, 8), 15);
        assert_eq!(spill_merge_fan_in(0, 0), 2);
    }

    /// A spilling query runtime carries the derived fan-in.
    ///
    /// # Panics
    ///
    /// Panics when the runtime cannot be built or carries another fan-in.
    #[test]
    fn spilling_query_runtime_caps_merge_fan_in() {
        let root = tempfile::tempdir().expect("test spill root must exist");
        let runtime =
            OracleSpillRuntime::new(root.path()).expect("bounded spill owner must be created");
        let limit = 8 * 1024 * 1024 * 1024;
        let query = build_query_runtime(
            Some(&runtime),
            Arc::new(GreedyMemoryPool::new(1_024)),
            limit,
            4,
            8,
        )
        .expect("bounded query runtime must be created");
        assert_eq!(
            query.disk_manager.max_spill_merge_fan_in(),
            spill_merge_fan_in(limit, 4)
        );
    }

    /// A zero query share installs a disk manager that refuses temp files.
    #[test]
    fn oracle_zero_spill_share_disables_temp_files() {
        let root = tempfile::tempdir().expect("test spill root must exist");
        let runtime =
            OracleSpillRuntime::new(root.path()).expect("bounded spill owner must be created");
        let query = build_query_runtime(
            Some(&runtime),
            Arc::new(GreedyMemoryPool::new(1_024)),
            1_024,
            1,
            0,
        )
        .expect("memory-only runtime must be created");
        assert!(matches!(
            query.disk_manager.create_tmp_file("disabled"),
            Err(datafusion::error::DataFusionError::ResourcesExhausted(_))
        ));
    }

    /// Exceeding the native per-query spill limit fails only that query.
    ///
    /// `DataFusion` 55 enforces the temp-directory quota inside the spill
    /// writer's `Write::write`, so growth past the limit surfaces as a write
    /// error naming the limit. A sibling query keeps spilling, and dropping the
    /// failed query removes its temporary files.
    ///
    /// # Panics
    ///
    /// Panics when the limit is not enforced, the sibling is refused, or a
    /// temporary file outlives its query.
    #[test]
    fn native_spill_limit_fails_one_query_and_cleans_up() {
        let root = tempfile::tempdir().expect("test spill root must exist");
        let runtime =
            OracleSpillRuntime::new(root.path()).expect("bounded spill owner must be created");
        let query = build_query_runtime(
            Some(&runtime),
            Arc::new(GreedyMemoryPool::new(1_024)),
            1_024,
            1,
            8,
        )
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
        let sibling = build_query_runtime(
            Some(&runtime),
            Arc::new(GreedyMemoryPool::new(1_024)),
            1_024,
            1,
            8,
        )
        .expect("sibling query runtime must be created");
        let sibling_file = sibling
            .disk_manager
            .create_tmp_file("sibling")
            .expect("a sibling query still spills");
        sibling_file
            .open_writer()
            .expect("sibling spill file must open a writer")
            .write_all(&[0; 8])
            .expect("the sibling's own limit is untouched");
        drop((sibling_file, sibling));
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
