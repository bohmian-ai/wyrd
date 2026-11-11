//! Per-tenant cache of gateway admission snapshots.
//!
//! Every gateway call routes on its tenant's [`GatewayTenantSnapshot`]:
//! deployments, credentials, fallback, governance, pricing, and capture
//! policy. These change only when an administrator writes them, so each
//! process keeps the last snapshot it loaded per tenant and serves it with no
//! IO. Triggers on the four snapshot tables send a [`SNAPSHOT_CHANNEL`]
//! notification naming the tenant from inside the committing transaction;
//! the notification queues that tenant for a background reload, and the
//! previous snapshot keeps serving until the reload installs the committed
//! configuration. The administrator's own server reloads right after its
//! commit.
//!
//! A snapshot serves only while this node hears every change: like the
//! Verifier target cache, the listener's absence or any lost connection
//! forgets everything and queues every forgotten tenant for reload. A call
//! finding no snapshot never loads one itself: it queues its tenant and is
//! refused with the retryable 503, because an unknown policy cannot be
//! assumed to carry no quotas. A reload that does not install stays queued
//! until one does, and a failed one forgets the tenant's snapshot meanwhile;
//! see [`GatewaySnapshots::install`]. A notification is delivered after its commit,
//! so a call racing an administrative write can still route on the previous
//! snapshot for that delivery and reload latency, as a call admitted just
//! before the write would.
//!
//! At most [`CACHE_TENANTS`] tenants' snapshots are retained, evicting the
//! least recently used, and at most as many tenants wait for a reload,
//! because every Wyrd-owned buffer is bounded.

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use lru::LruCache;
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wyrd_gateway::GatewayTenantSnapshot;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;

use super::service::load_snapshot;
use crate::config::GatewayConfig;

/// Postgres channel the snapshot triggers notify on.
pub(crate) const SNAPSHOT_CHANNEL: &str = "wyrd_gateway_snapshot";

/// Most tenants whose snapshots one process retains.
const CACHE_TENANTS: NonZeroUsize = NonZeroUsize::new(4_096).expect("4096 is non-zero");

/// Cache state guarded by one lock, so a fill and an invalidation are ordered
/// by it.
struct Entries {
    /// Cached snapshots by tenant, least recently used evicted first.
    snapshots: LruCache<DataTenantId, Arc<GatewayTenantSnapshot>>,
    /// Bumped by every invalidation; a fill that began under an older value
    /// is not cached.
    generation: u64,
    /// Whether this node is currently listening for invalidations.
    live: bool,
    /// Tenants waiting for a background reload, at most [`CACHE_TENANTS`].
    reloads: HashSet<DataTenantId>,
}

impl Default for Entries {
    /// No snapshots, generation zero, and not yet listening.
    fn default() -> Self {
        Self {
            snapshots: LruCache::new(CACHE_TENANTS),
            generation: 0,
            live: false,
            reloads: HashSet::new(),
        }
    }
}

impl Entries {
    /// Queues `tenant` for reload unless the queue is full.
    fn queue(&mut self, tenant: DataTenantId) {
        if self.reloads.len() < CACHE_TENANTS.get() {
            self.reloads.insert(tenant);
        }
    }
}

/// Tenant-keyed cache of gateway snapshots; see the module docs for when it
/// serves and how much it retains.
// ponytail: bounded by tenant count, not bytes; charge each snapshot's size
// like the Verifier target cache if tenants' configurations grow large.
pub struct GatewaySnapshots {
    /// Entries behind one short synchronous lock; no await happens under it.
    entries: Mutex<Entries>,
    /// Wakes the reload worker when a tenant is queued.
    wake: Notify,
    /// Whether the listener is live, for waiters.
    live: watch::Sender<bool>,
}

impl Default for GatewaySnapshots {
    /// No snapshots and not yet listening.
    fn default() -> Self {
        Self {
            entries: Mutex::default(),
            wake: Notify::new(),
            live: watch::Sender::new(false),
        }
    }
}

impl GatewaySnapshots {
    /// Locks the entries; a poisoned lock is recovered, because every holder
    /// leaves them consistent before anything under it could panic.
    fn entries(&self) -> MutexGuard<'_, Entries> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The snapshot cached for `tenant`, served only while listening.
    pub(crate) fn cached(&self, tenant: DataTenantId) -> Option<Arc<GatewayTenantSnapshot>> {
        let mut entries = self.entries();
        if !entries.live {
            return None;
        }
        entries.snapshots.get(&tenant).map(Arc::clone)
    }

