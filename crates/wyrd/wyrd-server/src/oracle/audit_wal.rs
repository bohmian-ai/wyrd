//! Durable, tenant-scoped WAL used to accept Oracle read audit decisions.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crc32c::crc32c;
use rustix::fs::{FlockOperation, flock};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Notify, mpsc, oneshot};
use tokio::task::JoinHandle;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;
use wyrd_spec::vala::error::BifrostError;

/// Fixed frame magic identifying Oracle audit WAL segments.
const MAGIC: &[u8; 8] = b"WYRDAUD1";
/// Version of the big-endian frame layout.
const VERSION: u16 = 1;
/// Maximum segment size before rotating to a new first-LSN file.
const SEGMENT_LIMIT: u64 = 64 * 1024 * 1024;
/// Bytes through the tenant-length field in one frame.
const HEADER_LEN: usize = 8 + 2 + 8 + 8 + 4;
/// Bytes occupied by the trailing CRC32C.
const TRAILER_LEN: usize = 4;

/// Validated bounds for the Oracle audit WAL and relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OracleAuditWalConfig {
    /// Root directory containing the lock, checkpoint, and segments.
    pub audit_wal_root: Option<PathBuf>,
    /// Maximum number of accepted but unrelayed records.
    pub audit_wal_max_records: usize,
    /// Maximum bytes occupied by accepted but unrelayed records.
    pub audit_wal_max_bytes: u64,
    /// Maximum age of the oldest unrelayed record.
    pub audit_wal_max_age_seconds: u64,
    /// Number of records the relay attempts in one pass.
    pub audit_relay_batch_records: usize,
    /// Per-event Postgres attempt timeout.
    pub audit_relay_attempt_timeout_ms: u64,
    /// Initial retry backoff.
    pub audit_relay_backoff_initial_ms: u64,
    /// Maximum retry backoff.
    pub audit_relay_backoff_max_ms: u64,
    /// Maximum shutdown drain duration.
    pub audit_relay_shutdown_timeout_ms: u64,
}

impl Default for OracleAuditWalConfig {
    /// Supplies the bounded D64 defaults.
    fn default() -> Self {
        Self {
            audit_wal_root: None,
            audit_wal_max_records: 100_000,
            audit_wal_max_bytes: 1 << 30,
            audit_wal_max_age_seconds: 300,
            audit_relay_batch_records: 128,
            audit_relay_attempt_timeout_ms: 5_000,
            audit_relay_backoff_initial_ms: 50,
            audit_relay_backoff_max_ms: 5_000,
            audit_relay_shutdown_timeout_ms: 10_000,
        }
    }
}

/// One decoded, durably accepted Oracle audit event.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AuditWalRecord {
    /// Root-scoped monotonically increasing log sequence number.
    pub lsn: u64,
    /// Unix epoch timestamp captured before the frame was written.
    pub accepted_at_micros: i64,
    /// Tenant owning the eventual outbox row.
    pub tenant: DataTenantId,
    /// Existing canonical audit event payload.
    pub event: AuditEvent,
}

/// Recoverable local audit log with an exclusive root lock.
pub(crate) struct AuditWal {
    /// Root directory retained for segment and checkpoint operations.
    root: PathBuf,
    /// Exclusive kernel lock held for publisher lifetime.
    _lock: File,
    /// Last checkpoint durably committed to disk.
    checkpoint: u64,
    /// Highest fsynced frame LSN.
    high_water: u64,
    /// Recovered records in LSN order.
    records: Vec<AuditWalRecord>,
    /// Total bytes represented by recovered segments.
    bytes: u64,
    /// Capacity and age bounds used on append.
    config: OracleAuditWalConfig,
    /// Segment receiving the next append.
    segment: PathBuf,
}

