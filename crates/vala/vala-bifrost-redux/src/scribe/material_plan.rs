//! Synchronous, pre-allocation material planning for Scribe ingress.
//!
//! Canonical planning measures the batches Gate already materialized. Native
//! planning only bounds the wire frame: Arrow's `StreamDecoder` validates the
//! stream during preprocessing, and the decoded output is held to the expanded
//! ceiling there, before WAL.

use std::io::Write;

use crate::contracts::ScribeError;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;

/// Immutable facts used by one complete Scribe root admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IngestMaterialPlan {
    /// Encoded request length in bytes, retained through admission.
    pub(crate) request_len: usize,
    /// Materialized Arrow bytes the admitted payload already holds.
    ///
    /// Canonical batches were materialized by Gate before Scribe admission, so
    /// their retained buffers are charged now. A native stream's source bytes
    /// are the transport-owned wire body, so it holds none; its decoded output
    /// is bounded when preprocessing materializes it.
    pub(crate) held_material_bytes: usize,
    /// Largest decoded-plus-encoded canonical material admitted for the request.
    ///
    /// Zero for a native stream, whose decoded output is bounded during
    /// preprocessing rather than planned.
    pub(crate) current_material_bytes: usize,
}

/// Canonical two-phase materialization plan used by every Scribe producer.
///
/// The alias keeps the existing field-level planner representation while
/// making the preflight owner explicit at ingress and recovery call sites.
pub(crate) type MaterialPlan = IngestMaterialPlan;

/// Scribe-owned planner for native and typed OTLP ingress.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScribeIngressPlanner {
    /// Immutable operator-selected limits validated by server boot.
    limits: crate::gate::limits::IngestLimits,
}

impl ScribeIngressPlanner {
    /// Constructs one planner from the same frozen limits snapshot used by Gate.
    #[must_use]
    pub(crate) const fn new(limits: crate::gate::limits::IngestLimits) -> Self {
        Self { limits }
    }
}

impl Default for ScribeIngressPlanner {
    /// Uses immutable V1 maxima for embedded and unit-test construction.
    fn default() -> Self {
        Self::new(crate::gate::limits::IngestLimits::default())
    }
}

impl ScribeIngressPlanner {
    /// Plans one canonical validated batch set from observable Arrow facts.
    ///
    /// Every public write reaches this path: Gate has already decoded, projected,
    /// and authorized the caller's user columns, so the plan is derived from the
    /// arrays that exist rather than from a wire request that has yet to be
    /// materialized. The final output — the batches' retained logical buffers
    /// plus the managed-column projection — must fit the expanded ceiling
    /// before WAL. Scribe still acquires its complete root here, before physical
    /// binding and durable preprocessing.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] on row-count overflow and
    /// [`ScribeError::DecodedPayloadTooLarge`] when output exceeds the expanded
    /// ceiling or checked material arithmetic overflows.
    pub(crate) fn plan_canonical(
        &self,
        batches: &[RecordBatch],
        request_bytes: usize,
    ) -> Result<IngestMaterialPlan, ScribeError> {
        let overflow = || ScribeError::DecodedPayloadTooLarge {
            bytes: usize::MAX,
            limit: self.limits.expanded_bytes(),
        };
        let mut rows = 0_usize;
        let mut total_bytes = 0_usize;
        let mut expanded_bytes = 0_usize;
        let mut max_ipc_bytes = 0_usize;
        for batch in batches {
            rows = rows
                .checked_add(batch.num_rows())
                .ok_or(ScribeError::InvalidFrame)?;
            max_ipc_bytes = max_ipc_bytes.max(count_ipc_bytes(batch)?);
            total_bytes = total_bytes
                .checked_add(batch.get_array_memory_size())
                .ok_or_else(overflow)?;
            expanded_bytes = expanded_bytes
                .checked_add(self::expanded_bytes(batch)?)
                .ok_or_else(overflow)?;
        }
        if expanded_bytes > self.limits.expanded_bytes() {
            return Err(ScribeError::DecodedPayloadTooLarge {
                bytes: expanded_bytes,
                limit: self.limits.expanded_bytes(),
            });
        }
        let managed_bytes = managed_projection_bytes(rows, 36)?;
        let decoded_bytes =
            total_bytes
                .checked_add(managed_bytes)
                .ok_or(ScribeError::DecodedPayloadTooLarge {
                    bytes: usize::MAX,
                    limit: self.limits.max_frame_bytes,
                })?;
        let encoded_bytes = max_ipc_bytes
            .checked_add(managed_bytes)
            .and_then(|value| value.checked_add(self.limits.wal_workspace_bytes))
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.max_frame_bytes,
            })?;
        let current_material_bytes = decoded_bytes.checked_add(encoded_bytes).ok_or(
            ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: self.limits.max_frame_bytes,
            },
        )?;
        Ok(IngestMaterialPlan {
            request_len: request_bytes,
            held_material_bytes: if rows == 0 { 0 } else { total_bytes },
            current_material_bytes,
        })
    }

    /// Bounds one native Arrow IPC stream by its wire size before admission.
    ///
    /// Nothing else is read here: Arrow's `StreamDecoder` validates the stream
    /// when preprocessing decodes it, and that path refuses output above the
    /// expanded ceiling before WAL.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::PayloadTooLarge`] when the frame exceeds the wire
    /// ceiling.
    pub(crate) fn plan_native(&self, bytes: &Bytes) -> Result<IngestMaterialPlan, ScribeError> {
        if bytes.len() > self.limits.max_frame_bytes {
            return Err(ScribeError::PayloadTooLarge {
                bytes: bytes.len(),
                limit: self.limits.max_frame_bytes,
            });
        }
        Ok(IngestMaterialPlan {
            request_len: bytes.len(),
            held_material_bytes: 0,
            current_material_bytes: 0,
        })
    }
}

