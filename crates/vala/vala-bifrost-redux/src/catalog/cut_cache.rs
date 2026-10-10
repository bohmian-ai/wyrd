//! Node-local cache of Oracle read cuts, invalidated by Postgres notifications.
//!
//! A cut is one table's catalog pointer plus its unresolved hot candidates
//! (`vala.file_list` rows not yet in a snapshot). Every write that changes
//! which rows a cut sees changes `vala.file_list`, and a trigger on that table
//! sends a `bifrost_cut` notification naming the table from inside the
//! committing transaction, so it is delivered only if the write commits.
//! Pointer moves that leave the candidates alone — compaction rewrites,
//! expiry, manifest rewrites — return the same rows through the older pointer,
//! so a cached cut stays a correct answer across them.
//!
//! A cached cut is served only while this node is listening: before the first
//! `LISTEN` and after any lost connection the cache is cleared and every query
//! reads Postgres. A notification is delivered after its commit, so a query
//! racing a publication can see the pre-publication cut for that delivery
//! latency; this is the same best-effort freshness a cut already has between
//! its read and the live scan opening.
//!
//! Snapshot safety comes from the caller's `not_before` bound, not from age:
//! the cached pointer was current when read, and Forge keeps a replaced
//! snapshot for at least the maximum query deadline after it is replaced, so a
//! cut read no earlier than `deadline - MAX_QUERY_DEADLINE` outlives its query.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use sqlx::postgres::PgListener;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::oracle_reader_authority::{CutTableRef, TableCut};
use wyrd_spec::DataTenantId;

/// Postgres channel the `vala.file_list` trigger notifies on.
pub(crate) const CUT_CHANNEL: &str = "bifrost_cut";

/// Pause before reconnecting after the listener fails to connect or listen.
const RECONNECT_DELAY: Duration = Duration::from_secs(1);

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

/// The cache state guarded by one lock, so a fill and an invalidation are
/// ordered by it.
#[derive(Default)]
struct CutEntries {
    /// Cached cuts by table.
    cuts: HashMap<CutKey, CachedCut>,
    /// Bumped by every invalidation; a fill that began under an older value
    /// is discarded.
    generation: u64,
    /// Whether this node is currently listening for invalidations.
    live: bool,
}

