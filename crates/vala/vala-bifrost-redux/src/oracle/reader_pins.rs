//! Snapshots this Oracle node's in-flight cuts still depend on.
//!
//! A pinned cut is immutable for the life of its query, which means the
//! Iceberg snapshot it names must survive that long. Forge maintenance runs in
//! another process and cannot see an in-flight query, so this node keeps a
//! ledger of what its live cuts need and republishes it durably on its ordinary
//! heartbeat. The ledger is the in-process half of that: pins are held by RAII
//! for exactly the query's lifetime, so a query that fails, is cancelled, or
//! panics stops protecting its snapshot without any cleanup path of its own.
//!
//! Nothing here decides whether a snapshot may be expired. It reports what this
//! node needs; the maintenance owner corroborates that against Iceberg.

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

    /// Records the snapshots one query's pinned cuts depend on.
    ///
    /// A cut with no Iceberg snapshot, or one whose snapshot the loaded
    /// metadata cannot date, contributes nothing: there is no snapshot for
    /// maintenance to protect, and a watermark without a corroborating
    /// timestamp would fail the maintenance owner's own validation rather than
    /// protect anything.
    #[must_use]
    pub fn pin(self: &Arc<Self>, cuts: &[PinnedSealedTable]) -> ReaderPinGuard {
        let mut held = Vec::new();
        if let Ok(mut state) = self.pins.lock() {
            for cut in cuts {
                let Some(snapshot_id) = cut.snapshot_id else {
                    continue;
                };
                let Some(snapshot) = cut.iceberg_table.metadata().snapshot_by_id(snapshot_id)
                else {
                    continue;
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
        ReaderPinGuard {
            ledger: Arc::clone(self),
            held,
        }
    }

    /// Projects the current dependency set into one publication per tenant.
    ///
    /// Each table reports the oldest snapshot any live cut still needs, because
    /// protecting the newest would leave an older in-flight cut unprotected.
    /// Tenants with no live pin are absent, and the caller must still clear
    /// that tenant's previous publication for this node.
    #[must_use]
    pub fn publications(
        &self,
        published_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> BTreeMap<DataTenantId, ReaderWatermarkPublication> {
        let mut oldest = BTreeMap::<PinnedTable, SnapshotWatermark>::new();
        if let Ok(state) = self.pins.lock() {
            for (table, watermark) in state.entries.values() {
                oldest
                    .entry(table.clone())
                    .and_modify(|held| {
                        if watermark.timestamp_ms < held.timestamp_ms {
                            *held = *watermark;
                        }
                    })
                    .or_insert(*watermark);
            }
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
        publications
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
    /// process is already failing, and the publication that would read this
    /// state fails closed on the same lock.
    fn drop(&mut self) {
        if let Ok(mut state) = self.ledger.pins.lock() {
            for pin_id in self.held.drain(..) {
                state.entries.remove(&pin_id);
            }
        }
    }
}

/// Republishes one Oracle node's live snapshot dependencies durably.
///
/// The publisher exists because the ledger is process-local and Forge is not.
/// It runs on its own cadence rather than inside the query path so a reader
/// never pays a Postgres round trip to start reading, and it publishes an
/// expiry one cadence ahead so a node that stops publishing — crashed, wedged,
/// or partitioned — stops protecting snapshots without anyone deciding that it
/// should.
pub struct ReaderWatermarkPublisher {
    /// Ledger whose current state is published each round.
    ledger: Arc<ReaderPinLedger>,
    /// Tenant-scoped SQL handle used for every publication.
    vala: vala_sql::ValaPostgres,
    /// Physical node identity every published row is attributed to.
    node_id: uuid::Uuid,
    /// Delay between publications.
    interval: std::time::Duration,
    /// How far past a publication its rows keep protecting snapshots.
    lease: chrono::Duration,
    /// Tenants this node published for last round, so a released tenant clears.
    published_tenants: std::collections::BTreeSet<DataTenantId>,
    /// Lifecycle token that ends the loop.
    shutdown: tokio_util::sync::CancellationToken,
}

impl ReaderWatermarkPublisher {
    /// Builds one publisher for this node's ledger.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when the interval is
    /// zero, which would spin, or when the lease does not outlast one interval,
    /// which would let a healthy node's protection lapse between publications.
    fn new(
        ledger: Arc<ReaderPinLedger>,
        vala: vala_sql::ValaPostgres,
        node_id: uuid::Uuid,
        interval: std::time::Duration,
        shutdown: tokio_util::sync::CancellationToken,
    ) -> Result<Self, BifrostError> {
        let lease = chrono::Duration::from_std(interval.saturating_mul(3)).map_err(|_| {
            BifrostError::Internal {
                detail: "Oracle reader-watermark lease is not representable".to_owned(),
            }
        })?;
        if interval.is_zero() {
            return Err(BifrostError::Internal {
                detail: "Oracle reader-watermark interval must be positive".to_owned(),
            });
        }
        Ok(Self {
            ledger,
            vala,
            node_id,
            interval,
            lease,
            published_tenants: std::collections::BTreeSet::new(),
            shutdown,
        })
    }

    /// Spawns one publisher onto the ambient runtime at the lifecycle cadence.
    ///
    /// Construction and spawning live together because a publisher that is
    /// built but never polled is indistinguishable from one that is running,
    /// yet protects nothing.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::Internal`] when no Tokio runtime is entered, or
    /// the construction errors described by [`Self::new`].
    pub(crate) fn spawn(
        ledger: Arc<ReaderPinLedger>,
        vala: vala_sql::ValaPostgres,
        node_id: uuid::Uuid,
        shutdown: tokio_util::sync::CancellationToken,
    ) -> Result<tokio::task::JoinHandle<()>, BifrostError> {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| BifrostError::Internal {
            detail: "Oracle reader-watermark publication requires an active Tokio runtime"
                .to_owned(),
        })?;
        Ok(handle.spawn(
            Self::new(
                ledger,
                vala,
                node_id,
                READER_WATERMARK_PUBLISH_INTERVAL,
                shutdown,
            )?
            .run(),
        ))
    }

    /// Publishes until shutdown, then retires this node's rows.
    ///
    /// A failed round is logged and retried on the next tick rather than
    /// ending the loop: the previous publication is still valid for the rest of
    /// its lease, so a transient Postgres failure costs freshness, not safety.
    /// Retirement on shutdown is best effort for the same reason — a node that
    /// cannot retire cleanly still lapses when its lease expires.
    pub async fn run(mut self) {
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                biased;
                () = self.shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    if let Err(error) = self.publish_round().await {
                        tracing::warn!(
                            error = %error,
                            "Oracle could not publish its reader watermarks"
                        );
                    }
                }
            }
        }
        if let Err(error) = self.retire_all().await {
            tracing::warn!(
                error = %error,
                "Oracle could not retire its reader watermarks on shutdown"
            );
        }
    }

    /// Writes one complete publication round.
    ///
    /// # Errors
    ///
    /// Returns the SQL failure from the first tenant whose publication could
    /// not commit. Earlier tenants in the round are already committed and stay
    /// published; each tenant is its own transaction because they are separate
    /// isolation scopes, not one atomic fact.
    async fn publish_round(&mut self) -> Result<(), vala_sql::SqlError> {
        let published_at = chrono::Utc::now();
        let publications = self
            .ledger
            .publications(published_at, published_at + self.lease);
        for tenant in self
            .published_tenants
            .iter()
            .copied()
            .filter(|tenant| !publications.contains_key(tenant))
            .collect::<Vec<_>>()
        {
            self.retire_tenant(tenant).await?;
        }
        self.published_tenants = publications.keys().copied().collect();
        for (tenant, publication) in &publications {
            let mut conn = self.vala.tenant_conn(*tenant).await?;
            vala_sql::queries::reader_watermarks::BifrostReaderWatermarks::new(&mut conn)
                .publish(self.node_id, publication)
                .await?;
            conn.commit().await?;
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
    async fn retire_all(&mut self) -> Result<(), vala_sql::SqlError> {
        for tenant in std::mem::take(&mut self.published_tenants) {
            self.retire_tenant(tenant).await?;
        }
        Ok(())
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
}
