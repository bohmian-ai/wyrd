//! The sustained public ingest and hot-read journey.
//!
//! The focused owners in this binary each prove one seam. This module proves
//! they integrate: four tenants driving both a registered dynamic table and the
//! lazy built-in `vala.traces.spans` through public clients, across adjacent
//! partitions and disjoint key ranges, on a pod whose capacity forces real
//! contention — and reading exactly what they acknowledged at every authority
//! the rows pass through.

use std::sync::Arc;

use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use vala_bifrost_redux::scribe::geometry::ScribeArtifactPolicy;
use vala_bifrost_redux::scribe::routing::SCRIBE_SHARD_COUNT;
use wyrd_spec::DataTenantId;
use wyrd_testing::WyrdTestServer;

use super::support::{
    append_batch, append_values_at, read_sql, register_table, sorted_values, span_batch,
    start_scribe_server_with_admission, tenant_client, unique_table, until_admitted,
};

/// Tenants that drive the journey concurrently.
///
/// Four is the source task's shape: enough that the hierarchical scheduler has
/// to rotate between tenants rather than between tables of one tenant, and few
/// enough that the whole journey stays quick.
const TENANTS: usize = 4;
/// Batches each tenant sends to each of its two tables.
///
/// Eight per table across eight tables is sixty-four public appends with
/// distinct batch identities, which is enough repeated routing for the fixed
/// lane set to be exercised many times over rather than sampled once.
const BATCHES_PER_TABLE: usize = 8;
/// Rows in one batch.
const ROWS_PER_BATCH: i64 = 16;
/// First value one tenant owns, so no two tenants can share a row identity.
const TENANT_KEY_STRIDE: i64 = 100_000;
/// The lazy built-in every tenant writes beside its own dynamic table.
const SYSTEM_TABLE: &str = "vala.traces.spans";

/// One tenant's public participation in the journey.
///
/// Holds the authenticated client, the tenant's own registered table, and the
/// exact row identities it acknowledged on each of its two tables, so the reads
/// at every authority boundary compare against what this tenant actually sent
/// rather than against a recomputed expectation.
struct Participant {
    /// Tenant this participant writes and reads as.
    tenant: DataTenantId,
    /// Authenticated public SDK client bound to [`Self::tenant`].
    client: Arc<wyrd_client::WyrdClient>,
    /// Fully qualified name of this tenant's registered dynamic table.
    dynamic_table: String,
    /// Unqualified dynamic table name, for published-object inspection.
    dynamic_name: String,
    /// Sorted values this tenant acknowledged on its dynamic table.
    expected_dynamic: Vec<i64>,
    /// Sorted values this tenant acknowledged on the built-in spans table.
    expected_system: Vec<i64>,
}

impl Participant {
    /// Reads this tenant's dynamic table back through the public strict route.
    async fn read_dynamic(&self) -> Vec<i64> {
        sorted_values(&self.client, &self.dynamic_table).await
    }

    /// Reads this tenant's built-in spans back through the public strict route.
    ///
    /// The read is deliberately narrow and filtered: it requests one column of
    /// a wide built-in and constrains a second one the result never carries.
    /// That is the signed projection closure `[duration_nano, status_code,
    /// data_tenant_id]` exercised end to end through whichever authority
    /// currently owns the rows — active buckets, staged members, or published
    /// hot objects. Every fixture row carries the canonical `Ok` status code,
    /// so the predicate cannot change which rows come back; a row lost here is
    /// a projection or predicate defect, not a fixture one.
    async fn read_system(&self) -> Vec<i64> {
        let mut values = read_sql(
            &self.client,
            &format!(
                "SELECT duration_nano AS value FROM {SYSTEM_TABLE} \
                 WHERE status_code = 1"
            ),
        )
        .await;
        values.sort_unstable();
        values
    }

    /// Asserts both of this tenant's tables read back exactly what it sent.
    ///
    /// `authority` names the source that currently owns the rows and is quoted
    /// in the failure, so a loss or duplication localizes to the boundary that
    /// introduced it rather than to the journey as a whole.
    ///
    /// # Panics
    ///
    /// Panics when either read returns other than the exact acknowledged rows.
    async fn assert_exact(&self, authority: &str) {
        assert_eq!(
            self.read_dynamic().await,
            self.expected_dynamic,
            "a strict read of the {authority} source must return exactly the rows \
             this tenant acknowledged on its dynamic table"
        );
        assert_eq!(
            self.read_system().await,
            self.expected_system,
            "a strict read of the {authority} source must return exactly the rows \
             this tenant acknowledged on the built-in spans table"
        );
    }
}

