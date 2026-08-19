//! Write-Ahead Log (WAL) for Scribe — crash-consistent framed records.
//!
//! Every prepared day slice emits one self-describing v4 `SLICE` record. After
//! all ordered slices are durable, one `COMMIT` record authenticates their
//! complete digest before replay may restore any of them.
//!
//! Segment format per `scribe/00-architecture.md §Crash-consistent WAL format`:
//! - Fixed 64-byte header with magic, version, `node_id`, `writer_epoch`, CRC
//! - Fixed 72-byte framed-record headers plus payload and CRC32C trailers
//! - `SLICE` records carrying tenant/batch/ordinal identity and payload
//! - `COMMIT` records carrying the closed slice-set digest
//! - Atomic segment rollover: write-fsync-rename-fsync-parent
//! - Torn-tail truncation only for incomplete final writes; CRC, structure, and
//!   monotonicity failures in a committed prefix fail closed
//!
//! Version 4 is the only accepted format. Earlier segment versions are rejected
//! before replay.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use num_traits::ToPrimitive;
use rustix::fs::statvfs;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::resources::{ScribeMemoryLease, ScribeResources};
use crate::scribe::seal_key::{EventDay, SealKey};
use crate::scribe::stream_identity::StreamIdentity;

/// Record one physical WAL append attempt while preserving its original result.
fn record_wal_append(result: &Result<(), ScribeError>, bytes: usize, started: Instant) {
    let outcome = if result.is_ok() { "success" } else { "failed" };
    metrics::counter!("bifrost_scribe_wal_append_total", "outcome" => outcome).increment(1);
    metrics::histogram!("bifrost_scribe_wal_append_seconds", "outcome" => outcome)
        .record(started.elapsed().as_secs_f64());
    if result.is_ok() {
        metrics::counter!("bifrost_scribe_wal_append_bytes_total").increment(bytes as u64);
    }
}

/// Record one physical WAL fsync attempt while preserving its original result.
fn record_wal_fsync(result: &Result<(), ScribeError>, started: Instant) {
    let outcome = if result.is_ok() { "success" } else { "failed" };
    metrics::counter!("bifrost_scribe_wal_fsync_total", "outcome" => outcome).increment(1);
    metrics::histogram!("bifrost_scribe_wal_fsync_seconds", "outcome" => outcome)
        .record(started.elapsed().as_secs_f64());
}

#[cfg(test)]
static WAL_COUNT_ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static WAL_REPLAY_PAYLOAD_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
thread_local! {
    static WAL_ENCODE_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static WAL_WALK_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static WAL_PARTIAL_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// One-shot failure after record mutation but before provisional ownership transfer.
    static WAL_FAIL_VOLUME_RETAIN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// One-shot failure after durable rename and parent fsync but before final open.
    static WAL_FAIL_SEGMENT_FINAL_OPEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static WAL_RECOVERY_SYNC_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// One-shot injection that forces the next `retain_segments` call on this
    /// thread to fail before it mutates any retention refcount. Self-consuming
    /// (`replace(false)`), mirroring `WAL_PARTIAL_WRITE`, so a retried retain
    /// after the forced failure succeeds. Lets a seal-path test drive the
    /// state-A retention-failure branch without a poisoned mutex.
    static WAL_FAIL_RETAIN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(test)]
static WAL_FAULT_LOCK: Mutex<()> = Mutex::new(());

/// Arm a one-shot forced failure of the next `retain_segments` call on the
/// calling thread.
///
/// Consumed on the next `retain_segments`, which then fails before mutating any
/// retention refcount; subsequent calls (a retry) succeed. Seal-path tests in
/// sibling modules use this to drive the retention-failure branch that
/// otherwise fires only on a poisoned `retirement_refs` mutex. The flag is
/// thread-local, so arm it on the same thread that invokes the retention.
#[cfg(test)]
pub(crate) fn arm_retain_failure_for_test() {
    WAL_FAIL_RETAIN.with(|flag| flag.set(true));
}

/// WAL log sequence number — monotonic per `(node_id, writer_epoch)` stream.
///
/// LSNs are never comparable across different pods or different epochs of the
/// same pod. Each epoch starts a fresh LSN sequence at 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WalLsn(u64);

impl WalLsn {
    /// Construct a WAL LSN from a raw u64.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the raw u64 value.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// The zero LSN — the first LSN in a new epoch stream.
    pub const ZERO: Self = Self(0);
}

impl std::fmt::Display for WalLsn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// WAL segment header (fixed 64 bytes).
///
/// Written once at segment creation; immutable for the segment's lifetime.
#[derive(Debug, Clone)]
pub struct SegmentHeader {
    /// Magic bytes: 0x57415257 ("WRAW" — Wyrd Redux Arrow Wal).
    pub magic: u32,
    /// Format version (4 is the only accepted implementation).
    pub version: u16,
    /// Reserved field (must be zero).
    pub reserved: u16,
    /// Stable pod identifier (uuid).
    pub node_id: [u8; 16],
    /// Writer epoch from `vala.cluster_nodes.fencing_token`.
    pub writer_epoch: i64,
    /// Monotonic segment sequence number per stream.
    pub seg_seq: u64,
    /// Fixed pod-local shard that owns this segment.
    pub shard_id: u8,
    /// CRC32C over the preceding 60 bytes.
    pub crc32c: u32,
}

const WAL_MAGIC: u32 = 0x5741_5257; // "WRAW"
/// The only persisted WAL version accepted by this Scribe.
const WAL_VERSION: u16 = 4;
const SEGMENT_HEADER_SIZE: usize = 64;
#[cfg(test)]
const APPEND_FRAME_MAGIC_V3: [u8; 4] = *b"SWF3";
/// Fixed byte length of every v4 record header before its payload and CRC.
const RECORD_HEADER_SIZE: usize = 72;
/// `u16` wire encoding of the immutable v4 record-header length.
const RECORD_HEADER_SIZE_U16: u16 = 72;
/// Exact v4 record magic.
const RECORD_MAGIC: [u8; 8] = *b"WYRDWAL4";
/// A record carrying one ordered batch slice.
const RECORD_FLAG_SLICE: u32 = 1;
/// A record carrying the digest that closes one ordered slice set.
const RECORD_FLAG_COMMIT: u32 = 2;
/// Maximum number of eligible WAL files considered during one V1 recovery.
const WAL_RECOVERY_FILE_LIMIT: usize = 4_096;
/// Maximum UTF-8 path length accepted for one eligible recovery file.
const WAL_RECOVERY_PATH_LIMIT: usize = 4_096;

impl SegmentHeader {
    /// Construct a new segment header.
    #[must_use]
    pub fn new(node_id: [u8; 16], writer_epoch: i64, seg_seq: u64, shard_id: u8) -> Self {
        let mut header = Self {
            magic: WAL_MAGIC,
            version: WAL_VERSION,
            reserved: 0,
            node_id,
            writer_epoch,
            seg_seq,
            shard_id,
            crc32c: 0,
        };
        header.crc32c = header.compute_crc();
        header
    }

    /// Encode the header to bytes (64 bytes).
    #[must_use]
    pub fn encode(&self) -> [u8; SEGMENT_HEADER_SIZE] {
        let mut buf = [0u8; SEGMENT_HEADER_SIZE];
        buf[0..4].copy_from_slice(&self.magic.to_le_bytes());
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..8].copy_from_slice(&self.reserved.to_le_bytes());
        buf[8..24].copy_from_slice(&self.node_id);
        buf[24..32].copy_from_slice(&self.writer_epoch.to_le_bytes());
        buf[32] = self.shard_id;
        buf[40..48].copy_from_slice(&self.seg_seq.to_le_bytes());
        // crc32c is stored at [60..64] when we finalize, but not yet
        buf
    }

    /// Decode a segment header from bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the magic, version, reserved
    /// fields, shard identifier, or header checksum violates WAL v4 framing.
    pub fn decode(buf: &[u8; SEGMENT_HEADER_SIZE]) -> Result<Self, ScribeError> {
        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != WAL_MAGIC {
            return Err(ScribeError::Internal {
                detail: format!("invalid WAL magic: {magic:#x}"),
            });
        }

        let version = u16::from_le_bytes([buf[4], buf[5]]);
        let reserved = u16::from_le_bytes([buf[6], buf[7]]);
        let mut node_id = [0u8; 16];
        node_id.copy_from_slice(&buf[8..24]);
        let writer_epoch = i64::from_le_bytes([
            buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
        ]);
        let shard_id = buf[32];
        if shard_id >= 16 {
            return Err(ScribeError::Internal {
                detail: format!("invalid WAL shard id: {shard_id}"),
            });
        }
        if buf[33..40].iter().any(|byte| *byte != 0) || buf[48..60].iter().any(|byte| *byte != 0) {
            return Err(ScribeError::Internal {
                detail: "segment header reserved bytes non-zero".to_owned(),
            });
        }
        let seg_seq = u64::from_le_bytes([
            buf[40], buf[41], buf[42], buf[43], buf[44], buf[45], buf[46], buf[47],
        ]);

        let crc32c = u32::from_le_bytes([buf[60], buf[61], buf[62], buf[63]]);

        let header = Self {
            magic,
            version,
            reserved,
            node_id,
            writer_epoch,
            seg_seq,
            shard_id,
            crc32c,
        };

        // Verify CRC
        if header.compute_crc() != crc32c {
            return Err(ScribeError::Internal {
                detail: "segment header CRC mismatch".to_string(),
            });
        }

        if version != WAL_VERSION {
            return Err(ScribeError::UnsupportedWalVersion { version });
        }

        if reserved != 0 {
            return Err(ScribeError::Internal {
                detail: "segment header reserved bits non-zero".to_string(),
            });
        }

        Ok(header)
    }

    fn compute_crc(&self) -> u32 {
        // CRC is computed over the first 60 bytes (everything except the CRC itself at [60..64])
        let encoded = self.encode();
        crc32c_hash(&encoded[0..60])
    }

    /// Write the complete header including CRC to the given writer.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let mut buf = self.encode();
        // Overwrite the last 4 bytes with the CRC
        buf[60..64].copy_from_slice(&self.crc32c.to_le_bytes());
        writer.write_all(&buf)
    }
}

/// WAL record — a v4 slice or commit framed entry.
///
/// Its fixed 72-byte little-endian header is followed by its declared payload
/// and a CRC32C trailer over both. A record never carries a mixed flag set.
///
#[derive(Debug, Clone)]
pub struct WalRecord {
    /// LSN for this record (monotonic per stream).
    pub lsn: WalLsn,
    /// Exactly one of `RECORD_FLAG_SLICE` or `RECORD_FLAG_COMMIT`.
    pub flags: u32,
    /// Authenticated tenant owning the record.
    pub tenant_id: [u8; 16],
    /// Batch ID for deduplication of the complete audit-plus-data record.
    pub batch_id: [u8; 16],
    /// Zero-based slice ordinal, or the closing slice count for a commit.
    pub slice_index: u32,
    /// Total slices in the batch's ordered slice set.
    pub slice_count: u32,
    /// Self-describing slice payload.
    pub payload: Vec<u8>,
}

/// Validated fixed v4 record header retained while replay selects the next LSN.
///
/// The header contains no scalable payload allocation. Replay can therefore
/// peek across the fixed shard set, choose the stream-wide next record, and
/// allocate only that record's declared payload.
#[derive(Debug)]
struct DecodedWalRecordHeader {
    /// Exact header bytes included in the frame CRC.
    encoded: [u8; RECORD_HEADER_SIZE],
    /// Stream-wide log sequence number.
    lsn: WalLsn,
    /// Validated SLICE or COMMIT flag.
    flags: u32,
    /// Authenticated tenant identity.
    tenant_id: [u8; 16],
    /// Stable logical batch identity.
    batch_id: [u8; 16],
    /// Slice ordinal or terminal slice count.
    slice_index: u32,
    /// Closed slice-set cardinality.
    slice_count: u32,
    /// Checked payload length declared by the frame.
    payload_len: u32,
}

impl DecodedWalRecordHeader {
    /// Returns the exact payload bytes that replay must admit before allocation.
    #[must_use]
    fn payload_bytes(&self) -> usize {
        usize::try_from(self.payload_len).expect("u32 fits usize on supported targets")
    }
}

/// Explicit WAL sizing used by every Scribe writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalConfig {
    /// Maximum segment bytes before a non-empty segment rolls.
    pub segment_bytes: u64,
    /// Optional explicit WAL disk budget. Filesystem capacity remains the
    /// upper bound when this is present.
    pub disk_limit_bytes: Option<u64>,
}

impl WalConfig {
    /// Construct a validated WAL configuration.
    pub fn new(segment_bytes: u64) -> Result<Self, ScribeError> {
        if segment_bytes == 0 {
            return Err(ScribeError::Internal {
                detail: "WAL segment_bytes must be greater than zero".to_owned(),
            });
        }
        Ok(Self {
            segment_bytes,
            disk_limit_bytes: None,
        })
    }

    /// Apply an explicit WAL disk budget.
    pub fn with_disk_limit(mut self, disk_limit_bytes: Option<u64>) -> Result<Self, ScribeError> {
        if disk_limit_bytes == Some(0) {
            return Err(ScribeError::Internal {
                detail: "WAL disk_limit_bytes must be greater than zero".to_owned(),
            });
        }
        self.disk_limit_bytes = disk_limit_bytes;
        Ok(self)
    }
}

impl Default for WalConfig {
    fn default() -> Self {
        Self {
            segment_bytes: 512 * 1024 * 1024,
            disk_limit_bytes: None,
        }
    }
}

/// A day slice prepared without allocating the final WAL frame.
#[derive(Debug)]
pub(crate) struct PreparedWalAppend {
    pub(crate) lsn: WalLsn,
    pub(crate) batch_id: [u8; 16],
    pub(crate) audit: Bytes,
    pub(crate) data: Bytes,
    pub(crate) seal_key: Option<SealKey>,
    pub(crate) schema_fingerprint: [u8; 32],
    /// Stable logical Arrow digest excluding volatile managed correlation columns.
    pub(crate) logical_data_digest: [u8; 32],
    /// Stable logical Arrow buffer length covered by `logical_data_digest`.
    pub(crate) logical_data_len: u32,
    pub(crate) shard_id: Option<u8>,
    /// Ordered ordinal assigned by the preprocessor before WAL allocation.
    pub(crate) slice_index: u32,
    /// Total slices in this batch's closed set.
    pub(crate) slice_count: u32,
    /// Terminal digest when this prepared record closes a batch slice set.
    pub(crate) commit_digest: Option<[u8; 32]>,
    /// Authenticated tenant encoded by a terminal commit record.
    pub(crate) commit_tenant: Option<[u8; 16]>,
}

/// Result of one append, including every segment whose bytes were touched.
#[derive(Debug)]
pub(crate) struct WalAppendResult {
    pub(crate) lsn: WalLsn,
    #[cfg(feature = "bench-support")]
    pub(crate) encoded_bytes: u64,
    pub(crate) touched_segments: Vec<Arc<WalSegment>>,
    /// Digest of the exact payload protected by this record's CRC.
    pub(crate) payload_digest: [u8; 32],
    /// Exact payload length represented by the digest.
    pub(crate) payload_len: u32,
}

impl PreparedWalAppend {
    pub(crate) fn new(lsn: WalLsn, batch_id: [u8; 16], audit: Bytes, data: Bytes) -> Self {
        Self {
            lsn,
            batch_id,
            audit,
            data,
            seal_key: None,
            schema_fingerprint: [0; 32],
            logical_data_digest: [0; 32],
            logical_data_len: 0,
            shard_id: None,
            slice_index: 0,
            slice_count: 1,
            commit_digest: None,
            commit_tenant: None,
        }
    }

    /// Builds a terminal v4 commit record without allocating a duplicate digest buffer.
    pub(crate) fn commit(
        batch_id: [u8; 16],
        tenant_id: [u8; 16],
        slice_count: u32,
        digest: [u8; 32],
    ) -> Self {
        Self {
            lsn: WalLsn::ZERO,
            batch_id,
            audit: Bytes::new(),
            data: Bytes::new(),
            seal_key: None,
            schema_fingerprint: [0; 32],
            logical_data_digest: [0; 32],
            logical_data_len: 0,
            shard_id: None,
            slice_index: slice_count,
            slice_count,
            commit_digest: Some(digest),
            commit_tenant: Some(tenant_id),
        }
    }

    pub(crate) fn for_slice(mut self, seal_key: SealKey, schema_fingerprint: [u8; 32]) -> Self {
        self.seal_key = Some(seal_key);
        self.schema_fingerprint = schema_fingerprint;
        self
    }

    /// Attaches the stable logical Arrow identity computed before WAL append.
    pub(crate) fn with_logical_data_identity(mut self, digest: [u8; 32], len: u32) -> Self {
        self.logical_data_digest = digest;
        self.logical_data_len = len;
        self
    }

    /// Updates the ordinal after batch splitting fixes the complete slice count.
    pub(crate) fn assign_slice_ordinal(&mut self, slice_index: u32, slice_count: u32) {
        self.slice_index = slice_index;
        self.slice_count = slice_count;
    }

    /// Computes the exact retained retry identity without allocating a WAL payload.
    ///
    /// # Errors
    ///
    /// Returns an internal invariant error when the prepared slice lacks its
    /// self-describing key or a fixed-width WAL-v4 field cannot represent it.
    pub(crate) fn payload_identity(&self) -> Result<ScribeAppendPayloadIdentity, ScribeError> {
        if self.commit_digest.is_some() {
            return Err(ScribeError::Internal {
                detail: "terminal WAL COMMIT has no retained slice identity".to_owned(),
            });
        }
        let _seal_key = self
            .seal_key
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "prepared WAL append is missing its self-describing seal key".to_owned(),
            })?;
        Ok(ScribeAppendPayloadIdentity {
            batch_id: self.batch_id,
            schema_fingerprint: self.schema_fingerprint,
            data_digest: self.logical_data_digest,
            data_len: self.logical_data_len,
            slice_index: self.slice_index,
            slice_count: self.slice_count,
        })
    }

    fn assign_lsn(&mut self, lsn: WalLsn) {
        self.lsn = lsn;
    }

    #[cfg(test)]
    /// Materializes the prepared record for bounded replay and framing tests.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when required tenant/seal identity is
    /// absent or self-describing payload encoding fails.
    fn record(&self) -> Result<WalRecord, ScribeError> {
        #[cfg(test)]
        if WAL_COUNT_ACTIVE.load(Ordering::Relaxed) {
            WAL_ENCODE_COUNT.with(|count| count.set(count.get() + 1));
        }
        if let Some(digest) = self.commit_digest {
            let tenant_id = self.commit_tenant.ok_or_else(|| ScribeError::Internal {
                detail: "WAL v4 commit is missing its authenticated tenant".to_owned(),
            })?;
            return Ok(WalRecord::commit(
                self.lsn,
                tenant_id,
                self.batch_id,
                self.slice_count,
                digest,
            ));
        }
        let seal_key = self
            .seal_key
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "prepared WAL append is missing its self-describing seal key".to_owned(),
            })?;
        let payload =
            encode_slice_payload(seal_key, self.schema_fingerprint, &self.audit, &self.data)?;
        Ok(WalRecord::slice(
            self.lsn,
            *seal_key.tenant.as_uuid().as_bytes(),
            self.batch_id,
            self.slice_index,
            self.slice_count,
            payload,
        ))
    }

    /// Returns the exact framed WAL record length without materializing it.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when payload sizing fails or framed
    /// length arithmetic overflows.
    pub(crate) fn encoded_len(&self) -> Result<usize, ScribeError> {
        self.payload_len()?
            .checked_add(RECORD_HEADER_SIZE + 4)
            .ok_or_else(|| ScribeError::Internal {
                detail: "encoded WAL record length overflow".to_owned(),
            })
    }

    /// Returns the exact uncompressed v4 packet payload length.
    ///
    /// This includes the self-describing seal-key, schema, audit, and Arrow
    /// fields that are part of the durable packet. Rotation must use this
    /// value rather than a JSON or audit-plus-Arrow proxy so live projection
    /// and replay apply identical packet semantics.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a self-describing field exceeds
    /// its v4 width or checked packet-length arithmetic overflows.
    pub(crate) fn uncompressed_len(&self) -> Result<usize, ScribeError> {
        self.payload_len()
    }

    /// Returns the exact payload length without materializing the payload.
    ///
    /// # Errors
    ///
    /// Returns an internal invariant error when a self-describing field exceeds
    /// its fixed WAL width or checked length arithmetic overflows.
    fn payload_len(&self) -> Result<usize, ScribeError> {
        if self.commit_digest.is_some() {
            return Ok(32);
        }
        let seal_key = self
            .seal_key
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "prepared WAL append is missing its self-describing seal key".to_owned(),
            })?;
        let table_len =
            u16::try_from(seal_key.table.namespace.as_str().len() + 1 + seal_key.table.name.len())
                .map_err(|_| ScribeError::Internal {
                    detail: "WAL table FQN exceeds v3 payload limits".to_owned(),
                })?;
        let mut day_len = 0;
        write!(
            CountWriter(&mut day_len),
            "{}",
            seal_key.day.as_date().format("%Y-%m-%d")
        )
        .map_err(|_| ScribeError::Internal {
            detail: "WAL partition day exceeds v3 payload limits".to_owned(),
        })?;
        let day_len = u8::try_from(day_len).map_err(|_| ScribeError::Internal {
            detail: "WAL partition day exceeds v3 payload limits".to_owned(),
        })?;
        let audit_len = u32::try_from(self.audit.len()).map_err(|_| ScribeError::Internal {
            detail: "WAL audit payload exceeds v3 payload limits".to_owned(),
        })?;
        let data_len = u32::try_from(self.data.len()).map_err(|_| ScribeError::Internal {
            detail: "WAL Arrow payload exceeds v3 payload limits".to_owned(),
        })?;
        let payload_len = 63usize
            .saturating_add(usize::from(table_len))
            .saturating_add(usize::from(day_len))
            .saturating_add(usize::try_from(audit_len).expect("invariant: u32 fits usize"))
            .saturating_add(usize::try_from(data_len).expect("invariant: u32 fits usize"));
        Ok(payload_len)
    }
}

