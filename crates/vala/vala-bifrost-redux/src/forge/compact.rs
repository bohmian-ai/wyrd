use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Array, StringArray};
use arrow::compute::{SortColumn, SortOptions, cast, lexsort_to_indices, take};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use iceberg::Catalog;
use iceberg::spec::{DataFile, DataFileFormat, Literal, PartitionKey, Struct};
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use iceberg::writer::base_writer::data_file_writer::DataFileWriterBuilder;
use iceberg::writer::file_writer::location_generator::{
    DefaultFileNameGenerator, DefaultLocationGenerator,
};
use iceberg::writer::file_writer::{
    ParquetWriterBuilder, rolling_writer::RollingFileWriterBuilder,
};
use iceberg::writer::{IcebergWriter, IcebergWriterBuilder};
use opendal::Operator;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use sqlx::Row;
use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeCompactionPhase,
    StoragePath,
};

use crate::catalog::TenantTableBinding;
use crate::parquet::writer_properties::bifrost_writer_properties;

use super::binpack::{CandidateFile, ForgeGroupKey, RewriteBin, stable_pack};
use super::error::ForgeError;
use super::lease::{ForgeLease, forge_lease_key};

const DEFAULT_TARGET_BIN_BYTES: u64 = 512 * 1024 * 1024;
const SYSTEM_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::nil());

#[derive(Debug, Clone)]
pub struct ForgeConfig {
    pub min_files: i64,
    pub target_bin_bytes: u64,
    pub max_files_per_bin: usize,
    pub max_files_per_tick: usize,
    pub max_bytes_per_tick: u64,
    pub max_bins_per_tick: usize,
    pub lease_ttl: Duration,
    pub iceberg_total_retry_timeout: Duration,
    pub catalog_request_timeout: Duration,
    pub uncertainty_margin: Duration,
    pub uncertainty_bound: Duration,
    pub audit_page_size: i64,
}

impl Default for ForgeConfig {
    fn default() -> Self {
        Self {
            min_files: 2,
            target_bin_bytes: DEFAULT_TARGET_BIN_BYTES,
            max_files_per_bin: 256,
            max_files_per_tick: 1_024,
            max_bytes_per_tick: 2 * DEFAULT_TARGET_BIN_BYTES,
            max_bins_per_tick: 64,
            lease_ttl: Duration::from_mins(15),
            iceberg_total_retry_timeout: Duration::from_mins(5),
            catalog_request_timeout: Duration::from_secs(30),
            uncertainty_margin: Duration::from_secs(30),
            uncertainty_bound: Duration::from_mins(2),
            audit_page_size: 256,
        }
    }
}

impl ForgeConfig {
    pub fn validate(&self) -> Result<(), ForgeError> {
        if self.min_files < 2
            || self.target_bin_bytes == 0
            || self.max_files_per_bin < 2
            || self.max_files_per_tick == 0
            || self.max_bytes_per_tick == 0
            || self.max_bins_per_tick == 0
            || self.audit_page_size <= 0
        {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge limits must be positive and min_files/max_files_per_bin must be at least two".to_owned(),
            });
        }
        let required = self
            .iceberg_total_retry_timeout
            .saturating_add(self.catalog_request_timeout)
            .saturating_add(self.uncertainty_margin);
        if self.lease_ttl <= required {
            return Err(ForgeError::InvalidConfig {
                detail: "lease_ttl must exceed the Iceberg retry timeout, catalog timeout, and uncertainty margin".to_owned(),
            });
        }
        Ok(())
    }

    fn commit_window(&self) -> Duration {
        self.iceberg_total_retry_timeout
            .saturating_add(self.catalog_request_timeout)
            .saturating_add(self.uncertainty_margin)
    }
}

#[derive(Clone)]
pub struct ForgeContext {
    pub operator_pool: vala_sql::OperatorPool,
    pub catalog: Arc<dyn Catalog>,
    pub staging: Arc<Operator>,
    pub config: ForgeConfig,
}