/// Byte-counting sink used to derive one exact IPC output capacity.
#[derive(Debug, Default)]
struct IpcByteCounter {
    /// Checked bytes emitted by Arrow's public stream writer.
    bytes: usize,
}

impl Write for IpcByteCounter {
    /// Counts one writer chunk without retaining it.
    ///
    /// # Errors
    ///
    /// Returns an IO error when the checked byte count overflows.
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(buffer.len())
            .ok_or_else(|| std::io::Error::other("Arrow IPC output length overflow"))?;
        Ok(buffer.len())
    }

    /// Counting has no buffered IO to flush.
    ///
    /// # Errors
    ///
    /// This implementation does not fail because it retains no IO state.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Counts the exact public Arrow IPC stream bytes for one batch without output.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when Arrow cannot initialize, encode, or
/// finish the count pass.
pub(crate) fn count_ipc_bytes(rows: &RecordBatch) -> Result<usize, ScribeError> {
    let mut counter = IpcByteCounter::default();
    let mut writer = StreamWriter::try_new(&mut counter, &rows.schema()).map_err(|error| {
        ScribeError::Internal {
            detail: format!("Arrow IPC count init failed: {error}"),
        }
    })?;
    writer.write(rows).map_err(|error| ScribeError::Internal {
        detail: format!("Arrow IPC count failed: {error}"),
    })?;
    writer.finish().map_err(|error| ScribeError::Internal {
        detail: format!("Arrow IPC count finish failed: {error}"),
    })?;
    Ok(counter.bytes)
}

/// Sums the Arrow value, offset, and validity bytes `batch`'s logical rows retain.
///
/// Slices of one shared backing allocation contribute only their own logical
/// range, so native IPC backing aliased by several columns is counted once.
/// This is the expanded-data metric every ingest decoder bounds before WAL.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] when Arrow cannot size a
/// column's logical range or the sum overflows.
pub(crate) fn retained_slice_bytes(batch: &RecordBatch) -> Result<usize, ScribeError> {
    batch.columns().iter().try_fold(0_usize, |total, column| {
        column
            .to_data()
            .get_slice_memory_size()
            .ok()
            .and_then(|bytes| total.checked_add(bytes))
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: usize::MAX,
            })
    })
}

/// Measures one caller batch's expanded data: its retained Arrow bytes plus
/// the Scribe-managed columns stamping will add for its rows.
///
/// This is the one expanded-data metric every ingest path holds to the
/// expanded ceiling before WAL: canonical planning sums it over Gate's
/// batches, and the native producer sums it over each batch Arrow decodes,
/// before stamping.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] when Arrow cannot size a
/// column or the checked sum overflows.
pub(crate) fn expanded_bytes(batch: &RecordBatch) -> Result<usize, ScribeError> {
    retained_slice_bytes(batch)?
        .checked_add(managed_projection_bytes(batch.num_rows(), 36)?)
        .ok_or(ScribeError::DecodedPayloadTooLarge {
            bytes: usize::MAX,
            limit: usize::MAX,
        })
}

