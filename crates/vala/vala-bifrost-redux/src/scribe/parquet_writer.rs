//! Parquet writer for frozen memtable snapshots — .
//!
//! `write_frozen_to_parquet` encodes a `FrozenMemtable` snapshot (a per-seal-key,
//! per-day slice from ) to Parquet bytes using the copied `bifrost_writer_properties`
//! (), with sort order `(data_tenant_id, wyrd_event_time)` and partition day = the
//! seal-key's `event_day` (never derived from row min/max).

use arrow::compute::SortColumn;
use arrow::compute::lexsort_to_indices;
use arrow::compute::take;
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::file::metadata::RowGroupMetaData;
use std::io::Cursor;
use wyrd_spec::vala::api::AuditEvent;

use crate::contracts::ScribeError;
use crate::parquet::writer_properties::bifrost_writer_properties;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::seal_key::EventDay;
use crate::scribe::wal::ScribeAppendMeta;

/// Result of encoding a frozen memtable to Parquet.
#[derive(Debug, Clone)]
pub struct ParquetEncoded {
    /// Parquet file bytes (ready for object-store PUT).
    pub bytes: Vec<u8>,
    /// Row group statistics.
    pub row_group_stats: Vec<RowGroupStats>,
    /// Partition day (from seal-key, not row min/max).
    pub partition_day: EventDay,
    /// `AuditEvent` list threaded forward for 's seal transaction.
    pub audit_events: Vec<AuditEvent>,
    /// `ScribeAppendMeta` list threaded forward for 's `file_list` INSERT.
    pub append_metas: Vec<ScribeAppendMeta>,
}

/// Statistics for a single row group.
#[derive(Debug, Clone)]
pub struct RowGroupStats {
    /// Number of rows in this row group.
    pub row_count: usize,
    /// Minimum `wyrd_event_time` timestamp (microseconds since epoch).
    pub min_event_time: Option<i64>,
    /// Maximum `wyrd_event_time` timestamp (microseconds since epoch).
    pub max_event_time: Option<i64>,
}

/// Encode a frozen memtable snapshot to Parquet bytes.
///
/// Returns encoded bytes, row-group stats, `partition_day` (from seal-key), and the paired
/// `AuditEvent` + `ScribeAppendMeta` lists unmodified (threaded forward for 's seal
/// transaction).
///
/// # Errors
/// Returns [`ScribeError::Internal`] if Arrow sorting or Parquet encoding fails.
pub fn write_frozen_to_parquet(frozen: &FrozenMemtable) -> Result<ParquetEncoded, ScribeError> {
    // 1. Sort the batch by (data_tenant_id, wyrd_event_time)
    let sorted_batch = sort_batch(&frozen.batch)?;

    // 2. Write to Parquet in-memory using bifrost_writer_properties
    let mut buf = Cursor::new(Vec::new());
    let props = bifrost_writer_properties();

    {
        let mut writer = ArrowWriter::try_new(&mut buf, sorted_batch.schema(), Some(props))
            .map_err(|e| ScribeError::Internal {
                detail: format!("failed to create Parquet writer: {e}"),
            })?;

        writer
            .write(&sorted_batch)
            .map_err(|e| ScribeError::Internal {
                detail: format!("failed to write Parquet batch: {e}"),
            })?;

        writer.close().map_err(|e| ScribeError::Internal {
            detail: format!("failed to close Parquet writer: {e}"),
        })?;
    }

    let bytes = buf.into_inner();

    // 3. Extract row-group stats (for verification only; derives file-level min/max)
    let row_group_stats = extract_row_group_stats(&bytes)?;

    // 4. Return encoded result with partition_day from seal-key (not row min/max)
    Ok(ParquetEncoded {
        bytes,
        row_group_stats,
        partition_day: frozen.seal_key.day,
        audit_events: frozen.events.clone(),
        append_metas: frozen.metas.clone(),
    })
}

/// Sort a `RecordBatch` by (`data_tenant_id`, `wyrd_event_time`) using Arrow compute kernels.
///
/// # Errors
/// Returns [`ScribeError::Internal`] if sorting fails.
fn sort_batch(batch: &RecordBatch) -> Result<RecordBatch, ScribeError> {
    // Find column indices
    let schema = batch.schema();
    let tenant_idx = schema
        .index_of("data_tenant_id")
        .map_err(|_| ScribeError::Internal {
            detail: "data_tenant_id column not found".to_string(),
        })?;
    let time_idx = schema
        .index_of("wyrd_event_time")
        .map_err(|_| ScribeError::Internal {
            detail: "wyrd_event_time column not found".to_string(),
        })?;

    // Build sort columns: (data_tenant_id ASC, wyrd_event_time ASC)
    let sort_columns = vec![
        SortColumn {
            values: batch.column(tenant_idx).clone(),
            options: Some(arrow::compute::SortOptions {
                descending: false,
                nulls_first: false,
            }),
        },
        SortColumn {
            values: batch.column(time_idx).clone(),
            options: Some(arrow::compute::SortOptions {
                descending: false,
                nulls_first: false,
            }),
        },
    ];

    // Compute sort indices
    let indices = lexsort_to_indices(&sort_columns, None).map_err(|e| ScribeError::Internal {
        detail: format!("failed to compute sort indices: {e}"),
    })?;

    // Reorder all columns using the indices
    let sorted_columns: Result<Vec<_>, _> = batch
        .columns()
        .iter()
        .map(|col| {
            take(col.as_ref(), &indices, None).map_err(|e| ScribeError::Internal {
                detail: format!("failed to reorder column during sort: {e}"),
            })
        })
        .collect();

    RecordBatch::try_new(batch.schema(), sorted_columns?).map_err(|e| ScribeError::Internal {
        detail: format!("failed to rebuild sorted RecordBatch: {e}"),
    })
}

