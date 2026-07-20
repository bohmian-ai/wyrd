//! Shared Postgres-backed Forge fixture for gated journeys and interleavings.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use arrow::array::{Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema, TimeUnit};
use async_trait::async_trait;
use iceberg::table::Table;
use iceberg::{Catalog, Namespace, NamespaceIdent, TableCommit, TableCreation, TableIdent};
use iceberg::{Error as IcebergError, ErrorKind as IcebergErrorKind};
use opendal::{
    Buffer, Entry, Error as ObjectStoreError, ErrorKind as ObjectStoreErrorKind, Metadata, Operator,
};
use parquet::arrow::ArrowWriter;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding, build_partition_spec};
use vala_bifrost_redux::forge::{ForgeConfig, ForgeContext, ForgeObjectStore};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_spec::DataTenantId;

use super::super::WyrdTestServer;

/// Durable objects and SQL identity used by a Forge integration test.
#[derive(Clone)]
pub struct ForgeFixture {
    /// The server-owned Forge context.
    pub context: Arc<ForgeContext>,
    /// The logical and physical identity of the seeded table.
    pub binding: TenantTableBinding,
    /// The tenant that owns the table and file-list rows.
    pub tenant: DataTenantId,
}

/// Catalog wrapper used by uncertainty tests.
///
/// `update_table` delegates the real commit first, then returns one retryable
/// error. This models a lost response after the catalog durably accepted the
/// commit, so the next production tick must reconcile Iceberg state rather
/// than create a second snapshot.
#[derive(Debug)]
pub struct CommitUncertaintyCatalog {
    inner: Arc<dyn Catalog>,
    fail_after_next_commit: AtomicBool,
    uncertainty_active: AtomicBool,
    pause_after_commit: AtomicBool,
    commit_reached: AtomicBool,
    reject_after_commit: AtomicBool,
    commit_ready: tokio::sync::Notify,
    commit_release: tokio::sync::Notify,
    pause_before_commit: AtomicBool,
    before_commit_reached: AtomicBool,
    reject_before_commit: AtomicBool,
    before_commit_ready: tokio::sync::Notify,
    before_commit_release: tokio::sync::Notify,
}

impl CommitUncertaintyCatalog {
    /// Wrap a real catalog and arm one post-commit retryable response.
    #[must_use]
    pub fn new(inner: Arc<dyn Catalog>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            fail_after_next_commit: AtomicBool::new(false),
            uncertainty_active: AtomicBool::new(false),
            pause_after_commit: AtomicBool::new(false),
            commit_reached: AtomicBool::new(false),
            reject_after_commit: AtomicBool::new(false),
            commit_ready: tokio::sync::Notify::new(),
            commit_release: tokio::sync::Notify::new(),
            pause_before_commit: AtomicBool::new(false),
            before_commit_reached: AtomicBool::new(false),
            reject_before_commit: AtomicBool::new(false),
            before_commit_ready: tokio::sync::Notify::new(),
            before_commit_release: tokio::sync::Notify::new(),
        })
    }

    /// Make the next successful catalog update appear retryably uncertain.
    pub fn fail_after_next_commit(&self) {
        self.fail_after_next_commit.store(true, Ordering::Release);
    }

    /// Pause after the real catalog has accepted one commit.
    pub fn pause_after_commit(&self) {
        self.commit_reached.store(false, Ordering::Release);
        self.reject_after_commit.store(false, Ordering::Release);
        self.pause_after_commit.store(true, Ordering::Release);
    }

    /// Wait until the real catalog commit has completed and the wrapper is
    /// holding the response boundary open.
    pub async fn wait_for_commit(&self) {
        while !self.commit_reached.load(Ordering::Acquire) {
            self.commit_ready.notified().await;
        }
    }

    /// Mark the paused caller stale and let it observe the injected response.
    pub fn reject_paused_commit(&self) {
        self.reject_after_commit.store(true, Ordering::Release);
        self.commit_release.notify_waiters();
    }

    /// Pause immediately before delegating a catalog commit.
    pub fn pause_before_commit(&self) {
        self.before_commit_reached.store(false, Ordering::Release);
        self.reject_before_commit.store(false, Ordering::Release);
        self.pause_before_commit.store(true, Ordering::Release);
    }

    /// Wait until the production commit has reached the wrapped catalog seam.
    pub async fn wait_for_before_commit(&self) {
        while !self.before_commit_reached.load(Ordering::Acquire) {
            self.before_commit_ready.notified().await;
        }
    }

    /// Reject the paused commit as stale and resume the caller.
    pub fn reject_paused_before_commit(&self) {
        self.reject_before_commit.store(true, Ordering::Release);
        self.before_commit_release.notify_waiters();
    }
}

