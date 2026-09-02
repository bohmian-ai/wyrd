//! Snapshots this Oracle node's attempted SQL cuts still depend on.
//!
//! A pinned cut is immutable for the life of its query, which means the
//! Iceberg snapshot it names must survive that long. Forge maintenance runs in
//! another process and cannot see an in-flight query, so this node keeps a
//! ledger of what its cuts need and republishes it durably on its ordinary
//! heartbeat. The ledger is the in-process half of that: pins are held by RAII
//! for exactly the pinned attempt's lifetime, so an attempt that fails, is
//! cancelled, or panics stops protecting its snapshot without any cleanup path
//! of its own.
//!
//! Nothing here decides whether a snapshot may be expired. It reports what this
//! node needs; the maintenance owner corroborates that against Iceberg.
//!
//! This ledger covers pinned Oracle snapshot cuts only. It is not a live-tail
//! lease registry and must not be read as one: a Scribe live tail retains
//! Arrow batches and locally staged resources, names no Forge-collectable
//! object-store key, and is not represented here.
//!
//! # Status
//!
//! Work in progress, not yet admissible as Forge protection evidence. The
//! known gaps are recorded in
//! `changes/active/forge-live-tail-authority-conflict.md`: only the attempt
//! path pins, the pin is released at plan scope rather than at stream
//! completion, there is an unprotected window between admission and this
//! node's first durable publication, publication loss and ledger poisoning do
//! not fail closed, admitted queries are not drained before shutdown, and the
//! destructive commit boundary does not revalidate reader protection.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use vala_sql::row_types::forge_tasks::SnapshotWatermark;
use vala_sql::row_types::reader_watermarks::{ReaderWatermarkEntry, ReaderWatermarkPublication};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::BifrostError;

use crate::catalog::PinnedSealedTable;

/// Delay between one Oracle node's durable reader-watermark publications.
///
/// The value is a lifecycle constant rather than configuration because it is
/// not a tuning knob a deployment benefits from turning: it only trades
/// Postgres write volume against how long a dead node keeps protecting
/// snapshots, and the lease derived from it already absorbs a missed round.
pub(crate) const READER_WATERMARK_PUBLISH_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(15);

/// Bounded wait for admitted queries to finish before this node stops
/// protecting their snapshots.
///
/// Shutdown is not a reason to drop protection: a query admitted a moment
/// before it still reads the snapshot it pinned. The bound exists so a wedged
/// query cannot hold a draining node open forever; past it the node retires and
/// falls back on its lease, which is the same guarantee a crash provides.
const READER_WATERMARK_DRAIN_BOUND: std::time::Duration = std::time::Duration::from_mins(1);

/// One table a live cut depends on, identified as maintenance sees it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PinnedTable {
    /// Tenant owning the pinned table.
    tenant: DataTenantId,
    /// Bifrost namespace of the pinned table.
    namespace: String,
    /// Physical table name of the pinned table.
    table_name: String,
}

/// Snapshots this node's live cuts still need, keyed by pin identity.
///
/// Held behind one mutex rather than a per-table counter because publication
/// wants the minimum over live pins, and reconstructing that from counters
/// after an arbitrary release order is exactly the bookkeeping this avoids.
#[derive(Debug, Default)]
pub struct ReaderPinLedger {
    /// Every unreleased pin, keyed by the identity issued when it was taken.
    pins: Mutex<ReaderPinState>,
}

/// Mutable ledger interior.
#[derive(Debug, Default)]
struct ReaderPinState {
    /// Monotonic identity issued to each pin, unique for this process.
    next_pin_id: u64,
    /// Unreleased pins and the snapshot each one still needs.
    entries: BTreeMap<u64, (PinnedTable, SnapshotWatermark)>,
}