impl AuditWal {
    /// Opens, locks, recovers, and validates one WAL root.
    ///
    /// # Errors
    /// Returns `QueryAuditUnavailable` when locking, framing, checkpoint,
    /// corruption, truncation, or filesystem durability checks fail.
    pub(crate) fn recover(
        root: &Path,
        config: &OracleAuditWalConfig,
    ) -> Result<Self, BifrostError> {
        fs::create_dir_all(root).map_err(|_| BifrostError::QueryAuditUnavailable)?;
        sync_dir(root)?;
        let lock_path = root.join(".oracle-audit.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        flock(&lock, FlockOperation::NonBlockingLockExclusive)
            .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        let checkpoint = read_checkpoint(root)?;
        let mut paths = fs::read_dir(root)
            .map_err(|_| BifrostError::QueryAuditUnavailable)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("audit-") && n.ends_with(".wal"))
            })
            .collect::<Vec<_>>();
        paths.sort();
        let mut records = Vec::new();
        let mut bytes = 0u64;
        for (index, path) in paths.iter().enumerate() {
            let final_segment = index + 1 == paths.len();
            let (mut decoded, consumed, file_len) =
                decode_segment(path, final_segment, config.audit_wal_max_bytes)?;
            if consumed < file_len {
                let file = OpenOptions::new()
                    .write(true)
                    .open(path)
                    .map_err(|_| BifrostError::QueryAuditUnavailable)?;
                file.set_len(consumed)
                    .map_err(|_| BifrostError::QueryAuditUnavailable)?;
                file.sync_all()
                    .map_err(|_| BifrostError::QueryAuditUnavailable)?;
            }
            bytes = bytes.saturating_add(consumed);
            records.append(&mut decoded);
        }
        records.sort_by_key(|record| record.lsn);
        if records
            .first()
            .is_some_and(|record| record.lsn > checkpoint.saturating_add(1))
        {
            return Err(BifrostError::QueryAuditUnavailable);
        }
        for pair in records.windows(2) {
            if pair[1].lsn != pair[0].lsn.saturating_add(1) {
                return Err(BifrostError::QueryAuditUnavailable);
            }
        }
        if checkpoint > records.last().map_or(0, |record| record.lsn) {
            return Err(BifrostError::QueryAuditUnavailable);
        }
        let segment = paths
            .last()
            .cloned()
            .unwrap_or_else(|| root.join("audit-00000000000000000001.wal"));
        Ok(Self {
            root: root.to_owned(),
            _lock: lock,
            checkpoint,
            high_water: records.last().map_or(0, |record| record.lsn),
            records,
            bytes,
            config: config.clone(),
            segment,
        })
    }

    /// Appends one record, fsyncs it, and returns the assigned LSN.
    ///
    /// # Errors
    /// Returns `QueryAuditUnavailable` when bounds, serialization, write, or
    /// sync operations reject the frame.
    #[cfg(test)]
    pub(crate) fn append(
        &mut self,
        tenant: DataTenantId,
        event: AuditEvent,
    ) -> Result<u64, BifrostError> {
        self.append_group(vec![(tenant, event)])?
            .into_iter()
            .next()
            .ok_or(BifrostError::QueryAuditUnavailable)
    }

    /// Appends a bounded group with one shared file and directory sync.
    ///
    /// # Errors
    /// Returns `QueryAuditUnavailable` when any frame exceeds capacity or the
    /// grouped write/sync fails. A failed group acknowledges no frame and does
    /// not advance the in-memory high-water mark.
    pub(crate) fn append_group(
        &mut self,
        entries: Vec<(DataTenantId, AuditEvent)>,
    ) -> Result<Vec<u64>, BifrostError> {
        if entries.is_empty() || entries.len() > 128 {
            return Err(BifrostError::QueryAuditUnavailable);
        }
        let initial_len = self.current_segment_len()?;
        let mut records = Vec::with_capacity(entries.len());
        let mut encoded = Vec::new();
        let mut next_lsn = self.high_water;
        for (tenant, event) in entries {
            let accepted_at_micros = epoch_micros(SystemTime::now());
            next_lsn = next_lsn.saturating_add(1);
            let record = AuditWalRecord {
                lsn: next_lsn,
                accepted_at_micros,
                tenant,
                event,
            };
            let frame = encode_record(&record)?;
            encoded.push(frame);
            records.push(record);
        }
        let pending_records = self
            .records
            .iter()
            .filter(|record| record.lsn > self.checkpoint)
            .count();
        let pending_bytes = self.pending_bytes();
        let total_bytes = encoded.iter().try_fold(0u64, |sum, frame| {
            sum.checked_add(
                u64::try_from(frame.len()).map_err(|_| BifrostError::QueryAuditUnavailable)?,
            )
            .ok_or(BifrostError::QueryAuditUnavailable)
        })?;
        if pending_records.saturating_add(records.len()) > self.config.audit_wal_max_records
            || pending_bytes.saturating_add(total_bytes) > self.config.audit_wal_max_bytes
            || self
                .oldest_unrelayed_age(SystemTime::now())
                .is_some_and(|age| age > Duration::from_secs(self.config.audit_wal_max_age_seconds))
        {
            return Err(BifrostError::QueryAuditUnavailable);
        }
        if initial_len.saturating_add(total_bytes) > SEGMENT_LIMIT {
            self.segment = self.root.join(format!("audit-{:020}.wal", records[0].lsn));
            File::create(&self.segment)
                .map_err(|_| BifrostError::QueryAuditUnavailable)?
                .sync_all()
                .map_err(|_| BifrostError::QueryAuditUnavailable)?;
            sync_dir(&self.root)?;
        }
        let write_start = self.current_segment_len()?;
        let result = (|| {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.segment)
                .map_err(|_| BifrostError::QueryAuditUnavailable)?;
            for frame in &encoded {
                file.write_all(frame)
                    .map_err(|_| BifrostError::QueryAuditUnavailable)?;
            }
            file.sync_all()
                .map_err(|_| BifrostError::QueryAuditUnavailable)?;
            sync_dir(&self.root)?;
            Ok::<(), BifrostError>(())
        })();
        if let Err(error) = result {
            if let Ok(file) = OpenOptions::new().write(true).open(&self.segment) {
                let _ = file.set_len(write_start);
                let _ = file.sync_all();
            }
            return Err(error);
        }
        self.bytes = self.bytes.saturating_add(total_bytes);
        self.high_water = next_lsn;
        let lsns = records.iter().map(|record| record.lsn).collect();
        self.records.extend(records);
        Ok(lsns)
    }
    /// Advances the durable checkpoint after a committed Postgres append.
    ///
    /// # Errors
    /// Returns `QueryAuditUnavailable` when the LSN is not pending or the
    /// temp-write, rename, directory sync, or reclamation fails.
    pub(crate) fn checkpoint(&mut self, lsn: u64) -> Result<(), BifrostError> {
        if lsn <= self.checkpoint || lsn > self.high_water {
            return Err(BifrostError::QueryAuditUnavailable);
        }
        let path = self.root.join("checkpoint");
        let temp = self.root.join("checkpoint.tmp");
        let mut file = File::create(&temp).map_err(|_| BifrostError::QueryAuditUnavailable)?;
        file.write_all(format!("{lsn}\n").as_bytes())
            .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        file.sync_all()
            .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        fs::rename(temp, path).map_err(|_| BifrostError::QueryAuditUnavailable)?;
        sync_dir(&self.root)?;
        self.checkpoint = lsn;
        self.reclaim_segments()?;
        Ok(())
    }

    /// Returns the oldest accepted age strictly above the checkpoint.
    pub(crate) fn oldest_unrelayed_age(&self, now: SystemTime) -> Option<Duration> {
        self.records
            .iter()
            .find(|record| record.lsn > self.checkpoint)
            .map(|record| {
                now.duration_since(UNIX_EPOCH)
                    .ok()
                    .and_then(|duration| {
                        let now_micros = i64::try_from(duration.as_micros()).ok()?;
                        let delta = now_micros.saturating_sub(record.accepted_at_micros);
                        Some(Duration::from_micros(
                            u64::try_from(delta.max(0)).unwrap_or(u64::MAX),
                        ))
                    })
                    .unwrap_or_default()
            })
    }

    /// Returns a stable view of pending records for the relay.
    pub(crate) fn pending(&self, limit: usize) -> Vec<AuditWalRecord> {
        self.records
            .iter()
            .filter(|record| record.lsn > self.checkpoint)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Returns records, bytes, and oldest age for test-support inspection.
    pub(crate) fn snapshot(&self) -> (u64, u64, Option<Duration>) {
        (
            self.records
                .iter()
                .filter(|record| record.lsn > self.checkpoint)
                .count() as u64,
            self.pending_bytes(),
            self.oldest_unrelayed_age(SystemTime::now()),
        )
    }

    /// Computes encoded bytes for records above the durable checkpoint.
    fn pending_bytes(&self) -> u64 {
        self.records
            .iter()
            .filter(|record| record.lsn > self.checkpoint)
            .filter_map(|record| encode_record(record).ok())
            .map(|bytes| bytes.len() as u64)
            .sum()
    }

    /// Reads the active segment length before rotation.
    fn current_segment_len(&self) -> Result<u64, BifrostError> {
        fs::metadata(&self.segment)
            .map(|metadata| metadata.len())
            .or_else(|error| {
                if error.kind() == ErrorKind::NotFound {
                    Ok(0)
                } else {
                    Err(BifrostError::QueryAuditUnavailable)
                }
            })
    }

    /// Removes only complete segments covered by the durable checkpoint.
    fn reclaim_segments(&mut self) -> Result<(), BifrostError> {
        let entries = fs::read_dir(&self.root).map_err(|_| BifrostError::QueryAuditUnavailable)?;
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with("audit-") || !name.ends_with(".wal") || path == self.segment {
                continue;
            }
            let Ok((records, _, _)) = decode_segment(&path, false, self.config.audit_wal_max_bytes)
            else {
                continue;
            };
            if records
                .last()
                .is_some_and(|record| record.lsn <= self.checkpoint)
            {
                fs::remove_file(path).map_err(|_| BifrostError::QueryAuditUnavailable)?;
            }
        }
        sync_dir(&self.root)
    }
}

