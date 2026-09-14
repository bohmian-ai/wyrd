//! The pod's fixed topology, its one shared budget, and whose turn it is.
//!
//! These are four separate claims and this file keeps them in four owners. The
//! topology and accounting owner runs on a production-shaped pod, because that
//! is the pod whose lane count and memory identities are being asserted. The
//! three fairness owners run on a pod whose measured capacity completes exactly
//! one table at a time, because a scheduling decision is not observable on a
//! pod that can serve every contender at once. Combining them would mean
//! asserting production accounting against a deliberately starved pod and
//! reading each scenario's state through the residue of the last.

use std::sync::{Arc, Mutex};

use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::admission::{AdmissionConfig, GLOBAL_INFLIGHT_ITEMS};
use vala_bifrost_redux::scribe::geometry::ScribeArtifactPolicy;
use vala_bifrost_redux::scribe::routing::{SCRIBE_SHARD_COUNT, shard_for};
use wyrd_spec::DataTenantId;

use super::support::{
    INGEST_BUSY, append_until_admitted, append_values, register_table, sorted_values,
    start_scribe_server, start_scribe_server_with_admission, tenant_client, unique_table,
    until_admitted,
};

/// Tenants that are concurrently resident on the topology pod.
const TENANTS: usize = 4;
/// Tables each resident tenant registers and writes.
const TABLES_PER_TENANT: usize = 2;
/// Rows in one topology batch.
const ROWS_PER_BATCH: usize = 64;
/// Lanes each resident table is deliberately routed onto.
///
/// Four tenants with two tables each is eight owners, and the pod has sixteen
/// lanes, so two lanes per table covers every lane exactly once with sixteen
/// appends. Coverage is chosen rather than sampled: the batch identities are
/// searched against the production routing function until each one lands on the
/// lane it was assigned, so an idle lane means the topology is narrower than
/// sixteen and never means the case was unlucky.
const LANES_PER_TABLE: usize = SCRIBE_SHARD_COUNT / (TENANTS * TABLES_PER_TENANT);
/// Tables one tenant rotates through the pod's single lifecycle vector.
///
/// Seventeen is deliberately one more than the fixed lane count: a scheduler
/// that quietly bound table turns to lanes would leave exactly one table
/// without a turn, and this case would see it as a table that never completed.
const ROTATION_TABLES: usize = 17;
/// Equal-demand tenants that contend for the same single vector.
const ROTATION_TENANTS: usize = 10;
/// Rows one rotation batch carries.
///
/// The rotation cases prove whose turn it is, not how much anyone can hold, so
/// the payload stays small and every refusal is a scheduling decision rather
/// than a byte ceiling.
const ROTATION_ROWS: usize = 32;
/// Batches each rotation participant sends.
///
/// Three rounds of demand is what makes continued rotation observable. One
/// round proves only that everyone was admitted once, which a pod that then
/// served its tables serially would also satisfy.
const ROTATION_BATCHES: usize = 3;
/// Consecutive turns one participant may take while peers are demonstrably
/// contending.
///
/// One: a released vector may not return to the participant that just held it
/// while other participants are asking. This is measured, not inherited. Two
/// was tried first, on the precedent of the system-and-dynamic owner, and is
/// too generous here — it admits a pod that serves one polite opening round
/// and then hands every participant two turns back to back forever, which
/// starves all but the participant being served. The production pod holds to
/// one because sixteen looping peers keep recorded demand in front of it at
/// every release, so `reserved_ahead` forbids the reacquisition. A turn
/// granted with fewer than [`MIN_LIVE_CONTENDERS`] live peers is not measured
/// at all, which is what keeps this bound from asserting perfect alternation
/// across asynchronous client loops.
const MAX_CONSECUTIVE_TURNS: usize = 1;
/// Peers that must have a public append in flight for a turn to be measured.
///
/// A participant between a refusal and its next attempt is not a contender the
/// scheduler can see, so a turn granted while nobody else is asking proves
/// nothing about rotation. Requiring two live peers means every measured turn
/// was granted against real, simultaneous demand.
const MIN_LIVE_CONTENDERS: usize = 2;
/// Turns one contended interval must contain before its bound means anything.
///
/// A bound satisfied over two or three turns is not evidence of rotation, so
/// the run must contain at least one full round of contended turns for every
/// participant. Without this an owner could pass by never contending at all.
const MIN_MEASURED_TURNS: usize = ROTATION_TABLES;

/// Returns the admission configuration that starves the pod to one vector.
///
/// Setting Scribe's child budget to the smallest budget its own policy calls
/// coherent drives the derived ownership ceiling to one: the pod can complete
/// exactly one table's lifecycle at a time. Nothing else is moved, so the
/// scheduler under test is the production scheduler making real decisions
/// under real pressure rather than a scaled imitation of one.
fn single_vector_admission() -> AdmissionConfig {
    AdmissionConfig {
        scribe_memory_limit_bytes: Some(
            ScribeArtifactPolicy::default().minimum_scribe_memory_bytes(),
        ),
        ..AdmissionConfig::default()
    }
}

