//! Atomic buffering and validation of worker attempts.

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::vec::IntoIter;
use tempfile::NamedTempFile;
use thiserror::Error;
use wyrd_spec::vala::api::{WorkerAttemptFrame, WorkerFooter};

use crate::scribe::memory::{BifrostMemoryGovernor, MemoryPurpose, ParentMemoryReservation};

/// Validated whole attempt returned to the leader.
#[derive(Debug)]
pub struct ValidatedAttempt {
    /// Schema bytes.
    pub schema: Vec<u8>,
    /// Batch reader preserving private spill rather than rehydrating the attempt.
    pub batches: AttemptBatchReader,
    /// Completion footer.
    pub footer: WorkerFooter,
}

/// Validated batch reader over either retained memory or a private spill file.
#[derive(Debug)]
pub enum AttemptBatchReader {
    /// In-memory batches transferred without copying after validation.
    Memory {
        /// Validated batches transferred without copying.
        batches: IntoIter<Vec<u8>>,
        /// Parent reservation retained until all in-memory batches are dropped.
        memory_reservation: Option<ParentMemoryReservation>,
    },
    /// Length-delimited batches read one at a time from the private spill.
    Spill {
        /// Temporary file deleted automatically when iteration or cancellation ends.
        file: NamedTempFile,
        /// Exact number of payloads remaining in the spill.
        remaining: usize,
        /// Parent reservation retained through schema and incremental spill decode.
        memory_reservation: Option<ParentMemoryReservation>,
    },
}

impl Iterator for AttemptBatchReader {
    type Item = Result<Vec<u8>, AttemptError>;

    /// Reads the next validated batch without rehydrating the complete attempt.
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Memory {
                batches,
                memory_reservation,
            } => {
                let _reserved_bytes = memory_reservation
                    .as_ref()
                    .map_or(0, ParentMemoryReservation::bytes);
                batches.next().map(Ok)
            }
            Self::Spill {
                file,
                remaining,
                memory_reservation,
            } if *remaining > 0 => {
                let _reserved_bytes = memory_reservation
                    .as_ref()
                    .map_or(0, ParentMemoryReservation::bytes);
                *remaining -= 1;
                Some(read_payload(file.as_file_mut()))
            }
            Self::Spill { .. } => None,
        }
    }
}

/// One buffered payload retained in memory or in a permission-restricted spill.
#[derive(Debug)]
enum AttemptPayload {
    /// In-memory batches below the configured threshold.
    Memory(Vec<Vec<u8>>),
    /// Length-delimited batches in a query-scoped temporary file.
    Spill {
        /// Temporary file deleted automatically on drop.
        file: NamedTempFile,
        /// Batch count used for exact rehydration.
        batch_count: usize,
    },
}

/// Attempt validation failure; no partial data is exposed.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum AttemptError {
    /// A frame arrived after completion.
    #[error("attempt footer must be last")]
    FrameAfterFooter,
    /// Schema was duplicated or omitted.
    #[error("attempt schema frame is invalid")]
    Schema,
    /// Footer fields do not match buffered bytes.
    #[error("attempt footer does not validate")]
    Footer,
    /// The configured bounded attempt budget was exceeded.
    #[error("attempt buffer limit exceeded")]
    Capacity,
    /// The shared Bifrost parent could not admit the in-memory attempt tier.
    #[error("attempt parent memory capacity unavailable")]
    ParentCapacity,
    /// Spill IO failed; no partial attempt is exposed.
    #[error("attempt spill failed")]
    Spill,
}

/// Whole-attempt buffer that exposes data only after footer validation.
#[derive(Debug)]
pub struct AttemptBuffer {
    /// Exactly one schema frame retained before any admitted result exists.
    schema: Option<Vec<u8>>,
    /// Batch payloads retained in memory or in a private spill.
    payload: AttemptPayload,
    /// Total encoded schema and batch bytes charged to the hard cap.
    bytes: usize,
    /// Hard maximum encoded attempt size.
    limit: usize,
    /// Threshold at which batch payloads move to a private spill.
    memory_limit: usize,
    /// Exact Arrow row count decoded from buffered batch frames.
    row_count: u64,
    /// Incremental digest over batch frames in protocol order.
    payload_hash: Sha256,
    /// Exactly one completion footer retained for final validation.
    footer: Option<WorkerFooter>,
    /// Optional up-front parent reservation covering all retained in-memory bytes.
    memory_reservation: Option<ParentMemoryReservation>,
}

