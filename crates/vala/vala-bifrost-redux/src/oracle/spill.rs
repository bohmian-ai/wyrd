//! Oracle-owned process and query spill runtime lifecycle.
//!
//! The process owner limits cleanup to its private child prefix, while each
//! admitted query receives a fresh `DataFusion` disk manager bounded by its
//! per-query spill limit. Every byte a query writes is also charged to the
//! process scratch ledger shared with the other disposable-scratch owners on
//! the data-root filesystem, so concurrent spillers cannot together exceed
//! its budget. Spill is a limit on bytes actually written, never a
//! reservation: an unspilled query holds no disk.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::disk_manager::{DiskManager, DiskManagerBuilder, DiskManagerMode};
use datafusion::execution::memory_pool::MemoryPool;
use datafusion::execution::runtime_env::{RuntimeEnv, RuntimeEnvBuilder};
use datafusion::execution::{SpillFile, SpillWriter, TempFileFactory};
use futures_util::Stream;
use wyrd_runtime::resources::{ByteCharge, ByteLedger, CpuLanes};

use crate::resources::BifrostResourceError;

const ORACLE_RUNTIME_PREFIX: &str = "oracle-runtime-";

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
    /// The process scratch ledger every written spill byte is charged to.
    scratch: ByteLedger,
}

impl OracleSpillRuntime {
    /// Creates the process spill owner after removing stale owned children.
    ///
    /// Cleanup is prefix-scoped to directories named `oracle-runtime-*` directly
    /// beneath `root`. Files and unrelated directories are preserved. Every
    /// query's written spill bytes are charged to `scratch`.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::Unavailable`] when the root,
    /// stale-child inspection/removal, or active-child creation fails.
    pub fn new(root: &Path, scratch: ByteLedger) -> Result<Self, BifrostResourceError> {
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
        Ok(Self { spill_dir, scratch })
    }

    /// Returns the active process child for crate tests and test-support inspection.
    #[must_use]
    #[cfg(any(test, feature = "test-support"))]
    pub fn spill_path(&self) -> &Path {
        self.spill_dir.path()
    }
}

/// Builds one query-owned runtime over the supplied shared memory pool, and
/// returns with it the query's own spill disk manager when spill is enabled.
///
/// This is the only construction of an Oracle query runtime: the resource
/// capability that issues a grant calls it once, and the grant carries the
/// result. With no spill owner, or a zero `spill_limit_bytes`, the runtime has
/// a disabled disk manager, so an operator that would spill fails with a typed
/// resource error instead of writing ungoverned files. Otherwise it spills
/// under the owner's active child up to exactly `spill_limit_bytes`, and each
/// byte is charged to the process scratch ledger before it is written. Merge
/// fan-in keeps `DataFusion`'s default: the query's memory pool is the only
/// bound, and a merge the pool refuses fails with a typed resource error.
///
/// The runtime's own disk manager only delegates file creation, so the spill
/// directories belong to the returned query disk manager; the query envelope
/// removes them through it when the query ends.
///
/// # Errors
///
/// Returns [`BifrostResourceError::Unavailable`] when `DataFusion` cannot
/// construct the bounded runtime.
pub(crate) fn build_query_runtime(
    spill: Option<&OracleSpillRuntime>,
    memory_pool: Arc<dyn MemoryPool>,
    spill_limit_bytes: u64,
) -> Result<(Arc<RuntimeEnv>, Option<Arc<DiskManager>>), BifrostResourceError> {
    let mut query_disk = None;
    let disk_manager = match spill {
        Some(spill) if spill_limit_bytes > 0 => {
            let (factory, query) =
                charged_spill_factory(spill.spill_dir.path(), spill_limit_bytes, &spill.scratch)
                    .map_err(|error| BifrostResourceError::Unavailable {
                        detail: format!("Oracle query disk manager construction failed: {error}"),
                    })?;
            query_disk = Some(query);
            DiskManagerBuilder::default().with_temp_file_factory(factory)
        }
        _ => DiskManagerBuilder::default().with_mode(DiskManagerMode::Disabled),
    };
    RuntimeEnvBuilder::new()
        .with_memory_pool(memory_pool)
        .with_disk_manager_builder(disk_manager)
        .build()
        .map(|runtime| (Arc::new(runtime), query_disk))
        .map_err(|error| BifrostResourceError::Unavailable {
            detail: format!("Oracle query runtime construction failed: {error}"),
        })
}