/// Streams one prepared record from borrowed payload owners into its segment.
///
/// # Errors
///
/// Returns an invariant error for invalid fixed-width metadata or an IO error
/// from the segment. No request-sized WAL payload or frame allocation occurs.
fn append_prepared_borrowed(
    segment: &WalSegment,
    prepared: &PreparedWalAppend,
    encoded_len: usize,
) -> Result<([u8; 32], u32), ScribeError> {
    let payload_len =
        u32::try_from(prepared.payload_len()?).map_err(|_| ScribeError::Internal {
            detail: "WAL v4 payload exceeds its u32 record bound".to_owned(),
        })?;
    let (flags, tenant_id) = if prepared.commit_digest.is_some() {
        (
            RECORD_FLAG_COMMIT,
            prepared
                .commit_tenant
                .ok_or_else(|| ScribeError::Internal {
                    detail: "WAL v4 commit is missing its authenticated tenant".to_owned(),
                })?,
        )
    } else {
        let seal_key = prepared
            .seal_key
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "prepared WAL append is missing its self-describing seal key".to_owned(),
            })?;
        (RECORD_FLAG_SLICE, *seal_key.tenant.as_uuid().as_bytes())
    };
    let header = encode_record_header(
        prepared.lsn,
        flags,
        tenant_id,
        prepared.batch_id,
        prepared.slice_index,
        prepared.slice_count,
        payload_len,
    );
    let write_parts = |parts: &[&[u8]]| -> Result<[u8; 32], ScribeError> {
        let mut digest = Sha256::new();
        let mut crc = crc32c::crc32c(&header);
        for part in parts {
            digest.update(part);
            crc = crc32c::crc32c_append(crc, part);
        }
        segment.append_borrowed(&header, parts, crc.to_le_bytes(), encoded_len)?;
        Ok(digest.finalize().into())
    };
    let payload_digest = if let Some(digest) = prepared.commit_digest.as_ref() {
        write_parts(&[digest.as_slice()])?
    } else {
        let seal_key = prepared
            .seal_key
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "prepared WAL append is missing its self-describing seal key".to_owned(),
            })?;
        let namespace = seal_key.table.namespace.as_str().as_bytes();
        let name = seal_key.table.name.as_bytes();
        let tenant_id = *seal_key.tenant.as_uuid().as_bytes();
        let table_len = u16::try_from(namespace.len() + 1 + name.len())
            .map_err(|_| ScribeError::Internal {
                detail: "WAL table FQN exceeds v3 payload limits".to_owned(),
            })?
            .to_le_bytes();
        let mut day = FixedText::<16>::new();
        write!(day, "{}", seal_key.day.as_date().format("%Y-%m-%d")).map_err(|_| {
            ScribeError::Internal {
                detail: "WAL partition day exceeds v3 payload limits".to_owned(),
            }
        })?;
        let day_len = [
            u8::try_from(day.as_bytes().len()).map_err(|_| ScribeError::Internal {
                detail: "WAL partition day exceeds v3 payload limits".to_owned(),
            })?,
        ];
        let audit_len = u32::try_from(prepared.audit.len())
            .map_err(|_| ScribeError::Internal {
                detail: "WAL audit payload exceeds v3 payload limits".to_owned(),
            })?
            .to_le_bytes();
        let data_len = u32::try_from(prepared.data.len())
            .map_err(|_| ScribeError::Internal {
                detail: "WAL Arrow payload exceeds v3 payload limits".to_owned(),
            })?
            .to_le_bytes();
        let parts: [&[u8]; 13] = [
            &SLICE_PAYLOAD_MAGIC,
            &tenant_id,
            &table_len,
            namespace,
            b".",
            name,
            &day_len,
            day.as_bytes(),
            &prepared.schema_fingerprint,
            &audit_len,
            &prepared.audit,
            &data_len,
            &prepared.data,
        ];
        write_parts(&parts)?
    };
    Ok((payload_digest, payload_len))
}

const SLICE_PAYLOAD_MAGIC: [u8; 4] = *b"S3SL";

struct CountWriter<'a>(&'a mut usize);

impl FmtWrite for CountWriter<'_> {
    /// Adds one fragment's byte length to the saturating count.
    ///
    /// # Errors
    ///
    /// This counting implementation is infallible and saturates at
    /// [`usize::MAX`].
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        *self.0 = self.0.saturating_add(value.len());
        Ok(())
    }
}

/// Stack-backed formatter for the fixed-width partition-day field.
struct FixedText<const N: usize> {
    /// Inline bytes written so far.
    bytes: [u8; N],
    /// Live prefix length within `bytes`.
    len: usize,
}

impl<const N: usize> FixedText<N> {
    /// Constructs an empty inline formatter.
    const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    /// Returns the initialized inline prefix.
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl<const N: usize> FmtWrite for FixedText<N> {
    /// Copies one formatted fragment into the fixed inline capacity.
    ///
    /// # Errors
    ///
    /// Returns [`std::fmt::Error`] when the fragment exceeds remaining space.
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(std::fmt::Error)?;
        let destination = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        destination.copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(test)]
fn encode_slice_payload(
    seal_key: &SealKey,
    schema_fingerprint: [u8; 32],
    audit: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, ScribeError> {
    encode_slice_payload_parts(
        seal_key.tenant.as_uuid().as_bytes(),
        &seal_key.table.fqn(),
        &seal_key.day.as_string(),
        &schema_fingerprint,
        audit,
        data,
    )
}

#[cfg(test)]
fn encode_slice_payload_parts(
    tenant: &[u8; 16],
    table_fqn: &str,
    partition_day: &str,
    schema_fingerprint: &[u8; 32],
    audit: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, ScribeError> {
    let table_len = u16::try_from(table_fqn.len()).map_err(|_| ScribeError::Internal {
        detail: "WAL table FQN exceeds v3 payload limits".to_owned(),
    })?;
    let day_len = u8::try_from(partition_day.len()).map_err(|_| ScribeError::Internal {
        detail: "WAL partition day exceeds v3 payload limits".to_owned(),
    })?;
    let audit_len = u32::try_from(audit.len()).map_err(|_| ScribeError::Internal {
        detail: "WAL audit payload exceeds v3 payload limits".to_owned(),
    })?;
    let data_len = u32::try_from(data.len()).map_err(|_| ScribeError::Internal {
        detail: "WAL Arrow payload exceeds v3 payload limits".to_owned(),
    })?;
    let capacity = 4usize
        .saturating_add(16)
        .saturating_add(2)
        .saturating_add(table_fqn.len())
        .saturating_add(1)
        .saturating_add(partition_day.len())
        .saturating_add(32)
        .saturating_add(4)
        .saturating_add(audit.len())
        .saturating_add(4)
        .saturating_add(data.len());
    let mut payload = Vec::with_capacity(capacity);
    payload.extend_from_slice(&SLICE_PAYLOAD_MAGIC);
    payload.extend_from_slice(tenant);
    payload.extend_from_slice(&table_len.to_le_bytes());
    payload.extend_from_slice(table_fqn.as_bytes());
    payload.push(day_len);
    payload.extend_from_slice(partition_day.as_bytes());
    payload.extend_from_slice(schema_fingerprint);
    payload.extend_from_slice(&audit_len.to_le_bytes());
    payload.extend_from_slice(audit);
    payload.extend_from_slice(&data_len.to_le_bytes());
    payload.extend_from_slice(data);
    Ok(payload)
}

/// Decoded metadata and payloads from one v3 WAL slice.
#[derive(Debug, Clone)]
pub(crate) struct DecodedSlicePayload {
    pub(crate) seal_key: SealKey,
    pub(crate) schema_fingerprint: [u8; 32],
    pub(crate) audit: Vec<u8>,
    pub(crate) data: Vec<u8>,
}

/// Cursor for the length-delimited fields in one v3 slice payload.
///
/// The cursor centralizes bounds checking so field decoders can describe the
/// payload schema without repeating offset arithmetic. It never advances past
/// the supplied payload, and it rejects arithmetic overflow as corruption.
struct SlicePayloadReader<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> SlicePayloadReader<'a> {
    /// Create a reader positioned at the first payload byte.
    fn new(payload: &'a [u8]) -> Self {
        Self { payload, offset: 0 }
    }

    /// Take exactly `length` bytes or report a truncated/corrupt payload.
    fn take(&mut self, length: usize) -> Result<&'a [u8], ScribeError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| ScribeError::Internal {
                detail: "WAL slice payload offset overflow".to_owned(),
            })?;
        let value = self
            .payload
            .get(self.offset..end)
            .ok_or_else(|| ScribeError::Internal {
                detail: "WAL slice payload is truncated".to_owned(),
            })?;
        self.offset = end;
        Ok(value)
    }

    /// Read a little-endian `u16` length field.
    fn read_u16(&mut self, detail: &'static str) -> Result<usize, ScribeError> {
        let bytes = self.take(2)?;
        let bytes: [u8; 2] = bytes.try_into().map_err(|_| ScribeError::Internal {
            detail: detail.to_owned(),
        })?;
        Ok(usize::from(u16::from_le_bytes(bytes)))
    }

    /// Read a little-endian `u32` length field.
    fn read_u32(&mut self, detail: &'static str) -> Result<usize, ScribeError> {
        let bytes = self.take(4)?;
        let bytes: [u8; 4] = bytes.try_into().map_err(|_| ScribeError::Internal {
            detail: detail.to_owned(),
        })?;
        usize::try_from(u32::from_le_bytes(bytes)).map_err(|_| ScribeError::Internal {
            detail: detail.to_owned(),
        })
    }

    /// Read a UTF-8 field and preserve the field-specific corruption detail.
    fn read_utf8(&mut self, length: usize, field: &str) -> Result<String, ScribeError> {
        let bytes = self.take(length)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|error| ScribeError::Internal {
                detail: format!("WAL {field} is not UTF-8: {error}"),
            })
    }

    /// Read a length-prefixed owned byte field.
    fn read_bytes_u32(&mut self, detail: &'static str) -> Result<Vec<u8>, ScribeError> {
        let length = self.read_u32(detail)?;
        Ok(self.take(length)?.to_vec())
    }

    /// Verify that the payload has no unparsed trailing bytes.
    fn finish(self) -> Result<(), ScribeError> {
        if self.offset != self.payload.len() {
            return Err(ScribeError::Internal {
                detail: "WAL v3 slice payload has trailing bytes".to_owned(),
            });
        }
        Ok(())
    }
}

/// Decode the tenant identity encoded in a v3 slice payload.
fn decode_slice_tenant(bytes: &[u8]) -> Result<DataTenantId, ScribeError> {
    let tenant_bytes: [u8; 16] = bytes.try_into().map_err(|_| ScribeError::Internal {
        detail: "WAL tenant id decode failed".to_owned(),
    })?;
    let tenant_uuid = uuid::Uuid::from_bytes(tenant_bytes);
    DataTenantId::try_from(tenant_uuid).map_err(|error| ScribeError::Internal {
        detail: format!("WAL v3 tenant id is invalid: {error}"),
    })
}

/// Decode and validate the canonical logical table reference.
fn decode_slice_table(table_fqn: &str) -> Result<TableRef, ScribeError> {
    TableRef::parse_fqn(table_fqn).ok_or_else(|| ScribeError::Internal {
        detail: format!("WAL table FQN is invalid: {table_fqn}"),
    })
}

/// Decode and validate the partition day encoded as `YYYY-MM-DD`.
fn decode_slice_day(day: &str) -> Result<EventDay, ScribeError> {
    let day = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|error| {
        ScribeError::Internal {
            detail: format!("WAL partition day is invalid: {error}"),
        }
    })?;
    Ok(EventDay::new(day))
}

/// Metadata decoded before the variable-size audit and Arrow fields.
struct DecodedSliceMetadata {
    seal_key: SealKey,
    schema_fingerprint: [u8; 32],
}

/// Validate the v3 marker and decode the tenant/table/day/schema identity.
fn decode_slice_metadata(
    reader: &mut SlicePayloadReader<'_>,
) -> Result<DecodedSliceMetadata, ScribeError> {
    if reader.take(SLICE_PAYLOAD_MAGIC.len())? != SLICE_PAYLOAD_MAGIC {
        return Err(ScribeError::Internal {
            detail: "WAL v3 slice magic mismatch".to_owned(),
        });
    }
    let tenant = decode_slice_tenant(reader.take(16)?)?;
    let table_len = reader.read_u16("WAL table length decode failed")?;
    let table_fqn = reader.read_utf8(table_len, "table FQN")?;
    let table = decode_slice_table(&table_fqn)?;
    let day_len = usize::from(reader.take(1)?[0]);
    let day = decode_slice_day(&reader.read_utf8(day_len, "partition day")?)?;
    let mut schema_fingerprint = [0_u8; 32];
    schema_fingerprint.copy_from_slice(reader.take(32)?);
    Ok(DecodedSliceMetadata {
        seal_key: SealKey::new(tenant, table, day),
        schema_fingerprint,
    })
}

/// Decode the audit envelope and Arrow IPC byte fields after metadata.
fn decode_slice_bodies(
    reader: &mut SlicePayloadReader<'_>,
) -> Result<(Vec<u8>, Vec<u8>), ScribeError> {
    let audit = reader.read_bytes_u32("WAL audit length decode failed")?;
    let data = reader.read_bytes_u32("WAL Arrow length decode failed")?;
    Ok((audit, data))
}

/// Decode one self-describing v3 WAL slice payload.
///
/// The payload contains the authenticated tenant, canonical table FQN,
/// partition day, schema fingerprint, canonical audit bytes, and Arrow IPC
/// bytes. Structural decoding is kept separate from replay's deduplication
/// and audit/Arrow reconstruction so this function only validates and returns
/// the one-record representation.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the payload magic, version,
/// metadata, lengths, or trailing-byte boundary is malformed.
pub(crate) fn decode_slice_payload(payload: &[u8]) -> Result<DecodedSlicePayload, ScribeError> {
    let mut reader = SlicePayloadReader::new(payload);
    let metadata = decode_slice_metadata(&mut reader)?;
    let (audit, data) = decode_slice_bodies(&mut reader)?;
    reader.finish()?;
    Ok(DecodedSlicePayload {
        seal_key: metadata.seal_key,
        schema_fingerprint: metadata.schema_fingerprint,
        audit,
        data,
    })
}

impl WalRecord {
    /// Construct a one-slice v4 record for a test or low-level caller.
    ///
    /// Production writers use [`Self::slice`] with the authenticated tenant.
    #[must_use]
    pub fn new(lsn: WalLsn, _record_kind: u8, batch_id: [u8; 16], payload: Vec<u8>) -> Self {
        Self::slice(lsn, [0; 16], batch_id, 0, 1, payload)
    }

    /// Construct one ordered slice record.
    #[must_use]
    pub fn slice(
        lsn: WalLsn,
        tenant_id: [u8; 16],
        batch_id: [u8; 16],
        slice_index: u32,
        slice_count: u32,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            lsn,
            flags: RECORD_FLAG_SLICE,
            tenant_id,
            batch_id,
            slice_index,
            slice_count,
            payload,
        }
    }

    /// Construct the terminal commit record for one complete ordered slice set.
    #[must_use]
    pub fn commit(
        lsn: WalLsn,
        tenant_id: [u8; 16],
        batch_id: [u8; 16],
        slice_count: u32,
        digest: [u8; 32],
    ) -> Self {
        Self {
            lsn,
            flags: RECORD_FLAG_COMMIT,
            tenant_id,
            batch_id,
            slice_index: slice_count,
            slice_count,
            payload: digest.to_vec(),
        }
    }

    /// Returns whether this record is one payload-bearing slice.
    #[must_use]
    pub const fn is_slice(&self) -> bool {
        self.flags == RECORD_FLAG_SLICE
    }

    /// Returns whether this record is a terminal slice-set commit.
    #[must_use]
    pub const fn is_commit(&self) -> bool {
        self.flags == RECORD_FLAG_COMMIT
    }

    /// Encode the record to bytes (including frame header and CRC).
    ///
    /// # Panics
    ///
    /// Panics only if an internal caller violates the configured `u32` WAL
    /// payload bound before this record is encoded.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let payload_len = u32::try_from(self.payload.len())
            .expect("invariant: WAL payload is bounded by the configured Scribe request ceiling");
        let mut buf = Vec::with_capacity(RECORD_HEADER_SIZE + self.payload.len() + 4);
        buf.extend_from_slice(&RECORD_MAGIC);
        buf.extend_from_slice(&WAL_VERSION.to_le_bytes());
        buf.extend_from_slice(&RECORD_HEADER_SIZE_U16.to_le_bytes());
        buf.extend_from_slice(&self.flags.to_le_bytes());
        buf.extend_from_slice(&self.lsn.as_u64().to_le_bytes());
        buf.extend_from_slice(&self.tenant_id);
        buf.extend_from_slice(&self.batch_id);
        buf.extend_from_slice(&self.slice_index.to_le_bytes());
        buf.extend_from_slice(&self.slice_count.to_le_bytes());
        buf.extend_from_slice(&payload_len.to_le_bytes());
        buf.extend_from_slice(&0_u32.to_le_bytes());
        buf.extend_from_slice(&self.payload);

        // Compute CRC over the complete v4 header followed by payload.
        let crc = crc32c_hash(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());

        buf
    }

    /// Decode a record from the given reader.
    ///
    /// Returns `None` at clean EOF (no bytes available).
    /// Returns `Err` on short read, CRC mismatch, or invalid record.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for a torn final write, a CRC mismatch, an
    /// unsupported version, or any invalid v4 header or payload bound.
    ///
    /// # Panics
    ///
    /// Panics only if the fixed-size local header slicing invariant is broken.
    pub fn decode_from<R: Read>(reader: &mut R) -> Result<Option<Self>, ScribeError> {
        let Some(header) = Self::decode_header_from(reader)? else {
            return Ok(None);
        };
        Self::decode_payload_from(reader, &header).map(Some)
    }

    /// Reads and validates one fixed record header without allocating payload memory.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for a torn header, unsupported version, invalid
    /// flags, non-zero reserved field, or inconsistent slice bounds.
    ///
    /// # Panics
    ///
    /// Panics only if the fixed-size local header slicing invariant is broken.
    fn decode_header_from<R: Read>(
        reader: &mut R,
    ) -> Result<Option<DecodedWalRecordHeader>, ScribeError> {
        let mut header = [0_u8; RECORD_HEADER_SIZE];
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(1) => {
                reader
                    .read_exact(&mut header[1..])
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("WAL torn v4 record header: {error}"),
                    })?;
            }
            Ok(_) => {
                return Err(ScribeError::Internal {
                    detail: "WAL one-byte header probe exceeded its buffer".to_owned(),
                });
            }
            Err(error) => {
                return Err(ScribeError::Internal {
                    detail: format!("WAL record header read error: {error}"),
                });
            }
        }
        if header[..8] != RECORD_MAGIC {
            return Err(ScribeError::Internal {
                detail: "invalid WAL v4 record magic".to_owned(),
            });
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != WAL_VERSION {
            return Err(ScribeError::UnsupportedWalVersion { version });
        }
        let header_len = u16::from_le_bytes([header[10], header[11]]);
        if usize::from(header_len) != RECORD_HEADER_SIZE {
            return Err(ScribeError::Internal {
                detail: "invalid WAL v4 record header length".to_owned(),
            });
        }
        let flags = u32::from_le_bytes(header[12..16].try_into().expect("fixed record header"));
        if !matches!(flags, RECORD_FLAG_SLICE | RECORD_FLAG_COMMIT) {
            return Err(ScribeError::Internal {
                detail: "invalid WAL v4 record flags".to_owned(),
            });
        }
        let lsn = WalLsn::new(u64::from_le_bytes(
            header[16..24].try_into().expect("fixed record header"),
        ));
        let mut tenant_id = [0_u8; 16];
        tenant_id.copy_from_slice(&header[24..40]);
        let mut batch_id = [0u8; 16];
        batch_id.copy_from_slice(&header[40..56]);
        let slice_index =
            u32::from_le_bytes(header[56..60].try_into().expect("fixed record header"));
        let slice_count =
            u32::from_le_bytes(header[60..64].try_into().expect("fixed record header"));
        let payload_len =
            u32::from_le_bytes(header[64..68].try_into().expect("fixed record header"));
        let reserved = u32::from_le_bytes(header[68..72].try_into().expect("fixed record header"));
        if reserved != 0
            || slice_count == 0
            || slice_index > slice_count
            || (flags == RECORD_FLAG_SLICE && slice_index >= slice_count)
            || (flags == RECORD_FLAG_COMMIT && (slice_index != slice_count || payload_len != 32))
        {
            return Err(ScribeError::Internal {
                detail: "invalid WAL v4 record bounds".to_owned(),
            });
        }
        Ok(Some(DecodedWalRecordHeader {
            encoded: header,
            lsn,
            flags,
            tenant_id,
            batch_id,
            slice_index,
            slice_count,
            payload_len,
        }))
    }

    /// Reads and authenticates the payload declared by a validated header.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the payload or CRC is torn, or when the
    /// complete frame CRC does not match.
    ///
    /// # Panics
    ///
    /// Panics only if the public `u32` payload bound does not fit `usize` on a
    /// supported target.
    fn decode_payload_from<R: Read>(
        reader: &mut R,
        header: &DecodedWalRecordHeader,
    ) -> Result<Self, ScribeError> {
        let payload_len =
            usize::try_from(header.payload_len).expect("u32 fits usize on supported targets");
        #[cfg(test)]
        WAL_REPLAY_PAYLOAD_ALLOCATIONS.fetch_add(1, Ordering::AcqRel);
        let mut payload = vec![0u8; payload_len];
        reader
            .read_exact(&mut payload)
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL torn v4 record payload: {e}"),
            })?;

        // Read CRC
        let mut crc_buf = [0u8; 4];
        reader
            .read_exact(&mut crc_buf)
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL torn v4 record CRC: {e}"),
            })?;
        let expected_crc = u32::from_le_bytes(crc_buf);

        let computed_crc = crc32c::crc32c_append(crc32c_hash(&header.encoded), &payload);
        if computed_crc != expected_crc {
            return Err(ScribeError::Internal {
                detail: format!(
                    "WAL record CRC mismatch: expected {expected_crc:#x}, got {computed_crc:#x}"
                ),
            });
        }

        Ok(Self {
            lsn: header.lsn,
            flags: header.flags,
            tenant_id: header.tenant_id,
            batch_id: header.batch_id,
            slice_index: header.slice_index,
            slice_count: header.slice_count,
            payload,
        })
    }
}