/// One bounded request sent to the sole WAL writer.
pub(crate) struct AuditWalAppendCommand {
    /// Tenant stamped into the accepted frame.
    pub tenant: DataTenantId,
    /// Canonical event payload.
    pub event: AuditEvent,
    /// Completion acknowledgement sent only after group sync.
    pub ack: oneshot::Sender<Result<u64, BifrostError>>,
}

/// Sole owner task for grouped, durably acknowledged WAL appends.
pub(crate) struct AuditWalWriter {
    /// Shared handle retained for the relay's read/checkpoint seam.
    wal: std::sync::Arc<Mutex<AuditWal>>,
    /// Bounded ingress receiver.
    receiver: mpsc::Receiver<AuditWalAppendCommand>,
    /// One-shot test fault that fails the next complete group.
    fail_next_group: std::sync::Arc<AtomicBool>,
    /// Testable lifecycle controls and the durable-sync counter.
    control: std::sync::Arc<AuditWalWriterControl>,
}

/// Controls the sole writer task and records completed group syncs.
pub(crate) struct AuditWalWriterControl {
    /// Prevents the writer from receiving the next group while a test fills ingress.
    pub(crate) paused: AtomicBool,
    /// Resumes a writer paused at the receive seam.
    pub(crate) notify: Notify,
    /// Number of groups whose append and filesystem sync completed.
    pub(crate) sync_count: AtomicU64,
}

