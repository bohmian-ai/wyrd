//! Tier-2 proof of the Scribe live-tail source lifetime and its Forge boundary.
//!
//! The scenario runs the real Scribe that sealed the fixture's hot objects, so
//! a live-tail lease is taken over the same writer state a production Oracle
//! reads: unsealed rows still in the memtable, alongside committed hot objects
//! that only `vala.file_list` protects. What it proves is the ownership rule
//! Forge maintenance depends on — Scribe keeps a leased source readable for the
//! lease lifetime and settles its owner exactly once on every terminal path,
//! while the lease itself names no Forge-collectable object and therefore adds
//! no Forge GC root at all.

use std::time::{Duration, Instant};

use vala_bifrost_redux::catalog::layout::{TimeGranularity, TimePartition};
use vala_bifrost_redux::scribe::tail_rpc::{
    ScribeTailReader, TailOwnershipSnapshot, TailReadError,
};
use wyrd_spec::vala::api::{
    AcquireTailFenceRequest, SchemaFingerprint, TailCursor, TailPageRequest, TenantTableBinding,
};

use crate::forge::support::{PromotionIntegrationFixture, fixture_day};

/// Builds the fixture table's daily live-tail partition.
///
/// # Panics
///
/// Panics when the fixture day is not a canonical daily boundary.
fn fixture_partition() -> TimePartition {
    TimePartition::new(
        TimeGranularity::Day,
        fixture_day()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time")
            .and_utc(),
    )
    .expect("the fixture day is a daily partition boundary")
}

/// Asserts the two refusals that never reach a lease settle their owners once.
///
/// An elapsed deadline is refused before any owner exists, so nothing can be
/// released. A schema mismatch is refused after the pre-material guard already
/// owns root capacity, so that guard is the one that must settle it — once.
///
/// # Panics
///
/// Panics when either refusal acquires an owner it does not settle.
async fn assert_refused_acquisitions_settle_once(
    reader: &ScribeTailReader,
    template: &AcquireTailFenceRequest,
) -> TailOwnershipSnapshot {
    let snapshot = || {
        reader
            .ownership_snapshot_for_test()
            .expect("refusal ownership snapshot")
    };
    let mut expired = template.clone();
    expired.deadline = chrono::Utc::now() - chrono::Duration::seconds(1);
    assert!(matches!(
        reader.acquire_fence(expired).await,
        Err(TailReadError::DeadlineElapsed)
    ));
    assert_eq!(snapshot().reservations, 0);
    assert_eq!(snapshot().releases, 0);

    let mut mismatched = template.clone();
    mismatched.schema_fingerprint =
        SchemaFingerprint::new("0".repeat(64)).expect("mismatched fingerprint");
    assert!(matches!(
        reader.acquire_fence(mismatched).await,
        Err(TailReadError::SchemaMismatch)
    ));
    let after_error = snapshot();
    assert_eq!(after_error.reservations, 1);
    assert_eq!(after_error.releases, 1);
    assert_eq!(after_error.active_reservations, 0);
    after_error
}

/// Asserts one successful lease stays readable and releases exactly once.
///
/// The page read is the point: the Scribe-local Arrow the fence admitted is
/// still owned while the lease is held. Release then settles that owner, and a
/// duplicate release must not settle it again.
///
/// # Panics
///
/// Panics when the leased source is unreadable, when release does not settle
/// the owner, or when a duplicate release settles it twice.
async fn assert_lease_is_readable_and_releases_once(
    reader: &ScribeTailReader,
    request: AcquireTailFenceRequest,
    before: TailOwnershipSnapshot,
) -> TailOwnershipSnapshot {
    let snapshot = || {
        reader
            .ownership_snapshot_for_test()
            .expect("lease ownership snapshot")
    };
    let fence = reader
        .acquire_fence(request)
        .await
        .expect("a live-tail fence over the unsealed rows");
    assert_eq!(snapshot().active_reservations, 1);
    let page = reader
        .read_page(&TailPageRequest {
            query_id: uuid::Uuid::nil(),
            fence_id: fence.fence_id,
            after: None,
            max_rows: 64,
            max_encoded_bytes: 1 << 20,
        })
        .expect("the leased source is readable while the lease is held");
    assert!(
        page.batches.iter().any(|batch| batch.num_rows() > 0),
        "the lease still owns the Arrow it admitted"
    );
    assert!(
        reader
            .release_fence(fence.fence_id)
            .expect("explicit release")
            .released
    );
    let after = snapshot();
    assert_eq!(after.releases, before.releases + 1);
    assert_eq!(after.active_reservations, 0);
    reader
        .release_fence(fence.fence_id)
        .expect("duplicate release is idempotent");
    assert_eq!(
        snapshot().releases,
        after.releases,
        "a released live-tail owner is never settled a second time"
    );
    after
}