    /// The snapshot cached for `tenant`, or `None` after queueing the
    /// tenant for a background reload.
    pub(crate) fn admitted(&self, tenant: DataTenantId) -> Option<Arc<GatewayTenantSnapshot>> {
        let mut entries = self.entries();
        if entries.live
            && let Some(snapshot) = entries.snapshots.get(&tenant)
        {
            return Some(Arc::clone(snapshot));
        }
        entries.queue(tenant);
        drop(entries);
        self.wake.notify_one();
        None
    }

    /// Takes every tenant waiting for a reload, waiting until one is queued.
    pub(crate) async fn reloads(&self) -> Vec<DataTenantId> {
        loop {
            let queued: Vec<DataTenantId> = self.entries().reloads.drain().collect();
            if !queued.is_empty() {
                return queued;
            }
            self.wake.notified().await;
        }
    }

    /// Loads `tenant`'s snapshot from `postgres` with `config`'s bindings and
    /// installs it; see [`Self::install`] for what a load that does not
    /// install leaves behind.
    ///
    /// Returns whether the snapshot was cached.
    ///
    /// # Errors
    /// Returns the load's error after logging it.
    pub(crate) async fn reload(
        &self,
        postgres: &wyrd_sql::WyrdPostgres,
        config: &GatewayConfig,
        tenant: DataTenantId,
    ) -> Result<bool, WyrdError> {
        let Some(generation) = self.fill_generation() else {
            return Ok(false);
        };
        let loaded = load_snapshot(postgres, config, tenant).await;
        self.install(tenant, generation, loaded)
    }

    /// Installs one reload of `tenant` that started under `generation`,
    /// keeping the reload owed until a current load installs.
    ///
    /// A load raced by any invalidation is not cached and queues `tenant`
    /// again, since the invalidation may have named another tenant. A failed
    /// load also forgets `tenant`'s snapshot, which a reload only replaces
    /// once it may be stale, so calls are refused through the cache-miss 503
    /// rather than served an obsolete policy while storage fails; other
    /// tenants keep serving. Returns whether the snapshot was cached.
    ///
    /// # Errors
    /// Returns the failed load's error after logging it.
    fn install(
        &self,
        tenant: DataTenantId,
        generation: u64,
        loaded: Result<GatewayTenantSnapshot, WyrdError>,
    ) -> Result<bool, WyrdError> {
        match loaded {
            Ok(snapshot) => {
                let cached = self.insert(tenant, Some(generation), snapshot).1;
                if !cached {
                    self.requeue(tenant);
                }
                Ok(cached)
            }
            Err(error) => {
                tracing::warn!(tenant = %tenant.as_uuid(), error = %error, "gateway snapshot reload failed");
                self.entries().snapshots.pop(&tenant);
                self.requeue(tenant);
                Err(error)
            }
        }
    }

    /// Queues `tenant` for reload again while listening; a node that is not
    /// listening queues every tenant it forgot once it listens again.
    fn requeue(&self, tenant: DataTenantId) {
        let mut entries = self.entries();
        if entries.live {
            entries.queue(tenant);
            drop(entries);
            self.wake.notify_one();
        }
    }

    /// Waits until the listener is live.
    ///
    /// # Panics
    /// Panics if the cache's own watch sender is gone, which cannot happen
    /// while `self` is borrowed.
    #[cfg(test)]
    pub(crate) async fn listening(&self) {
        self.live
            .subscribe()
            .wait_for(|live| *live)
            .await
            .expect("the cache owns its sender");
    }

    /// The generation a load must start under for its snapshot to be cached,
    /// or `None` when the node is not listening.
    pub(crate) fn fill_generation(&self) -> Option<u64> {
        let entries = self.entries();
        entries.live.then_some(entries.generation)
    }

    /// Caches `snapshot` for `tenant` when `generation` is still current while
    /// listening, and returns it with whether it was cached; a load that
    /// raced an invalidation is returned but not cached.
    pub(crate) fn insert(
        &self,
        tenant: DataTenantId,
        generation: Option<u64>,
        snapshot: GatewayTenantSnapshot,
    ) -> (Arc<GatewayTenantSnapshot>, bool) {
        let snapshot = Arc::new(snapshot);
        let mut entries = self.entries();
        let current = entries.live && generation == Some(entries.generation);
        if current {
            entries.snapshots.put(tenant, Arc::clone(&snapshot));
        }
        (snapshot, current)
    }