/// Encodes the fixed WAL record header into caller-owned stack storage.
fn encode_record_header(
    lsn: WalLsn,
    flags: u32,
    tenant_id: [u8; 16],
    batch_id: [u8; 16],
    slice_index: u32,
    slice_count: u32,
    payload_len: u32,
) -> [u8; RECORD_HEADER_SIZE] {
    let mut header = [0_u8; RECORD_HEADER_SIZE];
    header[0..8].copy_from_slice(&RECORD_MAGIC);
    header[8..10].copy_from_slice(&WAL_VERSION.to_le_bytes());
    header[10..12].copy_from_slice(&RECORD_HEADER_SIZE_U16.to_le_bytes());
    header[12..16].copy_from_slice(&flags.to_le_bytes());
    header[16..24].copy_from_slice(&lsn.as_u64().to_le_bytes());
    header[24..40].copy_from_slice(&tenant_id);
    header[40..56].copy_from_slice(&batch_id);
    header[56..60].copy_from_slice(&slice_index.to_le_bytes());
    header[60..64].copy_from_slice(&slice_count.to_le_bytes());
    header[64..68].copy_from_slice(&payload_len.to_le_bytes());
    header
}

/// Encode a compact test fixture that is decoded into a normal v3 slice append.
#[cfg(test)]
pub(crate) fn encode_append_frame(
    batch_id: [u8; 16],
    audit: &[u8],
    data: &[u8],
) -> Result<Bytes, ScribeError> {
    let audit_len = u32::try_from(audit.len()).map_err(|_| ScribeError::Internal {
        detail: "audit envelope exceeds WAL frame length".to_string(),
    })?;
    let data_len = u32::try_from(data.len()).map_err(|_| ScribeError::Internal {
        detail: "Arrow payload exceeds WAL frame length".to_string(),
    })?;
    let mut frame = Vec::with_capacity(28 + audit.len() + data.len());
    frame.extend_from_slice(&APPEND_FRAME_MAGIC_V3);
    frame.extend_from_slice(&batch_id);
    frame.extend_from_slice(&audit_len.to_le_bytes());
    frame.extend_from_slice(&data_len.to_le_bytes());
    frame.extend_from_slice(audit);
    frame.extend_from_slice(data);
    Ok(Bytes::from(frame))
}

#[cfg(test)]
struct DecodedAppendFrame<'a> {
    batch_id: [u8; 16],
    audit: &'a [u8],
    data: &'a [u8],
}

#[cfg(test)]
fn decode_append_frame(frame: &[u8]) -> Result<DecodedAppendFrame<'_>, ScribeError> {
    if frame.len() < 28 || frame[0..4] != APPEND_FRAME_MAGIC_V3 {
        return Err(ScribeError::Internal {
            detail: "invalid Scribe WAL append frame".to_string(),
        });
    }
    let mut batch_id = [0_u8; 16];
    batch_id.copy_from_slice(&frame[4..20]);
    let lengths_start = 20;
    let audit_len = u32::from_le_bytes([
        frame[lengths_start],
        frame[lengths_start + 1],
        frame[lengths_start + 2],
        frame[lengths_start + 3],
    ]) as usize;
    let data_len = u32::from_le_bytes([
        frame[lengths_start + 4],
        frame[lengths_start + 5],
        frame[lengths_start + 6],
        frame[lengths_start + 7],
    ]) as usize;
    let payload_len = audit_len
        .checked_add(data_len)
        .ok_or_else(|| ScribeError::Internal {
            detail: "Scribe WAL append frame length overflow".to_string(),
        })?;
    let payload_start = lengths_start + 8;
    let end = payload_start
        .checked_add(payload_len)
        .ok_or_else(|| ScribeError::Internal {
            detail: "Scribe WAL append frame offset overflow".to_string(),
        })?;
    if end != frame.len() {
        return Err(ScribeError::Internal {
            detail: "Scribe WAL append frame length mismatch".to_string(),
        });
    }
    Ok(DecodedAppendFrame {
        batch_id,
        audit: &frame[payload_start..payload_start + audit_len],
        data: &frame[payload_start + audit_len..end],
    })
}

/// WAL segment — one file in a fixed pod-local shard stream.
#[derive(Debug)]
pub struct WalSegment {
    path: PathBuf,
    file: Mutex<File>,
    header: SegmentHeader,
    /// Provisional physical-volume growth committed only after segment fsync.
    volume_growth: Mutex<Vec<crate::resources::WalVolumeGrowth>>,
}

/// Stable filesystem identity for one closed WAL segment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WalSegmentRef {
    /// Segment path retained until manifest publication and grace expiry.
    pub path: PathBuf,
}

/// WAL disk pressure thresholds from the Scribe contract.
pub const WAL_SOFT_PRESSURE_PERCENT: u64 = 80;
/// WAL hard admission threshold from the Scribe contract.
pub const WAL_HARD_PRESSURE_PERCENT: u64 = 90;
const WAL_SOFT_FREE_BYTES: u64 = 4 * 64 * 1024 * 1024;
const WAL_HARD_FREE_BYTES: u64 = 2 * 64 * 1024 * 1024;

/// Deterministic WAL capacity calculation used by admission and inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalDiskPressure {
    /// Effective capacity after applying the configured and filesystem caps.
    pub capacity_bytes: u64,
    /// Current WAL bytes before the proposed append.
    pub wal_bytes: u64,
    /// WAL bytes after the proposed append and any new segment header.
    pub projected_bytes: u64,
    /// Filesystem free bytes at the last one-second sample.
    pub filesystem_available_bytes: u64,
    /// Whether the append crosses the 80% soft pressure boundary.
    pub soft: bool,
    /// Whether the append must be rejected before WAL mutation.
    pub hard: bool,
}

fn evaluate_disk_pressure(
    capacity_bytes: u64,
    wal_bytes: u64,
    projected_bytes: u64,
    filesystem_available_bytes: u64,
) -> WalDiskPressure {
    let soft_limit = capacity_bytes.saturating_mul(WAL_SOFT_PRESSURE_PERCENT) / 100;
    let hard_limit = capacity_bytes.saturating_mul(WAL_HARD_PRESSURE_PERCENT) / 100;
    WalDiskPressure {
        capacity_bytes,
        wal_bytes,
        projected_bytes,
        filesystem_available_bytes,
        soft: projected_bytes >= soft_limit || filesystem_available_bytes < WAL_SOFT_FREE_BYTES,
        hard: projected_bytes >= hard_limit || filesystem_available_bytes < WAL_HARD_FREE_BYTES,
    }
}

#[derive(Debug, Clone, Copy)]
struct DiskSample {
    sampled_at: Instant,
    filesystem_capacity_bytes: u64,
    filesystem_available_bytes: u64,
}

#[derive(Debug)]
struct WalDiskState {
    base_dir: PathBuf,
    configured_limit_bytes: Option<u64>,
    sample: Mutex<Option<DiskSample>>,
    hard_failed: AtomicBool,
    accounted_bytes: AtomicU64,
    #[cfg(test)]
    forced_sample: Mutex<Option<ForcedSample>>,
    #[cfg(any(test, feature = "test-support"))]
    sync_failure: AtomicBool,
    #[cfg(any(test, feature = "test-support"))]
    post_sync_failure: AtomicBool,
}

#[cfg(test)]
#[derive(Debug)]
enum ForcedSample {
    Failure,
    Value((u64, u64)),
}

impl WalDiskState {
    fn new(base_dir: PathBuf, configured_limit_bytes: Option<u64>) -> Self {
        Self {
            base_dir,
            configured_limit_bytes,
            sample: Mutex::new(None),
            hard_failed: AtomicBool::new(false),
            accounted_bytes: AtomicU64::new(0),
            #[cfg(test)]
            forced_sample: Mutex::new(None),
            #[cfg(any(test, feature = "test-support"))]
            sync_failure: AtomicBool::new(false),
            #[cfg(any(test, feature = "test-support"))]
            post_sync_failure: AtomicBool::new(false),
        }
    }

    fn sample(&self) -> DiskSample {
        if let Ok(sample) = self.sample.lock()
            && let Some(sample) = *sample
            && sample.sampled_at.elapsed() < Duration::from_secs(1)
        {
            return sample;
        }
        #[cfg(test)]
        let forced = self
            .forced_sample
            .lock()
            .ok()
            .and_then(|mut value| value.take());
        #[cfg(test)]
        let sampled = match forced {
            Some(ForcedSample::Value(value)) => Some(value),
            Some(ForcedSample::Failure) => Some((0, 0)),
            None => filesystem_space(&self.base_dir),
        };
        #[cfg(not(test))]
        let sampled = None;
        let (filesystem_capacity_bytes, filesystem_available_bytes) = sampled
            .or_else(|| filesystem_space(&self.base_dir))
            .unwrap_or_else(|| {
            tracing::warn!(path = %self.base_dir.display(), "WAL filesystem capacity probe failed");
            (u64::MAX, 0)
        });
        let sample = DiskSample {
            sampled_at: Instant::now(),
            filesystem_capacity_bytes,
            filesystem_available_bytes,
        };
        if let Ok(mut current) = self.sample.lock() {
            *current = Some(sample);
        }
        sample
    }

    #[cfg(test)]
    fn force_sample(&self, sample: Option<(u64, u64)>) {
        if let Ok(mut forced) = self.forced_sample.lock() {
            *forced = Some(match sample {
                Some(value) => ForcedSample::Value(value),
                None => ForcedSample::Failure,
            });
        }
        if let Ok(mut cached) = self.sample.lock() {
            *cached = None;
        }
    }

    fn pressure(&self, wal_bytes: u64, append_bytes: u64) -> WalDiskPressure {
        let sample = self.sample();
        let capacity_bytes = self
            .configured_limit_bytes
            .unwrap_or(u64::MAX)
            .min(sample.filesystem_capacity_bytes);
        evaluate_disk_pressure(
            capacity_bytes,
            wal_bytes,
            wal_bytes.saturating_add(append_bytes),
            sample.filesystem_available_bytes,
        )
    }

    fn bytes(&self) -> u64 {
        self.accounted_bytes.load(Ordering::Acquire)
    }

    fn add_bytes(&self, bytes: u64) {
        self.accounted_bytes.fetch_add(bytes, Ordering::AcqRel);
    }

    fn subtract_bytes(&self, bytes: u64) {
        let mut current = self.bytes();
        loop {
            let next = current.saturating_sub(bytes);
            match self.accounted_bytes.compare_exchange(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
    }

    fn reconcile(&self) {
        let measured = directory_bytes(&self.base_dir);
        let current = self.bytes();
        if measured > current {
            self.accounted_bytes.store(measured, Ordering::Release);
        }
    }

    fn reject_if_hard(&self, wal_bytes: u64, append_bytes: u64) -> Result<(), ScribeError> {
        if self.hard_failed.load(Ordering::Acquire) {
            return Err(ScribeError::WalDiskFull);
        }
        if self.pressure(wal_bytes, append_bytes).hard {
            return Err(ScribeError::WalDiskFull);
        }
        Ok(())
    }

    fn mark_hard_failed(&self) {
        self.hard_failed.store(true, Ordering::Release);
    }

    #[cfg(any(test, feature = "test-support"))]
    fn trip_sync_failure(&self) {
        self.sync_failure.store(true, Ordering::Release);
    }

    #[cfg(any(test, feature = "test-support"))]
    fn trip_post_sync_failure(&self) {
        self.post_sync_failure.store(true, Ordering::Release);
    }

    #[cfg(any(test, feature = "test-support"))]
    fn take_sync_failure(&self) -> bool {
        self.sync_failure.swap(false, Ordering::AcqRel)
    }

    #[cfg(any(test, feature = "test-support"))]
    fn take_post_sync_failure(&self) -> bool {
        self.post_sync_failure.swap(false, Ordering::AcqRel)
    }
}

fn filesystem_space(path: &Path) -> Option<(u64, u64)> {
    let stats = statvfs(path).ok()?;
    let block_size = if stats.f_frsize == 0 {
        stats.f_bsize
    } else {
        stats.f_frsize
    };
    Some((
        stats.f_blocks.saturating_mul(block_size),
        stats.f_bavail.saturating_mul(block_size),
    ))
}

fn directory_bytes(path: &Path) -> u64 {
    #[cfg(test)]
    if WAL_COUNT_ACTIVE.load(Ordering::Relaxed) {
        WAL_WALK_COUNT.with(|count| count.set(count.get() + 1));
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                return 0;
            };
            if metadata.is_dir() {
                directory_bytes(&path)
            } else if metadata.is_file() {
                metadata.len()
            } else {
                0
            }
        })
        .sum()
}

fn wal_io_error(context: &str, error: &io::Error) -> ScribeError {
    if error.kind() == io::ErrorKind::StorageFull || error.raw_os_error() == Some(28) {
        ScribeError::WalDiskFull
    } else {
        ScribeError::Internal {
            detail: format!("{context}: {error}"),
        }
    }
}

/// Maps root physical-volume refusal into the existing Scribe WAL taxonomy.
fn resource_volume_error(error: crate::resources::BifrostResourceError) -> ScribeError {
    match error {
        crate::resources::BifrostResourceError::Occupied { .. } => ScribeError::WalDiskFull,
        error => ScribeError::Internal {
            detail: format!("WAL physical-volume accounting failed: {error}"),
        },
    }
}

impl WalSegment {
    /// Create a new WAL segment file at the given path.
    pub fn create(path: impl AsRef<Path>, header: SegmentHeader) -> Result<Self, ScribeError> {
        let path = path.as_ref();
        let tmp_path = path.with_extension("tmp");

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)
            .map_err(|error| wal_io_error("failed to create WAL segment temp file", &error))?;

        header
            .write_to(&mut file)
            .map_err(|error| wal_io_error("failed to write WAL segment header", &error))?;

        file.sync_all()
            .map_err(|error| wal_io_error("failed to fsync WAL segment temp file", &error))?;

        drop(file);

        std::fs::rename(&tmp_path, path)
            .map_err(|error| wal_io_error("failed to rename WAL segment", &error))?;

        if let Some(parent) = path.parent() {
            let parent_file = File::open(parent)
                .map_err(|error| wal_io_error("failed to open WAL segment parent dir", &error))?;
            parent_file
                .sync_all()
                .map_err(|error| wal_io_error("failed to fsync WAL segment parent dir", &error))?;
        }

        #[cfg(test)]
        if WAL_FAIL_SEGMENT_FINAL_OPEN.with(|flag| flag.replace(false)) {
            return Err(ScribeError::Internal {
                detail: "injected post-rename WAL segment open failure".to_owned(),
            });
        }
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(path)
            .map_err(|error| wal_io_error("failed to open WAL segment for append", &error))?;

        Ok(Self {
            path: path.to_path_buf(),
            file: Mutex::new(file),
            header,
            volume_growth: Mutex::new(Vec::new()),
        })
    }

    /// Open an existing WAL segment for reading.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ScribeError> {
        let path = path.as_ref();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| ScribeError::Internal {
                detail: format!("failed to open WAL segment: {e}"),
            })?;

        let mut header_buf = [0u8; SEGMENT_HEADER_SIZE];
        file.read_exact(&mut header_buf)
            .map_err(|e| ScribeError::Internal {
                detail: format!("failed to read WAL segment header: {e}"),
            })?;

        let header = SegmentHeader::decode(&header_buf)?;

        Ok(Self {
            path: path.to_path_buf(),
            file: Mutex::new(file),
            header,
            volume_growth: Mutex::new(Vec::new()),
        })
    }

    /// Append a record to the segment without forcing it to stable storage.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::WalDiskFull`] when the filesystem refuses the
    /// write for capacity, or [`ScribeError::Internal`] when the segment lock
    /// is poisoned or another write failure occurs.
    pub fn append(&self, record: &WalRecord) -> Result<(), ScribeError> {
        let span =
            tracing::info_span!("bifrost.scribe.wal.append", outcome = tracing::field::Empty);
        let _entered = span.enter();
        let encoded = record.encode();
        let mut file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (append)".to_string(),
        })?;

        let started = Instant::now();
        let result = file.write_all(&encoded).map_err(|e| {
            if e.kind() == io::ErrorKind::StorageFull || e.raw_os_error() == Some(28) {
                ScribeError::WalDiskFull
            } else {
                ScribeError::Internal {
                    detail: format!("WAL record write failed: {e}"),
                }
            }
        });
        record_wal_append(&result, encoded.len(), started);
        span.record("outcome", if result.is_ok() { "success" } else { "failed" });
        result
    }

    /// Streams one fixed header and borrowed payload parts without a frame copy.
    ///
    /// # Errors
    ///
    /// Returns a WAL IO error when any part cannot be written. The caller owns
    /// rollback to the measured pre-append file length.
    fn append_borrowed(
        &self,
        header: &[u8; RECORD_HEADER_SIZE],
        payload_parts: &[&[u8]],
        crc: [u8; 4],
        encoded_len: usize,
    ) -> Result<(), ScribeError> {
        let span =
            tracing::info_span!("bifrost.scribe.wal.append", outcome = tracing::field::Empty);
        let _entered = span.enter();
        let mut file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (borrowed append)".to_owned(),
        })?;
        #[cfg(test)]
        if WAL_PARTIAL_WRITE.with(|flag| flag.replace(false)) {
            let started = Instant::now();
            let prefix = header.len().max(1) / 2;
            let result = file
                .write_all(&header[..prefix])
                .map_err(|error| wal_io_error("WAL partial record write failed", &error))
                .and_then(|()| {
                    Err(ScribeError::Internal {
                        detail: "injected partial WAL write".to_owned(),
                    })
                });
            record_wal_append(&result, encoded_len, started);
            span.record("outcome", "failed");
            return result;
        }
        let started = Instant::now();
        let result = (|| -> Result<(), ScribeError> {
            file.write_all(header)
                .map_err(|error| wal_io_error("WAL record header write failed", &error))?;
            for part in payload_parts {
                file.write_all(part)
                    .map_err(|error| wal_io_error("WAL record payload write failed", &error))?;
            }
            file.write_all(&crc)
                .map_err(|error| wal_io_error("WAL record CRC write failed", &error))
        })();
        record_wal_append(&result, encoded_len, started);
        span.record("outcome", if result.is_ok() { "success" } else { "failed" });
        result
    }

    /// Restores the exact pre-append file length after a failed record write.
    ///
    /// # Errors
    ///
    /// Returns an internal WAL error when truncation or the repair fsync fails.
    fn rollback_failed_append(&self, prior_len: u64) -> Result<(), ScribeError> {
        let file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned during append rollback".to_owned(),
        })?;
        file.set_len(prior_len)
            .map_err(|error| wal_io_error("failed to truncate rejected WAL append", &error))?;
        file.sync_data()
            .map_err(|error| wal_io_error("failed to fsync rejected WAL rollback", &error))
    }

    /// Force all appended data for this segment to stable storage.
    pub fn sync_data(&self) -> Result<(), ScribeError> {
        let span = tracing::info_span!("bifrost.scribe.wal.fsync", outcome = tracing::field::Empty);
        let _entered = span.enter();
        let file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (sync_data)".to_string(),
        })?;
        let started = Instant::now();
        let result = file.sync_data().map_err(|e| {
            if e.kind() == io::ErrorKind::StorageFull || e.raw_os_error() == Some(28) {
                ScribeError::WalDiskFull
            } else {
                ScribeError::Internal {
                    detail: format!("WAL data sync failed: {e}"),
                }
            }
        });
        record_wal_fsync(&result, started);
        span.record("outcome", if result.is_ok() { "success" } else { "failed" });
        result?;
        let growth = {
            let mut pending = self
                .volume_growth
                .lock()
                .map_err(|_| ScribeError::Internal {
                    detail: "WAL volume-growth lock poisoned during fsync".to_owned(),
                })?;
            std::mem::take(&mut *pending)
        };
        for reservation in growth {
            reservation.commit().map_err(resource_volume_error)?;
        }
        Ok(())
    }

    /// Retains provisional volume growth until this segment is durably synced.
    fn retain_volume_growth(
        &self,
        growth: crate::resources::WalVolumeGrowth,
    ) -> Result<(), ScribeError> {
        let Ok(mut pending) = self.volume_growth.lock() else {
            growth.retain_and_poison();
            return Err(ScribeError::Internal {
                detail: "WAL volume-growth lock poisoned during append".to_owned(),
            });
        };
        #[cfg(test)]
        if WAL_FAIL_VOLUME_RETAIN.with(|flag| flag.replace(false)) {
            growth.retain_and_poison();
            return Err(ScribeError::Internal {
                detail: "injected WAL volume-growth retention failure".to_owned(),
            });
        }
        pending.push(growth);
        Ok(())
    }

    /// Append a record and force it to stable storage.
    pub fn append_and_fsync(&self, record: &WalRecord) -> Result<(), ScribeError> {
        self.append(record)?;
        self.sync_data()?;
        Ok(())
    }

    /// Read all records from the segment.
    ///
    /// # Errors
    ///
    /// Returns the framing, checksum, ordering, lock, seek, read, or torn-tail
    /// repair error reported while visiting the segment.
    pub fn read_records(&self) -> Result<Vec<WalRecord>, ScribeError> {
        let mut records: Vec<WalRecord> = Vec::new();
        self.for_each_record(|record| {
            records.push(record);
            Ok(())
        })?;
        Ok(records)
    }

    /// Visit complete records incrementally, truncating a torn tail before
    /// returning. Replay uses this to avoid constructing a directory-wide
    /// `Vec<(segment, record)>` before grouping state. Structural, CRC, and
    /// non-monotonic committed-prefix failures are returned without mutation.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the segment cannot be read or synced, the
    /// visitor fails, or a committed prefix violates the v4 frame or LSN
    /// invariants. A short final record is the sole truncation case.
    pub(crate) fn for_each_record<F>(&self, mut visit: F) -> Result<(), ScribeError>
    where
        F: FnMut(WalRecord) -> Result<(), ScribeError>,
    {
        let mut file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (read_records)".to_string(),
        })?;
        file.seek(SeekFrom::Start(SEGMENT_HEADER_SIZE as u64))
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL segment seek failed: {e}"),
            })?;

        let mut previous_lsn = None;
        loop {
            let record_offset = file
                .stream_position()
                .map_err(|error| ScribeError::Internal {
                    detail: format!("WAL record position failed: {error}"),
                })?;
            match WalRecord::decode_from(&mut *file) {
                Ok(Some(record)) => {
                    if previous_lsn.is_some_and(|previous| record.lsn <= previous) {
                        return Err(ScribeError::Internal {
                            detail: "WAL v4 contains non-monotonic committed LSNs".to_owned(),
                        });
                    }
                    previous_lsn = Some(record.lsn);
                    visit(record)?;
                }
                Ok(None) => break,
                Err(error) if is_torn_tail_error(&error) => {
                    file.set_len(record_offset)
                        .map_err(|error| ScribeError::Internal {
                            detail: format!("WAL torn-tail truncation failed: {error}"),
                        })?;
                    let started = Instant::now();
                    #[cfg(test)]
                    WAL_RECOVERY_SYNC_COUNT.with(|count| count.set(count.get() + 1));
                    let sync_result = file.sync_data().map_err(|error| ScribeError::Internal {
                        detail: format!("WAL torn-tail sync failed: {error}"),
                    });
                    record_wal_fsync(&sync_result, started);
                    sync_result?;
                    break;
                }
                Err(error) => return Err(error),
            }
        }

        Ok(())
    }

    /// Get the segment header.
    #[must_use]
    pub fn header(&self) -> &SegmentHeader {
        &self.header
    }

    /// Get the segment path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the immutable identity used by persistence and retirement.
    #[must_use]
    pub fn reference(&self) -> WalSegmentRef {
        WalSegmentRef {
            path: self.path.clone(),
        }
    }
}

