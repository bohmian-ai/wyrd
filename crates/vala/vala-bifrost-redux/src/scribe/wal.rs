//! Write-Ahead Log (WAL) for Scribe — crash-consistent framed records.
//!
//! Every prepared partition slice emits one self-describing `SLICE` record.
//! After
//! all ordered slices are durable, one `COMMIT` record authenticates their
//! complete digest before replay may restore any of them.
//!
//! One process stream carries every shard's records: shards share its
//! segments and its syncs, and each v9 record names its own shard.
//!
//! Segment format per `scribe/00-architecture.md §Crash-consistent WAL format`:
//! - Fixed 64-byte header with magic, version, `node_id`, `writer_epoch`, CRC
//! - Fixed 80-byte v9 framed-record headers (72-byte in v8) plus payload and
//!   CRC32C trailers
//! - `SLICE` records carrying tenant/batch/ordinal identity and payload
//! - `COMMIT` records carrying the closed slice-set digest
//! - Atomic segment rollover: write-fsync-rename-fsync-parent; a retired
//!   segment is recycled into the next one, overwritten in place
//! - Every record header names its segment, so a recycled file's stale
//!   records end the chain like any other bad record
//! - Torn-tail truncation only for incomplete final writes; CRC, structure, and
//!   monotonicity failures in a committed prefix fail closed
//!
//! New segments are version 9. Version 8 segments, one stream per shard, are
//! still read so an upgraded Scribe recovers them; earlier versions are
//! rejected before replay.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bytes::Bytes;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::PhysicalPartition;
use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::resources::{ScribeMemoryLease, ScribeResources};
use crate::scribe::seal_key::SealKey;
use crate::scribe::stream_identity::StreamIdentity;
use crate::tables::AuditLogTable;
use wyrd_spec::vala::ids::TableVersion;

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
static WAL_COUNT_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
thread_local! {
    /// Replay payload allocations observed on this thread.
    ///
    /// Thread-local rather than a process-wide static so a test that asserts an
    /// exact count is not perturbed by replays other tests drive concurrently.
    /// Replay decode is synchronous, so the allocation is always counted on the
    /// thread that requested it.
    static WAL_REPLAY_PAYLOAD_ALLOCATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static WAL_ENCODE_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static WAL_PARTIAL_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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
    /// Format version: 9 for every new segment, 8 for read-only recovery.
    pub version: u16,
    /// Reserved field (must be zero).
    pub reserved: u16,
    /// Stable pod identifier (uuid).
    pub node_id: [u8; 16],
    /// Writer epoch from `vala.cluster_nodes.fencing_token`.
    pub writer_epoch: i64,
    /// Monotonic segment sequence number per stream.
    pub seg_seq: u64,
    /// Shard whose v8 stream owns this segment; always zero in v9, whose
    /// one process stream holds every shard's records.
    pub shard_id: u8,
    /// CRC32C over the preceding 60 bytes.
    pub crc32c: u32,
}

const WAL_MAGIC: u32 = 0x5741_5257; // "WRAW"
/// The WAL version every new segment is written in.
const WAL_VERSION: u16 = 9;
/// The per-shard WAL version still read, never written, so an upgrade
/// recovers it.
const WAL_VERSION_V8: u16 = 8;
const SEGMENT_HEADER_SIZE: usize = 64;
#[cfg(test)]
const APPEND_FRAME_MAGIC_V3: [u8; 4] = *b"SWF3";
/// Fixed byte length of every v9 record header before its payload and CRC.
///
/// The v8 layout followed by the record's shard id at byte 72 and seven
/// zero bytes, all covered by the record CRC.
const RECORD_HEADER_SIZE: usize = 80;
/// `u16` wire encoding of the immutable v9 record-header length.
const RECORD_HEADER_SIZE_U16: u16 = 80;
/// Fixed byte length of every v8 record header, which names no shard.
const RECORD_HEADER_SIZE_V8: usize = 72;
/// Exact v9 record magic.
const RECORD_MAGIC: [u8; 8] = *b"WYRDWAL9";
/// Exact v8 record magic.
const RECORD_MAGIC_V8: [u8; 8] = *b"WYRDWAL8";
/// Exact magic prefixing every v8 COMMIT payload.
const COMMIT_PAYLOAD_MAGIC: [u8; 4] = *b"S7CM";
/// Exact encoded width of a v8 COMMIT payload.
///
/// The layout is fixed and total: magic, WAL digest, logical digest, request
/// identity. A COMMIT record whose declared payload is any other length is
/// refused rather than interpreted, so a record written by a different format
/// can never be read as a truncated or extended identity.
const COMMIT_PAYLOAD_BYTES: usize = COMMIT_PAYLOAD_MAGIC.len() + 32 + 32 + 16;

/// Complete batch identity carried by one terminal v8 COMMIT record.
///
/// A batch has two distinct identities and the terminal record persists both,
/// because each answers a question the other cannot.
///
/// `wal_digest` binds the exact ordered WAL slice frames this attempt wrote. It
/// is what proves a replayed slice set is the one the commit closed, and it is
/// framing-specific: it covers this attempt's exact slice boundaries and
/// managed correlation columns.
///
/// `logical_digest` identifies the rows independently of that framing. It is
/// the value the durable SQL fence stores, so replay must present it verbatim
/// rather than derive something else. Persisting it here is what removes the
/// need for recovery to decode Arrow payloads purely to reconstruct a
/// deduplication key: the live path computes it once, during preprocessing, and
/// replay reads the same durable field.
///
/// `request_id` is the lineage correlation the durable Scribe batch fence
/// stores. It is carried here because replay must reproduce the fence row
/// verbatim without consulting any other durable state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalCommitIdentity {
    /// SHA-256 over this attempt's ordered slice frames.
    pub wal_digest: [u8; 32],
    /// SHA-256 over the batch's attempt-independent logical row identity.
    pub logical_digest: [u8; 32],
    /// Request correlation recorded by the durable batch-commit fence.
    pub request_id: [u8; 16],
}

impl WalCommitIdentity {
    /// Encodes the fixed-width COMMIT payload.
    #[must_use]
    pub fn encode(&self) -> [u8; COMMIT_PAYLOAD_BYTES] {
        let mut encoded = [0_u8; COMMIT_PAYLOAD_BYTES];
        encoded[0..4].copy_from_slice(&COMMIT_PAYLOAD_MAGIC);
        encoded[4..36].copy_from_slice(&self.wal_digest);
        encoded[36..68].copy_from_slice(&self.logical_digest);
        encoded[68..84].copy_from_slice(&self.request_id);
        encoded
    }

    /// Decodes one terminal COMMIT payload.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the payload is not exactly
    /// [`COMMIT_PAYLOAD_BYTES`] long or does not carry the v8 COMMIT magic.
    pub fn decode(payload: &[u8]) -> Result<Self, ScribeError> {
        if payload.len() != COMMIT_PAYLOAD_BYTES {
            return Err(ScribeError::Internal {
                detail: format!(
                    "WAL v8 COMMIT payload is {} bytes, expected {COMMIT_PAYLOAD_BYTES}",
                    payload.len()
                ),
            });
        }
        if payload[0..4] != COMMIT_PAYLOAD_MAGIC {
            return Err(ScribeError::Internal {
                detail: "WAL v8 COMMIT payload magic mismatch".to_owned(),
            });
        }
        let mut wal_digest = [0_u8; 32];
        wal_digest.copy_from_slice(&payload[4..36]);
        let mut logical_digest = [0_u8; 32];
        logical_digest.copy_from_slice(&payload[36..68]);
        let mut request_id = [0_u8; 16];
        request_id.copy_from_slice(&payload[68..84]);
        Ok(Self {
            wal_digest,
            logical_digest,
            request_id,
        })
    }
}

/// A record carrying one ordered batch slice.
const RECORD_FLAG_SLICE: u32 = 1;
/// A record carrying the digest that closes one ordered slice set.
const RECORD_FLAG_COMMIT: u32 = 2;
/// Maximum number of eligible WAL files considered during one V1 recovery.
const WAL_RECOVERY_FILE_LIMIT: usize = 4_096;
/// Retired segment files one shard keeps to recycle into its next segments.
///
/// ponytail: fixed at two, the most a shard's alternating rollover and
/// retirement consume; each costs up to one segment of disk.
const FREE_SEGMENTS_PER_SHARD: usize = 2;

/// Maximum UTF-8 path length accepted for one eligible recovery file.
const WAL_RECOVERY_PATH_LIMIT: usize = 4_096;

impl SegmentHeader {
    /// The low 32 bits of `seg_seq`, written into every record of this segment.
    ///
    /// A recycled file still holds an older segment's records past the new
    /// tail; their tag differs, so replay reads them as bad records.
    #[must_use]
    pub fn record_tag(&self) -> u32 {
        let bytes = self.seg_seq.to_le_bytes();
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    /// Construct a new v9 process-stream segment header.
    #[must_use]
    pub fn new(node_id: [u8; 16], writer_epoch: i64, seg_seq: u64) -> Self {
        Self::with_version(WAL_VERSION, node_id, writer_epoch, seg_seq, 0)
    }

    /// Construct the header of a v8 shard segment, as an older Scribe wrote.
    ///
    /// Only fixtures write v8; production reads it to recover an upgrade.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn new_v8(node_id: [u8; 16], writer_epoch: i64, seg_seq: u64, shard_id: u8) -> Self {
        Self::with_version(WAL_VERSION_V8, node_id, writer_epoch, seg_seq, shard_id)
    }

