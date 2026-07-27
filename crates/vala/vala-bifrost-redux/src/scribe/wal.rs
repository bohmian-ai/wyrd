//! Write-Ahead Log (WAL) for Scribe — crash-consistent framed records.
//!
//! Every prepared day slice emits one self-describing WAL record. The record
//! contains its tenant/table/day identity, canonical audit envelope, and Arrow
//! IPC payload so replay does not depend on directory names or paired records.
//!
//! Segment format per `scribe/00-architecture.md §Crash-consistent WAL format`:
//! - Fixed 64-byte header with magic, version, `node_id`, `writer_epoch`, CRC
//! - Variable-length framed records: `[len][lsn][kind][reserved][batch_id][slice][crc32c]`
//! - Atomic segment rollover: write-fsync-rename-fsync-parent
//! - Torn tail truncation on replay at first CRC/length/monotonicity failure
//!
//! Version 3 is the only accepted format. Older paired formats are rejected.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use rustix::fs::statvfs;
use uuid::Uuid;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::scribe::seal_key::{EventDay, SealKey};
use crate::scribe::stream_identity::StreamIdentity;

#[cfg(test)]
static WAL_ENCODE_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static WAL_WALK_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static WAL_COUNT_ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static WAL_PARTIAL_WRITE: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static WAL_FAULT_LOCK: Mutex<()> = Mutex::new(());

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
    /// Format version (3 is the only accepted implementation).
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
const WAL_VERSION: u16 = 3;
const SEGMENT_HEADER_SIZE: usize = 64;
#[cfg(test)]
const APPEND_FRAME_MAGIC_V3: [u8; 4] = *b"SWF3";
const RECORD_RESERVED: [u8; 3] = *b"WYD";

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

/// WAL record — variable length framed entry.
///
/// Layout: `[len:u32][lsn:u64][kind:u8][reserved:u8×3][batch_id:u8×16][payload][crc32c:u32]`.
///
#[derive(Debug, Clone)]
pub struct WalRecord {
    /// LSN for this record (monotonic per stream).
    pub lsn: WalLsn,
    /// Record kind. Version 3 uses `2` for one complete day slice.
    pub record_kind: u8,
    /// Batch ID for deduplication of the complete audit-plus-data record.
    pub batch_id: [u8; 16],
    /// Self-describing slice payload.
    pub payload: Vec<u8>,
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
            segment_bytes: 64 * 1024 * 1024,
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
    pub(crate) shard_id: Option<u8>,
}

/// Result of one append, including every segment whose bytes were touched.
#[derive(Debug)]
pub(crate) struct WalAppendResult {
    pub(crate) lsn: WalLsn,
    #[cfg(feature = "bench-support")]
    pub(crate) encoded_bytes: u64,
    pub(crate) touched_segments: Vec<Arc<WalSegment>>,
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
            shard_id: None,
        }
    }

    pub(crate) fn for_slice(mut self, seal_key: SealKey, schema_fingerprint: [u8; 32]) -> Self {
        self.seal_key = Some(seal_key);
        self.schema_fingerprint = schema_fingerprint;
        self
    }

    fn assign_lsn(&mut self, lsn: WalLsn) {
        self.lsn = lsn;
    }

    fn record(&self) -> Result<WalRecord, ScribeError> {
        #[cfg(test)]
        if WAL_COUNT_ACTIVE.load(Ordering::Relaxed) {
            WAL_ENCODE_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        let seal_key = self
            .seal_key
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "prepared WAL append is missing its self-describing seal key".to_owned(),
            })?;
        let payload =
            encode_slice_payload(seal_key, self.schema_fingerprint, &self.audit, &self.data)?;
        Ok(WalRecord::new(self.lsn, 2, self.batch_id, payload))
    }

    pub(crate) fn encoded_len(&self) -> Result<usize, ScribeError> {
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
        payload_len
            .checked_add(36)
            .ok_or_else(|| ScribeError::Internal {
                detail: "encoded WAL record length overflow".to_owned(),
            })
    }
}