#[cfg(feature = "test-support")]
/// Guard that pauses the production writer at its bounded ingress seam.
pub struct AuditWalWriterPauseGuard {
    /// Shared control resumed when the guard is dropped.
    pub(crate) control: std::sync::Arc<AuditWalWriterControl>,
}

#[cfg(feature = "test-support")]
impl Drop for AuditWalWriterPauseGuard {
    /// Resumes the writer and wakes its receive loop.
    fn drop(&mut self) {
        self.control.paused.store(false, Ordering::Release);
        self.control.notify.notify_waiters();
    }
}

impl AuditWalWriter {
    /// Starts the bounded writer and returns its `try_send` ingress.
    pub(crate) fn spawn(
        wal: std::sync::Arc<Mutex<AuditWal>>,
        capacity: usize,
    ) -> (
        mpsc::Sender<AuditWalAppendCommand>,
        std::sync::Arc<AtomicBool>,
        std::sync::Arc<AuditWalWriterControl>,
        JoinHandle<()>,
    ) {
        let (sender, receiver) = mpsc::channel(capacity.max(1));
        let fail_next_group = std::sync::Arc::new(AtomicBool::new(false));
        let control = std::sync::Arc::new(AuditWalWriterControl {
            paused: AtomicBool::new(false),
            notify: Notify::new(),
            sync_count: AtomicU64::new(0),
        });
        let writer = Self {
            wal,
            receiver,
            fail_next_group: std::sync::Arc::clone(&fail_next_group),
            control: std::sync::Arc::clone(&control),
        };
        let handle = tokio::spawn(writer.run());
        (sender, fail_next_group, control, handle)
    }

    /// Drains at most 128 commands or one millisecond of immediately available work.
    async fn run(mut self) {
        loop {
            while self.control.paused.load(Ordering::Acquire) {
                let notified = self.control.notify.notified();
                if !self.control.paused.load(Ordering::Acquire) {
                    break;
                }
                notified.await;
            }
            let Some(first) = self.receiver.recv().await else {
                break;
            };
            let mut group = vec![first];
            let deadline = tokio::time::Instant::now() + Duration::from_millis(1);
            while group.len() < 128 {
                match tokio::time::timeout_at(deadline, self.receiver.recv()).await {
                    Ok(Some(command)) => group.push(command),
                    _ => break,
                }
            }
            let fail_group = self.fail_next_group.swap(false, Ordering::AcqRel);
            let wal = std::sync::Arc::clone(&self.wal);
            let entries = group
                .iter()
                .map(|command| (command.tenant, command.event.clone()))
                .collect::<Vec<_>>();
            let result = tokio::task::spawn_blocking(move || {
                if fail_group {
                    return Err(BifrostError::QueryAuditUnavailable);
                }
                wal.blocking_lock().append_group(entries)
            })
            .await
            .map_err(|_| BifrostError::QueryAuditUnavailable)
            .and_then(|result| result);
            match result {
                Ok(lsns) if lsns.len() == group.len() => {
                    self.control.sync_count.fetch_add(1, Ordering::AcqRel);
                    for (command, lsn) in group.into_iter().zip(lsns) {
                        let _ = command.ack.send(Ok(lsn));
                    }
                }
                Ok(_) | Err(_) => {
                    for command in group {
                        let _ = command.ack.send(Err(BifrostError::QueryAuditUnavailable));
                    }
                }
            }
        }
    }
}

/// Encodes one record using the versioned wire layout and CRC trailer.
fn encode_record(record: &AuditWalRecord) -> Result<Vec<u8>, BifrostError> {
    let tenant = record.tenant.to_string();
    let payload =
        serde_json::to_vec(&record.event).map_err(|_| BifrostError::QueryAuditUnavailable)?;
    let tenant_len =
        u32::try_from(tenant.len()).map_err(|_| BifrostError::QueryAuditUnavailable)?;
    let payload_len =
        u32::try_from(payload.len()).map_err(|_| BifrostError::QueryAuditUnavailable)?;
    let mut bytes = Vec::with_capacity(HEADER_LEN + tenant.len() + payload.len() + TRAILER_LEN);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.extend_from_slice(&record.lsn.to_be_bytes());
    bytes.extend_from_slice(&record.accepted_at_micros.to_be_bytes());
    bytes.extend_from_slice(&tenant_len.to_be_bytes());
    bytes.extend_from_slice(tenant.as_bytes());
    bytes.extend_from_slice(&payload_len.to_be_bytes());
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(&crc32c(&bytes).to_be_bytes());
    Ok(bytes)
}

