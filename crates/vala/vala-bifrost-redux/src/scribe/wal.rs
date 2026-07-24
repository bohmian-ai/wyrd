//! Write-Ahead Log (WAL) for Scribe — crash-consistent framed records.
//!
//! Every `ScribeAppend` emits two paired WAL records with the same LSN:
//! - `kind=1`: bincode-serialized `wyrd_spec::vala::api::AuditEvent` (canonical audit type)
//! - `kind=0`: Arrow IPC `RecordBatch` data
//!
//! Segment format per `scribe/00-architecture.md §Crash-consistent WAL format`:
//! - Fixed 64-byte header with magic, version, `node_id`, `writer_epoch`, CRC
//! - Variable-length framed records: `[len][lsn][kind][reserved][batch_id][payload][crc32c]`
//! - Atomic segment rollover: write-fsync-rename-fsync-parent
//! - Torn tail truncation on replay at first CRC/length/monotonicity failure
//!
//! ## WAL Format Version History
//!
//! - **Version 2** (): Added `batch_id` field to record header for deduplication.
//!   Breaking change from version 1 — segments written with version 1 cannot be
//!   replayed by version 2 readers. Delete WAL directory and restart if upgrading.
//! - **Version 1** (): Initial implementation with paired audit/data records.

use std::fs::{File, OpenOptions};
use std::io::{self, IoSlice, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use wyrd_spec::ids::DataTenantId;

use crate::contracts::ScribeError;
use crate::scribe::seal_key::SealKey;
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
    /// Format version (1 for this implementation).
    pub version: u16,
    /// Reserved field (must be zero).
    pub reserved: u16,
    /// Stable pod identifier (uuid).
    pub node_id: [u8; 16],
    /// Writer epoch from `vala.cluster_nodes.fencing_token`.
    pub writer_epoch: i64,
    /// Monotonic segment sequence number per stream.
    pub seg_seq: u64,
    /// Tenant ID for this segment's seal-key.
    pub tenant_id: [u8; 16],
    /// First LSN this segment may hold (u32 to fit 64-byte header).
    pub epoch_start_lsn: u32,
    /// CRC32C over the preceding 60 bytes.
    pub crc32c: u32,
}

const WAL_MAGIC: u32 = 0x5741_5257; // "WRAW"
const WAL_VERSION: u16 = 2;
const SEGMENT_HEADER_SIZE: usize = 64;
pub(crate) const WAL_RECORD_HEADER_SIZE: usize = 40;
const WAL_RECORD_CRC_SIZE: usize = 4;
#[cfg(test)]
const APPEND_FRAME_MAGIC_V2: [u8; 4] = *b"SWF2";
const FRAME_SEQUENCE_RESERVED: [u8; 3] = *b"WYD";