/// A spill file factory for one spiller, an Oracle query or a Forge rewrite
/// attempt: its files are created under `dir` by its own disk manager,
/// capped at `limit_bytes`, and every written byte is also charged to the
/// process `scratch` ledger before it is written. Returns the factory and
/// that disk manager, which owns the spill directories and removes them when
/// it drops.
///
/// # Errors
///
/// Returns `DataFusion`'s error when the disk manager cannot be built.
pub(crate) fn charged_spill_factory(
    dir: &Path,
    limit_bytes: u64,
    scratch: &ByteLedger,
) -> DataFusionResult<(Arc<dyn TempFileFactory>, Arc<DiskManager>)> {
    let disk = Arc::new(
        DiskManagerBuilder::default()
            .with_mode(DiskManagerMode::Directories(vec![dir.to_path_buf()]))
            .with_max_temp_directory_size(limit_bytes)
            .build()?,
    );
    let factory = Arc::new(ChargedSpillFactory {
        query: Arc::clone(&disk),
        scratch: scratch.clone(),
    });
    Ok((factory, disk))
}

/// One spiller's spill files: created by its own capped disk manager, with
/// every written byte also charged to the process scratch ledger.
struct ChargedSpillFactory {
    /// The query's disk manager, which enforces the per-query cap and deletes
    /// each file when its last reference drops.
    query: Arc<DiskManager>,
    /// The process scratch ledger shared with the other scratch owners.
    scratch: ByteLedger,
}

impl TempFileFactory for ChargedSpillFactory {
    fn create_temp_file(&self, description: &str) -> DataFusionResult<Arc<dyn SpillFile>> {
        let file = self.query.create_tmp_file(description)?;
        Ok(Arc::new(ChargedSpillFile {
            path: file.path().map(Path::to_path_buf),
            file: Some(file),
            charge: Arc::new(Mutex::new(self.scratch.empty_charge())),
        }))
    }
}

/// One spill file and the scratch bytes written to it.
///
/// The charge returns only after the file is deleted. A file that survives
/// deletion keeps its bytes charged for the rest of the process, because the
/// disk still holds them; the next boot's prefix cleanup reclaims it.
struct ChargedSpillFile {
    /// The file's local path, checked after deletion.
    path: Option<PathBuf>,
    /// The query disk manager's file; `None` only while dropping.
    file: Option<Arc<dyn SpillFile>>,
    /// Bytes written through every writer of this file.
    charge: Arc<Mutex<ByteCharge>>,
}

impl ChargedSpillFile {
    /// The wrapped file.
    ///
    /// # Panics
    /// Never in practice: the file is taken only in `Drop`.
    fn file(&self) -> &Arc<dyn SpillFile> {
        self.file
            .as_ref()
            .expect("a spill file is present until it drops")
    }
}

#[async_trait::async_trait]
impl SpillFile for ChargedSpillFile {
    fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn size(&self) -> Option<u64> {
        self.file().size()
    }

    fn read_stream(
        &self,
    ) -> DataFusionResult<Pin<Box<dyn Stream<Item = DataFusionResult<Bytes>> + Send>>> {
        self.file().read_stream()
    }

    fn open_writer(&self) -> DataFusionResult<Box<dyn SpillWriter>> {
        Ok(Box::new(ChargedSpillWriter {
            inner: self.file().open_writer()?,
            charge: Arc::clone(&self.charge),
        }))
    }
}