/// Identifies the only decode failures eligible for tail truncation.
///
/// A partially written final header, payload, or CRC is a recoverable torn
/// tail. Structural violations and CRC mismatches in a complete frame are
/// durable corruption and must fail-stop rather than silently discard data.
fn is_torn_tail_error(error: &ScribeError) -> bool {
    matches!(error, ScribeError::Internal { detail } if detail.starts_with("WAL torn v4"))
}

/// WAL writer — owns one fixed set of shard streams with automatic rollover.
#[derive(Debug)]
pub struct WalWriter {
    base_dir: PathBuf,
    node_id: [u8; 16],
    writer_epoch: i64,
    shard_id: u8,
    next_lsn: Arc<AtomicU64>,
    segment_bytes: u64,
    states: Arc<Vec<Mutex<WalState>>>,
    disk: Arc<WalDiskState>,
    retirement_refs: Arc<Mutex<HashMap<PathBuf, usize>>>,
    /// Physical WAL growth capability shared only by this writer's handles.
    volume: Option<Arc<crate::resources::WalVolume>>,
}

#[derive(Debug, Default)]
struct WalState {
    current_segment: Option<Arc<WalSegment>>,
    seg_seq: u64,
    current_segment_size: u64,
    current_segment_records: u64,
}

/// A WAL handle bound to one fixed shard owner.
#[derive(Debug, Clone)]
pub struct WalHandle {
    writer: Arc<WalWriter>,
    shard_id: u8,
}

impl WalHandle {
    fn for_shard(writer: Arc<WalWriter>, shard_id: u8) -> Self {
        Self { writer, shard_id }
    }

    /// Appends one prepared record through this handle's fixed shard identity.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the record lacks required
    /// self-describing identity, or propagates any writer append/capacity/IO
    /// failure.
    pub(crate) fn append_prepared(
        &self,
        mut append: PreparedWalAppend,
    ) -> Result<WalAppendResult, ScribeError> {
        if append.seal_key.is_none() && append.commit_digest.is_none() {
            return Err(ScribeError::Internal {
                detail: "shard WAL append is missing its self-describing seal key".to_owned(),
            });
        }
        append.shard_id = Some(self.shard_id);
        self.writer.append_prepared(append)
    }

    /// Returns this shard generation's current encoded WAL occupancy.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the fixed shard state is poisoned.
    pub(crate) fn current_segment_bytes(&self) -> Result<u64, ScribeError> {
        let state = self.writer.states[usize::from(self.shard_id)]
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "WAL state lock poisoned (current segment bytes)".to_owned(),
            })?;
        Ok(state.current_segment_size)
    }

    /// Reports whether the current shard generation contains accepted WAL records.
    ///
    /// This is intentionally separate from byte occupancy: a segment header is
    /// created before the first record, while rotation age applies only after a
    /// non-empty generation exists.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the fixed shard state is poisoned.
    pub(crate) fn has_active_records(&self) -> Result<bool, ScribeError> {
        let state = self.writer.states[usize::from(self.shard_id)]
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "WAL state lock poisoned (active record check)".to_owned(),
            })?;
        Ok(state.current_segment_records > 0)
    }

    /// Fsyncs and closes this shard's non-empty active WAL generation.
    ///
    /// The returned segment is detached from the append stream but remains on
    /// disk until the shard rotation cohort releases its sole retirement ref.
    /// Empty streams return `None` and create no retirement owner.
    ///
    /// # Errors
    ///
    /// Returns the fixed-shard state-lock or segment-fsync error. The stream is
    /// left open when fsync fails.
    pub(crate) fn close_active_generation(&self) -> Result<Option<WalSegmentRef>, ScribeError> {
        let mut state = self.writer.states[usize::from(self.shard_id)]
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "WAL state lock poisoned (close active generation)".to_owned(),
            })?;
        let Some(segment) = state.current_segment.as_ref() else {
            return Ok(None);
        };
        if state.current_segment_records == 0 {
            return Ok(None);
        }
        segment.sync_data()?;
        let reference = segment.reference();
        state.current_segment = None;
        state.current_segment_size = 0;
        state.current_segment_records = 0;
        Ok(Some(reference))
    }

    pub(crate) fn sync_segments(&self, segments: &[Arc<WalSegment>]) -> Result<(), ScribeError> {
        match self.writer.sync_segments_with_fault(segments) {
            Ok(()) => Ok(()),
            Err(error) => {
                if matches!(error, ScribeError::WalDiskFull) {
                    self.writer.disk.mark_hard_failed();
                }
                Err(error)
            }
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn take_post_sync_failure_for_test(&self) -> bool {
        self.writer.take_post_sync_failure_for_test()
    }

    /// Releases this handle's retained references to closed WAL segments.
    ///
    /// # Errors
    ///
    /// Returns the retirement-reference lock or segment-deletion error from
    /// the shared writer.
    pub(crate) fn retire_segments(&self, segments: &[WalSegmentRef]) -> Result<(), ScribeError> {
        self.writer.retire_segments(segments)
    }

    /// Retains closed WAL segments for one pending immutable generation.
    ///
    /// # Errors
    ///
    /// Returns the retirement-reference lock error, or the injected retention
    /// failure used by focused tests.
    pub(crate) fn retain_segments(&self, segments: &[WalSegmentRef]) -> Result<(), ScribeError> {
        self.writer.retain_segments(segments)
    }

    /// Releases a reader pin, deleting the closed file only at a proven safe boundary.
    ///
    /// # Errors
    ///
    /// Returns the retirement-reference lock or durable deletion error from
    /// the shared writer.
    fn release_replay_pin(
        &self,
        segment: &WalSegmentRef,
        delete_if_unreferenced: bool,
    ) -> Result<(), ScribeError> {
        self.writer
            .release_replay_pin(segment, delete_if_unreferenced)
    }

    /// Closes this shard's current segment when no active bucket owns it.
    ///
    /// # Errors
    ///
    /// Returns the writer state-lock or segment synchronization error.
    pub(crate) fn close_segments_if_unowned(
        &self,
        segments: &[WalSegmentRef],
        active_paths: &HashSet<PathBuf>,
    ) -> Result<(), ScribeError> {
        self.writer
            .close_segments_if_unowned(segments, active_paths)
    }
}

impl WalWriter {
    /// Restores segment and volume state after one prepared-record write fails.
    ///
    /// # Errors
    ///
    /// Returns the rollback, failed-segment removal, or volume-retirement error
    /// when physical state cannot be restored to its pre-append boundary.
    fn rollback_prepared_append(
        &self,
        state: &mut WalState,
        segment: &Arc<WalSegment>,
        prior_file_len: u64,
        new_segment_bytes: u64,
    ) -> Result<(), ScribeError> {
        if let Err(error) = segment.rollback_failed_append(prior_file_len) {
            if let Some(volume) = &self.volume {
                volume.poison_divergence();
            }
            return Err(error);
        }
        if new_segment_bytes == 0 {
            return Ok(());
        }
        state.current_segment = None;
        state.current_segment_size = 0;
        state.current_segment_records = 0;
        if let Err(error) = remove_failed_segment(segment.path()) {
            if let Some(volume) = &self.volume {
                volume.poison_divergence();
            }
            return Err(error);
        }
        if let Some(volume) = &self.volume {
            volume
                .retire(new_segment_bytes)
                .map_err(resource_volume_error)?;
        }
        Ok(())
    }

    /// Create a new pod-local WAL writer.
    ///
    /// Segment sizing is explicit and validated before the writer is returned.
    pub fn new(
        base_dir: impl AsRef<Path>,
        node_id: [u8; 16],
        writer_epoch: i64,
        config: WalConfig,
    ) -> Result<Self, ScribeError> {
        Self::new_with_shard_id_and_volume(base_dir, node_id, writer_epoch, 0, config, None)
    }

    /// Creates a WAL writer governed by the registered physical WAL device.
    ///
    /// # Errors
    ///
    /// Returns the same configuration, recovery, and filesystem errors as
    /// [`Self::new`]. Every later append reserves physical growth before I/O.
    pub fn new_with_volume(
        base_dir: impl AsRef<Path>,
        node_id: [u8; 16],
        writer_epoch: i64,
        config: WalConfig,
        volume: crate::resources::WalVolume,
    ) -> Result<Self, ScribeError> {
        Self::new_with_shard_id_and_volume(
            base_dir,
            node_id,
            writer_epoch,
            0,
            config,
            Some(Arc::new(volume)),
        )
    }

    /// Create one fixed-shard WAL stream.
    pub fn new_with_shard_id(
        base_dir: impl AsRef<Path>,
        node_id: [u8; 16],
        writer_epoch: i64,
        shard_id: u8,
        config: WalConfig,
    ) -> Result<Self, ScribeError> {
        Self::new_with_shard_id_and_volume(base_dir, node_id, writer_epoch, shard_id, config, None)
    }

    /// Constructs one shard stream with an optional live physical-volume owner.
    fn new_with_shard_id_and_volume(
        base_dir: impl AsRef<Path>,
        node_id: [u8; 16],
        writer_epoch: i64,
        shard_id: u8,
        config: WalConfig,
        volume: Option<Arc<crate::resources::WalVolume>>,
    ) -> Result<Self, ScribeError> {
        let config =
            WalConfig::new(config.segment_bytes)?.with_disk_limit(config.disk_limit_bytes)?;
        if shard_id >= 16 {
            return Err(ScribeError::Internal {
                detail: format!("WAL shard id must be below 16, got {shard_id}"),
            });
        }
        let base_dir = base_dir.as_ref().to_path_buf();
        let disk_dir = base_dir.clone();
        let writer = Self {
            base_dir,
            node_id,
            writer_epoch,
            shard_id,
            next_lsn: Arc::new(AtomicU64::new(0)),
            segment_bytes: config.segment_bytes,
            states: Arc::new((0..16).map(|_| Mutex::new(WalState::default())).collect()),
            disk: Arc::new(WalDiskState::new(disk_dir, config.disk_limit_bytes)),
            retirement_refs: Arc::new(Mutex::new(HashMap::new())),
            volume,
        };
        writer.initialize_existing_stream()?;
        Ok(writer)
    }

    /// Recover the next segment and LSN counters for the current stream.
    ///
    /// A restarted writer must never recreate an existing decimal segment
    /// filename or reuse an LSN in the same `(node_id, writer_epoch)` stream.
    /// Older epochs remain replayable but are deliberately excluded from the
    /// live writer counters.
    fn initialize_existing_stream(&self) -> Result<(), ScribeError> {
        let mut paths = Vec::new();
        if self.base_dir.exists() {
            collect_segment_paths(&self.base_dir, &mut paths)?;
        }
        let mut next_lsn = 0_u64;
        for path in paths {
            if let Ok(metadata) = std::fs::metadata(&path) {
                self.disk.add_bytes(metadata.len());
            }
            let segment = WalSegment::open(&path)?;
            let header = segment.header();
            if header.node_id != self.node_id || header.writer_epoch != self.writer_epoch {
                continue;
            }
            let shard = usize::from(header.shard_id);
            let mut state = self.states[shard]
                .lock()
                .map_err(|_| ScribeError::Internal {
                    detail: "WAL state lock poisoned during stream recovery".to_owned(),
                })?;
            state.seg_seq = state.seg_seq.max(header.seg_seq.saturating_add(1));
            drop(state);
            for record in segment.read_records()? {
                next_lsn = next_lsn.max(record.lsn.as_u64().saturating_add(1));
            }
        }
        self.next_lsn.store(next_lsn, Ordering::Release);
        Ok(())
    }

    /// Return the pod-local WAL root used for replay and diagnostics.
    #[must_use]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Reserves exact durable growth for a Scribe staged artifact lifecycle.
    ///
    /// # Errors
    ///
    /// Returns a typed internal refusal when the registered WAL volume cannot
    /// admit the exact staged bytes.
    pub(crate) fn reserve_staged_growth(
        &self,
        bytes: u64,
    ) -> Result<Option<crate::resources::WalVolumeGrowth>, ScribeError> {
        self.volume
            .as_ref()
            .map(|volume| {
                volume
                    .try_reserve_growth(bytes)
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("reserve Scribe staged WAL-volume growth: {error}"),
                    })
            })
            .transpose()
    }

    /// Releases exact staged occupancy after local removal and directory fsync.
    ///
    /// # Errors
    ///
    /// Returns poison when registered durable ownership cannot cover the bytes.
    pub(crate) fn retire_staged_bytes(&self, bytes: u64) -> Result<(), ScribeError> {
        if let Some(volume) = &self.volume {
            volume
                .retire(bytes)
                .map_err(|error| ScribeError::Internal {
                    detail: format!("retire Scribe staged WAL-volume bytes: {error}"),
                })?;
        }
        Ok(())
    }

    /// Trip the concrete WAL disk breaker for deterministic failure-path
    /// tests. This uses the same hard-state check as a real ENOSPC result and
    /// therefore exercises rejection before LSN allocation or file mutation.
    #[cfg(any(test, feature = "test-support"))]
    pub fn trip_disk_full_for_test(&self) {
        self.disk.mark_hard_failed();
    }

    /// Inject one WAL `sync_data` failure after the record write and before
    /// the durable acknowledgment boundary.
    #[cfg(any(test, feature = "test-support"))]
    pub fn trip_sync_failure_for_test(&self) {
        self.disk.trip_sync_failure();
    }

    /// Inject one failure after WAL sync and before memtable insertion.
    #[cfg(any(test, feature = "test-support"))]
    pub fn trip_post_sync_failure_for_test(&self) {
        self.disk.trip_post_sync_failure();
    }

    /// Return the single WAL handle owned by one fixed shard task.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when `shard_id` cannot be represented
    /// by the fixed WAL topology or lies outside the configured shard set.
    pub(crate) fn handle_for_shard(&self, shard_id: usize) -> Result<WalHandle, ScribeError> {
        let shard_id = u8::try_from(shard_id).map_err(|_| ScribeError::Internal {
            detail: format!("invalid WAL shard index: {shard_id}"),
        })?;
        if usize::from(shard_id) >= self.states.len() {
            return Err(ScribeError::Internal {
                detail: format!("WAL shard index is outside the fixed topology: {shard_id}"),
            });
        }
        Ok(WalHandle::for_shard(
            Arc::new(self.clone_for_handle()),
            shard_id,
        ))
    }

    fn clone_for_handle(&self) -> Self {
        Self {
            base_dir: self.base_dir.clone(),
            node_id: self.node_id,
            writer_epoch: self.writer_epoch,
            shard_id: self.shard_id,
            next_lsn: Arc::clone(&self.next_lsn),
            segment_bytes: self.segment_bytes,
            states: Arc::clone(&self.states),
            disk: Arc::clone(&self.disk),
            retirement_refs: Arc::clone(&self.retirement_refs),
            volume: self.volume.clone(),
        }
    }

    /// Append one self-describing v4 slice record for test-tier probes.
    #[cfg(any(test, feature = "test-support"))]
    pub fn append_and_fsync_for_test(
        &self,
        seal_key: &SealKey,
        batch_id: [u8; 16],
        audit_payload: &[u8],
        data_payload: &[u8],
    ) -> Result<WalLsn, ScribeError> {
        self.append_and_fsync_for_key(seal_key, batch_id, audit_payload, data_payload)
    }

    /// Append, commit, and fsync one one-slice v4 batch for replay tests.
    ///
    /// This test-only helper deliberately models the production `SLICE` then
    /// `COMMIT` ordering without changing lower-level WAL tests that need to
    /// observe an individual record.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when either append or the terminal fsync fails.
    ///
    /// # Panics
    ///
    /// Panics only if the fixed sixteen-shard routing domain cannot fit `u8`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn append_and_commit_for_replay_test(
        &self,
        seal_key: &SealKey,
        batch_id: [u8; 16],
        audit_payload: &[u8],
        data_payload: &[u8],
    ) -> Result<WalLsn, ScribeError> {
        let mut prepared = PreparedWalAppend::new(
            WalLsn::ZERO,
            batch_id,
            Bytes::copy_from_slice(audit_payload),
            Bytes::copy_from_slice(data_payload),
        )
        .for_slice(seal_key.clone(), [0; 32]);
        prepared.shard_id = Some(
            u8::try_from(crate::scribe::routing::shard_for(
                seal_key.tenant,
                &seal_key.table,
                uuid::Uuid::from_bytes(batch_id),
            ))
            .expect("fixed shard count fits in u8"),
        );
        let shard_id = prepared.shard_id;
        let slice = self.append_prepared(prepared)?;
        let mut digest = Sha256::new();
        digest.update(0_u32.to_le_bytes());
        digest.update(slice.payload_len.to_le_bytes());
        digest.update(slice.payload_digest);
        let mut commit = PreparedWalAppend::commit(
            batch_id,
            *seal_key.tenant.as_uuid().as_bytes(),
            1,
            digest.finalize().into(),
        );
        commit.shard_id = shard_id;
        let terminal = self.append_prepared(commit)?;
        let mut touched = slice.touched_segments;
        touched.extend(terminal.touched_segments);
        self.sync_segments_with_fault(&touched)?;
        Ok(slice.lsn)
    }

    /// Decodes, commits, and fsyncs one legacy compact frame for replay tests.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when frame decoding or durable WAL writes fail.
    #[cfg(test)]
    pub(crate) fn append_frame_and_commit_for_replay_test(
        &self,
        frame: &[u8],
        seal_key: &SealKey,
    ) -> Result<WalLsn, ScribeError> {
        let decoded = decode_append_frame(frame)?;
        self.append_and_commit_for_replay_test(
            seal_key,
            decoded.batch_id,
            decoded.audit,
            decoded.data,
        )
    }

    /// Append one prepared v4 record without syncing it.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for invalid record identity or length,
    /// poisoned writer state, WAL capacity/disk refusal, segment creation or
    /// append failure, or rollback/accounting divergence.
    pub(crate) fn append_prepared(
        &self,
        mut prepared: PreparedWalAppend,
    ) -> Result<WalAppendResult, ScribeError> {
        let encoded_bytes =
            u64::try_from(prepared.encoded_len()?).map_err(|_| ScribeError::Internal {
                detail: "encoded WAL record length does not fit accounting".to_owned(),
            })?;
        let shard_id = usize::from(prepared.shard_id.unwrap_or(0));
        let mut state = self.states[shard_id]
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "WAL state lock poisoned (append_prepared)".to_owned(),
            })?;
        let rolls_segment = state.current_segment.is_some()
            && state.current_segment_records > 0
            && state.current_segment_size.saturating_add(encoded_bytes) > self.segment_bytes();
        let new_segment_bytes = if state.current_segment.is_none() || rolls_segment {
            SEGMENT_HEADER_SIZE as u64
        } else {
            0
        };
        self.disk.reject_if_hard(
            self.bytes_on_disk(),
            encoded_bytes.saturating_add(new_segment_bytes),
        )?;
        let header_growth = self
            .volume
            .as_ref()
            .filter(|_| new_segment_bytes > 0)
            .map(|volume| volume.try_reserve_growth(new_segment_bytes))
            .transpose()
            .map_err(resource_volume_error)?;
        let record_growth = self
            .volume
            .as_ref()
            .map(|volume| volume.try_reserve_growth(encoded_bytes))
            .transpose()
            .map_err(resource_volume_error)?;
        let lsn = WalLsn::new(self.next_lsn.fetch_add(1, Ordering::SeqCst));
        prepared.assign_lsn(lsn);
        if rolls_segment {
            state.current_segment = None;
            state.current_segment_size = 0;
            state.current_segment_records = 0;
        }
        let segment = self.ensure_segment_locked(
            &mut state,
            u8::try_from(shard_id).map_err(|_| ScribeError::Internal {
                detail: "invalid WAL shard index".to_owned(),
            })?,
        );
        let segment = match segment {
            Ok(segment) => segment,
            Err(error) => {
                if let Some(growth) = header_growth {
                    growth.retain_and_poison();
                }
                return Err(error);
            }
        };
        if let Some(growth) = header_growth {
            growth.commit().map_err(resource_volume_error)?;
        }
        let prior_file_len = std::fs::metadata(segment.path())
            .map_err(|error| wal_io_error("failed to measure WAL before append", &error))?
            .len();
        let encoded_len = usize::try_from(encoded_bytes).map_err(|_| ScribeError::Internal {
            detail: "encoded WAL record length does not fit memory addressing".to_owned(),
        })?;
        let append_result = append_prepared_borrowed(&segment, &prepared, encoded_len);
        let (payload_digest, payload_len) = match append_result {
            Ok(identity) => identity,
            Err(error) => {
                self.rollback_prepared_append(
                    &mut state,
                    &segment,
                    prior_file_len,
                    new_segment_bytes,
                )?;
                if matches!(error, ScribeError::WalDiskFull) {
                    self.disk.mark_hard_failed();
                }
                drop(state);
                self.disk.reconcile();
                return Err(error);
            }
        };
        if let Some(growth) = record_growth {
            segment.retain_volume_growth(growth)?;
        }
        self.disk.add_bytes(encoded_bytes);
        metrics::gauge!("bifrost_scribe_wal_disk_bytes")
            .set(self.bytes_on_disk().to_f64().unwrap_or(f64::MAX));
        state.current_segment_size = state.current_segment_size.saturating_add(encoded_bytes);
        state.current_segment_records = state.current_segment_records.saturating_add(1);
        Ok(WalAppendResult {
            lsn,
            #[cfg(feature = "bench-support")]
            encoded_bytes,
            touched_segments: vec![segment],
            payload_digest,
            payload_len,
        })
    }

    /// Sync every distinct segment touched by a group exactly once.
    pub(crate) fn sync_segments(segments: &[Arc<WalSegment>]) -> Result<(), ScribeError> {
        let mut synced = Vec::with_capacity(segments.len());
        for segment in segments {
            if synced.iter().any(|path: &PathBuf| path == segment.path()) {
                continue;
            }
            segment.sync_data()?;
            synced.push(segment.path().to_path_buf());
        }
        Ok(())
    }

    fn sync_segments_with_fault(&self, segments: &[Arc<WalSegment>]) -> Result<(), ScribeError> {
        let _ = self;
        #[cfg(any(test, feature = "test-support"))]
        if self.disk.take_sync_failure() {
            let result = Err(ScribeError::Internal {
                detail: "injected WAL sync failure".to_owned(),
            });
            record_wal_fsync(&result, Instant::now());
            return result;
        }
        Self::sync_segments(segments)
    }

    /// Return the configured segment size.
    #[must_use]
    pub fn segment_bytes(&self) -> u64 {
        self.segment_bytes
    }

    /// Return the number of currently open fixed-shard WAL streams.
    #[must_use]
    pub fn open_stream_count(&self) -> usize {
        self.states
            .iter()
            .filter_map(|state| state.lock().ok())
            .filter(|state| state.current_segment.is_some())
            .count()
    }

    /// Close every fixed-shard active segment after Scribe has stopped WAL IO.
    ///
    /// Shutdown retains segment files for normal replay but must release every
    /// in-memory active-stream owner. A later process recovery, rather than the
    /// stopped Scribe instance, decides which segment to reopen.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] when a shard WAL-state lock is
    /// poisoned. States closed before the poisoned shard remain closed.
    pub(crate) fn close_all_streams(&self) -> Result<(), ScribeError> {
        for state in self.states.iter() {
            let mut state = state.lock().map_err(|_| ScribeError::Internal {
                detail: "WAL state lock poisoned (shutdown close)".to_owned(),
            })?;
            state.current_segment = None;
            state.current_segment_size = 0;
            state.current_segment_records = 0;
        }
        Ok(())
    }

    /// Append a raw WAL frame for test scenarios, routing by the decoded batch id.
    ///
    /// # Errors
    /// Returns [`ScribeError`] when the frame cannot be decoded or appended.
    #[cfg(test)]
    pub(crate) fn append_frame_for_test(
        &self,
        frame: &[u8],
        seal_key: &SealKey,
    ) -> Result<WalLsn, ScribeError> {
        let decoded = decode_append_frame(frame)?;
        let mut prepared = PreparedWalAppend::new(
            WalLsn::ZERO,
            decoded.batch_id,
            Bytes::copy_from_slice(decoded.audit),
            Bytes::copy_from_slice(decoded.data),
        )
        .for_slice(seal_key.clone(), [0; 32]);
        // Use the decoded batch_id for routing, matching the production path.
        prepared.shard_id = Some(
            u8::try_from(crate::scribe::routing::shard_for(
                seal_key.tenant,
                &seal_key.table,
                uuid::Uuid::from_bytes(decoded.batch_id),
            ))
            .expect("fixed shard count fits in u8"),
        );
        Ok(self.append_prepared(prepared)?.lsn)
    }

    /// Sync the WAL segment for one seal-key's shard to disk.
    ///
    /// The shard is derived from a zero-UUID placeholder because this
    /// test helper is used only for WAL durability probes; the exact
    /// shard is not significant.
    ///
    /// # Errors
    /// Returns [`ScribeError`] when the sync fails.
    #[cfg(test)]
    pub(crate) fn sync_data_for_test(&self, seal_key: &SealKey) -> Result<(), ScribeError> {
        // Attribution-only shard for WAL sync probing; placeholder batch_id.
        let shard_id = u8::try_from(crate::scribe::routing::shard_for(
            seal_key.tenant,
            &seal_key.table,
            uuid::Uuid::nil(),
        ))
        .expect("fixed shard count fits in u8");
        self.sync_data_for_shard(shard_id)
    }

    /// Append one WAL record and fsync, routing by the provided batch id.
    ///
    /// # Errors
    /// Returns [`ScribeError`] when the append or sync fails.
    #[cfg(any(test, feature = "test-support"))]
    fn append_and_fsync_for_key(
        &self,
        seal_key: &SealKey,
        batch_id: [u8; 16],
        audit_payload: &[u8],
        data_payload: &[u8],
    ) -> Result<WalLsn, ScribeError> {
        let mut prepared = PreparedWalAppend::new(
            WalLsn::ZERO,
            batch_id,
            Bytes::copy_from_slice(audit_payload),
            Bytes::copy_from_slice(data_payload),
        )
        .for_slice(seal_key.clone(), [0; 32]);
        // Route using the batch_id to match the production dispatch path.
        prepared.shard_id = Some(
            u8::try_from(crate::scribe::routing::shard_for(
                seal_key.tenant,
                &seal_key.table,
                uuid::Uuid::from_bytes(batch_id),
            ))
            .expect("fixed shard count fits in u8"),
        );
        let result = self.append_prepared(prepared)?;
        self.sync_segments_with_fault(&result.touched_segments)?;
        Ok(result.lsn)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn take_post_sync_failure_for_test(&self) -> bool {
        self.disk.take_post_sync_failure()
    }

    #[cfg(feature = "bench-support")]
    pub(crate) fn sync_data(&self) -> Result<(), ScribeError> {
        self.sync_data_for_shard(0)
    }

    #[cfg(any(test, feature = "bench-support"))]
    fn sync_data_for_shard(&self, shard_id: u8) -> Result<(), ScribeError> {
        let segment = self.states[usize::from(shard_id)]
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "segment lock poisoned (sync_data)".to_string(),
            })?
            .current_segment
            .clone();
        if let Some(segment) = segment
            && let Err(error) = segment.sync_data()
        {
            if matches!(error, ScribeError::WalDiskFull) {
                self.disk.mark_hard_failed();
            }
            return Err(error);
        }
        Ok(())
    }

    /// Retire only closed segments. The active segment remains until rollover
    /// makes every record in it eligible for the next persistence transaction.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when retirement ownership is poisoned
    /// or when a releasable segment cannot be deleted durably.
    pub(crate) fn retire_segments(&self, segments: &[WalSegmentRef]) -> Result<(), ScribeError> {
        let mut releasable = Vec::new();
        let mut references = self
            .retirement_refs
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "WAL retirement reference lock poisoned".to_owned(),
            })?;
        for segment in segments {
            let Some(count) = references.get_mut(&segment.path) else {
                releasable.push(segment.path.clone());
                continue;
            };
            if *count > 1 {
                *count -= 1;
            } else {
                references.remove(&segment.path);
                releasable.push(segment.path.clone());
            }
        }
        drop(references);
        self.delete_closed_segments(&releasable)
    }

    /// Retain segment references for one pending immutable generation.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the retirement-reference lock is
    /// poisoned or the focused retention-failure seam is armed.
    pub(crate) fn retain_segments(&self, segments: &[WalSegmentRef]) -> Result<(), ScribeError> {
        #[cfg(test)]
        if WAL_FAIL_RETAIN.with(|flag| flag.replace(false)) {
            // Fail before touching any refcount so the caller observes state A
            // with the segment map fully intact for an identical retry.
            return Err(ScribeError::Internal {
                detail: "forced WAL retention failure (test seam)".to_owned(),
            });
        }
        let mut references = self
            .retirement_refs
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "WAL retirement reference lock poisoned".to_owned(),
            })?;
        for segment in segments {
            let count = references.entry(segment.path.clone()).or_default();
            *count = count.saturating_add(1);
        }
        Ok(())
    }

    /// Releases one replay-reader reference without making unread bytes deletable.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when retirement ownership is poisoned
    /// or a safe, closed segment cannot be deleted durably.
    fn release_replay_pin(
        &self,
        segment: &WalSegmentRef,
        delete_if_unreferenced: bool,
    ) -> Result<(), ScribeError> {
        let mut references = self
            .retirement_refs
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "WAL retirement reference lock poisoned".to_owned(),
            })?;
        let mut releasable = false;
        if let Some(count) = references.get_mut(&segment.path) {
            if *count > 1 {
                *count -= 1;
            } else {
                references.remove(&segment.path);
                releasable = delete_if_unreferenced;
            }
        } else {
            releasable = delete_if_unreferenced;
        }
        drop(references);
        if releasable {
            self.delete_closed_segments(std::slice::from_ref(&segment.path))?;
        }
        Ok(())
    }

    /// Close a current segment after its final active bucket is detached.
    ///
    /// The segment remains on disk while its immutable generation is pending;
    /// the normal grace-period retirement then removes it. Closing here avoids
    /// retaining a low-volume current file forever when no later append causes
    /// a size rollover.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a shard state lock is poisoned or
    /// returns the segment synchronization error before ownership is cleared.
    pub(crate) fn close_segments_if_unowned(
        &self,
        segments: &[WalSegmentRef],
        active_paths: &HashSet<PathBuf>,
    ) -> Result<(), ScribeError> {
        let candidates = segments
            .iter()
            .map(|segment| segment.path.clone())
            .filter(|path| !active_paths.contains(path))
            .collect::<HashSet<_>>();
        if candidates.is_empty() {
            return Ok(());
        }
        for state in self.states.iter() {
            let mut state = state.lock().map_err(|_| ScribeError::Internal {
                detail: "WAL state lock poisoned (close segment)".to_owned(),
            })?;
            let Some(segment) = state.current_segment.as_ref() else {
                continue;
            };
            if candidates.contains(segment.path()) {
                segment.sync_data()?;
                state.current_segment = None;
                state.current_segment_size = 0;
                state.current_segment_records = 0;
            }
        }
        Ok(())
    }

    fn delete_closed_segments(&self, segments: &[PathBuf]) -> Result<(), ScribeError> {
        let mut active = Vec::new();
        for state in self.states.iter() {
            let state = state.lock().map_err(|_| ScribeError::Internal {
                detail: "WAL state lock poisoned (retire_segments)".to_owned(),
            })?;
            if let Some(segment) = &state.current_segment {
                active.push(segment.path().to_path_buf());
            }
        }
        for path in segments {
            if active.iter().any(|active_path| active_path == path) {
                continue;
            }
            if path.exists() {
                let file_len = std::fs::metadata(path).map_or(0, |metadata| metadata.len());
                std::fs::remove_file(path).map_err(|error| ScribeError::Internal {
                    detail: format!("WAL segment retirement failed: {error}"),
                })?;
                self.disk.subtract_bytes(file_len);
                if let Some(parent) = path.parent() {
                    let directory = File::open(parent).map_err(|error| ScribeError::Internal {
                        detail: format!("failed to open retired WAL directory: {error}"),
                    })?;
                    directory
                        .sync_all()
                        .map_err(|error| ScribeError::Internal {
                            detail: format!("failed to sync retired WAL directory: {error}"),
                        })?;
                }
                if let Some(volume) = &self.volume {
                    volume.retire(file_len).map_err(resource_volume_error)?;
                }
            }
        }
        metrics::gauge!("bifrost_scribe_wal_disk_bytes")
            .set(self.bytes_on_disk().to_f64().unwrap_or(f64::MAX));
        Ok(())
    }

    /// Return the current on-disk byte footprint of this WAL directory.
    #[must_use]
    pub fn bytes_on_disk(&self) -> u64 {
        self.disk.bytes()
    }

    /// Return the current WAL disk-pressure state without reserving an append.
    ///
    /// The one-second filesystem sample is shared with append admission. A
    /// soft result is consumed by Scribe's lifecycle scanner to flush the
    /// oldest unpublished bucket; a hard result rejects the next append.
    #[must_use]
    pub fn disk_pressure(&self) -> WalDiskPressure {
        self.disk.reconcile();
        let bytes = self.bytes_on_disk();
        self.disk.pressure(bytes, 0)
    }

    /// Returns the current shard segment or creates its next ordered segment.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the shard directory or segment
    /// cannot be created, or propagates segment-header persistence failures.
    fn ensure_segment_locked(
        &self,
        state: &mut WalState,
        shard_id: u8,
    ) -> Result<Arc<WalSegment>, ScribeError> {
        if let Some(ref segment) = state.current_segment {
            return Ok(Arc::clone(segment));
        }

        let seq = state.seg_seq;
        state.seg_seq = state.seg_seq.saturating_add(1);
        let stream_dir = self
            .base_dir
            .join(Uuid::from_bytes(self.node_id).simple().to_string())
            .join(self.writer_epoch.to_string());
        let shard_dir = stream_dir.join(format!("shard-{shard_id:02}"));
        std::fs::create_dir_all(&shard_dir).map_err(|error| ScribeError::Internal {
            detail: format!("failed to create WAL shard directory: {error}"),
        })?;
        let path = shard_dir.join(format!("{seq}.wal"));
        let header = SegmentHeader::new(self.node_id, self.writer_epoch, seq, shard_id);

        let segment = Arc::new(WalSegment::create(&path, header)?);
        self.disk.add_bytes(SEGMENT_HEADER_SIZE as u64);
        state.current_segment = Some(Arc::clone(&segment));

        // Initialize segment size to header size
        state.current_segment_size = SEGMENT_HEADER_SIZE as u64;
        state.current_segment_records = 0;

        Ok(segment)
    }
}

