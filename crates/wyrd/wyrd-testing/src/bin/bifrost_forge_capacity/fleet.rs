//! The independent tables one fleet size compacts, the writers that fill
//! them with a backlog, and the check that the backlog is in place.
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
//! promotion commits has accumulated since its last compaction. Each
//! promotion appends one Iceberg snapshot, so a table whose snapshot count
//! reaches the trigger holds at least that many staged data files and is due.
//! Every fleet size gets its own set of tables, so each drains an equal fresh
//! backlog.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

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
use wyrd_testing::capacity::{Percentiles, merged};
use wyrd_testing::release_server::LocalServer;

use crate::Result;
use crate::report::BacklogFill;

/// Iceberg property that opts one table into compaction.
const ENABLE_COMPACTION: &str = "wyrd.forge.enable-compaction";

/// Iceberg property naming the promotion commits that make a table due.
const TRIGGER_SNAPSHOT_COUNT: &str = "wyrd.forge.compaction.trigger-snapshot-count";

/// Public table name prefix; the set and the index follow.
const TABLE_PREFIX: &str = "forge_capacity_";

/// Leader series label of a Scribe promotion.
const PROMOTION: &str = "task_type=\"scribe_promotion\"";

/// Series label of a succeeded attempt.
const SUCCEEDED: &str = "result=\"succeeded\"";

/// How often the backlog check reloads every table.
const BACKLOG_POLL: Duration = Duration::from_secs(5);

/// One set of registered tables, the client that writes them, and the
/// catalog that reads their snapshots.
pub struct Fleet {
    /// One public client bound to the benchmark tenant.
    bifrost: Arc<Bifrost>,
    /// The operator catalog over the same Postgres and store.
    catalog: BifrostCatalog,
    /// Every table's namespace-qualified public name.
    tables: Vec<String>,
    /// Every table's catalog binding, in the order of `tables`.
    bindings: Vec<TenantTableBinding>,
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
    /// Registers table set `set` — `count` tables for the leader's first
    /// tenant — through the public client and commits each one's Forge
    /// properties: compaction on, due after `trigger` promotion commits.
    ///
    /// # Errors
    ///
    /// Returns a client, registration, catalog, or property-commit failure.
    pub async fn provision(
        leader: &LocalServer,
        set: usize,
        count: usize,
        trigger: usize,
    ) -> Result<Self> {
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
        let mut bindings = Vec::with_capacity(count);
        for index in 0..count {
            let name = format!("{TABLE_PREFIX}s{set}_{index:04}");
            let qualified = format!("{}.{name}", BifrostNamespace::Datasets.as_str());
            bifrost.use_table(TableConfig::from_arrow(&qualified, schema())?);
            bifrost.register().await?;
            let binding = TenantTableBinding::resolve((
                tenant,
                TableRef::new(BifrostNamespace::Datasets, &name),
            ))?;
            enable_compaction(&catalog, &binding, trigger).await?;
            tables.push(qualified);
            bindings.push(binding);
        }
        Ok(Self {
            bifrost: Arc::new(bifrost),
            catalog,
            tables,
            bindings,
        })
    }

    /// Waits until every table holds at least `snapshots` snapshots, each a
    /// promotion that appended staged data files, so the whole set is due.
    ///
    /// # Errors
    ///
    /// Returns a catalog load failure, or `timeout` passing first, naming how
    /// many tables were still short.
    pub async fn await_backlog(&self, snapshots: usize, timeout: Duration) -> Result<()> {
        let deadline = tokio::time::Instant::now() + timeout;
        let iceberg = self.catalog.iceberg_catalog();
        loop {
            let mut short = 0_usize;
            for binding in &self.bindings {
                let table = iceberg.load_table(&binding.table_ident()).await?;
                short += usize::from(table.metadata().snapshots().count() < snapshots);
            }
            if short == 0 {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "{short} of {} tables held fewer than {snapshots} snapshots after {} s",
                    self.bindings.len(),
                    timeout.as_secs()
                )
                .into());
            }
            tokio::time::sleep(BACKLOG_POLL).await;
        }
    }

    /// Writes the backlog for a `workers`-worker drain: starts the writers
    /// (see [`Self::write`]) with no worker running and waits until every
    /// table holds `snapshots` snapshots (see [`Self::await_backlog`]),
    /// recording the writes and the leader's promotions meanwhile. The
    /// writers keep running so the caller can measure under them; stop them
    /// before draining.
    ///
    /// # Errors
    ///
    /// Returns a leader scrape failure, or the backlog not forming within
    /// `timeout`; the writers are stopped first.
    pub async fn fill(
        &self,
        leader: &LocalServer,
        workers: usize,
        (interval, rows): (Duration, usize),
        snapshots: usize,
        timeout: Duration,
    ) -> Result<(BacklogFill, Writers)> {
        let before = leader.metrics().await?;
        let started = Instant::now();
        let writers = self.write(interval, rows);
        if let Err(error) = self.await_backlog(snapshots, timeout).await {
            writers.stop().await;
            return Err(error);
        }
        let seconds = started.elapsed().as_secs_f64();
        let after = leader.metrics().await?;
        let (acked, refused) = writers.counts();
        let promotions = |metrics: &wyrd_testing::release_server::Metrics| {
            metrics.sum("bifrost_forge_task_attempts_total", &[PROMOTION, SUCCEEDED])
        };
        let durations = |metrics| {
            merged(
                std::slice::from_ref(metrics),
                "bifrost_forge_task_duration_seconds",
                &[PROMOTION, SUCCEEDED],
            )
        };
        let fill = BacklogFill {
            workers,
            seconds,
            writes_per_second: acked as f64 / seconds,
            refused_writes: refused,
            promotion_commits_per_second: (promotions(&after) - promotions(&before)) / seconds,
            promotion_seconds: Percentiles::buckets(&durations(&before), &durations(&after)),
        };
        Ok((fill, writers))
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
