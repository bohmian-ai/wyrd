//! Bounded DataFusion runtime resources used by Forge rewrites.

use std::path::Path;
use std::sync::Arc;

use datafusion::execution::memory_pool::MemoryPool;
use datafusion::execution::runtime_env::{RuntimeEnv, RuntimeEnvBuilder};

use super::error::ForgeError;
use super::compact::ForgeObjectStore;

/// Owned rewrite dependencies shared by every Forge compaction operation.
pub(crate) struct ForgeRewritePipeline {
    /// Process-wide DataFusion runtime and spill ceiling.
    pub(crate) runtime: ForgeRewriteRuntime,
    /// Staging operator used for bounded input reads.
    pub(crate) staging: Arc<opendal::Operator>,
    /// Testable object-store seam for staged data.
    pub(crate) object_store: Arc<dyn ForgeObjectStore>,
    /// Maximum concurrent readers.
    pub(crate) max_concurrent_reads: usize,
    /// Maximum encoded bytes per output object.
    pub(crate) output_file_bytes: u64,
}

impl std::fmt::Debug for ForgeRewritePipeline {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ForgeRewritePipeline").finish_non_exhaustive()
    }
}

impl ForgeRewritePipeline {
    pub(crate) fn new(
        runtime: ForgeRewriteRuntime,
        staging: Arc<opendal::Operator>,
        object_store: Arc<dyn ForgeObjectStore>,
        max_concurrent_reads: usize,
        output_file_bytes: u64,
    ) -> Result<Self, ForgeError> {
        if max_concurrent_reads == 0 || output_file_bytes == 0 {
            return Err(ForgeError::InvalidConfig { detail: "rewrite limits must be positive".to_owned() });
        }
        Ok(Self { runtime, staging, object_store, max_concurrent_reads, output_file_bytes })
    }
}

/// DataFusion runtime with an owned, bounded spill directory.
pub struct ForgeRewriteRuntime {
    runtime: Arc<RuntimeEnv>,
    spill_dir: tempfile::TempDir,
    spill_limit_bytes: u64,
}

impl ForgeRewriteRuntime {
    /// Construct a runtime rooted under the pod-owned spill directory.
    ///
    /// # Errors
    /// Returns [`ForgeError::InvalidConfig`] for a zero ceiling or filesystem
    /// and DataFusion errors when the runtime cannot be created.
    pub fn new(
        memory_pool: Arc<dyn MemoryPool>,
        pod_spill_root: &Path,
        spill_limit_bytes: u64,
    ) -> Result<Self, ForgeError> {
        if spill_limit_bytes == 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge spill limit must be positive".to_owned(),
            });
        }
        std::fs::create_dir_all(pod_spill_root).map_err(|error| ForgeError::Parquet {
            detail: error.to_string(),
        })?;
        for entry in std::fs::read_dir(pod_spill_root).map_err(|error| ForgeError::Parquet {
            detail: error.to_string(),
        })? {
            let entry = entry.map_err(|error| ForgeError::Parquet {
                detail: error.to_string(),
            })?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("forge-runtime-") {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
        let spill_dir = tempfile::Builder::new()
            .prefix("forge-runtime-")
            .tempdir_in(pod_spill_root)
            .map_err(|error| ForgeError::Parquet {
                detail: error.to_string(),
            })?;
        let runtime = RuntimeEnvBuilder::new()
            .with_memory_pool(memory_pool)
            .with_temp_file_path(spill_dir.path())
            .with_max_temp_directory_size(spill_limit_bytes)
            .build()
            .map_err(ForgeError::DataFusion)?;
        Ok(Self {
            runtime: Arc::new(runtime),
            spill_dir,
            spill_limit_bytes,
        })
    }

    /// Return the configured operation spill ceiling.
    #[must_use]
    pub fn spill_limit_bytes(&self) -> u64 {
        self.spill_limit_bytes
    }

    /// Borrow the DataFusion runtime for an execution context.
    #[must_use]
    pub(crate) fn runtime(&self) -> Arc<RuntimeEnv> {
        Arc::clone(&self.runtime)
    }

    /// Return the owned spill path for diagnostics and tests.
    #[must_use]
    pub(crate) fn spill_path(&self) -> &Path {
        self.spill_dir.path()
    }
}