/// Node-local cut cache; see the module docs for its consistency rule.
#[derive(Default)]
pub(crate) struct CutCache {
    /// Entries, generation, and listening state.
    entries: RwLock<CutEntries>,
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
        let entries = self.entries.read().ok()?;
        if !entries.live {
            return None;
        }
        let mut cuts = Vec::with_capacity(tables.len());
        for (index, table) in tables.iter().enumerate() {
            if tables[..index].contains(table) {
                return None;
            }
            let cached = entries.cuts.get(&key(tenant, table))?;
            if not_before.is_some_and(|bound| cached.read_at < bound) {
                return None;
            }
            cuts.push(cached.cut.clone());
        }
        Some(cuts)
    }

    /// Returns the generation a database read must start under to be cached,
    /// or `None` when the node is not listening.
    pub(crate) fn fill_generation(&self) -> Option<u64> {
        let entries = self.entries.read().ok()?;
        entries.live.then_some(entries.generation)
    }

    /// Caches `cuts`, read under `generation` and completed at `read_at`,
    /// unless an invalidation or lost connection happened since.
    pub(crate) fn insert(&self, generation: u64, read_at: Instant, cuts: &[TableCut]) {
        let Ok(mut entries) = self.entries.write() else {
            return;
        };
        if !entries.live || entries.generation != generation {
            return;
        }
        for cut in cuts {
            let identity = &cut.identity;
            entries.cuts.insert(
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
    }

    /// Drops the cut a notification names, or every cut when the payload is
    /// not `tenant|namespace|table`.
    fn invalidate(&self, payload: &str) {
        let Ok(mut entries) = self.entries.write() else {
            return;
        };
        entries.generation = entries.generation.wrapping_add(1);
        let mut parts = payload.splitn(3, '|');
        match (
            parts.next().and_then(|tenant| Uuid::parse_str(tenant).ok()),
            parts.next(),
            parts.next(),
        ) {
            (Some(tenant), Some(namespace), Some(table)) => {
                entries
                    .cuts
                    .remove(&(tenant, namespace.to_owned(), table.to_owned()));
            }
            _ => entries.cuts.clear(),
        }
    }

    /// Clears every cut and records whether the node is now listening.
    fn reset(&self, live: bool) {
        if let Ok(mut entries) = self.entries.write() {
            entries.generation = entries.generation.wrapping_add(1);
            entries.cuts.clear();
            entries.live = live;
        }
    }

    /// Listens for cut invalidations on `pool` until `shutdown`.
    ///
    /// Holds one pool connection for the listener. The cache serves only
    /// between a successful `LISTEN` and the next lost connection; each loss
    /// clears it, and sqlx reconnects and re-listens before reporting the
    /// loss, so serving resumes from an empty cache. A failed connect or
    /// `LISTEN` leaves the cache off and retries after [`RECONNECT_DELAY`].
    pub(crate) async fn watch(&self, pool: &sqlx::PgPool, shutdown: CancellationToken) {
        while !shutdown.is_cancelled() {
            let listener = async {
                let mut listener = PgListener::connect_with(pool).await?;
                listener.listen(CUT_CHANNEL).await?;
                Ok::<_, sqlx::Error>(listener)
            };
            let mut listener = match listener.await {
                Ok(listener) => listener,
                Err(error) => {
                    tracing::warn!(%error, "Oracle cut cache listener could not start; cuts read Postgres");
                    tokio::select! {
                        () = shutdown.cancelled() => break,
                        () = tokio::time::sleep(RECONNECT_DELAY) => continue,
                    }
                }
            };
            self.reset(true);
            loop {
                let received = tokio::select! {
                    () = shutdown.cancelled() => break,
                    received = listener.try_recv() => received,
                };
                match received {
                    Ok(Some(notification)) => self.invalidate(notification.payload()),
                    Ok(None) => {
                        tracing::warn!("Oracle cut cache listener reconnected; cache cleared");
                        self.reset(true);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Oracle cut cache listener failed; cuts read Postgres");
                        break;
                    }
                }
            }
            self.reset(false);
        }
        self.reset(false);
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
    use super::*;
    use vala_sql::row_types::oracle_reader_authority::TableAuthorityIdentity;

    /// Builds a cut for `table` of `tenant` with no hot candidates.
    fn cut(tenant: DataTenantId, table: &str) -> TableCut {
        TableCut {
            identity: TableAuthorityIdentity {
                tenant,
                table_uid: [1; 16],
                catalog_name: crate::catalog::BIFROST_CATALOG_NAME.to_owned(),
                namespace_name: "vala.events".to_owned(),
                table_name: table.to_owned(),
            },
            metadata_location: format!("s3://w/{table}/v1.metadata.json"),
            hot_files: Vec::new(),
        }
    }

    /// Proves the cache serves only while listening, only cuts read at or
    /// after the bound, drops a fill that raced an invalidation, and drops
    /// exactly the table a notification names.
    ///
    /// # Panics
    /// Panics when any serve, miss, or invalidation assertion fails.
    #[test]
    fn cut_cache_serves_only_fresh_listened_cuts() {
        let cache = CutCache::default();
        let tenant = DataTenantId::new(Uuid::now_v7()).expect("tenant");
        let events = CutTableRef {
            namespace_name: "vala.events",
            table_name: "events",
        };
        let other = CutTableRef {
            namespace_name: "vala.events",
            table_name: "other",
        };
        let start = Instant::now();

        assert!(cache.fill_generation().is_none(), "not listening: no fill");
        cache.reset(true);
        let generation = cache.fill_generation().expect("listening");
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

        cache.invalidate(&format!("{}|vala.events|events", tenant.as_uuid()));
        assert!(
            cache.get(tenant, &[events], None).is_none(),
            "named table dropped"
        );
        assert!(
            cache.get(tenant, &[other], None).is_some(),
            "other table kept"
        );

        let stale = cache.fill_generation().expect("listening");
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

        let generation = cache.fill_generation().expect("listening");
        cache.insert(generation, start, &[cut(tenant, "events")]);
        cache.reset(false);
        assert!(
            cache.get(tenant, &[events], None).is_none(),
            "lost listener clears"
        );
    }
}