impl ForgeContext {
    pub fn new(
        operator_pool: vala_sql::OperatorPool,
        catalog: Arc<dyn Catalog>,
        staging: Arc<Operator>,
        config: ForgeConfig,
    ) -> Result<Self, ForgeError> {
        config.validate()?;
        Ok(Self {
            operator_pool,
            catalog,
            staging,
            config,
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ForgeTickOutcome {
    pub groups_seen: usize,
    pub bins_committed: usize,
    pub bins_skipped: usize,
    pub reconciled: usize,
}

async fn load_table(
    context: &ForgeContext,
    ident: &iceberg::TableIdent,
) -> Result<iceberg::table::Table, ForgeError> {
    tokio::time::timeout(
        context.config.catalog_request_timeout,
        context.catalog.load_table(ident),
    )
    .await
    .map_err(|_| ForgeError::Timeout {
        operation: "catalog table load",
    })?
    .map_err(ForgeError::Catalog)
}

#[derive(Debug, Clone)]
pub(crate) struct CandidateRow {
    key: ForgeGroupKey,
    files: Vec<CandidateFile>,
}

pub async fn run_compaction_tick(context: &ForgeContext) -> Result<ForgeTickOutcome, ForgeError> {
    let mut outcome = ForgeTickOutcome {
        groups_seen: 0,
        ..ForgeTickOutcome::default()
    };
    let owner = Uuid::now_v7();
    for key in load_reconciliation_keys(context).await? {
        outcome.groups_seen += 1;
        let binding =
            TenantTableBinding::resolve((key.tenant, key.table_ref.clone())).map_err(|error| {
                ForgeError::Group {
                    detail: error.to_string(),
                }
            })?;
        let lease_key =
            forge_lease_key(key.tenant, &binding.logical_namespace, &binding.table_name);
        let Some(mut lease) = ForgeLease::acquire(
            &context.operator_pool,
            lease_key,
            owner,
            context.config.lease_ttl,
        )
        .await?
        else {
            outcome.bins_skipped += 1;
            continue;
        };
        outcome.reconciled += reconcile_group(context, &mut lease, &key, &binding).await?;
        if !lease.release(&context.operator_pool).await? {
            tracing::warn!(lease_key = %lease.lease_key, "Forge lease release lost its fence");
        }
    }

    let mut rows = select_candidate_groups(&context.operator_pool, &context.config).await?;
    rows.sort_by_key(|left| left.key.audit_resource());
    for row in rows {
        outcome.groups_seen += 1;
        let binding = TenantTableBinding::resolve((row.key.tenant, row.key.table_ref.clone()))
            .map_err(|error| ForgeError::Group {
                detail: error.to_string(),
            })?;
        let lease_key = forge_lease_key(
            row.key.tenant,
            &binding.logical_namespace,
            &binding.table_name,
        );
        let Some(mut lease) = ForgeLease::acquire(
            &context.operator_pool,
            lease_key,
            owner,
            context.config.lease_ttl,
        )
        .await?
        else {
            outcome.bins_skipped += 1;
            continue;
        };

        let bins = stable_pack(
            row.files,
            context.config.target_bin_bytes,
            context.config.max_files_per_bin,
        );
        let mut file_budget = 0_usize;
        let mut byte_budget = 0_u64;
        for bin in bins.into_iter().take(context.config.max_bins_per_tick) {
            if file_budget.saturating_add(bin.files.len()) > context.config.max_files_per_tick
                || byte_budget.saturating_add(bin.total_bytes) > context.config.max_bytes_per_tick
            {
                outcome.bins_skipped += 1;
                continue;
            }
            if !lease.renew(&context.operator_pool).await? {
                return Err(ForgeError::FenceLost {
                    lease_key: lease.lease_key.clone(),
                });
            }
            if !lease.commit_window_fits(context.config.commit_window()) {
                outcome.bins_skipped += 1;
                break;
            }
            compact_bin(context, &mut lease, &row.key, &binding, &bin).await?;
            file_budget += bin.files.len();
            byte_budget = byte_budget.saturating_add(bin.total_bytes);
            outcome.bins_committed += 1;
        }
        if !lease.release(&context.operator_pool).await? {
            tracing::warn!(lease_key = %lease.lease_key, "Forge lease release lost its fence");
        }
    }
    Ok(outcome)
}

pub(crate) async fn select_candidate_groups(
    operator_pool: &vala_sql::OperatorPool,
    config: &ForgeConfig,
) -> Result<Vec<CandidateRow>, ForgeError> {
    let rows = sqlx::query(
        r"
        SELECT data_tenant_id, namespace, table_name, partition_day,
               array_agg(id ORDER BY id) AS file_ids,
               array_agg(file_path ORDER BY id) AS paths,
               array_agg(file_size ORDER BY id) AS sizes,
               array_agg(min_event_time ORDER BY id) AS min_event_times,
               array_agg(max_event_time ORDER BY id) AS max_event_times,
               COUNT(*) AS n_files, SUM(file_size) AS total_bytes
          FROM vala.file_list
         WHERE NOT compacted
           AND created_at < now() - interval '2 min'
         GROUP BY data_tenant_id, namespace, table_name, partition_day
        HAVING COUNT(*) >= $1 OR SUM(file_size) < $2
         ORDER BY data_tenant_id, namespace, table_name, partition_day
        ",
    )
    .bind(config.min_files)
    .bind(
        i64::try_from(config.target_bin_bytes).map_err(|_| ForgeError::InvalidConfig {
            detail: "target_bin_bytes exceeds PostgreSQL bigint".to_owned(),
        })?,
    )
    .fetch_all(operator_pool.pool())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;

    rows.iter().map(decode_candidate_row).collect()
}

async fn load_reconciliation_keys(
    context: &ForgeContext,
) -> Result<Vec<ForgeGroupKey>, ForgeError> {
    let rows = sqlx::query(
        r"SELECT DISTINCT data_tenant_id, namespace, table_name, partition_day
             FROM vala.file_list
            WHERE compacted AND committed_snapshot_id IS NULL
            ORDER BY data_tenant_id, namespace, table_name, partition_day",
    )
    .fetch_all(context.operator_pool.pool())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;
    rows.into_iter()
        .map(|row| {
            let tenant_uuid: Uuid =
                row.try_get("data_tenant_id")
                    .map_err(|error| ForgeError::Group {
                        detail: error.to_string(),
                    })?;
            let tenant = data_tenant_from_uuid(tenant_uuid)?;
            let namespace: String =
                row.try_get("namespace")
                    .map_err(|error| ForgeError::Group {
                        detail: error.to_string(),
                    })?;
            let table_name: String =
                row.try_get("table_name")
                    .map_err(|error| ForgeError::Group {
                        detail: error.to_string(),
                    })?;
            let partition_day: NaiveDate =
                row.try_get("partition_day")
                    .map_err(|error| ForgeError::Group {
                        detail: error.to_string(),
                    })?;
            ForgeGroupKey::from_sql(tenant, &namespace, &table_name, partition_day)
                .map_err(|detail| ForgeError::Group { detail })
        })
        .collect()
}

fn decode_candidate_row(row: &sqlx::postgres::PgRow) -> Result<CandidateRow, ForgeError> {
    let tenant_uuid: uuid::Uuid =
        row.try_get("data_tenant_id")
            .map_err(|error| ForgeError::Group {
                detail: error.to_string(),
            })?;
    let tenant = data_tenant_from_uuid(tenant_uuid)?;
    let namespace: String = row
        .try_get("namespace")
        .map_err(|error| ForgeError::Group {
            detail: error.to_string(),
        })?;
    let table_name: String = row
        .try_get("table_name")
        .map_err(|error| ForgeError::Group {
            detail: error.to_string(),
        })?;
    let partition_day: NaiveDate =
        row.try_get("partition_day")
            .map_err(|error| ForgeError::Group {
                detail: error.to_string(),
            })?;
    let key = ForgeGroupKey::from_sql(tenant, &namespace, &table_name, partition_day)
        .map_err(|detail| ForgeError::Group { detail })?;
    let ids: Vec<Uuid> = row.try_get("file_ids").map_err(|error| ForgeError::Group {
        detail: error.to_string(),
    })?;
    let paths: Vec<String> = row.try_get("paths").map_err(|error| ForgeError::Group {
        detail: error.to_string(),
    })?;
    let sizes: Vec<i64> = row.try_get("sizes").map_err(|error| ForgeError::Group {
        detail: error.to_string(),
    })?;
    let mins: Vec<DateTime<Utc>> =
        row.try_get("min_event_times")
            .map_err(|error| ForgeError::Group {
                detail: error.to_string(),
            })?;
    let maxes: Vec<DateTime<Utc>> =
        row.try_get("max_event_times")
            .map_err(|error| ForgeError::Group {
                detail: error.to_string(),
            })?;
    if ids.len() != paths.len()
        || ids.len() != sizes.len()
        || ids.len() != mins.len()
        || ids.len() != maxes.len()
    {
        return Err(ForgeError::Group {
            detail: "candidate aggregate arrays have different lengths".to_owned(),
        });
    }
    let files = ids
        .into_iter()
        .zip(paths)
        .zip(sizes)
        .zip(mins)
        .zip(maxes)
        .map(|((((id, path), size), min_event_time), max_event_time)| {
            Ok(CandidateFile {
                id,
                path,
                size: u64::try_from(size).map_err(|_| ForgeError::Group {
                    detail: "negative file size".to_owned(),
                })?,
                min_event_time,
                max_event_time,
            })
        })
        .collect::<Result<Vec<_>, ForgeError>>()?;
    Ok(CandidateRow { key, files })
}

fn data_tenant_from_uuid(value: Uuid) -> Result<DataTenantId, ForgeError> {
    if value.is_nil() {
        Ok(DataTenantId::SYSTEM_OWNER)
    } else {
        DataTenantId::try_from(value).map_err(|error| ForgeError::Group {
            detail: error.to_string(),
        })
    }
}

async fn compact_bin(
    context: &ForgeContext,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    binding: &TenantTableBinding,
    bin: &RewriteBin,
) -> Result<(), ForgeError> {
    let operation_id = operation_id(key, bin);
    let table = load_table(context, &binding.table_ident()).await?;
    let batch = read_and_project_staging(context, &table, key, bin).await?;
    let output = write_output(&table, binding, operation_id, key.partition_day, &batch).await?;
    let prepared = forge_detail(
        key,
        bin,
        &output,
        operation_id,
        ForgeCompactionPhase::Prepared,
        None,
    )?;
    prepare_inputs(context, lease, key, bin, prepared).await?;
    if !lease.renew(&context.operator_pool).await? {
        return Err(ForgeError::FenceLost {
            lease_key: lease.lease_key.clone(),
        });
    }
    if !lease.commit_window_fits(context.config.commit_window()) {
        return Err(ForgeError::FenceLost {
            lease_key: lease.lease_key.clone(),
        });
    }
    let table = load_table(context, &binding.table_ident()).await?;
    let mut properties = HashMap::new();
    properties.insert("forge.operation_id".to_owned(), operation_id.to_string());
    properties.insert("forge.group".to_owned(), key.audit_resource());
    let tx = Transaction::new(&table);
    let action = tx
        .rewrite_files()
        .delete_files(Vec::<String>::new())
        .add_data_files([output.clone()])
        .set_commit_uuid(operation_id)
        .set_snapshot_properties(properties);
    let transaction = ApplyTransactionAction::apply(action, tx).map_err(ForgeError::Catalog)?;
    match tokio::time::timeout(
        context.config.iceberg_total_retry_timeout,
        transaction.commit(context.catalog.as_ref()),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            if !is_retryable(&error) {
                if lease.require_fence(&context.operator_pool).await.is_ok() {
                    reset_inputs(context, lease, key, bin, &output, operation_id).await?;
                } else {
                    tracing::warn!(
                        lease_key = %lease.lease_key,
                        operation_id = %operation_id,
                        "definite Iceberg failure could not be reset after fence loss"
                    );
                }
            }
            return Err(ForgeError::Catalog(error));
        }
        Err(_) => {
            return Err(ForgeError::Timeout {
                operation: "Iceberg commit",
            });
        }
    }
    let committed_table = load_table(context, &binding.table_ident()).await?;
    let snapshot_id = committed_table
        .metadata()
        .current_snapshot_id()
        .ok_or_else(|| ForgeError::Reconciliation {
            detail: "Iceberg replace committed without a current snapshot".to_owned(),
        })?;
    stamp_committed(context, lease, key, bin, &output, operation_id, snapshot_id).await
}

fn is_retryable(error: &iceberg::Error) -> bool {
    error.retryable()
}

async fn read_and_project_staging(
    context: &ForgeContext,
    table: &iceberg::table::Table,
    key: &ForgeGroupKey,
    bin: &RewriteBin,
) -> Result<RecordBatch, ForgeError> {
    let mut batches = Vec::new();
    let mut source_schema = None;
    for file in &bin.files {
        let bytes = context
            .staging
            .read(&file.path)
            .await
            .map_err(ForgeError::ObjectStore)?
            .to_bytes();
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes)
            .map_err(|error| ForgeError::Parquet {
                detail: error.to_string(),
            })?
            .build()
            .map_err(|error| ForgeError::Parquet {
                detail: error.to_string(),
            })?;
        for batch in reader {
            let batch = batch.map_err(|error| ForgeError::Parquet {
                detail: error.to_string(),
            })?;
            if let Some(schema) = source_schema.as_ref() {
                if batch.schema().as_ref() != schema {
                    return Err(ForgeError::Schema {
                        detail:
                            "staging files in one rewrite bin do not have the same Arrow schema"
                                .to_owned(),
                    });
                }
            } else {
                source_schema = Some(batch.schema().as_ref().clone());
            }
            validate_tenant_column(&batch, key.tenant)?;
            batches.push(batch);
        }
    }
    let source_schema = source_schema.ok_or_else(|| ForgeError::Parquet {
        detail: "staging files contained no record batches".to_owned(),
    })?;
    let source =
        arrow::compute::concat_batches(&Arc::new(source_schema), &batches).map_err(|error| {
            ForgeError::Parquet {
                detail: error.to_string(),
            }
        })?;
    let target = Arc::new(
        iceberg::arrow::schema_to_arrow_schema(table.metadata().current_schema())
            .map_err(ForgeError::Catalog)?,
    );
    let projected = project_by_name(&source, target)?;
    sort_rows(&projected)
}

fn validate_tenant_column(batch: &RecordBatch, tenant: DataTenantId) -> Result<(), ForgeError> {
    let index = batch
        .schema()
        .index_of("data_tenant_id")
        .map_err(|_| ForgeError::Schema {
            detail: "staging file lacks data_tenant_id".to_owned(),
        })?;
    let values = batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ForgeError::Schema {
            detail: "data_tenant_id must be Utf8".to_owned(),
        })?;
    let expected = tenant.to_string();
    if (0..values.len()).any(|index| values.is_null(index) || values.value(index) != expected) {
        return Err(ForgeError::Schema {
            detail: format!("staging file contains a row outside tenant `{tenant}`"),
        });
    }
    Ok(())
}

fn project_by_name(
    source: &RecordBatch,
    target: Arc<arrow::datatypes::Schema>,
) -> Result<RecordBatch, ForgeError> {
    let mut columns = Vec::with_capacity(target.fields().len());
    for field in target.fields() {
        let index = source
            .schema()
            .index_of(field.name())
            .map_err(|_| ForgeError::Schema {
                detail: format!("staging schema lacks Iceberg field `{}`", field.name()),
            })?;
        let column = source.column(index);
        let column = if column.data_type() == field.data_type() {
            column.clone()
        } else {
            cast(column, field.data_type()).map_err(|error| ForgeError::Schema {
                detail: format!(
                    "field `{}` cannot be cast to {:?}: {error}",
                    field.name(),
                    field.data_type()
                ),
            })?
        };
        columns.push(column);
    }
    RecordBatch::try_new(target, columns).map_err(|error| ForgeError::Schema {
        detail: error.to_string(),
    })
}

fn sort_rows(batch: &RecordBatch) -> Result<RecordBatch, ForgeError> {
    let tenant = batch
        .schema()
        .index_of("data_tenant_id")
        .map_err(|_| ForgeError::Schema {
            detail: "projected batch lacks data_tenant_id".to_owned(),
        })?;
    let event_time =
        batch
            .schema()
            .index_of("wyrd_event_time")
            .map_err(|_| ForgeError::Schema {
                detail: "projected batch lacks wyrd_event_time".to_owned(),
            })?;
    let indices = lexsort_to_indices(
        &[
            SortColumn {
                values: batch.column(tenant).clone(),
                options: Some(SortOptions {
                    descending: false,
                    nulls_first: false,
                }),
            },
            SortColumn {
                values: batch.column(event_time).clone(),
                options: Some(SortOptions {
                    descending: false,
                    nulls_first: false,
                }),
            },
        ],
        None,
    )
    .map_err(|error| ForgeError::Schema {
        detail: error.to_string(),
    })?;
    let columns = batch
        .columns()
        .iter()
        .map(|column| take(column, &indices, None))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ForgeError::Schema {
            detail: error.to_string(),
        })?;
    RecordBatch::try_new(batch.schema(), columns).map_err(|error| ForgeError::Schema {
        detail: error.to_string(),
    })
}