    /// Builds a header of `version` with its checksum.
    fn with_version(
        version: u16,
        node_id: [u8; 16],
        writer_epoch: i64,
        seg_seq: u64,
        shard_id: u8,
    ) -> Self {
        let mut header = Self {
            magic: WAL_MAGIC,
            version,
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

    /// Byte length of each record header in this segment's version.
    fn record_header_len(&self) -> usize {
        if self.version == WAL_VERSION_V8 {
            RECORD_HEADER_SIZE_V8
        } else {
            RECORD_HEADER_SIZE
        }
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
    /// Returns [`ScribeError::UnsupportedWalVersion`] for a version other
    /// than 8 or 9, or [`ScribeError::Internal`] when the magic, reserved
    /// fields, checksum, or a v9 header naming a shard violates the framing.
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

        if version != WAL_VERSION && version != WAL_VERSION_V8 {
            return Err(ScribeError::UnsupportedWalVersion { version });
        }
        if version == WAL_VERSION && shard_id != 0 {
            return Err(ScribeError::Internal {
                detail: "WAL v9 segment header names a shard".to_owned(),
            });
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

/// WAL record — a slice or commit framed entry.
///
/// Its fixed little-endian header (80 bytes in v9, 72 in v8) is followed by
/// its declared payload and a CRC32C trailer over both. A record never
/// carries a mixed flag set.
///
#[derive(Debug, Clone)]
pub struct WalRecord {
    /// LSN for this record (monotonic per stream).
    pub lsn: WalLsn,
    /// Shard that wrote the record: its v9 header byte, or the v8 segment's
    /// shard, since a v8 segment holds one shard's records.
    pub shard_id: u8,
    /// Exactly one of `RECORD_FLAG_SLICE` or `RECORD_FLAG_COMMIT`.
    pub flags: u32,
    /// Authenticated tenant owning the record.
    pub tenant_id: [u8; 16],
    /// Batch ID for deduplication of the complete slice-set record.
    pub batch_id: [u8; 16],
    /// Zero-based slice ordinal, or the closing slice count for a commit.
    pub slice_index: u32,
    /// Total slices in the batch's ordered slice set.
    pub slice_count: u32,
    /// Self-describing slice payload.
    pub payload: Vec<u8>,
}

/// Validated fixed record header retained while replay selects the next LSN.
///
/// The header contains no scalable payload allocation. Replay can therefore
/// peek across the fixed shard set, choose the stream-wide next record, and
/// allocate only that record's declared payload.
#[derive(Debug)]
struct DecodedWalRecordHeader {
    /// Header bytes included in the frame CRC; a v8 header uses the first
    /// [`RECORD_HEADER_SIZE_V8`].
    encoded: [u8; RECORD_HEADER_SIZE],
    /// Byte length of the header within `encoded`.
    encoded_len: usize,
    /// Stream-wide log sequence number.
    lsn: WalLsn,
    /// Shard that wrote the record.
    shard_id: u8,
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
}

impl WalConfig {
    /// Construct a validated WAL configuration.
    pub fn new(segment_bytes: u64) -> Result<Self, ScribeError> {
        if segment_bytes == 0 {
            return Err(ScribeError::Internal {
                detail: "WAL segment_bytes must be greater than zero".to_owned(),
            });
        }
        Ok(Self { segment_bytes })
    }
}

impl Default for WalConfig {
    fn default() -> Self {
        Self {
            segment_bytes: 512 * 1024 * 1024,
        }
    }
}

/// A day slice prepared without allocating the final WAL frame.
#[derive(Debug)]
pub(crate) struct PreparedWalAppend {
    pub(crate) lsn: WalLsn,
    pub(crate) batch_id: [u8; 16],
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
    /// Terminal identity when this prepared record closes a batch slice set.
    pub(crate) commit_identity: Option<WalCommitIdentity>,
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
    pub(crate) fn new(lsn: WalLsn, batch_id: [u8; 16], data: Bytes) -> Self {
        Self {
            lsn,
            batch_id,
            data,
            seal_key: None,
            schema_fingerprint: [0; 32],
            logical_data_digest: [0; 32],
            logical_data_len: 0,
            shard_id: None,
            slice_index: 0,
            slice_count: 1,
            commit_identity: None,
            commit_tenant: None,
        }
    }

    /// Builds a terminal v8 commit record carrying the batch's complete identity.
    ///
    /// `identity` is computed once on the live path and is the only place the
    /// batch's logical row identity becomes durable in the WAL. Replay reads it
    /// back verbatim rather than reconstructing it from Arrow payloads.
    pub(crate) fn commit(
        batch_id: [u8; 16],
        tenant_id: [u8; 16],
        slice_count: u32,
        identity: WalCommitIdentity,
    ) -> Self {
        Self {
            lsn: WalLsn::ZERO,
            batch_id,
            data: Bytes::new(),
            seal_key: None,
            schema_fingerprint: [0; 32],
            logical_data_digest: [0; 32],
            logical_data_len: 0,
            shard_id: None,
            slice_index: slice_count,
            slice_count,
            commit_identity: Some(identity),
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
    /// self-describing key or a fixed-width WAL-v8 field cannot represent it.
    pub(crate) fn payload_identity(&self) -> Result<ScribeAppendPayloadIdentity, ScribeError> {
        if self.commit_identity.is_some() {
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
        if let Some(identity) = self.commit_identity {
            let tenant_id = self.commit_tenant.ok_or_else(|| ScribeError::Internal {
                detail: "WAL v8 commit is missing its authenticated tenant".to_owned(),
            })?;
            return Ok(WalRecord::commit(
                self.lsn,
                tenant_id,
                self.batch_id,
                self.slice_count,
                identity,
            ));
        }
        let seal_key = self
            .seal_key
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "prepared WAL append is missing its self-describing seal key".to_owned(),
            })?;
        let payload = encode_slice_payload(seal_key, self.schema_fingerprint, &self.data)?;
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

    /// Returns the exact uncompressed v8 packet payload length.
    ///
    /// This includes the self-describing seal-key, schema, and Arrow fields
    /// that are part of the durable packet. Rotation must use this value
    /// rather than a JSON proxy so live projection and replay apply identical
    /// packet semantics.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a self-describing field exceeds
    /// its v8 width or checked packet-length arithmetic overflows.
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
        if self.commit_identity.is_some() {
            return Ok(COMMIT_PAYLOAD_BYTES);
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
                    detail: "WAL table FQN exceeds v8 payload limits".to_owned(),
                })?;
        let data_len = u32::try_from(self.data.len()).map_err(|_| ScribeError::Internal {
            detail: "WAL Arrow payload exceeds v8 payload limits".to_owned(),
        })?;
        let payload_len = 58usize
            .saturating_add(usize::from(table_len))
            .saturating_add(2)
            .saturating_add(seal_key.partition.to_bytes().len())
            .saturating_add(TABLE_VERSION_BYTES)
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
    offset: u64,
) -> Result<([u8; 32], u32), ScribeError> {
    let payload_len =
        u32::try_from(prepared.payload_len()?).map_err(|_| ScribeError::Internal {
            detail: "WAL v8 payload exceeds its u32 record bound".to_owned(),
        })?;
    let (flags, tenant_id) = if prepared.commit_identity.is_some() {
        (
            RECORD_FLAG_COMMIT,
            prepared
                .commit_tenant
                .ok_or_else(|| ScribeError::Internal {
                    detail: "WAL v8 commit is missing its authenticated tenant".to_owned(),
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
    let mut header = encode_record_header(
        prepared.lsn,
        flags,
        tenant_id,
        prepared.batch_id,
        prepared.slice_index,
        prepared.slice_count,
        payload_len,
    );
    header[68..72].copy_from_slice(&segment.header.record_tag().to_le_bytes());
    header[72] = prepared.shard_id.unwrap_or(0);
    let write_parts = |parts: &[&[u8]]| -> Result<[u8; 32], ScribeError> {
        let mut digest = Sha256::new();
        let mut crc = crc32c::crc32c(&header);
        for part in parts {
            digest.update(part);
            crc = crc32c::crc32c_append(crc, part);
        }
        segment.append_borrowed(&header, parts, crc.to_le_bytes(), encoded_len, offset)?;
        Ok(digest.finalize().into())
    };
    let payload_digest = if let Some(identity) = prepared.commit_identity.as_ref() {
        write_parts(&[identity.encode().as_slice()])?
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
                detail: "WAL table FQN exceeds v8 payload limits".to_owned(),
            })?
            .to_le_bytes();
        let partition = seal_key.partition.to_bytes();
        let partition_len = u16::try_from(partition.len())
            .map_err(|_| ScribeError::Internal {
                detail: "WAL partition identity exceeds v8 payload limits".to_owned(),
            })?
            .to_le_bytes();
        let data_len = u32::try_from(prepared.data.len())
            .map_err(|_| ScribeError::Internal {
                detail: "WAL Arrow payload exceeds v8 payload limits".to_owned(),
            })?
            .to_le_bytes();
        let table_version = seal_key.table_version.get().to_be_bytes();
        let parts: [&[u8]; 12] = [
            &SLICE_PAYLOAD_MAGIC,
            &tenant_id,
            &table_len,
            namespace,
            b".",
            name,
            &partition_len,
            &partition,
            &table_version,
            &prepared.schema_fingerprint,
            &data_len,
            &prepared.data,
        ];
        write_parts(&parts)?
    };
    Ok((payload_digest, payload_len))
}

const SLICE_PAYLOAD_MAGIC: [u8; 4] = *b"S9SL";

/// Encoded width of a slice's [`TableVersion`]: one big-endian `i64`.
const TABLE_VERSION_BYTES: usize = 8;

/// Encodes one slice payload for `seal_key`, the test entry to the v8 codec.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when a field exceeds v8 payload limits.
#[cfg(test)]
fn encode_slice_payload(
    seal_key: &SealKey,
    schema_fingerprint: [u8; 32],
    data: &[u8],
) -> Result<Vec<u8>, ScribeError> {
    encode_slice_payload_parts(
        seal_key.tenant.as_uuid().as_bytes(),
        &seal_key.table.fqn(),
        &seal_key.partition,
        seal_key.table_version,
        &schema_fingerprint,
        data,
    )
}

/// Encodes one slice payload from raw parts, so tests can write identities
/// a [`SealKey`] would refuse.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the table FQN, partition or Arrow
/// payload exceeds v8 payload limits.
#[cfg(test)]
fn encode_slice_payload_parts(
    tenant: &[u8; 16],
    table_fqn: &str,
    partition: &PhysicalPartition,
    table_version: TableVersion,
    schema_fingerprint: &[u8; 32],
    data: &[u8],
) -> Result<Vec<u8>, ScribeError> {
    let table_len = u16::try_from(table_fqn.len()).map_err(|_| ScribeError::Internal {
        detail: "WAL table FQN exceeds v8 payload limits".to_owned(),
    })?;
    let data_len = u32::try_from(data.len()).map_err(|_| ScribeError::Internal {
        detail: "WAL Arrow payload exceeds v8 payload limits".to_owned(),
    })?;
    let partition = partition.to_bytes();
    let partition_len = u16::try_from(partition.len()).map_err(|_| ScribeError::Internal {
        detail: "WAL partition identity exceeds v8 payload limits".to_owned(),
    })?;
    let capacity = 4usize
        .saturating_add(16)
        .saturating_add(2)
        .saturating_add(table_fqn.len())
        .saturating_add(2)
        .saturating_add(partition.len())
        .saturating_add(TABLE_VERSION_BYTES)
        .saturating_add(32)
        .saturating_add(4)
        .saturating_add(data.len());
    let mut payload = Vec::with_capacity(capacity);
    payload.extend_from_slice(&SLICE_PAYLOAD_MAGIC);
    payload.extend_from_slice(tenant);
    payload.extend_from_slice(&table_len.to_le_bytes());
    payload.extend_from_slice(table_fqn.as_bytes());
    payload.extend_from_slice(&partition_len.to_le_bytes());
    payload.extend_from_slice(&partition);
    payload.extend_from_slice(&table_version.get().to_be_bytes());
    payload.extend_from_slice(schema_fingerprint);
    payload.extend_from_slice(&data_len.to_le_bytes());
    payload.extend_from_slice(data);
    Ok(payload)
}

/// Decoded metadata and payloads from one v3 WAL slice.
#[derive(Debug, Clone)]
pub(crate) struct DecodedSlicePayload {
    pub(crate) seal_key: SealKey,
    pub(crate) schema_fingerprint: [u8; 32],
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
///
/// `decode_slice_metadata` limits [`DataTenantId::SYSTEM_OWNER`] to the audit log.
///
/// # Errors
/// Returns [`ScribeError::Internal`] when the field is not 16 bytes or names an
/// identity that is not a `UUIDv7`.
fn decode_slice_tenant(bytes: &[u8]) -> Result<DataTenantId, ScribeError> {
    let tenant_bytes: [u8; 16] = bytes.try_into().map_err(|_| ScribeError::Internal {
        detail: "WAL tenant id decode failed".to_owned(),
    })?;
    DataTenantId::try_from(uuid::Uuid::from_bytes(tenant_bytes)).map_err(|error| {
        ScribeError::Internal {
            detail: format!("WAL v3 tenant id is invalid: {error}"),
        }
    })
}

/// Decode and validate the canonical logical table reference.
fn decode_slice_table(table_fqn: &str) -> Result<TableRef, ScribeError> {
    TableRef::parse_fqn(table_fqn).ok_or_else(|| ScribeError::Internal {
        detail: format!("WAL table FQN is invalid: {table_fqn}"),
    })
}

/// Decode and validate the complete partition identity a slice carries.
///
/// A slice whose identity does not decode exactly — an unknown tag, a
/// noncanonical time start, a malformed value — is unrestorable: replay
/// refuses it rather than re-partitioning rows into a partition the writer
/// never chose.
///
/// # Errors
/// Returns [`ScribeError::Internal`] when the identity is malformed.
fn decode_slice_partition(encoded: &[u8]) -> Result<PhysicalPartition, ScribeError> {
    PhysicalPartition::decode(encoded).map_err(|error| ScribeError::Internal {
        detail: format!("WAL v8 slice partition is invalid: {error}"),
    })
}

/// Metadata decoded before the variable-size Arrow field.
struct DecodedSliceMetadata {
    seal_key: SealKey,
    schema_fingerprint: [u8; 32],
}

/// Validate the v8 marker and decode the tenant/table/partition/schema identity.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the magic differs, a field is
/// truncated or malformed, or the system owner names a table it cannot write.
fn decode_slice_metadata(
    reader: &mut SlicePayloadReader<'_>,
) -> Result<DecodedSliceMetadata, ScribeError> {
    if reader.take(SLICE_PAYLOAD_MAGIC.len())? != SLICE_PAYLOAD_MAGIC {
        return Err(ScribeError::Internal {
            detail: "WAL v8 slice magic mismatch".to_owned(),
        });
    }
    let tenant = decode_slice_tenant(reader.take(16)?)?;
    let table_len = reader.read_u16("WAL table length decode failed")?;
    let table_fqn = reader.read_utf8(table_len, "table FQN")?;
    let table = decode_slice_table(&table_fqn)?;
    if tenant == DataTenantId::SYSTEM_OWNER && !AuditLogTable::admits_system_owner(&table) {
        return Err(ScribeError::Internal {
            detail: format!("WAL system owner cannot write `{table_fqn}`"),
        });
    }
    let partition_len = reader.read_u16("WAL partition length decode failed")?;
    let partition = decode_slice_partition(reader.take(partition_len)?)?;
    let table_version = i64::from_be_bytes(
        reader
            .take(TABLE_VERSION_BYTES)?
            .try_into()
            .expect("invariant: 8-byte table version"),
    );
    let table_version =
        TableVersion::new(table_version).map_err(|error| ScribeError::Internal {
            detail: format!("WAL slice table version is invalid: {error}"),
        })?;
    let mut schema_fingerprint = [0_u8; 32];
    schema_fingerprint.copy_from_slice(reader.take(32)?);
    Ok(DecodedSliceMetadata {
        seal_key: SealKey::new(tenant, table, partition).with_table_version(table_version),
        schema_fingerprint,
    })
}

/// Decode the Arrow IPC byte field after metadata.
fn decode_slice_bodies(reader: &mut SlicePayloadReader<'_>) -> Result<Vec<u8>, ScribeError> {
    reader.read_bytes_u32("WAL Arrow length decode failed")
}

/// Decode one self-describing v3 WAL slice payload.
///
/// The payload contains the authenticated tenant, canonical table FQN,
/// partition day, schema fingerprint, and Arrow IPC bytes. Structural decoding
/// is kept separate from replay's deduplication and Arrow reconstruction so
/// this function only validates and returns the one-record representation.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when the payload magic, version,
/// metadata, lengths, or trailing-byte boundary is malformed.
pub(crate) fn decode_slice_payload(payload: &[u8]) -> Result<DecodedSlicePayload, ScribeError> {
    let mut reader = SlicePayloadReader::new(payload);
    let metadata = decode_slice_metadata(&mut reader)?;
    let data = decode_slice_bodies(&mut reader)?;
    reader.finish()?;
    Ok(DecodedSlicePayload {
        seal_key: metadata.seal_key,
        schema_fingerprint: metadata.schema_fingerprint,
        data,
    })
}

impl WalRecord {
    /// Construct a one-slice shard-zero record for a test or low-level caller.
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
            shard_id: 0,
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
        identity: WalCommitIdentity,
    ) -> Self {
        Self {
            lsn,
            shard_id: 0,
            flags: RECORD_FLAG_COMMIT,
            tenant_id,
            batch_id,
            slice_index: slice_count,
            slice_count,
            payload: identity.encode().to_vec(),
        }
    }

    /// Decodes the complete batch identity carried by a terminal record.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when this record is not a COMMIT or
    /// its payload is not a well-formed [`WalCommitIdentity`].
    pub fn commit_identity(&self) -> Result<WalCommitIdentity, ScribeError> {
        if !self.is_commit() {
            return Err(ScribeError::Internal {
                detail: "WAL record is not a terminal COMMIT".to_owned(),
            });
        }
        WalCommitIdentity::decode(&self.payload)
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

    /// Encode the record to bytes (including frame header and CRC) in the
    /// version and segment tag of `segment`.
    ///
    /// A v8 frame has no shard byte: its segment names the shard.
    ///
    /// # Panics
    ///
    /// Panics only if an internal caller violates the configured `u32` WAL
    /// payload bound before this record is encoded.
    #[must_use]
    pub fn encode(&self, segment: &SegmentHeader) -> Vec<u8> {
        let payload_len = u32::try_from(self.payload.len())
            .expect("invariant: WAL payload is bounded by the configured Scribe request ceiling");
        let v8 = segment.version == WAL_VERSION_V8;
        let header_len = segment.record_header_len();
        let mut buf = Vec::with_capacity(header_len + self.payload.len() + 4);
        buf.extend_from_slice(if v8 { &RECORD_MAGIC_V8 } else { &RECORD_MAGIC });
        buf.extend_from_slice(&segment.version.to_le_bytes());
        buf.extend_from_slice(
            &u16::try_from(header_len)
                .expect("record header length fits u16")
                .to_le_bytes(),
        );
        buf.extend_from_slice(&self.flags.to_le_bytes());
        buf.extend_from_slice(&self.lsn.as_u64().to_le_bytes());
        buf.extend_from_slice(&self.tenant_id);
        buf.extend_from_slice(&self.batch_id);
        buf.extend_from_slice(&self.slice_index.to_le_bytes());
        buf.extend_from_slice(&self.slice_count.to_le_bytes());
        buf.extend_from_slice(&payload_len.to_le_bytes());
        buf.extend_from_slice(&segment.record_tag().to_le_bytes());
        if !v8 {
            buf.push(self.shard_id);
            buf.extend_from_slice(&[0; RECORD_HEADER_SIZE - RECORD_HEADER_SIZE_V8 - 1]);
        }
        buf.extend_from_slice(&self.payload);

        // Compute CRC over the complete header followed by payload.
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
    /// Returns [`ScribeError`] for a torn final write, a CRC mismatch, a
    /// version other than `segment`'s, or any invalid header or payload bound.
    ///
    /// # Panics
    ///
    /// Panics only if the fixed-size local header slicing invariant is broken.
    pub fn decode_from<R: Read>(
        reader: &mut R,
        segment: &SegmentHeader,
    ) -> Result<Option<Self>, ScribeError> {
        let Some(header) = Self::decode_header_from(reader, segment)? else {
            return Ok(None);
        };
        Self::decode_payload_from(reader, &header).map(Some)
    }

    /// Reads and validates one fixed record header without allocating payload memory.
    ///
    /// The header layout is `segment`'s version: a record of another version
    /// in it is refused, so one segment never mixes formats. A v8 record takes
    /// its shard from `segment`; a v9 record carries its own.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for a torn header, a version other than
    /// `segment`'s, invalid flags, a segment tag other than `segment`'s,
    /// non-zero v9 reserved bytes, or inconsistent slice bounds.
    ///
    /// # Panics
    ///
    /// Panics only if the fixed-size local header slicing invariant is broken.
    fn decode_header_from<R: Read>(
        reader: &mut R,
        segment: &SegmentHeader,
    ) -> Result<Option<DecodedWalRecordHeader>, ScribeError> {
        let encoded_len = segment.record_header_len();
        let mut header = [0_u8; RECORD_HEADER_SIZE];
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(1) => {
                reader
                    .read_exact(&mut header[1..encoded_len])
                    .map_err(|error| ScribeError::Internal {
                        detail: format!("WAL torn record header: {error}"),
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
        let v8 = segment.version == WAL_VERSION_V8;
        if header[..8] != *(if v8 { &RECORD_MAGIC_V8 } else { &RECORD_MAGIC }) {
            return Err(ScribeError::Internal {
                detail: "invalid WAL record magic".to_owned(),
            });
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != segment.version {
            return Err(ScribeError::Internal {
                detail: "invalid WAL record version for its segment".to_owned(),
            });
        }
        let header_len = u16::from_le_bytes([header[10], header[11]]);
        if usize::from(header_len) != encoded_len {
            return Err(ScribeError::Internal {
                detail: "invalid WAL record header length".to_owned(),
            });
        }
        let flags = u32::from_le_bytes(header[12..16].try_into().expect("fixed record header"));
        if !matches!(flags, RECORD_FLAG_SLICE | RECORD_FLAG_COMMIT) {
            return Err(ScribeError::Internal {
                detail: "invalid WAL record flags".to_owned(),
            });
        }
        let shard_id = if v8 {
            segment.shard_id
        } else {
            if header[73..RECORD_HEADER_SIZE].iter().any(|byte| *byte != 0) {
                return Err(ScribeError::Internal {
                    detail: "invalid WAL record reserved bytes".to_owned(),
                });
            }
            header[72]
        };
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
        let tag = u32::from_le_bytes(header[68..72].try_into().expect("fixed record header"));
        if tag != segment.record_tag() {
            return Err(ScribeError::Internal {
                detail: "invalid WAL record segment tag".to_owned(),
            });
        }
        if slice_count == 0
            || slice_index > slice_count
            || (flags == RECORD_FLAG_SLICE && slice_index >= slice_count)
            || (flags == RECORD_FLAG_COMMIT
                && (slice_index != slice_count
                    || usize::try_from(payload_len).ok() != Some(COMMIT_PAYLOAD_BYTES)))
        {
            return Err(ScribeError::Internal {
                detail: "invalid WAL record bounds".to_owned(),
            });
        }
        Ok(Some(DecodedWalRecordHeader {
            encoded: header,
            encoded_len,
            lsn,
            shard_id,
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
        WAL_REPLAY_PAYLOAD_ALLOCATIONS.with(|count| count.set(count.get() + 1));
        let mut payload = vec![0u8; payload_len];
        reader
            .read_exact(&mut payload)
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL torn record payload: {e}"),
            })?;

        // Read CRC
        let mut crc_buf = [0u8; 4];
        reader
            .read_exact(&mut crc_buf)
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL torn record CRC: {e}"),
            })?;
        let expected_crc = u32::from_le_bytes(crc_buf);

        let computed_crc =
            crc32c::crc32c_append(crc32c_hash(&header.encoded[..header.encoded_len]), &payload);
        if computed_crc != expected_crc {
            return Err(ScribeError::Internal {
                detail: format!(
                    "WAL record CRC mismatch: expected {expected_crc:#x}, got {computed_crc:#x}"
                ),
            });
        }

        Ok(Self {
            lsn: header.lsn,
            shard_id: header.shard_id,
            flags: header.flags,
            tenant_id: header.tenant_id,
            batch_id: header.batch_id,
            slice_index: header.slice_index,
            slice_count: header.slice_count,
            payload,
        })
    }
}

/// Encodes the fixed v9 WAL record header into caller-owned stack storage;
/// the appending segment writes its record tag into bytes 68..72 and the
/// record's shard into byte 72.
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
pub(crate) fn encode_append_frame(batch_id: [u8; 16], data: &[u8]) -> Result<Bytes, ScribeError> {
    let data_len = u32::try_from(data.len()).map_err(|_| ScribeError::Internal {
        detail: "Arrow payload exceeds WAL frame length".to_string(),
    })?;
    let mut frame = Vec::with_capacity(24 + data.len());
    frame.extend_from_slice(&APPEND_FRAME_MAGIC_V3);
    frame.extend_from_slice(&batch_id);
    frame.extend_from_slice(&data_len.to_le_bytes());
    frame.extend_from_slice(data);
    Ok(Bytes::from(frame))
}

#[cfg(test)]
struct DecodedAppendFrame<'a> {
    batch_id: [u8; 16],
    data: &'a [u8],
}

#[cfg(test)]
fn decode_append_frame(frame: &[u8]) -> Result<DecodedAppendFrame<'_>, ScribeError> {
    if frame.len() < 24 || frame[0..4] != APPEND_FRAME_MAGIC_V3 {
        return Err(ScribeError::Internal {
            detail: "invalid Scribe WAL append frame".to_string(),
        });
    }
    let mut batch_id = [0_u8; 16];
    batch_id.copy_from_slice(&frame[4..20]);
    let lengths_start = 20;
    let data_len = u32::from_le_bytes([
        frame[lengths_start],
        frame[lengths_start + 1],
        frame[lengths_start + 2],
        frame[lengths_start + 3],
    ]) as usize;
    let payload_start = lengths_start + 4;
    let end = payload_start
        .checked_add(data_len)
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
        data: &frame[payload_start..end],
    })
}

/// WAL segment — one file in a fixed pod-local shard stream.
#[derive(Debug)]
pub struct WalSegment {
    path: PathBuf,
    file: Mutex<File>,
    header: SegmentHeader,
    /// Whether this file was recycled from a retired segment, so bytes past
    /// the logical end are an older segment's records until [`Self::trim`].
    recycled: bool,
    /// Whether an append wrote bytes since the last successful data sync.
    ///
    /// Read and written only under `file`, so a sync that finds it clear
    /// knows every earlier append, by any shard, is already durable and
    /// skips its own `fdatasync`. This is the WAL's group commit.
    unsynced: std::sync::atomic::AtomicBool,
}

/// Stable filesystem identity for one closed WAL segment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WalSegmentRef {
    /// Segment path retained until manifest publication and grace expiry.
    pub path: PathBuf,
}

/// Test-only WAL sync fault injections shared by every handle of one writer.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
struct WalFaults {
    /// Fails the next group sync before the durable acknowledgment boundary.
    sync_failure: std::sync::atomic::AtomicBool,
    /// Fails the next step after sync and before memtable insertion.
    post_sync_failure: std::sync::atomic::AtomicBool,
    /// Refuses every append as a full device until cleared.
    storage_full: std::sync::atomic::AtomicBool,
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

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| wal_io_error("failed to open WAL segment for append", &error))?;
        file.seek(SeekFrom::End(0))
            .map_err(|error| wal_io_error("failed to position WAL segment for append", &error))?;

        Ok(Self {
            path: path.to_path_buf(),
            file: Mutex::new(file),
            header,
            recycled: false,
            unsynced: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Turns a retired segment file at `free` into the new segment `path`.
    ///
    /// Appends then overwrite blocks the filesystem already allocated, so a
    /// group's `fdatasync` writes data without the inode size or allocation
    /// change that otherwise commits a filesystem journal transaction. The
    /// new header is durable before the rename names the file; until a later
    /// append overwrites them, the old records past the logical end carry
    /// another segment tag and read as bad records.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::WalDiskFull`] or [`ScribeError::Internal`] when
    /// the header write, its sync, the rename, or the directory sync fails.
    pub fn recycle(
        free: impl AsRef<Path>,
        path: impl AsRef<Path>,
        header: SegmentHeader,
    ) -> Result<Self, ScribeError> {
        let path = path.as_ref();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(free.as_ref())
            .map_err(|error| wal_io_error("failed to open recycled WAL segment", &error))?;
        header
            .write_to(&mut file)
            .map_err(|error| wal_io_error("failed to write recycled WAL segment header", &error))?;
        file.sync_data()
            .map_err(|error| wal_io_error("failed to fsync recycled WAL segment header", &error))?;
        std::fs::rename(free.as_ref(), path)
            .map_err(|error| wal_io_error("failed to rename recycled WAL segment", &error))?;
        if let Some(parent) = path.parent() {
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| wal_io_error("failed to fsync WAL segment parent dir", &error))?;
        }
        Ok(Self {
            path: path.to_path_buf(),
            file: Mutex::new(file),
            header,
            recycled: true,
            unsynced: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Cuts a recycled segment's stale bytes off at `logical_len` and syncs it.
    ///
    /// Called before the segment stops being the stream's current one, so
    /// only the chain's last segment can ever end in stale records, where
    /// replay's fence check decides them. A segment created fresh has no
    /// stale bytes and this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the lock is poisoned or the
    /// truncation or its sync fails.
    fn trim(&self, logical_len: u64) -> Result<(), ScribeError> {
        if !self.recycled {
            return Ok(());
        }
        let file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (trim)".to_owned(),
        })?;
        file.set_len(logical_len)
            .map_err(|error| wal_io_error("failed to trim recycled WAL segment", &error))?;
        file.sync_all()
            .map_err(|error| wal_io_error("failed to sync trimmed WAL segment", &error))
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
            recycled: false,
            unsynced: std::sync::atomic::AtomicBool::new(false),
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
            tracing::debug_span!("bifrost.scribe.wal.append", outcome = tracing::field::Empty);
        let _entered = span.enter();
        let encoded = record.encode(&self.header);
        let mut file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (append)".to_string(),
        })?;

        let started = Instant::now();
        self.unsynced.store(true, Ordering::Relaxed);
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

    /// Streams one fixed header and borrowed payload parts at `offset`, the
    /// segment's logical end, without a frame copy.
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
        offset: u64,
    ) -> Result<(), ScribeError> {
        let span =
            tracing::debug_span!("bifrost.scribe.wal.append", outcome = tracing::field::Empty);
        let _entered = span.enter();
        let mut file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (borrowed append)".to_owned(),
        })?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| wal_io_error("WAL record seek failed", &error))?;
        self.unsynced.store(true, Ordering::Relaxed);
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
    ///
    /// A sync that finds no append since the last successful sync returns
    /// without an `fdatasync`: an earlier sync, possibly another shard's,
    /// already made every appended byte durable.
    pub fn sync_data(&self) -> Result<(), ScribeError> {
        let span =
            tracing::debug_span!("bifrost.scribe.wal.fsync", outcome = tracing::field::Empty);
        let _entered = span.enter();
        let file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (sync_data)".to_string(),
        })?;
        if !self.unsynced.load(Ordering::Relaxed) {
            span.record("outcome", "covered");
            return Ok(());
        }
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
        if result.is_ok() {
            self.unsynced.store(false, Ordering::Relaxed);
        }
        span.record("outcome", if result.is_ok() { "success" } else { "failed" });
        result
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
    /// visitor fails, or a committed prefix violates the v8 frame or LSN
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
            match WalRecord::decode_from(&mut *file, &self.header) {
                Ok(Some(record)) => {
                    if previous_lsn.is_some_and(|previous| record.lsn <= previous) {
                        return Err(ScribeError::Internal {
                            detail: "WAL contains non-monotonic committed LSNs".to_owned(),
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
/// The newest acknowledgement fence of one WAL shard stream: the segment
/// and COMMIT LSN of the last batch whose ACK may have been sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NewestFence {
    /// Sequence of the segment holding the fenced COMMIT.
    pub(crate) segment_sequence: u64,
    /// The fenced COMMIT's LSN.
    pub(crate) lsn: WalLsn,
}

/// Recovery's view of the acknowledgement fences: the newest fence of one
/// `(stream, shard)`, or `None` when that stream acknowledged nothing.
///
/// Replay consults it only when a shard chain ends in a bad record, to prove
/// no acknowledged batch lies in the region it discards.
pub(crate) type FenceLookup<'a> =
    &'a dyn Fn(StreamIdentity, u8) -> Result<Option<NewestFence>, ScribeError>;

/// Whether `error` is a complete but invalid record: a bad header or a CRC
/// mismatch. Such a record ends a shard chain when it lies in the
/// unacknowledged tail one group fsync left incomplete; anywhere else it is
/// corruption.
fn is_bad_record_error(error: &ScribeError) -> bool {
    matches!(error, ScribeError::Internal { detail }
        if detail.starts_with("invalid WAL record") || detail.starts_with("WAL record CRC mismatch"))
}

/// A partially written final header, payload, or CRC is a recoverable torn
/// tail. Structural violations and CRC mismatches in a complete frame are
/// durable corruption and must fail-stop rather than silently discard data.
fn is_torn_tail_error(error: &ScribeError) -> bool {
    matches!(error, ScribeError::Internal { detail } if detail.starts_with("WAL torn record"))
}

/// WAL writer — owns the one process stream every shard appends to, with
/// automatic rollover.
///
/// Shards share its segments, so one segment sync makes every shard's
/// appends before it durable. Each shard's active generation holds a
/// retirement reference on every segment it appended to, so retiring one
/// shard's records never removes a segment another shard still needs.
#[derive(Debug)]
pub struct WalWriter {
    base_dir: PathBuf,
    node_id: [u8; 16],
    writer_epoch: i64,
    next_lsn: Arc<AtomicU64>,
    segment_bytes: u64,
    /// The stream's append position and every shard's active generation.
    ///
    /// Lock order: this state, then `retirement_refs`.
    state: Arc<Mutex<WalState>>,
    /// Test-only sync fault injections shared across handles.
    #[cfg(any(test, feature = "test-support"))]
    faults: Arc<WalFaults>,
    retirement_refs: Arc<Mutex<HashMap<PathBuf, usize>>>,
    /// Released segments whose deletion has not yet succeeded.
    ///
    /// A path lands here when it was still the current segment or its removal
    /// failed; every later deletion pass retries it, so releasing the last
    /// reference never loses the only record that the file must be removed.
    deferred_deletes: Arc<Mutex<BTreeSet<PathBuf>>>,
    /// Retired segment files of this stream kept for reuse, by shard directory.
    ///
    /// They exist only while this writer lives: construction deletes every
    /// leftover one, since no replay reads them.
    free_segments: Arc<Mutex<HashMap<PathBuf, Vec<PathBuf>>>>,
    /// Scribe-local fault raised when the WAL no longer matches what the
    /// writer reported; shared by every handle and never reset in process.
    fault: tokio_util::sync::CancellationToken,
}

/// Append position of the process stream plus each shard's generation.
#[derive(Debug)]
struct WalState {
    current_segment: Option<Arc<WalSegment>>,
    seg_seq: u64,
    current_segment_size: u64,
    current_segment_records: u64,
    /// Each shard's active generation, indexed by shard id.
    generations: Vec<ShardGeneration>,
}

impl Default for WalState {
    fn default() -> Self {
        Self {
            current_segment: None,
            seg_seq: 0,
            current_segment_size: 0,
            current_segment_records: 0,
            generations: (0..=u8::MAX).map(|_| ShardGeneration::default()).collect(),
        }
    }
}

/// One shard's share of the stream since its last rotation.
///
/// Rotation thresholds use only this shard's bytes and records, never the
/// shared segment's size. Each listed segment carries one retirement
/// reference owned by this generation, released when rotation hands the
/// segments to the rotated cohort.
#[derive(Debug, Default)]
struct ShardGeneration {
    /// Encoded record bytes this shard appended in the generation.
    bytes: u64,
    /// Records this shard appended in the generation.
    records: u64,
    /// Distinct segments this shard appended to, each retained once.
    segments: Vec<WalSegmentRef>,
}

/// A WAL handle bound to one fixed shard owner of the shared stream.
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
    /// The record joins this shard's active generation, which retains its
    /// segment until rotation.
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
        if append.seal_key.is_none() && append.commit_identity.is_none() {
            return Err(ScribeError::Internal {
                detail: "shard WAL append is missing its self-describing seal key".to_owned(),
            });
        }
        append.shard_id = Some(self.shard_id);
        self.writer.append_record(append, true)
    }

    /// Returns the encoded WAL bytes this shard appended in its generation.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the stream state is poisoned.
    pub(crate) fn current_segment_bytes(&self) -> Result<u64, ScribeError> {
        let state = self.writer.lock_state("current segment bytes")?;
        Ok(state.generations[usize::from(self.shard_id)].bytes)
    }

    /// Returns every segment this shard's active generation appended to.
    ///
    /// These stay retained by the generation until rotation, so a member
    /// sealed out of it must leave them to the rotated cohort.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the stream state is poisoned.
    pub(crate) fn active_generation_segments(&self) -> Result<Vec<WalSegmentRef>, ScribeError> {
        let state = self.writer.lock_state("active generation segments")?;
        Ok(state.generations[usize::from(self.shard_id)]
            .segments
            .clone())
    }

    /// Reports whether the current shard generation contains accepted WAL records.
    ///
    /// This is intentionally separate from byte occupancy: a segment header is
    /// created before the first record, while rotation age applies only after a
    /// non-empty generation exists.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the stream state is poisoned.
    pub(crate) fn has_active_records(&self) -> Result<bool, ScribeError> {
        let state = self.writer.lock_state("active record check")?;
        Ok(state.generations[usize::from(self.shard_id)].records > 0)
    }

    /// Syncs and closes this shard's non-empty active WAL generation.
    ///
    /// The returned segments are every segment the generation appended to,
    /// each still holding the one retirement reference the generation took.
    /// The caller retains them for the rotated cohort and then releases these
    /// generation references. The shared current segment stays open for the
    /// other shards; it is deleted while current only once no shard
    /// references it. An empty generation
    /// returns no segments and creates no retirement owner.
    ///
    /// # Errors
    ///
    /// Returns the stream state-lock or segment-sync error. The generation is
    /// left open when the sync fails.
    pub(crate) fn close_active_generation(&self) -> Result<Vec<WalSegmentRef>, ScribeError> {
        let mut state = self.writer.lock_state("close active generation")?;
        let shard = usize::from(self.shard_id);
        if state.generations[shard].records == 0 {
            return Ok(Vec::new());
        }
        if let Some(segment) = state.current_segment.as_ref()
            && state.generations[shard]
                .segments
                .iter()
                .any(|member| member.path == segment.path())
        {
            segment.sync_data()?;
        }
        Ok(std::mem::take(&mut state.generations[shard]).segments)
    }

    /// Closes an active generation whose every record a sealed member owns.
    ///
    /// Selective seals hand each key's records to an immutable member but
    /// leave the generation open, so once no active bucket remains its
    /// retirement references would outlive every member. This drops them
    /// without deleting anything and returns the segments, which the
    /// committing member then retires once no other owner reaches them.
    ///
    /// # Errors
    ///
    /// Returns the stream state-lock, segment-sync, or retirement-reference
    /// lock error. The generation is left open when the sync fails.
    pub(crate) fn release_sealed_generation(&self) -> Result<Vec<WalSegmentRef>, ScribeError> {
        let closed = self.close_active_generation()?;
        for segment in &closed {
            self.release_replay_pin(segment, false)?;
        }
        Ok(closed)
    }

    pub(crate) fn sync_segments(&self, segments: &[Arc<WalSegment>]) -> Result<(), ScribeError> {
        self.writer.sync_segments_with_fault(segments)
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
}

impl WalWriter {
    /// Restores segment state after one prepared-record write fails.
    ///
    /// # Errors
    ///
    /// Returns the rollback or failed-segment removal error when physical state
    /// cannot be restored to its pre-append boundary; the writer is then
    /// faulted because the WAL no longer matches what it reported.
    fn rollback_prepared_append(
        &self,
        state: &mut WalState,
        segment: &Arc<WalSegment>,
        prior_file_len: u64,
        new_segment_bytes: u64,
    ) -> Result<(), ScribeError> {
        if let Err(error) = segment.rollback_failed_append(prior_file_len) {
            self.mark_faulted(&error);
            return Err(error);
        }
        if new_segment_bytes == 0 {
            return Ok(());
        }
        state.current_segment = None;
        state.current_segment_size = 0;
        state.current_segment_records = 0;
        if let Err(error) = remove_failed_segment(segment.path()) {
            self.mark_faulted(&error);
            return Err(error);
        }
        Ok(())
    }

    /// Faults this Scribe's WAL after an integrity or ambiguous-mutation error.
    ///
    /// The fault is role-local: it refuses every later append before any
    /// acknowledgment, stops segment retirement so WAL files stay for replay,
    /// and signals the owning role to withdraw readiness. It never touches
    /// process health, and only a restart's replay clears it.
    fn mark_faulted(&self, error: &ScribeError) {
        if !self.fault.is_cancelled() {
            tracing::error!(%error, "Scribe WAL faulted; Scribe is unready until restart replay");
            metrics::counter!("bifrost_scribe_wal_fault_total").increment(1);
        }
        self.fault.cancel();
    }

    /// Reports whether this WAL has faulted and refuses further mutation.
    #[must_use]
    pub fn is_faulted(&self) -> bool {
        self.fault.is_cancelled()
    }

    /// Returns the signal cancelled when this WAL faults.
    #[must_use]
    pub fn fault_signal(&self) -> tokio_util::sync::CancellationToken {
        self.fault.clone()
    }

    /// Create a new pod-local WAL writer.
    ///
    /// Segment sizing is explicit and validated before the writer is returned.
    ///
    /// The WAL is not capacity-governed: a full device refuses the append as
    /// [`ScribeError::WalDiskFull`] through the write's own IO error.
    ///
    /// # Errors
    ///
    /// Returns a [`ScribeError`] when `config` is invalid, or when the existing
    /// stream under `base_dir` cannot be listed or read to recover its counters.
    pub fn new(
        base_dir: impl AsRef<Path>,
        node_id: [u8; 16],
        writer_epoch: i64,
        config: WalConfig,
    ) -> Result<Self, ScribeError> {
        let config = WalConfig::new(config.segment_bytes)?;
        let base_dir = base_dir.as_ref().to_path_buf();
        let writer = Self {
            base_dir,
            node_id,
            writer_epoch,
            next_lsn: Arc::new(AtomicU64::new(0)),
            segment_bytes: config.segment_bytes,
            state: Arc::default(),
            #[cfg(any(test, feature = "test-support"))]
            faults: Arc::default(),
            retirement_refs: Arc::new(Mutex::new(HashMap::new())),
            deferred_deletes: Arc::new(Mutex::new(BTreeSet::new())),
            free_segments: Arc::new(Mutex::new(HashMap::new())),
            fault: tokio_util::sync::CancellationToken::new(),
        };
        remove_free_segments(
            &writer
                .base_dir
                .join(Uuid::from_bytes(node_id).simple().to_string()),
        )?;
        writer.initialize_existing_stream()?;
        Ok(writer)
    }

    /// The one directory holding this `(node_id, writer_epoch)` stream's
    /// segments and shard manifests; everything this writer creates lives
    /// beneath it.
    fn stream_dir(&self) -> PathBuf {
        self.base_dir
            .join(Uuid::from_bytes(self.node_id).simple().to_string())
            .join(self.writer_epoch.to_string())
    }

    /// Locks the stream state, naming `operation` if the lock is poisoned.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the state lock is poisoned.
    fn lock_state(
        &self,
        operation: &str,
    ) -> Result<std::sync::MutexGuard<'_, WalState>, ScribeError> {
        self.state.lock().map_err(|_| ScribeError::Internal {
            detail: format!("WAL state lock poisoned ({operation})"),
        })
    }

    /// Recover the next segment and LSN counters for the current stream.
    ///
    /// A restarted writer must never recreate an existing decimal segment
    /// filename or reuse an LSN in the same `(node_id, writer_epoch)` stream.
    /// Only that stream's own directory is read: older epochs remain
    /// replayable but are excluded from the live writer counters, so their
    /// integrity is judged by replay, whose failure is role-local, and never
    /// by writer construction.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the stream directory cannot be listed or
    /// one of its segments cannot be opened or read.
    fn initialize_existing_stream(&self) -> Result<(), ScribeError> {
        let mut paths = Vec::new();
        let stream_dir = self.stream_dir();
        if stream_dir.exists() {
            collect_segment_paths(&stream_dir, &mut paths)?;
        }
        let mut next_lsn = 0_u64;
        for path in paths {
            let segment = WalSegment::open(&path)?;
            let header = segment.header();
            if header.node_id != self.node_id || header.writer_epoch != self.writer_epoch {
                continue;
            }
            let mut state = self.lock_state("stream recovery")?;
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

    /// Makes every append fail as a full device, or restores normal appends.
    ///
    /// The refusal happens before any WAL mutation, exactly where a real
    /// `ENOSPC` from creating a segment would surface.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_device_full_for_test(&self, full: bool) {
        self.faults.storage_full.store(full, Ordering::Release);
    }

    /// Inject one WAL `sync_data` failure after the record write and before
    /// the durable acknowledgment boundary.
    #[cfg(any(test, feature = "test-support"))]
    pub fn trip_sync_failure_for_test(&self) {
        self.faults.sync_failure.store(true, Ordering::Release);
    }

    /// Inject one failure after WAL sync and before memtable insertion.
    #[cfg(any(test, feature = "test-support"))]
    pub fn trip_post_sync_failure_for_test(&self) {
        self.faults.post_sync_failure.store(true, Ordering::Release);
    }

    /// Return the single WAL handle owned by one fixed shard task.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when `shard_id` does not fit the WAL
    /// segment header's one-byte shard id.
    pub(crate) fn handle_for_shard(&self, shard_id: usize) -> Result<WalHandle, ScribeError> {
        let shard_id = u8::try_from(shard_id).map_err(|_| ScribeError::Internal {
            detail: format!("invalid WAL shard index: {shard_id}"),
        })?;
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
            next_lsn: Arc::clone(&self.next_lsn),
            segment_bytes: self.segment_bytes,
            state: Arc::clone(&self.state),
            #[cfg(any(test, feature = "test-support"))]
            faults: Arc::clone(&self.faults),
            retirement_refs: Arc::clone(&self.retirement_refs),
            deferred_deletes: Arc::clone(&self.deferred_deletes),
            free_segments: Arc::clone(&self.free_segments),
            fault: self.fault.clone(),
        }
    }

    /// Append one self-describing slice record for test-tier probes.
    #[cfg(any(test, feature = "test-support"))]
    pub fn append_and_fsync_for_test(
        &self,
        seal_key: &SealKey,
        batch_id: [u8; 16],
        data_payload: &[u8],
    ) -> Result<WalLsn, ScribeError> {
        self.append_and_fsync_for_key(seal_key, batch_id, data_payload)
    }

    /// Append, commit, and fsync one one-slice batch for replay tests.
    ///
    /// This test-only helper deliberately models the production `SLICE` then
    /// `COMMIT` ordering, including joining the routed shard's active
    /// generation, without changing lower-level WAL tests that need to
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
        data_payload: &[u8],
    ) -> Result<WalLsn, ScribeError> {
        let mut prepared =
            PreparedWalAppend::new(WalLsn::ZERO, batch_id, Bytes::copy_from_slice(data_payload))
                .for_slice(seal_key.clone(), [0; 32]);
        // Framing tests write opaque payloads, so the logical identity is taken
        // over the payload bytes themselves. The production path substitutes the
        // Arrow logical identity here; the WAL format is indifferent to which,
        // which is the property these tests exercise.
        let logical_data_len =
            u32::try_from(data_payload.len()).map_err(|_| ScribeError::Internal {
                detail: "test WAL payload exceeds its u32 logical length bound".to_owned(),
            })?;
        prepared = prepared
            .with_logical_data_identity(Sha256::digest(data_payload).into(), logical_data_len);
        prepared.shard_id = Some(
            u8::try_from(crate::scribe::routing::shard_for(
                seal_key.tenant,
                &seal_key.table,
                uuid::Uuid::from_bytes(batch_id),
                crate::scribe::routing::TEST_SHARD_COUNT,
            ))
            .expect("fixed shard count fits in u8"),
        );
        let shard_id = prepared.shard_id;
        let mut logical = crate::scribe::preprocess::LogicalBatchDigest::new();
        logical.push_slice(
            0,
            &prepared.schema_fingerprint,
            &prepared.logical_data_digest,
            prepared.logical_data_len,
        );
        let logical_digest = logical.finish();
        let slice = self.append_record(prepared, true)?;
        let mut digest = Sha256::new();
        digest.update(0_u32.to_le_bytes());
        digest.update(slice.payload_len.to_le_bytes());
        digest.update(slice.payload_digest);
        let mut commit = PreparedWalAppend::commit(
            batch_id,
            *seal_key.tenant.as_uuid().as_bytes(),
            1,
            WalCommitIdentity {
                wal_digest: digest.finalize().into(),
                logical_digest,
                request_id: *uuid::Uuid::now_v7().as_bytes(),
            },
        );
        commit.shard_id = shard_id;
        let terminal = self.append_record(commit, true)?;
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
        self.append_and_commit_for_replay_test(seal_key, decoded.batch_id, decoded.data)
    }

    /// Append one prepared v9 record without syncing it, outside any shard
    /// generation.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for invalid record identity or length,
    /// poisoned writer state, WAL capacity/disk refusal, segment creation or
    /// append failure, or rollback/accounting divergence.
    #[cfg(any(test, feature = "test-support", feature = "bench-support"))]
    pub(crate) fn append_prepared(
        &self,
        prepared: PreparedWalAppend,
    ) -> Result<WalAppendResult, ScribeError> {
        self.append_record(prepared, false)
    }

    /// Appends one prepared v9 record to the shared stream without syncing it.
    ///
    /// The record carries `prepared.shard_id`. With `in_generation`, it also
    /// joins that shard's active generation: the shard's bytes and records
    /// grow, and the first record of the generation in a segment retains
    /// that segment and creates the shard's manifest directory.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for invalid record identity or length,
    /// poisoned writer state, WAL capacity/disk refusal, segment creation or
    /// append failure, rollback/accounting divergence, or a failed
    /// generation retention or manifest directory, which faults the writer.
    fn append_record(
        &self,
        mut prepared: PreparedWalAppend,
        in_generation: bool,
    ) -> Result<WalAppendResult, ScribeError> {
        if self.is_faulted() {
            return Err(ScribeError::IngressClosed);
        }
        let encoded_bytes =
            u64::try_from(prepared.encoded_len()?).map_err(|_| ScribeError::Internal {
                detail: "encoded WAL record length does not fit accounting".to_owned(),
            })?;
        let shard_id = prepared.shard_id.unwrap_or(0);
        let mut state = self.lock_state("append")?;
        let rolls_segment = state.current_segment.is_some()
            && state.current_segment_records > 0
            && state.current_segment_size.saturating_add(encoded_bytes) > self.segment_bytes();
        let new_segment_bytes = if state.current_segment.is_none() || rolls_segment {
            SEGMENT_HEADER_SIZE as u64
        } else {
            0
        };
        #[cfg(any(test, feature = "test-support"))]
        if self.faults.storage_full.load(Ordering::Acquire) {
            return Err(ScribeError::WalDiskFull);
        }
        let lsn = WalLsn::new(self.next_lsn.fetch_add(1, Ordering::SeqCst));
        prepared.assign_lsn(lsn);
        if rolls_segment {
            if let Some(segment) = &state.current_segment {
                segment.trim(state.current_segment_size)?;
            }
            state.current_segment = None;
            state.current_segment_size = 0;
            state.current_segment_records = 0;
        }
        let segment = self.ensure_segment_locked(&mut state)?;
        // The logical end, not the file length: a recycled file is longer.
        let prior_file_len = state.current_segment_size;
        let encoded_len = usize::try_from(encoded_bytes).map_err(|_| ScribeError::Internal {
            detail: "encoded WAL record length does not fit memory addressing".to_owned(),
        })?;
        let append_result =
            append_prepared_borrowed(&segment, &prepared, encoded_len, prior_file_len);
        let (payload_digest, payload_len) = match append_result {
            Ok(identity) => identity,
            Err(error) => {
                self.rollback_prepared_append(
                    &mut state,
                    &segment,
                    prior_file_len,
                    new_segment_bytes,
                )?;
                return Err(error);
            }
        };
        state.current_segment_size = state.current_segment_size.saturating_add(encoded_bytes);
        state.current_segment_records = state.current_segment_records.saturating_add(1);
        if in_generation {
            self.join_generation(&mut state, shard_id, &segment, encoded_bytes)
                .inspect_err(|error| self.mark_faulted(error))?;
        }
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

    /// Syncs appended segments, faulting the WAL when durability is unknown.
    ///
    /// A failed sync leaves written record bytes whose durability the writer
    /// cannot state, so the WAL is faulted rather than retried in process.
    /// A faulted writer refuses every later sync: a retried `fdatasync` can
    /// report success for pages the failed one already dropped, so no shard
    /// may treat a sync after the fault as proof of durability.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngressClosed`] once the writer is faulted, the
    /// segment sync error, or the injected sync failure.
    fn sync_segments_with_fault(&self, segments: &[Arc<WalSegment>]) -> Result<(), ScribeError> {
        if self.is_faulted() {
            return Err(ScribeError::IngressClosed);
        }
        #[cfg(any(test, feature = "test-support"))]
        if self.faults.sync_failure.swap(false, Ordering::AcqRel) {
            let result = Err(ScribeError::Internal {
                detail: "injected WAL sync failure".to_owned(),
            });
            record_wal_fsync(&result, Instant::now());
            if let Err(error) = &result {
                self.mark_faulted(error);
            }
            return result;
        }
        Self::sync_segments(segments).inspect_err(|error| self.mark_faulted(error))
    }

    /// Return the configured segment size.
    #[must_use]
    pub fn segment_bytes(&self) -> u64 {
        self.segment_bytes
    }

    /// Adds one just-appended record to `shard_id`'s active generation.
    ///
    /// The generation's first record in `segment` retains it, so no other
    /// shard's retirement can remove it, and creates the shard's manifest
    /// directory beside the stream's segments.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the retirement-reference lock
    /// is poisoned or the shard directory cannot be created.
    fn join_generation(
        &self,
        state: &mut WalState,
        shard_id: u8,
        segment: &WalSegment,
        encoded_bytes: u64,
    ) -> Result<(), ScribeError> {
        let generation = &mut state.generations[usize::from(shard_id)];
        if !generation
            .segments
            .iter()
            .any(|member| member.path == segment.path())
        {
            std::fs::create_dir_all(self.stream_dir().join(format!("shard-{shard_id:02}")))
                .map_err(|error| ScribeError::Internal {
                    detail: format!("failed to create WAL shard directory: {error}"),
                })?;
            self.add_references(std::slice::from_ref(&segment.reference()))?;
            generation.segments.push(segment.reference());
        }
        generation.bytes = generation.bytes.saturating_add(encoded_bytes);
        generation.records = generation.records.saturating_add(1);
        Ok(())
    }

    /// Return the number of currently open WAL streams: one or none.
    #[must_use]
    pub fn open_stream_count(&self) -> usize {
        self.state
            .lock()
            .map_or(0, |state| usize::from(state.current_segment.is_some()))
    }

    /// Close the active segment after Scribe has stopped WAL IO.
    ///
    /// Shutdown retains segment files for normal replay but must release the
    /// in-memory active-stream owner. A later process recovery, rather than the
    /// stopped Scribe instance, decides which segment to reopen.
    ///
    /// # Errors
    /// Returns [`ScribeError::Internal`] when the WAL-state lock is poisoned
    /// or a recycled segment cannot be trimmed.
    pub(crate) fn close_all_streams(&self) -> Result<(), ScribeError> {
        let mut state = self.lock_state("shutdown close")?;
        if let Some(segment) = &state.current_segment {
            segment.trim(state.current_segment_size)?;
        }
        state.current_segment = None;
        state.current_segment_size = 0;
        state.current_segment_records = 0;
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
            Bytes::copy_from_slice(decoded.data),
        )
        .for_slice(seal_key.clone(), [0; 32]);
        // Use the decoded batch_id for routing, matching the production path.
        prepared.shard_id = Some(
            u8::try_from(crate::scribe::routing::shard_for(
                seal_key.tenant,
                &seal_key.table,
                uuid::Uuid::from_bytes(decoded.batch_id),
                crate::scribe::routing::TEST_SHARD_COUNT,
            ))
            .expect("fixed shard count fits in u8"),
        );
        Ok(self.append_prepared(prepared)?.lsn)
    }

    /// Sync the stream's current WAL segment to disk.
    ///
    /// Every shard shares the segment, so the seal key does not choose it.
    ///
    /// # Errors
    /// Returns [`ScribeError`] when the sync fails.
    #[cfg(test)]
    pub(crate) fn sync_data_for_test(&self, _seal_key: &SealKey) -> Result<(), ScribeError> {
        self.sync_current_segment()
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
        data_payload: &[u8],
    ) -> Result<WalLsn, ScribeError> {
        let mut prepared =
            PreparedWalAppend::new(WalLsn::ZERO, batch_id, Bytes::copy_from_slice(data_payload))
                .for_slice(seal_key.clone(), [0; 32]);
        // Route using the batch_id to match the production dispatch path.
        prepared.shard_id = Some(
            u8::try_from(crate::scribe::routing::shard_for(
                seal_key.tenant,
                &seal_key.table,
                uuid::Uuid::from_bytes(batch_id),
                crate::scribe::routing::TEST_SHARD_COUNT,
            ))
            .expect("fixed shard count fits in u8"),
        );
        let result = self.append_prepared(prepared)?;
        self.sync_segments_with_fault(&result.touched_segments)?;
        Ok(result.lsn)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn take_post_sync_failure_for_test(&self) -> bool {
        self.faults.post_sync_failure.swap(false, Ordering::AcqRel)
    }

    #[cfg(feature = "bench-support")]
    pub(crate) fn sync_data(&self) -> Result<(), ScribeError> {
        self.sync_current_segment()
    }

    /// Syncs the stream's current segment, if any.
    ///
    /// # Errors
    ///
    /// Returns the state-lock or segment-sync error.
    #[cfg(any(test, feature = "bench-support"))]
    fn sync_current_segment(&self) -> Result<(), ScribeError> {
        let segment = self.lock_state("sync_data")?.current_segment.clone();
        if let Some(segment) = segment {
            segment.sync_data()?;
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
        self.add_references(segments)
    }

    /// Adds one retirement reference to each of `segments`.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the retirement-reference lock is
    /// poisoned.
    fn add_references(&self, segments: &[WalSegmentRef]) -> Result<(), ScribeError> {
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

    /// Deletes released segments plus every earlier deferred deletion.
    ///
    /// The released paths join the shared deferred set before any file is
    /// touched. A released current segment that no owner references is closed
    /// first, so a quiet stream retires without waiting for a rollover. Each
    /// pass then removes every deferred path that is no longer the current
    /// segment and that no owner has retained again since its release; a
    /// re-retained path or a failed removal stays deferred for the next pass,
    /// so partial progress never loses a pending deletion.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a state or deferral lock is
    /// poisoned, the closing segment cannot be trimmed or synced, or the
    /// first removal or directory-sync failure after every other deferred
    /// path has been attempted.
    ///
    /// Lock order: stream state, then retirement references.
    fn delete_closed_segments(&self, segments: &[PathBuf]) -> Result<(), ScribeError> {
        if self.is_faulted() {
            // A faulted WAL keeps every file for the restart replay that
            // decides what it holds; the paths stay deferred, not forgotten.
            self.deferred_deletes
                .lock()
                .map_err(|_| ScribeError::Internal {
                    detail: "WAL deferred deletion lock poisoned".to_owned(),
                })?
                .extend(segments.iter().cloned());
            return Ok(());
        }
        let pending = {
            let mut deferred = self
                .deferred_deletes
                .lock()
                .map_err(|_| ScribeError::Internal {
                    detail: "WAL deferred deletion lock poisoned".to_owned(),
                })?;
            deferred.extend(segments.iter().cloned());
            deferred.iter().cloned().collect::<Vec<_>>()
        };
        let mut state = self.lock_state("retire_segments")?;
        let retained = self
            .retirement_refs
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "WAL retirement reference lock poisoned".to_owned(),
            })?
            .keys()
            .filter(|path| pending.contains(path))
            .cloned()
            .collect::<HashSet<_>>();
        // A released current segment nobody references holds no record any
        // owner still needs: every generation that appended to it took a
        // reference under this lock. Closing it here lets a low-volume stream
        // retire without waiting for a size rollover; the next append opens
        // a fresh segment.
        if let Some(segment) = state.current_segment.as_ref()
            && pending.iter().any(|path| path == segment.path())
            && !retained.contains(segment.path())
        {
            segment.trim(state.current_segment_size)?;
            segment.sync_data()?;
            state.current_segment = None;
            state.current_segment_size = 0;
            state.current_segment_records = 0;
        }
        let current = state
            .current_segment
            .as_ref()
            .map(|segment| segment.path().to_path_buf());
        drop(state);
        let mut first_error = None;
        for path in pending {
            if current.as_ref() == Some(&path) || retained.contains(&path) {
                continue;
            }
            match self.retire_segment_file(&path) {
                Ok(()) => {
                    self.deferred_deletes
                        .lock()
                        .map_err(|_| ScribeError::Internal {
                            detail: "WAL deferred deletion lock poisoned".to_owned(),
                        })?
                        .remove(&path);
                }
                Err(error) => {
                    tracing::warn!(path = %path.display(), error = %error, "WAL segment deletion deferred for retry");
                    first_error.get_or_insert(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Keeps one closed segment of this stream for reuse, or deletes it.
    ///
    /// A directory keeps at most [`FREE_SEGMENTS_PER_SHARD`] files; a segment of
    /// an older stream, which this writer never appends to, is deleted. A
    /// segment already gone was retired by a concurrent caller, which is
    /// success.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the free-segment lock is
    /// poisoned, or the rename, removal, or directory sync fails.
    fn retire_segment_file(&self, path: &Path) -> Result<(), ScribeError> {
        if let Some(directory) = path.parent()
            && directory.starts_with(self.stream_dir())
            && path.exists()
        {
            let mut free = self
                .free_segments
                .lock()
                .map_err(|_| ScribeError::Internal {
                    detail: "WAL free segment lock poisoned".to_owned(),
                })?;
            let pool = free.entry(directory.to_path_buf()).or_default();
            if pool.len() < FREE_SEGMENTS_PER_SHARD {
                let target = path.with_extension("free");
                match std::fs::rename(path, &target) {
                    // Another retirement of the same segment won the race.
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                    renamed => renamed
                        .and_then(|()| File::open(directory)?.sync_all())
                        .map_err(|error| ScribeError::Internal {
                            detail: format!("WAL segment recycling failed: {error}"),
                        })?,
                }
                pool.push(target);
                return Ok(());
            }
        }
        Self::delete_segment_file(path)
    }

    /// Removes one closed segment file and syncs its parent directory.
    ///
    /// A missing file is already deleted; after a sync failure the next pass
    /// finds the file gone and clears its deferral.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the file cannot be removed or
    /// its directory cannot be synced.
    fn delete_segment_file(path: &Path) -> Result<(), ScribeError> {
        match std::fs::remove_file(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            removed => removed.map_err(|error| ScribeError::Internal {
                detail: format!("WAL segment retirement failed: {error}"),
            })?,
        }
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
        Ok(())
    }

    /// Returns the current stream segment or creates its next ordered segment.
    ///
    /// v9 segments live in the `shard-00` directory under the v8 naming, so
    /// a v8 Scribe discovers them and refuses their version instead of
    /// overlooking live data. The location assigns no record to shard 0.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the segment directory or segment
    /// cannot be created, or propagates segment-header persistence failures.
    fn ensure_segment_locked(&self, state: &mut WalState) -> Result<Arc<WalSegment>, ScribeError> {
        if let Some(ref segment) = state.current_segment {
            return Ok(Arc::clone(segment));
        }

        let seq = state.seg_seq;
        state.seg_seq = state.seg_seq.saturating_add(1);
        let shard_dir = self.stream_dir().join("shard-00");
        std::fs::create_dir_all(&shard_dir).map_err(|error| ScribeError::Internal {
            detail: format!("failed to create WAL shard directory: {error}"),
        })?;
        let path = shard_dir.join(format!("{seq}.wal"));
        let header = SegmentHeader::new(self.node_id, self.writer_epoch, seq);
        let free = self
            .free_segments
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "WAL free segment lock poisoned".to_owned(),
            })?
            .get_mut(&shard_dir)
            .and_then(Vec::pop);

        let segment = Arc::new(match free {
            Some(free) => WalSegment::recycle(free, &path, header)?,
            None => WalSegment::create(&path, header)?,
        });
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
    /// Header of the current segment, whose version and record tag every
    /// record in it must carry; a record with another tag is stale.
    segment_header: Option<SegmentHeader>,
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
            segment_header: None,
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
    fn peek_lsn(&mut self, fences: Option<FenceLookup<'_>>) -> Result<Option<WalLsn>, ScribeError> {
        self.ensure_header(fences)?;
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
        fences: Option<FenceLookup<'_>>,
    ) -> Result<Option<(PathBuf, WalRecord, Option<ScribeMemoryLease>)>, ScribeError> {
        self.ensure_header(fences)?;
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
            Err(error) if is_bad_record_error(&error) => {
                self.end_at_bad_record(error, fences)?;
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        if self
            .previous_lsn
            .is_some_and(|previous| record.lsn <= previous)
        {
            return Err(ScribeError::Internal {
                detail: "WAL contains non-monotonic committed LSNs across ordered segments"
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
    fn ensure_header(&mut self, fences: Option<FenceLookup<'_>>) -> Result<(), ScribeError> {
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
            let segment = self
                .segment_header
                .as_ref()
                .ok_or_else(|| ScribeError::Internal {
                    detail: "WAL replay cursor lost its segment header".to_owned(),
                })?;
            match WalRecord::decode_header_from(file, segment) {
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
                Err(error) if is_bad_record_error(&error) => {
                    self.end_at_bad_record(error, fences)?;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Ends this shard chain at the complete but invalid record at
    /// `record_offset` when it lies in the unacknowledged tail.
    ///
    /// One fsync makes a group's SLICE and COMMIT records durable together,
    /// so a crash during it can leave any of the group's blocks unwritten. No
    /// batch of that group was acknowledged: its fence commits only after the
    /// fsync. The record is in that tail when it is in the chain's last
    /// segment and the stream's newest fence lies before it; the chain is then
    /// truncated there exactly like a torn tail.
    ///
    /// # Errors
    ///
    /// Returns `error` when no fence authority is supplied or a later segment
    /// follows, an error naming the fence when an acknowledged COMMIT lies at
    /// or beyond the record, or the fence lookup or truncation failure.
    fn end_at_bad_record(
        &mut self,
        error: ScribeError,
        fences: Option<FenceLookup<'_>>,
    ) -> Result<(), ScribeError> {
        let Some(fences) = fences else {
            return Err(error);
        };
        let Some(segment) = self
            .segments
            .last()
            .filter(|_| self.next_segment == self.segments.len())
        else {
            return Err(error);
        };
        let header = segment.header();
        let (seg_seq, shard_id) = (header.seg_seq, header.shard_id);
        let stream = StreamIdentity::new(
            crate::scribe::stream_identity::NodeId::new(Uuid::from_bytes(header.node_id)),
            crate::scribe::stream_identity::WriterEpoch::new(header.writer_epoch),
        );
        let acknowledged = fences(stream, shard_id)?.filter(|fence| {
            fence.segment_sequence >= seg_seq
                && self.previous_lsn.is_none_or(|last| fence.lsn > last)
        });
        if let Some(fence) = acknowledged {
            return Err(ScribeError::Internal {
                detail: format!(
                    "WAL bad record precedes acknowledged COMMIT LSN {} in segment {}: {error}",
                    fence.lsn, fence.segment_sequence
                ),
            });
        }
        tracing::warn!(
            segment = ?self.path,
            offset = self.record_offset,
            last_lsn = ?self.previous_lsn,
            %error,
            "WAL recovery ended a shard chain at its first bad record, in the unacknowledged tail"
        );
        self.repair_torn_tail()
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
        self.segment_header = Some(segment.header().clone());
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
                detail: "WAL contains a torn record before a later segment".to_owned(),
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
    /// Every caller lives in this module's own test suite: production recovery
    /// reads through the accounted replay visitor, which never materializes a
    /// whole WAL. The `cfg(test)` gate keeps the unbounded collector out of any
    /// build a server or SDK can link.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when segment framing, CRC, ordering, identity,
    /// payload decoding, or torn-tail recovery validation fails.
    #[cfg(test)]
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
    #[cfg(test)]
    pub(crate) fn for_each_stream_record<F>(&self, visit: F) -> Result<(), ScribeError>
    where
        F: FnMut(StreamIdentity, u8, PathBuf, WalRecord) -> Result<(), ScribeError>,
    {
        let mut visit = visit;
        self.for_each_stream_record_accounted(None, None, None, |stream, shard, accounted| {
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
        fences: Option<FenceLookup<'_>>,
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
                    let Some(lsn) = cursor.peek_lsn(fences)? else {
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
                let Some((path, record, payload_memory)) = cursor.take_record(governor, fences)?
                else {
                    continue;
                };
                if previous_lsn.is_some_and(|previous| record.lsn <= previous) {
                    return Err(ScribeError::Internal {
                        detail: "WAL contains non-monotonic stream-wide committed LSNs".to_owned(),
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

/// Deletes every retired segment file kept for reuse under `dir`.
///
/// A `.free` file is never replayed, so a writer that did not create it
/// removes it rather than reuse a file it cannot account for.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when a directory cannot be read or a
/// file cannot be removed.
fn remove_free_segments(dir: &Path) -> Result<(), ScribeError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(ScribeError::Internal {
                detail: format!("failed to read WAL directory: {error}"),
            });
        }
    };
    for entry in entries {
        let path = entry
            .map_err(|error| ScribeError::Internal {
                detail: format!("failed to read WAL directory entry: {error}"),
            })?
            .path();
        if path.is_dir() {
            remove_free_segments(&path)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "free")
        {
            std::fs::remove_file(&path).map_err(|error| ScribeError::Internal {
                detail: format!("failed to remove free WAL segment: {error}"),
            })?;
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
    /// SHA-256 digest of canonical Arrow data, excluding managed correlation columns.
    pub data_digest: [u8; 32],
    /// Canonical Arrow data length, excluding managed correlation columns.
    pub data_len: u32,
    /// SHA-256 digest of the exact WAL-v8 slice payload.
    pub payload_digest: [u8; 32],
    /// Exact WAL-v8 slice payload length.
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

/// Resets this thread's test-only count of replay payload allocations.
#[cfg(test)]
pub(crate) fn reset_replay_payload_allocations_for_test() {
    WAL_REPLAY_PAYLOAD_ALLOCATIONS.with(|count| count.set(0));
}

/// Returns this thread's test-only count of replay payload allocations.
#[cfg(test)]
pub(crate) fn replay_payload_allocations_for_test() -> u64 {
    WAL_REPLAY_PAYLOAD_ALLOCATIONS.with(std::cell::Cell::get)
}

/// Sums the `.wal` segment bytes under `root`, recursing into shard directories.
///
/// Tests compare this physical measurement before and after a refused or
/// rolled-back append to prove it left no bytes.
#[cfg(test)]
pub(crate) fn wal_file_bytes_for_test(root: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            match entry.metadata() {
                Ok(metadata) if metadata.is_dir() => wal_file_bytes_for_test(&path),
                Ok(metadata) if path.extension().is_some_and(|extension| extension == "wal") => {
                    metadata.len()
                }
                _ => 0,
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    use crate::namespaces::BifrostNamespace;

    fn test_seal_key(tenant: DataTenantId) -> SealKey {
        SealKey::new(
            tenant,
            TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "wal-test"),
            crate::test_support::day_partition(2026, 1, 1),
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
                b"payload",
            )
            .expect("complete replay batch");
        let resources =
            crate::scribe::embedded_scribe_resources(crate::scribe::AdmissionConfig::default());
        let baseline = resources.snapshot().expect("baseline snapshot");
        let reader = WalReader::open_directory_unfiltered(directory.path()).expect("reader");

        reader
            .for_each_stream_record_accounted(Some(&resources), None, None, |_, _, accounted| {
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
            .for_each_stream_record_accounted(Some(&resources), None, None, |_, _, accounted| {
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
            .for_each_stream_record_accounted(Some(&resources), None, None, |_, _, accounted| {
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
            let append = PreparedWalAppend::new(WalLsn::ZERO, batch, Bytes::copy_from_slice(data))
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
                SegmentHeader::new_v8([32_u8; 16], 1, 0, shard_id),
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
                if detail == "WAL contains non-monotonic stream-wide committed LSNs"
        ));
    }

    /// Proves a torn record before a later shard segment fails without repair.
    #[test]
    fn wal_reader_rejects_torn_nonfinal_segment_without_truncation() {
        let directory = TempDir::new().expect("temporary WAL directory");
        let shard_dir = directory.path().join("shard-00");
        std::fs::create_dir_all(&shard_dir).expect("shard directory");
        let first_path = shard_dir.join("0.wal");
        let first = WalSegment::create(&first_path, SegmentHeader::new([33_u8; 16], 1, 0))
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
            SegmentHeader::new([33_u8; 16], 1, 1),
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
                if detail == "WAL contains a torn record before a later segment"
        ));
        assert_eq!(
            std::fs::metadata(first_path)
                .expect("first segment metadata after rejection")
                .len(),
            torn_len,
            "fail-stop must not truncate a non-final segment"
        );
    }

    /// The system owner decodes like any v8 tenant; nil and non-v8 ids fail.
    #[test]
    fn slice_tenant_decoder_admits_system_owner_and_rejects_non_v7() {
        assert!(matches!(
            decode_slice_tenant(DataTenantId::SYSTEM_OWNER.as_uuid().as_bytes()),
            Ok(tenant) if tenant == DataTenantId::SYSTEM_OWNER
        ));
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
        let header = SegmentHeader::new_v8(node_id, 42, 7, 3);

        let mut encoded = header.encode();
        encoded[60..64].copy_from_slice(&header.crc32c.to_le_bytes());

        let decoded = SegmentHeader::decode(&encoded).expect("decode header");
        assert_eq!(decoded.node_id, node_id);
        assert_eq!(decoded.writer_epoch, 42);
        assert_eq!(decoded.seg_seq, 7);
        assert_eq!(decoded.shard_id, 3);
    }

    /// Proves both reserved byte ranges in the v8 segment prologue fail closed.
    #[test]
    fn segment_header_rejects_nonzero_reserved_byte_ranges() {
        for reserved_index in [33_usize, 39, 48, 59] {
            let header = SegmentHeader::new_v8([1_u8; 16], 42, 7, 3);
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
        let mut header = SegmentHeader::new([1_u8; 16], 42, 7);
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

    /// Proves the v8 record header carries the required slice identity.
    #[test]
    fn wal_record_roundtrip() {
        let batch_id = [42u8; 16];
        let record = WalRecord::new(WalLsn::new(5), 2, batch_id, b"test data".to_vec());
        let encoded = record.encode(&SegmentHeader::new([0; 16], 0, 0));

        let mut cursor = std::io::Cursor::new(encoded);
        let decoded = WalRecord::decode_from(&mut cursor, &SegmentHeader::new([0; 16], 0, 0))
            .expect("decode record")
            .expect("non-empty");

        assert_eq!(decoded.lsn, WalLsn::new(5));
        assert!(decoded.is_slice());
        assert_eq!(decoded.batch_id, batch_id);
        assert_eq!(decoded.payload, b"test data");
    }

    /// Proves a v8 commit has the exact fixed header fields and identity payload.
    ///
    /// The terminal record is the only durable carrier of the batch's logical
    /// row identity, so this pins both digests surviving a round trip rather
    /// than only the frame fields.
    #[test]
    fn wal_commit_record_uses_the_terminal_slice_identity() {
        let identity = WalCommitIdentity {
            wal_digest: [9; 32],
            logical_digest: [11; 32],
            request_id: [13; 16],
        };
        let record = WalRecord::commit(WalLsn::new(9), [7; 16], [8; 16], 3, identity);
        let encoded = record.encode(&SegmentHeader::new([0; 16], 0, 0));

        assert_eq!(&encoded[..8], b"WYRDWAL9");
        assert_eq!(encoded.len(), RECORD_HEADER_SIZE + COMMIT_PAYLOAD_BYTES + 4);
        let decoded = WalRecord::decode_from(
            &mut std::io::Cursor::new(encoded),
            &SegmentHeader::new([0; 16], 0, 0),
        )
        .expect("decode terminal record")
        .expect("terminal record exists");
        assert!(decoded.is_commit());
        assert_eq!(decoded.slice_index, 3);
        assert_eq!(decoded.slice_count, 3);
        assert_eq!(
            decoded
                .commit_identity()
                .expect("terminal identity decodes"),
            identity
        );
    }

    /// Proves a COMMIT payload of any other shape is refused, not interpreted.
    ///
    /// The v8 identity is fixed-width and total. A record written under a
    /// different layout must fail closed rather than be read as a truncated or
    /// extended identity, which is what makes the version bump load-bearing.
    #[test]
    fn wal_commit_identity_rejects_a_foreign_payload() {
        let identity = WalCommitIdentity {
            wal_digest: [1; 32],
            logical_digest: [2; 32],
            request_id: [3; 16],
        };
        let encoded = identity.encode();

        assert!(WalCommitIdentity::decode(&encoded[..COMMIT_PAYLOAD_BYTES - 1]).is_err());
        let mut extended = encoded.to_vec();
        extended.push(0);
        assert!(WalCommitIdentity::decode(&extended).is_err());
        let mut wrong_magic = encoded;
        wrong_magic[0] = b'X';
        assert!(WalCommitIdentity::decode(&wrong_magic).is_err());
        assert!(WalCommitIdentity::decode(&[0_u8; 32]).is_err());
    }

    #[test]
    fn slice_payload_decoder_rejects_trailing_bytes() {
        let seal_key = test_seal_key(crate::test_support::tenant());
        let mut payload =
            encode_slice_payload(&seal_key, [7_u8; 32], b"data").expect("encode slice payload");
        payload.push(0xff);

        let error = decode_slice_payload(&payload).expect_err("trailing bytes must fail closed");
        assert!(error.to_string().contains("trailing bytes"));
    }

    /// A slice carries its table version through the codec exactly, beyond
    /// 2^53, and a version outside `1..=i64::MAX` is refused.
    ///
    /// # Panics
    ///
    /// Panics when the version does not round-trip or an invalid version
    /// decodes.
    #[test]
    fn slice_payload_round_trips_its_table_version() {
        let version = TableVersion::new((1_i64 << 53) + 1).expect("valid version");
        let seal_key = test_seal_key(crate::test_support::tenant()).with_table_version(version);
        let payload =
            encode_slice_payload(&seal_key, [7_u8; 32], b"data").expect("encode slice payload");
        let decoded = decode_slice_payload(&payload).expect("slice decodes");
        assert_eq!(decoded.seal_key, seal_key);
        assert_eq!(decoded.seal_key.table_version, version);

        let offset = payload.len() - (TABLE_VERSION_BYTES + 32 + 4 + 4);
        for invalid in [0_i64, -1] {
            let mut payload = payload.clone();
            payload[offset..offset + TABLE_VERSION_BYTES].copy_from_slice(&invalid.to_be_bytes());
            let error = decode_slice_payload(&payload).expect_err("invalid version fails closed");
            assert!(error.to_string().contains("table version is invalid"));
        }
    }

    /// Recovery refuses a WAL slice whose table FQN is unsafe or whose magic is
    /// corrupted, so a malformed identity never replays into a table.
    ///
    /// # Panics
    ///
    /// Panics when the fixture fails to encode, or either malformed payload decodes
    /// or fails with another refusal than the invalid-FQN or magic-mismatch error.
    #[test]
    fn slice_payload_decoder_rejects_invalid_table_identity() {
        let tenant = crate::test_support::tenant();
        let mut payload = encode_slice_payload_parts(
            tenant.as_uuid().as_bytes(),
            "bifrost.bifrost.invalid/table",
            &crate::catalog::PhysicalPartition::from(crate::test_support::day_partition(
                2026, 1, 1,
            )),
            TableVersion::INITIAL,
            &[0_u8; 32],
            b"data",
        )
        .expect("encode malformed identity fixture");

        let error = decode_slice_payload(&payload).expect_err("invalid table must fail closed");
        assert!(error.to_string().contains("WAL table FQN is invalid"));
        payload[0] = b'X';
        let error = decode_slice_payload(&payload).expect_err("magic must fail closed");
        assert!(error.to_string().contains("magic mismatch"));
    }

    /// Recovery reads system-owner audit-log slices but refuses the sentinel elsewhere.
    ///
    /// # Panics
    ///
    /// Panics when a fixture fails to encode or a decode outcome differs.
    #[test]
    fn slice_payload_decoder_limits_system_owner_to_the_audit_log() {
        let system = DataTenantId::SYSTEM_OWNER.as_uuid();
        let audit_log = TableRef::new(BifrostNamespace::Audit, "audit_log").fqn();
        let payload = encode_slice_payload_parts(
            system.as_bytes(),
            &audit_log,
            &crate::catalog::PhysicalPartition::from(crate::test_support::day_partition(
                2026, 1, 1,
            )),
            TableVersion::INITIAL,
            &[0_u8; 32],
            b"data",
        )
        .expect("encode system audit slice");
        let decoded = decode_slice_payload(&payload).expect("system audit slice decodes");
        assert_eq!(decoded.seal_key.tenant, DataTenantId::SYSTEM_OWNER);

        let payload = encode_slice_payload_parts(
            system.as_bytes(),
            "bifrost.bifrost.events",
            &crate::catalog::PhysicalPartition::from(crate::test_support::day_partition(
                2026, 1, 1,
            )),
            TableVersion::INITIAL,
            &[0_u8; 32],
            b"data",
        )
        .expect("encode system non-audit slice");
        let error = decode_slice_payload(&payload).expect_err("non-audit system slice fails");
        assert!(error.to_string().contains("system owner cannot write"));
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
            .append_and_fsync_for_test(&test_seal_key(tenant_id), batch_id, b"data")
            .expect("append");

        assert_eq!(lsn, WalLsn::new(0));

        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        let records = reader.read_all_records().expect("read records");

        assert_eq!(records.len(), 1);
        assert!(records[0].is_slice());
        let decoded = decode_slice_payload(&records[0].payload).expect("slice");
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
            crate::test_support::day_partition(2026, 1, 1),
        );
        let second = SealKey::new(
            crate::test_support::tenant(),
            table,
            crate::test_support::day_partition(2026, 1, 2),
        );
        let frame = encode_append_frame([1u8; 16], b"data").expect("frame");

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
            WalConfig { segment_bytes: 500 },
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
                .append_and_fsync_for_test(&test_seal_key(tenant_id), batch_id, payload.as_bytes())
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
        let config = WalConfig { segment_bytes: 500 };
        let key = test_seal_key(tenant_id);
        let data = vec![b'd'; 600];
        let first_lsn = {
            let writer = WalWriter::new(temp_dir.path(), node_id, 1, config).expect("writer");
            writer
                .append_and_fsync_for_test(&key, [1_u8; 16], &data)
                .expect("first append")
        };

        let writer = WalWriter::new(temp_dir.path(), node_id, 1, config).expect("reopened writer");
        // Use the same batch_id as the first append so both writes route to the
        // same shard and the per-shard seg_seq counter advances to 1 after the
        // first segment fills.  The WAL itself does not deduplicate; only replay
        // does.
        let second_lsn = writer
            .append_and_fsync_for_test(&key, [1_u8; 16], &data)
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
            WalConfig { segment_bytes: 500 },
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
                    &[index; 120],
                )
                .expect("append");
        }
        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        assert!(reader.segments.len() > 1);
        let first = reader.segments[0].reference();
        let current = reader.segments.last().expect("current segment").reference();
        // A second owner still holds the current segment.
        writer
            .retain_segments(&[current.clone(), current.clone()])
            .expect("two owners retain the current segment");
        writer
            .retire_segments(&[first.clone(), current.clone()])
            .expect("retire closed segment");
        assert!(!first.path.exists());
        assert!(current.path.exists());
        writer
            .retire_segments(std::slice::from_ref(&current))
            .expect("last owner releases the current segment");
        assert!(
            !current.path.exists(),
            "an unreferenced current segment closes and retires"
        );
    }

    /// A retired segment is recycled into the next one; its stale records end
    /// replay only when no acknowledged batch lies beyond them, and rolling
    /// out of the recycled segment trims them away.
    #[test]
    fn retired_segment_is_recycled_and_its_stale_tail_ends_replay() {
        let temp_dir = TempDir::new().expect("temp dir");
        let writer = WalWriter::new(
            temp_dir.path(),
            [10_u8; 16],
            1,
            WalConfig {
                segment_bytes: 1200,
            },
        )
        .expect("writer");
        let key = test_seal_key(crate::test_support::tenant());
        let append = |fill: u8| {
            writer
                .append_and_fsync_for_test(&key, [1_u8; 16], &[fill; 300])
                .expect("append")
        };
        // Two equal records fill a segment, so segments 0 and 1 hold A B and C D.
        for fill in 0..4 {
            append(fill);
        }
        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        let first = reader.segments[0].reference();
        let free = first.path.with_extension("free");
        writer.retire_segments(&[first]).expect("retire");
        assert!(free.exists(), "the retired segment waits for reuse");

        // E rolls into segment 2, recycled from segment 0: E overwrites A and
        // B remains whole after it, CRC intact but tagged for segment 0.
        append(4);
        assert!(!free.exists(), "the next segment reused the retired file");
        let stream_lsns = |fences: Option<FenceLookup<'_>>| {
            let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
            let mut lsns = Vec::new();
            reader
                .for_each_stream_record_accounted(None, None, fences, |_, _, accounted| {
                    lsns.push(accounted.into_parts().1.lsn.as_u64());
                    Ok(false)
                })
                .map(|()| lsns)
        };
        stream_lsns(None).expect_err("without fences a stale record fails closed");
        let acknowledged_beyond = |_: StreamIdentity, _: u8| {
            Ok(Some(NewestFence {
                segment_sequence: 2,
                lsn: WalLsn::new(9),
            }))
        };
        stream_lsns(Some(&acknowledged_beyond))
            .expect_err("an acknowledged batch past the stale record fails closed");
        let unacknowledged = |_: StreamIdentity, _: u8| Ok(None);
        assert_eq!(
            stream_lsns(Some(&unacknowledged)).expect("the stale tail ends the chain"),
            vec![2, 3, 4]
        );

        // F lands after E and G rolls into a fresh segment 3, trimming
        // segment 2 so no stale record precedes a later segment.
        writer
            .append_and_fsync_for_test(&key, [1_u8; 16], &[5; 10])
            .expect("append");
        append(6);
        assert_eq!(
            stream_lsns(None).expect("no stale bytes remain"),
            vec![2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn segment_retirement_waits_for_every_generation_reference() {
        let temp_dir = TempDir::new().expect("temp dir");
        let writer = WalWriter::new(
            temp_dir.path(),
            [11u8; 16],
            1,
            WalConfig { segment_bytes: 500 },
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

    /// Finds two batch ids that route to the same fixed shard.
    ///
    /// Segment-pin behavior is only observable when two replay groups land in
    /// one segment, which requires both batches to route to the same shard.
    /// Search from `first + 1` upward so the pair is deterministic for a given
    /// seal key rather than depending on hash luck.
    ///
    /// Returns both batch ids and the shard they share.
    ///
    /// # Panics
    ///
    /// Panics if no later batch id routes to the same shard, which would mean
    /// the fixed shard count or routing function changed underneath this test.
    fn co_sharded_batch_pair(seal_key: &SealKey, first: u8) -> ([u8; 16], [u8; 16], usize) {
        let first_batch = [first; 16];
        let shard = crate::scribe::routing::shard_for(
            seal_key.tenant,
            &seal_key.table,
            uuid::Uuid::from_bytes(first_batch),
            crate::scribe::routing::TEST_SHARD_COUNT,
        );
        let second_batch = (first.saturating_add(1)..=u8::MAX)
            .map(|value| [value; 16])
            .find(|batch_id| {
                crate::scribe::routing::shard_for(
                    seal_key.tenant,
                    &seal_key.table,
                    uuid::Uuid::from_bytes(*batch_id),
                    crate::scribe::routing::TEST_SHARD_COUNT,
                ) == shard
            })
            .expect("a second batch routes to the same fixed shard");
        (first_batch, second_batch, shard)
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
        let (first_batch, second_batch, shard) = co_sharded_batch_pair(&seal_key, 1);
        for batch_id in [first_batch, second_batch] {
            writer
                .append_and_commit_for_replay_test(&seal_key, batch_id, b"payload")
                .expect("append replay group");
        }
        let reader = WalReader::open_directory_unfiltered(temp_dir.path()).expect("reader");
        assert_eq!(reader.segments.len(), 1, "both groups share one segment");
        let segment = reader.segments[0].reference();
        writer.close_all_streams().expect("close shared segment");
        writer
            .retain_segments(&[segment.clone(), segment.clone()])
            .expect("retain both generation references");
        let writer = Arc::new(writer);
        let mut commits = 0_usize;
        let cancelled = reader
            .for_each_stream_record_accounted(
                None,
                Some(writer.as_ref()),
                None,
                |_, _, accounted| {
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
                },
            )
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
            .for_each_stream_record_accounted(
                None,
                Some(restarted.as_ref()),
                None,
                |_, _, accounted| {
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
                },
            )
            .expect("restart replay");
        assert_eq!(restart_commits, 2, "restart recovers the unread group");
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
        let (first_batch, second_batch, shard) = co_sharded_batch_pair(&seal_key, 3);
        for batch_id in [first_batch, second_batch] {
            writer
                .append_and_commit_for_replay_test(&seal_key, batch_id, b"payload")
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
        writer.close_all_streams().expect("close shared segment");
        let stream = crate::scribe::stream_identity::StreamIdentity::new(
            node,
            crate::scribe::stream_identity::WriterEpoch::new(1),
        );
        let mut manifest = crate::scribe::manifest::Manifest::new(stream);
        manifest.update_sealed_lsn(&seal_key, sealed_lsn);
        // The watermark belongs to the writing shard, not the shared segment.
        let manifest_path = crate::scribe::replay::shard_manifest_path(
            temp_dir.path(),
            stream,
            u8::try_from(shard).expect("fixed shard count fits in u8"),
        );
        crate::scribe::manifest::write_atomic(&manifest_path, &manifest).expect("sealed manifest");
        assert_eq!(
            crate::scribe::manifest::read_manifest(&manifest_path)
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
            None,
            |_| panic!("fully watermarked groups must not be republished"),
        )
        .expect("sealed replay cleanup");
        assert!(
            !segment.path.exists(),
            "safe EOF removes the fully watermarked closed segment"
        );
    }

    /// Upgrading recovers v8 shard segments through their own decoder and
    /// keeps them, routes v9 records by their checked shard so one shard's
    /// watermark suppresses only its own records, and retires a shared
    /// segment only once every shard generation and cohort released it.
    #[test]
    fn v8_recovery_and_v9_shared_retirement_are_exact() {
        use crate::scribe::stream_identity::{StreamIdentity, WriterEpoch};
        let temp_dir = TempDir::new().expect("WAL root");
        let node = crate::scribe::stream_identity::NodeId::generate();
        let seal_key = test_seal_key(crate::test_support::tenant());
        let shard_of = |batch_id: [u8; 16]| {
            u8::try_from(crate::scribe::routing::shard_for(
                seal_key.tenant,
                &seal_key.table,
                uuid::Uuid::from_bytes(batch_id),
                crate::scribe::routing::TEST_SHARD_COUNT,
            ))
            .expect("fixed shard count fits in u8")
        };
        let first_batch = [1_u8; 16];
        let second_batch = (2..=u8::MAX)
            .map(|value| [value; 16])
            .find(|batch_id| shard_of(*batch_id) != shard_of(first_batch))
            .expect("a batch on another shard");
        let (first, second) = (shard_of(first_batch), shard_of(second_batch));
        let stream_dir = |epoch: i64| {
            temp_dir
                .path()
                .join(node.as_uuid().simple().to_string())
                .join(epoch.to_string())
        };
        let config = || WalConfig::new(16 * 1024 * 1024).expect("single-segment config");

        // Epoch 1 is a v8 stream: each shard's records in its own segment.
        let v8_writer =
            WalWriter::new(temp_dir.path(), *node.as_bytes(), 1, config()).expect("epoch 1");
        for batch_id in [first_batch, second_batch] {
            v8_writer
                .append_and_commit_for_replay_test(&seal_key, batch_id, b"v8")
                .expect("epoch 1 group");
        }
        v8_writer.close_all_streams().expect("close epoch 1");
        drop(v8_writer);
        let records = WalReader::open_directory_unfiltered(temp_dir.path())
            .expect("epoch 1 reader")
            .read_all_records()
            .expect("epoch 1 records");
        std::fs::remove_dir_all(stream_dir(1)).expect("drop v9 fixture source");
        let mut v8_paths = Vec::new();
        for shard in [first, second] {
            let shard_dir = stream_dir(1).join(format!("shard-{shard:02}"));
            std::fs::create_dir_all(&shard_dir).expect("v8 shard directory");
            let path = shard_dir.join("0.wal");
            let segment =
                WalSegment::create(&path, SegmentHeader::new_v8(*node.as_bytes(), 1, 0, shard))
                    .expect("v8 segment");
            for record in records.iter().filter(|record| record.shard_id == shard) {
                segment.append_and_fsync(record).expect("v8 record");
            }
            v8_paths.push(path);
        }

        // Epoch 2 is v9: both shards interleave in one shared segment, and
        // only the first shard's group is watermarked.
        let v9_writer =
            WalWriter::new(temp_dir.path(), *node.as_bytes(), 2, config()).expect("epoch 2");
        for batch_id in [first_batch, second_batch] {
            v9_writer
                .append_and_commit_for_replay_test(&seal_key, batch_id, b"v9")
                .expect("epoch 2 group");
        }
        v9_writer.close_all_streams().expect("close epoch 2");
        drop(v9_writer);
        let epoch_two = StreamIdentity::new(node, WriterEpoch::new(2));
        let shared_segments = std::fs::read_dir(stream_dir(2).join("shard-00"))
            .expect("shared segment directory")
            .map(|entry| entry.expect("shared segment entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "wal"))
            .collect::<Vec<_>>();
        assert_eq!(shared_segments.len(), 1, "v9 shards share one segment");
        let epoch_two_records = WalReader::open_directory_unfiltered(stream_dir(2))
            .expect("epoch 2 reader")
            .read_all_records()
            .expect("epoch 2 records");
        let first_sealed = epoch_two_records
            .iter()
            .filter(|record| record.shard_id == first)
            .map(|record| record.lsn)
            .max()
            .expect("first shard records");
        let mut manifest = crate::scribe::manifest::Manifest::new(epoch_two);
        manifest.update_sealed_lsn(&seal_key, first_sealed);
        crate::scribe::manifest::write_atomic(
            crate::scribe::replay::shard_manifest_path(temp_dir.path(), epoch_two, first),
            &manifest,
        )
        .expect("first shard watermark");

        // A corrupted shard byte fails the record CRC instead of rerouting it.
        let header = SegmentHeader::new(*node.as_bytes(), 2, 0);
        let mut corrupted = epoch_two_records[0].encode(&header);
        corrupted[RECORD_HEADER_SIZE_V8] ^= 1;
        assert!(WalRecord::decode_from(&mut corrupted.as_slice(), &header).is_err());

        // Restart replays both formats with no retirement references rebuilt.
        let restarted =
            WalWriter::new(temp_dir.path(), *node.as_bytes(), 3, config()).expect("epoch 3");
        let mut replayed = Vec::new();
        crate::scribe::replay::replay_wal_directory_stream_accounted(
            temp_dir.path(),
            Some(StreamIdentity::new(node, WriterEpoch::new(3))),
            None,
            Some(&restarted),
            None,
            None,
            |mut chunk| {
                // The shard owner retains replayed segments until it publishes.
                for state in chunk.states.values() {
                    restarted.retain_segments(&state.wal_segments)?;
                }
                replayed.extend(chunk.states.values().map(|state| {
                    (
                        state.stream.writer_epoch.as_i64(),
                        state.shard_id,
                        state.wal_lane,
                        state.append_metas.len(),
                    )
                }));
                Ok(crate::scribe::replay::ReplayChunkSettlement {
                    identity_memory: chunk.identity_memory.take(),
                })
            },
        )
        .expect("mixed-format replay");
        replayed.sort_unstable();
        let mut expected = vec![
            (1, first, first, 1),
            (1, second, second, 1),
            (2, second, 0, 1),
        ];
        expected.sort_unstable();
        assert_eq!(
            replayed, expected,
            "the watermark suppresses only its shard"
        );
        for path in &v8_paths {
            assert!(path.exists(), "upgrading never deletes v8 segments");
        }
        assert!(
            shared_segments[0].exists(),
            "one safe shard never retires a segment another shard still reads"
        );

        // Live shared retirement: two shard generations, then their cohorts.
        let live = WalWriter::new(stream_dir(9), [43_u8; 16], 1, config()).expect("live writer");
        let (zero, seven) = (
            live.handle_for_shard(0).expect("shard zero"),
            live.handle_for_shard(7).expect("shard seven"),
        );
        let append = |handle: &WalHandle, batch: u8| {
            let prepared =
                PreparedWalAppend::new(WalLsn::ZERO, [batch; 16], Bytes::from_static(b"live"))
                    .for_slice(seal_key.clone(), [7_u8; 32]);
            let result = handle.append_prepared(prepared).expect("live append");
            handle
                .sync_segments(&result.touched_segments)
                .expect("live sync");
        };
        let rotate = |handle: &WalHandle| {
            let closed = handle.close_active_generation().expect("close generation");
            handle.retain_segments(&closed).expect("cohort retains");
            handle
                .retire_segments(&closed)
                .expect("generation releases");
            closed
        };
        append(&zero, 1);
        append(&seven, 2);
        let zero_cohort = rotate(&zero);
        assert_eq!(zero_cohort.len(), 1);
        let shared = zero_cohort[0].path.clone();
        zero.retire_segments(&zero_cohort)
            .expect("zero cohort publishes");
        assert!(shared.exists(), "shard seven's generation still holds it");
        let seven_cohort = rotate(&seven);
        assert_eq!(seven_cohort[0].path, shared);
        seven
            .retire_segments(&seven_cohort)
            .expect("seven cohort publishes");
        assert!(
            !shared.exists(),
            "the last release closes and retires even the current segment"
        );
        append(&zero, 3);
        let next = rotate(&zero);
        assert_ne!(next[0].path, shared, "the next append opens a new segment");
        assert!(next[0].path.exists(), "the new segment's cohort holds it");
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
            .append_and_fsync_for_test(&test_seal_key(tenant_id), batch_id, b"data1")
            .expect("append 1");
        writer
            .append_and_fsync_for_test(&test_seal_key(tenant_id), batch_id, b"data2")
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
                b"data",
            )
            .expect("expected append");

        let foreign_segment_path = temp_dir.path().join("foreign.wal");
        let foreign_segment = WalSegment::create(
            &foreign_segment_path,
            SegmentHeader::new(foreign_node, 9, 0),
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
            "the expected stream's data records remain readable"
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

    #[test]
    fn prepared_encoded_len_matches_actual_for_payload_boundaries() {
        let key = test_seal_key(crate::test_support::tenant());
        for size in [0, 1, 255, 256, 4096] {
            let prepared =
                PreparedWalAppend::new(WalLsn::ZERO, [7; 16], Bytes::from(vec![2; size]))
                    .for_slice(key.clone(), [3; 32]);
            let actual = prepared
                .record()
                .expect("record")
                .encode(&SegmentHeader::new([0; 16], 0, 0))
                .len();
            assert_eq!(prepared.encoded_len().expect("encoded len"), actual);
        }
    }

    /// A released segment survives a failed deletion and is removed by the
    /// next retirement pass.
    ///
    /// # Panics
    ///
    /// Panics if a failed removal forgets the path, or if a later empty pass
    /// leaves it on disk.
    #[test]
    fn released_segment_deletion_retries_until_it_succeeds() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = TempDir::new().expect("temp dir");
        let key = test_seal_key(crate::test_support::tenant());
        let writer =
            WalWriter::new(temp_dir.path(), [23; 16], 1, WalConfig::default()).expect("writer");
        writer
            .append_and_fsync_for_test(&key, [3; 16], b"data")
            .expect("append");
        let segment = WalReader::open_directory_unfiltered(temp_dir.path())
            .expect("reader")
            .segments
            .first()
            .expect("segment")
            .reference();
        writer.close_all_streams().expect("close");
        let shard_dir = segment.path.parent().expect("shard directory");
        std::fs::set_permissions(shard_dir, std::fs::Permissions::from_mode(0o500))
            .expect("read-only shard directory");
        let refused = writer.retire_segments(std::slice::from_ref(&segment));
        std::fs::set_permissions(shard_dir, std::fs::Permissions::from_mode(0o700))
            .expect("writable shard directory");
        refused.expect_err("removal from a read-only directory fails");
        assert!(segment.path.exists());
        writer.retire_segments(&[]).expect("retry pass");
        assert!(!segment.path.exists(), "the deferred deletion was retried");
    }

    /// Proves production append streams borrowed payloads without frame copies.
    #[test]
    fn append_streams_without_frame_encoding() {
        let _guard = WAL_FAULT_LOCK.lock().expect("test hook lock");
        let temp_dir = TempDir::new().expect("temp dir");
        let writer =
            WalWriter::new(temp_dir.path(), [22; 16], 1, WalConfig::default()).expect("writer");
        let key = test_seal_key(crate::test_support::tenant());
        WAL_ENCODE_COUNT.with(|count| count.set(0));
        WAL_COUNT_ACTIVE.store(true, Ordering::Relaxed);
        writer
            .append_and_fsync_for_test(&key, [2; 16], b"data")
            .expect("append");
        assert_eq!(WAL_ENCODE_COUNT.with(std::cell::Cell::get), 0);
        WAL_COUNT_ACTIVE.store(false, Ordering::Relaxed);
    }

    #[test]
    fn partial_write_rolls_back_and_allows_later_append() {
        let _guard = WAL_FAULT_LOCK.lock().expect("test hook lock");
        WAL_PARTIAL_WRITE.with(|flag| flag.set(false));
        let temp_dir = TempDir::new().expect("temp dir");
        let writer =
            WalWriter::new(temp_dir.path(), [25; 16], 1, WalConfig::default()).expect("writer");
        let key = test_seal_key(crate::test_support::tenant());
        WAL_PARTIAL_WRITE.with(|flag| flag.set(true));
        let error = writer
            .append_and_fsync_for_test(&key, [6; 16], b"data")
            .expect_err("partial write must fail");
        assert!(matches!(error, ScribeError::Internal { .. }));
        writer
            .append_and_fsync_for_test(&key, [7; 16], b"data")
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
                .append_and_fsync_for_test(&key, [1; 16], b"data")
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
                .append_and_fsync_for_test(&key, [2; 16], b"data")
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

    /// A failed sync after the record is written faults only this WAL.
    ///
    /// Later appends are refused before mutation, the signal the owning role
    /// watches fires, and retirement keeps every file for restart replay.
    ///
    /// # Panics
    ///
    /// Panics when the WAL is not faulted, a later append is admitted, or a
    /// retired segment file is deleted while faulted.
    #[test]
    fn sync_fault_is_sticky_and_keeps_files() {
        let directory = TempDir::new().expect("temporary WAL directory");
        let writer = WalWriter::new(directory.path(), [31; 16], 1, WalConfig::default())
            .expect("WAL writer");
        let key = test_seal_key(crate::test_support::tenant());
        let signal = writer.fault_signal();
        writer.trip_sync_failure_for_test();
        writer
            .append_and_fsync_for_test(&key, [1; 16], b"data")
            .expect_err("injected sync failure");
        assert!(writer.is_faulted() && signal.is_cancelled());
        let written = wal_file_bytes_for_test(directory.path());
        assert!(written > 0, "the ambiguous record stays on disk");

        let refused = writer
            .append_and_fsync_for_test(&key, [2; 16], b"data")
            .expect_err("faulted WAL refuses appends");
        assert!(matches!(refused, ScribeError::IngressClosed), "{refused:?}");
        let segments: Vec<PathBuf> = std::fs::read_dir(directory.path())
            .expect("WAL directory")
            .map(|entry| entry.expect("WAL entry").path())
            .collect();
        writer
            .delete_closed_segments(&segments)
            .expect("faulted retirement defers");
        assert_eq!(wal_file_bytes_for_test(directory.path()), written);
    }

    /// A full device refuses the next append before WAL mutation, and appends
    /// resume as soon as the device has space again.
    ///
    /// There is no WAL-only latch or capacity ledger: the append's own
    /// filesystem outcome is the only decision.
    ///
    /// # Panics
    ///
    /// Panics when the refused append is not `WalDiskFull`, leaves WAL bytes
    /// behind, or the append after space returns is still refused.
    #[test]
    fn wal_capacity_refusal_is_not_sticky() {
        let directory = TempDir::new().expect("temporary WAL directory");
        let writer = WalWriter::new(directory.path(), [30; 16], 1, WalConfig::default())
            .expect("WAL writer");
        let key = test_seal_key(crate::test_support::tenant());
        writer
            .append_and_fsync_for_test(&key, [1; 16], b"data")
            .expect("durable append");
        let before = wal_file_bytes_for_test(directory.path());

        writer.set_device_full_for_test(true);
        let refusal = writer
            .append_and_fsync_for_test(&key, [2; 16], b"data")
            .expect_err("a full device refuses the append");
        assert!(matches!(refusal, ScribeError::WalDiskFull), "{refusal:?}");
        assert_eq!(
            wal_file_bytes_for_test(directory.path()),
            before,
            "the refused append wrote nothing"
        );

        writer.set_device_full_for_test(false);
        writer
            .append_and_fsync_for_test(&key, [2; 16], b"data")
            .expect("appends resume once space returns");
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
