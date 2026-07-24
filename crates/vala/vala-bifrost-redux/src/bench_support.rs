//! Narrow production seams used by the Scribe benchmark.

use std::path::Path;

use bytes::Bytes;
use wyrd_spec::ids::DataTenantId;

use crate::contracts::ScribeError;
use crate::scribe::wal::{PreparedWalAppend, WalConfig, WalWriter};

/// A prepared v3 WAL append. Preparation performs the production payload
/// accounting, but no filesystem work.
pub struct PreparedWalAppendFixture {
    append: PreparedWalAppend,
}

impl PreparedWalAppendFixture {
    /// Compute payload checksums without touching the filesystem.
    #[must_use]
    pub fn crc32(&self) -> (u32, u32) {
        let audit = crc32c::crc32c(&self.append.audit);
        let data = crc32c::crc32c(&self.append.data);
        (audit, data)
    }
}

/// Result of one production WAL append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalAppendEvidence {
    /// Encoded bytes appended to the WAL.
    pub bytes: u64,
}

/// Evidence from an opportunistic production WAL group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalGroupEvidence {
    /// Number of prepared frames appended.
    pub frames: u64,
    /// Number of physical segments touched.
    pub segments: u64,
    /// Number of sync calls issued.
    pub fsyncs: u64,
    /// Encoded WAL bytes appended.
    pub bytes: u64,
}

/// Production WAL harness for independent component measurements.
pub struct WalBenchSupport {
    writer: WalWriter,
}

impl WalBenchSupport {
    /// Create a real WAL writer rooted at `path`.
    pub fn new(path: impl AsRef<Path>, tenant: DataTenantId) -> Result<Self, ScribeError> {
        let node_id = *uuid::Uuid::now_v7().as_bytes();
        Ok(Self {
            writer: WalWriter::new(path, node_id, 1, tenant, WalConfig::default())?,
        })
    }

    /// Prepare a v3 audit/data record without filesystem work.
    pub fn prepare(
        &self,
        batch_id: [u8; 16],
        audit: &[u8],
        data: &[u8],
    ) -> Result<PreparedWalAppendFixture, ScribeError> {
        Ok(PreparedWalAppendFixture {
            append: PreparedWalAppend::new(
                crate::scribe::wal::WalLsn::ZERO,
                batch_id,
                Bytes::copy_from_slice(audit),
                Bytes::copy_from_slice(data),
            ),
        })
    }

    /// Append one prepared record through production WAL IO.
    pub fn append_no_sync(
        &self,
        fixture: PreparedWalAppendFixture,
    ) -> Result<WalAppendEvidence, ScribeError> {
        let result = self.writer.append_prepared(fixture.append)?;
        Ok(WalAppendEvidence {
            bytes: result.encoded_bytes,
        })
    }

    /// Sync the current production WAL segment.
    pub fn sync_alone(&self) -> Result<(), ScribeError> {
        self.writer.sync_data()
    }

    /// Append one prepared frame and sync its production segment.
    pub fn append_and_sync(
        &self,
        fixture: PreparedWalAppendFixture,
    ) -> Result<WalAppendEvidence, ScribeError> {
        let result = self.writer.append_prepared(fixture.append)?;
        WalWriter::sync_segments(&result.touched_segments)?;
        Ok(WalAppendEvidence {
            bytes: result.encoded_bytes,
        })
    }

    /// Append exactly one group and issue one sync per distinct touched segment.
    pub fn append_group_one_sync(
        &self,
        fixtures: Vec<PreparedWalAppendFixture>,
    ) -> Result<WalGroupEvidence, ScribeError> {
        let mut touched_segments = Vec::new();
        let mut bytes = 0_u64;
        let frames = u64::try_from(fixtures.len()).map_err(|_| ScribeError::Internal {
            detail: "benchmark group frame count overflow".to_owned(),
        })?;
        for fixture in fixtures {
            let result = self.writer.append_prepared(fixture.append)?;
            bytes = bytes.saturating_add(result.encoded_bytes);
            touched_segments.extend(result.touched_segments);
        }
        let mut paths = Vec::new();
        for segment in &touched_segments {
            if !paths.iter().any(|path| path == segment.path()) {
                paths.push(segment.path().to_owned());
            }
        }
        let segments = u64::try_from(paths.len()).map_err(|_| ScribeError::Internal {
            detail: "benchmark segment count overflow".to_owned(),
        })?;
        WalWriter::sync_segments(&touched_segments)?;
        Ok(WalGroupEvidence {
            frames,
            segments,
            fsyncs: segments,
            bytes,
        })
    }

    /// Return production WAL bytes currently retained on disk.
    #[must_use]
    pub fn bytes_on_disk(&self) -> u64 {
        self.writer.bytes_on_disk()
    }
}
