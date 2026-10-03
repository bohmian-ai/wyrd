//! The per-process cache of parsed Verifier Cards.
//!
//! A Card's spec never changes under its UID, so a cached entry needs no
//! content invalidation: the claim transaction reports whether the Card still
//! exists, and only on a miss reads its spec, so loading a Verifier never
//! opens its own connection. Entries are keyed by tenant and Card UID and are
//! never shared across tenants. The cache is bounded by total bytes with
//! least-recently-used eviction, because every Wyrd-owned buffer is bounded.

use std::sync::{Arc, Mutex};

use lru::LruCache;
use wyrd_spec::DataTenantId;
use wyrd_spec::card::verifier::VerifierImplementation;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;

/// Total bytes of parsed Verifier Cards one process keeps.
const CACHE_BYTES: usize = 64 * 1024 * 1024;

/// One parsed Verifier Card: its exact reference and implementation.
#[derive(Debug)]
pub(crate) struct CachedVerifier {
    /// The exact Verifier, with its UID, that result rows are attributed to.
    pub(crate) reference: CardRef,
    /// The implementation the run executes.
    pub(crate) implementation: VerifierImplementation,
    /// The entry's charge against [`CACHE_BYTES`]: its serialized size.
    bytes: usize,
}

impl CachedVerifier {
    /// Build an entry charged at the serialized size of `reference` and
    /// `implementation`.
    pub(crate) fn new(reference: CardRef, implementation: VerifierImplementation) -> Self {
        let bytes = serde_json::to_vec(&(&reference, &implementation)).map_or(0, |json| json.len());
        Self {
            reference,
            implementation,
            bytes,
        }
    }
}

/// Least-recently-used entries and the bytes they hold.
struct Entries {
    /// Parsed Cards by tenant and UID, most recently used last.
    cards: LruCache<(DataTenantId, CardUid), Arc<CachedVerifier>>,
    /// Sum of every held entry's charge.
    bytes: usize,
}

/// Byte-bounded, tenant-keyed cache of parsed Verifier Cards.
pub(crate) struct VerifierCache {
    /// Entries behind one short synchronous lock; no await happens under it.
    entries: Mutex<Entries>,
    /// Byte ceiling, [`CACHE_BYTES`] in production.
    capacity: usize,
}

impl Default for VerifierCache {
    /// An empty cache bounded at 64 MiB.
    fn default() -> Self {
        Self::with_capacity(CACHE_BYTES)
    }
}

impl VerifierCache {
    /// An empty cache holding at most `capacity` bytes.
    fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(Entries {
                cards: LruCache::unbounded(),
                bytes: 0,
            }),
            capacity,
        }
    }

    /// The cached Verifier `uid` of `tenant`, marked most recently used.
    ///
    /// # Panics
    /// Panics when the cache lock is poisoned, which no holder can cause
    /// because nothing under it panics.
    pub(crate) fn get(&self, tenant: DataTenantId, uid: &CardUid) -> Option<Arc<CachedVerifier>> {
        let mut entries = self.entries.lock().expect("verifier cache lock");
        entries.cards.get(&(tenant, uid.clone())).map(Arc::clone)
    }

    /// Cache `verifier` as `uid` of `tenant` and return it.
    ///
    /// Evicts least-recently-used entries until the total fits the byte
    /// ceiling; an entry larger than the whole ceiling is returned but not
    /// kept.
    ///
    /// # Panics
    /// Panics when the cache lock is poisoned.
    pub(crate) fn insert(
        &self,
        tenant: DataTenantId,
        uid: CardUid,
        verifier: CachedVerifier,
    ) -> Arc<CachedVerifier> {
        let verifier = Arc::new(verifier);
        let mut entries = self.entries.lock().expect("verifier cache lock");
        entries.bytes += verifier.bytes;
        if let Some((_, replaced)) = entries.cards.push((tenant, uid), Arc::clone(&verifier)) {
            entries.bytes -= replaced.bytes;
        }
        while entries.bytes > self.capacity {
            let Some((_, evicted)) = entries.cards.pop_lru() else {
                break;
            };
            entries.bytes -= evicted.bytes;
        }
        verifier
    }
}

#[cfg(test)]
mod tests {
    use wyrd_spec::DataTenantId;
    use wyrd_spec::card::verifier::VerifierImplementation;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, CardUid, SpaceName};
    use wyrd_spec::reference::CardRef;

    use super::{CachedVerifier, VerifierCache};

    /// A fresh `UUIDv7` Card UID.
    ///
    /// # Panics
    /// Panics when a `UUIDv7` is not a valid Card UID.
    fn uid() -> CardUid {
        CardUid::from_uuid(uuid::Uuid::now_v7()).expect("a UUIDv7 is a valid Card UID")
    }

    /// A parsed Eval Verifier named `name`.
    ///
    /// # Panics
    /// Panics when the static Card identity or spec is invalid.
    fn verifier(name: &str) -> CachedVerifier {
        CachedVerifier::new(
            CardRef {
                kind: CardKind::Verifier,
                name: CardName::new(format!("verifier-{name}")).expect("static name"),
                version: "1.0.0".parse().expect("static version"),
                space: Some(SpaceName::new("default").expect("static space")),
                uid: None,
            },
            serde_json::from_value::<VerifierImplementation>(
                serde_json::json!({ "kind": "eval", "spec": { "tasks": {} } }),
            )
            .expect("static Eval implementation"),
        )
    }

    /// Entries are per tenant, the byte ceiling evicts the least recently
    /// used entry first, and an entry larger than the ceiling is not kept.
    #[test]
    fn entries_are_tenant_scoped_and_evicted_least_recently_used_by_bytes() {
        let size = verifier("a").bytes;
        let cache = VerifierCache::with_capacity(2 * size);
        let (tenant, other) = (DataTenantId::new_v7(), DataTenantId::new_v7());
        let (a, b, c) = (uid(), uid(), uid());
        cache.insert(tenant, a.clone(), verifier("a"));
        assert!(
            cache.get(other, &a).is_none(),
            "never shared across tenants"
        );
        cache.insert(tenant, b.clone(), verifier("b"));
        assert!(cache.get(tenant, &a).is_some(), "a is now most recent");
        cache.insert(tenant, c.clone(), verifier("c"));
        assert!(cache.get(tenant, &b).is_none(), "b was least recently used");
        assert!(cache.get(tenant, &a).is_some() && cache.get(tenant, &c).is_some());

        let tiny = VerifierCache::with_capacity(size - 1);
        assert_eq!(
            tiny.insert(tenant, a.clone(), verifier("a"))
                .reference
                .name
                .as_str(),
            "verifier-a"
        );
        assert!(
            tiny.get(tenant, &a).is_none(),
            "an oversized entry is not kept"
        );
    }
}