impl Drop for ChargedSpillFile {
    fn drop(&mut self) {
        drop(self.file.take());
        let Some(path) = self.path.as_deref().filter(|path| path.exists()) else {
            return;
        };
        let Ok(mut charge) = self.charge.lock() else {
            return;
        };
        let retained = charge.bytes();
        tracing::error!(
            path = %path.display(),
            retained_bytes = retained,
            "a spill file survived deletion; its scratch bytes stay charged"
        );
        std::mem::forget(charge.split(retained));
    }
}

/// A spill writer that charges the process scratch ledger before each write
/// and does its disk IO off the query's CPU lane.
struct ChargedSpillWriter {
    /// The query disk manager's writer, which enforces the per-query cap.
    inner: Box<dyn SpillWriter>,
    /// The owning file's charge.
    charge: Arc<Mutex<ByteCharge>>,
}

impl std::io::Write for ChargedSpillWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let requested = u64::try_from(buf.len()).map_err(std::io::Error::other)?;
        let mut charge = self
            .charge
            .lock()
            .map_err(|_| std::io::Error::other("spill scratch charge lock poisoned"))?;
        charge.try_grow(requested).map_err(|refusal| {
            std::io::Error::other(format!(
                "spill exceeds the process scratch budget: {refusal}"
            ))
        })?;
        let written = CpuLanes::off_lane(|| self.inner.write(buf));
        let kept = written
            .as_ref()
            .map_or(0, |written| u64::try_from(*written).unwrap_or(requested));
        charge.shrink(requested.saturating_sub(kept));
        written
    }

    fn flush(&mut self) -> std::io::Result<()> {
        CpuLanes::off_lane(|| self.inner.flush())
    }
}

