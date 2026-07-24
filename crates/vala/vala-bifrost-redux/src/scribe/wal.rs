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

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::scribe::seal_key::{EventDay, SealKey};
use crate::scribe::stream_identity::StreamIdentity;

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
            return Err(ScribeError::Internal {
                detail: format!("unsupported WAL version: {version}"),
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
    /// Batch ID for deduplication (shared across paired audit+data records).
    pub batch_id: [u8; 16],
    /// Self-describing slice payload.
    pub payload: Vec<u8>,
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
            segment_bytes: 64 * 1024 * 1024,
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
    pub(crate) touched_segments: Vec<Arc<WalSegment>>,
    pub(crate) encoded_bytes: u64,
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
        let payload = match &self.seal_key {
            Some(seal_key) => {
                encode_slice_payload(seal_key, self.schema_fingerprint, &self.audit, &self.data)?
            }
            None => encode_unscoped_payload(&self.audit, &self.data)?,
        };
        Ok(WalRecord::new(self.lsn, 2, self.batch_id, payload))
    }
}

const SLICE_PAYLOAD_MAGIC: [u8; 4] = *b"S3SL";

fn encode_unscoped_payload(audit: &[u8], data: &[u8]) -> Result<Vec<u8>, ScribeError> {
    encode_slice_payload_parts(&[0; 16], "", "1970-01-01", &[0; 32], audit, data)
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

pub(crate) fn decode_slice_payload(payload: &[u8]) -> Result<DecodedSlicePayload, ScribeError> {
    let mut cursor = 0usize;
    let take = |cursor: &mut usize, length: usize| -> Result<&[u8], ScribeError> {
        let end = cursor
            .checked_add(length)
            .ok_or_else(|| ScribeError::Internal {
                detail: "WAL slice payload offset overflow".to_owned(),
            })?;
        let value = payload
            .get(*cursor..end)
            .ok_or_else(|| ScribeError::Internal {
                detail: "WAL slice payload is truncated".to_owned(),
            })?;
        *cursor = end;
        Ok(value)
    };
    if take(&mut cursor, 4)? != SLICE_PAYLOAD_MAGIC {
        return Err(ScribeError::Internal {
            detail: "WAL v3 slice magic mismatch".to_owned(),
        });
    }
    let mut tenant_bytes = [0u8; 16];
    tenant_bytes.copy_from_slice(take(&mut cursor, 16)?);
    let tenant_uuid = uuid::Uuid::from_bytes(tenant_bytes);
    let tenant = if tenant_uuid.is_nil() {
        DataTenantId::SYSTEM_OWNER
    } else {
        DataTenantId::try_from(tenant_uuid).map_err(|error| ScribeError::Internal {
            detail: format!("WAL v3 tenant id is invalid: {error}"),
        })?
    };
    let table_len =
        u16::from_le_bytes(
            take(&mut cursor, 2)?
                .try_into()
                .map_err(|_| ScribeError::Internal {
                    detail: "WAL table length decode failed".to_owned(),
                })?,
        ) as usize;
    let table_fqn = std::str::from_utf8(take(&mut cursor, table_len)?).map_err(|error| {
        ScribeError::Internal {
            detail: format!("WAL table FQN is not UTF-8: {error}"),
        }
    })?;
    let day_len = usize::from(take(&mut cursor, 1)?[0]);
    let day = std::str::from_utf8(take(&mut cursor, day_len)?).map_err(|error| {
        ScribeError::Internal {
            detail: format!("WAL partition day is not UTF-8: {error}"),
        }
    })?;
    let mut schema_fingerprint = [0u8; 32];
    schema_fingerprint.copy_from_slice(take(&mut cursor, 32)?);
    let audit_len =
        u32::from_le_bytes(
            take(&mut cursor, 4)?
                .try_into()
                .map_err(|_| ScribeError::Internal {
                    detail: "WAL audit length decode failed".to_owned(),
                })?,
        ) as usize;
    let audit = take(&mut cursor, audit_len)?.to_vec();
    let data_len =
        u32::from_le_bytes(
            take(&mut cursor, 4)?
                .try_into()
                .map_err(|_| ScribeError::Internal {
                    detail: "WAL Arrow length decode failed".to_owned(),
                })?,
        ) as usize;
    let data = take(&mut cursor, data_len)?.to_vec();
    if cursor != payload.len() {
        return Err(ScribeError::Internal {
            detail: "WAL v3 slice payload has trailing bytes".to_owned(),
        });
    }
    let table = TableRef::parse_fqn(table_fqn).ok_or_else(|| ScribeError::Internal {
        detail: format!("WAL table FQN is invalid: {table_fqn}"),
    })?;
    let day = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|error| {
        ScribeError::Internal {
            detail: format!("WAL partition day is invalid: {error}"),
        }
    })?;
    Ok(DecodedSlicePayload {
        seal_key: SealKey::new(tenant, table, EventDay::new(day)),
        schema_fingerprint,
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

/// Encode one legacy test fixture that is wrapped into a v3 slice payload.
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
            .map_err(|e| ScribeError::Internal {
                detail: format!("failed to create WAL segment temp file: {e}"),
            })?;

        header
            .write_to(&mut file)
            .map_err(|e| ScribeError::Internal {
                detail: format!("failed to write WAL segment header: {e}"),
            })?;

        file.sync_all().map_err(|e| ScribeError::Internal {
            detail: format!("failed to fsync WAL segment temp file: {e}"),
        })?;

        drop(file);

        std::fs::rename(&tmp_path, path).map_err(|e| ScribeError::Internal {
            detail: format!("failed to rename WAL segment: {e}"),
        })?;

        if let Some(parent) = path.parent() {
            let parent_file = File::open(parent).map_err(|e| ScribeError::Internal {
                detail: format!("failed to open WAL segment parent dir: {e}"),
            })?;
            parent_file.sync_all().map_err(|e| ScribeError::Internal {
                detail: format!("failed to fsync WAL segment parent dir: {e}"),
            })?;
        }

        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(path)
            .map_err(|e| ScribeError::Internal {
                detail: format!("failed to open WAL segment for append: {e}"),
            })?;

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

    fn append_prepared(&self, prepared: &PreparedWalAppend) -> Result<(), ScribeError> {
        let record = prepared.record()?;
        self.append(&record)
    }

    /// Force all appended data for this segment to stable storage.
    pub fn sync_data(&self) -> Result<(), ScribeError> {
        let file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (sync_data)".to_string(),
        })?;
        file.sync_data().map_err(|e| ScribeError::Internal {
            detail: format!("WAL data sync failed: {e}"),
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
        let mut file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (read_records)".to_string(),
        })?;
        file.seek(SeekFrom::Start(SEGMENT_HEADER_SIZE as u64))
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL segment seek failed: {e}"),
            })?;

        let mut records = Vec::new();
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
                    records.push(record);
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

        Ok(records)
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
}