/// Removes one newly-created segment after its first record write fails.
///
/// # Errors
///
/// Returns an internal WAL error when unlink or parent-directory fsync fails.
fn remove_failed_segment(path: &Path) -> Result<(), ScribeError> {
    std::fs::remove_file(path)
        .map_err(|error| wal_io_error("failed to unlink rejected WAL segment", &error))?;
    let parent = path.parent().ok_or_else(|| ScribeError::Internal {
        detail: "rejected WAL segment has no parent directory".to_owned(),
    })?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| wal_io_error("failed to fsync rejected WAL segment directory", &error))
}

/// WAL reader — reads segments and returns records.
#[derive(Debug)]
pub struct WalReader {
    segments: Vec<Arc<WalSegment>>,
}

/// Stream identity and shard grouping used to merge WAL segments by LSN.
type WalStreams = BTreeMap<([u8; 16], i64), BTreeMap<u8, Vec<Arc<WalSegment>>>>;

/// Lazy record cursor over one shard's ordered segment chain.
///
/// The cursor retains only a fixed record header while participating in the
/// stream-wide LSN merge. It allocates a payload only after that record is
/// selected as the next record in the stream.
#[derive(Debug)]
struct ShardRecordCursor {
    /// Segments for one `(node, epoch, shard)` ordered by segment sequence.
    segments: Vec<Arc<WalSegment>>,
    /// Index of the next segment that has not yet been opened.
    next_segment: usize,
    /// Independently opened current segment used for replay and tail repair.
    file: Option<File>,
    /// Current segment path returned with the selected record.
    path: Option<PathBuf>,
    /// Byte offset at which the peeked record begins.
    record_offset: u64,
    /// Validated header waiting for stream-wide LSN selection.
    header: Option<DecodedWalRecordHeader>,
    /// Last consumed LSN, spanning every ordered segment in this shard chain.
    previous_lsn: Option<WalLsn>,
    /// Existing WAL handle used to pin each segment while it is being read.
    pin_wal: Option<WalHandle>,
    /// Current segment pin retained until the reader proves the file complete.
    segment_pin: Option<ReplaySegmentPin>,
    /// Whether the final visited record left no unpublished group in this segment.
    segment_retirement_safe: bool,
}

/// Pins one replay segment in the existing WAL retirement reference owner.
///
/// A failed or cancelled scan deliberately drops this object without releasing
/// its reference, preserving unread WAL until process restart. Successful EOF
/// consumes the pin through [`Self::release`].
#[derive(Debug)]
struct ReplaySegmentPin {
    /// Existing shard WAL handle that owns the retirement reference map.
    wal: WalHandle,
    /// Exact segment protected while its reader may still consume records.
    segment: WalSegmentRef,
}

impl ReplaySegmentPin {
    /// Acquires one reader-owned reference before a segment file is opened.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the existing WAL reference owner cannot
    /// retain the segment.
    fn acquire(wal: WalHandle, segment: WalSegmentRef) -> Result<Self, ScribeError> {
        wal.retain_segments(std::slice::from_ref(&segment))?;
        Ok(Self { wal, segment })
    }

    /// Releases the reader reference after the segment reaches a safe EOF.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when retirement reference settlement or durable
    /// closed-file deletion fails.
    fn release(self, delete_if_unreferenced: bool) -> Result<(), ScribeError> {
        self.wal
            .release_replay_pin(&self.segment, delete_if_unreferenced)
    }
}

impl ShardRecordCursor {
    /// Creates a lazy cursor for one ordered shard segment chain.
    #[must_use]
    fn new(mut segments: Vec<Arc<WalSegment>>, wal: Option<WalHandle>) -> Self {
        segments.sort_by(|left, right| {
            left.header()
                .seg_seq
                .cmp(&right.header().seg_seq)
                .then_with(|| left.path().cmp(right.path()))
        });
        Self {
            segments,
            next_segment: 0,
            file: None,
            path: None,
            record_offset: SEGMENT_HEADER_SIZE as u64,
            header: None,
            previous_lsn: None,
            pin_wal: wal,
            segment_pin: None,
            segment_retirement_safe: false,
        }
    }

    /// Returns the next validated LSN without allocating its payload.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a segment cannot be opened or positioned,
    /// a complete header is invalid, or torn-tail repair cannot be persisted.
    fn peek_lsn(&mut self) -> Result<Option<WalLsn>, ScribeError> {
        self.ensure_header()?;
        Ok(self.header.as_ref().map(|header| header.lsn))
    }