/// Starts a pod that can complete exactly one table's lifecycle at a time.
///
/// # Panics
///
/// Panics when the pod's measured capacity does not derive a ceiling of one,
/// which would make every fairness assertion in this file vacuous.
async fn start_single_vector_pod() -> wyrd_testing::WyrdTestServer {
    let server = start_scribe_server_with_admission(single_vector_admission()).await;
    assert_eq!(
        server
            .scribe_ownership_ceiling_for_test()
            .expect("the pod reports its derived ownership ceiling"),
        1,
        "a fairness case only observes scheduling decisions on a pod whose \
         measured capacity completes exactly one table at a time"
    );
    server
}

/// Sixteen fixed lanes share one bounded pod budget that is lent, not divided.
///
/// Scribe's topology is a fixed constant, not a function of how many tenants or
/// tables the pod happens to be serving, and its memory is one pod budget that
/// every lane and every tenant draws from. The failures this owner guards
/// against are silent ones: a pod that grows a lane or a WAL stream per tenant,
/// a lane that is declared but never carries work, accounting that does not add
/// up across lanes and buckets, or a tenant that keeps a share its neighbours
/// were owed. Every one of them still returns correct rows.
///
/// The pod runs on production admission. This owner asserts what a real pod's
/// topology and accounting look like, so starving it would be asserting against
/// a pod no deployment runs.
///
/// Lane coverage is constructed rather than sampled. Routing is a public,
/// deterministic function of `(tenant, table, batch_id)`, so each table's batch
/// identities are searched until they land on the two lanes that table was
/// assigned, and the sixteen appends between them cover all sixteen lanes
/// exactly once. The identities still go through the ordinary public ingest
/// route; nothing here places a row on a lane directly.
///
/// # Panics
///
/// Panics when the topology is not exactly sixteen lanes, when a lane carries
/// no work, when WAL streams or queued work exceed their per-pod ceilings, when
/// accounted memory disagrees with the per-shard or per-bucket totals, when
/// equal demand produces lopsided tenant ownership, when publication leaves a
/// bucket behind, or when a tenant does not read back exactly what it
/// acknowledged.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_sixteen_shards_obey_global_and_tenant_budgets() {
    let server = start_scribe_server().await;

    let mut tenants: Vec<DataTenantId> = vec![server.data_tenant_id()];
    for ordinal in 1..TENANTS {
        tenants.push(
            server
                .seed_tenant(&format!("scribe-budget-{ordinal}"))
                .await
                .expect("the resident tenant is seeded"),
        );
    }

    let mut residents = Vec::with_capacity(TENANTS);
    for tenant in &tenants {
        let client = tenant_client(&server, *tenant).await;
        let mut tables = Vec::with_capacity(TABLES_PER_TENANT);
        for table_ordinal in 0..TABLES_PER_TENANT {
            let name = unique_table(&format!("budget_{table_ordinal}"));
            let table = register_table(&server, *tenant, BifrostNamespace::Datasets, &name).await;
            tables.push((name, table));
        }
        residents.push((*tenant, client, tables));
    }

    // Every lane is assigned to exactly one (tenant, table) owner, so the
    // appends below cover the whole topology with one batch per lane.
    let mut expected: Vec<Vec<i64>> = vec![Vec::new(); TENANTS];
    let mut covered: Vec<bool> = vec![false; SCRIBE_SHARD_COUNT];
    for (index, (tenant, client, tables)) in residents.iter().enumerate() {
        for (table_ordinal, (name, table)) in tables.iter().enumerate() {
            let owner = index * TABLES_PER_TENANT + table_ordinal;
            for slot in 0..LANES_PER_TABLE {
                let lane = owner * LANES_PER_TABLE + slot;
                let batch_id = batch_id_routing_to(*tenant, name, lane);
                let first = ((owner * LANES_PER_TABLE + slot) * ROWS_PER_BATCH) as i64;
                let rows: Vec<i64> = (first..first + ROWS_PER_BATCH as i64).collect();
                append_values(client, table, batch_id, &rows)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("tenant {index} lane {lane} is acknowledged: {error:?}")
                    });
                expected[index].extend_from_slice(&rows);
                covered[lane] = true;
            }
        }
    }
    assert!(
        covered.iter().all(|hit| *hit),
        "the case must route a batch onto every lane it asserts about; missed {:?}",
        covered
            .iter()
            .enumerate()
            .filter(|(_, hit)| !**hit)
            .map(|(lane, _)| lane)
            .collect::<Vec<_>>()
    );

    let loaded = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");

    assert_eq!(
        loaded.shard_task_count, SCRIBE_SHARD_COUNT,
        "the pod must own exactly sixteen shard tasks whatever it is serving"
    );
    assert_eq!(
        loaded.shard_channel_count, SCRIBE_SHARD_COUNT,
        "the pod must own exactly sixteen shard channels whatever it is serving"
    );
    assert!(
        loaded.open_wal_stream_count <= SCRIBE_SHARD_COUNT,
        "WAL streams are per lane, not per tenant or table: {} open for {TENANTS} tenants",
        loaded.open_wal_stream_count
    );
    assert!(
        loaded.queued_items <= GLOBAL_INFLIGHT_ITEMS,
        "accepted work must stay inside the pod in-flight ceiling: {} queued",
        loaded.queued_items
    );

    let by_shard: usize = loaded.memory_by_shard.iter().sum();
    let by_bucket: usize = loaded
        .memory_by_bucket
        .iter()
        .map(|bucket| bucket.writable_bytes + bucket.immutable_bytes)
        .sum();
    assert_eq!(
        by_shard, by_bucket,
        "per-lane ownership {by_shard} must account for exactly the buckets {by_bucket}"
    );
    assert!(
        by_bucket > 0 && by_bucket <= loaded.total_accounted_memory,
        "bucket ownership {by_bucket} must be live and inside the accounted pod total {}",
        loaded.total_accounted_memory
    );
    assert!(
        loaded.scribe_used_memory <= loaded.scribe_memory_limit,
        "Scribe's reservation {} exceeded its child ceiling {}",
        loaded.scribe_used_memory,
        loaded.scribe_memory_limit
    );
    assert!(
        loaded.ingress_used_memory <= loaded.ingress_memory_limit,
        "ingress occupancy {} exceeded its ceiling {}",
        loaded.ingress_used_memory,
        loaded.ingress_memory_limit
    );
    assert!(
        loaded.parent_used_memory <= loaded.parent_memory_limit,
        "Bifrost reservation {} exceeded the parent ceiling {}",
        loaded.parent_used_memory,
        loaded.parent_memory_limit
    );

    let idle: Vec<usize> = (0..SCRIBE_SHARD_COUNT)
        .filter(|shard| loaded.memory_by_shard[*shard] == 0)
        .collect();
    assert!(
        idle.is_empty(),
        "every fixed lane must carry admitted work; idle lanes {idle:?} of {:?}",
        loaded.memory_by_shard
    );

    let mut owned = vec![0_usize; TENANTS];
    for bucket in &loaded.memory_by_bucket {
        if let Some(index) = tenants
            .iter()
            .position(|tenant| *tenant == bucket.seal_key.tenant)
        {
            owned[index] += bucket.writable_bytes + bucket.immutable_bytes;
        }
    }
    let smallest = owned.iter().copied().min().unwrap_or_default();
    let largest = owned.iter().copied().max().unwrap_or_default();
    assert!(
        smallest > 0,
        "every resident tenant must own the rows it was acknowledged for: {owned:?}"
    );
    assert!(
        largest <= smallest.saturating_mul(2),
        "equal demand must not produce lopsided tenant ownership: {owned:?}"
    );

    server
        .flush_bifrost()
        .await
        .expect("the staged members publish");
    let settled = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    let owned = super::support::journey_buckets(&settled);
    assert!(
        owned.is_empty(),
        "publication must leave no bucket owning published rows; surviving: {owned:?}"
    );
    assert_eq!(
        settled.shard_task_count, SCRIBE_SHARD_COUNT,
        "the fixed topology must survive publication unchanged"
    );

    for (index, (_, client, tables)) in residents.iter().enumerate() {
        let mut read_back = Vec::new();
        for (_, table) in tables {
            read_back.extend(sorted_values(client, table).await);
        }
        read_back.sort_unstable();
        let mut mine = expected[index].clone();
        mine.sort_unstable();
        assert_eq!(
            read_back, mine,
            "tenant {index} must read back exactly the rows it acknowledged"
        );
    }

    server.shutdown().await.expect("the server drains cleanly");
}