impl ReaderPinLedger {
    /// Creates an empty ledger for one Oracle node.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrows the ledger interior, refusing a poisoned lock.
    ///
    /// A poisoned ledger has an unknown pin set, and an unknown pin set cannot
    /// be published safely: a lost entry silently stops protecting a snapshot a
    /// live query is reading. Every reader and writer therefore fails closed
    /// here rather than substituting an empty or partial ledger.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the ledger mutex is poisoned.
    fn state(&self) -> Result<std::sync::MutexGuard<'_, ReaderPinState>, BifrostError> {
        self.pins.lock().map_err(|_| BifrostError::Internal {
            detail: "Oracle reader-pin ledger is poisoned and can no longer protect snapshots"
                .to_owned(),
        })
    }

    /// Records the snapshots one query's pinned cuts depend on.
    ///
    /// A cut with no Iceberg snapshot contributes nothing, because there is no
    /// snapshot for maintenance to protect. A cut that names a snapshot the
    /// loaded metadata cannot date is a different case and fails closed: the
    /// query is about to read that snapshot, and a watermark without a
    /// corroborating timestamp is one the maintenance owner rejects, so
    /// admitting the query would leave it reading unprotected files.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the ledger is poisoned or when a
    /// pinned cut names a snapshot absent from its own table metadata.
    pub fn pin(
        self: &Arc<Self>,
        cuts: &[PinnedSealedTable],
    ) -> Result<ReaderPinGuard, BifrostError> {
        let mut held = Vec::new();
        {
            let mut state = self.state()?;
            for cut in cuts {
                let Some(snapshot_id) = cut.snapshot_id else {
                    continue;
                };
                let Some(snapshot) = cut.iceberg_table.metadata().snapshot_by_id(snapshot_id)
                else {
                    drop(state);
                    drop(ReaderPinGuard {
                        ledger: Arc::clone(self),
                        held,
                    });
                    return Err(BifrostError::Internal {
                        detail: format!(
                            "Oracle pinned snapshot {snapshot_id} that its own table metadata \
                             cannot date, so it cannot be protected from maintenance"
                        ),
                    });
                };
                let pin_id = state.next_pin_id;
                state.next_pin_id = state.next_pin_id.wrapping_add(1);
                state.entries.insert(
                    pin_id,
                    (
                        PinnedTable {
                            tenant: cut.binding.tenant,
                            namespace: cut.binding.table_ref.namespace.as_str().to_owned(),
                            table_name: cut.binding.table_ref.name.as_str().to_owned(),
                        },
                        SnapshotWatermark {
                            snapshot_id,
                            timestamp_ms: snapshot.timestamp_ms(),
                        },
                    ),
                );
                held.push(pin_id);
            }
        }
        Ok(ReaderPinGuard {
            ledger: Arc::clone(self),
            held,
        })
    }

    /// Reports whether any pin is still unreleased.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the ledger is poisoned.
    fn is_drained(&self) -> Result<bool, BifrostError> {
        Ok(self.state()?.entries.is_empty())
    }

    /// Projects the current dependency set into one publication per tenant.
    ///
    /// Each table reports the oldest snapshot any live cut still needs, because
    /// protecting the newest would leave an older in-flight cut unprotected.
    /// Tenants with no live pin are absent, and the caller must still clear
    /// that tenant's previous publication for this node.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the ledger is poisoned.
    pub fn publications(
        &self,
        published_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<BTreeMap<DataTenantId, ReaderWatermarkPublication>, BifrostError> {
        let mut oldest = BTreeMap::<PinnedTable, SnapshotWatermark>::new();
        for (table, watermark) in self.state()?.entries.values() {
            oldest
                .entry(table.clone())
                .and_modify(|held| {
                    if watermark.timestamp_ms < held.timestamp_ms {
                        *held = *watermark;
                    }
                })
                .or_insert(*watermark);
        }
        let mut publications = BTreeMap::<DataTenantId, ReaderWatermarkPublication>::new();
        for (table, watermark) in oldest {
            publications
                .entry(table.tenant)
                .or_insert_with(|| ReaderWatermarkPublication {
                    entries: Vec::new(),
                    published_at,
                    expires_at,
                })
                .entries
                .push(ReaderWatermarkEntry {
                    namespace: table.namespace,
                    table_name: table.table_name,
                    watermark,
                });
        }
        Ok(publications)
    }
}

/// A live query's claim on the snapshots its cuts pinned.
///
/// Releasing is a drop rather than a call so that every way a query can end —
/// success, error, cancellation, unwind — releases the claim identically. A
/// guard that outlived its query would keep a snapshot unexpirable forever.
#[derive(Debug)]
pub struct ReaderPinGuard {
    /// Ledger this guard returns its pins to.
    ledger: Arc<ReaderPinLedger>,
    /// Pin identities this guard holds.
    held: Vec<u64>,
}