impl SegmentHeader {
    /// Construct a new segment header.
    #[must_use]
    pub fn new(
        node_id: [u8; 16],
        writer_epoch: i64,
        seg_seq: u64,
        tenant_id: [u8; 16],
        epoch_start_lsn: u32,
    ) -> Self {
        let mut header = Self {
            magic: WAL_MAGIC,
            version: WAL_VERSION,
            reserved: 0,
            node_id,
            writer_epoch,
            seg_seq,
            tenant_id,
            epoch_start_lsn,
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
        buf[32..40].copy_from_slice(&self.seg_seq.to_le_bytes());
        buf[40..56].copy_from_slice(&self.tenant_id);
        buf[56..60].copy_from_slice(&self.epoch_start_lsn.to_le_bytes());
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
        let seg_seq = u64::from_le_bytes([
            buf[32], buf[33], buf[34], buf[35], buf[36], buf[37], buf[38], buf[39],
        ]);
        let mut tenant_id = [0u8; 16];
        tenant_id.copy_from_slice(&buf[40..56]);
        let epoch_start_lsn = u32::from_le_bytes([buf[56], buf[57], buf[58], buf[59]]);

        let crc32c = u32::from_le_bytes([buf[60], buf[61], buf[62], buf[63]]);

        let header = Self {
            magic,
            version,
            reserved,
            node_id,
            writer_epoch,
            seg_seq,
            tenant_id,
            epoch_start_lsn,
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
/// Layout: `[len:u32][lsn:u64][kind:u8][reserved:u8×3][batch_id:u8×16][frame_sequence:u64][payload][crc32c:u32]`.
///
#[derive(Debug, Clone)]
pub struct WalRecord {
    /// LSN for this record (monotonic per stream).
    pub lsn: WalLsn,
    /// Envelope kind: 0 = data, 1 = audit-envelope-only.
    pub envelope_kind: u8,
    /// Batch ID for deduplication (shared across paired audit+data records).
    pub batch_id: [u8; 16],
    /// Per-batch frame sequence used for bounded-stream idempotency.
    pub frame_sequence: u64,
    /// Payload bytes (Arrow IPC for kind=0, JSON `AuditEvent` for kind=1).
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
            segment_bytes: 512 * 1024 * 1024,
        }
    }
}

/// A paired audit/data append prepared without allocating an encoded record.
#[derive(Debug)]
pub(crate) struct PreparedWalAppend {
    pub(crate) lsn: WalLsn,
    pub(crate) batch_id: [u8; 16],
    pub(crate) frame_sequence: u64,
    pub(crate) audit: Bytes,
    pub(crate) data: Bytes,
    pub(crate) audit_header: [u8; WAL_RECORD_HEADER_SIZE],
    pub(crate) data_header: [u8; WAL_RECORD_HEADER_SIZE],
    pub(crate) encoded_bytes: u64,
}

/// Result of one append, including every segment whose bytes were touched.
#[derive(Debug)]
pub(crate) struct WalAppendResult {
    pub(crate) lsn: WalLsn,
    pub(crate) touched_segments: Vec<Arc<WalSegment>>,
    pub(crate) encoded_bytes: u64,
}

impl PreparedWalAppend {
    pub(crate) fn new(
        lsn: WalLsn,
        batch_id: [u8; 16],
        frame_sequence: u64,
        audit: Bytes,
        data: Bytes,
    ) -> Result<Self, ScribeError> {
        let audit_header = record_header(lsn, 1, batch_id, frame_sequence, audit.len())?;
        let data_header = record_header(lsn, 0, batch_id, frame_sequence, data.len())?;
        let audit_bytes = u64::try_from(audit.len()).map_err(|_| ScribeError::Internal {
            detail: "audit payload length does not fit WAL accounting".to_owned(),
        })?;
        let data_bytes = u64::try_from(data.len()).map_err(|_| ScribeError::Internal {
            detail: "data payload length does not fit WAL accounting".to_owned(),
        })?;
        let record_overhead = u64::try_from(WAL_RECORD_HEADER_SIZE + WAL_RECORD_CRC_SIZE)
            .expect("WAL record overhead fits u64");
        let encoded_bytes = record_overhead
            .saturating_mul(2)
            .saturating_add(audit_bytes)
            .saturating_add(data_bytes);
        Ok(Self {
            lsn,
            batch_id,
            frame_sequence,
            audit,
            data,
            audit_header,
            data_header,
            encoded_bytes,
        })
    }

    fn assign_lsn(&mut self, lsn: WalLsn) -> Result<(), ScribeError> {
        self.lsn = lsn;
        self.audit_header =
            record_header(lsn, 1, self.batch_id, self.frame_sequence, self.audit.len())?;
        self.data_header =
            record_header(lsn, 0, self.batch_id, self.frame_sequence, self.data.len())?;
        Ok(())
    }
}

fn record_header(
    lsn: WalLsn,
    envelope_kind: u8,
    batch_id: [u8; 16],
    frame_sequence: u64,
    payload_len: usize,
) -> Result<[u8; WAL_RECORD_HEADER_SIZE], ScribeError> {
    let payload_len = u32::try_from(payload_len).map_err(|_| ScribeError::Internal {
        detail: "WAL payload exceeds the v2 record length".to_owned(),
    })?;
    let mut header = [0_u8; WAL_RECORD_HEADER_SIZE];
    header[0..4].copy_from_slice(&payload_len.to_le_bytes());
    header[4..12].copy_from_slice(&lsn.as_u64().to_le_bytes());
    header[12] = envelope_kind;
    header[13..16].copy_from_slice(&FRAME_SEQUENCE_RESERVED);
    header[16..32].copy_from_slice(&batch_id);
    header[32..40].copy_from_slice(&frame_sequence.to_le_bytes());
    Ok(header)
}

impl WalRecord {
    /// Construct a new WAL record.
    #[must_use]
    pub fn new(lsn: WalLsn, envelope_kind: u8, batch_id: [u8; 16], payload: Vec<u8>) -> Self {
        Self {
            lsn,
            envelope_kind,
            batch_id,
            frame_sequence: 0,
            payload,
        }
    }

    /// Construct a record carrying an explicit bounded-stream frame sequence.
    #[must_use]
    pub fn new_with_frame_sequence(
        lsn: WalLsn,
        envelope_kind: u8,
        batch_id: [u8; 16],
        frame_sequence: u64,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            lsn,
            envelope_kind,
            batch_id,
            frame_sequence,
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
        let mut buf = Vec::with_capacity(4 + 8 + 1 + 3 + 16 + 8 + self.payload.len() + 4);

        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(&self.lsn.as_u64().to_le_bytes());
        buf.push(self.envelope_kind);
        buf.extend_from_slice(&FRAME_SEQUENCE_RESERVED);
        buf.extend_from_slice(&self.batch_id);
        buf.extend_from_slice(&self.frame_sequence.to_le_bytes());
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
        if envelope_kind > 1 {
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
        if reserved_buf != FRAME_SEQUENCE_RESERVED {
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

        let mut sequence_bytes = [0u8; 8];
        reader
            .read_exact(&mut sequence_bytes)
            .map_err(|e| ScribeError::Internal {
                detail: format!("WAL record frame sequence read error: {e}"),
            })?;
        let frame_sequence = u64::from_le_bytes(sequence_bytes);

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
        if reserved_buf == FRAME_SEQUENCE_RESERVED {
            crc_input.extend_from_slice(&frame_sequence.to_le_bytes());
        }
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
            envelope_kind,
            batch_id,
            frame_sequence,
            payload,
        }))
    }
}

/// Encode one complete paired audit/data append for the blocking WAL worker.
#[cfg(test)]
pub(crate) fn encode_append_frame(
    batch_id: [u8; 16],
    audit: &[u8],
    data: &[u8],
) -> Result<Bytes, ScribeError> {
    encode_append_frame_with_sequence(batch_id, 0, audit, data)
}

/// Encode one complete paired append with its bounded-stream frame sequence.
#[cfg(test)]
pub(crate) fn encode_append_frame_with_sequence(
    batch_id: [u8; 16],
    frame_sequence: u64,
    audit: &[u8],
    data: &[u8],
) -> Result<Bytes, ScribeError> {
    let audit_len = u32::try_from(audit.len()).map_err(|_| ScribeError::Internal {
        detail: "audit envelope exceeds WAL frame length".to_string(),
    })?;
    let data_len = u32::try_from(data.len()).map_err(|_| ScribeError::Internal {
        detail: "Arrow payload exceeds WAL frame length".to_string(),
    })?;
    let mut frame = Vec::with_capacity(36 + audit.len() + data.len());
    frame.extend_from_slice(&APPEND_FRAME_MAGIC_V2);
    frame.extend_from_slice(&batch_id);
    frame.extend_from_slice(&frame_sequence.to_le_bytes());
    frame.extend_from_slice(&audit_len.to_le_bytes());
    frame.extend_from_slice(&data_len.to_le_bytes());
    frame.extend_from_slice(audit);
    frame.extend_from_slice(data);
    Ok(Bytes::from(frame))
}

#[cfg(test)]
struct DecodedAppendFrame<'a> {
    batch_id: [u8; 16],
    frame_sequence: u64,
    audit: &'a [u8],
    data: &'a [u8],
}