/// A queued contender is served before the incumbent can take the vector back.
///
/// This is the scheduling decision that fairness reduces to. When the pod is
/// lending its whole capacity to one owner and a second owner has already been
/// refused, releasing that capacity must hand it to the one that waited. A pod
/// that instead let the incumbent reacquire would still acknowledge every row
/// and still refuse under pressure; the only difference is that the contender
/// never makes progress. Nothing about that is visible in a row count.
///
/// Contention is placed with the production ingest barrier rather than with
/// timing: the barrier holds one already-admitted request at the seam where it
/// owns its reserve and has not yet touched the WAL or a shard mailbox, so the
/// competitor's refusal is observed against a real resident owner.
///
/// The case ends by driving the incumbent's own append to completion. Its
/// refused attempt left a demand record behind, which is exactly what holds the
/// vector for it, and abandoning that record would leave the pod holding a
/// demand for a caller that is never coming back.
///
/// # Panics
///
/// Panics when the pod does not derive a single-vector ceiling, when the
/// uncontended tenant is not lent the whole vector, when the contender is not
/// refused with the stable retryable refusal or leaves durable state behind,
/// when the released vector goes back to the incumbent ahead of the waiting
/// contender, or when either tenant does not read back exactly what it
/// acknowledged.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_waiting_contender_precedes_incumbent_reacquisition() {
    let server = start_single_vector_pod().await;

    let incumbent = server.data_tenant_id();
    let contender = server
        .seed_tenant("scribe-budget-contender")
        .await
        .expect("the contending tenant is seeded");
    let incumbent_client = Arc::new(tenant_client(&server, incumbent).await);
    let contender_client = tenant_client(&server, contender).await;
    let incumbent_table = register_table(
        &server,
        incumbent,
        BifrostNamespace::Datasets,
        &unique_table("budget_incumbent"),
    )
    .await;
    let contender_table = register_table(
        &server,
        contender,
        BifrostNamespace::Datasets,
        &unique_table("budget_contender"),
    )
    .await;

    // 1. One tenant borrows the pod's whole idle vector. Nothing is held back
    //    for a tenant that is not there: the ceiling is one and the live count
    //    reaches it, so the entire measured capacity is lent to one owner.
    let held: Vec<i64> = (0..ROTATION_ROWS as i64).collect();
    let stall = server
        .stall_next_bifrost_write()
        .expect("the production ingest barrier is installable");
    let borrowed = tokio::spawn({
        let client = Arc::clone(&incumbent_client);
        let table = incumbent_table.clone();
        let rows = held.clone();
        async move { append_values(&client, &table, Uuid::now_v7(), &rows).await }
    });
    stall.wait_entered().await;
    let lent = server
        .scribe_contention_totals_for_test()
        .expect("the contention registry is readable");
    assert_eq!(
        lent.live_vectors(),
        1,
        "an uncontended tenant must be lent the pod's whole idle vector, not a \
         reserve computed for tenants that are not writing"
    );

    // 2. A second tenant under real pressure is refused before it can mutate
    //    anything durable, and the refusal is the stable retryable one.
    let refusal = append_values(&contender_client, &contender_table, Uuid::now_v7(), &held)
        .await
        .expect_err("a pod already lending its only vector must refuse the contender");
    assert_eq!(
        refusal.code(),
        INGEST_BUSY,
        "capacity pressure must reach the caller as the stable retryable refusal: {refusal:?}"
    );
    let refused = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    assert!(
        refused
            .memory_by_bucket
            .iter()
            .all(|bucket| bucket.seal_key.tenant != contender),
        "a refused request must own no bucket: refusal happens before WAL or \
         mailbox mutation, not after"
    );
    assert!(
        sorted_values(&contender_client, &contender_table)
            .await
            .is_empty(),
        "a refused request must leave no readable row behind"
    );

    // 3. Releasing the incumbent releases exactly one vector, and the vector
    //    goes to the tenant that waited rather than back to the incumbent.
    stall.release();
    borrowed
        .await
        .expect("the barrier-held request task completes")
        .expect("the barrier-held request is acknowledged once released");
    let released = server
        .scribe_contention_totals_for_test()
        .expect("the contention registry is readable");
    assert_eq!(
        released.live_vectors(),
        0,
        "completing the held request must release exactly the one vector it held"
    );
    let second: Vec<i64> = (ROTATION_ROWS as i64..2 * ROTATION_ROWS as i64).collect();
    let reacquisition = append_values(&incumbent_client, &incumbent_table, Uuid::now_v7(), &second)
        .await
        .expect_err("the incumbent must not reacquire the vector a contender is owed");
    assert_eq!(
        reacquisition.code(),
        INGEST_BUSY,
        "an incumbent held behind a queued contender is backpressure, not a fault: {reacquisition:?}"
    );
    append_values(&contender_client, &contender_table, Uuid::now_v7(), &held)
        .await
        .expect("the contender that waited must be admitted on the released vector");

    // 4. The incumbent's refusal queued a demand record on its behalf. Driving
    //    that append to completion is what retires it: leaving it outstanding
    //    would leave the pod holding capacity for a caller that never returns.
    let incumbent_refusals = append_until_admitted(
        &incumbent_client,
        &incumbent_table,
        &second,
        "the incumbent's queued demand",
    )
    .await;
    assert!(
        incumbent_refusals < usize::MAX,
        "the incumbent's own queued demand must eventually be served"
    );

    let mut incumbent_rows = held.clone();
    incumbent_rows.extend_from_slice(&second);
    incumbent_rows.sort_unstable();
    assert_eq!(
        sorted_values(&incumbent_client, &incumbent_table).await,
        incumbent_rows,
        "the incumbent must read back exactly the rows it was acknowledged for"
    );
    assert_eq!(
        sorted_values(&contender_client, &contender_table).await,
        held,
        "the contender must read back exactly the rows it was acknowledged for"
    );

    let settled = server
        .scribe_contention_totals_for_test()
        .expect("the contention registry is readable");
    assert_eq!(
        settled.live_vectors(),
        0,
        "the case must leave the pod holding no lifecycle vector"
    );

    server.shutdown().await.expect("the server drains cleanly");
}