/// Sustained four-tenant ingest stays exact through every hot-read authority.
///
/// This is the integration proof for the focused owners in this binary. Four
/// tenants each drive a registered dynamic table and the lazy built-in
/// `vala.traces.spans` through public authenticated clients: disjoint key
/// ranges so no tenant can be credited another's row, two adjacent hour
/// partitions so the physical layout is not a single cell, and sixty-four
/// distinct batch identities so the fixed lane set is routed across repeatedly.
///
/// The pod is started with its Scribe child budget at exactly one complete
/// lifecycle vector. That is what makes the run sustained rather than
/// sequential: with every tenant in flight at once, capacity is genuinely
/// contended, the pod refuses with the stable retryable code, and the shared
/// bounded retry proves those refusals are pressure a real client recovers from
/// rather than lost work. The journey asserts that at least one refusal
/// happened — a run in which nothing was ever refused would prove nothing about
/// pressure — and that every participant was nonetheless admitted.
///
/// Reads then follow the rows through all three authorities: active writable
/// buckets, durable staged members after the freeze, and committed hot objects
/// after publication. Each read is the public strict fused query, so it is the
/// Oracle path answering from whichever source currently owns the rows, and it
/// must not be able to tell which one that was. Tenant isolation is asserted at
/// the same time by construction: an exact comparison against disjoint expected
/// ranges fails if one tenant's read ever carries another's row.
///
/// Terminal reconciliation closes the run: the pod drains its ownership, the
/// retained telemetry owner shows every admission transition it opened also
/// closed, no vector lent, no claim outstanding, and no staged member surviving
/// the objects that replaced it.
///
/// # Panics
///
/// Panics when the pod does not derive a single-vector ceiling, when a public
/// append is refused for any reason other than capacity, when no participant is
/// ever refused, when any read returns other than the exact acknowledged rows,
/// when publication does not account for every row, or when terminal ownership
/// and telemetry do not reconcile.
// Four tenants must actually run at once for the pod's single vector to be
// contended; a current-thread runtime lets a loaded host serialize them and the
// run then observes no refusal at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres and object storage"]
async fn scribe_sustained_ingest_oracle_hot_read_journey() {
    let admission = AdmissionConfig {
        scribe_memory_limit_bytes: Some(
            ScribeArtifactPolicy::default().minimum_scribe_memory_bytes(),
        ),
        ..AdmissionConfig::default()
    };
    let server = start_scribe_server_with_admission(admission).await;
    assert_eq!(
        server
            .scribe_ownership_ceiling_for_test()
            .expect("the pod reports its derived ownership ceiling"),
        1,
        "a sustained run only observes real contention on a pod whose measured \
         capacity completes one table's lifecycle at a time"
    );

    let mut participants = build_participants(&server).await;

    // Every tenant drives both of its tables at once, so the pod's one vector
    // is contended for the whole phase and each refusal is real pressure a
    // public client recovers from.
    let refusals = drive_sustained_ingest(&mut participants).await;
    assert!(
        refusals > 0,
        "a pod lending a single vector to four concurrent tenants must refuse at \
         least once; a run with no refusal proves nothing about pressure or retry"
    );

    // Active authority: the rows live in writable buckets across the fixed lane
    // set and nothing has been staged or published.
    let active = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    assert_eq!(
        active.shard_task_count, SCRIBE_SHARD_COUNT,
        "sustained multi-tenant ingest must not change the fixed lane topology"
    );
    assert!(
        active
            .memory_by_shard
            .iter()
            .filter(|held| **held > 0)
            .count()
            > 1,
        "sixty-four distinct batch identities must route across more than one \
         lane: {:?}",
        active.memory_by_shard
    );
    for participant in &participants {
        participant.assert_exact("active").await;
    }

    // Staged authority: the freeze turns every writable bucket into a durable
    // staged member without publishing anything.
    let scribe = server.bifrost_scribe().expect("the server owns a Scribe");
    scribe
        .flush_writable_for_test()
        .await
        .expect("every writable bucket freezes");
    super::support::await_persistence_drained(&scribe).await;
    let staged = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    let frozen = super::support::journey_buckets(&staged);
    assert!(
        frozen.iter().all(|bucket| bucket.writable_bytes == 0),
        "the freeze must leave no writable bucket owning acknowledged rows; \
         surviving: {frozen:?}"
    );
    assert!(
        server
            .scribe_staging_totals_for_test()
            .expect("the pod's staged and claim totals are inspectable")
            .live_members()
            > 0,
        "the freeze must make at least one member durable"
    );
    for participant in &participants {
        participant.assert_exact("staged").await;
    }

    // Published-hot authority: the staged members become committed objects that
    // account for every acknowledged row exactly once.
    server
        .flush_bifrost()
        .await
        .expect("the staged members publish");
    for participant in &participants {
        let published = server
            .published_hot_files_for_test(
                participant.tenant,
                BifrostNamespace::Datasets.as_str(),
                &participant.dynamic_name,
            )
            .await
            .expect("published hot files are inspectable");
        assert!(
            !published.is_empty(),
            "each tenant's dynamic table must publish at least one hot object"
        );
        let rows: u64 = published.iter().map(|file| file.row_count).sum();
        assert_eq!(
            rows,
            participant.expected_dynamic.len() as u64,
            "the published objects must account for every acknowledged row exactly once"
        );
        participant.assert_exact("published-hot").await;
    }

    // Every read above committed a read decision that the server's audit
    // publisher appends through this same Scribe. Settle that retained history
    // and publish what it wrote, so the pod is drained rather than mid-append.
    for tenant in participants
        .iter()
        .map(|participant| participant.tenant)
        .chain([server.data_tenant_id()])
    {
        server
            .await_audit_published(tenant)
            .await
            .expect("retained audit history settles");
    }
    server
        .flush_bifrost()
        .await
        .expect("the published audit history publishes");
    assert_terminal_reconciliation(&server);
    server.shutdown().await.expect("the server drains cleanly");
}