#[derive(Debug, Default)]
struct WalState {
    current_segment: Option<Arc<WalSegment>>,
    seg_seq: u64,
    current_segment_size: u64,
    current_segment_records: u64,
}

/// A WAL handle bound to one seal-key.
#[derive(Debug, Clone)]
pub struct WalHandle {
    writer: Arc<WalWriter>,
    seal_key: SealKey,
    shard_id: u8,
}

impl WalHandle {
    pub(crate) fn new(writer: Arc<WalWriter>, seal_key: SealKey) -> Self {
        let shard_id = u8::try_from(crate::scribe::routing::shard_for(
            seal_key.tenant,
            &seal_key.table,
        ))
        .expect("fixed shard count fits in u8");
        Self {
            writer,
            seal_key,
            shard_id,
        }
    }

    /// The seal-key routed by this handle.
    #[must_use]
    pub fn seal_key(&self) -> &SealKey {
        &self.seal_key
    }

    /// Return the current segment, if this handle has received an append.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "legacy writer lifecycle code is test-only")
    )]
    pub(crate) fn current_segment(&self) -> Result<Option<Arc<WalSegment>>, ScribeError> {
        self.writer.states[usize::from(self.shard_id)]
            .lock()
            .map(|state| state.current_segment.clone())
            .map_err(|_| ScribeError::Internal {
                detail: "WAL state lock poisoned (current_segment)".to_owned(),
            })
    }

    #[cfg(test)]
    pub(crate) fn append_frame(&self, frame: &[u8]) -> Result<WalLsn, ScribeError> {
        self.writer.append_frame_for_key(frame, &self.seal_key)
    }

    pub(crate) fn append_prepared(
        &self,
        append: PreparedWalAppend,
    ) -> Result<WalAppendResult, ScribeError> {
        let mut append = if append.seal_key.is_some() {
            append
        } else {
            append.for_slice(self.seal_key.clone(), [0; 32])
        };
        append.shard_id = Some(self.shard_id);
        self.writer.append_prepared(append)
    }

    pub(crate) fn sync_segments(segments: &[Arc<WalSegment>]) -> Result<(), ScribeError> {
        WalWriter::sync_segments(segments)
    }

    #[cfg(test)]
    pub(crate) fn append_and_fsync(
        &self,
        batch_id: [u8; 16],
        audit_payload: &[u8],
        data_payload: &[u8],
    ) -> Result<WalLsn, ScribeError> {
        self.writer
            .append_and_fsync_for_key(&self.seal_key, batch_id, audit_payload, data_payload)
    }

    /// Append one complete v3 record for deterministic test-tier replay probes.
    pub fn append_and_fsync_for_test(
        &self,
        batch_id: [u8; 16],
        audit_payload: &[u8],
        data_payload: &[u8],
    ) -> Result<WalLsn, ScribeError> {
        self.writer
            .append_and_fsync_for_key(&self.seal_key, batch_id, audit_payload, data_payload)
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "legacy writer lifecycle code is test-only")
    )]
    pub(crate) fn sync_data(&self) -> Result<(), ScribeError> {
        self.writer.sync_data_for_shard(self.shard_id)
    }

    pub(crate) fn retire_segments(&self, segments: &[WalSegmentRef]) -> Result<(), ScribeError> {
        self.writer.retire_segments(segments)
    }
}