/// Seventeen tables of one tenant all get a turn on the single vector.
///
/// One more table than the pod has lanes, on purpose: a scheduler that quietly
/// bound a table's turn to a lane would leave exactly one table without one,
/// and this case sees that as a table that never completed its demand. The
/// tenant is the same throughout, so nothing here can be explained by tenant
/// fairness; the only question is whether the pod rotates between the tables of
/// one owner.
///
/// # Panics
///
/// Panics when the pod does not derive a single-vector ceiling, when the tables
/// never actually contend, when any table is refused past the shared admission
/// deadline, when a table is acknowledged a different number of times than it
/// declared, when one table holds the vector for more consecutive turns than
/// the bound allows while peers are demonstrably asking, when no contended
/// stretch was long enough to measure that bound, or when a table does not read
/// back exactly the rows its turns acknowledged.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_seventeen_tables_rotate_without_starvation() {
    let server = start_single_vector_pod().await;
    let tenant = server.data_tenant_id();
    let client = Arc::new(tenant_client(&server, tenant).await);

    let mut tables = Vec::with_capacity(ROTATION_TABLES);
    for ordinal in 0..ROTATION_TABLES {
        tables.push(
            register_table(
                &server,
                tenant,
                BifrostNamespace::Datasets,
                &unique_table(&format!("budget_rotation_{ordinal}")),
            )
            .await,
        );
    }

    let ledger = Arc::new(Mutex::new(RotationLedger::default()));
    let mut turns = Vec::with_capacity(ROTATION_TABLES);
    for (ordinal, table) in tables.iter().enumerate() {
        turns.push(tokio::spawn(rotation_participant(
            Arc::clone(&client),
            table.clone(),
            ordinal,
            0,
            Arc::clone(&ledger),
        )));
    }
    let mut contested = 0_usize;
    for turn in turns {
        if turn.await.expect("table rotation task completes") > 0 {
            contested += 1;
        }
    }
    assert!(
        contested > 0,
        "seventeen tables sharing one vector must actually contend; none was refused"
    );

    // Progress alone is not the claim, and neither is participation. A pod
    // that gave every table one turn and then served them one at a time to
    // completion would satisfy "everybody started before anybody finished"
    // while starving sixteen tables for the rest of the run. What the pod owes
    // its tables is that it keeps rotating for as long as they keep asking, so
    // that is what is measured — against the demand the scheduler could
    // actually see at each turn.
    let recorded = ledger
        .lock()
        .expect("the rotation ledger is readable")
        .turns
        .clone();
    let order: Vec<usize> = recorded.iter().map(|turn| turn.participant).collect();
    for ordinal in 0..ROTATION_TABLES {
        assert_eq!(
            order.iter().filter(|table| **table == ordinal).count(),
            ROTATION_BATCHES,
            "table {ordinal} must be acknowledged exactly once per declared batch: {order:?}"
        );
    }
    assert_bounded_rotation_under_live_contention(&recorded);

    for (ordinal, table) in tables.iter().enumerate() {
        assert_eq!(
            sorted_values(&client, table).await,
            rotation_expected(ordinal),
            "table {ordinal} must read back exactly the rows its turns acknowledged"
        );
    }

    server.shutdown().await.expect("the server drains cleanly");
}