/// Seeds one tenant, client, dynamic table and built-in table per participant.
///
/// Each tenant owns a disjoint value range, which is what makes the later exact
/// comparisons a tenant-isolation assertion as well as a completeness one.
///
/// # Panics
///
/// Panics when a tenant cannot be seeded or its built-in table provisioned.
async fn build_participants(server: &WyrdTestServer) -> Vec<Participant> {
    let mut participants = Vec::with_capacity(TENANTS);
    for ordinal in 0..TENANTS {
        let tenant = server
            .seed_tenant(&format!("scribe-sustained-{ordinal}"))
            .await
            .expect("the sustained-journey tenant is seeded");
        server
            .ensure_traces_spans_table_for_test(tenant)
            .await
            .expect("the built-in traces table is provisioned");
        let dynamic_name = unique_table("sustained");
        let dynamic_table =
            register_table(server, tenant, BifrostNamespace::Datasets, &dynamic_name).await;
        participants.push(Participant {
            tenant,
            client: Arc::new(tenant_client(server, tenant).await),
            dynamic_table,
            dynamic_name,
            expected_dynamic: Vec::new(),
            expected_system: Vec::new(),
        });
    }
    participants
}

/// Drives every participant's whole demand concurrently and returns the refusals.
///
/// Each tenant runs one bounded task that alternates between its dynamic table
/// and the built-in, placing consecutive batches in two adjacent hour
/// partitions. The tasks rendezvous on a barrier before their first append, so
/// contention for the pod's single vector is structural rather than dependent
/// on scheduling order. Every attempt goes through the shared bounded retry, so
/// the returned total is the number of times the pod applied typed capacity
/// pressure before admitting the work — never a measure of how long anything
/// took.
///
/// # Panics
///
/// Panics when a participant task does not complete, or when any append fails
/// for a reason other than capacity pressure.
async fn drive_sustained_ingest(participants: &mut [Participant]) -> usize {
    let base = super::support::hour_start(chrono::Utc::now());
    // Every participant blocks here until all of them are ready, so the pod's
    // single vector is contended by the first append of every tenant at once
    // rather than whenever the runtime happens to schedule them. Without the
    // barrier a skewed scheduling order can serialize the tenants and the run
    // observes no refusal at all, which proves nothing about pressure.
    let start = Arc::new(tokio::sync::Barrier::new(participants.len()));
    let mut tasks = Vec::with_capacity(participants.len());
    for (ordinal, participant) in participants.iter().enumerate() {
        let client = Arc::clone(&participant.client);
        let dynamic_table = participant.dynamic_table.clone();
        let start = Arc::clone(&start);
        tasks.push(tokio::spawn(async move {
            let mut refusals = 0_usize;
            let mut dynamic = Vec::new();
            let mut system = Vec::new();
            start.wait().await;
            for batch in 0..BATCHES_PER_TABLE {
                let rows = batch_values(ordinal, batch);
                // Consecutive batches alternate between two adjacent hours, so
                // one table's rows span two physical partitions.
                let event_time = base + chrono::Duration::hours((batch % 2) as i64);
                refusals += until_admitted(&format!("tenant {ordinal} dynamic {batch}"), || {
                    append_values_at(
                        &client,
                        &dynamic_table,
                        uuid::Uuid::now_v7(),
                        &rows,
                        event_time,
                    )
                })
                .await;
                dynamic.extend_from_slice(&rows);

                let spans = batch_values(ordinal, batch + BATCHES_PER_TABLE);
                let span_rows = span_batch(&spans);
                refusals += until_admitted(&format!("tenant {ordinal} spans {batch}"), || {
                    append_batch(&client, SYSTEM_TABLE, uuid::Uuid::now_v7(), &span_rows)
                })
                .await;
                system.extend_from_slice(&spans);
            }
            dynamic.sort_unstable();
            system.sort_unstable();
            (refusals, dynamic, system)
        }));
    }

    let mut refusals = 0_usize;
    for (participant, task) in participants.iter_mut().zip(tasks) {
        let (refused, dynamic, system) = task.await.expect("the participant task completes");
        refusals += refused;
        participant.expected_dynamic = dynamic;
        participant.expected_system = system;
    }
    refusals
}