#[async_trait]
impl Catalog for CommitUncertaintyCatalog {
    async fn list_namespaces(
        &self,
        parent: Option<&NamespaceIdent>,
    ) -> iceberg::Result<Vec<NamespaceIdent>> {
        self.inner.list_namespaces(parent).await
    }

    async fn create_namespace(
        &self,
        namespace: &NamespaceIdent,
        properties: HashMap<String, String>,
    ) -> iceberg::Result<Namespace> {
        self.inner.create_namespace(namespace, properties).await
    }

    async fn get_namespace(&self, namespace: &NamespaceIdent) -> iceberg::Result<Namespace> {
        self.inner.get_namespace(namespace).await
    }

    async fn namespace_exists(&self, namespace: &NamespaceIdent) -> iceberg::Result<bool> {
        self.inner.namespace_exists(namespace).await
    }

    async fn update_namespace(
        &self,
        namespace: &NamespaceIdent,
        properties: HashMap<String, String>,
    ) -> iceberg::Result<()> {
        self.inner.update_namespace(namespace, properties).await
    }

    async fn drop_namespace(&self, namespace: &NamespaceIdent) -> iceberg::Result<()> {
        self.inner.drop_namespace(namespace).await
    }

    async fn list_tables(&self, namespace: &NamespaceIdent) -> iceberg::Result<Vec<TableIdent>> {
        self.inner.list_tables(namespace).await
    }

    async fn create_table(
        &self,
        namespace: &NamespaceIdent,
        creation: TableCreation,
    ) -> iceberg::Result<Table> {
        self.inner.create_table(namespace, creation).await
    }

    async fn load_table(&self, table: &TableIdent) -> iceberg::Result<Table> {
        self.inner.load_table(table).await
    }

    async fn drop_table(&self, table: &TableIdent) -> iceberg::Result<()> {
        self.inner.drop_table(table).await
    }

    async fn purge_table(&self, table: &TableIdent) -> iceberg::Result<()> {
        self.inner.purge_table(table).await
    }

    async fn table_exists(&self, table: &TableIdent) -> iceberg::Result<bool> {
        self.inner.table_exists(table).await
    }

    async fn rename_table(&self, src: &TableIdent, dest: &TableIdent) -> iceberg::Result<()> {
        self.inner.rename_table(src, dest).await
    }

    async fn register_table(
        &self,
        table: &TableIdent,
        metadata_location: String,
    ) -> iceberg::Result<Table> {
        self.inner.register_table(table, metadata_location).await
    }

    async fn update_table(&self, commit: TableCommit) -> iceberg::Result<Table> {
        if self.uncertainty_active.load(Ordering::Acquire) {
            return Err(IcebergError::new(
                IcebergErrorKind::Unexpected,
                "injected post-commit uncertainty remains unresolved",
            )
            .with_retryable(true));
        }
        if self.pause_before_commit.swap(false, Ordering::AcqRel) {
            self.before_commit_reached.store(true, Ordering::Release);
            self.before_commit_ready.notify_waiters();
            self.before_commit_release.notified().await;
            if self.reject_before_commit.load(Ordering::Acquire) {
                return Err(IcebergError::new(
                    IcebergErrorKind::Unexpected,
                    "injected stale Forge lease before catalog commit",
                )
                .with_retryable(false));
            }
        }
        let table = self.inner.update_table(commit).await?;
        if self.pause_after_commit.swap(false, Ordering::AcqRel) {
            self.commit_reached.store(true, Ordering::Release);
            self.commit_ready.notify_waiters();
            self.commit_release.notified().await;
            if self.reject_after_commit.load(Ordering::Acquire) {
                return Err(IcebergError::new(
                    IcebergErrorKind::Unexpected,
                    "injected stale Forge lease after catalog commit",
                )
                .with_retryable(true));
            }
        }
        if self.fail_after_next_commit.swap(false, Ordering::AcqRel) {
            self.uncertainty_active.store(true, Ordering::Release);
            return Err(IcebergError::new(
                IcebergErrorKind::Unexpected,
                "injected post-commit uncertainty",
            )
            .with_retryable(true));
        }
        Ok(table)
    }
}

