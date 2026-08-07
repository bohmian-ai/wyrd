//! Parquet writer for frozen memtable snapshots.
//!
//! `encode_batch` stamps the authenticated tenant before encoding a
//! `FrozenMemtable` snapshot to Parquet. Every file uses sort order
//! `(data_tenant_id, wyrd_event_time)` and takes its partition day from the
//! seal-key (never from row min/max).

use std::io::Cursor;
use std::sync::Arc;

use arrow::array::{Array, StringArray};
use arrow::compute::SortColumn;
use arrow::compute::concat_batches;
use arrow::compute::lexsort_to_indices;
use arrow::compute::take;
use arrow::datatypes::{DataType, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::file::metadata::RowGroupMetaData;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;
use wyrd_spec::vala::managed_columns::DATA_TENANT_ID;

use crate::catalog::TenantTableBinding;
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

/// Encode a frozen memtable snapshot to Parquet bytes after stamping its tenant.
///
/// Returns encoded bytes, row-group stats, `partition_day` (from seal-key), and the paired
/// `AuditEvent` + `ScribeAppendMeta` lists unmodified (threaded forward for 's seal
/// transaction).
///
/// # Errors
/// Returns [`ScribeError::Internal`] when the binding, tenant column, tenant
/// values, sort keys, or Parquet encoding is invalid.
pub fn encode_batch(
    frozen: &FrozenMemtable,
    binding: &TenantTableBinding,
    seal_tenant: DataTenantId,
) -> Result<ParquetEncoded, ScribeError> {
    if binding.tenant != seal_tenant || binding.tenant != frozen.seal_key.tenant {
        return Err(ScribeError::Internal {
            detail: format!(
                "tenant-table binding mismatch before encoding: binding tenant `{}`; seal tenant `{}`; frozen tenant `{}`",
                binding.tenant, seal_tenant, frozen.seal_key.tenant
            ),
        });
    }
    if binding.table_ref != frozen.seal_key.table {
        return Err(ScribeError::Internal {
            detail: format!(
                "tenant-table binding table mismatch before encoding: binding `{}`; seal table `{}`",
                binding.table_ref, frozen.seal_key.table
            ),
        });
    }

    // 1. Materialize one temporary encoder batch in the bounded persistence
    // worker, then stamp and sort by the same two keys for every physical
    // table. FrozenMemtable retains only append batches so the merged form is
    // not held alongside the originals.
    let encoder_batch =
        concat_batches(&frozen.schema, &frozen.batches).map_err(|error| ScribeError::Internal {
            detail: format!("failed to materialize frozen memtable for encoding: {error}"),
        })?;
    let stamped_batch = stamp_tenant(&encoder_batch, seal_tenant)?;
    let sorted_batch = sort_batch(&stamped_batch)?;

    // 2. Write to Parquet in-memory using bifrost_writer_properties
    let mut buf = Cursor::new(Vec::new());
    let props = bifrost_writer_properties(sorted_batch.num_rows());

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

fn stamp_tenant(
    batch: &RecordBatch,
    seal_tenant: DataTenantId,
) -> Result<RecordBatch, ScribeError> {
    let schema = batch.schema();
    let tenant_idx = schema
        .index_of(DATA_TENANT_ID)
        .map_err(|_| ScribeError::Internal {
            detail: format!("{DATA_TENANT_ID} column not found after system-column construction"),
        })?;
    let tenant_array = batch
        .column(tenant_idx)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ScribeError::Internal {
            detail: format!("{DATA_TENANT_ID} column must be Utf8"),
        })?;
    let expected = seal_tenant.to_string();

    for row_index in 0..tenant_array.len() {
        if !tenant_array.is_null(row_index) {
            let observed = tenant_array.value(row_index);
            if observed != expected {
                return Err(ScribeError::Internal {
                    detail: format!(
                        "{DATA_TENANT_ID} mismatch at row {row_index}: expected `{expected}`, observed `{observed}`"
                    ),
                });
            }
        }
    }

    let mut fields: Vec<_> = schema.fields.iter().cloned().collect();
    fields[tenant_idx] = Arc::new(
        fields[tenant_idx]
            .as_ref()
            .clone()
            .with_data_type(DataType::Utf8)
            .with_nullable(false),
    );
    let stamped_schema = Arc::new(Schema {
        fields: fields.into(),
        metadata: schema.metadata.clone(),
    });
    let mut columns = batch.columns().to_vec();
    columns[tenant_idx] = Arc::new(StringArray::from(vec![expected; batch.num_rows()]));

    RecordBatch::try_new(stamped_schema, columns).map_err(|error| ScribeError::Internal {
        detail: format!("failed to stamp {DATA_TENANT_ID}: {error}"),
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
        seal_tenant: DataTenantId,
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
            seal_tenant,
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            EventDay::new(seal_day),
        );

        FrozenMemtable {
            seal_id: 0,
            seal_key,
            shard_id: 0,
            schema,
            batches: vec![batch],
            events: vec![],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        }
    }

    #[test]
    fn parquet_writer_partition_day_matches_seal_key() {
        // Regression test for C2: partition_day = seal_key.event_day, not row min/max
        let seal_day = NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
        let tenant = DataTenantId::new_v7();
        let tenant_string = tenant.to_string();
        let frozen = build_test_frozen(
            seal_day,
            tenant,
            vec![tenant_string.as_str(), tenant_string.as_str()],
            vec![1_000_000, 2_000_000], // timestamps don't matter for partition_day
        );

        let binding =
            TenantTableBinding::resolve((frozen.seal_key.tenant, frozen.seal_key.table.clone()))
                .unwrap();
        let encoded = encode_batch(&frozen, &binding, frozen.seal_key.tenant).unwrap();
        assert_eq!(encoded.partition_day.as_date(), &seal_day);
    }

    #[test]
    fn parquet_writer_honors_sort_order() {
        // Round-trip: write shuffled input, verify sort order in column stats
        let tenant = DataTenantId::new_v7();
        let tenant_string = tenant.to_string();

        let frozen = build_test_frozen(
            NaiveDate::from_ymd_opt(2026, 7, 14).unwrap(),
            tenant,
            vec![
                tenant_string.as_str(),
                tenant_string.as_str(),
                tenant_string.as_str(),
                tenant_string.as_str(),
            ],
            vec![400, 100, 300, 200], // Shuffled timestamps
        );

        let binding =
            TenantTableBinding::resolve((frozen.seal_key.tenant, frozen.seal_key.table.clone()))
                .unwrap();
        let encoded = encode_batch(&frozen, &binding, frozen.seal_key.tenant).unwrap();

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

        // Every row is stamped with the seal tenant; timestamps are the second sort key.
        assert_eq!(tenant_col.value(0), tenant_string);
        assert_eq!(time_col.value(0), 100);

        assert_eq!(tenant_col.value(1), tenant_string);
        assert_eq!(time_col.value(1), 200);

        assert_eq!(tenant_col.value(2), tenant_string);
        assert_eq!(time_col.value(2), 300);

        assert_eq!(tenant_col.value(3), tenant_string);
        assert_eq!(time_col.value(3), 400);
    }

    #[test]
    fn parquet_writer_writes_bloom_and_page_index() {
        let tenant = DataTenantId::new_v7();
        let tenant_string = tenant.to_string();
        let frozen = build_test_frozen(
            NaiveDate::from_ymd_opt(2026, 7, 14).unwrap(),
            tenant,
            vec![tenant_string.as_str(), tenant_string.as_str()],
            vec![1_000_000, 2_000_000],
        );

        let binding =
            TenantTableBinding::resolve((frozen.seal_key.tenant, frozen.seal_key.table.clone()))
                .unwrap();
        let encoded = encode_batch(&frozen, &binding, frozen.seal_key.tenant).unwrap();

        let reader = SerializedFileReader::new(bytes::Bytes::from(encoded.bytes)).unwrap();
        let metadata = reader.metadata();

        // Verify page index (offset index) is present
        let rg = metadata.row_groups().first().unwrap();
        let tenant_col = rg
            .columns()
            .iter()
            .find(|column| column.column_descr().name() == DATA_TENANT_ID)
            .unwrap();
        assert!(
            tenant_col.bloom_filter_offset().is_some(),
            "allowlisted tenant column must have a bloom filter"
        );

        let col = rg
            .columns()
            .iter()
            .find(|column| column.column_descr().name() == "wyrd_event_time")
            .unwrap();

        // Page index presence is indicated by offset_index_offset being set
        assert!(
            col.offset_index_offset().is_some() || col.column_index_offset().is_some(),
            "page index metadata not found"
        );
    }

    #[test]
    fn parquet_writer_returns_envelopes_unmodified() {
        // Encoding does not touch either the `AuditEvent` list or `ScribeAppendMeta` list
        let tenant = DataTenantId::new_v7();
        let tenant_string = tenant.to_string();
        let frozen = build_test_frozen(
            NaiveDate::from_ymd_opt(2026, 7, 14).unwrap(),
            tenant,
            vec![tenant_string.as_str()],
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

        let binding = TenantTableBinding::resolve((
            frozen_with_envelopes.seal_key.tenant,
            frozen_with_envelopes.seal_key.table.clone(),
        ))
        .unwrap();
        let encoded = encode_batch(
            &frozen_with_envelopes,
            &binding,
            frozen_with_envelopes.seal_key.tenant,
        )
        .unwrap();

        assert_eq!(encoded.audit_events.len(), 1);
        assert_eq!(encoded.append_metas.len(), 1);

        // Verify pairing preserved (index i ↔ index i)
        assert_eq!(encoded.audit_events[0].request_id, event.request_id);
        assert_eq!(encoded.append_metas[0].batch_id, meta.batch_id);
    }

    #[test]
    fn sort_batch_uses_one_key_for_every_table() {
        let schema = Arc::new(Schema::new(vec![
            arrow::datatypes::Field::new(DATA_TENANT_ID, DataType::Utf8, false),
            arrow::datatypes::Field::new(
                "wyrd_event_time",
                DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None),
                false,
            ),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["tenant-b", "tenant-a", "tenant-a"])),
                Arc::new(TimestampMicrosecondArray::from(vec![3, 2, 1])),
            ],
        )
        .unwrap();

        let sorted = sort_batch(&batch).unwrap();
        let tenants = sorted
            .column_by_name(DATA_TENANT_ID)
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let times = sorted
            .column_by_name("wyrd_event_time")
            .unwrap()
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .unwrap();
        assert_eq!((tenants.value(0), times.value(0)), ("tenant-a", 1));
        assert_eq!((tenants.value(1), times.value(1)), ("tenant-a", 2));
        assert_eq!((tenants.value(2), times.value(2)), ("tenant-b", 3));
    }

    #[test]
    fn missing_tenant_column_fails_closed() {
        let tenant = DataTenantId::new_v7();
        let schema = Arc::new(Schema::new(vec![arrow::datatypes::Field::new(
            "wyrd_event_time",
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None),
            false,
        )]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(TimestampMicrosecondArray::from(vec![1]))],
        )
        .unwrap();
        let frozen = FrozenMemtable {
            seal_id: 0,
            seal_key: SealKey::new(
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "events"),
                EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()),
            ),
            shard_id: 0,
            schema,
            batches: vec![batch],
            events: vec![],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        };
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone())).unwrap();

        let error = encode_batch(&frozen, &binding, tenant).unwrap_err();
        assert!(
            matches!(error, ScribeError::Internal { detail } if detail.contains(DATA_TENANT_ID))
        );
    }

    #[test]
    fn mismatched_tenant_value_fails_closed() {
        let expected = DataTenantId::new_v7();
        let observed = DataTenantId::new_v7();
        let expected_string = expected.to_string();
        let observed_string = observed.to_string();
        let frozen = build_test_frozen(
            NaiveDate::from_ymd_opt(2026, 7, 14).unwrap(),
            expected,
            vec![expected_string.as_str(), observed_string.as_str()],
            vec![1, 2],
        );
        let binding =
            TenantTableBinding::resolve((expected, frozen.seal_key.table.clone())).unwrap();

        let error = encode_batch(&frozen, &binding, expected).unwrap_err();
        assert!(matches!(error, ScribeError::Internal { detail }
            if detail.contains("row 1")
                && detail.contains(&expected.to_string())
                && detail.contains(&observed.to_string())));
    }

    #[test]
    fn tenant_binding_mismatch_fails_before_encoding() {
        let seal_tenant = DataTenantId::new_v7();
        let binding_tenant = DataTenantId::new_v7();
        let frozen = build_test_frozen(
            NaiveDate::from_ymd_opt(2026, 7, 14).unwrap(),
            seal_tenant,
            vec![seal_tenant.to_string().as_str()],
            vec![1],
        );
        let binding =
            TenantTableBinding::resolve((binding_tenant, frozen.seal_key.table.clone())).unwrap();

        let error = encode_batch(&frozen, &binding, seal_tenant).unwrap_err();
        assert!(
            matches!(error, ScribeError::Internal { detail } if detail.contains("binding mismatch"))
        );
    }
}