/// Decodes one segment, optionally accepting only an incomplete final frame.
fn decode_segment(
    path: &Path,
    allow_torn_tail: bool,
    max_component_bytes: u64,
) -> Result<(Vec<AuditWalRecord>, u64, u64), BifrostError> {
    let mut file = File::open(path).map_err(|_| BifrostError::QueryAuditUnavailable)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| BifrostError::QueryAuditUnavailable)?;
    let file_len = u64::try_from(bytes.len()).map_err(|_| BifrostError::QueryAuditUnavailable)?;
    let mut offset = 0usize;
    let mut records = Vec::new();
    while offset < bytes.len() {
        let start = offset;
        if bytes.len() - offset < HEADER_LEN {
            if allow_torn_tail {
                break;
            }
            return Err(BifrostError::QueryAuditUnavailable);
        }
        if &bytes[offset..offset + 8] != MAGIC
            || u16::from_be_bytes(
                bytes[offset + 8..offset + 10]
                    .try_into()
                    .map_err(|_| BifrostError::QueryAuditUnavailable)?,
            ) != VERSION
        {
            return Err(BifrostError::QueryAuditUnavailable);
        }
        let lsn = u64::from_be_bytes(
            bytes[offset + 10..offset + 18]
                .try_into()
                .map_err(|_| BifrostError::QueryAuditUnavailable)?,
        );
        let accepted_at_micros = i64::from_be_bytes(
            bytes[offset + 18..offset + 26]
                .try_into()
                .map_err(|_| BifrostError::QueryAuditUnavailable)?,
        );
        let tenant_len = usize::try_from(u32::from_be_bytes(
            bytes[offset + 26..offset + 30]
                .try_into()
                .map_err(|_| BifrostError::QueryAuditUnavailable)?,
        ))
        .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        if u64::try_from(tenant_len).map_err(|_| BifrostError::QueryAuditUnavailable)?
            > max_component_bytes
        {
            return Err(BifrostError::QueryAuditUnavailable);
        }
        let tenant_start = offset + HEADER_LEN;
        let payload_len_pos = tenant_start
            .checked_add(tenant_len)
            .ok_or(BifrostError::QueryAuditUnavailable)?;
        if bytes.len() < payload_len_pos + 4 {
            if allow_torn_tail {
                break;
            }
            return Err(BifrostError::QueryAuditUnavailable);
        }
        let payload_len = usize::try_from(u32::from_be_bytes(
            bytes[payload_len_pos..payload_len_pos + 4]
                .try_into()
                .map_err(|_| BifrostError::QueryAuditUnavailable)?,
        ))
        .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        if u64::try_from(payload_len).map_err(|_| BifrostError::QueryAuditUnavailable)?
            > max_component_bytes
        {
            return Err(BifrostError::QueryAuditUnavailable);
        }
        let payload_start = payload_len_pos + 4;
        let end = payload_start
            .checked_add(payload_len)
            .and_then(|end| end.checked_add(4))
            .ok_or(BifrostError::QueryAuditUnavailable)?;
        if end > bytes.len() {
            if allow_torn_tail {
                break;
            }
            return Err(BifrostError::QueryAuditUnavailable);
        }
        if crc32c(&bytes[start..end - 4])
            != u32::from_be_bytes(
                bytes[end - 4..end]
                    .try_into()
                    .map_err(|_| BifrostError::QueryAuditUnavailable)?,
            )
        {
            return Err(BifrostError::QueryAuditUnavailable);
        }
        let tenant = std::str::from_utf8(&bytes[tenant_start..payload_len_pos])
            .map_err(|_| BifrostError::QueryAuditUnavailable)?
            .parse()
            .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        let event = serde_json::from_slice(&bytes[payload_start..payload_start + payload_len])
            .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        records.push(AuditWalRecord {
            lsn,
            accepted_at_micros,
            tenant,
            event,
        });
        offset = end;
    }
    Ok((
        records,
        u64::try_from(offset).map_err(|_| BifrostError::QueryAuditUnavailable)?,
        file_len,
    ))
}

/// Reads and validates the atomic checkpoint file, defaulting to LSN zero.
fn read_checkpoint(root: &Path) -> Result<u64, BifrostError> {
    let path = root.join("checkpoint");
    match fs::read_to_string(path) {
        Ok(value) => value
            .trim()
            .parse()
            .map_err(|_| BifrostError::QueryAuditUnavailable),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(0),
        Err(_) => Err(BifrostError::QueryAuditUnavailable),
    }
}

/// Flushes directory metadata so root and rename durability are acknowledged.
fn sync_dir(path: &Path) -> Result<(), BifrostError> {
    File::open(path)
        .map_err(|_| BifrostError::QueryAuditUnavailable)?
        .sync_all()
        .map_err(|_| BifrostError::QueryAuditUnavailable)
}