/// Ten equal-demand tenants take turns, and arriving first buys no privilege.
///
/// Each tenant asks for exactly the same work, so any tenant that finishes
/// while another is still being refused is holding a share it was not owed.
/// The pod starts clean: a tenant that inherited an outstanding demand from an
/// earlier scenario would be measured against a queue it never joined.
///
/// First-arrival privilege is the failure this case is shaped around, so the
/// arrival order is made definite rather than left to task scheduling. Tenant
/// zero takes the pod's only vector and is held there, and every other tenant
/// is then refused once, which is what puts a queued demand record on the
/// scheduler in a known order. Only then is the incumbent released. Because
/// all ten have demonstrably arrived before any turn is given, "tenant zero
/// finished its demand while a tenant that was already waiting had not been
/// admitted once" is a scheduling verdict rather than a race between the
/// tasks that issue the appends.
///
/// # Panics
///
/// Panics when the pod does not derive a single-vector ceiling, when a queued
/// tenant is refused for anything but capacity pressure, when the tenants never
/// actually contend with each other, when the first arrival's last turn is
/// given before every waiting tenant has been acknowledged once, when any
/// tenant is refused past the shared admission deadline, or when a tenant does
/// not read back exactly the rows its turns acknowledged.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_ten_tenants_rotate_fairly() {
    let server = start_single_vector_pod().await;

    let mut equals = Vec::with_capacity(ROTATION_TENANTS);
    for ordinal in 0..ROTATION_TENANTS {
        let tenant = server
            .seed_tenant(&format!("scribe-budget-equal-{ordinal}"))
            .await
            .expect("the equal-demand tenant is seeded");
        let client = Arc::new(tenant_client(&server, tenant).await);
        let table = register_table(
            &server,
            tenant,
            BifrostNamespace::Datasets,
            &unique_table("budget_equal"),
        )
        .await;
        equals.push((client, table));
    }

    // Tenant zero arrives first and is held inside the pod's only vector, so
    // every other tenant meets a pod that is genuinely out of capacity.
    let stall = server
        .stall_next_bifrost_write()
        .expect("the production ingest barrier is installable");
    let (incumbent_client, incumbent_table) = &equals[0];
    let incumbent_first = tokio::spawn({
        let client = Arc::clone(incumbent_client);
        let table = incumbent_table.clone();
        let rows = rotation_batch(0, 0);
        async move { append_values(&client, &table, Uuid::now_v7(), &rows).await }
    });
    stall.wait_entered().await;

    // Each waiting tenant is refused exactly once. The refusal is what queues
    // its demand, so after this loop the scheduler holds nine waiting tenants
    // in a known arrival order behind one incumbent.
    for (ordinal, (client, table)) in equals.iter().enumerate().skip(1) {
        let refusal = append_values(client, table, Uuid::now_v7(), &rotation_batch(ordinal, 0))
            .await
            .expect_err("a pod already lending its only vector must refuse a later arrival");
        assert_eq!(
            refusal.code(),
            INGEST_BUSY,
            "capacity pressure must reach tenant {ordinal} as the stable retryable \
             refusal: {refusal:?}"
        );
    }

    stall.release();
    // The released incumbent is not entitled to finish: it re-enters admission
    // holding no priority over the nine tenants that queued while it was
    // stalled, so a refusal here is the anti-reacquisition rule working and its
    // first batch simply becomes part of the demand it drives below.
    let incumbent_first_admitted = incumbent_first
        .await
        .expect("the incumbent's first append completes")
        .is_ok();

    // Every tenant now drives its whole demand through the shared bounded
    // retry, and each acknowledgement is recorded in the order the pod gave it.
    let ledger = Arc::new(Mutex::new(RotationLedger::default()));
    let mut demands = Vec::with_capacity(ROTATION_TENANTS);
    for (ordinal, (client, table)) in equals.iter().enumerate() {
        demands.push(tokio::spawn(rotation_participant(
            Arc::clone(client),
            table.clone(),
            ordinal,
            // Tenant zero resumes at its second batch only if the append it
            // held inside the barrier was actually acknowledged.
            usize::from(ordinal == 0 && incumbent_first_admitted),
            Arc::clone(&ledger),
        )));
    }
    let mut contested = 0_usize;
    for demand in demands {
        if demand.await.expect("tenant rotation task completes") > 0 {
            contested += 1;
        }
    }
    assert!(
        contested > 1,
        "ten equal-demand tenants sharing one vector must contend with each \
         other; only {contested} was ever refused"
    );

    let order = ledger
        .lock()
        .expect("the rotation ledger is readable")
        .participants();
    let incumbent_completed = order
        .iter()
        .rposition(|tenant| *tenant == 0)
        .expect("the first arrival completes its declared demand");
    for ordinal in 1..ROTATION_TENANTS {
        let first_turn = order
            .iter()
            .position(|tenant| *tenant == ordinal)
            .expect("every waiting tenant is admitted");
        assert!(
            first_turn < incumbent_completed,
            "tenant {ordinal} was already queued when the vector was released, so \
             the tenant that arrived first may not finish its demand before that \
             tenant is admitted once: {order:?}"
        );
    }

    for (ordinal, (client, table)) in equals.iter().enumerate() {
        assert_eq!(
            sorted_values(client, table).await,
            rotation_expected(ordinal),
            "tenant {ordinal} must read back exactly the rows its turns acknowledged"
        );
    }

    server.shutdown().await.expect("the server drains cleanly");
}

