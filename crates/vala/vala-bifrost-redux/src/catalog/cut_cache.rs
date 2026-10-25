//! Node-local cache of Oracle read cuts, invalidated by Postgres notifications.
//!
//! A cut is one table's catalog pointer plus its unresolved hot candidates
//! (`vala.file_list` rows not yet in a snapshot). Every write that changes
//! which rows a cut sees changes `vala.file_list`, and a trigger on that table
//! sends a `bifrost_cut` notification naming the table from inside the
//! committing transaction, so it is delivered only if the write commits.
//! A Forge rewrite moves the pointer without touching `vala.file_list`, so its
//! terminal transition sends the same notification: the older pointer would
//! still answer correctly, but would keep reading the replaced files. Expiry
//! and manifest rewrites send nothing; they change neither rows nor files.
//!
//! A cached cut is served only while this node is listening: before the first
//! `LISTEN` and after any lost or silently stalled connection the cache is
//! cleared and every query reads Postgres. A notification is delivered after its commit, so a query
//! racing a publication can see the pre-publication cut for that delivery
//! latency. Notifications only keep the cache fresh; they never validate a
//! read. Oracle validates each served cut against the publication revision
//! every captured Scribe reports, and [`CutCache::evict`]s a cut a Scribe
//! proves stale before reading it again.
//!
//! Snapshot safety comes from the caller's `not_before` bound, not from age:
//! the cached pointer was current when read, and Forge keeps a replaced
//! snapshot for at least the maximum query deadline after it is replaced, so a
//! cut read no earlier than `deadline - MAX_QUERY_DEADLINE` outlives its query.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Instant;

use uuid::Uuid;
use vala_sql::queries::oracle_reader_authority::{CutTableRef, TableCut};
use wyrd_spec::DataTenantId;

/// Postgres channel the `vala.file_list` trigger notifies on.
pub(crate) const CUT_CHANNEL: &str = "bifrost_cut";

/// Cache key: tenant, logical namespace, table name.
type CutKey = (Uuid, String, String);

/// One table's cut and when it was read.
#[derive(Clone)]
struct CachedCut {
    /// The cut exactly as Postgres returned it.
    cut: TableCut,
    /// When the statement that returned it completed.
    read_at: Instant,
}

/// Entries whose fills are fenced by invalidations and by listener liveness.
///
/// One lock orders every fill against every invalidation: a reader captures
/// [`Self::fill_generation`] before its awaited load and [`Self::fill`]s
/// under it afterwards, and any [`Self::invalidate`] or [`Self::reset`] in
/// between bumps the generation so the older result is discarded. Nothing is
/// served or filled while the node is not listening, so a missed
/// notification cannot leave an entry current; callers then read their
/// authority uncached. A poisoned lock behaves as not listening.
#[derive(Default)]
pub(crate) struct ListenedCache<T> {
    /// Entries, generation, and listening state.
    state: RwLock<Listened<T>>,
}

/// The state [`ListenedCache`] guards with one lock.
#[derive(Default)]
struct Listened<T> {
    /// Cached entries.
    entries: T,
    /// Bumped by every invalidation; a fill that began under an older value
    /// is discarded.
    generation: u64,
    /// Whether this node is currently listening for invalidations.
    live: bool,
}

impl<T: Default> ListenedCache<T> {
    /// Answers `read` from the entries, or `None` when the node is not
    /// listening.
    pub(crate) fn get<R>(&self, read: impl FnOnce(&T) -> Option<R>) -> Option<R> {
        let state = self.state.read().ok()?;
        if !state.live {
            return None;
        }
        read(&state.entries)
    }

    /// Returns the generation an awaited load must start under to be cached,
    /// or `None` when the node is not listening.
    pub(crate) fn fill_generation(&self) -> Option<u64> {
        let state = self.state.read().ok()?;
        state.live.then_some(state.generation)
    }

    /// Applies `fill` for a load that began under `generation`, unless the
    /// load began while not listening or an invalidation or reset happened
    /// since.
    pub(crate) fn fill(&self, generation: Option<u64>, fill: impl FnOnce(&mut T)) {
        let Ok(mut state) = self.state.write() else {
            return;
        };
        if state.live && Some(state.generation) == generation {
            fill(&mut state.entries);
        }
    }