impl Drop for ReaderPinGuard {
    /// Returns every pin this guard holds to its ledger.
    ///
    /// A poisoned ledger lock is ignored rather than panicking in a drop: the
    /// process is already failing, and every path that would read this state
    /// fails closed on the same lock.
    fn drop(&mut self) {
        if let Ok(mut state) = self.ledger.pins.lock() {
            for pin_id in self.held.drain(..) {
                state.entries.remove(&pin_id);
            }
        }
    }
}

/// Publishes one Oracle node's live snapshot dependencies durably.
///
/// The publisher exists because the ledger is process-local and Forge is not.
/// The periodic round projects process-local pins for Forge to consume. The
/// ordering protocol that closes the gap between acquiring a new pin and its
/// first background publication is intentionally unresolved WIP; query
/// execution must not turn that gap into an unapproved synchronous Postgres
/// write on every read.
pub struct ReaderWatermarks {
    /// Ledger whose current state every round publishes.
    ledger: Arc<ReaderPinLedger>,
    /// Tenant-scoped SQL handle used for every publication.
    vala: vala_sql::ValaPostgres,
    /// Physical node identity every published row is attributed to.
    node_id: uuid::Uuid,
    /// Delay between periodic refreshes.
    interval: std::time::Duration,
    /// How far past a publication its rows keep protecting snapshots.
    lease: chrono::Duration,
    /// Tenants this node published for, so a released tenant clears.
    ///
    /// Async because a query-path publication and the refresh round contend for
    /// it across await points; the set is only ever held across the SQL work of
    /// one round.
    published_tenants: tokio::sync::Mutex<std::collections::BTreeSet<DataTenantId>>,
}

impl ReaderWatermarks {
    /// Builds one publisher for this node's ledger.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the interval is zero, which
    /// would spin, or when the lease is not representable.
    fn new(
        ledger: Arc<ReaderPinLedger>,
        vala: vala_sql::ValaPostgres,
        node_id: uuid::Uuid,
        interval: std::time::Duration,
    ) -> Result<Self, BifrostError> {
        if interval.is_zero() {
            return Err(BifrostError::Internal {
                detail: "Oracle reader-watermark interval must be positive".to_owned(),
            });
        }
        let lease = chrono::Duration::from_std(interval.saturating_mul(3)).map_err(|_| {
            BifrostError::Internal {
                detail: "Oracle reader-watermark lease is not representable".to_owned(),
            }
        })?;
        Ok(Self {
            ledger,
            vala,
            node_id,
            interval,
            lease,
            published_tenants: tokio::sync::Mutex::new(std::collections::BTreeSet::new()),
        })
    }