/// Finds a batch identity that public ingest will route onto `lane`.
///
/// Routing is a public, deterministic hash of the authenticated tenant, the
/// canonical table FQN, and the batch id, so a case that needs a specific lane
/// covered can search the one input it owns until the function agrees. The
/// identity returned is an ordinary valid batch id and is sent through the
/// ordinary public route; nothing here places a row on a lane directly or
/// depends on the hash staying the same across builds.
///
/// # Panics
///
/// Panics when no identity in the searched space routes to `lane`, which would
/// mean the routing function no longer spreads across the declared topology.
fn batch_id_routing_to(tenant: DataTenantId, table_name: &str, lane: usize) -> Uuid {
    let table = TableRef::new(BifrostNamespace::Datasets, table_name);
    for nonce in 0_u64..4_096 {
        let mut bytes = [0_u8; 16];
        bytes[..8].copy_from_slice(&(lane as u64).to_be_bytes());
        bytes[8..].copy_from_slice(&nonce.to_be_bytes());
        bytes[6] = 0x70 | (bytes[6] & 0x0f);
        bytes[8] = 0x80 | (bytes[8] & 0x3f);
        let candidate = Uuid::from_bytes(bytes);
        if shard_for(tenant, &table, candidate) == lane {
            return candidate;
        }
    }
    panic!("no batch identity in the searched space routes {table_name} onto lane {lane}");
}