/// Scoped OpenDAL controls for deterministic list/delete interleavings.
///
/// The wrapper delegates every operation to the real local filesystem. A test
/// may pause one list return to insert a durable file-list reference, or fail
/// one selected delete, without changing production Forge behavior.
#[derive(Debug)]
pub struct ForgeObjectStoreControl {
    inner: Arc<Operator>,
    pause_next_list: AtomicBool,
    list_returned: AtomicBool,
    list_ready: tokio::sync::Notify,
    list_release: tokio::sync::Notify,
    delete_count: AtomicUsize,
    fail_delete_at: AtomicUsize,
    pause_next_delete: AtomicBool,
    delete_reached: AtomicBool,
    reject_delete: AtomicBool,
    delete_ready: tokio::sync::Notify,
    delete_release: tokio::sync::Notify,
}

impl ForgeObjectStoreControl {
    /// Wrap the real staging operator used by a Forge context.
    #[must_use]
    pub fn new(inner: Arc<Operator>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            pause_next_list: AtomicBool::new(false),
            list_returned: AtomicBool::new(false),
            list_ready: tokio::sync::Notify::new(),
            list_release: tokio::sync::Notify::new(),
            delete_count: AtomicUsize::new(0),
            fail_delete_at: AtomicUsize::new(0),
            pause_next_delete: AtomicBool::new(false),
            delete_reached: AtomicBool::new(false),
            reject_delete: AtomicBool::new(false),
            delete_ready: tokio::sync::Notify::new(),
            delete_release: tokio::sync::Notify::new(),
        })
    }

    /// Pause one list after the real object-store response is available.
    pub fn pause_next_list(&self) {
        self.list_returned.store(false, Ordering::Release);
        self.pause_next_list.store(true, Ordering::Release);
    }

    /// Wait until the paused list has completed against the real store.
    pub async fn wait_for_list(&self) {
        while !self.list_returned.load(Ordering::Acquire) {
            self.list_ready.notified().await;
        }
    }

    /// Resume the paused list call.
    pub fn release_list(&self) {
        self.list_release.notify_waiters();
    }

    /// Fail the selected one-based delete call with a transient object error.
    pub fn fail_delete_at(&self, call: usize) {
        self.fail_delete_at.store(call, Ordering::Release);
    }

    /// Pause immediately before one real delete call.
    pub fn pause_next_delete(&self) {
        self.delete_reached.store(false, Ordering::Release);
        self.reject_delete.store(false, Ordering::Release);
        self.pause_next_delete.store(true, Ordering::Release);
    }

    /// Wait until the selected delete reached the wrapped OpenDAL boundary.
    pub async fn wait_for_delete(&self) {
        while !self.delete_reached.load(Ordering::Acquire) {
            self.delete_ready.notified().await;
        }
    }

    /// Reject the paused delete as a stale-worker effect and resume it.
    pub fn reject_paused_delete(&self) {
        self.reject_delete.store(true, Ordering::Release);
        self.delete_release.notify_waiters();
    }

    /// Return the number of delete calls that reached this scoped wrapper.
    #[must_use]
    pub fn delete_calls(&self) -> usize {
        self.delete_count.load(Ordering::Acquire)
    }
}

#[async_trait]
impl ForgeObjectStore for ForgeObjectStoreControl {
    async fn read(&self, path: &str) -> opendal::Result<Buffer> {
        self.inner.read(path).await
    }

    async fn list(&self, prefix: &str) -> opendal::Result<Vec<Entry>> {
        let entries = self.inner.list_with(prefix).recursive(true).await?;
        if self.pause_next_list.swap(false, Ordering::AcqRel) {
            self.list_returned.store(true, Ordering::Release);
            self.list_ready.notify_waiters();
            self.list_release.notified().await;
        }
        Ok(entries)
    }

    async fn stat(&self, path: &str) -> opendal::Result<Metadata> {
        self.inner.stat(path).await
    }