    /// Builds this node's watermark owner and spawns its refresh loop.
    ///
    /// Construction and spawning live together because a publisher that is
    /// built but never polled is indistinguishable from one that is running,
    /// yet protects nothing.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when no Tokio runtime is entered, or
    /// the construction errors described by [`Self::new`].
    pub(crate) fn start(
        vala: vala_sql::ValaPostgres,
        node_id: uuid::Uuid,
        shutdown: tokio_util::sync::CancellationToken,
    ) -> Result<(Arc<Self>, tokio::task::JoinHandle<()>), BifrostError> {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| BifrostError::Internal {
            detail: "Oracle reader-watermark publication requires an active Tokio runtime"
                .to_owned(),
        })?;
        let watermarks = Arc::new(Self::new(
            Arc::new(ReaderPinLedger::new()),
            vala,
            node_id,
            READER_WATERMARK_PUBLISH_INTERVAL,
        )?);
        let refresh = handle.spawn(Arc::clone(&watermarks).run(shutdown));
        Ok((watermarks, refresh))
    }

    /// Borrows this node's live reader-pin ledger.
    #[must_use]
    pub fn ledger(&self) -> &Arc<ReaderPinLedger> {
        &self.ledger
    }

    /// Refreshes publications until shutdown, drains, then retires this node.
    ///
    /// A failed refresh is logged and retried on the next tick rather than
    /// ending the loop: the previous publication is still valid for the rest of
    /// its lease, so a transient Postgres failure costs freshness, not safety.
    /// After cancellation the loop keeps refreshing while admitted queries are
    /// still holding pins, because a draining node's queries are still reading;
    /// it stops at [`READER_WATERMARK_DRAIN_BOUND`] so a wedged query cannot
    /// hold shutdown open, and falls back on the lease exactly as a crash does.
    async fn run(self: Arc<Self>, shutdown: tokio_util::sync::CancellationToken) {
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break,
                _ = ticker.tick() => self.refresh_once().await,
            }
        }
        let deadline = tokio::time::Instant::now() + READER_WATERMARK_DRAIN_BOUND;
        while !self.ledger.is_drained().unwrap_or(true) {
            if tokio::time::Instant::now() >= deadline {
                tracing::warn!(
                    "Oracle stopped refreshing reader watermarks with queries still admitted; \
                     their protection now rests on the published lease"
                );
                break;
            }
            tokio::time::sleep(self.interval.min(READER_WATERMARK_DRAIN_BOUND)).await;
            self.refresh_once().await;
        }
        if let Err(error) = self.retire_all().await {
            tracing::warn!(
                error = %error,
                "Oracle could not retire its reader watermarks on shutdown"
            );
        }
    }

    /// Publishes one round, logging rather than propagating its failure.
    async fn refresh_once(&self) {
        if let Err(error) = self.publish_round().await {
            tracing::warn!(
                error = %error,
                "Oracle could not publish its reader watermarks"
            );
        }
    }

    /// Writes one complete publication round.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the ledger is poisoned, and the
    /// SQL failure from the first tenant whose publication could not commit.
    /// Earlier tenants in the round are already committed and stay published;
    /// each tenant is its own transaction because they are separate isolation
    /// scopes, not one atomic fact.
    async fn publish_round(&self) -> Result<(), BifrostError> {
        let published_at = chrono::Utc::now();
        let publications = self
            .ledger
            .publications(published_at, published_at + self.lease)?;
        let mut published_tenants = self.published_tenants.lock().await;
        for tenant in published_tenants
            .iter()
            .copied()
            .filter(|tenant| !publications.contains_key(tenant))
            .collect::<Vec<_>>()
        {
            self.retire_tenant(tenant)
                .await
                .map_err(|error| sql_failure(&error))?;
            published_tenants.remove(&tenant);
        }
        for (tenant, publication) in &publications {
            let mut conn = self
                .vala
                .tenant_conn(*tenant)
                .await
                .map_err(|error| sql_failure(&error))?;
            vala_sql::queries::reader_watermarks::BifrostReaderWatermarks::new(&mut conn)
                .publish(self.node_id, publication)
                .await
                .map_err(|error| sql_failure(&error))?;
            conn.commit().await.map_err(|error| sql_failure(&error))?;
            published_tenants.insert(*tenant);
        }
        Ok(())
    }

    /// Drops this node's rows for one tenant that no longer has a live cut.
    ///
    /// # Errors
    ///
    /// Returns the SQL failure; the caller leaves the tenant in the published
    /// set so the next round retries the retirement.
    async fn retire_tenant(&self, tenant: DataTenantId) -> Result<(), vala_sql::SqlError> {
        let mut conn = self.vala.tenant_conn(tenant).await?;
        vala_sql::queries::reader_watermarks::BifrostReaderWatermarks::new(&mut conn)
            .retire_node(self.node_id)
            .await?;
        conn.commit().await
    }

    /// Drops every row this node still has published.
    ///
    /// # Errors
    ///
    /// Returns the SQL failure from the first tenant that could not be retired.
    async fn retire_all(&self) -> Result<(), vala_sql::SqlError> {
        let mut published_tenants = self.published_tenants.lock().await;
        for tenant in std::mem::take(&mut *published_tenants) {
            self.retire_tenant(tenant).await?;
        }
        Ok(())
    }
}