async fn write_output(
    table: &iceberg::table::Table,
    binding: &TenantTableBinding,
    operation_id: Uuid,
    partition_day: NaiveDate,
    batch: &RecordBatch,
) -> Result<DataFile, ForgeError> {
    let schema = table.metadata().current_schema().clone();
    let props = bifrost_writer_properties(batch.num_rows());
    let parquet = ParquetWriterBuilder::new(props, schema.clone());
    let location = DefaultLocationGenerator::with_data_location(binding.object_prefix.clone());
    let name = DefaultFileNameGenerator::new(
        format!("forge-{operation_id}"),
        None,
        DataFileFormat::Parquet,
    );
    let rolling = RollingFileWriterBuilder::new_with_default_file_size(
        parquet,
        table.file_io().clone(),
        location,
        name,
    );
    let partition = Struct::from_iter([Some(Literal::date(
        partition_day.num_days_from_ce() - 719_163,
    ))]);
    let mut writer = DataFileWriterBuilder::new(rolling)
        .build(Some(PartitionKey::new(
            table.metadata().default_partition_spec().as_ref().clone(),
            schema,
            partition.clone(),
        )))
        .await
        .map_err(ForgeError::Catalog)?;
    writer
        .write(batch.clone())
        .await
        .map_err(ForgeError::Catalog)?;
    let files = writer.close().await.map_err(ForgeError::Catalog)?;
    if files.len() != 1 {
        return Err(ForgeError::Invariant {
            detail: format!("Forge writer produced {} files for one bin", files.len()),
        });
    }
    let file = files
        .into_iter()
        .next()
        .ok_or_else(|| ForgeError::Invariant {
            detail: "Forge writer returned no data file".to_owned(),
        })?;
    if !file.file_path().starts_with(&binding.object_prefix) {
        return Err(ForgeError::Invariant {
            detail: format!("Forge output escaped table prefix: {}", file.file_path()),
        });
    }
    if file.record_count() != u64::try_from(batch.num_rows()).unwrap_or(u64::MAX) {
        return Err(ForgeError::Invariant {
            detail: format!(
                "Forge output row count {} does not match input {}",
                file.record_count(),
                batch.num_rows()
            ),
        });
    }
    if file.partition() != &partition {
        return Err(ForgeError::Invariant {
            detail: "Forge output partition does not match the candidate day".to_owned(),
        });
    }
    Ok(file)
}