impl AttemptBuffer {
    /// Creates a bounded attempt buffer.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self::with_spill_limit(limit, limit)
    }

    /// Creates a hard-capped attempt buffer that spills after `memory_limit`.
    #[must_use]
    pub fn with_spill_limit(limit: usize, memory_limit: usize) -> Self {
        Self {
            schema: None,
            payload: AttemptPayload::Memory(Vec::new()),
            bytes: 0,
            limit,
            memory_limit: memory_limit.min(limit),
            row_count: 0,
            payload_hash: Sha256::new(),
            footer: None,
            memory_reservation: None,
        }
    }

    /// Creates a spill-backed buffer after reserving its complete memory tier.
    ///
    /// Reserving before the transport stream is polled ensures frame decoding
    /// cannot begin unless the shared Bifrost parent admits the leader buffer.
    ///
    /// # Errors
    ///
    /// Returns [`AttemptError::ParentCapacity`] when the process-wide parent cannot
    /// reserve the configured in-memory threshold.
    pub fn with_memory_governor(
        limit: usize,
        memory_limit: usize,
        governor: &BifrostMemoryGovernor,
    ) -> Result<Self, AttemptError> {
        let mut buffer = Self::with_spill_limit(limit, memory_limit);
        buffer.memory_reservation = Some(
            governor
                .try_reserve_parent_classified(buffer.memory_limit, MemoryPurpose::OracleQuery)
                .map_err(|_| AttemptError::ParentCapacity)?,
        );
        Ok(buffer)
    }
    /// Buffers one frame without admitting it to the leader plan.
    ///
    /// # Errors
    /// Returns [`AttemptError`] for ordering, duplication, or capacity violations.
    pub fn push(&mut self, frame: WorkerAttemptFrame) -> Result<(), AttemptError> {
        if self.footer.is_some() {
            return Err(AttemptError::FrameAfterFooter);
        }
        match frame {
            WorkerAttemptFrame::Schema(bytes) if self.schema.is_none() => {
                self.bytes += bytes.len();
                self.schema = Some(bytes);
            }
            WorkerAttemptFrame::Schema(_) => return Err(AttemptError::Schema),
            WorkerAttemptFrame::Batch(bytes) => {
                self.push_batch(bytes)?;
            }
            WorkerAttemptFrame::Footer(footer) => self.footer = Some(footer),
        }
        if self.bytes > self.limit {
            return Err(AttemptError::Capacity);
        }
        Ok(())
    }

    /// Buffers one complete encoded batch and spills atomically when needed.
    ///
    /// # Errors
    /// Returns [`AttemptError::Capacity`] or [`AttemptError::Spill`] without exposing bytes.
    fn push_batch(&mut self, bytes: Vec<u8>) -> Result<(), AttemptError> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or(AttemptError::Capacity)?;
        if self.bytes > self.limit {
            return Err(AttemptError::Capacity);
        }
        self.row_count = self
            .row_count
            .checked_add(encoded_batch_rows(&bytes)?)
            .ok_or(AttemptError::Capacity)?;
        self.payload_hash.update(&bytes);
        if matches!(self.payload, AttemptPayload::Memory(_)) && self.bytes > self.memory_limit {
            self.start_spill()?;
        }
        match &mut self.payload {
            AttemptPayload::Memory(batches) => batches.push(bytes),
            AttemptPayload::Spill { file, batch_count } => {
                write_payload(file.as_file_mut(), &bytes)?;
                *batch_count += 1;
            }
        }
        Ok(())
    }

    /// Moves already-buffered batches into one permission-restricted temporary file.
    ///
    /// # Errors
    /// Returns [`AttemptError::Spill`] if the temporary file cannot be created or written.
    fn start_spill(&mut self) -> Result<(), AttemptError> {
        let AttemptPayload::Memory(batches) =
            std::mem::replace(&mut self.payload, AttemptPayload::Memory(Vec::new()))
        else {
            return Ok(());
        };
        let mut file = tempfile::Builder::new()
            .prefix("wyrd-oracle-attempt-")
            .tempfile()
            .map_err(|_| AttemptError::Spill)?;
        for batch in &batches {
            write_payload(file.as_file_mut(), batch)?;
        }
        self.payload = AttemptPayload::Spill {
            file,
            batch_count: batches.len(),
        };
        Ok(())
    }
    /// Validates footer counts and digests, then transfers ownership atomically.
    ///
    /// # Errors
    /// Returns [`AttemptError::Footer`] when schema, counts, bytes, or digest do not match.
    pub fn finish(self) -> Result<ValidatedAttempt, AttemptError> {
        let schema = self.schema.ok_or(AttemptError::Schema)?;
        let footer = self.footer.ok_or(AttemptError::Footer)?;
        let digest = hex::encode(self.payload_hash.finalize());
        if !footer.completed
            || footer.encoded_bytes != self.bytes as u64
            || footer.payload_digest.as_str() != digest
            || self.row_count != footer.row_count
        {
            return Err(AttemptError::Footer);
        }
        let batches = match self.payload {
            AttemptPayload::Memory(batches) => AttemptBatchReader::Memory {
                batches: batches.into_iter(),
                memory_reservation: self.memory_reservation,
            },
            AttemptPayload::Spill {
                mut file,
                batch_count,
            } => {
                file.as_file_mut()
                    .seek(SeekFrom::Start(0))
                    .map_err(|_| AttemptError::Spill)?;
                AttemptBatchReader::Spill {
                    file,
                    remaining: batch_count,
                    memory_reservation: self.memory_reservation,
                }
            }
        };
        Ok(ValidatedAttempt {
            schema,
            batches,
            footer,
        })
    }
}