/// Reports one durable publication failure as a reader-protection failure.
///
/// The caller cannot distinguish a connection failure from a rejected write,
/// and neither is safe to read through, so both collapse to the same refusal.
fn sql_failure(error: &vala_sql::SqlError) -> BifrostError {
    BifrostError::Internal {
        detail: format!("Oracle reader-watermark publication failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two live cuts on one table publish the older snapshot, and only it.
    ///
    /// The younger cut finishing must not raise the published watermark while
    /// the older one is still running, and the older one finishing must let the
    /// watermark rise. Publishing a per-table maximum, or forgetting to release
    /// on drop, each break exactly one half of that.
    #[test]
    fn reader_pins_publish_the_oldest_live_snapshot_per_table() {
        let ledger = Arc::new(ReaderPinLedger::new());
        let tenant = DataTenantId::new_v7();
        let table = PinnedTable {
            tenant,
            namespace: "vala.traces".to_owned(),
            table_name: "spans".to_owned(),
        };
        let insert = |id: u64, snapshot_id: i64, timestamp_ms: i64| {
            let mut state = ledger.pins.lock().expect("ledger lock");
            state.entries.insert(
                id,
                (
                    table.clone(),
                    SnapshotWatermark {
                        snapshot_id,
                        timestamp_ms,
                    },
                ),
            );
        };
        let published_at = Utc::now();
        let expires_at = published_at + chrono::Duration::seconds(15);
        let published = |ledger: &ReaderPinLedger| {
            ledger
                .publications(published_at, expires_at)
                .expect("a healthy ledger publishes")
                .get(&tenant)
                .map(|publication| {
                    publication
                        .entries
                        .iter()
                        .map(|entry| entry.watermark.snapshot_id)
                        .collect::<Vec<_>>()
                })
        };

        insert(1, 700, 7_000);
        insert(2, 900, 9_000);
        assert_eq!(
            published(&ledger),
            Some(vec![700]),
            "a live older cut is what the table still needs"
        );

        ledger.pins.lock().expect("ledger lock").entries.remove(&2);
        assert_eq!(
            published(&ledger),
            Some(vec![700]),
            "the younger cut finishing does not release the older one's snapshot"
        );

        ledger.pins.lock().expect("ledger lock").entries.remove(&1);
        assert_eq!(
            published(&ledger),
            None,
            "with no live cut the table is unprotected by this node"
        );
    }

    /// A dropped guard stops protecting everything it pinned.
    #[test]
    fn reader_pin_guard_releases_every_pin_on_drop() {
        let ledger = Arc::new(ReaderPinLedger::new());
        let guard = ReaderPinGuard {
            ledger: Arc::clone(&ledger),
            held: vec![10, 11],
        };
        {
            let mut state = ledger.pins.lock().expect("ledger lock");
            for (index, id) in [10_u64, 11].into_iter().enumerate() {
                state.entries.insert(
                    id,
                    (
                        PinnedTable {
                            tenant: DataTenantId::new_v7(),
                            namespace: "vala.traces".to_owned(),
                            table_name: format!("spans_{index}"),
                        },
                        SnapshotWatermark {
                            snapshot_id: 1,
                            timestamp_ms: 1,
                        },
                    ),
                );
            }
        }
        drop(guard);
        assert!(
            ledger.pins.lock().expect("ledger lock").entries.is_empty(),
            "a query that ended for any reason protects nothing"
        );
    }

    /// A poisoned ledger refuses to pin and refuses to publish.
    ///
    /// Poisoning means the live pin set is unknown. Reading through it would
    /// publish a set that silently omits a snapshot a running query needs, so
    /// both directions fail closed instead of substituting what is left.
    #[test]
    fn poisoned_reader_pin_ledger_fails_closed_in_both_directions() {
        let ledger = Arc::new(ReaderPinLedger::new());
        let poisoner = Arc::clone(&ledger);
        let panicked = std::thread::spawn(move || {
            let _state = poisoner.pins.lock().expect("ledger lock");
            panic!("poison the ledger");
        })
        .join();
        assert!(panicked.is_err(), "the helper thread poisons the ledger");

        let now = Utc::now();
        assert!(
            ledger.publications(now, now).is_err(),
            "an unknown pin set is never published as if it were known"
        );
        assert!(
            ledger.pin(&[]).is_err(),
            "a query is never admitted against a ledger that cannot protect it"
        );
        assert!(
            ledger.is_drained().is_err(),
            "shutdown never treats an unreadable ledger as drained"
        );
    }
}