    async fn delete(&self, path: &str) -> opendal::Result<()> {
        let call = self.delete_count.fetch_add(1, Ordering::AcqRel) + 1;
        if call == self.fail_delete_at.load(Ordering::Acquire) {
            return Err(ObjectStoreError::new(
                ObjectStoreErrorKind::Unexpected,
                "injected Forge delete failure",
            ));
        }
        if self.pause_next_delete.swap(false, Ordering::AcqRel) {
            self.delete_reached.store(true, Ordering::Release);
            self.delete_ready.notify_waiters();
            self.delete_release.notified().await;
            if self.reject_delete.load(Ordering::Acquire) {
                return Err(ObjectStoreError::new(
                    ObjectStoreErrorKind::Unexpected,
                    "injected stale Forge lease before delete",
                ));
            }
        }
        self.inner.delete(path).await
    }
}

impl ForgeFixture {
    /// Clone the server-owned context with a test-specific validated config.
    #[must_use]
    pub fn context_with_config(&self, config: ForgeConfig) -> ForgeContext {
        ForgeContext::new(
            self.context.vala.clone(),
            self.context.operator_pool.clone(),
            Arc::clone(&self.context.catalog),
            Arc::clone(&self.context.staging),
            config,
        )
        .expect("validated Forge fixture config")
        .with_object_store(Arc::clone(&self.context.object_store))
    }

    /// Clone the context with a scoped catalog wrapper for one test journey.
    pub fn context_with_catalog(
        &self,
        config: ForgeConfig,
        catalog: Arc<dyn Catalog>,
    ) -> ForgeContext {
        ForgeContext::new(
            self.context.vala.clone(),
            self.context.operator_pool.clone(),
            catalog,
            Arc::clone(&self.context.staging),
            config,
        )
        .expect("validated Forge fixture config")
        .with_object_store(Arc::clone(&self.context.object_store))
    }

    /// Clone the context with a scoped OpenDAL wrapper for one test journey.
    pub fn context_with_object_store<S>(
        &self,
        config: ForgeConfig,
        object_store: Arc<S>,
    ) -> ForgeContext
    where
        S: ForgeObjectStore + 'static,
    {
        ForgeContext::new(
            self.context.vala.clone(),
            self.context.operator_pool.clone(),
            Arc::clone(&self.context.catalog),
            Arc::clone(&self.context.staging),
            config,
        )
        .expect("validated Forge fixture config")
        .with_object_store(object_store)
    }