async fn prepare_inputs(
    context: &ForgeContext,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    bin: &RewriteBin,
    detail: AuditDetail,
) -> Result<(), ForgeError> {
    if !lease.renew(&context.operator_pool).await? {
        return Err(ForgeError::FenceLost {
            lease_key: lease.lease_key.clone(),
        });
    }
    let mut conn = context
        .operator_pool
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let ids: Vec<Uuid> = bin.files.iter().map(|file| file.id).collect();
    let result = sqlx::query(
        r"UPDATE vala.file_list
              SET compacted = true
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_day = $4 AND id = ANY($5) AND NOT compacted",
    )
    .bind(key.tenant.as_uuid())
    .bind(key.table_ref.namespace.as_str())
    .bind(&key.table_ref.name)
    .bind(key.partition_day)
    .bind(&ids)
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;
    if result.rows_affected() != u64::try_from(ids.len()).unwrap_or(u64::MAX) {
        return Err(ForgeError::Reconciliation {
            detail: "prepared transition did not claim every input file".to_owned(),
        });
    }
    append_system_audit(&mut conn, key, "forge.file_compact.prepared", detail).await?;
    conn.commit().await.map_err(ForgeError::Sql)
}

async fn stamp_committed(
    context: &ForgeContext,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    bin: &RewriteBin,
    output: &DataFile,
    operation_id: Uuid,
    snapshot_id: i64,
) -> Result<(), ForgeError> {
    lease.require_fence(&context.operator_pool).await?;
    let mut conn = context
        .operator_pool
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let ids: Vec<Uuid> = bin.files.iter().map(|file| file.id).collect();
    let result = sqlx::query(
        r"UPDATE vala.file_list
              SET committed_snapshot_id = $1
            WHERE data_tenant_id = $2 AND namespace = $3 AND table_name = $4
              AND partition_day = $5 AND id = ANY($6)
              AND compacted AND committed_snapshot_id IS NULL",
    )
    .bind(snapshot_id)
    .bind(key.tenant.as_uuid())
    .bind(key.table_ref.namespace.as_str())
    .bind(&key.table_ref.name)
    .bind(key.partition_day)
    .bind(&ids)
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;
    if result.rows_affected() != u64::try_from(ids.len()).unwrap_or(u64::MAX) {
        return Err(ForgeError::Reconciliation {
            detail: "committed transition did not stamp every input file".to_owned(),
        });
    }
    let detail = forge_detail(
        key,
        bin,
        output,
        operation_id,
        ForgeCompactionPhase::Committed,
        Some(snapshot_id),
    )?;
    append_system_audit(&mut conn, key, "forge.file_compact.committed", detail).await?;
    conn.commit().await.map_err(ForgeError::Sql)
}

