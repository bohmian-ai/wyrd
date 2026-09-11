//! Narrow production seams used by the Scribe benchmark.

use std::path::Path;

use bytes::Bytes;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api::QueryTerminalOutcome;

use crate::catalog::TableRef;
use crate::catalog::layout::{TimeGranularity, TimePartition};
use crate::contracts::ScribeError;
use crate::namespaces::BifrostNamespace;
use crate::scribe::seal_key::SealKey;
use crate::scribe::wal::{PreparedWalAppend, WalConfig, WalWriter};

/// Closed Oracle telemetry label domains exported only for benchmark contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OracleTelemetryLabelDomains {
    /// Query class values emitted by production Oracle metrics.
    pub classes: [&'static str; 2],
    /// Admission outcome values emitted by production Oracle metrics.
    pub outcomes: [&'static str; 2],
    /// Admission reason values emitted by production Oracle metrics.
    pub reasons: [&'static str; 9],
    /// Query terminal outcomes emitted by the Oracle stream owner.
    pub terminal_outcomes: [&'static str; 3],
    /// Query cancellation reasons emitted by the Oracle stream owner.
    pub cancellation_reasons: [&'static str; 4],
    /// Fragment terminal outcomes emitted by the Oracle dispatcher.
    pub fragment_outcomes: [&'static str; 2],
}

/// Return the production Oracle closed label domains for projection contracts.
#[must_use]
pub const fn oracle_telemetry_label_domains() -> OracleTelemetryLabelDomains {
    use crate::oracle::telemetry::{
        FragmentOutcome, OracleAdmissionOutcome, OracleAdmissionReason, OracleCancellationReason,
        OracleQueryClassLabel,
    };

    let classes = OracleQueryClassLabel::ALL;
    let outcomes = OracleAdmissionOutcome::ALL;
    let reasons = OracleAdmissionReason::ALL;
    let cancellation_reasons = OracleCancellationReason::ALL;
    let fragment_outcomes = FragmentOutcome::ALL;
    OracleTelemetryLabelDomains {
        classes: [classes[0].as_str(), classes[1].as_str()],
        outcomes: [outcomes[0].as_str(), outcomes[1].as_str()],
        reasons: [
            reasons[0].as_str(),
            reasons[1].as_str(),
            reasons[2].as_str(),
            reasons[3].as_str(),
            reasons[4].as_str(),
            reasons[5].as_str(),
            reasons[6].as_str(),
            reasons[7].as_str(),
            reasons[8].as_str(),
        ],
        terminal_outcomes: [
            query_terminal_outcome_label(QueryTerminalOutcome::Success),
            query_terminal_outcome_label(QueryTerminalOutcome::Degraded),
            query_terminal_outcome_label(QueryTerminalOutcome::Failed),
        ],
        cancellation_reasons: [
            cancellation_reasons[0].as_str(),
            cancellation_reasons[1].as_str(),
            cancellation_reasons[2].as_str(),
            cancellation_reasons[3].as_str(),
        ],
        fragment_outcomes: [fragment_outcomes[0].as_str(), fragment_outcomes[1].as_str()],
    }
}

/// Projects the shared wire terminal enum into the production metric labels.
const fn query_terminal_outcome_label(value: QueryTerminalOutcome) -> &'static str {
    match value {
        QueryTerminalOutcome::Success => "success",
        QueryTerminalOutcome::Degraded => "degraded",
        QueryTerminalOutcome::Failed => "failed",
    }
}

/// A prepared v3 WAL append. Preparation performs the production payload
/// accounting, but no filesystem work.
pub struct PreparedWalAppendFixture {
    append: PreparedWalAppend,
}

impl PreparedWalAppendFixture {
    /// Compute the data payload checksum without touching the filesystem.
    #[must_use]
    pub fn crc32(&self) -> u32 {
        crc32c::crc32c(&self.append.data)
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
    tenant: DataTenantId,
}

impl WalBenchSupport {
    /// Create a real WAL writer rooted at `path`.
    pub fn new(path: impl AsRef<Path>, tenant: DataTenantId) -> Result<Self, ScribeError> {
        let node_id = *uuid::Uuid::now_v7().as_bytes();
        Ok(Self {
            writer: WalWriter::new(path, node_id, 1, WalConfig::default())?,
            tenant,
        })
    }

    /// Prepare a v6 data record without filesystem work.
    pub fn prepare(
        &self,
        batch_id: [u8; 16],
        data: &[u8],
    ) -> Result<PreparedWalAppendFixture, ScribeError> {
        let seal_key = SealKey::new(
            self.tenant,
            TableRef::new(BifrostNamespace::Bifrost, "bench"),
            TimePartition::new(TimeGranularity::Day, chrono::DateTime::UNIX_EPOCH).map_err(
                |error| ScribeError::Internal {
                    detail: format!("benchmark WAL epoch partition is invalid: {error}"),
                },
            )?,
        );
        Ok(PreparedWalAppendFixture {
            append: PreparedWalAppend::new(
                crate::scribe::wal::WalLsn::ZERO,
                batch_id,
                Bytes::copy_from_slice(data),
            )
            .for_slice(seal_key, [0; 32]),
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