    /// Consumes the selected record and authenticates its payload and CRC.
    ///
    /// A torn final payload or CRC is truncated and returns `None`; the caller
    /// can continue with the next segment. Complete corruption fails closed.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for payload corruption, non-monotonic shard LSN,
    /// filesystem errors, or a failed durable torn-tail repair.
    fn take_record(
        &mut self,
        governor: Option<&ScribeResources>,
    ) -> Result<Option<(PathBuf, WalRecord, Option<ScribeMemoryLease>)>, ScribeError> {
        self.ensure_header()?;
        let Some(header) = self.header.take() else {
            return Ok(None);
        };
        if governor.is_some_and(|governor| {
            header.payload_bytes() > governor.maximum_ingress_envelope_bytes()
        }) {
            return Err(ScribeError::IngestBusy {
                table: "WAL recovery segment".to_owned(),
            });
        }
        // The fixed header is validated before this reservation. Holding the
        // lease across decode and the visitor prevents the payload allocation
        // from ever becoming unaccounted recovery memory.
        let payload_memory = governor
            .map(|governor| {
                governor.try_reserve_maintenance(
                    crate::scribe::memory::MemoryCategory::Decode,
                    header.payload_bytes(),
                )
            })
            .transpose()?;
        let file = self.file.as_mut().ok_or_else(|| ScribeError::Internal {
            detail: "WAL replay cursor lost its current segment".to_owned(),
        })?;
        let record = match WalRecord::decode_payload_from(file, &header) {
            Ok(record) => record,
            Err(error) if is_torn_tail_error(&error) => {
                self.repair_torn_tail()?;
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        if self
            .previous_lsn
            .is_some_and(|previous| record.lsn <= previous)
        {
            return Err(ScribeError::Internal {
                detail: "WAL v4 contains non-monotonic committed LSNs across ordered segments"
                    .to_owned(),
            });
        }
        self.previous_lsn = Some(record.lsn);
        let path = self.path.clone().ok_or_else(|| ScribeError::Internal {
            detail: "WAL replay cursor lost its segment path".to_owned(),
        })?;
        Ok(Some((path, record, payload_memory)))
    }

    /// Advances through clean segment ends until one header or stream EOF exists.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for open, seek, header validation, or torn-tail
    /// repair failures.
    fn ensure_header(&mut self) -> Result<(), ScribeError> {
        while self.header.is_none() {
            if self.file.is_none() && !self.open_next_segment()? {
                return Ok(());
            }
            let file = self.file.as_mut().ok_or_else(|| ScribeError::Internal {
                detail: "WAL replay cursor failed to retain an opened segment".to_owned(),
            })?;
            self.record_offset = file
                .stream_position()
                .map_err(|error| ScribeError::Internal {
                    detail: format!("WAL record position failed: {error}"),
                })?;
            match WalRecord::decode_header_from(file) {
                Ok(Some(header)) => self.header = Some(header),
                Ok(None) => {
                    self.file = None;
                    self.path = None;
                    if let Some(pin) = self.segment_pin.take() {
                        pin.release(self.segment_retirement_safe)?;
                    }
                    self.segment_retirement_safe = false;
                }
                Err(error) if is_torn_tail_error(&error) => self.repair_torn_tail()?,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Opens and positions the next segment in this shard chain.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the segment cannot be opened or positioned
    /// immediately after its already-validated prologue.
    fn open_next_segment(&mut self) -> Result<bool, ScribeError> {
        let Some(segment) = self.segments.get(self.next_segment) else {
            return Ok(false);
        };
        self.next_segment = self.next_segment.saturating_add(1);
        self.segment_retirement_safe = false;
        if let Some(wal) = self.pin_wal.as_ref() {
            self.segment_pin = Some(ReplaySegmentPin::acquire(wal.clone(), segment.reference())?);
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(segment.path())
            .map_err(|error| wal_io_error("failed to open WAL segment for replay", &error))?;
        file.seek(SeekFrom::Start(SEGMENT_HEADER_SIZE as u64))
            .map_err(|error| wal_io_error("failed to seek WAL segment for replay", &error))?;
        self.path = Some(segment.path().to_path_buf());
        self.file = Some(file);
        Ok(true)
    }

    /// Truncates and fsyncs the current incomplete final record.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a later segment proves the torn record is
    /// not the stream tail, or when truncation or the repair fsync fails.
    fn repair_torn_tail(&mut self) -> Result<(), ScribeError> {
        if self.next_segment < self.segments.len() {
            return Err(ScribeError::Internal {
                detail: "WAL v4 contains a torn record before a later shard segment".to_owned(),
            });
        }
        let file = self.file.as_mut().ok_or_else(|| ScribeError::Internal {
            detail: "WAL torn-tail repair lost its current segment".to_owned(),
        })?;
        file.set_len(self.record_offset)
            .map_err(|error| wal_io_error("WAL torn-tail truncation failed", &error))?;
        let started = Instant::now();
        #[cfg(test)]
        WAL_RECOVERY_SYNC_COUNT.with(|count| count.set(count.get() + 1));
        let result = file
            .sync_data()
            .map_err(|error| wal_io_error("WAL torn-tail sync failed", &error));
        record_wal_fsync(&result, started);
        result?;
        self.file = None;
        self.path = None;
        self.header = None;
        if let Some(pin) = self.segment_pin.take() {
            pin.release(self.segment_retirement_safe)?;
        }
        self.segment_retirement_safe = false;
        Ok(())
    }
}

/// One validated WAL record paired with its already-charged payload owner.
///
/// Field order keeps the record alive until the payload lease is released on
/// every callback exit. Replay consumes the value to merge that lease into its
/// Decode owner without a release-and-reacquire window.
pub(crate) struct AccountedWalRecord {
    /// Durable segment path contributing the record.
    path: PathBuf,
    /// Validated decoded WAL record.
    record: WalRecord,
    /// Root-backed payload lease present for governed replay.
    payload_memory: Option<ScribeMemoryLease>,
}

impl AccountedWalRecord {
    /// Separates the owned record handoff for replay assembly.
    pub(crate) fn into_parts(self) -> (PathBuf, WalRecord, Option<ScribeMemoryLease>) {
        (self.path, self.record, self.payload_memory)
    }
}

impl WalReader {
    /// Open only segments belonging to `stream`.
    ///
    /// A Scribe WAL directory may retain segments from another writer epoch
    /// after a restart. The live-tail path is stream-scoped, so it must reject
    /// those segments at the read boundary. Tenant IDs are intentionally not a
    /// filter here: one pod stream can contain multiple tenants, while the
    /// tail response is scoped by the stream identity.
    pub fn open_directory(
        dir: impl AsRef<Path>,
        stream: StreamIdentity,
    ) -> Result<Self, ScribeError> {
        Self::open_directory_filtered(dir, Some(stream))
    }

    /// Open all segments in the given directory for replay.
    ///
    /// Replay intentionally reconstructs state across writer epochs. Callers
    /// serving a live stream must use [`Self::open_directory`] so stale or
    /// foreign stream segments are excluded at the read boundary.
    pub fn open_directory_unfiltered(dir: impl AsRef<Path>) -> Result<Self, ScribeError> {
        Self::open_directory_filtered(dir, None)
    }

    /// Opens bounded same-node segments from epochs older than the live writer.
    ///
    /// Recovery begins at the current node directory, so foreign or malformed
    /// WAL trees are never opened or charged as replay work. Eligible files are
    /// bounded by count, path length, individual size, and aggregate bytes
    /// before any record payload is inspected.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when eligible discovery exceeds a V1 bound, an
    /// eligible directory or file cannot be inspected, or segment identities
    /// are invalid, duplicated, or out of order.
    pub fn open_directory_for_recovery(
        dir: impl AsRef<Path>,
        current: StreamIdentity,
    ) -> Result<Self, ScribeError> {
        let node_dir = dir
            .as_ref()
            .join(current.node_id.as_uuid().simple().to_string());
        if !node_dir.exists() {
            return Ok(Self {
                segments: Vec::new(),
            });
        }
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(&node_dir).map_err(|error| ScribeError::Internal {
            detail: format!("failed to read WAL node directory: {error}"),
        })? {
            let entry = entry.map_err(|error| ScribeError::Internal {
                detail: format!("failed to read WAL epoch directory entry: {error}"),
            })?;
            if !entry.path().is_dir() {
                continue;
            }
            let Some(epoch) = entry
                .file_name()
                .to_str()
                .and_then(|value| value.parse::<i64>().ok())
            else {
                continue;
            };
            if epoch >= current.writer_epoch.as_i64() {
                continue;
            }
            collect_bounded_segment_paths(&entry.path(), &mut paths)?;
        }
        Self::open_paths(paths, Some(*current.node_id.as_uuid().as_bytes()))
    }

    /// Opens and orders every segment admitted by the optional stream filter.
    ///
    /// # Errors
    ///
    /// Returns the directory traversal, segment open/header validation,
    /// duplicate-sequence, or stream-ordering error encountered while building
    /// the reader.
    fn open_directory_filtered(
        dir: impl AsRef<Path>,
        stream: Option<StreamIdentity>,
    ) -> Result<Self, ScribeError> {
        let dir = dir.as_ref();
        if !dir.exists() {
            return Ok(Self {
                segments: Vec::new(),
            });
        }

        let mut paths = Vec::new();
        collect_segment_paths(dir, &mut paths)?;
        let mut reader = Self::open_paths(
            paths,
            stream.map(|stream| *stream.node_id.as_uuid().as_bytes()),
        )?;
        if let Some(stream) = stream {
            reader
                .segments
                .retain(|segment| segment.header().writer_epoch == stream.writer_epoch.as_i64());
        }
        Ok(reader)
    }

    /// Opens, sorts, and validates already-discovered WAL paths.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when a segment cannot be opened, belongs to a
    /// different node, or duplicates an existing stream/shard/sequence key.
    fn open_paths(paths: Vec<PathBuf>, node_filter: Option<[u8; 16]>) -> Result<Self, ScribeError> {
        let mut segments = Vec::new();
        for path in paths {
            if path.extension().and_then(|s| s.to_str()) == Some("wal") {
                let segment = WalSegment::open(&path)?;
                if node_filter.is_some_and(|node_id| segment.header().node_id != node_id) {
                    continue;
                }
                segments.push(Arc::new(segment));
            }
        }

        // Segment filenames are decimal sequence numbers, so lexicographic
        // path ordering would place `10.wal` before `2.wal`. Replay and live
        // tail both depend on physical stream order for LSN monotonicity;
        // sort by the self-describing header fields and use the path only as
        // a deterministic tie breaker for duplicate paths.
        segments.sort_by(|left, right| {
            let left_header = left.header();
            let right_header = right.header();
            left_header
                .node_id
                .cmp(&right_header.node_id)
                .then_with(|| left_header.writer_epoch.cmp(&right_header.writer_epoch))
                .then_with(|| left_header.shard_id.cmp(&right_header.shard_id))
                .then_with(|| left_header.seg_seq.cmp(&right_header.seg_seq))
                .then_with(|| left.path().cmp(right.path()))
        });

        for pair in segments.windows(2) {
            let left = pair[0].header();
            let right = pair[1].header();
            if left.node_id == right.node_id
                && left.writer_epoch == right.writer_epoch
                && left.shard_id == right.shard_id
                && left.seg_seq == right.seg_seq
            {
                return Err(ScribeError::Internal {
                    detail: "duplicate WAL stream/shard/segment identity".to_owned(),
                });
            }
        }

        Ok(Self { segments })
    }

    /// Read all records from all segments.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when segment framing, CRC, ordering, identity,
    /// payload decoding, or torn-tail recovery validation fails.
    pub fn read_all_records(&self) -> Result<Vec<WalRecord>, ScribeError> {
        let mut all_records = Vec::new();
        self.for_each_stream_record(|_stream, _shard_id, _path, record| {
            all_records.push(record);
            Ok(())
        })?;
        Ok(all_records)
    }

    /// Visit records together with the self-described stream and shard that
    /// owns them.
    ///
    /// Recovery uses this form so records from an earlier writer epoch retain
    /// their original publication and manifest identity, and the exact
    /// `shard_id` from the segment header is threaded through to the visitor
    /// so that replayed appends can be dispatched back to their recorded lane
    /// without recomputing the routing key.
    ///
    /// # Errors
    /// Returns [`ScribeError`] when a segment cannot be read or its records
    /// fail WAL validation.
    pub(crate) fn for_each_stream_record<F>(&self, visit: F) -> Result<(), ScribeError>
    where
        F: FnMut(StreamIdentity, u8, PathBuf, WalRecord) -> Result<(), ScribeError>,
    {
        let mut visit = visit;
        self.for_each_stream_record_accounted(None, None, |stream, shard, accounted| {
            let (path, record, memory) = accounted.into_parts();
            drop(memory);
            visit(stream, shard, path, record).map(|()| false)
        })
    }

    /// Visits records while reserving each declared payload before allocation.
    ///
    /// The reservation is derived only from a validated fixed header and stays
    /// live until the visitor returns. Replay therefore holds at most one
    /// separately accounted encoded record while it constructs the current
    /// decoded batch. When `pin_writer` is present, the visitor returns `true`
    /// only after the current record's complete group is durably settled; EOF
    /// may unlink the pinned segment only when its final callback returned true.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for admission refusal, invalid WAL framing,
    /// filesystem failures, non-monotonic ordering, or visitor failure.
    pub(crate) fn for_each_stream_record_accounted<F>(
        &self,
        governor: Option<&ScribeResources>,
        pin_writer: Option<&WalWriter>,
        mut visit: F,
    ) -> Result<(), ScribeError>
    where
        F: FnMut(StreamIdentity, u8, AccountedWalRecord) -> Result<bool, ScribeError>,
    {
        let mut streams: WalStreams = BTreeMap::new();
        for segment in &self.segments {
            let header = segment.header();
            streams
                .entry((header.node_id, header.writer_epoch))
                .or_default()
                .entry(header.shard_id)
                .or_default()
                .push(Arc::clone(segment));
        }
        for ((node_id, writer_epoch), shards) in streams {
            let stream = StreamIdentity::new(
                crate::scribe::stream_identity::NodeId::new(uuid::Uuid::from_bytes(node_id)),
                crate::scribe::stream_identity::WriterEpoch::new(writer_epoch),
            );
            let mut cursors = shards
                .into_iter()
                .map(|(shard_id, segments)| {
                    let pin_wal = pin_writer
                        .as_ref()
                        .map(|writer| writer.handle_for_shard(usize::from(shard_id)))
                        .transpose()?;
                    Ok((shard_id, ShardRecordCursor::new(segments, pin_wal)))
                })
                .collect::<Result<Vec<_>, ScribeError>>()?;
            let mut previous_lsn = None;
            loop {
                let mut selected: Option<(usize, WalLsn)> = None;
                for (index, (_shard_id, cursor)) in cursors.iter_mut().enumerate() {
                    let Some(lsn) = cursor.peek_lsn()? else {
                        continue;
                    };
                    if selected.is_none_or(|(_, selected_lsn)| lsn < selected_lsn) {
                        selected = Some((index, lsn));
                    }
                }
                let Some((index, _)) = selected else {
                    break;
                };
                let (shard_id, cursor) = &mut cursors[index];
                let Some((path, record, payload_memory)) = cursor.take_record(governor)? else {
                    continue;
                };
                if previous_lsn.is_some_and(|previous| record.lsn <= previous) {
                    return Err(ScribeError::Internal {
                        detail: "WAL v4 contains non-monotonic stream-wide committed LSNs"
                            .to_owned(),
                    });
                }
                previous_lsn = Some(record.lsn);
                cursor.segment_retirement_safe = visit(
                    stream,
                    *shard_id,
                    AccountedWalRecord {
                        path,
                        record,
                        payload_memory,
                    },
                )?;
            }
        }
        Ok(())
    }
}

fn collect_segment_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), ScribeError> {
    for entry in std::fs::read_dir(dir).map_err(|error| ScribeError::Internal {
        detail: format!("failed to read WAL directory: {error}"),
    })? {
        let entry = entry.map_err(|error| ScribeError::Internal {
            detail: format!("failed to read WAL directory entry: {error}"),
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_segment_paths(&path, paths)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("wal") {
            paths.push(path);
        }
    }
    Ok(())
}

/// Collects eligible WAL paths within structural V1 recovery limits.
///
/// # Errors
///
/// Returns [`ScribeError`] for filesystem failures or when file count, path,
/// or path bounds are exceeded.
fn collect_bounded_segment_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), ScribeError> {
    for entry in std::fs::read_dir(dir).map_err(|error| ScribeError::Internal {
        detail: format!("failed to read eligible WAL directory: {error}"),
    })? {
        let entry = entry.map_err(|error| ScribeError::Internal {
            detail: format!("failed to read eligible WAL directory entry: {error}"),
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_bounded_segment_paths(&path, paths)?;
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("wal") {
            continue;
        }
        if path.as_os_str().as_encoded_bytes().len() > WAL_RECOVERY_PATH_LIMIT {
            return Err(ScribeError::Internal {
                detail: "eligible WAL path exceeds recovery limit".to_owned(),
            });
        }
        if paths.len() == WAL_RECOVERY_FILE_LIMIT {
            return Err(ScribeError::IngestBusy {
                table: "WAL recovery file inventory".to_owned(),
            });
        }
        paths.push(path);
    }
    Ok(())
}

/// Scribe-local metadata derived from one WAL slice record and its Arrow data.
///
/// This struct carries seal-key context and LSN range; it does NOT live inside
/// the `AuditEvent` (which is the canonical CONTRACTS §11 shape).
#[derive(Debug, Clone)]
pub struct ScribeAppendMeta {
    /// Opaque batch ID for dedup.
    pub batch_id: [u8; 16],
    /// Stable Arrow schema identity for logical retry comparison.
    pub schema_fingerprint: [u8; 32],
    /// SHA-256 digest of canonical Arrow data, excluding volatile audit bytes.
    pub data_digest: [u8; 32],
    /// Canonical Arrow data length, excluding volatile audit bytes.
    pub data_len: u32,
    /// SHA-256 digest of the exact WAL-v4 slice payload.
    pub payload_digest: [u8; 32],
    /// Exact WAL-v4 slice payload length.
    pub payload_len: u32,
    /// Zero-based ordinal in the committed batch slice set.
    pub slice_index: u32,
    /// Complete committed batch slice count.
    pub slice_count: u32,
    /// Number of rows accepted in this append.
    pub rows_accepted: usize,
    /// Minimum LSN for this append's WAL records.
    pub wal_lsn_min: WalLsn,
    /// Maximum LSN for this append's WAL records.
    pub wal_lsn_max: WalLsn,
    /// Canonical seal-key path components for this append.
    pub seal_key: String,
}

/// Exact retry identity retained with one active or immutable append slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScribeAppendPayloadIdentity {
    /// Stable caller batch identity.
    pub(crate) batch_id: [u8; 16],
    /// Stable Arrow schema identity.
    pub(crate) schema_fingerprint: [u8; 32],
    /// SHA-256 digest of canonical Arrow data.
    pub(crate) data_digest: [u8; 32],
    /// Canonical Arrow data length.
    pub(crate) data_len: u32,
    /// Zero-based ordinal in the committed batch slice set.
    pub(crate) slice_index: u32,
    /// Complete committed batch slice count.
    pub(crate) slice_count: u32,
}

/// Compute CRC32C hash of the given bytes.
fn crc32c_hash(data: &[u8]) -> u32 {
    crc32c::crc32c(data)
}

/// Resets the test-only count of replay payload allocations.
#[cfg(test)]
pub(crate) fn reset_replay_payload_allocations_for_test() {
    WAL_REPLAY_PAYLOAD_ALLOCATIONS.store(0, Ordering::Release);
}

/// Returns the test-only count of replay payload allocations.
#[cfg(test)]
pub(crate) fn replay_payload_allocations_for_test() -> u64 {
    WAL_REPLAY_PAYLOAD_ALLOCATIONS.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scribe::seal_key::EventDay;
    use tempfile::TempDir;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

    fn test_seal_key(tenant: DataTenantId) -> SealKey {
        SealKey::new(
            tenant,
            TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "wal-test"),
            EventDay::new(chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("test date")),
        )
    }

    /// Accounted replay transfers payload ownership through every callback exit.
    ///
    /// # Panics
    ///
    /// Panics if a successful, failed, or cancellation-shaped callback lacks
    /// its payload lease or leaves charged root memory after returning.
    #[test]
    fn accounted_replay_handoff_owns_payload_on_success_error_and_cancellation() {
        let directory = TempDir::new().expect("temporary WAL directory");
        let writer =
            WalWriter::new(directory.path(), [9; 16], 1, WalConfig::default()).expect("writer");
        writer
            .append_and_commit_for_replay_test(
                &test_seal_key(crate::test_support::tenant()),
                [7; 16],
                b"audit",
                b"payload",
            )
            .expect("complete replay batch");
        let resources =
            crate::scribe::embedded_scribe_resources(&crate::scribe::AdmissionConfig::default());
        let baseline = resources.snapshot().expect("baseline snapshot");
        let reader = WalReader::open_directory_unfiltered(directory.path()).expect("reader");

        reader
            .for_each_stream_record_accounted(Some(&resources), None, |_, _, accounted| {
                let (_, _, payload) = accounted.into_parts();
                assert!(payload.is_some());
                Ok(false)
            })
            .expect("successful accounted handoff");
        assert_eq!(
            resources
                .snapshot()
                .expect("success snapshot")
                .scribe_memory_used_bytes,
            baseline.scribe_memory_used_bytes
        );

        let error = reader
            .for_each_stream_record_accounted(Some(&resources), None, |_, _, accounted| {
                let (_, _, payload) = accounted.into_parts();
                assert!(payload.is_some());
                Err(ScribeError::Internal {
                    detail: "injected replay callback failure".to_owned(),
                })
            })
            .expect_err("callback failure");
        assert!(
            error
                .to_string()
                .contains("injected replay callback failure")
        );
        assert_eq!(
            resources
                .snapshot()
                .expect("failure snapshot")
                .scribe_memory_used_bytes,
            baseline.scribe_memory_used_bytes
        );

        let cancelled = std::sync::atomic::AtomicBool::new(true);
        let _ = reader
            .for_each_stream_record_accounted(Some(&resources), None, |_, _, accounted| {
                let (_, _, payload) = accounted.into_parts();
                assert!(payload.is_some());
                if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(ScribeError::Internal {
                        detail: "WAL replay cancelled".to_owned(),
                    });
                }
                Ok(false)
            })
            .expect_err("cancellation-shaped callback failure");
        assert_eq!(
            resources
                .snapshot()
                .expect("cancel snapshot")
                .scribe_memory_used_bytes,
            baseline.scribe_memory_used_bytes
        );
    }

    /// Encodes one valid audit envelope for replay identity reconstruction.
    fn replay_audit() -> Vec<u8> {
        crate::scribe::audit_envelope::encode_audit_event(&AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "scribe.replay".to_owned(),
            resource: "vala.bifrost.replay".to_owned(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Jwt,
            permission: "bifrost:write".to_owned(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "1 row".to_owned(),
            detail: None,
        })
        .expect("audit envelope")
    }

    #[test]
    fn wal_lsn_ordering() {
        assert!(WalLsn::new(1) > WalLsn::ZERO);
        assert!(WalLsn::new(100) > WalLsn::new(99));
    }

    /// Proves directory replay merges shard segments by the pod-global LSN.
    #[test]
    fn wal_reader_orders_records_by_stream_wide_lsn_across_shards() {
        let directory = TempDir::new().expect("temporary WAL directory");
        let writer = WalWriter::new(directory.path(), [31_u8; 16], 1, WalConfig::default())
            .expect("WAL writer");
        let tenant = crate::test_support::tenant();
        let seal_key = test_seal_key(tenant);
        let shard_zero = writer.handle_for_shard(0).expect("shard zero");
        let shard_five = writer.handle_for_shard(5).expect("shard five");

        for (handle, batch, data) in [
            (&shard_zero, [1_u8; 16], b"zero".as_slice()),
            (&shard_five, [2_u8; 16], b"one".as_slice()),
            (&shard_zero, [3_u8; 16], b"two".as_slice()),
        ] {
            let append = PreparedWalAppend::new(
                WalLsn::ZERO,
                batch,
                Bytes::new(),
                Bytes::copy_from_slice(data),
            )
            .for_slice(seal_key.clone(), [7_u8; 32]);
            let result = handle.append_prepared(append).expect("append shard record");
            handle
                .sync_segments(&result.touched_segments)
                .expect("sync shard record");
        }

        let reader = WalReader::open_directory_unfiltered(directory.path()).expect("WAL reader");
        let lsns = reader
            .read_all_records()
            .expect("stream-wide ordered records")
            .into_iter()
            .map(|record| record.lsn.as_u64())
            .collect::<Vec<_>>();
        assert_eq!(lsns, vec![0, 1, 2]);
    }

    /// Proves duplicate LSNs in different shard segments fail the stream.
    #[test]
    fn wal_reader_rejects_duplicate_stream_wide_lsn_across_shards() {
        let directory = TempDir::new().expect("temporary WAL directory");
        for shard_id in [0_u8, 1] {
            let shard_dir = directory.path().join(format!("shard-{shard_id:02}"));
            std::fs::create_dir_all(&shard_dir).expect("shard directory");
            let segment = WalSegment::create(
                shard_dir.join("0.wal"),
                SegmentHeader::new([32_u8; 16], 1, 0, shard_id),
            )
            .expect("WAL segment");
            segment
                .append_and_fsync(&WalRecord::new(
                    WalLsn::new(7),
                    0,
                    [shard_id; 16],
                    vec![shard_id],
                ))
                .expect("duplicate-LSN fixture");
        }

        let reader = WalReader::open_directory_unfiltered(directory.path()).expect("WAL reader");
        let error = reader
            .read_all_records()
            .expect_err("duplicate stream-wide LSN must fail closed");
        assert!(matches!(
            error,
            ScribeError::Internal { detail }
                if detail == "WAL v4 contains non-monotonic stream-wide committed LSNs"
        ));
    }

    /// Proves a torn record before a later shard segment fails without repair.
    #[test]
    fn wal_reader_rejects_torn_nonfinal_segment_without_truncation() {
        let directory = TempDir::new().expect("temporary WAL directory");
        let shard_dir = directory.path().join("shard-00");
        std::fs::create_dir_all(&shard_dir).expect("shard directory");
        let first_path = shard_dir.join("0.wal");
        let first = WalSegment::create(&first_path, SegmentHeader::new([33_u8; 16], 1, 0, 0))
            .expect("first WAL segment");
        first
            .append_and_fsync(&WalRecord::new(WalLsn::new(0), 0, [1_u8; 16], vec![1_u8]))
            .expect("first durable record");
        drop(first);
        OpenOptions::new()
            .append(true)
            .open(&first_path)
            .expect("open first segment tail")
            .write_all(b"torn")
            .expect("append torn record header");
        let torn_len = std::fs::metadata(&first_path)
            .expect("first segment metadata")
            .len();

        let second = WalSegment::create(
            shard_dir.join("1.wal"),
            SegmentHeader::new([33_u8; 16], 1, 1, 0),
        )
        .expect("second WAL segment");
        second
            .append_and_fsync(&WalRecord::new(WalLsn::new(1), 0, [2_u8; 16], vec![2_u8]))
            .expect("later durable record");

        let reader = WalReader::open_directory_unfiltered(directory.path()).expect("WAL reader");
        let error = reader
            .read_all_records()
            .expect_err("torn non-final record must fail closed");
        assert!(matches!(
            error,
            ScribeError::Internal { detail }
                if detail == "WAL v4 contains a torn record before a later shard segment"
        ));
        assert_eq!(
            std::fs::metadata(first_path)
                .expect("first segment metadata after rejection")
                .len(),
            torn_len,
            "fail-stop must not truncate a non-final segment"
        );
    }

    #[test]
    fn slice_tenant_decoder_rejects_nil_and_non_v7() {
        assert!(decode_slice_tenant(&[0; 16]).is_err());
        assert!(decode_slice_tenant(Uuid::new_v4().as_bytes()).is_err());
    }

    #[test]
    fn slice_tenant_decoder_round_trips_v7() {
        let tenant = DataTenantId::new_v7();
        let encoded = uuid::Uuid::from(tenant).into_bytes();
        assert!(matches!(decode_slice_tenant(&encoded), Ok(decoded) if decoded == tenant));
    }

    #[test]
    fn segment_header_roundtrip() {
        let node_id = [1u8; 16];
        let header = SegmentHeader::new(node_id, 42, 7, 3);

        let mut encoded = header.encode();
        encoded[60..64].copy_from_slice(&header.crc32c.to_le_bytes());

        let decoded = SegmentHeader::decode(&encoded).expect("decode header");
        assert_eq!(decoded.node_id, node_id);
        assert_eq!(decoded.writer_epoch, 42);
        assert_eq!(decoded.seg_seq, 7);
        assert_eq!(decoded.shard_id, 3);
    }

    /// Proves both reserved byte ranges in the v4 segment prologue fail closed.
    #[test]
    fn segment_header_rejects_nonzero_reserved_byte_ranges() {
        for reserved_index in [33_usize, 39, 48, 59] {
            let header = SegmentHeader::new([1_u8; 16], 42, 7, 3);
            let mut encoded = header.encode();
            encoded[reserved_index] = 1;
            let crc = crc32c_hash(&encoded[..60]);
            encoded[60..64].copy_from_slice(&crc.to_le_bytes());

            let error = SegmentHeader::decode(&encoded)
                .expect_err("non-zero reserved segment byte must fail closed");
            assert!(matches!(
                error,
                ScribeError::Internal { detail }
                    if detail == "segment header reserved bytes non-zero"
            ));
        }
    }

    #[test]
    fn wal_v2_header_is_rejected_with_typed_error() {
        let mut header = SegmentHeader::new([1_u8; 16], 42, 7, 3);
        header.version = 2;
        let mut encoded = header.encode();
        let crc = crc32c_hash(&encoded[..60]);
        encoded[60..64].copy_from_slice(&crc.to_le_bytes());

        let error = SegmentHeader::decode(&encoded).expect_err("v2 header must fail closed");
        assert!(matches!(
            error,
            ScribeError::UnsupportedWalVersion { version: 2 }
        ));
    }

    /// Proves the v4 record header carries the required slice identity.
    #[test]
    fn wal_record_roundtrip() {
        let batch_id = [42u8; 16];
        let record = WalRecord::new(WalLsn::new(5), 2, batch_id, b"test data".to_vec());
        let encoded = record.encode();

        let mut cursor = std::io::Cursor::new(encoded);
        let decoded = WalRecord::decode_from(&mut cursor)
            .expect("decode record")
            .expect("non-empty");

        assert_eq!(decoded.lsn, WalLsn::new(5));
        assert!(decoded.is_slice());
        assert_eq!(decoded.batch_id, batch_id);
        assert_eq!(decoded.payload, b"test data");
    }

    /// Proves a v4 commit has the exact fixed header fields and digest payload.
    #[test]
    fn wal_commit_record_uses_the_terminal_slice_identity() {
        let record = WalRecord::commit(WalLsn::new(9), [7; 16], [8; 16], 3, [9; 32]);
        let encoded = record.encode();

        assert_eq!(&encoded[..8], b"WYRDWAL4");
        assert_eq!(encoded.len(), RECORD_HEADER_SIZE + 32 + 4);
        let decoded = WalRecord::decode_from(&mut std::io::Cursor::new(encoded))
            .expect("decode terminal record")
            .expect("terminal record exists");
        assert!(decoded.is_commit());
        assert_eq!(decoded.slice_index, 3);
        assert_eq!(decoded.slice_count, 3);
        assert_eq!(decoded.payload, vec![9; 32]);
    }

    #[test]
    fn slice_payload_decoder_rejects_trailing_bytes() {
        let seal_key = test_seal_key(crate::test_support::tenant());
        let mut payload = encode_slice_payload(&seal_key, [7_u8; 32], b"audit", b"data")
            .expect("encode slice payload");
        payload.push(0xff);

        let error = decode_slice_payload(&payload).expect_err("trailing bytes must fail closed");
        assert!(error.to_string().contains("trailing bytes"));
    }

    #[test]
    fn slice_payload_decoder_rejects_invalid_table_identity() {
        let tenant = crate::test_support::tenant();
        let mut payload = encode_slice_payload_parts(
            tenant.as_uuid().as_bytes(),
            "vala.bifrost.invalid/table",
            "2026-01-01",
            &[0_u8; 32],
            b"audit",
            b"data",
        )
        .expect("encode malformed identity fixture");

        let error = decode_slice_payload(&payload).expect_err("invalid table must fail closed");
        assert!(error.to_string().contains("WAL table FQN is invalid"));
        payload[0] = b'X';
        let error = decode_slice_payload(&payload).expect_err("magic must fail closed");
        assert!(error.to_string().contains("magic mismatch"));
    }

    #[test]
    fn wal_writer_append_and_read() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = [3u8; 16];
        let tenant_id = crate::test_support::tenant();

        let writer =
            WalWriter::new(temp_dir.path(), node_id, 1, WalConfig::default()).expect("writer");

        let batch_id = [1u8; 16];
        let lsn = writer
            .append_and_fsync_for_test(&test_seal_key(tenant_id), batch_id, b"audit", b"data")
            .expect("append");

        assert_eq!(lsn, WalLsn::new(0));

        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        let records = reader.read_all_records().expect("read records");

        assert_eq!(records.len(), 1);
        assert!(records[0].is_slice());
        let decoded = decode_slice_payload(&records[0].payload).expect("slice");
        assert_eq!(decoded.audit, b"audit");
        assert_eq!(decoded.data, b"data");
    }

    /// A durable append owns one successful append and fsync observation.
    #[test]
    fn wal_append_and_fsync_emit_truthful_owner_metrics() {
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let directory = TempDir::new().expect("temporary WAL directory");
            let writer = WalWriter::new(
                directory.path(),
                [7; 16],
                1,
                WalConfig::new(1024 * 1024).expect("valid WAL config"),
            )
            .expect("WAL writer");
            writer
                .append_and_fsync_for_test(
                    &test_seal_key(crate::test_support::tenant()),
                    [3; 16],
                    b"audit",
                    b"data",
                )
                .expect("durable append");
        });
        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_wal_append_total{outcome=\"success\"}"),
            Some(&1)
        );
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_wal_fsync_total{outcome=\"success\"}"),
            Some(&1)
        );
        assert!(
            snapshot
                .counters
                .get("bifrost_scribe_wal_append_bytes_total")
                .is_some_and(|bytes| *bytes > 0)
        );
        assert!(
            snapshot
                .gauges
                .get("bifrost_scribe_wal_disk_bytes")
                .is_some_and(|bytes| *bytes > 0.0)
        );
    }

    #[test]
    fn wal_handles_share_stream_lsn_allocation() {
        let temp_dir = TempDir::new().expect("temp dir");
        let writer =
            WalWriter::new(temp_dir.path(), [8u8; 16], 1, WalConfig::default()).expect("writer");
        let table =
            crate::catalog::TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "events");
        let first = SealKey::new(
            crate::test_support::tenant(),
            table.clone(),
            EventDay::new(chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("date")),
        );
        let second = SealKey::new(
            crate::test_support::tenant(),
            table,
            EventDay::new(chrono::NaiveDate::from_ymd_opt(2026, 1, 2).expect("date")),
        );
        let frame = encode_append_frame([1u8; 16], b"audit", b"data").expect("frame");

        let first_lsn = writer
            .append_frame_for_test(&frame, &first)
            .expect("first append");
        let second_lsn = writer
            .append_frame_for_test(&frame, &second)
            .expect("second append");

        assert_eq!(first_lsn, WalLsn::new(0));
        assert_eq!(second_lsn, WalLsn::new(1));
    }

