//! Ordered admission of `bifrost.metrics.series` ahead of its points.
//!
//! Bifrost admits one table per batch, and every batch has its own durable
//! commit, so a points batch and its series rows cannot share one. Scribe
//! orders them instead: the series rows a points batch needs are admitted,
//! and durable, before the points are dispatched, so a durable point always
//! has its series row for its hour. A series admission that fails fails the
//! points admission; the writer retries the whole batch and a repeated series
//! row is allowed.

use std::collections::HashSet;
use std::sync::Mutex;

use sha2::{Digest, Sha256};
use uuid::Uuid;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::managed_columns::{
    ACT_PRINCIPAL_ID, RUN_ID, SUB_PRINCIPAL_ID, SUBJECT_CARD_UID, WORKLOAD_CARD_UID,
    WYRD_REQUEST_ID,
};

use crate::catalog::TableRef;
use crate::contracts::{
    CanonicalIngress, IngressPayload, ScribeError, ScribeIngressFrame, StampedAttribution,
};
use crate::namespaces::BifrostNamespace;
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::ScribeImpl;
use crate::scribe::preprocess::{PreparedAppend, PreparedSliceSet};
use crate::tables::metrics::series::{SeriesKey, series_rows};
use crate::tables::{DomainTable, PointsTable, SeriesTable};

/// Series keys one process remembers before it forgets them all.
const SEEN_CAPACITY: usize = 262_144;

/// The series rows this process has made durable, per tenant, hour and
/// context.
///
/// A points batch whose series are all here admits no series batch. An entry
/// joins only after its row is durable, so the set never hides a missing row;
/// forgetting one, on overflow or restart, only writes that row again.
pub(crate) struct SeriesSeen {
    /// Durable series rows, keyed by tenant, hour, series id, and context.
    entries: Mutex<HashSet<(DataTenantId, SeriesKey)>>,
    /// Entries held before the set is cleared.
    capacity: usize,
}

impl SeriesSeen {
    /// An empty set that holds at most `capacity` entries.
    #[must_use]
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(HashSet::new()),
            capacity,
        }
    }

    /// Whether `tenant`'s row for `key` is known to be durable.
    ///
    /// A poisoned lock answers `false`, which only writes the row again.
    fn has(&self, tenant: DataTenantId, key: SeriesKey) -> bool {
        self.entries
            .lock()
            .is_ok_and(|entries| entries.contains(&(tenant, key)))
    }

    /// Record `tenant`'s rows for `keys` as durable.
    ///
    /// A set that would pass its capacity is cleared first, and at most
    /// `capacity` keys join it.
    fn remember(&self, tenant: DataTenantId, keys: &[SeriesKey]) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        // ponytail: forgets every series at the bound; keep the hottest
        // (an LRU) if re-writing series after an overflow ever costs too much.
        if entries.len() + keys.len() > self.capacity {
            entries.clear();
        }
        entries.extend(keys.iter().take(self.capacity).map(|key| (tenant, *key)));
    }
}

impl Default for SeriesSeen {
    /// A set bounded at [`SEEN_CAPACITY`] entries.
    fn default() -> Self {
        Self::new(SEEN_CAPACITY)
    }
}

/// The frame a points batch's series rows are admitted under.
///
/// It is the points frame's principal, attribution, workload, Card scope,
/// dimensions, and request, naming `bifrost.metrics.series`; its rows and
/// batch id are set once the points are prepared.
pub(super) struct SeriesFrame(ScribeIngressFrame);

impl SeriesFrame {
    /// The series frame of `points`, `None` unless it writes
    /// `bifrost.metrics.points`.
    pub(super) fn for_points(points: &ScribeIngressFrame) -> Option<Self> {
        (points.table.namespace == BifrostNamespace::Metrics
            && points.table.name == PointsTable::NAME)
            .then(|| {
                Self(ScribeIngressFrame {
                    principal: points.principal.clone(),
                    attribution: points.attribution,
                    workload: points.workload.clone(),
                    authenticated_tenant: points.authenticated_tenant,
                    attributed_cards: points.attributed_cards.clone(),
                    dimensions: points.dimensions.clone(),
                    table: TableRef::new(BifrostNamespace::Metrics, SeriesTable::NAME),
                    expected_schema_fingerprint: Some(SchemaFingerprint(
                        SeriesTable::schema_fingerprint(),
                    )),
                    table_version: None,
                    request_id: points.request_id.clone(),
                    batch_id: points.batch_id,
                    measured_wire_bytes: 0,
                    payload: IngressPayload::Canonical(CanonicalIngress::unreserved(Vec::new())),
                })
            })
    }
}

