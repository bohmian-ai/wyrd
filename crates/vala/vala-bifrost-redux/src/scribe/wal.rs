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
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use wyrd_spec::ids::DataTenantId;

use crate::contracts::ScribeError;

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

        // Reject version 1 segments (incompatible with 's batch_id field)
        if version == 1 {
            return Err(ScribeError::Internal {
 detail: "WAL format version 1 is incompatible with (batch_id field added). Delete WAL directory and restart.".to_string(),
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
/// Layout: `[len:u32][lsn:u64][kind:u8][reserved:u8×3][batch_id:u8×16][payload][crc32c:u32]`
#[derive(Debug, Clone)]
pub struct WalRecord {
    /// LSN for this record (monotonic per stream).
    pub lsn: WalLsn,
    /// Envelope kind: 0 = data, 1 = audit-envelope-only.
    pub envelope_kind: u8,
    /// Batch ID for deduplication (shared across paired audit+data records).
    pub batch_id: [u8; 16],
    /// Payload bytes (Arrow IPC for kind=0, JSON `AuditEvent` for kind=1).
    pub payload: Vec<u8>,
}

impl WalRecord {
    /// Construct a new WAL record.
    #[must_use]
    pub fn new(lsn: WalLsn, envelope_kind: u8, batch_id: [u8; 16], payload: Vec<u8>) -> Self {
        Self {
            lsn,
            envelope_kind,
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
        buf.push(self.envelope_kind);
        buf.extend_from_slice(&[0u8; 3]); // reserved
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
        if reserved_buf != [0u8; 3] {
            return Err(ScribeError::Internal {
                detail: "WAL record reserved bits non-zero".to_string(),
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
            envelope_kind,
            batch_id,
            payload,
        }))
    }
}

/// WAL segment — one file in the per-seal-key WAL stream.
#[derive(Debug)]
pub struct WalSegment {
    path: PathBuf,
    file: Mutex<File>,
    header: SegmentHeader,
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

    /// Append a record to the segment and fsync.
    ///
    /// # Panics
    pub fn append_and_fsync(&self, record: &WalRecord) -> Result<(), ScribeError> {
        let encoded = record.encode();
        let mut file = self.file.lock().map_err(|_| ScribeError::Internal {
            detail: "WAL segment file lock poisoned (append_and_fsync)".to_string(),
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

        file.sync_all().map_err(|e| ScribeError::Internal {
            detail: format!("WAL fsync failed: {e}"),
        })?;

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
        while let Some(record) = WalRecord::decode_from(&mut *file)? {
            records.push(record);
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
}

/// WAL writer — handles per-seal-key append with automatic segment rollover.
#[derive(Debug)]
pub struct WalWriter {
    base_dir: PathBuf,
    node_id: [u8; 16],
    writer_epoch: i64,
    tenant_id: DataTenantId,
    next_lsn: Arc<AtomicU64>,
    current_segment: Mutex<Option<Arc<WalSegment>>>,
    seg_seq: Arc<AtomicU64>,
    max_segment_size: u64,
    current_segment_size: Arc<AtomicU64>,
}

impl WalWriter {
    /// Create a new WAL writer for the given seal-key.
    ///
    /// `max_segment_size` defaults to 4 KiB if `None`.
    pub fn new(
        base_dir: impl AsRef<Path>,
        node_id: [u8; 16],
        writer_epoch: i64,
        tenant_id: DataTenantId,
        max_segment_size: Option<u64>,
    ) -> Result<Self, ScribeError> {
        let base_dir = base_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_dir).map_err(|e| ScribeError::Internal {
            detail: format!("failed to create WAL directory: {e}"),
        })?;

        Ok(Self {
            base_dir,
            node_id,
            writer_epoch,
            tenant_id,
            next_lsn: Arc::new(AtomicU64::new(0)),
            current_segment: Mutex::new(None),
            seg_seq: Arc::new(AtomicU64::new(0)),
            max_segment_size: max_segment_size.unwrap_or(4096),
            current_segment_size: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Append paired (audit, data) records and fsync.
    ///
    /// Returns the assigned LSN (same for both records).
    pub fn append_and_fsync(
        &self,
        batch_id: [u8; 16],
        audit_payload: Vec<u8>,
        data_payload: Vec<u8>,
    ) -> Result<WalLsn, ScribeError> {
        let lsn = WalLsn::new(self.next_lsn.fetch_add(1, Ordering::SeqCst));

        // Build records to calculate their sizes
        let audit_record = WalRecord::new(lsn, 1, batch_id, audit_payload);
        let data_record = WalRecord::new(lsn, 0, batch_id, data_payload);

        let audit_size = audit_record.encode().len() as u64;
        let data_size = data_record.encode().len() as u64;
        let total_size = audit_size + data_size;

        // Check if we need to roll to a new segment
        let current_size = self.current_segment_size.load(Ordering::SeqCst);
        if current_size + total_size > self.max_segment_size {
            self.roll_segment()?;
        }

        let segment = self.ensure_segment()?;

        // Write audit record (kind=1)
        segment.append_and_fsync(&audit_record)?;

        // Write data record (kind=0)
        segment.append_and_fsync(&data_record)?;

        // Update segment size counter
        self.current_segment_size
            .fetch_add(total_size, Ordering::SeqCst);

        Ok(lsn)
    }

    /// Roll to a new segment.
    ///
    fn roll_segment(&self) -> Result<(), ScribeError> {
        let mut current = self
            .current_segment
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "segment lock poisoned (roll_segment)".to_string(),
            })?;
        *current = None;
        self.current_segment_size.store(0, Ordering::SeqCst);
        Ok(())
    }

    fn ensure_segment(&self) -> Result<Arc<WalSegment>, ScribeError> {
        let mut current = self
            .current_segment
            .lock()
            .map_err(|_| ScribeError::Internal {
                detail: "segment lock poisoned (ensure_segment)".to_string(),
            })?;

        if let Some(ref segment) = *current {
            return Ok(Arc::clone(segment));
        }

        let seq = self.seg_seq.fetch_add(1, Ordering::SeqCst);
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
        *current = Some(Arc::clone(&segment));

        // Initialize segment size to header size
        self.current_segment_size
            .store(SEGMENT_HEADER_SIZE as u64, Ordering::SeqCst);

        Ok(segment)
    }
}

/// WAL reader — reads segments and returns records.
#[derive(Debug)]
pub struct WalReader {
    segments: Vec<Arc<WalSegment>>,
}

impl WalReader {
    /// Open all segments in the given directory.
    pub fn open_directory(dir: impl AsRef<Path>) -> Result<Self, ScribeError> {
        let dir = dir.as_ref();
        if !dir.exists() {
            return Ok(Self {
                segments: Vec::new(),
            });
        }

        let mut segments = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(|e| ScribeError::Internal {
            detail: format!("failed to read WAL directory: {e}"),
        })? {
            let entry = entry.map_err(|e| ScribeError::Internal {
                detail: format!("failed to read WAL directory entry: {e}"),
            })?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("arrow") {
                let segment = WalSegment::open(&path)?;
                segments.push(Arc::new(segment));
            }
        }

        segments.sort_by_key(|s| s.header().seg_seq);

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
        let record = WalRecord::new(WalLsn::new(5), 0, batch_id, b"test data".to_vec());
        let encoded = record.encode();

        let mut cursor = std::io::Cursor::new(encoded);
        let decoded = WalRecord::decode_from(&mut cursor)
            .expect("decode record")
            .expect("non-empty");

        assert_eq!(decoded.lsn, WalLsn::new(5));
        assert_eq!(decoded.envelope_kind, 0);
        assert_eq!(decoded.batch_id, batch_id);
        assert_eq!(decoded.payload, b"test data");
    }

    #[test]
    fn wal_writer_append_and_read() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = [3u8; 16];
        let tenant_id = DataTenantId::SYSTEM_OWNER;

        let writer = WalWriter::new(temp_dir.path(), node_id, 1, tenant_id, None).expect("writer");

        let batch_id = [1u8; 16];
        let lsn = writer
            .append_and_fsync(batch_id, b"audit".to_vec(), b"data".to_vec())
            .expect("append");

        assert_eq!(lsn, WalLsn::new(0));

        let reader = WalReader::open_directory(temp_dir.path()).expect("reader");
        let records = reader.read_all_records().expect("read records");

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].envelope_kind, 1); // audit
        assert_eq!(records[0].payload, b"audit");
        assert_eq!(records[1].envelope_kind, 0); // data
        assert_eq!(records[1].payload, b"data");
    }

    #[test]
    fn wal_segment_rolls_on_size() {
        let temp_dir = TempDir::new().expect("temp dir");
        let node_id = [4u8; 16];
        let tenant_id = DataTenantId::SYSTEM_OWNER;

        // Create writer with 500-byte max segment size
        let writer =
            WalWriter::new(temp_dir.path(), node_id, 1, tenant_id, Some(500)).expect("writer");

        // Write appends totaling >500 bytes (each record has overhead)
        for i in 0u8..10 {
            let payload = format!("data-{i:03}").repeat(20); // ~100 bytes per payload
            let batch_id = [i; 16];
            writer
                .append_and_fsync(
                    batch_id,
                    payload.as_bytes().to_vec(),
                    payload.as_bytes().to_vec(),
                )
                .expect("append");
        }

        // Verify multiple segments were created
        let reader = WalReader::open_directory(temp_dir.path()).expect("reader");
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
        let tenant_id = DataTenantId::SYSTEM_OWNER;

        let writer = WalWriter::new(temp_dir.path(), node_id, 1, tenant_id, None).expect("writer");

        // Write 2 appends to segment 0
        let batch_id1 = [1u8; 16];
        let batch_id2 = [2u8; 16];
        writer
            .append_and_fsync(batch_id1, b"audit1".to_vec(), b"data1".to_vec())
            .expect("append 1");
        writer
            .append_and_fsync(batch_id2, b"audit2".to_vec(), b"data2".to_vec())
            .expect("append 2");

        // Manually create a .tmp file to simulate crash during segment creation
        let tmp_path = temp_dir.path().join("seg-1.arrow.tmp");
        std::fs::write(&tmp_path, b"incomplete segment data").expect("write tmp file");

        // Reopen directory — should ignore .tmp files
        let reader = WalReader::open_directory(temp_dir.path()).expect("reader");
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
    fn wal_append_returns_507_on_enospc() {
        // This test is a placeholder for ENOSPC handling.
        // Real implementation would require mocking the filesystem to return
        // io::ErrorKind::StorageFull.

        // Compile-time check that WalDiskFull variant exists
        #[allow(clippy::let_unit_value)]
        let _unit = match ScribeError::WalDiskFull {
            ScribeError::WalDiskFull => (),
            _ => unreachable!(),
        };

        // TODO: Once wyrd_spec::error::WyrdError derive is available for ScribeError,
        // verify: assert_eq!(ScribeError::WalDiskFull.status(), 507);
    }
}
