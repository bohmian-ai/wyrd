//! The independent tables the workload compacts and the writers that keep
//! them committing.
//!
//! Every table is registered through the public Bifrost client, as a user
//! would. Compaction is off by default and no public route sets Iceberg table
//! properties, so the benchmark commits each table's Forge properties as an
//! operator would: through the Iceberg catalog over the serving Postgres
//! login and object store the Postgres wrapper and mise task export. Only
//! the opt-in and the snapshot-count trigger are set: the compaction type
//! stays the default `full` and the file target the default 1 GiB. The
//! writers then send one small batch to every table each write interval, so
//! Scribe seals each table at the configured age, the leader promotes and
//! notifies every seal, and a table becomes due once the trigger's count of
//! promotion commits has accumulated since its last compaction.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use arrow::array::{Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use secrecy::ExposeSecret as _;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{BifrostCatalog, TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_client::bifrost::TableConfig;
use wyrd_client::config::ClientConfig;
use wyrd_client::{Bifrost, GlobalConfig, WyrdClient};
use wyrd_spec::DataTenantId;
use wyrd_testing::release_server::LocalServer;

use crate::Result;

/// Iceberg property that opts one table into compaction.
const ENABLE_COMPACTION: &str = "wyrd.forge.enable-compaction";

/// Iceberg property naming the promotion commits that make a table due.
const TRIGGER_SNAPSHOT_COUNT: &str = "wyrd.forge.compaction.trigger-snapshot-count";

/// Public table name prefix; the index follows, zero-padded.
const TABLE_PREFIX: &str = "forge_capacity_";

/// The registered tables and the client that writes them.
pub struct Fleet {
    /// One public client bound to the benchmark tenant.
    bifrost: Arc<Bifrost>,
    /// Every table's namespace-qualified public name.
    tables: Vec<String>,
}

/// Running writers and their acknowledgement counters.
pub struct Writers {
    /// Stops every writer.
    stop: CancellationToken,
    /// One task per table.
    tasks: JoinSet<()>,
    /// Batches durably acknowledged.
    acked: Arc<AtomicU64>,
    /// Batches refused.
    refused: Arc<AtomicU64>,
}

impl Fleet {
    /// Registers `count` tables for the leader's first tenant through the
    /// public client and commits each one's Forge properties: compaction on,
    /// due after `trigger` promotion commits.
    ///
    /// # Errors
    ///
    /// Returns a client, registration, catalog, or property-commit failure.
    pub async fn provision(leader: &LocalServer, count: usize, trigger: usize) -> Result<Self> {
        let client = WyrdClient::with_config(ClientConfig {
            credential: Some(leader.api_key().clone()),
            ..ClientConfig::from_global_with_overrides(
                &GlobalConfig::default(),
                Some(&leader.url()),
                None,
            )
        })?;
        let bifrost = Bifrost::connect(&client).await?;
        let setup = leader
            .tenants()
            .first()
            .ok_or("the leader was started without a tenant")?;
        let tenant: DataTenantId = setup.tenant_id.parse()?;
        let catalog = operator_catalog().await?;
        let mut tables = Vec::with_capacity(count);
        for index in 0..count {
            let name = format!("{TABLE_PREFIX}{index:04}");
            let qualified = format!("{}.{name}", BifrostNamespace::Datasets.as_str());
            bifrost.use_table(TableConfig::from_arrow(&qualified, schema())?);
            bifrost.register().await?;
            let binding = TenantTableBinding::resolve((
                tenant,
                TableRef::new(BifrostNamespace::Datasets, &name),
            ))?;
            enable_compaction(&catalog, &binding, trigger).await?;
            tables.push(qualified);
        }
        Ok(Self {
            bifrost: Arc::new(bifrost),
            tables,
        })
    }