/// The batch id of the series rows `keys` derived from points batch `points`.
///
/// The same points batch needing the same rows, contexts included, always
/// reaches the same id, so a retry deduplicates; needing other rows is
/// another batch.
fn series_batch_id(points: Uuid, keys: &[SeriesKey]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"bifrost.metrics.series.batch-id.v2");
    digest.update(points.as_bytes());
    for (hour, id, context) in keys {
        digest.update(hour.to_be_bytes());
        digest.update(id);
        digest.update(context);
    }
    let digest: [u8; 32] = digest.finalize().into();
    let mut bytes: [u8; 16] = digest[..16].try_into().expect("sixteen digest bytes");
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

impl ScribeImpl {
    /// Make the series rows `points` needs durable, before `points` is
    /// dispatched.
    ///
    /// The rows are the first point of each series, hour and context in
    /// `points` that this process has not already made durable. With none, nothing is
    /// admitted and no sync is spent. Otherwise they are admitted as one
    /// canonical `bifrost.metrics.series` batch through the ordinary ingress
    /// path, and their keys join [`SeriesSeen`] once it is durable.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::ContractViolation`] when a prepared slice cannot
    /// yield its series rows, and any failure of the series admission, which
    /// the caller returns instead of dispatching the points.
    pub(super) async fn admit_series_first(
        &self,
        series: SeriesFrame,
        points: &PreparedAppend,
    ) -> Result<(), ScribeError> {
        let SeriesFrame(mut frame) = series;
        let tenant = points.tenant;
        let carried: &[&str] = match frame.attribution {
            StampedAttribution::Frame(_) => &[SUBJECT_CARD_UID, RUN_ID],
            StampedAttribution::Rows => &[
                SUBJECT_CARD_UID,
                RUN_ID,
                WORKLOAD_CARD_UID,
                SUB_PRINCIPAL_ID,
                ACT_PRINCIPAL_ID,
                WYRD_REQUEST_ID,
            ],
        };
        let PreparedSliceSet::Materialized(slices) = &points.slices;
        let mut admitted = HashSet::new();
        let (mut batches, mut keys) = (Vec::new(), Vec::new());
        for slice in slices {
            if let Some((rows, added)) = series_rows(&slice.rows, carried, |key| {
                !self.series_seen.has(tenant, key) && admitted.insert(key)
            })
            .map_err(ScribeError::ContractViolation)?
            {
                batches.push(rows);
                keys.extend(added);
            }
        }
        if batches.is_empty() {
            return Ok(());
        }
        frame.batch_id = series_batch_id(points.batch_id, &keys);
        frame.payload = IngressPayload::Canonical(CanonicalIngress::unreserved(batches));
        Box::pin(self.prepare_and_dispatch(frame)).await?;
        self.series_seen.remember(tenant, &keys);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A remembered series is seen, per tenant, hour and context; overflow
    /// forgets every series, so the next batch writes its row again rather
    /// than losing it.
    ///
    /// # Panics
    ///
    /// Panics when a remembered key is not seen, another tenant, hour or
    /// context sees it, or a key survives an overflow.
    #[test]
    fn the_seen_set_suppresses_repeats_up_to_its_bound() {
        let seen = SeriesSeen::new(2);
        let tenant = DataTenantId::new_v7();
        let key = |n: u8| (0, [n; 16], [0; 16]);
        seen.remember(tenant, &[key(1), key(2)]);
        assert!(seen.has(tenant, key(1)) && seen.has(tenant, key(2)));
        assert!(
            !seen.has(DataTenantId::new_v7(), key(1)),
            "the set is per tenant"
        );
        assert!(
            !seen.has(tenant, (1, [1; 16], [0; 16])),
            "the set is per hour"
        );
        assert!(
            !seen.has(tenant, (0, [1; 16], [1; 16])),
            "the set is per context"
        );

        seen.remember(tenant, &[key(3)]);
        assert!(seen.has(tenant, key(3)));
        assert!(
            !seen.has(tenant, key(1)) && !seen.has(tenant, key(2)),
            "an overflow forgets, so those rows are only written again"
        );
    }

    /// The series batch id is fixed by the points batch and the rows it needs.
    ///
    /// # Panics
    ///
    /// Panics when the same inputs reach another id, or other rows or
    /// contexts the same id.
    #[test]
    fn the_series_batch_id_follows_its_points_batch_and_rows() {
        let points = Uuid::now_v7();
        let rows = [(0, [1; 16], [0; 16]), (0, [2; 16], [0; 16])];
        assert_eq!(
            series_batch_id(points, &rows),
            series_batch_id(points, &rows)
        );
        assert_ne!(
            series_batch_id(points, &rows),
            series_batch_id(points, &rows[..1])
        );
        assert_ne!(
            series_batch_id(points, &rows),
            series_batch_id(Uuid::now_v7(), &rows)
        );
        assert_ne!(
            series_batch_id(points, &rows),
            series_batch_id(points, &[rows[0], (0, [2; 16], [1; 16])])
        );
    }
}