impl SpillWriter for ChargedSpillWriter {
    fn finish(&mut self) -> DataFusionResult<()> {
        CpuLanes::off_lane(|| self.inner.finish())
    }
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
            let runtime = OracleSpillRuntime::new(root.path(), ByteLedger::new(u64::MAX))
                .expect("bounded spill owner must be created");
            assert!(!stale.exists());
            assert!(unrelated.exists());
            runtime.spill_path().to_path_buf()
        };
        assert!(!active.exists());
        assert!(root.path().exists());
        assert!(unrelated.exists());
    }

    /// Concurrent spillers share one process scratch ledger: each keeps its
    /// per-query cap, together they cannot exceed the ledger, a deleted file
    /// returns its bytes, and a file that survives deletion keeps them.
    ///
    /// # Panics
    /// Panics when the aggregate or per-query cap is not enforced, or bytes
    /// return while their file is still on disk.
    #[test]
    fn spillers_share_the_process_scratch_ledger() {
        use std::os::unix::fs::PermissionsExt as _;
        let root = tempfile::tempdir().expect("test spill root must exist");
        let scratch = ByteLedger::new(12);
        let runtime = OracleSpillRuntime::new(root.path(), scratch.clone())
            .expect("bounded spill owner must be created");
        let pool = || Arc::new(GreedyMemoryPool::new(1_024)) as Arc<dyn MemoryPool>;
        let (first, _) =
            build_query_runtime(Some(&runtime), pool(), 8).expect("first query runtime");
        let (second, _) =
            build_query_runtime(Some(&runtime), pool(), 8).expect("second query runtime");
        let first_file = first
            .disk_manager
            .create_tmp_file("first")
            .expect("first file");
        let mut first_writer = first_file.open_writer().expect("first writer");
        first_writer
            .write_all(&[0; 8])
            .expect("the first query fits its cap and the ledger");
        assert!(
            first_writer.write(&[0]).is_err(),
            "the per-query cap still bounds one query"
        );
        assert_eq!(scratch.used(), 8, "a refused write charges nothing");
        let second_file = second
            .disk_manager
            .create_tmp_file("second")
            .expect("second file");
        let mut second_writer = second_file.open_writer().expect("second writer");
        second_writer
            .write_all(&[0; 4])
            .expect("the ledger has four bytes left");
        let refused = second_writer
            .write(&[0])
            .expect_err("the shared ledger refuses past the filesystem budget");
        assert!(
            refused.to_string().contains("process scratch budget"),
            "refusal must name the scratch budget, got: {refused}"
        );
        assert_eq!(scratch.used(), 12);
        drop((first_writer, first_file));
        assert_eq!(scratch.used(), 4, "a deleted file returns its bytes");
        second_writer
            .write_all(&[0; 4])
            .expect("returned bytes are borrowable by another query");
        drop(second_writer);
        let path = second_file.path().expect("local spill file").to_path_buf();
        let directory = path.parent().expect("spill directory").to_path_buf();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o500))
            .expect("the spill directory becomes undeletable");
        drop(second_file);
        let survived = path.exists();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .expect("the spill directory is restored");
        assert!(survived, "the fixture must make deletion fail");
        assert_eq!(
            scratch.used(),
            8,
            "an undeleted file keeps its bytes charged"
        );
        drop((first, second));
    }

    /// A spill write inside a laned query poll returns the poll's CPU lane
    /// to other work instead of holding it across the disk IO.
    ///
    /// # Panics
    /// Panics when the lane is still held after the write.
    #[test]
    fn spill_writes_return_the_query_lane() {
        use std::pin::pin;
        use std::task::{Context, Poll, Waker};

        use wyrd_runtime::resources::{ProcessResources, Workload};

        let root = tempfile::tempdir().expect("test spill root must exist");
        let runtime = OracleSpillRuntime::new(root.path(), ByteLedger::new(1 << 20))
            .expect("bounded spill owner must be created");
        let pool = Arc::new(GreedyMemoryPool::new(1_024)) as Arc<dyn MemoryPool>;
        let (query, _) = build_query_runtime(Some(&runtime), pool, 1 << 20).expect("query runtime");
        let file = query.disk_manager.create_tmp_file("laned").expect("file");
        let mut writer = file.open_writer().expect("writer");
        let resources = ProcessResources::new(std::num::NonZeroUsize::MIN, 1 << 30, 1 << 30);
        let workloads = resources.workloads();
        let operation = workloads.begin(Workload::Bifrost, None, None);
        let lanes = workloads.lanes();
        let mut poll = pin!(lanes.laned(operation.context(), async {
            let before = lanes.free();
            writer.write_all(&[0; 64]).expect("the spill write fits");
            (before, lanes.free())
        }));
        let Poll::Ready((before, after)) =
            poll.as_mut().poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("the poll completes");
        };
        assert_eq!(before, 0, "the poll runs on the only lane");
        assert_eq!(after, 1, "the spill write returned the lane");
        assert_eq!(lanes.free(), 1, "the lane is returned exactly once");
    }

    /// A zero query share installs a disk manager that refuses temp files.
    #[test]
    fn oracle_zero_spill_share_disables_temp_files() {
        let root = tempfile::tempdir().expect("test spill root must exist");
        let runtime = OracleSpillRuntime::new(root.path(), ByteLedger::new(u64::MAX))
            .expect("bounded spill owner must be created");
        let (query, query_disk) =
            build_query_runtime(Some(&runtime), Arc::new(GreedyMemoryPool::new(1_024)), 0)
                .expect("memory-only runtime must be created");
        assert!(query_disk.is_none());
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
        let runtime = OracleSpillRuntime::new(root.path(), ByteLedger::new(u64::MAX))
            .expect("bounded spill owner must be created");
        let (query, _) =
            build_query_runtime(Some(&runtime), Arc::new(GreedyMemoryPool::new(1_024)), 8)
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
        let (sibling, _) =
            build_query_runtime(Some(&runtime), Arc::new(GreedyMemoryPool::new(1_024)), 8)
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