/// Returns the exact values one tenant's batch carries.
///
/// Tenants are separated by [`TENANT_KEY_STRIDE`] and batches by their row
/// count, so every value in the whole run belongs to exactly one tenant, table
/// and batch. That is what lets a read-back comparison detect a row credited to
/// the wrong tenant as well as a lost or duplicated one.
fn batch_values(tenant_ordinal: usize, batch: usize) -> Vec<i64> {
    let base = tenant_ordinal as i64 * TENANT_KEY_STRIDE + batch as i64 * ROWS_PER_BATCH;
    (base..base + ROWS_PER_BATCH).collect()
}

/// Asserts the drained pod's ownership and retained telemetry reconcile.
///
/// Every published object has replaced the members that produced it, so a
/// settled pod must lend no vector, hold no admission transition, own no
/// writable bucket, and have closed every claim it opened. These are the totals
/// of the one retained observation owner checked against the inspected
/// ownership beside them.
///
/// # Panics
///
/// Panics when the totals are not inspectable or any of them does not settle.
fn assert_terminal_reconciliation(server: &WyrdTestServer) {
    let contention = server
        .scribe_contention_totals_for_test()
        .expect("the pod's contention totals are inspectable");
    assert_eq!(
        contention.starts(),
        contention.terminals(),
        "every admission transition the sustained run opened was closed exactly once"
    );
    assert_eq!(
        contention.active_transitions(),
        0,
        "a drained pod holds no admission transition in flight"
    );
    assert_eq!(
        contention.live_vectors(),
        0,
        "a drained pod lends no lifecycle vector"
    );
    let staging = server
        .scribe_staging_totals_for_test()
        .expect("the pod's staged and claim totals are inspectable");
    assert_eq!(
        staging.outstanding_claims(),
        0,
        "a drained pod has no publication in flight"
    );
    let snapshot = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    let owned = super::support::journey_buckets(&snapshot);
    assert!(
        owned.iter().all(|bucket| bucket.writable_bytes == 0),
        "a published pod owns no writable bucket; surviving: {owned:?}"
    );
    assert_eq!(
        snapshot.queued_items, 0,
        "a drained pod holds no accepted append in shard scheduling"
    );
}