/// Reads the exact row count from one Arrow IPC stream batch.
///
/// # Errors
/// Returns [`AttemptError::Footer`] when the batch is not valid Arrow IPC.
fn encoded_batch_rows(bytes: &[u8]) -> Result<u64, AttemptError> {
    let mut reader = arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None)
        .map_err(|_| AttemptError::Footer)?;
    let mut rows = 0_u64;
    for batch in &mut reader {
        let batch = batch.map_err(|_| AttemptError::Footer)?;
        rows = rows
            .checked_add(u64::try_from(batch.num_rows()).map_err(|_| AttemptError::Footer)?)
            .ok_or(AttemptError::Footer)?;
    }
    Ok(rows)
}

/// Writes one length-delimited payload to a spill file.
///
/// # Errors
/// Returns [`AttemptError::Spill`] for an IO failure or an oversized payload.
fn write_payload(file: &mut File, bytes: &[u8]) -> Result<(), AttemptError> {
    let length = u64::try_from(bytes.len()).map_err(|_| AttemptError::Capacity)?;
    file.write_all(&length.to_le_bytes())
        .and_then(|()| file.write_all(bytes))
        .map_err(|_| AttemptError::Spill)
}

/// Reads one length-delimited payload from a validated private spill.
///
/// # Errors
/// Returns [`AttemptError::Spill`] when the payload length or bytes cannot be read exactly.
fn read_payload(reader: &mut File) -> Result<Vec<u8>, AttemptError> {
    let mut length = [0_u8; 8];
    reader
        .read_exact(&mut length)
        .map_err(|_| AttemptError::Spill)?;
    let length = usize::try_from(u64::from_le_bytes(length)).map_err(|_| AttemptError::Spill)?;
    let mut bytes = vec![0_u8; length];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| AttemptError::Spill)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use wyrd_spec::vala::api::QueryAuditDigest;

    use super::*;

    /// Encodes one real two-row Arrow IPC batch.
    fn batch_bytes() -> Vec<u8> {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![1_i64, 2]))],
        )
        .expect("record batch");
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("IPC writer");
        writer.write(&batch).expect("IPC batch");
        writer.finish().expect("IPC finish");
        drop(writer);
        bytes
    }

    /// Footer validation uses Arrow row count and succeeds through the spill path.
    #[test]
    fn oracle_attempt_spills_and_admits_only_complete_footer() {
        let batch = batch_bytes();
        let digest = hex::encode(Sha256::digest(&batch));
        let mut buffer = AttemptBuffer::with_spill_limit(1024 * 1024, 1);
        buffer
            .push(WorkerAttemptFrame::Schema(vec![1]))
            .expect("schema");
        buffer
            .push(WorkerAttemptFrame::Batch(batch.clone()))
            .expect("batch");
        buffer
            .push(WorkerAttemptFrame::Footer(WorkerFooter {
                fragment_id: "fragment".to_owned(),
                manifest_digest: QueryAuditDigest::new("manifest").expect("digest"),
                row_count: 2,
                encoded_bytes: u64::try_from(batch.len() + 1).expect("encoded bytes"),
                payload_digest: QueryAuditDigest::new(digest).expect("digest"),
                completed: true,
            }))
            .expect("footer");
        let validated = buffer.finish().expect("validated attempt");
        assert_eq!(
            validated.batches.collect::<Result<Vec<_>, _>>(),
            Ok(vec![batch])
        );
        assert_eq!(validated.footer.row_count, 2);
    }

    /// Missing, incomplete, or count-mismatched footers expose no attempt.
    #[test]
    fn oracle_attempt_rejects_partial_and_mismatched_footer() {
        let batch = batch_bytes();
        let mut missing = AttemptBuffer::new(1024 * 1024);
        missing
            .push(WorkerAttemptFrame::Schema(vec![1]))
            .expect("schema");
        missing
            .push(WorkerAttemptFrame::Batch(batch.clone()))
            .expect("batch");
        assert!(matches!(missing.finish(), Err(AttemptError::Footer)));

        let mut mismatch = AttemptBuffer::new(1024 * 1024);
        mismatch
            .push(WorkerAttemptFrame::Schema(vec![1]))
            .expect("schema");
        mismatch
            .push(WorkerAttemptFrame::Batch(batch.clone()))
            .expect("batch");
        mismatch
            .push(WorkerAttemptFrame::Footer(WorkerFooter {
                fragment_id: "fragment".to_owned(),
                manifest_digest: QueryAuditDigest::new("manifest").expect("digest"),
                row_count: 3,
                encoded_bytes: u64::try_from(batch.len() + 1).expect("encoded bytes"),
                payload_digest: QueryAuditDigest::new(hex::encode(Sha256::digest(&batch)))
                    .expect("digest"),
                completed: true,
            }))
            .expect("footer");
        assert!(matches!(mismatch.finish(), Err(AttemptError::Footer)));
    }
}