    /// Queues the tenant a notification names for reload, or every cached
    /// tenant for a payload that names none; cached snapshots keep serving
    /// until their reload installs.
    fn invalidate(&self, payload: &str) {
        tracing::debug!(payload, "gateway snapshot invalidated");
        let mut entries = self.entries();
        entries.generation = entries.generation.wrapping_add(1);
        if let Some(tenant) = Uuid::parse_str(payload)
            .ok()
            .and_then(|tenant| DataTenantId::new(tenant).ok())
        {
            entries.queue(tenant);
        } else {
            let cached: Vec<DataTenantId> = entries.snapshots.iter().map(|(t, _)| *t).collect();
            for tenant in cached {
                entries.queue(tenant);
            }
        }
        drop(entries);
        self.wake.notify_one();
    }

    /// Forgets every snapshot, queues each forgotten tenant for reload, and
    /// records whether the node is now listening.
    fn reset(&self, live: bool) {
        let mut entries = self.entries();
        entries.generation = entries.generation.wrapping_add(1);
        let forgotten: Vec<DataTenantId> = entries.snapshots.iter().map(|(t, _)| *t).collect();
        for tenant in forgotten {
            entries.queue(tenant);
        }
        entries.snapshots.clear();
        entries.live = live;
        drop(entries);
        self.live.send_replace(live);
        self.wake.notify_one();
    }

