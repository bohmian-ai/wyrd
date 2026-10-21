//! Per-tenant cache of gateway admission snapshots.
//!
//! Every gateway call routes on its tenant's [`GatewayTenantSnapshot`]:
//! deployments, credentials, fallback, governance, pricing, and capture
//! policy. These change only when an administrator writes them, so each
//! process keeps the last snapshot it loaded per tenant and serves it with no
//! IO. Triggers on the four snapshot tables send a [`SNAPSHOT_CHANNEL`]
//! notification naming the tenant from inside the committing transaction,
//! and the notification forgets that tenant's snapshot, so the next call
//! loads the committed configuration.
//!
//! A snapshot serves only while this node hears every change: like the
//! Verifier target cache, the listener's absence or any lost connection
//! forgets everything, so every call then reads Postgres. A notification is
//! delivered after its commit, so a call racing an administrative write can
//! still route on the previous snapshot for that delivery latency, as a call
//! admitted just before the write would.
//!
//! At most [`CACHE_TENANTS`] tenants' snapshots are retained, evicting the
//! least recently used, because every Wyrd-owned buffer is bounded; an
//! evicted tenant's next call reads Postgres.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use lru::LruCache;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wyrd_gateway::GatewayTenantSnapshot;
use wyrd_spec::DataTenantId;

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
}

impl Default for Entries {
    /// No snapshots, generation zero, and not yet listening.
    fn default() -> Self {
        Self {
            snapshots: LruCache::new(CACHE_TENANTS),
            generation: 0,
            live: false,
        }
    }
}

/// Tenant-keyed cache of gateway snapshots; see the module docs for when it
/// serves and how much it retains.
// ponytail: bounded by tenant count, not bytes; charge each snapshot's size
// like the Verifier target cache if tenants' configurations grow large.
#[derive(Default)]
pub struct GatewaySnapshots {
    /// Entries behind one short synchronous lock; no await happens under it.
    entries: Mutex<Entries>,
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

    /// The generation a load must start under for its snapshot to be cached,
    /// or `None` when the node is not listening.
    pub(crate) fn fill_generation(&self) -> Option<u64> {
        let entries = self.entries();
        entries.live.then_some(entries.generation)
    }

    /// Caches `snapshot` for `tenant` when `generation` is still current while
    /// listening, and returns it either way; a load that raced an
    /// invalidation is returned but not cached.
    pub(crate) fn insert(
        &self,
        tenant: DataTenantId,
        generation: Option<u64>,
        snapshot: GatewayTenantSnapshot,
    ) -> Arc<GatewayTenantSnapshot> {
        let snapshot = Arc::new(snapshot);
        let mut entries = self.entries();
        if entries.live && generation == Some(entries.generation) {
            entries.snapshots.put(tenant, Arc::clone(&snapshot));
        }
        snapshot
    }

    /// Forgets the tenant a notification names, or every tenant for a
    /// payload that names none.
    fn invalidate(&self, payload: &str) {
        tracing::debug!(payload, "gateway snapshot invalidated");
        let mut entries = self.entries();
        entries.generation = entries.generation.wrapping_add(1);
        match Uuid::parse_str(payload)
            .ok()
            .and_then(|tenant| DataTenantId::new(tenant).ok())
        {
            Some(tenant) => {
                entries.snapshots.pop(&tenant);
            }
            None => entries.snapshots.clear(),
        }
    }

    /// Forgets every snapshot and records whether the node is now listening.
    fn reset(&self, live: bool) {
        let mut entries = self.entries();
        entries.generation = entries.generation.wrapping_add(1);
        entries.snapshots.clear();
        entries.live = live;
    }

    /// How many invalidations and listener resets this cache has applied, so
    /// a test can wait until a committed write's notification has arrived
    /// instead of sleeping.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn invalidations(&self) -> u64 {
        self.entries().generation
    }

    /// Listens for snapshot invalidations from `postgres` until `shutdown`; see
    /// [`crate::postgres::watch_channel`].
    pub async fn watch(&self, postgres: &wyrd_sql::WyrdPostgres, shutdown: CancellationToken) {
        crate::postgres::watch_channel(
            postgres,
            SNAPSHOT_CHANNEL,
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

    use wyrd_gateway::GatewayTenantSnapshot;
    use wyrd_spec::DataTenantId;
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
    /// tenant notification forgets that tenant, a malformed one and a lost
    /// listener everything, and a load that raced an invalidation is not
    /// cached.
    #[test]
    fn snapshots_serve_only_while_listening_and_until_invalidated() {
        let cache = GatewaySnapshots::default();
        let (tenant, other) = (DataTenantId::new_v7(), DataTenantId::new_v7());

        cache.insert(tenant, cache.fill_generation(), snapshot());
        assert!(cache.cached(tenant).is_none(), "not listening");

        cache.reset(true);
        cache.insert(tenant, cache.fill_generation(), snapshot());
        cache.insert(other, cache.fill_generation(), snapshot());
        assert!(cache.cached(tenant).is_some());

        cache.invalidate(&tenant.as_uuid().to_string());
        assert!(cache.cached(tenant).is_none(), "the tenant changed");
        assert!(cache.cached(other).is_some(), "another tenant is kept");

        let stale = cache.fill_generation();
        cache.invalidate("malformed");
        assert!(
            cache.cached(other).is_none(),
            "a malformed payload forgets all"
        );
        cache.insert(tenant, stale, snapshot());
        assert!(cache.cached(tenant).is_none(), "a raced load is not cached");

        cache.insert(tenant, cache.fill_generation(), snapshot());
        cache.reset(false);
        cache.reset(true);
        assert!(cache.cached(tenant).is_none(), "a lost listener forgets");
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