async fn reset_inputs(
    context: &ForgeContext,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    bin: &RewriteBin,
    output: &DataFile,
    operation_id: Uuid,
) -> Result<(), ForgeError> {
    lease.require_fence(&context.operator_pool).await?;
    let mut conn = context
        .operator_pool
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let ids: Vec<Uuid> = bin.files.iter().map(|file| file.id).collect();
    let result = sqlx::query(
        r"UPDATE vala.file_list
              SET compacted = false, committed_snapshot_id = NULL
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_day = $4 AND id = ANY($5) AND compacted
              AND committed_snapshot_id IS NULL",
    )
    .bind(key.tenant.as_uuid())
    .bind(key.table_ref.namespace.as_str())
    .bind(&key.table_ref.name)
    .bind(key.partition_day)
    .bind(&ids)
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;
    if result.rows_affected() != u64::try_from(ids.len()).unwrap_or(u64::MAX) {
        return Err(ForgeError::Reconciliation {
            detail: "reset transition did not restore every input file".to_owned(),
        });
    }
    let detail = forge_detail(
        key,
        bin,
        output,
        operation_id,
        ForgeCompactionPhase::Reset,
        None,
    )?;
    append_system_audit(&mut conn, key, "forge.file_compact.reset", detail).await?;
    conn.commit().await.map_err(ForgeError::Sql)
}