    /// Tables in the fleet.
    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    /// Starts one writer per table, each sending a `rows`-row batch every
    /// `interval`, staggered across the interval so sends spread evenly.
    pub fn write(&self, interval: Duration, rows: usize) -> Writers {
        let stop = CancellationToken::new();
        let (acked, refused) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
        let mut tasks = JoinSet::new();
        let count = u32::try_from(self.tables.len()).unwrap_or(u32::MAX).max(1);
        for (index, table) in self.tables.iter().cloned().enumerate() {
            let (bifrost, stop, acked, refused) = (
                Arc::clone(&self.bifrost),
                stop.clone(),
                Arc::clone(&acked),
                Arc::clone(&refused),
            );
            let offset = interval * u32::try_from(index).unwrap_or(0) / count;
            tasks.spawn(async move {
                let mut ticker =
                    tokio::time::interval_at(tokio::time::Instant::now() + offset, interval);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                let mut next = 0_i64;
                loop {
                    tokio::select! {
                        () = stop.cancelled() => return,
                        _ = ticker.tick() => {}
                    }
                    let Ok(batch) = batch(next, rows) else {
                        refused.fetch_add(1, Ordering::Relaxed);
                        continue;
                    };
                    next += i64::try_from(rows).unwrap_or(i64::MAX);
                    match bifrost.write_batch(&table, &batch).await {
                        Ok(()) => acked.fetch_add(1, Ordering::Relaxed),
                        Err(error) => {
                            tracing::warn!(%table, %error, "benchmark write refused");
                            refused.fetch_add(1, Ordering::Relaxed)
                        }
                    };
                }
            });
        }
        Writers {
            stop,
            tasks,
            acked,
            refused,
        }
    }
}

impl Writers {
    /// Batches acknowledged and refused so far.
    pub fn counts(&self) -> (u64, u64) {
        (
            self.acked.load(Ordering::Relaxed),
            self.refused.load(Ordering::Relaxed),
        )
    }

    /// Stops every writer and waits for in-flight sends to finish.
    pub async fn stop(mut self) {
        self.stop.cancel();
        while self.tasks.join_next().await.is_some() {}
    }
}

/// The Iceberg catalog over the serving platform login and the shared store,
/// as an operator reaches it.
///
/// # Errors
///
/// Returns missing DSN or storage settings, or a connection failure.
async fn operator_catalog() -> Result<BifrostCatalog> {
    let dsns = wyrd_sql::dsn::ResolvedDsns::from_env()?;
    let storage =
        wyrd_storage::handle::StorageHandle::from_settings(wyrd_storage::settings::from_env()?)
            .await?;
    Ok(BifrostCatalog::new(
        dsns.catalog().expose_secret(),
        wyrd_testing::server::test_storage_owner(&storage),
        vala_sql::ValaPostgres::connect_from_dsns(&dsns).await?,
    )
    .await?)
}

/// Commits [`ENABLE_COMPACTION`] and a [`TRIGGER_SNAPSHOT_COUNT`] of
/// `trigger` onto one registered table.
///
/// # Errors
///
/// Returns a load, transaction, or commit failure.
async fn enable_compaction(
    catalog: &BifrostCatalog,
    binding: &TenantTableBinding,
    trigger: usize,
) -> Result<()> {
    let iceberg = catalog.iceberg_catalog();
    let table = iceberg.load_table(&binding.table_ident()).await?;
    let tx = Transaction::new(&table);
    tx.update_table_properties()
        .set(ENABLE_COMPACTION.to_owned(), "true".to_owned())
        .set(TRIGGER_SNAPSHOT_COUNT.to_owned(), trigger.to_string())
        .apply(tx)?
        .commit_once(iceberg.as_ref())
        .await?;
    Ok(())
}

/// The fleet's table schema: an id, a value, and a short payload.
fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Int64, false),
        Field::new("payload", DataType::Utf8, false),
    ]))
}

/// `rows` rows whose ids start at `first`.
///
/// # Errors
///
/// Returns the Arrow construction failure.
fn batch(first: i64, rows: usize) -> Result<RecordBatch> {
    let ids: Vec<i64> = (first..).take(rows).collect();
    let payload: Vec<String> = ids
        .iter()
        .map(|id| format!("forge-capacity-row-{id:016}"))
        .collect();
    Ok(RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(Int64Array::from(ids.clone())),
            Arc::new(Int64Array::from_iter_values(ids.iter().map(|id| id * 7))),
            Arc::new(StringArray::from(payload)),
        ],
    )?)
}