/// A live-tail lease keeps its Scribe source readable and releases it once.
///
/// The committed hot objects the fixture sealed are the Forge roots here: they
/// carry no promotion evidence yet, so `vala.file_list` alone protects them.
/// Around that durable state the scenario drives every terminal live-tail path
/// — deadline refusal, failed acquisition, successful lease and release,
/// duplicate release, lease expiry, and reader drop — and requires that the
/// number of settled owners equals the number acquired and that no owner
/// settles twice. The
/// durable `vala.file_list` roster is compared across the whole lifetime: a
/// live-tail lease that introduced a Forge root would have changed it.
///
/// # Panics
///
/// Panics when a leased source is unreadable, when a terminal path leaks or
/// double-releases an owner, or when the live tail changes the Forge roster.
#[tokio::test]
async fn scribe_sources_live_for_lease_and_release_once() {
    let fixture = PromotionIntegrationFixture::start("live_tail_lifetime").await;
    let roots_before = fixture.file_rows().await;
    assert!(
        roots_before
            .iter()
            .all(|row| row.committed_snapshot_id.is_none()),
        "committed hot objects without promotion evidence are the Forge roots: {roots_before:?}"
    );

    // Unsealed rows are the state this tier can put under a lease against the
    // real Scribe; the frozen immutable and staged source states are read under
    // their leases by the tier-3 proofs in `scribe::tail_rpc` and
    // `scribe::hot_source`.
    fixture.append_without_seal(7).await;
    let reader = fixture
        .scribe()
        .tail_reader()
        .expect("the production Scribe builds its live-tail reader");
    let fingerprint = fixture.schema_fingerprint().await;
    let partition = fixture_partition();
    // The wire binding names the domain segment of the logical namespace, which
    // is what a live-tail caller resolves against.
    let namespace = fixture
        .binding
        .logical_namespace
        .split('.')
        .next_back()
        .expect("a logical namespace has a domain segment")
        .to_owned();
    let template = AcquireTailFenceRequest {
        query_id: uuid::Uuid::nil(),
        binding: TenantTableBinding {
            tenant_id: fixture.tenant,
            namespace: namespace.clone(),
            table: fixture.binding.table_name.clone(),
        },
        time_partition: partition.to_wire(),
        exclusive_sealed: TailCursor {
            writer_epoch: 1,
            wal_lsn: 0,
            batch_id: uuid::Uuid::nil(),
            row_ordinal: 0,
        },
        deadline: chrono::Utc::now() + chrono::Duration::seconds(5),
        schema_fingerprint: fingerprint,
        tail_protocol_version: 1,
    };
    let snapshot = || {
        reader
            .ownership_snapshot_for_test()
            .expect("live-tail ownership snapshot")
    };

    let after_error = assert_refused_acquisitions_settle_once(&reader, &template).await;
    let after_success =
        assert_lease_is_readable_and_releases_once(&reader, template.clone(), after_error).await;

    // Expiry: the lease deadline settles the retained owner exactly once.
    reader
        .acquire_fence(template.clone())
        .await
        .expect("an expiring live-tail fence");
    let due = Instant::now() + Duration::from_mins(1);
    assert_eq!(reader.expire_due(due, 64).released, 1);
    assert_eq!(reader.expire_due(due, 64).released, 0);
    let after_expiry = snapshot();
    assert_eq!(after_expiry.releases, after_success.releases + 1);
    assert_eq!(after_expiry.active_reservations, 0);

    // Drop: reader destruction settles the one owner still retained.
    reader
        .acquire_fence(template)
        .await
        .expect("a fence retained across reader destruction");
    assert_eq!(snapshot().active_reservations, 1);
    drop(reader);

    assert_eq!(
        fixture.file_rows().await,
        roots_before,
        "a live-tail lease names no Forge-collectable object, so it adds no root"
    );
    assert_eq!(
        fixture.forge_tasks().await.len(),
        0,
        "a live-tail lease enqueues no Forge maintenance of any kind"
    );
}