/// Computes public Arrow buffer capacities for Scribe-managed columns.
///
/// The calculation includes the nullable `run_id`/`card_uid` validity and
/// offsets, three non-null UTF-8 columns, one aliased receipt timestamp buffer,
/// and fixed batch identity. Source-owned buffers remain in the
/// native body fact and are not charged twice.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] when checked Arrow capacity
/// arithmetic overflows.
fn managed_projection_bytes(rows: usize, request_id_bytes: usize) -> Result<usize, ScribeError> {
    let overflow = || ScribeError::DecodedPayloadTooLarge {
        bytes: usize::MAX,
        limit: usize::MAX,
    };
    let offsets = rows
        .checked_add(1)
        .and_then(|value| value.checked_mul(size_of::<i32>()))
        .ok_or_else(overflow)?;
    let validity = rows.checked_add(7).ok_or_else(overflow)? / 8;
    let repeated_values = rows
        .checked_mul(
            36_usize
                .checked_add(request_id_bytes)
                .ok_or_else(overflow)?,
        )
        .and_then(|value| value.checked_add(rows.checked_mul(36)?))
        .and_then(|value| value.checked_add(rows.checked_mul(36)?))
        .ok_or_else(overflow)?;
    let fixed_values = rows
        .checked_mul(size_of::<i64>() + 16)
        .ok_or_else(overflow)?;
    let array_owners = size_of::<arrow::array::StringArray>()
        .checked_mul(5)
        .and_then(|value| {
            value.checked_add(size_of::<arrow::array::TimestampMicrosecondArray>() * 2)
        })
        .and_then(|value| value.checked_add(size_of::<arrow::array::FixedSizeBinaryArray>()))
        .ok_or_else(overflow)?;
    offsets
        .checked_mul(5)
        .and_then(|value| value.checked_add(validity.checked_mul(2)?))
        .and_then(|value| value.checked_add(repeated_values))
        .and_then(|value| value.checked_add(fixed_values))
        .and_then(|value| value.checked_add(array_owners))
        .ok_or_else(overflow)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::ScribeIngressPlanner;
    use crate::contracts::ScribeError;
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use bytes::Bytes;

    /// Encodes `rows` sequential `Int64` values as one canonical native stream.
    ///
    /// # Panics
    ///
    /// Panics only when the fixture cannot encode valid Arrow.
    fn int64_stream(rows: usize) -> (RecordBatch, Bytes) {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let values = (0..i64::try_from(rows).expect("fixture rows fit i64")).collect::<Vec<_>>();
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(values))],
        )
        .expect("int64 batch");
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("stream writer");
        writer.write(&batch).expect("batch write");
        writer.finish().expect("stream finish");
        (batch, Bytes::from(bytes))
    }

    /// Returns ingest limits with a 4 KiB wire ceiling and 16 KiB expanded ceiling.
    fn small_limits() -> crate::gate::limits::IngestLimits {
        crate::gate::limits::IngestLimits {
            max_frame_bytes: 4096,
            ..crate::gate::limits::IngestLimits::default()
        }
    }

    /// A native frame above the wire ceiling is refused before admission.
    ///
    /// # Panics
    ///
    /// Panics when the small stream is refused or the oversized one is planned.
    #[test]
    fn native_plan_refuses_frame_above_wire_ceiling() {
        let planner = ScribeIngressPlanner::new(small_limits());
        let (_, fits) = int64_stream(10);
        planner.plan_native(&fits).expect("small stream fits");
        let (_, oversized) = int64_stream(1_000);
        assert!(matches!(
            planner.plan_native(&oversized),
            Err(ScribeError::PayloadTooLarge { limit, .. })
                if limit == small_limits().max_frame_bytes
        ));
    }

    /// Canonical output above the expanded ceiling is refused, while more than
    /// 131,072 tiny rows within it are planned: row count is not a limit.
    ///
    /// # Panics
    ///
    /// Panics when the oversized batch is planned or the many-row batch is not.
    #[test]
    fn canonical_plan_bounds_output_bytes_not_rows() {
        let (small, _) = int64_stream(200);
        assert!(matches!(
            ScribeIngressPlanner::new(small_limits()).plan_canonical(&[small], 0),
            Err(ScribeError::DecodedPayloadTooLarge { limit, .. })
                if limit == small_limits().expanded_bytes()
        ));
        let (many, _) = int64_stream(131_073);
        ScribeIngressPlanner::default()
            .plan_canonical(&[many], 0)
            .expect("many tiny rows fit the default expanded ceiling");
    }
}