const SLICE_PAYLOAD_MAGIC: [u8; 4] = *b"S3SL";

struct CountWriter<'a>(&'a mut usize);

impl FmtWrite for CountWriter<'_> {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        *self.0 = self.0.saturating_add(value.len());
        Ok(())
    }
}

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
    /// Construct a new WAL record.
    #[must_use]
    pub fn new(lsn: WalLsn, record_kind: u8, batch_id: [u8; 16], payload: Vec<u8>) -> Self {
        Self {
            lsn,
            record_kind,
            batch_id,
            payload,
        }
    }

    /// Encode the record to bytes (including frame header and CRC).
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "WAL payload len constrained by record format"
    )]
    pub fn encode(&self) -> Vec<u8> {
        let len = self.payload.len() as u32;
        let mut buf = Vec::with_capacity(4 + 8 + 1 + 3 + 16 + self.payload.len() + 4);

        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(&self.lsn.as_u64().to_le_bytes());
        buf.push(self.record_kind);
        buf.extend_from_slice(&RECORD_RESERVED);
        buf.extend_from_slice(&self.batch_id);
        buf.extend_from_slice(&self.payload);

        // Compute CRC over len + lsn + kind + reserved + batch_id + payload
        let crc = crc32c_hash(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());

        buf
    }

    /// Decode a record from the given reader.
    ///
    /// Returns `None` at clean EOF (no bytes available).
    /// Returns `Err` on short read, CRC mismatch, or invalid record.
    pub fn decode_from<R: Read>(reader: &mut R) -> Result<Option<Self>, ScribeError> {
        // Read len
        let mut len_buf = [0u8; 4];
        match reader.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => {
                return Err(ScribeError::Internal {
                    detail: format!("WAL record read error: {e}"),
                });
            }
        }
        let len = u32::from_le_bytes(len_buf);

        // Read lsn
        let mut lsn_bytes = [0u8; 8];
        reader
            .read_exact(&mut lsn_bytes)
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL record LSN read error: {e}"),
            })?;
        let lsn = WalLsn::new(u64::from_le_bytes(lsn_bytes));

        // Read kind
        let mut kind_buf = [0u8; 1];
        reader
            .read_exact(&mut kind_buf)
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL record kind read error: {e}"),
            })?;
        let envelope_kind = kind_buf[0];

        // Validate kind
        if envelope_kind != 2 {
            return Err(ScribeError::Internal {
                detail: format!("invalid WAL record kind: {envelope_kind}"),
            });
        }

        // Read reserved
        let mut reserved_buf = [0u8; 3];
        reader
            .read_exact(&mut reserved_buf)
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL record reserved read error: {e}"),
            })?;
        if reserved_buf != RECORD_RESERVED {
            return Err(ScribeError::Internal {
                detail: "invalid WAL record format marker".to_string(),
            });
        }

        // Read batch_id
        let mut batch_id = [0u8; 16];
        reader
            .read_exact(&mut batch_id)
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL record batch_id read error: {e}"),
            })?;

        // Read payload
        let mut payload = vec![0u8; len as usize];
        reader
            .read_exact(&mut payload)
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL record payload read error: {e}"),
            })?;

        // Read CRC
        let mut crc_buf = [0u8; 4];
        reader
            .read_exact(&mut crc_buf)
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL record CRC read error: {e}"),
            })?;
        let expected_crc = u32::from_le_bytes(crc_buf);

        // Verify CRC over [len][lsn][kind][reserved][batch_id][payload]
        let mut crc_input = Vec::with_capacity(4 + 8 + 1 + 3 + 16 + payload.len());
        crc_input.extend_from_slice(&len_buf);
        crc_input.extend_from_slice(&lsn_bytes);
        crc_input.extend_from_slice(&kind_buf);
        crc_input.extend_from_slice(&reserved_buf);
        crc_input.extend_from_slice(&batch_id);
        crc_input.extend_from_slice(&payload);

        let computed_crc = crc32c_hash(&crc_input);
        if computed_crc != expected_crc {
            return Err(ScribeError::Internal {
                detail: format!(
                    "WAL record CRC mismatch: expected {expected_crc:#x}, got {computed_crc:#x}"
                ),
            });
        }

        Ok(Some(Self {
            lsn,
            record_kind: envelope_kind,
            batch_id,
            payload,
        }))
    }
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
        WAL_WALK_COUNT.fetch_add(1, Ordering::Relaxed);
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

        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(path)
            .map_err(|error| wal_io_error("failed to open WAL segment for append", &error))?;

        Ok(Self {
            path: path.to_path_buf(),
            file: Mutex::new(file),
            header,
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
        })
    }

    /// Append a record to the segment without forcing it to stable storage.
    pub fn append(&self, record: &WalRecord) -> Result<(), ScribeError> {
        let encoded = record.encode();
        let mut file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (append)".to_string(),
        })?;

        file.write_all(&encoded).map_err(|e| {
            if e.kind() == io::ErrorKind::StorageFull || e.raw_os_error() == Some(28) {
                ScribeError::WalDiskFull
            } else {
                ScribeError::Internal {
                    detail: format!("WAL record write failed: {e}"),
                }
            }
        })?;
        Ok(())
    }

    fn append_encoded(&self, encoded: &[u8]) -> Result<(), ScribeError> {
        let mut file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (append)".to_string(),
        })?;
        #[cfg(test)]
        if WAL_PARTIAL_WRITE.swap(false, Ordering::AcqRel) {
            let prefix = encoded.len().max(1) / 2;
            file.write_all(&encoded[..prefix])
                .map_err(|e| wal_io_error("WAL partial record write failed", &e))?;
            return Err(ScribeError::Internal {
                detail: "injected partial WAL write".to_owned(),
            });
        }
        file.write_all(encoded)
            .map_err(|e| wal_io_error("WAL record write failed", &e))
    }

    /// Force all appended data for this segment to stable storage.
    pub fn sync_data(&self) -> Result<(), ScribeError> {
        let file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (sync_data)".to_string(),
        })?;
        file.sync_data().map_err(|e| {
            if e.kind() == io::ErrorKind::StorageFull || e.raw_os_error() == Some(28) {
                ScribeError::WalDiskFull
            } else {
                ScribeError::Internal {
                    detail: format!("WAL data sync failed: {e}"),
                }
            }
        })
    }

    /// Append a record and force it to stable storage.
    pub fn append_and_fsync(&self, record: &WalRecord) -> Result<(), ScribeError> {
        self.append(record)?;
        self.sync_data()?;
        Ok(())
    }

    /// Read all records from the segment.
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
    /// `Vec<(segment, record)>` before grouping state.
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
                        file.set_len(record_offset)
                            .map_err(|error| ScribeError::Internal {
                                detail: format!(
                                    "WAL non-monotonic tail truncation failed: {error}"
                                ),
                            })?;
                        file.sync_data().map_err(|error| ScribeError::Internal {
                            detail: format!("WAL non-monotonic tail sync failed: {error}"),
                        })?;
                        break;
                    }
                    previous_lsn = Some(record.lsn);
                    visit(record)?;
                }
                Ok(None) => break,
                Err(_) => {
                    file.set_len(record_offset)
                        .map_err(|error| ScribeError::Internal {
                            detail: format!("WAL torn-tail truncation failed: {error}"),
                        })?;
                    file.sync_data().map_err(|error| ScribeError::Internal {
                        detail: format!("WAL torn-tail sync failed: {error}"),
                    })?;
                    break;
                }
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

    pub(crate) fn append_prepared(
        &self,
        mut append: PreparedWalAppend,
    ) -> Result<WalAppendResult, ScribeError> {
        if append.seal_key.is_none() {
            return Err(ScribeError::Internal {
                detail: "shard WAL append is missing its self-describing seal key".to_owned(),
            });
        }
        append.shard_id = Some(self.shard_id);
        self.writer.append_prepared(append)
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

    pub(crate) fn retire_segments(&self, segments: &[WalSegmentRef]) -> Result<(), ScribeError> {
        self.writer.retire_segments(segments)
    }

    pub(crate) fn retain_segments(&self, segments: &[WalSegmentRef]) -> Result<(), ScribeError> {
        self.writer.retain_segments(segments)
    }

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
    /// Create a new pod-local WAL writer.
    ///
    /// Segment sizing is explicit and validated before the writer is returned.
    pub fn new(
        base_dir: impl AsRef<Path>,
        node_id: [u8; 16],
        writer_epoch: i64,
        config: WalConfig,
    ) -> Result<Self, ScribeError> {
        Self::new_with_shard_id(base_dir, node_id, writer_epoch, 0, config)
    }

    /// Create one fixed-shard WAL stream.
    pub fn new_with_shard_id(
        base_dir: impl AsRef<Path>,
        node_id: [u8; 16],
        writer_epoch: i64,
        shard_id: u8,
        config: WalConfig,
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
        }
    }

    /// Append one self-describing v3 record for deterministic test-tier probes.
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

    /// Append one prepared v3 record without syncing it.
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
        let lsn = WalLsn::new(self.next_lsn.fetch_add(1, Ordering::SeqCst));
        prepared.assign_lsn(lsn);
        let encoded = prepared.record()?.encode();
        debug_assert_eq!(
            usize::try_from(encoded_bytes).expect("invariant: encoded length fits usize"),
            encoded.len()
        );
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
        )?;
        if let Err(error) = segment.append_encoded(&encoded) {
            if matches!(error, ScribeError::WalDiskFull) {
                self.disk.mark_hard_failed();
            }
            drop(state);
            self.disk.reconcile();
            return Err(error);
        }
        self.disk.add_bytes(encoded_bytes);
        state.current_segment_size = state.current_segment_size.saturating_add(encoded_bytes);
        state.current_segment_records = state.current_segment_records.saturating_add(1);
        Ok(WalAppendResult {
            lsn,
            #[cfg(feature = "bench-support")]
            encoded_bytes,
            touched_segments: vec![segment],
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
        #[cfg(any(test, feature = "test-support"))]
        if self.disk.take_sync_failure() {
            return Err(ScribeError::Internal {
                detail: "injected WAL sync failure".to_owned(),
            });
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
        prepared.shard_id = Some(
            u8::try_from(crate::scribe::routing::shard_for(
                seal_key.tenant,
                &seal_key.table,
            ))
            .expect("fixed shard count fits in u8"),
        );
        Ok(self.append_prepared(prepared)?.lsn)
    }

    #[cfg(test)]
    pub(crate) fn sync_data_for_test(&self, seal_key: &SealKey) -> Result<(), ScribeError> {
        let shard_id = u8::try_from(crate::scribe::routing::shard_for(
            seal_key.tenant,
            &seal_key.table,
        ))
        .expect("fixed shard count fits in u8");
        self.sync_data_for_shard(shard_id)
    }

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
        prepared.shard_id = Some(
            u8::try_from(crate::scribe::routing::shard_for(
                seal_key.tenant,
                &seal_key.table,
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
    pub(crate) fn retain_segments(&self, segments: &[WalSegmentRef]) -> Result<(), ScribeError> {
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

    /// Close a current segment after its final active bucket is detached.
    ///
    /// The segment remains on disk while its immutable generation is pending;
    /// the normal grace-period retirement then removes it. Closing here avoids
    /// retaining a low-volume current file forever when no later append causes
    /// a size rollover.
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
            }
        }
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

/// WAL reader — reads segments and returns records.
#[derive(Debug)]
pub struct WalReader {
    segments: Vec<Arc<WalSegment>>,
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
        let mut segments = Vec::new();
        for path in paths {
            if path.extension().and_then(|s| s.to_str()) == Some("wal") {
                let segment = WalSegment::open(&path)?;
                if let Some(stream) = stream
                    && (segment.header().node_id != *stream.node_id.as_uuid().as_bytes()
                        || segment.header().writer_epoch != stream.writer_epoch.as_i64())
                {
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

        Ok(Self { segments })
    }

    /// Read all records from all segments.
    pub fn read_all_records(&self) -> Result<Vec<WalRecord>, ScribeError> {
        let mut all_records = Vec::new();
        for segment in &self.segments {
            let records = segment.read_records()?;
            all_records.extend(records);
        }
        Ok(all_records)
    }

    /// Visit records from every segment without retaining the whole WAL
    /// directory in an intermediate vector.
    pub(crate) fn for_each_record<F>(&self, mut visit: F) -> Result<(), ScribeError>
    where
        F: FnMut(PathBuf, WalRecord) -> Result<(), ScribeError>,
    {
        for segment in &self.segments {
            let path = segment.path().to_path_buf();
            segment.for_each_record(|record| visit(path.clone(), record))?;
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

/// Scribe-local metadata derived from one WAL slice record and its Arrow data.
///
/// This struct carries seal-key context and LSN range; it does NOT live inside
/// the `AuditEvent` (which is the canonical CONTRACTS §11 shape).
#[derive(Debug, Clone)]
pub struct ScribeAppendMeta {
    /// Opaque batch ID for dedup.
    pub batch_id: [u8; 16],
    /// Number of rows accepted in this append.
    pub rows_accepted: usize,
    /// Minimum LSN for this append's WAL records.
    pub wal_lsn_min: WalLsn,
    /// Maximum LSN for this append's WAL records.
    pub wal_lsn_max: WalLsn,
    /// Canonical seal-key path components for this append.
    pub seal_key: String,
}

/// Compute CRC32C hash of the given bytes.
fn crc32c_hash(data: &[u8]) -> u32 {
    crc32c::crc32c(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scribe::seal_key::EventDay;
    use tempfile::TempDir;

    fn test_seal_key(tenant: DataTenantId) -> SealKey {
        SealKey::new(
            tenant,
            TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "wal-test"),
            EventDay::new(chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("test date")),
        )
    }

    #[test]
    fn wal_lsn_ordering() {
        assert!(WalLsn::new(1) > WalLsn::ZERO);
        assert!(WalLsn::new(100) > WalLsn::new(99));
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
        assert_eq!(decoded.record_kind, 2);
        assert_eq!(decoded.batch_id, batch_id);
        assert_eq!(decoded.payload, b"test data");
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
        assert_eq!(records[0].record_kind, 2);
        let decoded = decode_slice_payload(&records[0].payload).expect("slice");
        assert_eq!(decoded.audit, b"audit");
        assert_eq!(decoded.data, b"data");
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

        // Write appends totaling >500 bytes (each record has overhead)
        for i in 0u8..10 {
            let payload = format!("data-{i:03}").repeat(20); // ~100 bytes per payload
            let batch_id = [i; 16];
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
        let second_lsn = writer
            .append_and_fsync_for_test(&key, [2_u8; 16], b"audit", &data)
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
        for index in 0u8..10 {
            writer
                .append_and_fsync_for_test(
                    &test_seal_key(crate::test_support::tenant()),
                    [index; 16],
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
        for index in 0u8..10 {
            writer
                .append_and_fsync_for_test(
                    &test_seal_key(crate::test_support::tenant()),
                    [index; 16],
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

    #[test]
    fn wal_segment_roll_is_atomic_across_crash() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = [5u8; 16];
        let tenant_id = crate::test_support::tenant();

        let writer =
            WalWriter::new(temp_dir.path(), node_id, 1, WalConfig::default()).expect("writer");

        // Write 2 appends to segment 0
        let batch_id1 = [1u8; 16];
        let batch_id2 = [2u8; 16];
        writer
            .append_and_fsync_for_test(&test_seal_key(tenant_id), batch_id1, b"audit1", b"data1")
            .expect("append 1");
        writer
            .append_and_fsync_for_test(&test_seal_key(tenant_id), batch_id2, b"audit2", b"data2")
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

    #[test]
    fn statvfs_matches_direct_measurement_without_df() {
        let temp_dir = TempDir::new().expect("temp dir");
        let stats = rustix::fs::statvfs(temp_dir.path()).expect("statvfs");
        let expected_capacity = stats.f_blocks.saturating_mul(stats.f_frsize);
        let expected_available = stats.f_bavail.saturating_mul(stats.f_frsize);
        let measured = filesystem_space(temp_dir.path()).expect("filesystem sample");
        assert_eq!(measured, (expected_capacity, expected_available));
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
            .append_and_fsync_for_test(&key, [1; 16], b"audit", b"data")
            .expect("append");
        let actual = directory_bytes(temp_dir.path());
        assert_eq!(writer.bytes_on_disk(), actual);
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

    #[test]
    fn append_encodes_once_and_does_not_walk_unrelated_wal_files() {
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
        WAL_ENCODE_COUNT.store(0, Ordering::Relaxed);
        WAL_WALK_COUNT.store(0, Ordering::Relaxed);
        WAL_COUNT_ACTIVE.store(true, Ordering::Relaxed);
        writer
            .append_and_fsync_for_test(&key, [2; 16], b"audit", b"data")
            .expect("append");
        assert!(WAL_ENCODE_COUNT.load(Ordering::Relaxed) >= 1);
        assert_eq!(WAL_WALK_COUNT.load(Ordering::Relaxed), 0);
        let _ = writer.disk_pressure();
        assert!(WAL_WALK_COUNT.load(Ordering::Relaxed) > 0);
        WAL_COUNT_ACTIVE.store(false, Ordering::Relaxed);
    }

    #[test]
    fn reconciliation_repairs_under_count_without_shard_state_lock() {
        let _guard = WAL_FAULT_LOCK.lock().expect("test hook lock");
        WAL_PARTIAL_WRITE.store(false, Ordering::Release);
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
        writer.disk.force_sample(None);
        let key = test_seal_key(crate::test_support::tenant());
        let before = writer.bytes_on_disk();
        writer.disk.force_sample(Some((u64::MAX, u64::MAX)));
        writer.disk.force_sample(None);
        let error = writer
            .append_and_fsync_for_test(&key, [4; 16], b"audit", b"data")
            .expect_err("forced zero sample must reject");
        assert!(matches!(error, ScribeError::WalDiskFull));
        assert_eq!(writer.bytes_on_disk(), before);
        writer.disk.force_sample(Some((u64::MAX, u64::MAX)));
        writer
            .append_and_fsync_for_test(&key, [5; 16], b"audit", b"data")
            .expect("successful sample recovers");
    }

    #[test]
    fn partial_write_reconciles_conservatively_and_allows_later_append() {
        let _guard = WAL_FAULT_LOCK.lock().expect("test hook lock");
        WAL_PARTIAL_WRITE.store(false, Ordering::Release);
        let temp_dir = TempDir::new().expect("temp dir");
        let writer =
            WalWriter::new(temp_dir.path(), [25; 16], 1, WalConfig::default()).expect("writer");
        let key = test_seal_key(crate::test_support::tenant());
        WAL_PARTIAL_WRITE.store(true, Ordering::Release);
        let error = writer
            .append_and_fsync_for_test(&key, [6; 16], b"audit", b"data")
            .expect_err("partial write must fail");
        assert!(matches!(error, ScribeError::Internal { .. }));
        assert!(writer.bytes_on_disk() >= directory_bytes(temp_dir.path()));
        writer
            .append_and_fsync_for_test(&key, [7; 16], b"audit", b"data")
            .expect("state lock and later append remain usable");
    }
}