    #[test]
    fn wal_segment_rolls_on_size() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = [4u8; 16];
        let tenant_id = crate::test_support::tenant();

        // Create writer with 500-byte max segment size
        let writer = WalWriter::new(
            temp_dir.path(),
            node_id,
            1,
            WalConfig {
                segment_bytes: 500,
                disk_limit_bytes: None,
            },
        )
        .expect("writer");

        // Write appends totaling >500 bytes (each record has overhead).
        // All appends use the same batch_id so they route to the same shard and
        // the per-shard seg_seq counter increments monotonically.  (The WAL does
        // not deduplicate; only replay does.)
        let batch_id = [1u8; 16];
        for i in 0u8..10 {
            let payload = format!("data-{i:03}").repeat(20); // ~100 bytes per payload
            writer
                .append_and_fsync_for_test(
                    &test_seal_key(tenant_id),
                    batch_id,
                    payload.as_bytes(),
                    payload.as_bytes(),
                )
                .expect("append");
        }

        // Verify multiple segments were created
        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        assert!(
            reader.segments.len() > 1,
            "expected multiple segments, got {}",
            reader.segments.len()
        );

        // Verify all records are readable across segments
        let records = reader.read_all_records().expect("read records");
        assert_eq!(records.len(), 10);

        // Verify seg_seq increments
        let seg0_header = &reader.segments[0].header;
        let seg1_header = &reader.segments[1].header;
        assert_eq!(seg1_header.seg_seq, seg0_header.seg_seq + 1);
    }

    #[test]
    fn reopening_a_stream_advances_segment_and_lsn_counters() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = [5_u8; 16];
        let tenant_id = crate::test_support::tenant();
        let config = WalConfig {
            segment_bytes: 500,
            disk_limit_bytes: None,
        };
        let key = test_seal_key(tenant_id);
        let data = vec![b'd'; 600];
        let first_lsn = {
            let writer = WalWriter::new(temp_dir.path(), node_id, 1, config).expect("writer");
            writer
                .append_and_fsync_for_test(&key, [1_u8; 16], b"audit", &data)
                .expect("first append")
        };

        let writer = WalWriter::new(temp_dir.path(), node_id, 1, config).expect("reopened writer");
        // Use the same batch_id as the first append so both writes route to the
        // same shard and the per-shard seg_seq counter advances to 1 after the
        // first segment fills.  The WAL itself does not deduplicate; only replay
        // does.
        let second_lsn = writer
            .append_and_fsync_for_test(&key, [1_u8; 16], b"audit", &data)
            .expect("second append");

        assert_eq!(first_lsn, WalLsn::new(0));
        assert_eq!(second_lsn, WalLsn::new(1));
        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        assert_eq!(reader.read_all_records().expect("records").len(), 2);
        assert!(
            reader
                .segments
                .iter()
                .any(|segment| segment.header.seg_seq == 1)
        );
    }

    #[test]
    fn retirement_deletes_only_closed_segments() {
        let temp_dir = TempDir::new().expect("temp dir");
        let writer = WalWriter::new(
            temp_dir.path(),
            [9u8; 16],
            1,
            WalConfig {
                segment_bytes: 500,
                disk_limit_bytes: None,
            },
        )
        .expect("writer");
        // Use a fixed batch_id so all appends route to the same shard and the
        // per-shard segment sequence advances predictably across the 500-byte limit.
        let batch_id = [1u8; 16];
        for index in 0u8..10 {
            writer
                .append_and_fsync_for_test(
                    &test_seal_key(crate::test_support::tenant()),
                    batch_id,
                    b"audit",
                    &[index; 120],
                )
                .expect("append");
        }
        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        assert!(reader.segments.len() > 1);
        let first = reader.segments[0].reference();
        let current = reader.segments.last().expect("current segment").reference();
        writer
            .retire_segments(&[first.clone(), current.clone()])
            .expect("retire closed segment");
        assert!(!first.path.exists());
        assert!(current.path.exists());
    }

    #[test]
    fn low_volume_current_segment_closes_when_last_bucket_detaches() {
        let temp_dir = TempDir::new().expect("temp dir");
        let writer =
            WalWriter::new(temp_dir.path(), [12_u8; 16], 1, WalConfig::default()).expect("writer");
        let seal_key = test_seal_key(crate::test_support::tenant());
        writer
            .append_and_fsync_for_test(&seal_key, [1_u8; 16], b"audit", b"data")
            .expect("append");

        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        assert_eq!(reader.segments.len(), 1);
        let current = reader
            .segments
            .first()
            .expect("one current segment")
            .reference();
        writer
            .close_segments_if_unowned(
                std::slice::from_ref(&current),
                &std::collections::HashSet::new(),
            )
            .expect("close current segment");
        assert!(current.path.exists(), "grace retirement owns deletion");

        writer
            .append_and_fsync_for_test(&seal_key, [2_u8; 16], b"audit", b"data")
            .expect("append after close");
        let reopened = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        assert_eq!(reopened.read_all_records().expect("records").len(), 2);
        assert!(
            reopened.segments.len() >= 2,
            "closed segment must not be reused"
        );
    }

    #[test]
    fn disk_pressure_uses_soft_and_hard_boundaries() {
        let soft = evaluate_disk_pressure(1_000, 799, 800, u64::MAX);
        assert!(soft.soft);
        assert!(!soft.hard);

        let hard = evaluate_disk_pressure(1_000, 899, 900, u64::MAX);
        assert!(hard.hard);

        let low_free = evaluate_disk_pressure(1_000, 0, 1, WAL_HARD_FREE_BYTES - 1);
        assert!(low_free.hard);
    }

    #[test]
    fn configured_disk_limit_rejects_before_creating_a_segment() {
        let temp_dir = TempDir::new().expect("temp dir");
        let writer = WalWriter::new(
            temp_dir.path(),
            [10u8; 16],
            1,
            WalConfig {
                segment_bytes: 500,
                disk_limit_bytes: Some(100),
            },
        )
        .expect("writer");

        let error = writer
            .append_and_fsync_for_test(
                &test_seal_key(crate::test_support::tenant()),
                [1u8; 16],
                b"audit",
                b"data",
            )
            .expect_err("disk hard limit");
        assert!(matches!(error, ScribeError::WalDiskFull));
        assert_eq!(writer.bytes_on_disk(), 0);
    }

    #[test]
    fn segment_retirement_waits_for_every_generation_reference() {
        let temp_dir = TempDir::new().expect("temp dir");
        let writer = WalWriter::new(
            temp_dir.path(),
            [11u8; 16],
            1,
            WalConfig {
                segment_bytes: 500,
                disk_limit_bytes: None,
            },
        )
        .expect("writer");
        // Use a fixed batch_id so all appends route to the same shard and
        // produce a contiguous sequence of segments on that shard's directory.
        let batch_id = [1u8; 16];
        for index in 0u8..10 {
            writer
                .append_and_fsync_for_test(
                    &test_seal_key(crate::test_support::tenant()),
                    batch_id,
                    b"audit",
                    &[index; 120],
                )
                .expect("append");
        }
        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        let first = reader.segments[0].reference();
        writer
            .retain_segments(std::slice::from_ref(&first))
            .expect("retain first generation");
        writer
            .retain_segments(std::slice::from_ref(&first))
            .expect("retain second generation");

        writer
            .retire_segments(std::slice::from_ref(&first))
            .expect("first generation retires");
        assert!(first.path.exists());
        writer
            .retire_segments(std::slice::from_ref(&first))
            .expect("second generation retires");
        assert!(!first.path.exists());
    }

    /// A cancelled replay keeps a shared segment until a restart publishes its final group.
    #[test]
    fn replay_segment_pin_preserves_unread_group_across_restart() {
        let temp_dir = TempDir::new().expect("WAL root");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let seal_key = test_seal_key(crate::test_support::tenant());
        let writer = WalWriter::new(
            temp_dir.path(),
            *node.as_bytes(),
            1,
            WalConfig::new(16 * 1024 * 1024).expect("single-segment config"),
        )
        .expect("first writer");
        let first_batch = [1_u8; 16];
        let shard = crate::scribe::routing::shard_for(
            seal_key.tenant,
            &seal_key.table,
            uuid::Uuid::from_bytes(first_batch),
        );
        let second_batch = (2_u8..=u8::MAX)
            .map(|value| [value; 16])
            .find(|batch_id| {
                crate::scribe::routing::shard_for(
                    seal_key.tenant,
                    &seal_key.table,
                    uuid::Uuid::from_bytes(*batch_id),
                ) == shard
            })
            .expect("a second batch routes to the same fixed shard");
        for batch_id in [first_batch, second_batch] {
            writer
                .append_and_commit_for_replay_test(&seal_key, batch_id, b"audit", b"payload")
                .expect("append replay group");
        }
        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        assert_eq!(reader.segments.len(), 1, "both groups share one segment");
        let segment = reader.segments[0].reference();
        writer
            .close_segments_if_unowned(
                std::slice::from_ref(&segment),
                &std::collections::HashSet::new(),
            )
            .expect("close shared segment");
        writer
            .retain_segments(&[segment.clone(), segment.clone()])
            .expect("retain both generation references");
        let writer = Arc::new(writer);
        let mut commits = 0_usize;
        let cancelled = reader
            .for_each_stream_record_accounted(None, Some(writer.as_ref()), |_, _, accounted| {
                let (_, record, memory) = accounted.into_parts();
                drop(memory);
                if record.is_commit() {
                    commits = commits.saturating_add(1);
                    writer.retire_segments(std::slice::from_ref(&segment))?;
                    return Err(ScribeError::Internal {
                        detail: "injected replay cancellation".to_owned(),
                    });
                }
                Ok(false)
            })
            .expect_err("first group cancellation");
        assert!(cancelled.to_string().contains("cancellation"));
        assert_eq!(commits, 1);
        assert!(segment.path.exists(), "unread group remains durable");

        drop(reader);
        drop(writer);
        let restarted = Arc::new(
            WalWriter::new(
                temp_dir.path(),
                *node.as_bytes(),
                2,
                WalConfig::new(16 * 1024 * 1024).expect("restart config"),
            )
            .expect("restart writer"),
        );
        let current = crate::scribe::stream_identity::StreamIdentity::new(
            node,
            crate::scribe::stream_identity::WriterEpoch::new(2),
        );
        let replay = WalReader::open_directory_for_recovery(temp_dir.path(), current)
            .expect("restart reader");
        let mut restart_commits = 0_usize;
        replay
            .for_each_stream_record_accounted(None, Some(restarted.as_ref()), |_, _, accounted| {
                let (_, record, memory) = accounted.into_parts();
                drop(memory);
                if record.is_commit() {
                    restart_commits = restart_commits.saturating_add(1);
                    if restart_commits == 2 {
                        let wal = restarted.handle_for_shard(shard)?;
                        wal.retain_segments(std::slice::from_ref(&segment))?;
                        wal.retire_segments(std::slice::from_ref(&segment))?;
                        assert!(segment.path.exists(), "reader pin precedes final unlink");
                    }
                }
                Ok(restart_commits == 2)
            })
            .expect("restart replay");
        assert_eq!(
            restart_commits, 2,
            "restart recovers the unread second group"
        );
        assert!(!segment.path.exists(), "final EOF releases the segment pin");
        restarted
            .handle_for_shard(shard)
            .expect("retry handle")
            .retire_segments(std::slice::from_ref(&segment))
            .expect("retirement retry is idempotent");
    }

    /// Fresh replay removes a closed segment whose groups are already watermarked.
    #[test]
    fn replay_segment_pin_retires_fully_watermarked_segment() {
        let temp_dir = TempDir::new().expect("WAL root");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let seal_key = test_seal_key(crate::test_support::tenant());
        let writer = WalWriter::new(
            temp_dir.path(),
            *node.as_bytes(),
            1,
            WalConfig::new(16 * 1024 * 1024).expect("single-segment config"),
        )
        .expect("first writer");
        let first_batch = [3_u8; 16];
        let shard = crate::scribe::routing::shard_for(
            seal_key.tenant,
            &seal_key.table,
            uuid::Uuid::from_bytes(first_batch),
        );
        let second_batch = (4_u8..=u8::MAX)
            .map(|value| [value; 16])
            .find(|batch_id| {
                crate::scribe::routing::shard_for(
                    seal_key.tenant,
                    &seal_key.table,
                    uuid::Uuid::from_bytes(*batch_id),
                ) == shard
            })
            .expect("a second batch routes to the same fixed shard");
        let audit = replay_audit();
        for batch_id in [first_batch, second_batch] {
            writer
                .append_and_commit_for_replay_test(&seal_key, batch_id, &audit, b"payload")
                .expect("append replay group");
        }
        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        assert_eq!(reader.segments.len(), 1, "both groups share one segment");
        let segment = reader.segments[0].reference();
        let records = reader.read_all_records().expect("read records");
        for record in records.iter().filter(|record| record.is_slice()) {
            assert_eq!(
                decode_slice_payload(&record.payload)
                    .expect("slice payload")
                    .seal_key,
                seal_key
            );
        }
        let sealed_lsn = records
            .into_iter()
            .map(|record| record.lsn)
            .max()
            .expect("terminal commit LSN");
        writer
            .close_segments_if_unowned(
                std::slice::from_ref(&segment),
                &std::collections::HashSet::new(),
            )
            .expect("close shared segment");
        let stream = crate::scribe::stream_identity::StreamIdentity::new(
            node,
            crate::scribe::stream_identity::WriterEpoch::new(1),
        );
        let mut manifest = crate::scribe::manifest::Manifest::new(stream);
        manifest.update_sealed_lsn(&seal_key, sealed_lsn);
        crate::scribe::manifest::write_atomic(
            crate::scribe::replay::stream_directory(temp_dir.path(), stream).join("manifest"),
            &manifest,
        )
        .expect("sealed manifest");
        assert_eq!(
            crate::scribe::manifest::read_manifest(
                crate::scribe::replay::stream_directory(temp_dir.path(), stream).join("manifest")
            )
            .expect("manifest read")
            .expect("manifest exists")
            .get_sealed_lsn(&seal_key),
            Some(sealed_lsn)
        );
        assert!(
            segment.path.exists(),
            "crash leaves the sealed segment behind"
        );
        drop(reader);
        drop(writer);

        let restarted = WalWriter::new(
            temp_dir.path(),
            *node.as_bytes(),
            2,
            WalConfig::new(16 * 1024 * 1024).expect("restart config"),
        )
        .expect("restart writer");
        let current = crate::scribe::stream_identity::StreamIdentity::new(
            node,
            crate::scribe::stream_identity::WriterEpoch::new(2),
        );
        crate::scribe::replay::replay_wal_directory_stream_accounted(
            temp_dir.path(),
            Some(current),
            None,
            Some(&restarted),
            None,
            |_| panic!("fully watermarked groups must not be republished"),
        )
        .expect("sealed replay cleanup");
        assert!(
            !segment.path.exists(),
            "safe EOF removes the fully watermarked closed segment"
        );
    }

    /// A filesystem retirement failure preserves the source path for a later retry.
    #[test]
    fn segment_retirement_failure_preserves_retryable_source() {
        let temp_dir = TempDir::new().expect("temp dir");
        let writer =
            WalWriter::new(temp_dir.path(), [13_u8; 16], 1, WalConfig::default()).expect("writer");
        let blocked_path = temp_dir.path().join("closed-segment-directory");
        std::fs::create_dir(&blocked_path).expect("blocked retirement path");
        let segment = WalSegmentRef {
            path: blocked_path.clone(),
        };

        let error = writer
            .retire_segments(&[segment])
            .expect_err("directory cannot be retired as a WAL file");
        assert!(error.to_string().contains("retirement failed"));
        assert!(
            blocked_path.is_dir(),
            "failed retirement must remain retryable"
        );
    }

    #[test]
    fn wal_segment_roll_is_atomic_across_crash() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = [5u8; 16];
        let tenant_id = crate::test_support::tenant();

        let writer =
            WalWriter::new(temp_dir.path(), node_id, 1, WalConfig::default()).expect("writer");

        // Write 2 appends to segment 0.  Use the same batch_id so both writes
        // route to the same shard and land in a single segment.  The WAL does
        // not deduplicate; both records are stored.
        let batch_id = [1u8; 16];
        writer
            .append_and_fsync_for_test(&test_seal_key(tenant_id), batch_id, b"audit1", b"data1")
            .expect("append 1");
        writer
            .append_and_fsync_for_test(&test_seal_key(tenant_id), batch_id, b"audit2", b"data2")
            .expect("append 2");

        // Manually create a .tmp file to simulate crash during segment creation
        let tmp_path = temp_dir.path().join("seg-1.arrow.tmp");
        std::fs::write(&tmp_path, b"incomplete segment data").expect("write tmp file");

        // Reopen directory — should ignore .tmp files
        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        assert_eq!(
            reader.segments.len(),
            1,
            "only complete segment should load"
        );

        // Verify segment 0 reads cleanly
        let records = reader.read_all_records().expect("read records");
        assert_eq!(records.len(), 2);

        // Verify seg-1.arrow does not exist
        let seg1_path = temp_dir.path().join("seg-1.arrow");
        assert!(!seg1_path.exists(), "seg-1.arrow should not exist");
    }

    #[test]
    fn wal_reader_filters_segments_by_stream_identity() {
        let temp_dir = TempDir::new().expect("temp dir");
        let expected_node = [6u8; 16];
        let foreign_node = [7u8; 16];
        let expected_stream = StreamIdentity::new(
            crate::scribe::stream_identity::NodeId::new(uuid::Uuid::from_bytes(expected_node)),
            crate::scribe::stream_identity::WriterEpoch::new(3),
        );

        let expected_writer =
            WalWriter::new(temp_dir.path(), expected_node, 3, WalConfig::default())
                .expect("expected writer");
        expected_writer
            .append_and_fsync_for_test(
                &test_seal_key(crate::test_support::tenant()),
                [1u8; 16],
                b"audit",
                b"data",
            )
            .expect("expected append");

        let foreign_segment_path = temp_dir.path().join("foreign.wal");
        let foreign_segment = WalSegment::create(
            &foreign_segment_path,
            SegmentHeader::new(foreign_node, 9, 0, 0),
        )
        .expect("foreign segment");
        foreign_segment
            .append_and_fsync(&WalRecord::new(
                WalLsn::new(0),
                2,
                [2u8; 16],
                b"foreign".to_vec(),
            ))
            .expect("foreign append");

        let reader =
            WalReader::open_directory(temp_dir.path(), expected_stream).expect("filtered reader");
        assert_eq!(
            reader.read_all_records().expect("filtered records").len(),
            1,
            "the expected stream's audit and data records remain readable"
        );

        let unfiltered =
            WalReader::open_directory_unfiltered(temp_dir.path()).expect("unfiltered reader");
        assert_eq!(
            unfiltered
                .read_all_records()
                .expect("unfiltered records")
                .len(),
            2,
            "the test fixture contains both expected and foreign segments"
        );
    }

    #[test]
    fn wal_io_maps_storage_full_to_507_error() {
        let error = wal_io_error(
            "test WAL write",
            &io::Error::new(io::ErrorKind::StorageFull, "injected full disk"),
        );
        assert!(matches!(error, ScribeError::WalDiskFull));

        let error = wal_io_error("test WAL write", &io::Error::from_raw_os_error(28));
        assert!(matches!(error, ScribeError::WalDiskFull));
    }

    /// Proves the WAL capacity probe agrees with direct bracketing `statvfs` samples.
    ///
    /// The available-space assertion uses the direct samples immediately before
    /// and after the implementation probe so unrelated filesystem activity cannot
    /// impose a false monotonic ordering on the three observations.
    ///
    /// # Panics
    ///
    /// Panics when the temporary filesystem cannot be sampled or when capacity,
    /// block-size, or observed available-space invariants diverge.
    #[test]
    fn statvfs_matches_direct_measurement_without_df() {
        let temp_dir = TempDir::new().expect("temp dir");
        let before = rustix::fs::statvfs(temp_dir.path()).expect("statvfs before probe");
        let measured = filesystem_space(temp_dir.path()).expect("filesystem sample");
        let after = rustix::fs::statvfs(temp_dir.path()).expect("statvfs after probe");

        let before_block_size = if before.f_frsize == 0 {
            before.f_bsize
        } else {
            before.f_frsize
        };
        let after_block_size = if after.f_frsize == 0 {
            after.f_bsize
        } else {
            after.f_frsize
        };
        assert_ne!(before_block_size, 0, "filesystem block size is nonzero");
        assert_eq!(before.f_blocks, after.f_blocks, "total blocks are stable");
        assert_eq!(
            before_block_size, after_block_size,
            "filesystem block size is stable"
        );

        let before_capacity = before.f_blocks.saturating_mul(before_block_size);
        let after_capacity = after.f_blocks.saturating_mul(after_block_size);
        assert_eq!(measured.0, before_capacity);
        assert_eq!(measured.0, after_capacity);

        let before_available = before.f_bavail.saturating_mul(before_block_size);
        let after_available = after.f_bavail.saturating_mul(after_block_size);
        let one_block = before_block_size.max(after_block_size);
        let lower_bound = before_available
            .min(after_available)
            .saturating_sub(one_block);
        let upper_bound = before_available
            .max(after_available)
            .saturating_add(one_block);
        assert!(
            (lower_bound..=upper_bound).contains(&measured.1),
            "measured available bytes {} must fall within bracket {lower_bound}..={upper_bound}",
            measured.1
        );
    }

    #[test]
    fn prepared_encoded_len_matches_actual_for_payload_boundaries() {
        let key = test_seal_key(crate::test_support::tenant());
        for size in [0, 1, 255, 256, 4096] {
            let prepared = PreparedWalAppend::new(
                WalLsn::ZERO,
                [7; 16],
                Bytes::from(vec![1; size]),
                Bytes::from(vec![2; size]),
            )
            .for_slice(key.clone(), [3; 32]);
            let actual = prepared.record().expect("record").encode().len();
            assert_eq!(prepared.encoded_len().expect("encoded len"), actual);
        }
    }

    #[test]
    fn wal_accounting_tracks_nested_state_and_retirement_actual_length() {
        let temp_dir = TempDir::new().expect("temp dir");
        let key = test_seal_key(crate::test_support::tenant());
        let writer =
            WalWriter::new(temp_dir.path(), [21; 16], 1, WalConfig::default()).expect("writer");
        writer
            .append_and_fsync_for_test(&key, [2; 16], b"audit", b"data")
            .expect("append");
        let actual = directory_bytes(temp_dir.path());
        assert_eq!(writer.bytes_on_disk(), actual);
        drop(writer);
        let writer =
            WalWriter::new(temp_dir.path(), [21; 16], 1, WalConfig::default()).expect("reopen");
        assert_eq!(
            writer.bytes_on_disk(),
            actual,
            "startup accounting must include existing nested shard WAL bytes"
        );
        let segment = WalReader::open_directory_unfiltered(temp_dir.path())
            .expect("reader")
            .segments
            .first()
            .expect("segment")
            .reference();
        writer
            .close_segments_if_unowned(std::slice::from_ref(&segment), &HashSet::new())
            .expect("close");
        writer
            .retire_segments(std::slice::from_ref(&segment))
            .expect("retire");
        assert_eq!(writer.bytes_on_disk(), directory_bytes(temp_dir.path()));
    }

    /// Proves production append streams borrowed payloads without frame copies or scans.
    #[test]
    fn append_streams_without_frame_encoding_or_unrelated_wal_walks() {
        let _guard = WAL_FAULT_LOCK.lock().expect("test hook lock");
        let temp_dir = TempDir::new().expect("temp dir");
        let writer =
            WalWriter::new(temp_dir.path(), [22; 16], 1, WalConfig::default()).expect("writer");
        let unrelated_dir = temp_dir.path().join("unrelated");
        std::fs::create_dir_all(&unrelated_dir).expect("fixture dir");
        for index in 0..32 {
            let path = unrelated_dir.join(format!("unrelated-{index}.wal"));
            std::fs::write(path, [0_u8; 8]).expect("fixture");
        }
        let key = test_seal_key(crate::test_support::tenant());
        WAL_ENCODE_COUNT.with(|count| count.set(0));
        WAL_WALK_COUNT.with(|count| count.set(0));
        WAL_COUNT_ACTIVE.store(true, Ordering::Relaxed);
        writer
            .append_and_fsync_for_test(&key, [2; 16], b"audit", b"data")
            .expect("append");
        assert_eq!(WAL_ENCODE_COUNT.with(std::cell::Cell::get), 0);
        assert_eq!(WAL_WALK_COUNT.with(std::cell::Cell::get), 0);
        let _ = writer.disk_pressure();
        assert!(WAL_WALK_COUNT.with(std::cell::Cell::get) > 0);
        WAL_COUNT_ACTIVE.store(false, Ordering::Relaxed);
    }

    #[test]
    fn reconciliation_repairs_under_count_without_shard_state_lock() {
        let _guard = WAL_FAULT_LOCK.lock().expect("test hook lock");
        WAL_PARTIAL_WRITE.with(|flag| flag.set(false));
        let temp_dir = TempDir::new().expect("temp dir");
        let writer =
            WalWriter::new(temp_dir.path(), [23; 16], 1, WalConfig::default()).expect("writer");
        let key = test_seal_key(crate::test_support::tenant());
        writer
            .append_and_fsync_for_test(&key, [3; 16], b"audit", b"data")
            .expect("append");
        let measured = directory_bytes(temp_dir.path());
        writer.disk.accounted_bytes.store(0, Ordering::Release);
        let pressure = writer.disk_pressure();
        assert!(pressure.wal_bytes >= measured);
        assert!(writer.bytes_on_disk() >= measured);
    }

    #[test]
    fn failed_capacity_sample_rejects_before_mutation_then_recovers() {
        let _guard = WAL_FAULT_LOCK.lock().expect("test hook lock");
        let temp_dir = TempDir::new().expect("temp dir");
        let writer =
            WalWriter::new(temp_dir.path(), [24; 16], 1, WalConfig::default()).expect("writer");
        let key = test_seal_key(crate::test_support::tenant());
        let before = writer.bytes_on_disk();
        let files_before = std::fs::read_dir(temp_dir.path())
            .expect("list WAL directory")
            .map(|entry| entry.expect("WAL directory entry").path())
            .collect::<Vec<_>>();
        writer.disk.force_sample(None);
        let error = writer
            .append_and_fsync_for_test(&key, [4; 16], b"audit", b"data")
            .expect_err("forced zero sample must reject");
        assert!(matches!(error, ScribeError::WalDiskFull));
        assert_eq!(writer.bytes_on_disk(), before);
        assert_eq!(directory_bytes(temp_dir.path()), before);
        let files_after = std::fs::read_dir(temp_dir.path())
            .expect("list WAL directory")
            .map(|entry| entry.expect("WAL directory entry").path())
            .collect::<Vec<_>>();
        assert_eq!(files_after, files_before);
        let sample = writer
            .disk
            .sample
            .lock()
            .expect("disk sample lock")
            .expect("failed probe is cached");
        assert_eq!(sample.filesystem_available_bytes, 0);
        writer.disk.force_sample(Some((u64::MAX, u64::MAX)));
        assert!(!writer.disk_pressure().hard);
        writer
            .append_and_fsync_for_test(&key, [5; 16], b"audit", b"data")
            .expect("successful sample recovers");
    }

    #[test]
    fn partial_write_reconciles_conservatively_and_allows_later_append() {
        let _guard = WAL_FAULT_LOCK.lock().expect("test hook lock");
        WAL_PARTIAL_WRITE.with(|flag| flag.set(false));
        let temp_dir = TempDir::new().expect("temp dir");
        let writer =
            WalWriter::new(temp_dir.path(), [25; 16], 1, WalConfig::default()).expect("writer");
        let key = test_seal_key(crate::test_support::tenant());
        WAL_PARTIAL_WRITE.with(|flag| flag.set(true));
        let error = writer
            .append_and_fsync_for_test(&key, [6; 16], b"audit", b"data")
            .expect_err("partial write must fail");
        assert!(matches!(error, ScribeError::Internal { .. }));
        assert!(writer.bytes_on_disk() >= directory_bytes(temp_dir.path()));
        writer
            .append_and_fsync_for_test(&key, [7; 16], b"audit", b"data")
            .expect("state lock and later append remain usable");
    }

    /// Physical append and fsync failures retain errors and exact failed evidence.
    #[test]
    fn wal_injected_failures_emit_exact_failed_attempts_without_fabricated_bytes() {
        let _guard = WAL_FAULT_LOCK.lock().expect("test hook lock");
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let directory = TempDir::new().expect("temporary WAL directory");
            let writer = WalWriter::new(directory.path(), [26; 16], 1, WalConfig::default())
                .expect("writer");
            let key = test_seal_key(crate::test_support::tenant());
            WAL_PARTIAL_WRITE.with(|flag| flag.set(true));
            let append_error = writer
                .append_and_fsync_for_test(&key, [1; 16], b"audit", b"data")
                .expect_err("injected append failure");
            assert!(matches!(append_error, ScribeError::Internal { .. }));
            let after_append_failure = recorder.snapshot();
            assert_eq!(
                after_append_failure
                    .counters
                    .get("bifrost_scribe_wal_append_bytes_total"),
                None
            );
            writer.trip_sync_failure_for_test();
            let sync_error = writer
                .append_and_fsync_for_test(&key, [2; 16], b"audit", b"data")
                .expect_err("injected sync failure");
            assert!(matches!(sync_error, ScribeError::Internal { .. }));
        });
        let snapshot = recorder.snapshot();
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_wal_append_total{outcome=\"failed\"}"),
            Some(&1)
        );
        assert_eq!(
            snapshot
                .counters
                .get("bifrost_scribe_wal_fsync_total{outcome=\"failed\"}"),
            Some(&1)
        );
    }

    /// Root volume accounting commits only fsynced WAL bytes and rolls failures back.
    ///
    /// # Panics
    ///
    /// Panics when the deterministic filesystem fixture or WAL lifecycle fails.
    #[test]
    fn wal_volume_admission_tracks_fsync_rollback_and_retirement_boundaries() {
        let _guard = WAL_FAULT_LOCK.lock().expect("test hook lock");
        let directory = TempDir::new().expect("temporary WAL directory");
        let scribe = directory.path().join("scribe-output-scratch");
        let forge = directory.path().join("forge");
        let oracle = directory.path().join("oracle");
        for path in [&scribe, &forge, &oracle] {
            std::fs::create_dir(path).expect("registered volume root");
        }
        let governor = crate::resources::BifrostVolumeGovernor::register(
            crate::resources::BifrostVolumeRoots {
                wal: directory.path().to_owned(),
                scribe_output_scratch: scribe,
                forge_scratch: forge,
                oracle_scratch: oracle,
            },
            64 * 1024 * 1024,
            crate::resources::BifrostResourceHealth::default(),
        )
        .expect("volume registration");
        let writer = WalWriter::new_with_volume(
            directory.path(),
            [27; 16],
            1,
            WalConfig::default(),
            governor.capabilities().wal,
        )
        .expect("volume-backed writer");
        let key = test_seal_key(crate::test_support::tenant());
        writer
            .append_and_fsync_for_test(&key, [1; 16], b"audit", b"data")
            .expect("durable append");
        let durable = directory_bytes(directory.path());
        assert_eq!(
            governor
                .usage_for_test(crate::resources::BifrostVolumeClass::Wal)
                .expect("committed volume state"),
            (durable, 0, 0)
        );

        WAL_PARTIAL_WRITE.with(|flag| flag.set(true));
        writer
            .append_and_fsync_for_test(&key, [1; 16], b"audit", b"data")
            .expect_err("write failure rolls provisional growth back");
        assert_eq!(directory_bytes(directory.path()), durable);
        assert_eq!(
            governor
                .usage_for_test(crate::resources::BifrostVolumeClass::Wal)
                .expect("write rollback volume state"),
            (durable, 0, 0)
        );

        writer.trip_sync_failure_for_test();
        writer
            .append_and_fsync_for_test(&key, [1; 16], b"audit", b"data")
            .expect_err("sync failure remains provisional");
        let after_failed_sync = directory_bytes(directory.path());
        assert_eq!(
            governor
                .usage_for_test(crate::resources::BifrostVolumeClass::Wal)
                .expect("provisional sync-failure state"),
            (durable, after_failed_sync - durable, 0)
        );
        writer
            .append_and_fsync_for_test(&key, [1; 16], b"audit", b"data")
            .expect("later fsync commits all pending growth");
        let committed = directory_bytes(directory.path());
        assert_eq!(
            governor
                .usage_for_test(crate::resources::BifrostVolumeClass::Wal)
                .expect("recovered volume state"),
            (committed, 0, 0)
        );

        let segment = WalReader::open_directory_unfiltered(directory.path())
            .expect("reader")
            .segments
            .first()
            .expect("segment")
            .reference();
        writer
            .close_segments_if_unowned(std::slice::from_ref(&segment), &HashSet::new())
            .expect("close segment");
        writer
            .retire_segments(std::slice::from_ref(&segment))
            .expect("unlink and directory-fsync retirement");
        assert_eq!(
            governor
                .usage_for_test(crate::resources::BifrostVolumeClass::Wal)
                .expect("retired volume state"),
            (0, 0, 0)
        );
    }

    /// Post-mutation ownership failures retain exact WAL charge and poison health.
    ///
    /// # Panics
    ///
    /// Panics when injected post-mutation failures do not fail closed.
    #[test]
    fn wal_post_mutation_accounting_failures_retain_charge_and_poison() {
        let _guard = WAL_FAULT_LOCK.lock().expect("test hook lock");
        for fail_final_open in [true, false] {
            let directory = TempDir::new().expect("temporary WAL directory");
            let scribe = directory.path().join("scribe-output-scratch");
            let forge = directory.path().join("forge");
            let oracle = directory.path().join("oracle");
            for path in [&scribe, &forge, &oracle] {
                std::fs::create_dir(path).expect("registered volume root");
            }
            let health = crate::resources::BifrostResourceHealth::default();
            let governor = crate::resources::BifrostVolumeGovernor::register(
                crate::resources::BifrostVolumeRoots {
                    wal: directory.path().to_owned(),
                    scribe_output_scratch: scribe,
                    forge_scratch: forge,
                    oracle_scratch: oracle,
                },
                64 * 1024 * 1024,
                health.clone(),
            )
            .expect("volume registration");
            let writer = WalWriter::new_with_volume(
                directory.path(),
                [28; 16],
                1,
                WalConfig::default(),
                governor.capabilities().wal,
            )
            .expect("volume-backed writer");
            if fail_final_open {
                WAL_FAIL_SEGMENT_FINAL_OPEN.with(|flag| flag.set(true));
            } else {
                WAL_FAIL_VOLUME_RETAIN.with(|flag| flag.set(true));
            }
            writer
                .append_and_fsync_for_test(
                    &test_seal_key(crate::test_support::tenant()),
                    [1; 16],
                    b"audit",
                    b"data",
                )
                .expect_err("injected ownership transfer failure");
            let physical = directory_bytes(directory.path());
            let (durable, provisional, scratch) = governor
                .usage_for_test(crate::resources::BifrostVolumeClass::Wal)
                .expect("retained fail-closed state");
            assert_eq!(durable + provisional, physical);
            assert_eq!(scratch, 0);
            assert_eq!(
                health.reason(),
                Some(crate::resources::BifrostResourcePoisonReason::Volume)
            );
            assert!(governor.capabilities().forge.try_acquire(1).is_err());
        }
    }

    /// Torn-tail recovery emits one successful fsync for its one physical repair sync.
    #[test]
    fn recovery_tail_repair_reconciles_physical_and_telemetry_fsync_delta() {
        WAL_RECOVERY_SYNC_COUNT.with(|count| count.set(0));
        let directory = TempDir::new().expect("temporary WAL directory");
        let writer =
            WalWriter::new(directory.path(), [27; 16], 1, WalConfig::default()).expect("writer");
        writer
            .append_and_fsync_for_test(
                &test_seal_key(crate::test_support::tenant()),
                [1; 16],
                b"audit",
                b"data",
            )
            .expect("durable fixture");
        let reader = WalReader::open_directory_unfiltered(directory.path()).expect("reader");
        let path = reader.segments[0].reference().path;
        drop(reader);
        OpenOptions::new()
            .append(true)
            .open(path)
            .expect("open WAL tail")
            .write_all(b"torn")
            .expect("append torn tail");

        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let repaired =
                WalReader::open_directory_unfiltered(directory.path()).expect("repair reader");
            repaired.read_all_records().expect("repair torn tail");
        });
        let physical = WAL_RECOVERY_SYNC_COUNT.with(std::cell::Cell::get);
        assert_eq!(physical, 1);
        assert_eq!(
            recorder
                .snapshot()
                .counters
                .get("bifrost_scribe_wal_fsync_total{outcome=\"success\"}"),
            Some(&physical)
        );
    }
}