#[cfg(test)]
fn decode_append_frame(frame: &[u8]) -> Result<DecodedAppendFrame<'_>, ScribeError> {
    if frame.len() < 36 || frame[0..4] != APPEND_FRAME_MAGIC_V2 {
        return Err(ScribeError::Internal {
            detail: "invalid Scribe WAL append frame".to_string(),
        });
    }
    let mut batch_id = [0_u8; 16];
    batch_id.copy_from_slice(&frame[4..20]);
    let mut sequence_bytes = [0_u8; 8];
    sequence_bytes.copy_from_slice(&frame[20..28]);
    let frame_sequence = u64::from_le_bytes(sequence_bytes);
    let lengths_start = 28;
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
        frame_sequence,
        audit: &frame[payload_start..payload_start + audit_len],
        data: &frame[payload_start + audit_len..end],
    })
}

/// WAL segment — one file in the per-seal-key WAL stream.
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
        let mut file = File::open(path).map_err(|e| ScribeError::Internal {
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
        let mut file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (append_prepared)".to_owned(),
        })?;
        write_record_vectored(&mut file, &prepared.audit_header, &prepared.audit)?;
        write_record_vectored(&mut file, &prepared.data_header, &prepared.data)
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
        loop {
            let record_offset = file
                .stream_position()
                .map_err(|error| ScribeError::Internal {
                    detail: format!("WAL record position failed: {error}"),
                })?;
            match WalRecord::decode_from(&mut *file) {
                Ok(Some(record)) => records.push(record),
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

fn write_record_vectored(
    file: &mut File,
    header: &[u8; WAL_RECORD_HEADER_SIZE],
    payload: &Bytes,
) -> Result<(), ScribeError> {
    let mut crc = crc32c::crc32c_append(0, header);
    crc = crc32c::crc32c_append(crc, payload);
    let crc_bytes = crc.to_le_bytes();
    let mut remaining: Vec<&[u8]> = vec![header, payload, &crc_bytes];
    while !remaining.is_empty() {
        let parts: Vec<IoSlice<'_>> = remaining.iter().map(|part| IoSlice::new(part)).collect();
        let mut written = file.write_vectored(&parts).map_err(|error| {
            if error.kind() == io::ErrorKind::StorageFull || error.raw_os_error() == Some(28) {
                ScribeError::WalDiskFull
            } else {
                ScribeError::Internal {
                    detail: format!("WAL record vectored write failed: {error}"),
                }
            }
        })?;
        if written == 0 {
            return Err(ScribeError::Internal {
                detail: "WAL vectored write made no progress".to_owned(),
            });
        }
        while written > 0 && !remaining.is_empty() {
            let first_len = remaining[0].len();
            if written >= first_len {
                written -= first_len;
                remaining.remove(0);
            } else {
                remaining[0] = &remaining[0][written..];
                break;
            }
        }
    }
    Ok(())
}

/// WAL writer — handles per-seal-key append with automatic segment rollover.
#[derive(Debug)]
pub struct WalWriter {
    base_dir: PathBuf,
    node_id: [u8; 16],
    writer_epoch: i64,
    tenant_id: DataTenantId,
    next_lsn: Arc<AtomicU64>,
    segment_bytes: u64,
    state: Mutex<WalState>,
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
}

impl WalHandle {
    pub(crate) fn new(writer: Arc<WalWriter>, seal_key: SealKey) -> Self {
        Self { writer, seal_key }
    }

    /// The seal-key routed by this handle.
    #[must_use]
    pub fn seal_key(&self) -> &SealKey {
        &self.seal_key
    }

    /// Return the current segment, if this handle has received an append.
    pub(crate) fn current_segment(&self) -> Result<Option<Arc<WalSegment>>, ScribeError> {
        self.writer
            .state
            .lock()
            .map(|state| state.current_segment.clone())
            .map_err(|_| ScribeError::Internal {
                detail: "WAL state lock poisoned (current_segment)".to_owned(),
            })
    }

    #[cfg(test)]
    pub(crate) fn append_frame(&self, frame: &[u8]) -> Result<WalLsn, ScribeError> {
        self.writer.append_frame(frame)
    }

    pub(crate) fn append_prepared(
        &self,
        append: PreparedWalAppend,
    ) -> Result<WalAppendResult, ScribeError> {
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
            .append_and_fsync(batch_id, audit_payload, data_payload)
    }

    /// Append paired records for deterministic test-tier replay probes.
    pub fn append_and_fsync_for_test(
        &self,
        batch_id: [u8; 16],
        audit_payload: &[u8],
        data_payload: &[u8],
    ) -> Result<WalLsn, ScribeError> {
        self.writer
            .append_and_fsync(batch_id, audit_payload, data_payload)
    }

    pub(crate) fn sync_data(&self) -> Result<(), ScribeError> {
        self.writer.sync_data()
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
        tenant_id: DataTenantId,
        config: WalConfig,
    ) -> Result<Self, ScribeError> {
        let config = WalConfig::new(config.segment_bytes)?;
        let base_dir = base_dir.as_ref().to_path_buf();
        Ok(Self {
            base_dir,
            node_id,
            writer_epoch,
            tenant_id,
            next_lsn: Arc::new(AtomicU64::new(0)),
            segment_bytes: config.segment_bytes,
            state: Mutex::new(WalState::default()),
        })
    }

    /// Return the pod-local WAL root used for replay and diagnostics.
    #[must_use]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub(crate) fn handle_for_seal_key(&self, key: SealKey) -> Result<WalHandle, ScribeError> {
        let base_dir = self.base_dir.join(key.as_path_components());
        let mut writer = Self::new(
            base_dir,
            self.node_id,
            self.writer_epoch,
            key.tenant,
            WalConfig {
                segment_bytes: self.segment_bytes(),
            },
        )?;
        // LSNs identify the pod's WAL stream, not an individual tenant/table/day
        // directory. Keep allocation global across all lazy seal-key handles so
        // file-list replay keys cannot collide between independent writers.
        writer.next_lsn = Arc::clone(&self.next_lsn);
        Ok(WalHandle::new(Arc::new(writer), key))
    }

    /// Open a keyed WAL handle for deterministic test-tier replay probes.
    pub fn handle_for_seal_key_for_test(&self, key: SealKey) -> Result<WalHandle, ScribeError> {
        self.handle_for_seal_key(key)
    }

    /// Append paired (audit, data) records and fsync.
    ///
    /// Returns the assigned LSN (same for both records).
    pub fn append_and_fsync(
        &self,
        batch_id: [u8; 16],
        audit_payload: &[u8],
        data_payload: &[u8],
    ) -> Result<WalLsn, ScribeError> {
        let prepared = PreparedWalAppend::new(
            WalLsn::ZERO,
            batch_id,
            0,
            Bytes::copy_from_slice(audit_payload),
            Bytes::copy_from_slice(data_payload),
        )?;
        let result = self.append_prepared(prepared)?;
        Self::sync_segments(&result.touched_segments)?;
        Ok(result.lsn)
    }

    /// Append one prepared paired record without syncing it.
    pub(crate) fn append_prepared(
        &self,
        mut prepared: PreparedWalAppend,
    ) -> Result<WalAppendResult, ScribeError> {
        let lsn = WalLsn::new(self.next_lsn.fetch_add(1, Ordering::SeqCst));
        prepared.assign_lsn(lsn)?;
        let mut state = self.state.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL state lock poisoned (append_prepared)".to_owned(),
        })?;
        if state.current_segment.is_some()
            && state.current_segment_records > 0
            && state
                .current_segment_size
                .saturating_add(prepared.encoded_bytes)
                > self.segment_bytes()
        {
            state.current_segment = None;
            state.current_segment_size = 0;
            state.current_segment_records = 0;
        }
        let segment = self.ensure_segment_locked(&mut state)?;
        segment.append_prepared(&prepared)?;
        state.current_segment_size = state
            .current_segment_size
            .saturating_add(prepared.encoded_bytes);
        state.current_segment_records = state.current_segment_records.saturating_add(2);
        Ok(WalAppendResult {
            lsn,
            touched_segments: vec![segment],
            encoded_bytes: prepared.encoded_bytes,
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
    /// Append a fixture frame through the reference decoder.
    pub(crate) fn append_frame(&self, frame: &[u8]) -> Result<WalLsn, ScribeError> {
        let decoded = decode_append_frame(frame)?;
        let prepared = PreparedWalAppend::new(
            WalLsn::ZERO,
            decoded.batch_id,
            decoded.frame_sequence,
            Bytes::copy_from_slice(decoded.audit),
            Bytes::copy_from_slice(decoded.data),
        )?;
        Ok(self.append_prepared(prepared)?.lsn)
    }

    pub(crate) fn sync_data(&self) -> Result<(), ScribeError> {
        let segment = self
            .state
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

    fn ensure_segment_locked(&self, state: &mut WalState) -> Result<Arc<WalSegment>, ScribeError> {
        if let Some(ref segment) = state.current_segment {
            return Ok(Arc::clone(segment));
        }

        let seq = state.seg_seq;
        state.seg_seq = state.seg_seq.saturating_add(1);
        std::fs::create_dir_all(&self.base_dir).map_err(|error| ScribeError::Internal {
            detail: format!("failed to create WAL directory: {error}"),
        })?;
        let path = self.base_dir.join(format!("seg-{seq}.arrow"));
        #[expect(
            clippy::cast_possible_truncation,
            reason = "epoch_start_lsn is u32 per 64-byte header constraint"
        )]
        let epoch_start_lsn = self.next_lsn.load(Ordering::SeqCst) as u32;

        let header = SegmentHeader::new(
            self.node_id,
            self.writer_epoch,
            seq,
            *self.tenant_id.as_uuid().as_bytes(),
            epoch_start_lsn,
        );

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
            if path.extension().and_then(|s| s.to_str()) == Some("arrow") {
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
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("arrow") {
            paths.push(path);
        }
    }
    Ok(())
}

/// Scribe-local metadata derived from WAL records and paired data.
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
        let tenant_id = [2u8; 16];
        let header = SegmentHeader::new(node_id, 42, 7, tenant_id, 1000);

        let mut encoded = header.encode();
        encoded[60..64].copy_from_slice(&header.crc32c.to_le_bytes());

        let decoded = SegmentHeader::decode(&encoded).expect("decode header");
        assert_eq!(decoded.node_id, node_id);
        assert_eq!(decoded.writer_epoch, 42);
        assert_eq!(decoded.seg_seq, 7);
        assert_eq!(decoded.epoch_start_lsn, 1000);
    }

    #[test]
    fn wal_record_roundtrip() {
        let batch_id = [42u8; 16];
        let record = WalRecord::new_with_frame_sequence(
            WalLsn::new(5),
            0,
            batch_id,
            7,
            b"test data".to_vec(),
        );
        let encoded = record.encode();

        let mut cursor = std::io::Cursor::new(encoded);
        let decoded = WalRecord::decode_from(&mut cursor)
            .expect("decode record")
            .expect("non-empty");

        assert_eq!(decoded.lsn, WalLsn::new(5));
        assert_eq!(decoded.envelope_kind, 0);
        assert_eq!(decoded.batch_id, batch_id);
        assert_eq!(decoded.frame_sequence, 7);
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

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].envelope_kind, 1); // audit
        assert_eq!(records[0].payload, b"audit");
        assert_eq!(records[1].envelope_kind, 0); // data
        assert_eq!(records[1].payload, b"data");
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
        let first_handle = writer.handle_for_seal_key(first).expect("first handle");
        let second_handle = writer.handle_for_seal_key(second).expect("second handle");
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
        assert_eq!(records.len(), 20); // 10 appends × 2 records each

        // Verify seg_seq increments
        let seg0_header = &reader.segments[0].header;
        let seg1_header = &reader.segments[1].header;
        assert_eq!(seg1_header.seg_seq, seg0_header.seg_seq + 1);
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
        assert_eq!(records.len(), 4); // 2 appends × 2 records each

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

        let foreign_segment_path = temp_dir.path().join("foreign.arrow");
        let foreign_segment = WalSegment::create(
            &foreign_segment_path,
            SegmentHeader::new(
                foreign_node,
                9,
                0,
                *crate::test_support::tenant().as_uuid().as_bytes(),
                0,
            ),
        )
        .expect("foreign segment");
        foreign_segment
            .append_and_fsync(&WalRecord::new(
                WalLsn::new(0),
                0,
                [2u8; 16],
                b"foreign".to_vec(),
            ))
            .expect("foreign append");

        let reader =
            WalReader::open_directory(temp_dir.path(), expected_stream).expect("filtered reader");
        assert_eq!(
            reader.read_all_records().expect("filtered records").len(),
            2,
            "the expected stream's audit and data records remain readable"
        );

        let unfiltered =
            WalReader::open_directory_unfiltered(temp_dir.path()).expect("unfiltered reader");
        assert_eq!(
            unfiltered
                .read_all_records()
                .expect("unfiltered records")
                .len(),
            3,
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