/// Extract row-group statistics from encoded Parquet bytes.
///
/// # Errors
/// Returns [`ScribeError::Internal`] if Parquet metadata parsing fails.
fn extract_row_group_stats(bytes: &[u8]) -> Result<Vec<RowGroupStats>, ScribeError> {
    use parquet::file::reader::{FileReader, SerializedFileReader};

    let reader = SerializedFileReader::new(bytes::Bytes::from(bytes.to_vec())).map_err(|e| {
        ScribeError::Internal {
            detail: format!("failed to parse Parquet metadata: {e}"),
        }
    })?;

    let metadata = reader.metadata();
    let mut stats = Vec::new();

    for rg in metadata.row_groups() {
        stats.push(extract_row_group_time_range(rg)?);
    }

    Ok(stats)
}

/// Extract min/max event time from a single row group.
///
/// # Errors
/// Returns [`ScribeError::Internal`] if column stats are missing.
fn extract_row_group_time_range(rg: &RowGroupMetaData) -> Result<RowGroupStats, ScribeError> {
    // Parquet row-group `num_rows()` is i64; RowGroupStats.row_count is usize.
    // A negative row count would be a Parquet-writer invariant violation.
    let row_count = usize::try_from(rg.num_rows()).map_err(|_| ScribeError::Internal {
        detail: format!("row group has invalid num_rows: {}", rg.num_rows()),
    })?;

    // Find wyrd_event_time column index (column order matches Arrow schema order)
    let time_col = rg
        .columns()
        .iter()
        .find(|col| col.column_descr().name() == "wyrd_event_time")
        .ok_or_else(|| ScribeError::Internal {
            detail: "wyrd_event_time column not found in row group".to_string(),
        })?;

    let stats = time_col.statistics().ok_or_else(|| ScribeError::Internal {
        detail: "wyrd_event_time column has no statistics".to_string(),
    })?;

    // Extract min/max as i64 (TimestampMicrosecondType)
    let min = if let Some(min_val) = stats.min_bytes_opt() {
        min_val.get(0..8).and_then(|b| {
            let arr: [u8; 8] = b.try_into().ok()?;
            Some(i64::from_le_bytes(arr))
        })
    } else {
        None
    };

    let max = if let Some(max_val) = stats.max_bytes_opt() {
        max_val.get(0..8).and_then(|b| {
            let arr: [u8; 8] = b.try_into().ok()?;
            Some(i64::from_le_bytes(arr))
        })
    } else {
        None
    };

    Ok(RowGroupStats {
        row_count,
        min_event_time: min,
        max_event_time: max,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{RecordBatch, StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use chrono::NaiveDate;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use parquet::file::reader::{FileReader, SerializedFileReader};
    use std::sync::Arc;
    use wyrd_spec::ids::DataTenantId;

    use crate::catalog::TableRef;
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::seal_key::{EventDay, SealKey};

    fn build_test_frozen(
        seal_day: NaiveDate,
        tenant_ids: Vec<&str>,
        timestamps: Vec<i64>,
    ) -> FrozenMemtable {
        let schema = Arc::new(Schema::new(vec![
            Field::new("data_tenant_id", DataType::Utf8, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(tenant_ids)),
                Arc::new(TimestampMicrosecondArray::from(timestamps)),
            ],
        )
        .unwrap();

        let seal_key = SealKey::new(
            DataTenantId::SYSTEM_OWNER,
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            EventDay::new(seal_day),
        );

        FrozenMemtable {
            seal_key,
            schema,
            batch,
            events: vec![],
            metas: vec![],
        }
    }

    #[test]
    fn parquet_writer_partition_day_matches_seal_key() {
        // Regression test for C2: partition_day = seal_key.event_day, not row min/max
        let seal_day = NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let frozen = build_test_frozen(
            seal_day,
            vec![tenant_a.to_string().as_str(), tenant_b.to_string().as_str()],
            vec![1_000_000, 2_000_000], // timestamps don't matter for partition_day
        );

        let encoded = write_frozen_to_parquet(&frozen).unwrap();
        assert_eq!(encoded.partition_day.as_date(), &seal_day);
    }

    #[test]
    fn parquet_writer_honors_sort_order() {
        // Round-trip: write shuffled input, verify sort order in column stats
        let t1 = DataTenantId::new_v7().to_string();
        let t2 = DataTenantId::new_v7().to_string();
        let t3 = DataTenantId::new_v7().to_string();

        let frozen = build_test_frozen(
            NaiveDate::from_ymd_opt(2026, 7, 14).unwrap(),
            vec![t3.as_str(), t1.as_str(), t2.as_str(), t1.as_str()], // Shuffled tenant IDs
            vec![400, 100, 300, 200],                                 // Shuffled timestamps
        );

        let encoded = write_frozen_to_parquet(&frozen).unwrap();

        // Re-read and verify sorted order
        let bytes_copy = bytes::Bytes::from(encoded.bytes);
        let builder = ParquetRecordBatchReaderBuilder::try_new(bytes_copy).unwrap();
        let mut reader = builder.build().unwrap();
        let batch = reader.next().unwrap().unwrap();

        let tenant_col = batch
            .column_by_name("data_tenant_id")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();

        let time_col = batch
            .column_by_name("wyrd_event_time")
            .unwrap()
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .unwrap();

        // Expected sort: (tenant=t1, time=100), (tenant=t1, time=200), (tenant=t2, time=300), (tenant=t3, time=400)
        assert_eq!(tenant_col.value(0), t1);
        assert_eq!(time_col.value(0), 100);

        assert_eq!(tenant_col.value(1), t1);
        assert_eq!(time_col.value(1), 200);

        assert_eq!(tenant_col.value(2), t2);
        assert_eq!(time_col.value(2), 300);

        assert_eq!(tenant_col.value(3), t3);
        assert_eq!(time_col.value(3), 400);
    }

    #[test]
    fn parquet_writer_writes_bloom_and_page_index() {
        // Note: bifrost_writer_properties intentionally has bloom filters OFF per its doc.
        // This test verifies page index is present; bloom test removed per actual contract.
        let t1 = DataTenantId::new_v7().to_string();
        let t2 = DataTenantId::new_v7().to_string();
        let frozen = build_test_frozen(
            NaiveDate::from_ymd_opt(2026, 7, 14).unwrap(),
            vec![t1.as_str(), t2.as_str()],
            vec![1_000_000, 2_000_000],
        );

        let encoded = write_frozen_to_parquet(&frozen).unwrap();

        let reader = SerializedFileReader::new(bytes::Bytes::from(encoded.bytes)).unwrap();
        let metadata = reader.metadata();

        // Verify page index (offset index) is present
        let rg = metadata.row_groups().first().unwrap();
        let col = rg.columns().first().unwrap();

        // Page index presence is indicated by offset_index_offset being set
        assert!(
            col.offset_index_offset().is_some() || col.column_index_offset().is_some(),
            "page index metadata not found"
        );
    }

    #[test]
    fn parquet_writer_returns_envelopes_unmodified() {
        // Encoding does not touch either the `AuditEvent` list or `ScribeAppendMeta` list
        let t1 = DataTenantId::new_v7().to_string();
        let frozen = build_test_frozen(
            NaiveDate::from_ymd_opt(2026, 7, 14).unwrap(),
            vec![t1.as_str()],
            vec![1_000_000],
        );

        let event = AuditEvent {
            request_id: wyrd_spec::request_id::RequestId::now_v7(),
            trace_id: None,
            operation: "test".to_string(),
            resource: "test".to_string(),
            card_ref: None,
            principal_id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
            principal_kind: wyrd_spec::auth::PrincipalKindTag::Service,
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "test".to_string(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "test".to_string(),
            detail: None,
        };

        let meta = crate::scribe::wal::ScribeAppendMeta {
            batch_id: [0u8; 16],
            rows_accepted: 1,
            wal_lsn_min: crate::scribe::wal::WalLsn::new(0),
            wal_lsn_max: crate::scribe::wal::WalLsn::new(0),
            seal_key: "test".to_string(),
        };

        let mut frozen_with_envelopes = frozen;
        frozen_with_envelopes.events = vec![event.clone()];
        frozen_with_envelopes.metas = vec![meta.clone()];

        let encoded = write_frozen_to_parquet(&frozen_with_envelopes).unwrap();

        assert_eq!(encoded.audit_events.len(), 1);
        assert_eq!(encoded.append_metas.len(), 1);

        // Verify pairing preserved (index i ↔ index i)
        assert_eq!(encoded.audit_events[0].request_id, event.request_id);
        assert_eq!(encoded.append_metas[0].batch_id, meta.batch_id);
    }
}
