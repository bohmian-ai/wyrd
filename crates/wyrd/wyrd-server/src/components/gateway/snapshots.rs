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
//! finding no snapshot loads it from Postgres and caches it, and every call
//! missing the same tenant at once shares that one load; see
//! [`GatewaySnapshots::load_through`]. Only a miss reads Postgres; a hit is
//! served from memory. A reload that does not install stays queued until one
//! does, and a failed one forgets the tenant's snapshot meanwhile; see
//! [`GatewaySnapshots::install`]. A notification is delivered after its commit,
//! so a call racing an administrative write can still route on the previous
//! snapshot for that delivery and reload latency, as a call admitted just
//! before the write would.
//!
//! At most [`CACHE_TENANTS`] tenants' snapshots are retained, evicting the
//! least recently used, and at most as many tenants wait for a reload,
//! because every Wyrd-owned buffer is bounded. A tenant whose reload the full
//! queue cannot hold stops serving until a later cache miss queues it.

use std::collections::{HashMap, HashSet};
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

/// One tenant's in-flight miss load, whose result every call that missed
/// the tenant meanwhile shares.
type Load = Arc<tokio::sync::OnceCell<Result<Arc<GatewayTenantSnapshot>, WyrdError>>>;

/// One call's participation in its tenant's in-flight miss load.
///
/// Every clone of a [`Load`] except the map's own belongs to a
/// participant and is taken or dropped only under the map's lock, so the
/// participant that drops the last of them sees it and removes the map
/// entry; a cancelled last caller therefore leaves no entry behind.
struct Participant<'a> {
    /// The in-flight loads by tenant.
    loads: &'a Mutex<HashMap<DataTenantId, Load>>,
    /// The tenant whose load this call shares.
    tenant: DataTenantId,
    /// This call's share of the load, taken when the participant ends.
    load: Option<Load>,
}

impl<'a> Participant<'a> {
    /// Joins `tenant`'s in-flight load in `loads`, starting a new one when
    /// none is in flight.
    fn join(loads: &'a Mutex<HashMap<DataTenantId, Load>>, tenant: DataTenantId) -> Self {
        let mut map = loads.lock().unwrap_or_else(PoisonError::into_inner);
        let load = Arc::clone(map.entry(tenant).or_default());
        Self {
            loads,
            tenant,
            load: Some(load),
        }
    }

    /// The shared load.
    ///
    /// # Panics
    /// Panics if called after the participant ended, which its owner
    /// cannot do because ending consumes it.
    fn load(&self) -> &Load {
        self.load
            .as_ref()
            .expect("a participant holds its load until it ends")
    }

    /// Ends a participant whose load completed, removing the load's entry
    /// even while peers still hold it, so a later miss starts afresh
    /// rather than sharing an ended result such as a failure.
    fn finish(mut self) {
        self.leave(true);
    }

    /// Releases this participant's share under the map's lock, removing
    /// the tenant's entry when it is still this load and either `ended` or
    /// no other participant holds it; a replacement load is never removed.
    fn leave(&mut self, ended: bool) {
        let Some(load) = self.load.take() else {
            return;
        };
        let mut map = self.loads.lock().unwrap_or_else(PoisonError::into_inner);
        if map.get(&self.tenant).is_some_and(|current| {
            Arc::ptr_eq(current, &load) && (ended || Arc::strong_count(&load) == 2)
        }) {
            map.remove(&self.tenant);
        }
        drop(load);
    }
}