    /// Applies `invalidate` and bumps the generation, so a load that began
    /// before it cannot refill what it dropped.
    pub(crate) fn invalidate(&self, invalidate: impl FnOnce(&mut T)) {
        if let Ok(mut state) = self.state.write() {
            state.generation = state.generation.wrapping_add(1);
            invalidate(&mut state.entries);
        }
    }

    /// Clears every entry and records whether the node is now listening.
    pub(crate) fn reset(&self, live: bool) {
        if let Ok(mut state) = self.state.write() {
            state.generation = state.generation.wrapping_add(1);
            state.entries = T::default();
            state.live = live;
        }
    }
}

/// Node-local cut cache; see the module docs for its consistency rule.
#[derive(Default)]
pub(crate) struct CutCache {
    /// Cached cuts by table, fenced by the listener lifecycle.
    cuts: ListenedCache<HashMap<CutKey, CachedCut>>,
}

impl CutCache {
    /// Returns every requested table's cached cut read at or after
    /// `not_before` (any read when `None`), in request order, or `None` when
    /// any is missing, too old, or the node is not listening.
    ///
    /// A request naming a table twice always misses, so the database keeps
    /// its one-result-per-distinct-table rule.
    pub(crate) fn get(
        &self,
        tenant: DataTenantId,
        tables: &[CutTableRef<'_>],
        not_before: Option<Instant>,
    ) -> Option<Vec<TableCut>> {
        self.cuts.get(|cuts| {
            let mut found = Vec::with_capacity(tables.len());
            for (index, table) in tables.iter().enumerate() {
                if tables[..index].contains(table) {
                    return None;
                }
                let cached = cuts.get(&key(tenant, table))?;
                if not_before.is_some_and(|bound| cached.read_at < bound) {
                    return None;
                }
                found.push(cached.cut.clone());
            }
            Some(found)
        })
    }

    /// Returns the generation a database read must start under to be cached,
    /// or `None` when the node is not listening.
    pub(crate) fn fill_generation(&self) -> Option<u64> {
        self.cuts.fill_generation()
    }

    /// Caches `cuts`, read under `generation` and completed at `read_at`,
    /// unless the read began while not listening or an invalidation or lost
    /// connection happened since.
    pub(crate) fn insert(&self, generation: Option<u64>, read_at: Instant, cuts: &[TableCut]) {
        self.cuts.fill(generation, |entries| {
            for cut in cuts {
                let identity = &cut.identity;
                entries.insert(
                    (
                        identity.tenant.as_uuid(),
                        identity.namespace_name.clone(),
                        identity.table_name.clone(),
                    ),
                    CachedCut {
                        cut: cut.clone(),
                        read_at,
                    },
                );
            }
        });
    }

    /// Drops one table's cut, which Oracle found older than a Scribe's
    /// reported publication revision.
    ///
    /// Bumps the generation exactly as a notification does, so a fill that
    /// began before the eviction cannot reinsert the stale cut.
    pub(crate) fn evict(&self, tenant: DataTenantId, table: &CutTableRef<'_>) {
        self.cuts.invalidate(|cuts| {
            cuts.remove(&key(tenant, table));
        });
    }

    /// Drops the cut a notification names, or every cut when the payload is
    /// not `tenant|namespace|table`.
    pub(crate) fn invalidate(&self, payload: &str) {
        self.cuts.invalidate(|cuts| {
            let mut parts = payload.splitn(3, '|');
            match (
                parts.next().and_then(|tenant| Uuid::parse_str(tenant).ok()),
                parts.next(),
                parts.next(),
            ) {
                (Some(tenant), Some(namespace), Some(table)) => {
                    cuts.remove(&(tenant, namespace.to_owned(), table.to_owned()));
                }
                _ => cuts.clear(),
            }
        });
    }

    /// Clears every cut and records whether the node is now listening.
    pub(crate) fn reset(&self, live: bool) {
        self.cuts.reset(live);
    }
}

/// Builds the cache key of one requested table.
fn key(tenant: DataTenantId, table: &CutTableRef<'_>) -> CutKey {
    (
        tenant.as_uuid(),
        table.namespace_name.to_owned(),
        table.table_name.to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use vala_sql::row_types::oracle_reader_authority::TableAuthorityIdentity;

    /// Builds a cut for `table` of `tenant` with no hot candidates.
    fn cut(tenant: DataTenantId, table: &str) -> TableCut {
        TableCut {
            identity: TableAuthorityIdentity {
                tenant,
                table_uid: [1; 16],
                catalog_name: crate::catalog::BIFROST_CATALOG_NAME.to_owned(),
                namespace_name: "bifrost.events".to_owned(),
                table_name: table.to_owned(),
            },
            metadata_location: format!("s3://w/{table}/v1.metadata.json"),
            publication_revisions: std::collections::HashMap::new(),
            hot_files: Vec::new(),
        }
    }

    /// Proves the cache serves only while listening, only cuts read at or
    /// after the bound, drops a fill that raced an invalidation, eviction, or
    /// reconnect or began while not listening, and drops exactly the table a
    /// notification or eviction names.
    ///
    /// # Panics
    /// Panics when any serve, miss, or invalidation assertion fails.
    #[test]
    fn cut_cache_serves_only_fresh_listened_cuts() {
        let cache = CutCache::default();
        let tenant = DataTenantId::new(Uuid::now_v7()).expect("tenant");
        let events = CutTableRef {
            namespace_name: "bifrost.events",
            table_name: "events",
        };
        let other = CutTableRef {
            namespace_name: "bifrost.events",
            table_name: "other",
        };
        let start = Instant::now();

        assert!(cache.fill_generation().is_none(), "not listening: no fill");
        cache.reset(true);
        let generation = cache.fill_generation();
        cache.insert(
            generation,
            start,
            &[cut(tenant, "events"), cut(tenant, "other")],
        );
        assert_eq!(
            cache
                .get(tenant, &[events, other], Some(start))
                .map(|c| c.len()),
            Some(2)
        );
        assert!(
            cache.get(tenant, &[events, events], None).is_none(),
            "repeated table"
        );
        assert!(
            cache
                .get(tenant, &[events], Some(start + Duration::from_secs(1)))
                .is_none(),
            "read before the bound"
        );

        cache.invalidate(&format!("{}|bifrost.events|events", tenant.as_uuid()));
        assert!(
            cache.get(tenant, &[events], None).is_none(),
            "named table dropped"
        );
        assert!(
            cache.get(tenant, &[other], None).is_some(),
            "other table kept"
        );

        let stale = cache.fill_generation();
        cache.invalidate("malformed");
        cache.insert(stale, start, &[cut(tenant, "events")]);
        assert!(
            cache.get(tenant, &[events], None).is_none(),
            "raced fill dropped"
        );
        assert!(
            cache.get(tenant, &[other], None).is_none(),
            "malformed clears all"
        );

        let generation = cache.fill_generation();
        cache.insert(generation, start, &[cut(tenant, "events")]);
        let raced = cache.fill_generation();
        cache.evict(tenant, &events);
        assert!(
            cache.get(tenant, &[events], None).is_none(),
            "evicted table dropped"
        );
        cache.insert(raced, start, &[cut(tenant, "events")]);
        assert!(
            cache.get(tenant, &[events], None).is_none(),
            "a fill that raced an eviction is dropped"
        );

        let generation = cache.fill_generation();
        cache.insert(generation, start, &[cut(tenant, "events")]);
        cache.reset(false);
        assert!(
            cache.get(tenant, &[events], None).is_none(),
            "lost listener clears"
        );

        let disconnected = cache.fill_generation();
        cache.insert(disconnected, start, &[cut(tenant, "events")]);
        cache.reset(true);
        cache.insert(disconnected, start, &[cut(tenant, "events")]);
        assert!(
            cache.get(tenant, &[events], None).is_none(),
            "a read that began while not listening is never cached"
        );
        let raced = cache.fill_generation();
        cache.reset(true);
        cache.insert(raced, start, &[cut(tenant, "events")]);
        assert!(
            cache.get(tenant, &[events], None).is_none(),
            "a fill that raced a reconnect is dropped"
        );
    }
}