impl WalWriter {
    /// Create a new WAL writer for the given seal-key.
    ///
    /// Segment sizing is explicit and validated before the writer is returned.
    pub fn new(
        base_dir: impl AsRef<Path>,
        node_id: [u8; 16],
        writer_epoch: i64,
        _tenant_id: DataTenantId,
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
        let config = WalConfig::new(config.segment_bytes)?;
        if shard_id >= 16 {
            return Err(ScribeError::Internal {
                detail: format!("WAL shard id must be below 16, got {shard_id}"),
            });
        }
        let base_dir = base_dir.as_ref().to_path_buf();
        Ok(Self {
            base_dir,
            node_id,
            writer_epoch,
            shard_id,
            next_lsn: Arc::new(AtomicU64::new(0)),
            segment_bytes: config.segment_bytes,
            states: Arc::new((0..16).map(|_| Mutex::new(WalState::default())).collect()),
        })
    }

    /// Return the pod-local WAL root used for replay and diagnostics.
    #[must_use]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub(crate) fn handle_for_seal_key(&self, key: SealKey) -> WalHandle {
        WalHandle::new(Arc::new(self.clone_for_handle()), key)
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
        }
    }

    /// Open a keyed WAL handle for deterministic test-tier replay probes.
    pub fn handle_for_seal_key_for_test(&self, key: SealKey) -> Result<WalHandle, ScribeError> {
        Ok(self.handle_for_seal_key(key))
    }

    /// Append one unscoped v3 record and fsync.
    pub fn append_and_fsync(
        &self,
        batch_id: [u8; 16],
        audit_payload: &[u8],
        data_payload: &[u8],
    ) -> Result<WalLsn, ScribeError> {
        let day =
            chrono::NaiveDate::from_ymd_opt(1970, 1, 1).ok_or_else(|| ScribeError::Internal {
                detail: "invalid WAL epoch day".to_owned(),
            })?;
        let key = SealKey::new(
            DataTenantId::SYSTEM_OWNER,
            TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "wal"),
            EventDay::new(day),
        );
        self.append_and_fsync_for_key(&key, batch_id, audit_payload, data_payload)
    }

    /// Append one prepared v3 record without syncing it.
    pub(crate) fn append_prepared(
        &self,
        mut prepared: PreparedWalAppend,
    ) -> Result<WalAppendResult, ScribeError> {
        let lsn = WalLsn::new(self.next_lsn.fetch_add(1, Ordering::SeqCst));
        prepared.assign_lsn(lsn);
        let encoded_bytes = u64::try_from(prepared.record()?.encode().len()).map_err(|_| {
            ScribeError::Internal {
                detail: "encoded WAL record length does not fit accounting".to_owned(),
            }
        })?;
        let shard_id = usize::from(prepared.shard_id.unwrap_or(0));
        let mut state = self.states[shard_id]
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "WAL state lock poisoned (append_prepared)".to_owned(),
            })?;
        if state.current_segment.is_some()
            && state.current_segment_records > 0
            && state.current_segment_size.saturating_add(encoded_bytes) > self.segment_bytes()
        {
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
        segment.append_prepared(&prepared)?;
        state.current_segment_size = state.current_segment_size.saturating_add(encoded_bytes);
        state.current_segment_records = state.current_segment_records.saturating_add(1);
        Ok(WalAppendResult {
            lsn,
            touched_segments: vec![segment],
            encoded_bytes,
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

    /// Return the configured segment size.
    #[must_use]
    pub fn segment_bytes(&self) -> u64 {
        self.segment_bytes
    }

    #[cfg(test)]
    #[expect(dead_code, reason = "test-only legacy frame decoder compatibility")]
    /// Append a fixture frame through the reference decoder.
    pub(crate) fn append_frame(&self, frame: &[u8]) -> Result<WalLsn, ScribeError> {
        self.append_frame_for_key(
            frame,
            &SealKey::new(
                DataTenantId::SYSTEM_OWNER,
                TableRef::new(crate::namespaces::BifrostNamespace::Bifrost, "wal"),
                EventDay::new(chrono::NaiveDate::from_ymd_opt(1970, 1, 1).ok_or_else(|| {
                    ScribeError::Internal {
                        detail: "invalid WAL test epoch".to_owned(),
                    }
                })?),
            ),
        )
    }

    #[cfg(test)]
    fn append_frame_for_key(
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
        Self::sync_segments(&result.touched_segments)?;
        Ok(result.lsn)
    }

    #[cfg(any(feature = "bench-support", test))]
    pub(crate) fn sync_data(&self) -> Result<(), ScribeError> {
        self.sync_data_for_shard(0)
    }

    fn sync_data_for_shard(&self, shard_id: u8) -> Result<(), ScribeError> {
        let segment = self.states[usize::from(shard_id)]
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "segment lock poisoned (sync_data)".to_string(),
            })?
            .current_segment
            .clone();
        if let Some(segment) = segment {
            segment.sync_data()?;
        }
        Ok(())
    }

    /// Retire only closed segments. The active segment remains until rollover
    /// makes every record in it eligible for the next persistence transaction.
    pub(crate) fn retire_segments(&self, segments: &[WalSegmentRef]) -> Result<(), ScribeError> {
        let mut active = Vec::new();
        for state in self.states.iter() {
            let state = state.lock().map_err(|_| ScribeError::Internal {
                detail: "WAL state lock poisoned (retire_segments)".to_owned(),
            })?;
            if let Some(segment) = &state.current_segment {
                active.push(segment.path().to_path_buf());
            }
        }
        for segment in segments {
            if active.iter().any(|path| path == &segment.path) {
                continue;
            }
            if segment.path.exists() {
                std::fs::remove_file(&segment.path).map_err(|error| ScribeError::Internal {
                    detail: format!("WAL segment retirement failed: {error}"),
                })?;
                if let Some(parent) = segment.path.parent() {
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
    ///
    /// Current Scribe does not truncate committed WAL segments yet, so this
    /// diagnostic reports retained bytes rather than pretending they are an
    /// active queue depth.
    #[must_use]
    pub fn bytes_on_disk(&self) -> u64 {
        fn directory_bytes(path: &Path) -> u64 {
            let Ok(entries) = std::fs::read_dir(path) else {
                return 0;
            };
            entries
                .filter_map(Result::ok)
                .map(|entry| {
                    let Ok(metadata) = entry.metadata() else {
                        return 0;
                    };
                    if metadata.is_dir() {
                        directory_bytes(&entry.path())
                    } else if metadata.is_file() {
                        metadata.len()
                    } else {
                        0
                    }
                })
                .sum()
        }

        directory_bytes(&self.base_dir)
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
        let shard_dir = self.base_dir.join(format!("shard-{shard_id:02}"));
        std::fs::create_dir_all(&shard_dir).map_err(|error| ScribeError::Internal {
            detail: format!("failed to create WAL shard directory: {error}"),
        })?;
        let path = shard_dir.join(format!("{seq}.wal"));
        let header = SegmentHeader::new(self.node_id, self.writer_epoch, seq, shard_id);

        let segment = Arc::new(WalSegment::create(&path, header)?);
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

        segments.sort_by(|left, right| {
            left.path()
                .cmp(right.path())
                .then_with(|| left.header().seg_seq.cmp(&right.header().seg_seq))
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

    /// Read records while retaining the segment path used to reconstruct its seal-key.
    pub(crate) fn read_all_records_with_paths(
        &self,
    ) -> Result<Vec<(PathBuf, WalRecord)>, ScribeError> {
        let mut records = Vec::new();
        for segment in &self.segments {
            let path = segment.path().to_path_buf();
            for record in segment.read_records()? {
                records.push((path.clone(), record));
            }
        }
        Ok(records)
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
    /// Seal-key this append belongs to.
    pub seal_key: String, // placeholder — real SealKey in seal_key.rs
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

    #[test]
    fn wal_lsn_ordering() {
        assert!(WalLsn::new(1) > WalLsn::ZERO);
        assert!(WalLsn::new(100) > WalLsn::new(99));
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
    fn wal_writer_append_and_read() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = [3u8; 16];
        let tenant_id = crate::test_support::tenant();

        let writer = WalWriter::new(temp_dir.path(), node_id, 1, tenant_id, WalConfig::default())
            .expect("writer");

        let batch_id = [1u8; 16];
        let lsn = writer
            .append_and_fsync(batch_id, b"audit", b"data")
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
        let writer = WalWriter::new(
            temp_dir.path(),
            [8u8; 16],
            1,
            crate::test_support::tenant(),
            WalConfig::default(),
        )
        .expect("writer");
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
        let first_handle = writer.handle_for_seal_key(first);
        let second_handle = writer.handle_for_seal_key(second);
        let frame = encode_append_frame([1u8; 16], b"audit", b"data").expect("frame");

        let first_lsn = first_handle.append_frame(&frame).expect("first append");
        let second_lsn = second_handle.append_frame(&frame).expect("second append");

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
            tenant_id,
            WalConfig { segment_bytes: 500 },
        )
        .expect("writer");

        // Write appends totaling >500 bytes (each record has overhead)
        for i in 0u8..10 {
            let payload = format!("data-{i:03}").repeat(20); // ~100 bytes per payload
            let batch_id = [i; 16];
            writer
                .append_and_fsync(batch_id, payload.as_bytes(), payload.as_bytes())
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
    fn retirement_deletes_only_closed_segments() {
        let temp_dir = TempDir::new().expect("temp dir");
        let writer = WalWriter::new(
            temp_dir.path(),
            [9u8; 16],
            1,
            crate::test_support::tenant(),
            WalConfig { segment_bytes: 500 },
        )
        .expect("writer");
        for index in 0u8..10 {
            writer
                .append_and_fsync([index; 16], b"audit", &[index; 120])
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
    fn wal_segment_roll_is_atomic_across_crash() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = [5u8; 16];
        let tenant_id = crate::test_support::tenant();

        let writer = WalWriter::new(temp_dir.path(), node_id, 1, tenant_id, WalConfig::default())
            .expect("writer");

        // Write 2 appends to segment 0
        let batch_id1 = [1u8; 16];
        let batch_id2 = [2u8; 16];
        writer
            .append_and_fsync(batch_id1, b"audit1", b"data1")
            .expect("append 1");
        writer
            .append_and_fsync(batch_id2, b"audit2", b"data2")
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

        let expected_writer = WalWriter::new(
            temp_dir.path(),
            expected_node,
            3,
            crate::test_support::tenant(),
            WalConfig::default(),
        )
        .expect("expected writer");
        expected_writer
            .append_and_fsync([1u8; 16], b"audit", b"data")
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
    fn wal_append_returns_507_on_enospc() {
        // This test is a placeholder for ENOSPC handling.
        // Real implementation would require mocking the filesystem to return
        // io::ErrorKind::StorageFull.

        // Compile-time check that WalDiskFull variant exists
        match ScribeError::WalDiskFull {
            ScribeError::WalDiskFull => (),
            _ => unreachable!(),
        }

        // TODO: Once wyrd_spec::error::WyrdError derive is available for ScribeError,
        // verify: assert_eq!(ScribeError::WalDiskFull.status(), 507);
    }
}