/// Sends one rotation participant's whole demand and reports what it cost.
///
/// Every participant sends the same number of batches with the same row count,
/// so the returned refusal count is the only thing that can differ between them
/// and is therefore the measure of whose turn the scheduler kept giving away.
/// The retry policy is the shared bounded one, so a participant's attempt rate
/// is not an input to the measurement.
///
/// `first_batch` lets a participant resume a demand it has already partly sent,
/// which is what a case that seeds the queue with a real incumbent needs.
/// Every attempt is registered in `ledger` for the whole time it is on the
/// wire, so the ledger records each acknowledgement together with the peers
/// that were demonstrably asking alongside it. A case can then judge whose turn
/// came when, and against how much live demand, instead of only how many
/// refusals it took.
///
/// # Panics
///
/// Panics when a batch is refused for anything other than capacity pressure, or
/// when a participant is still being refused at the shared admission deadline.
async fn rotation_participant(
    client: Arc<wyrd_client::WyrdClient>,
    table: String,
    ordinal: usize,
    first_batch: usize,
    ledger: Arc<Mutex<RotationLedger>>,
) -> usize {
    let mut refusals = 0_usize;
    for batch in first_batch..ROTATION_BATCHES {
        let rows = rotation_batch(ordinal, batch);
        let label = format!("participant {ordinal} batch {batch}");
        refusals += until_admitted(&label, || async {
            ledger
                .lock()
                .expect("the rotation ledger is writable")
                .enter(ordinal);
            let outcome = append_values(&client, &table, Uuid::now_v7(), &rows).await;
            ledger
                .lock()
                .expect("the rotation ledger is writable")
                .leave(ordinal, outcome.is_ok());
            outcome
        })
        .await;
    }
    refusals
}

/// One acknowledged turn, together with the contention that was live when the
/// pod granted it.
///
/// Rotation can only be judged against demand the scheduler could actually
/// see. Recording the contending peers beside the turn is what lets the owner
/// measure the pod where it was choosing between askers and stay silent where
/// it was not.
#[derive(Debug, Clone, Copy)]
struct RotationTurn {
    /// The participant the pod admitted.
    participant: usize,
    /// Peers that had their own public append in flight at that moment.
    contending_peers: usize,
}

/// The live-demand ledger the rotation owners measure the pod against.
///
/// Participants drive asynchronous client loops, so "still has batches to
/// send" is not the same as "is asking right now": between a refusal and the
/// next attempt a participant is invisible to the scheduler. This ledger
/// tracks the requests actually outstanding, so each recorded turn carries the
/// contention that existed when the pod chose.
#[derive(Debug, Default)]
struct RotationLedger {
    /// Participants with a public append outstanding right now.
    in_flight: std::collections::BTreeSet<usize>,
    /// Acknowledged turns, in the order the pod granted them.
    turns: Vec<RotationTurn>,
}

impl RotationLedger {
    /// Records that `participant` has put one public append on the wire.
    fn enter(&mut self, participant: usize) {
        self.in_flight.insert(participant);
    }

    /// Retires `participant`'s outstanding append, recording a turn when the
    /// pod admitted it.
    ///
    /// The contending-peer count is taken before the participant is removed,
    /// so it names the peers that were asking alongside it rather than after
    /// it withdrew.
    fn leave(&mut self, participant: usize, admitted: bool) {
        if admitted {
            self.turns.push(RotationTurn {
                participant,
                contending_peers: self.in_flight.len().saturating_sub(1),
            });
        }
        self.in_flight.remove(&participant);
    }

    /// Returns the acknowledged participants in the order the pod admitted them.
    fn participants(&self) -> Vec<usize> {
        self.turns.iter().map(|turn| turn.participant).collect()
    }
}