impl Drop for Participant<'_> {
    /// Leaves the load as a participant that stopped waiting before it
    /// ended, such as a cancelled call.
    fn drop(&mut self) {
        self.leave(false);
    }
}

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
    /// Queues `tenant` for reload, or forgets its snapshot when the full
    /// queue cannot hold it, so a cached policy never outlives its owed
    /// reload; a forgotten tenant is queued again by its next cache miss.
    fn queue(&mut self, tenant: DataTenantId) {
        if self.reloads.len() < CACHE_TENANTS.get() || self.reloads.contains(&tenant) {
            self.reloads.insert(tenant);
        } else {
            self.snapshots.pop(&tenant);
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
    /// The miss load in flight for each tenant, removed once it ends or
    /// its last participant stops waiting; see [`Participant`]. Held only
    /// to join or leave a load, never across one.
    loads: Mutex<HashMap<DataTenantId, Load>>,
}

impl Default for GatewaySnapshots {
    /// No snapshots and not yet listening.
    fn default() -> Self {
        Self {
            entries: Mutex::default(),
            wake: Notify::new(),
            live: watch::Sender::new(false),
            loads: Mutex::default(),
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

    /// The snapshot `tenant`'s calls route on: the cached one, else one
    /// loaded from `postgres` with `config`'s bindings and cached while
    /// listening.
    ///
    /// Calls missing the same tenant at once share one load and its result,
    /// so a cold cache reads each tenant once rather than once per call. A
    /// failed load is shared by the calls waiting on it and not kept, so the
    /// next miss loads again. A load that raced an invalidation is returned
    /// but not cached, as [`Self::insert`] decides.
    ///
    /// # Errors
    /// Returns the load's error, such as `ServiceUnavailable` when storage
    /// fails.
    pub(crate) async fn load_through(
        &self,
        postgres: &wyrd_sql::WyrdPostgres,
        config: &GatewayConfig,
        tenant: DataTenantId,
    ) -> Result<Arc<GatewayTenantSnapshot>, WyrdError> {
        if let Some(snapshot) = self.cached(tenant) {
            return Ok(snapshot);
        }
        self.share(tenant, load_snapshot(postgres, config, tenant))
            .await
    }

    /// Runs `load` as `tenant`'s one miss load unless a load for it is
    /// already in flight, whose result this call then shares, and caches a
    /// loaded snapshot as [`Self::insert`] decides.
    ///
    /// The in-flight entry lives only while a call participates in it: it
    /// is removed once its load ends, and when its last participant stops
    /// waiting first, as a cancelled call does. A cancelled participant
    /// with live peers leaves the entry, and the initialization of a
    /// cancelled initializer passes to a remaining peer.
    ///
    /// # Errors
    /// Returns the shared load's error.
    async fn share(
        &self,
        tenant: DataTenantId,
        load: impl Future<Output = Result<GatewayTenantSnapshot, WyrdError>>,
    ) -> Result<Arc<GatewayTenantSnapshot>, WyrdError> {
        let participant = Participant::join(&self.loads, tenant);
        let loaded = participant
            .load()
            .get_or_init(|| async {
                let generation = self.fill_generation();
                Ok(self.insert(tenant, generation, load.await?).0)
            })
            .await
            .clone();
        participant.finish();
        loaded
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
    /// once it may be stale, so its calls load through and fail with storage
    /// rather than being served an obsolete policy; other tenants keep
    /// serving. Returns whether the snapshot was cached.
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::oneshot;
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
    /// forgets and queues everything, and a load that raced an invalidation
    /// is not cached.
    ///
    /// # Panics
    /// Panics when a snapshot serves while not listening or for another
    /// tenant, a notification queues the wrong tenants, a lost listener
    /// keeps serving, or a raced load is cached.
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

    /// A reload the full queue cannot hold stops its tenant serving, whether
    /// a fenced install or an invalidation owes it, so no obsolete policy
    /// outlives its lost reload; the tenant recovers through an ordinary
    /// cache miss once the queue drains, and a healthy tenant keeps serving.
    ///
    /// # Panics
    /// Panics when a tenant keeps serving with no queued reload, the queue
    /// exceeds its bound, the healthy tenant stops serving, or the tenant
    /// does not recover without another edit.
    #[tokio::test]
    async fn reloads_the_full_queue_cannot_hold_stop_serving() {
        let cache = GatewaySnapshots::default();
        cache.reset(true);
        let (a, b, healthy) = (
            DataTenantId::new_v7(),
            DataTenantId::new_v7(),
            DataTenantId::new_v7(),
        );
        for tenant in [a, b, healthy] {
            cache.insert(tenant, cache.fill_generation(), snapshot());
        }
        cache.invalidate(&a.as_uuid().to_string());
        assert_eq!(cache.reloads().await, vec![a], "a's load is in flight");
        let started = cache.fill_generation().expect("listening");
        for _ in 0..CACHE_TENANTS.get() {
            cache.invalidate(&DataTenantId::new_v7().as_uuid().to_string());
        }

        assert_eq!(cache.install(a, started, Ok(snapshot())).ok(), Some(false));
        assert!(cache.cached(a).is_none(), "a fenced reload is not lost");
        cache.invalidate(&b.as_uuid().to_string());
        assert!(cache.cached(b).is_none(), "an invalidation is not lost");
        assert!(cache.cached(healthy).is_some(), "healthy keeps serving");
        assert_eq!(cache.reloads().await.len(), CACHE_TENANTS.get());

        let retry = cache.fill_generation().expect("listening");
        assert_eq!(cache.install(a, retry, Ok(snapshot())).ok(), Some(true));
        assert!(cache.cached(a).is_some(), "a recovers to current policy");
    }

    /// Yields until `ready` holds, letting spawned calls make progress.
    async fn until(ready: impl Fn() -> bool) {
        while !ready() {
            tokio::task::yield_now().await;
        }
    }

    /// A miss load counted in `started` once it runs, which completes with
    /// a snapshot only when its returned sender fires.
    fn held(
        started: &Arc<AtomicUsize>,
    ) -> (
        oneshot::Sender<()>,
        impl Future<Output = Result<GatewayTenantSnapshot, WyrdError>> + Send + 'static,
    ) {
        let (release, released) = oneshot::channel();
        let started = Arc::clone(started);
        (release, async move {
            started.fetch_add(1, Ordering::SeqCst);
            let _ = released.await;
            Ok(snapshot())
        })
    }

    /// Spawns one call sharing `tenant`'s miss load through `load`.
    fn call(
        cache: &Arc<GatewaySnapshots>,
        tenant: DataTenantId,
        load: impl Future<Output = Result<GatewayTenantSnapshot, WyrdError>> + Send + 'static,
    ) -> tokio::task::JoinHandle<Result<Arc<GatewayTenantSnapshot>, WyrdError>> {
        let cache = Arc::clone(cache);
        tokio::spawn(async move { cache.share(tenant, load).await })
    }

    /// The participants holding `tenant`'s in-flight load, or zero when
    /// none is in flight.
    ///
    /// # Panics
    /// Panics when a participant panicked while holding the loads lock.
    fn participants(cache: &GatewaySnapshots, tenant: DataTenantId) -> usize {
        cache
            .loads
            .lock()
            .expect("loads lock")
            .get(&tenant)
            .map_or(0, |load| Arc::strong_count(load) - 1)
    }

    /// A cancelled miss caller leaves its peers sharing one load, the last
    /// participant's cancellation retires only that tenant's entry, cancelled
    /// tenants never accumulate entries, and a failed load is not kept so a
    /// retry loads again and then serves warm.
    ///
    /// # Panics
    /// Panics when peers stop sharing one load, a cancelled last caller or
    /// an ended load leaves an entry, another tenant's entry is removed, or
    /// a failure is served to a later miss.
    #[tokio::test]
    async fn miss_loads_retire_with_their_last_caller() {
        let cache = Arc::new(GatewaySnapshots::default());
        cache.reset(true);
        let started = Arc::new(AtomicUsize::new(0));
        let (tenant, other) = (DataTenantId::new_v7(), DataTenantId::new_v7());

        let (_other_release, other_load) = held(&started);
        let other_call = call(&cache, other, other_load);
        let (_first_release, first_load) = held(&started);
        let first = call(&cache, tenant, first_load);
        until(|| started.load(Ordering::SeqCst) == 2).await;
        let (second_release, second_load) = held(&started);
        let second = call(&cache, tenant, second_load);
        until(|| participants(&cache, tenant) == 2).await;

        first.abort();
        assert!(first.await.is_err_and(|error| error.is_cancelled()));
        until(|| started.load(Ordering::SeqCst) == 3).await;
        assert_eq!(
            participants(&cache, tenant),
            1,
            "the survivor keeps the load"
        );
        let (_unused, third_load) = held(&started);
        let third = call(&cache, tenant, third_load);
        until(|| participants(&cache, tenant) == 2).await;
        second_release.send(()).expect("the survivor loads");
        let (second, third) = (
            second.await.expect("joined").expect("loaded"),
            third.await.expect("joined").expect("loaded"),
        );
        assert!(Arc::ptr_eq(&second, &third), "peers share one result");
        assert_eq!(started.load(Ordering::SeqCst), 3, "the joiner did not load");
        assert_eq!(participants(&cache, tenant), 0, "an ended load retires");
        assert!(cache.cached(tenant).is_some(), "the load serves warm");

        for _ in 0..8 {
            let cancelled = DataTenantId::new_v7();
            let before = started.load(Ordering::SeqCst);
            let (_release, load) = held(&started);
            let last = call(&cache, cancelled, load);
            until(|| started.load(Ordering::SeqCst) == before + 1).await;
            last.abort();
            assert!(last.await.is_err_and(|error| error.is_cancelled()));
            assert_eq!(
                participants(&cache, cancelled),
                0,
                "the last caller retires"
            );
        }
        assert_eq!(
            cache.loads.lock().expect("loads lock").len(),
            1,
            "only the other tenant's live load remains"
        );
        assert_eq!(participants(&cache, other), 1, "other tenants keep theirs");
        other_call.abort();
        assert!(other_call.await.is_err_and(|error| error.is_cancelled()));

        let failing = DataTenantId::new_v7();
        let failure = cache
            .share(failing, async {
                Err(WyrdError::ServiceUnavailable {
                    message: "storage is down".to_owned(),
                    details: serde_json::json!({}),
                })
            })
            .await;
        assert!(failure.is_err());
        assert_eq!(participants(&cache, failing), 0, "a failed load retires");
        assert!(cache.share(failing, async { Ok(snapshot()) }).await.is_ok());
        assert!(cache.cached(failing).is_some(), "the retry serves warm");
        assert!(cache.loads.lock().expect("loads lock").is_empty());
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