    /// Append one aged Scribe-shaped Parquet file and its durable file-list row.
    pub async fn append_forge_file(&self, sequence: i64) {
        let schema = ArrowSchema::new(vec![
            Field::new("value", DataType::Int64, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ]);
        let base = chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
            .expect("Forge fixture timestamp")
            .timestamp_micros()
            + sequence * 1_000_000;
        let batch = RecordBatch::try_new(
            Arc::new(schema.clone()),
            vec![
                Arc::new(Int64Array::from(vec![sequence])),
                Arc::new(TimestampMicrosecondArray::from(vec![base]).with_timezone("UTC")),
                Arc::new(StringArray::from(vec![self.tenant.to_string()])),
            ],
        )
        .expect("Forge fixture append batch");
        let mut bytes = Vec::new();
        let mut writer =
            ArrowWriter::try_new(&mut bytes, batch.schema(), None).expect("Parquet writer");
        writer.write(&batch).expect("Parquet batch");
        writer.close().expect("Parquet close");
        let path = format!(
            "{}/sustained-{sequence}.parquet",
            self.binding.object_prefix
        );
        let file_size = i64::try_from(bytes.len()).expect("Forge fixture file size");
        self.context
            .staging
            .write(&path, Buffer::from(bytes))
            .await
            .expect("Forge fixture append object");
        let file_id = uuid::Uuid::now_v7();
        sqlx::query(
            "INSERT INTO vala.file_list (id, data_tenant_id, namespace, table_name, file_path, file_size, row_count, min_event_time, max_event_time, partition_day, node_id, writer_epoch, wal_lsn_min, wal_lsn_max) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(file_id)
        .bind(self.tenant.as_uuid())
        .bind(&self.binding.logical_namespace)
        .bind(&self.binding.table_name)
        .bind(&path)
        .bind(file_size)
        .bind(1_i64)
        .bind(chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z").expect("timestamp"))
        .bind(chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:01Z").expect("timestamp"))
        .bind(chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day"))
        .bind(uuid::Uuid::now_v7())
        .bind(1_i64)
        .bind(sequence * 2 + 1)
        .bind(sequence * 2 + 2)
        .execute(self.context.operator_pool.pool())
        .await
        .expect("Forge fixture file-list append");
        sqlx::query(
            "UPDATE vala.file_list SET created_at = now() - interval '3 minutes' WHERE id = $1",
        )
        .bind(file_id)
        .execute(self.context.operator_pool.pool())
        .await
        .expect("Forge fixture append aging");
    }

    /// Add a pending `file_list` reference for an existing object.
    ///
    /// GC race tests call this after the real object listing has paused. The
    /// next production live-set rebuild must therefore protect the path and
    /// skip deletion even though it was absent from the initial live set.
    pub async fn protect_path(&self, path: &str) {
        sqlx::query(
            "INSERT INTO vala.file_list (id, data_tenant_id, namespace, table_name, file_path, file_size, row_count, min_event_time, max_event_time, partition_day, node_id, writer_epoch, wal_lsn_min, wal_lsn_max) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(self.tenant.as_uuid())
        .bind(&self.binding.logical_namespace)
        .bind(&self.binding.table_name)
        .bind(path)
        .bind(1_i64)
        .bind(1_i64)
        .bind(chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z").expect("timestamp"))
        .bind(chrono::DateTime::parse_from_rfc3339("2026-07-14T12:00:01Z").expect("timestamp"))
        .bind(chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day"))
        .bind(uuid::Uuid::now_v7())
        .bind(1_i64)
        .bind(1_i64)
        .bind(1_i64)
        .execute(self.context.operator_pool.pool())
        .await
        .expect("Forge fixture live reference");
    }

    /// Count one tenant/table-scoped Forge audit operation.
    pub async fn operation_count(&self, operation: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND resource = $2 AND operation = $3",
        )
        .bind(self.tenant.as_uuid())
        .bind(format!(
            "bifrost://{}/{}/{}",
            self.tenant, self.binding.logical_namespace, self.binding.table_name
        ))
        .bind(operation)
        .fetch_one(self.context.operator_pool.pool())
        .await
        .expect("Forge fixture audit count")
    }
}

/// Create an Iceberg table, staging Parquet files, and aged `vala.file_list`
/// rows in the real Wyrd test server.
pub async fn seed_forge_group(server: &WyrdTestServer, table_name: &str) -> ForgeFixture {
    seed_forge_group_for_tenant(server, server.data_tenant_id(), table_name).await
}

/// Create the same durable Forge fixture for an explicitly selected tenant.
///
/// The server owns the shared catalog, object store, and SQL pools; selecting
/// the tenant here lets one production scheduler tick prove tenant-qualified
/// discovery and object prefixes without introducing a second fixture stack.
pub async fn seed_forge_group_for_tenant(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table_name: &str,
) -> ForgeFixture {
    seed_forge_group_for_tenant_with_schema(server, tenant, table_name, false).await
}

/// Create a durable Forge fixture with an optional table-specific schema
/// column. The variant is useful for proving that schema fingerprints do not
/// cross physical-table boundaries during one scheduler tick.
pub async fn seed_forge_group_for_tenant_with_schema(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table_name: &str,
    schema_variant: bool,
) -> ForgeFixture {
    seed_forge_group_for_tenant_with_schema_and_days(
        server,
        tenant,
        table_name,
        schema_variant,
        &[chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day")],
    )
    .await
}

/// Create a durable Forge fixture with explicit partition days and an optional
/// table-specific schema column.
pub async fn seed_forge_group_for_tenant_with_schema_and_days(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table_name: &str,
    schema_variant: bool,
    partition_days: &[chrono::NaiveDate],
) -> ForgeFixture {
    assert!(
        !partition_days.is_empty(),
        "Forge fixture needs one partition day"
    );
    let context = server
        .state()
        .forge_context
        .as_ref()
        .cloned()
        .expect("production server has Redux Forge context");
    let binding =
        TenantTableBinding::resolve((tenant, TableRef::new(BifrostNamespace::Bifrost, table_name)))
            .expect("Forge fixture table binding");
    let mut fields = vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("data_tenant_id", DataType::Utf8, false),
    ];
    if schema_variant {
        fields.push(Field::new("schema_variant", DataType::Int64, false));
    }
    let schema = ArrowSchema::new(fields);
    let iceberg_schema =
        iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&schema).expect("Iceberg schema");
    let spec = build_partition_spec(&iceberg_schema, &binding.partition_columns())
        .expect("Forge fixture partition spec");
    let warehouse =
        vala_bifrost::catalog::storage::warehouse_uri(server.state().storage.backend_config());
    if let Err(error) = context
        .catalog
        .create_namespace(binding.physical_namespace(), HashMap::new())
        .await
        && error.kind() != iceberg::ErrorKind::NamespaceAlreadyExists
    {
        panic!("Forge fixture namespace: {error}");
    }
    context
        .catalog
        .create_table(
            binding.physical_namespace(),
            TableCreation::builder()
                .name(binding.table_name.clone())
                .location(format!("{warehouse}/{}", binding.object_prefix))
                .schema(iceberg_schema)
                .partition_spec(spec)
                .build(),
        )
        .await
        .expect("Forge fixture table");

    let mut conn = context
        .vala
        .tenant_conn(tenant)
        .await
        .expect("Forge fixture tenant connection");
    for (day_index, partition_day) in partition_days.iter().enumerate() {
        let base = partition_day
            .and_hms_opt(12, 0, 0)
            .expect("Forge fixture timestamp")
            .and_utc()
            .timestamp_micros();
        for file_number in 0..2_i64 {
            let file_number =
                i64::try_from(day_index).expect("partition day index") * 2 + file_number;
            let mut columns = vec![
                Arc::new(Int64Array::from(vec![file_number, file_number + 10]))
                    as Arc<dyn arrow::array::Array>,
                Arc::new(
                    TimestampMicrosecondArray::from(vec![
                        base + file_number * 1_000_000,
                        base + file_number * 1_000_000 + 1_000,
                    ])
                    .with_timezone("UTC"),
                ) as Arc<dyn arrow::array::Array>,
                Arc::new(StringArray::from(vec![tenant.to_string(); 2]))
                    as Arc<dyn arrow::array::Array>,
            ];
            if schema_variant {
                columns
                    .push(Arc::new(Int64Array::from(vec![1_i64, 1_i64]))
                        as Arc<dyn arrow::array::Array>);
            }
            let batch = RecordBatch::try_new(Arc::new(schema.clone()), columns)
                .expect("Forge fixture batch");
            let mut bytes = Vec::new();
            let mut writer =
                ArrowWriter::try_new(&mut bytes, batch.schema(), None).expect("Parquet writer");
            writer.write(&batch).expect("Parquet batch");
            writer.close().expect("Parquet close");
            let path = format!("{}/journey-{file_number}.parquet", binding.object_prefix);
            let size = i64::try_from(bytes.len()).expect("Forge fixture file size");
            context
                .staging
                .write(&path, Buffer::from(bytes))
                .await
                .expect("Forge fixture object");
            let min_time = chrono::DateTime::from_timestamp_micros(base + file_number * 1_000_000)
                .expect("Forge fixture timestamp");
            sqlx::query(
            "INSERT INTO vala.file_list (id, data_tenant_id, namespace, table_name, file_path, file_size, row_count, min_event_time, max_event_time, partition_day, node_id, writer_epoch, wal_lsn_min, wal_lsn_max) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(tenant.as_uuid())
        .bind(&binding.logical_namespace)
        .bind(&binding.table_name)
        .bind(path)
        .bind(size)
        .bind(2_i64)
        .bind(min_time)
        .bind(min_time + chrono::Duration::milliseconds(1))
        .bind(*partition_day)
        .bind(uuid::Uuid::now_v7())
        .bind(1_i64)
        .bind(file_number * 2 + 1)
        .bind(file_number * 2 + 2)
        .execute(&mut **conn.transaction())
        .await
        .expect("Forge fixture file-list row");
        }
    }
    conn.commit().await.expect("Forge fixture commit");
    sqlx::query(
        "UPDATE vala.file_list SET created_at = now() - interval '3 minutes' WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3",
    )
    .bind(tenant.as_uuid())
    .bind(&binding.logical_namespace)
    .bind(&binding.table_name)
    .execute(context.operator_pool.pool())
    .await
    .expect("Forge fixture aging");

    ForgeFixture {
        context,
        binding,
        tenant,
    }
}