/// Asserts the pod kept rotating for as long as its participants kept asking.
///
/// Only turns granted while at least [`MIN_LIVE_CONTENDERS`] peers had a
/// public append in flight are measured: elsewhere the pod had no one to
/// rotate to, and holding it to a rotation bound there would assert perfect
/// alternation between asynchronous client loops rather than the contract.
/// Within each contended interval no participant may hold the vector for more
/// than [`MAX_CONSECUTIVE_TURNS`] turns in a row, and one interval must run at
/// least [`MIN_MEASURED_TURNS`] turns so the bound is asserted over a real
/// contended stretch rather than a lucky pair.
///
/// # Panics
///
/// Panics when a participant exceeds the consecutive-turn bound under live
/// contention, or when no contended interval was long enough to measure.
fn assert_bounded_rotation_under_live_contention(turns: &[RotationTurn]) {
    let mut longest_interval = 0_usize;
    let mut interval = 0_usize;
    let mut run = 0_usize;
    let mut previous: Option<usize> = None;
    for (index, turn) in turns.iter().enumerate() {
        if turn.contending_peers < MIN_LIVE_CONTENDERS {
            interval = 0;
            run = 0;
            previous = None;
            continue;
        }
        interval += 1;
        longest_interval = longest_interval.max(interval);
        run = if previous == Some(turn.participant) {
            run + 1
        } else {
            1
        };
        previous = Some(turn.participant);
        assert!(
            run <= MAX_CONSECUTIVE_TURNS,
            "participant {} took {run} consecutive turns at index {index} while \
             {} peers had a public append in flight; a released vector may not \
             return to its incumbent ahead of recorded contenders: {turns:?}",
            turn.participant,
            turn.contending_peers
        );
    }
    assert!(
        longest_interval >= MIN_MEASURED_TURNS,
        "the rotation bound must be asserted over a contended stretch of at \
         least {MIN_MEASURED_TURNS} turns; the longest run of turns granted \
         against {MIN_LIVE_CONTENDERS} or more live peers was \
         {longest_interval}: {turns:?}"
    );
}

/// A pod that serves its participants serially after one opening round is
/// rejected, though every participant did start before any of them finished.
///
/// This is the failure the previous formulation of this owner could not see.
/// "Nobody finishes before everybody starts" is satisfied by one polite
/// opening round followed by strictly serial service, which starves every
/// participant but the one being served for the rest of the run. The bounded
/// claim measures what the pod owes its participants — that it keeps rotating
/// while they keep asking — so it rejects the same order.
///
/// # Panics
///
/// Panics when the serial order is not accepted by the superseded start/finish
/// condition, or when the bounded claim fails to reject it.
#[test]
fn serial_service_after_one_round_is_rejected() {
    let mut turns: Vec<RotationTurn> = (0..ROTATION_TABLES)
        .map(|participant| RotationTurn {
            participant,
            contending_peers: ROTATION_TABLES - 1,
        })
        .collect();
    for participant in 0..ROTATION_TABLES {
        for _ in 1..ROTATION_BATCHES {
            turns.push(RotationTurn {
                participant,
                contending_peers: ROTATION_TABLES - 1,
            });
        }
    }
    let order: Vec<usize> = turns.iter().map(|turn| turn.participant).collect();

    // The superseded condition accepts this order: the last participant to
    // start did so in the opening round, before any participant finished.
    let latest_start = (0..ROTATION_TABLES)
        .map(|participant| {
            order
                .iter()
                .position(|taken| *taken == participant)
                .expect("every participant starts")
        })
        .max()
        .expect("participants declare turns");
    let earliest_finish = (0..ROTATION_TABLES)
        .map(|participant| {
            order
                .iter()
                .rposition(|taken| *taken == participant)
                .expect("every participant finishes")
        })
        .min()
        .expect("participants declare turns");
    assert!(
        latest_start < earliest_finish,
        "the superseded condition must accept this order, or it proves nothing \
         about what the bounded claim adds: {order:?}"
    );

    let rejected = std::panic::catch_unwind(|| {
        assert_bounded_rotation_under_live_contention(&turns);
    });
    assert!(
        rejected.is_err(),
        "serial service under live contention must be rejected: {order:?}"
    );
}

/// Returns the rows one rotation participant sends in one batch.
///
/// Values are unique per participant so a read-back can only match when the
/// rows that participant acknowledged are the rows its own table holds.
fn rotation_batch(ordinal: usize, batch: usize) -> Vec<i64> {
    let first = ((ordinal * ROTATION_BATCHES + batch) * ROTATION_ROWS) as i64;
    (first..first + ROTATION_ROWS as i64).collect()
}

/// Returns every row one rotation participant is expected to read back.
fn rotation_expected(ordinal: usize) -> Vec<i64> {
    let mut expected: Vec<i64> = (0..ROTATION_BATCHES)
        .flat_map(|batch| rotation_batch(ordinal, batch))
        .collect();
    expected.sort_unstable();
    expected
}