/// Converts a system timestamp to the persisted signed microsecond field.
fn epoch_micros(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_micros()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};
    use std::thread;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditOutcome, AuthMethod};

    /// Creates a minimal valid audit payload for WAL tests.
    fn event() -> AuditEvent {
        event_with_summary("test")
    }

    /// Creates a valid payload with a caller-selected summary size.
    fn event_with_summary(summary: &str) -> AuditEvent {
        AuditEvent::new(
            RequestId::now_v7(),
            None,
            summary.to_owned(),
            "bifrost.query".to_owned(),
            None,
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKindTag::User,
            AuthMethod::Internal,
            "bifrost.query.read".to_owned(),
            AuditOutcome::Allowed,
            AuditOutcome::Allowed,
            "test".to_owned(),
        )
    }

    /// Creates small deterministic test bounds rooted at `root`.
    fn config(root: &Path) -> OracleAuditWalConfig {
        OracleAuditWalConfig {
            audit_wal_root: Some(root.to_owned()),
            audit_wal_max_records: 8,
            audit_wal_max_bytes: 1 << 20,
            ..OracleAuditWalConfig::default()
        }
    }

    #[test]
    /// Proves one fsynced frame can be recovered after reopening the root.
    fn audit_wal_append_is_recoverable() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let tenant = DataTenantId::new_v7();
        let mut wal = AuditWal::recover(root.path(), &config(root.path())).expect("recover");
        wal.append(tenant, event()).expect("append");
        drop(wal);
        let recovered = AuditWal::recover(root.path(), &config(root.path())).expect("restart");
        assert_eq!(recovered.snapshot().0, 1);
    }

    #[test]
    /// Proves a durable checkpoint leaves no pending records after restart.
    fn audit_wal_checkpoint_resumes_after_restart() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let tenant = DataTenantId::new_v7();
        let mut wal = AuditWal::recover(root.path(), &config(root.path())).expect("recover");
        let lsn = wal.append(tenant, event()).expect("append");
        wal.checkpoint(lsn).expect("checkpoint");
        drop(wal);
        let recovered = AuditWal::recover(root.path(), &config(root.path())).expect("restart");
        assert_eq!(recovered.snapshot().0, 0);
    }

    #[test]
    /// Proves record capacity rejects a second accepted frame.
    fn audit_wal_full_fails_closed() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let tenant = DataTenantId::new_v7();
        let mut bounds = config(root.path());
        bounds.audit_wal_max_records = 1;
        let mut wal = AuditWal::recover(root.path(), &bounds).expect("recover");
        wal.append(tenant, event()).expect("first append");
        assert_eq!(
            wal.append(tenant, event()),
            Err(BifrostError::QueryAuditUnavailable)
        );
    }

    #[test]
    /// Proves persisted acceptance timestamps survive restart and skew clamps.
    fn audit_wal_oldest_age_survives_restart() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let tenant = DataTenantId::new_v7();
        let mut wal = AuditWal::recover(root.path(), &config(root.path())).expect("recover");
        wal.append(tenant, event()).expect("append");
        let before = wal.oldest_unrelayed_age(SystemTime::now()).expect("age");
        drop(wal);
        let recovered = AuditWal::recover(root.path(), &config(root.path())).expect("restart");
        let after = recovered
            .oldest_unrelayed_age(SystemTime::now())
            .expect("age after restart");
        assert!(after >= before);
        assert_eq!(
            recovered.oldest_unrelayed_age(UNIX_EPOCH),
            Some(Duration::ZERO)
        );
    }

    #[test]
    /// Proves only an incomplete final frame is truncated during recovery.
    fn audit_wal_torn_tail_is_truncated() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let tenant = DataTenantId::new_v7();
        let mut wal = AuditWal::recover(root.path(), &config(root.path())).expect("recover");
        wal.append(tenant, event()).expect("append");
        let segment = root.path().join("audit-00000000000000000001.wal");
        let length = fs::metadata(&segment).expect("segment metadata").len();
        let file = OpenOptions::new()
            .write(true)
            .open(segment)
            .expect("segment");
        file.set_len(length - 3).expect("tear tail");
        drop(wal);
        let recovered =
            AuditWal::recover(root.path(), &config(root.path())).expect("recover torn tail");
        assert_eq!(recovered.snapshot().0, 0);
    }

    #[test]
    /// Proves CRC corruption in a complete frame fails readiness.
    fn audit_wal_mid_log_corruption_fails_readiness() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let tenant = DataTenantId::new_v7();
        let mut wal = AuditWal::recover(root.path(), &config(root.path())).expect("recover");
        wal.append(tenant, event()).expect("append");
        wal.append(tenant, event()).expect("append");
        let segment = root.path().join("audit-00000000000000000001.wal");
        let mut bytes = fs::read(&segment).expect("read segment");
        bytes[12] ^= 0x01;
        fs::write(segment, bytes).expect("corrupt segment");
        drop(wal);
        assert!(AuditWal::recover(root.path(), &config(root.path())).is_err());
    }

    #[test]
    /// Proves the root lock rejects a concurrent publisher owner.
    fn audit_wal_second_owner_is_rejected() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let first = AuditWal::recover(root.path(), &config(root.path())).expect("first owner");
        assert!(AuditWal::recover(root.path(), &config(root.path())).is_err());
        drop(first);
        AuditWal::recover(root.path(), &config(root.path())).expect("lock released");
    }

    #[test]
    /// Proves segment rotation occurs before the 64 MiB boundary.
    fn audit_wal_rotates_before_segment_limit() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let mut bounds = config(root.path());
        bounds.audit_wal_max_bytes = 100 * 1024 * 1024;
        let tenant = DataTenantId::new_v7();
        let mut wal = AuditWal::recover(root.path(), &bounds).expect("recover");
        let summary = "x".repeat(40 * 1024 * 1024);
        wal.append(tenant, event_with_summary(&summary))
            .expect("first large append");
        wal.append(tenant, event_with_summary(&summary))
            .expect("rotated append");
        let segments = fs::read_dir(root.path())
            .expect("segments")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".wal"))
            .count();
        assert_eq!(segments, 2);
    }

    #[test]
    /// Proves an uncheckpointed accepted frame is selected again after restart.
    fn oracle_audit_relay_replays_uncheckpointed_record() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let tenant = DataTenantId::new_v7();
        let mut wal = AuditWal::recover(root.path(), &config(root.path())).expect("recover");
        wal.append(tenant, event()).expect("append");
        drop(wal);
        let recovered = AuditWal::recover(root.path(), &config(root.path())).expect("restart");
        assert_eq!(recovered.pending(8).len(), 1);
    }

    #[test]
    /// Proves relay selection remains LSN ordered for hash-chain append order.
    fn oracle_audit_relay_preserves_hash_chain_order() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let tenant = DataTenantId::new_v7();
        let mut wal = AuditWal::recover(root.path(), &config(root.path())).expect("recover");
        wal.append(tenant, event()).expect("first append");
        wal.append(tenant, event()).expect("second append");
        let records = wal.pending(8);
        assert_eq!(records[0].lsn + 1, records[1].lsn);
    }

    #[test]
    /// Proves durable backlog inspection reports records and bytes at shutdown.
    fn oracle_audit_relay_shutdown_reports_backlog() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let tenant = DataTenantId::new_v7();
        let mut wal = AuditWal::recover(root.path(), &config(root.path())).expect("recover");
        wal.append(tenant, event()).expect("append");
        let snapshot = wal.snapshot();
        assert_eq!(snapshot.0, 1);
        assert!(snapshot.1 > 0);
    }

    #[test]
    /// Proves concurrent append callers are serialized and all fsynced frames recover.
    fn audit_wal_concurrent_group_commit_acknowledgements() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let wal = Arc::new(StdMutex::new(
            AuditWal::recover(root.path(), &config(root.path())).expect("recover"),
        ));
        let handles = (0..4)
            .map(|_| {
                let wal = Arc::clone(&wal);
                thread::spawn(move || {
                    wal.lock()
                        .expect("WAL lock")
                        .append(DataTenantId::new_v7(), event())
                        .expect("append")
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().expect("append thread");
        }
        drop(wal);
        let recovered = AuditWal::recover(root.path(), &config(root.path())).expect("restart");
        assert_eq!(recovered.snapshot().0, 4);
    }

    #[test]
    /// Proves checkpoint replacement persists the new high-water before reclamation.
    fn audit_wal_checkpoint_and_segment_reclamation_are_durable() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let mut bounds = config(root.path());
        bounds.audit_wal_max_bytes = 100 * 1024 * 1024;
        let mut wal = AuditWal::recover(root.path(), &bounds).expect("recover");
        let summary = "x".repeat(40 * 1024 * 1024);
        wal.append(DataTenantId::new_v7(), event_with_summary(&summary))
            .expect("first append");
        let second = wal
            .append(DataTenantId::new_v7(), event_with_summary(&summary))
            .expect("second append");
        wal.checkpoint(second).expect("checkpoint");
        assert_eq!(
            fs::read_to_string(root.path().join("checkpoint"))
                .expect("checkpoint file")
                .trim(),
            second.to_string()
        );
        let segment_count = fs::read_dir(root.path())
            .expect("segments")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".wal"))
            .count();
        assert_eq!(segment_count, 1);
    }

    #[test]
    /// Proves oversized length fields fail recovery before allocation.
    fn audit_wal_oversized_frame_is_rejected() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let segment = root.path().join("audit-00000000000000000001.wal");
        fs::create_dir_all(root.path()).expect("root");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_be_bytes());
        bytes.extend_from_slice(&1u64.to_be_bytes());
        bytes.extend_from_slice(&0i64.to_be_bytes());
        bytes.extend_from_slice(&u32::MAX.to_be_bytes());
        fs::write(segment, bytes).expect("oversized frame");
        assert!(AuditWal::recover(root.path(), &config(root.path())).is_err());
    }

    /// Builds one writer command and retains its completion receiver.
    fn writer_command(
        tenant: DataTenantId,
    ) -> (
        AuditWalAppendCommand,
        oneshot::Receiver<Result<u64, BifrostError>>,
    ) {
        let (ack, result) = oneshot::channel();
        (
            AuditWalAppendCommand {
                tenant,
                event: event(),
                ack,
            },
            result,
        )
    }

    #[tokio::test]
    /// Proves the real bounded ingress rejects saturation before the writer runs.
    async fn audit_wal_writer_saturation_fails_closed() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let wal = Arc::new(Mutex::new(
            AuditWal::recover(root.path(), &config(root.path())).expect("recover"),
        ));
        let (sender, _, control, task) = AuditWalWriter::spawn(Arc::clone(&wal), 1);
        control.paused.store(true, Ordering::Release);
        let (first, first_ack) = writer_command(DataTenantId::new_v7());
        sender.try_send(first).expect("first command fits");
        let (second, _second_ack) = writer_command(DataTenantId::new_v7());
        assert!(sender.try_send(second).is_err(), "full ingress rejects");
        control.paused.store(false, Ordering::Release);
        control.notify.notify_waiters();
        drop(sender);
        assert!(first_ack.await.expect("first acknowledgement").is_ok());
        task.await.expect("writer task");
    }

    #[tokio::test]
    /// Proves one group performs one durable sync before acknowledging every member.
    async fn audit_wal_writer_group_acknowledges_after_shared_sync() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let wal = Arc::new(Mutex::new(
            AuditWal::recover(root.path(), &config(root.path())).expect("recover"),
        ));
        let (sender, _, control, task) = AuditWalWriter::spawn(Arc::clone(&wal), 8);
        control.paused.store(true, Ordering::Release);
        let mut acknowledgements = Vec::new();
        for _ in 0..4 {
            let (command, acknowledgement) = writer_command(DataTenantId::new_v7());
            sender.try_send(command).expect("group command fits");
            acknowledgements.push(acknowledgement);
        }
        control.paused.store(false, Ordering::Release);
        control.notify.notify_waiters();
        for acknowledgement in acknowledgements {
            assert!(
                acknowledgement
                    .await
                    .expect("group acknowledgement")
                    .is_ok()
            );
        }
        assert_eq!(control.sync_count.load(Ordering::Acquire), 1);
        assert_eq!(wal.lock().await.snapshot().0, 4);
        drop(sender);
        task.await.expect("writer task");
    }

    #[tokio::test]
    /// Proves a failed group fans out errors and leaves synced high-water unchanged.
    async fn audit_wal_writer_group_failure_fans_out_without_high_water() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let wal = Arc::new(Mutex::new(
            AuditWal::recover(root.path(), &config(root.path())).expect("recover"),
        ));
        let (sender, fail_next, control, task) = AuditWalWriter::spawn(Arc::clone(&wal), 8);
        fail_next.store(true, Ordering::Release);
        control.paused.store(true, Ordering::Release);
        let mut acknowledgements = Vec::new();
        for _ in 0..4 {
            let (command, acknowledgement) = writer_command(DataTenantId::new_v7());
            sender.try_send(command).expect("group command fits");
            acknowledgements.push(acknowledgement);
        }
        control.paused.store(false, Ordering::Release);
        control.notify.notify_waiters();
        for acknowledgement in acknowledgements {
            assert_eq!(
                acknowledgement.await.expect("group acknowledgement"),
                Err(BifrostError::QueryAuditUnavailable)
            );
        }
        assert_eq!(control.sync_count.load(Ordering::Acquire), 0);
        assert_eq!(wal.lock().await.snapshot().0, 0);
        drop(sender);
        task.await.expect("writer task");
    }

    #[tokio::test]
    /// Proves closing ingress drains accepted commands into a durable WAL group.
    async fn audit_wal_writer_close_drains_accepted_commands() {
        let root = tempfile::tempdir().expect("temporary WAL root");
        let wal = Arc::new(Mutex::new(
            AuditWal::recover(root.path(), &config(root.path())).expect("recover"),
        ));
        let (sender, _, control, task) = AuditWalWriter::spawn(Arc::clone(&wal), 8);
        control.paused.store(true, Ordering::Release);
        let mut acknowledgements = Vec::new();
        for _ in 0..2 {
            let (command, acknowledgement) = writer_command(DataTenantId::new_v7());
            sender.try_send(command).expect("accepted command");
            acknowledgements.push(acknowledgement);
        }
        drop(sender);
        control.paused.store(false, Ordering::Release);
        control.notify.notify_waiters();
        for acknowledgement in acknowledgements {
            assert!(
                acknowledgement
                    .await
                    .expect("group acknowledgement")
                    .is_ok()
            );
        }
        task.await.expect("writer task");
        assert_eq!(wal.lock().await.snapshot().0, 2);
    }
}