    /// How many invalidations and listener resets this cache has applied, so
    /// a test can wait until a committed write's notification has arrived
    /// instead of sleeping.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn invalidations(&self) -> u64 {
        self.entries().generation
    }

    /// Listens for snapshot invalidations from `postgres` until `shutdown`.
    ///
    /// Snapshots serve only while the listener is subscribed and its
    /// heartbeat answers, so a lost or silently stalled listener stops
    /// serving within the heartbeat bound; see
    /// [`wyrd_sql::NotificationListener::watch`].
    pub async fn watch(&self, postgres: &wyrd_sql::WyrdPostgres, shutdown: CancellationToken) {
        postgres
            .listener(SNAPSHOT_CHANNEL)
            .watch(
                shutdown,
                |live| self.reset(live),
                |payload| self.invalidate(payload),
            )
            .await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::num::NonZeroU64;
    use std::sync::Arc;

    use wyrd_gateway::GatewayTenantSnapshot;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::gateway::{GatewayCaptureMode, GatewayCapturePolicy};

    use super::{CACHE_TENANTS, GatewaySnapshots};

    /// An empty snapshot.
    fn snapshot() -> GatewayTenantSnapshot {
        GatewayTenantSnapshot {
            deployments: Vec::new(),
            credentials: Vec::new(),
            fallback: wyrd_spec::gateway::GatewayFallbackPolicy::default(),
            governance: wyrd_spec::gateway::GatewayGovernancePolicy::default(),
            capture: GatewayCapturePolicy {
                mode: GatewayCaptureMode::Disabled,
                payload_fields: BTreeSet::new(),
                version: NonZeroU64::MIN,
            },
        }
    }

    /// Snapshots serve only while listening and only for their tenant; a
    /// tenant notification queues that tenant's reload while its snapshot
    /// keeps serving, a malformed one queues every tenant, a lost listener
    /// forgets and queues everything, a miss is queued rather than loaded,
    /// and a load that raced an invalidation is not cached.
    #[tokio::test]
    async fn snapshots_serve_only_while_listening_and_reload_in_background() {
        let cache = GatewaySnapshots::default();
        let (tenant, other) = (DataTenantId::new_v7(), DataTenantId::new_v7());

        cache.insert(tenant, cache.fill_generation(), snapshot());
        assert!(cache.cached(tenant).is_none(), "not listening");

        cache.reset(true);
        cache.insert(tenant, cache.fill_generation(), snapshot());
        cache.insert(other, cache.fill_generation(), snapshot());
        assert!(cache.cached(tenant).is_some());

        let stale = cache.fill_generation();
        cache.invalidate(&tenant.as_uuid().to_string());
        assert!(cache.cached(tenant).is_some(), "serves until reloaded");
        assert!(
            !cache.insert(tenant, stale, snapshot()).1,
            "a raced load is not cached"
        );
        assert_eq!(cache.reloads().await, vec![tenant], "the tenant is queued");

        cache.invalidate("malformed");
        let mut queued = cache.reloads().await;
        queued.sort();
        let mut both = vec![tenant, other];
        both.sort();
        assert_eq!(queued, both, "a malformed payload queues every tenant");

        let missing = DataTenantId::new_v7();
        assert!(cache.admitted(missing).is_none(), "a miss never loads");
        assert_eq!(cache.reloads().await, vec![missing], "a miss is queued");

        cache.reset(false);
        cache.reset(true);
        assert!(cache.cached(tenant).is_none(), "a lost listener forgets");
        assert_eq!(cache.reloads().await.len(), 2, "forgotten tenants reload");
    }

    /// A reload fenced by another tenant's invalidation stays queued and
    /// installs on its next attempt without another edit, and a failed load
    /// forgets only that tenant's snapshot and stays queued until a load
    /// installs.
    ///
    /// # Panics
    /// Panics when a fenced or failed reload is dropped, a fenced load is
    /// cached, a failed tenant keeps serving, or another tenant stops serving.
    #[tokio::test]
    async fn reloads_that_do_not_install_stay_owed() {
        let cache = GatewaySnapshots::default();
        cache.reset(true);
        let (a, b) = (DataTenantId::new_v7(), DataTenantId::new_v7());
        cache.insert(a, cache.fill_generation(), snapshot());
        cache.insert(b, cache.fill_generation(), snapshot());
        let obsolete = cache.cached(a).expect("a serves");

        cache.invalidate(&a.as_uuid().to_string());
        assert_eq!(cache.reloads().await, vec![a]);
        let started = cache.fill_generation().expect("listening");
        cache.invalidate(&b.as_uuid().to_string());
        assert_eq!(
            cache.install(a, started, Ok(snapshot())).ok(),
            Some(false),
            "a fenced load is not cached"
        );
        let mut owed = cache.reloads().await;
        owed.sort();
        let mut both = vec![a, b];
        both.sort();
        assert_eq!(owed, both, "the fenced reload stays queued beside b");
        let retry = cache.fill_generation().expect("listening");
        assert_eq!(cache.install(a, retry, Ok(snapshot())).ok(), Some(true));
        assert!(
            !Arc::ptr_eq(&cache.cached(a).expect("a serves"), &obsolete),
            "the retry replaces the obsolete snapshot"
        );

        let failing = cache.fill_generation().expect("listening");
        let failure = WyrdError::ServiceUnavailable {
            message: "storage is down".to_owned(),
            details: serde_json::json!({}),
        };
        assert!(cache.install(a, failing, Err(failure)).is_err());
        assert!(cache.cached(a).is_none(), "a failed reload stops serving");
        assert!(cache.cached(b).is_some(), "other tenants keep serving");
        assert_eq!(cache.reloads().await, vec![a], "the failed reload is owed");
        let recovered = cache.fill_generation().expect("listening");
        assert_eq!(cache.install(a, recovered, Ok(snapshot())).ok(), Some(true));
        assert!(cache.cached(a).is_some(), "recovery serves again");
    }

    /// The cache retains at most [`CACHE_TENANTS`] tenants, evicting the
    /// least recently served one, while every retained tenant keeps serving.
    #[test]
    fn snapshots_retain_a_bounded_number_of_tenants() {
        let cache = GatewaySnapshots::default();
        cache.reset(true);
        let first = DataTenantId::new_v7();
        let second = DataTenantId::new_v7();
        cache.insert(first, cache.fill_generation(), snapshot());
        cache.insert(second, cache.fill_generation(), snapshot());
        for _ in 2..CACHE_TENANTS.get() {
            cache.insert(DataTenantId::new_v7(), cache.fill_generation(), snapshot());
        }
        assert!(cache.cached(first).is_some(), "serving refreshes recency");

        cache.insert(DataTenantId::new_v7(), cache.fill_generation(), snapshot());
        assert!(
            cache.cached(first).is_some(),
            "the recently served tenant stays"
        );
        assert!(
            cache.cached(second).is_none(),
            "the least recently served tenant is evicted"
        );
    }
}