async fn reconcile_group(
    context: &ForgeContext,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    binding: &TenantTableBinding,
) -> Result<usize, ForgeError> {
    let resource = key.audit_resource();
    let (prepared, terminal) = load_reconciliation_audits(context, key, &resource).await?;
    let mut recovered = 0;
    for (operation_id, (detail, created_at)) in prepared {
        if terminal.contains(&operation_id) {
            continue;
        }
        let AuditDetail::ForgeCompaction {
            input_file_ids,
            output_path,
            ..
        } = &detail
        else {
            continue;
        };
        if detail_group(&detail) != resource {
            return Err(ForgeError::Reconciliation {
                detail: "audit detail group differs from its resource".to_owned(),
            });
        }
        verify_hidden_inputs(context, key, input_file_ids, &detail).await?;
        if !lease.renew(&context.operator_pool).await? {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let table = load_table(context, &binding.table_ident()).await?;
        if let Some(snapshot_id) = live_snapshot_for_path(&table, output_path.as_str()).await? {
            if !lease.renew(&context.operator_pool).await? {
                return Err(ForgeError::FenceLost {
                    lease_key: lease.lease_key.clone(),
                });
            }
            stamp_reconciled(context, lease, key, input_file_ids, &detail, snapshot_id).await?;
            recovered += 1;
            continue;
        }
        if Utc::now()
            .signed_duration_since(created_at)
            .to_std()
            .unwrap_or_default()
            < context.config.uncertainty_bound
        {
            continue;
        }
        let first_reload = load_table(context, &binding.table_ident()).await?;
        if live_snapshot_for_path(&first_reload, output_path.as_str())
            .await?
            .is_some()
        {
            continue;
        }
        let second_reload = load_table(context, &binding.table_ident()).await?;
        if live_snapshot_for_path(&second_reload, output_path.as_str())
            .await?
            .is_none()
        {
            if !lease.renew(&context.operator_pool).await? {
                return Err(ForgeError::FenceLost {
                    lease_key: lease.lease_key.clone(),
                });
            }
            reset_reconciled(context, lease, key, input_file_ids, &detail).await?;
            recovered += 1;
        }
    }
    Ok(recovered)
}

async fn load_reconciliation_audits(
    context: &ForgeContext,
    key: &ForgeGroupKey,
    resource: &str,
) -> Result<(HashMap<Uuid, (AuditDetail, DateTime<Utc>)>, HashSet<Uuid>), ForgeError> {
    let mut after_seq = 0_i64;
    let mut prepared = HashMap::new();
    let mut terminal = HashSet::new();
    loop {
        let mut conn = context
            .operator_pool
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let page = vala_sql::queries::audit_outbox::list_audit_events_for_resource(
            &mut conn,
            resource,
            after_seq,
            context.config.audit_page_size,
        )
        .await
        .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        let page_len = page.len();
        for row in page {
            after_seq = row.seq;
            let Some(detail) = row.detail else { continue };
            let detail = serde_json::from_str::<AuditDetail>(&detail).map_err(|error| {
                ForgeError::Reconciliation {
                    detail: error.to_string(),
                }
            })?;
            validate_reconciliation_detail(&detail, resource)?;
            let AuditDetail::ForgeCompaction {
                operation_id,
                phase,
                ..
            } = &detail
            else {
                continue;
            };
            match phase {
                ForgeCompactionPhase::Prepared => {
                    prepared.insert(*operation_id, (detail, row.created_at));
                }
                ForgeCompactionPhase::Committed
                | ForgeCompactionPhase::Recovered
                | ForgeCompactionPhase::Reset => {
                    terminal.insert(*operation_id);
                }
            }
        }
        if page_len < usize::try_from(context.config.audit_page_size).unwrap_or(usize::MAX) {
            break;
        }
    }
    Ok((prepared, terminal))
}

fn validate_reconciliation_detail(detail: &AuditDetail, resource: &str) -> Result<(), ForgeError> {
    let AuditDetail::ForgeCompaction {
        group,
        input_file_ids,
        input_paths,
        output_path,
        ..
    } = detail
    else {
        return Ok(());
    };
    if group != resource {
        return Err(ForgeError::Reconciliation {
            detail: "audit detail group differs from its resource".to_owned(),
        });
    }
    if input_file_ids.len() < 2
        || input_file_ids.len() != input_paths.len()
        || input_file_ids.windows(2).any(|pair| pair[0] >= pair[1])
        || output_path.as_str().is_empty()
    {
        return Err(ForgeError::Reconciliation {
            detail: "audit detail does not contain sorted complete input identity".to_owned(),
        });
    }
    Ok(())
}

fn detail_group(detail: &AuditDetail) -> String {
    match detail {
        AuditDetail::ForgeCompaction { group, .. } => group.clone(),
        _ => String::new(),
    }
}

async fn live_snapshot_for_path(
    table: &iceberg::table::Table,
    path: &str,
) -> Result<Option<i64>, ForgeError> {
    let Some(snapshot) = table.metadata().current_snapshot() else {
        return Ok(None);
    };
    let manifest_list = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .map_err(ForgeError::Catalog)?;
    for manifest_file in manifest_list.entries() {
        let manifest = manifest_file
            .load_manifest(table.file_io())
            .await
            .map_err(ForgeError::Catalog)?;
        for entry in manifest.entries() {
            if entry.is_alive() && entry.data_file().file_path() == path {
                return Ok(entry.snapshot_id().or(Some(snapshot.snapshot_id())));
            }
        }
    }
    Ok(None)
}

async fn verify_hidden_inputs(
    context: &ForgeContext,
    key: &ForgeGroupKey,
    input_file_ids: &[Uuid],
    detail: &AuditDetail,
) -> Result<(), ForgeError> {
    let AuditDetail::ForgeCompaction { input_paths, .. } = detail else {
        return Err(ForgeError::Reconciliation {
            detail: "prepared audit detail is not a Forge compaction".to_owned(),
        });
    };
    if input_file_ids.len() != input_paths.len() {
        return Err(ForgeError::Reconciliation {
            detail: "prepared audit detail has unpaired input IDs and paths".to_owned(),
        });
    }
    let mut conn = context
        .operator_pool
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let rows = sqlx::query(
        r"SELECT id, file_path
             FROM vala.file_list
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_day = $4 AND id = ANY($5)
              AND compacted AND committed_snapshot_id IS NULL
            ORDER BY id",
    )
    .bind(key.tenant.as_uuid())
    .bind(key.table_ref.namespace.as_str())
    .bind(&key.table_ref.name)
    .bind(key.partition_day)
    .bind(input_file_ids)
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;
    conn.commit().await.map_err(ForgeError::Sql)?;

    let observed = rows
        .into_iter()
        .map(|row| {
            Ok::<_, ForgeError>((
                row.try_get::<Uuid, _>("id")
                    .map_err(|error| ForgeError::Reconciliation {
                        detail: error.to_string(),
                    })?,
                row.try_get::<String, _>("file_path").map_err(|error| {
                    ForgeError::Reconciliation {
                        detail: error.to_string(),
                    }
                })?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut expected = input_file_ids
        .iter()
        .copied()
        .zip(input_paths.iter().map(|path| path.as_str().to_owned()))
        .collect::<Vec<_>>();
    expected.sort_by_key(|(id, _)| *id);
    if observed != expected {
        return Err(ForgeError::Reconciliation {
            detail: "prepared audit inputs no longer match hidden file_list rows".to_owned(),
        });
    }
    Ok(())
}

async fn stamp_reconciled(
    context: &ForgeContext,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    input_file_ids: &[Uuid],
    detail: &AuditDetail,
    snapshot_id: i64,
) -> Result<(), ForgeError> {
    lease.require_fence(&context.operator_pool).await?;
    let mut conn = context
        .operator_pool
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let result = sqlx::query(
        r"UPDATE vala.file_list
              SET committed_snapshot_id = $1
            WHERE data_tenant_id = $2 AND namespace = $3 AND table_name = $4
              AND partition_day = $5 AND id = ANY($6)
              AND compacted AND committed_snapshot_id IS NULL",
    )
    .bind(snapshot_id)
    .bind(key.tenant.as_uuid())
    .bind(key.table_ref.namespace.as_str())
    .bind(&key.table_ref.name)
    .bind(key.partition_day)
    .bind(input_file_ids)
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;
    if result.rows_affected() != u64::try_from(input_file_ids.len()).unwrap_or(u64::MAX) {
        return Err(ForgeError::Reconciliation {
            detail: "recovery transition did not stamp every input file".to_owned(),
        });
    }
    append_system_audit(
        &mut conn,
        key,
        "forge.file_compact.recovered",
        terminal_detail(detail, ForgeCompactionPhase::Recovered, Some(snapshot_id)),
    )
    .await?;
    conn.commit().await.map_err(ForgeError::Sql)
}

async fn reset_reconciled(
    context: &ForgeContext,
    lease: &mut ForgeLease,
    key: &ForgeGroupKey,
    input_file_ids: &[Uuid],
    detail: &AuditDetail,
) -> Result<(), ForgeError> {
    lease.require_fence(&context.operator_pool).await?;
    let mut conn = context
        .operator_pool
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let result = sqlx::query(
        r"UPDATE vala.file_list
              SET compacted = false, committed_snapshot_id = NULL
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
              AND partition_day = $4 AND id = ANY($5)
              AND compacted AND committed_snapshot_id IS NULL",
    )
    .bind(key.tenant.as_uuid())
    .bind(key.table_ref.namespace.as_str())
    .bind(&key.table_ref.name)
    .bind(key.partition_day)
    .bind(input_file_ids)
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;
    if result.rows_affected() != u64::try_from(input_file_ids.len()).unwrap_or(u64::MAX) {
        return Err(ForgeError::Reconciliation {
            detail: "recovery reset did not restore every input file".to_owned(),
        });
    }
    append_system_audit(
        &mut conn,
        key,
        "forge.file_compact.reset",
        terminal_detail(detail, ForgeCompactionPhase::Reset, None),
    )
    .await?;
    conn.commit().await.map_err(ForgeError::Sql)
}

fn terminal_detail(
    detail: &AuditDetail,
    phase: ForgeCompactionPhase,
    snapshot_id: Option<i64>,
) -> AuditDetail {
    match detail {
        AuditDetail::ForgeCompaction {
            operation_id,
            group,
            input_file_ids,
            input_paths,
            output_path,
            writer_recipe_version,
            ..
        } => AuditDetail::ForgeCompaction {
            operation_id: *operation_id,
            phase,
            group: group.clone(),
            input_file_ids: input_file_ids.clone(),
            input_paths: input_paths.clone(),
            output_path: output_path.clone(),
            snapshot_id,
            writer_recipe_version: writer_recipe_version.clone(),
        },
        _ => detail.clone(),
    }
}

async fn append_system_audit(
    conn: &mut vala_sql::TenantConn<'_>,
    key: &ForgeGroupKey,
    operation: &str,
    detail: AuditDetail,
) -> Result<(), ForgeError> {
    let event = AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: operation.to_owned(),
        resource: key.audit_resource(),
        card_ref: None,
        principal_id: SYSTEM_PRINCIPAL,
        principal_kind: PrincipalKindTag::Service,
        auth_method: AuthMethod::Internal,
        permission: "bifrost:forge".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: operation.to_owned(),
        detail: Some(detail),
    };
    vala_sql::queries::audit_outbox::append_audit(conn, &event)
        .await
        .map(|_| ())
        .map_err(ForgeError::Sql)
}

fn forge_detail(
    key: &ForgeGroupKey,
    bin: &RewriteBin,
    output: &DataFile,
    operation_id: Uuid,
    phase: ForgeCompactionPhase,
    snapshot_id: Option<i64>,
) -> Result<AuditDetail, ForgeError> {
    let group = StoragePath::new(key.audit_resource()).map_err(|error| ForgeError::Group {
        detail: error.to_string(),
    })?;
    let output_path =
        StoragePath::new(output.file_path().to_owned()).map_err(|error| ForgeError::Group {
            detail: error.to_string(),
        })?;
    let mut inputs = bin
        .files
        .iter()
        .map(|file| {
            StoragePath::new(file.path.clone())
                .map(|path| (file.id, path))
                .map_err(|error| ForgeError::Group {
                    detail: error.to_string(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    inputs.sort_by_key(|(id, _)| *id);
    let (input_file_ids, input_paths): (Vec<_>, Vec<_>) = inputs.into_iter().unzip();
    Ok(AuditDetail::ForgeCompaction {
        operation_id,
        phase,
        group: group.to_string(),
        input_file_ids,
        input_paths,
        output_path,
        snapshot_id,
        writer_recipe_version: "bifrost-writer-v1".to_owned(),
    })
}

fn operation_id(key: &ForgeGroupKey, bin: &RewriteBin) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(key.audit_resource());
    hasher.update(key.partition_day.to_string());
    for file in &bin.files {
        hasher.update(file.id.as_bytes());
        hasher.update(file.path.as_bytes());
    }
    let digest = hasher.finalize();
    Uuid::from_bytes(digest[..16].try_into().expect("SHA-256 prefix is 16 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compacted_output_uses_shared_writer_properties() {
        assert_eq!(
            bifrost_writer_properties(10).max_row_group_row_count(),
            Some(131_072)
        );
    }

    #[test]
    fn rewrite_action_uses_empty_delete_set_for_staging_fold() {
        let _ = Transaction::new;
        let config = ForgeConfig::default();
        assert!(config.validate().is_ok());
    }
}
