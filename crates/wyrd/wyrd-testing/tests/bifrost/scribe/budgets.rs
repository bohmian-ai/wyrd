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
const ROTATION_BATCHES: usize = 3;

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
    assert_eq!(
        settled.writable_bucket_count, 0,
        "publication must leave no writable bucket owning published rows"
    );
    assert_eq!(
        settled.immutable_bucket_count, 0,
        "publication must leave no immutable bucket owning published rows"
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
/// declared, when one table completes its whole demand before every peer has
/// taken a turn, or when a table does not read back exactly the rows its turns
/// acknowledged.
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

    let order = Arc::new(Mutex::new(Vec::<usize>::new()));
    let mut turns = Vec::with_capacity(ROTATION_TABLES);
    for (ordinal, table) in tables.iter().enumerate() {
        turns.push(tokio::spawn(rotation_participant(
            Arc::clone(&client),
            table.clone(),
            ordinal,
            0,
            Arc::clone(&order),
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

    // Progress alone is not the claim. A pod that served one table to
    // completion, then the next, would also finish every table inside the
    // deadline while starving sixteen of them for the whole run. The
    // acknowledgement order is what separates rotation from that: the earliest
    // table to finish its demand may not finish before the latest table to
    // start has had its first turn.
    let order: Vec<usize> = order
        .lock()
        .expect("the acknowledgement order is readable")
        .clone();
    let mut first_turns = Vec::with_capacity(ROTATION_TABLES);
    let mut last_turns = Vec::with_capacity(ROTATION_TABLES);
    for ordinal in 0..ROTATION_TABLES {
        assert_eq!(
            order.iter().filter(|table| **table == ordinal).count(),
            ROTATION_BATCHES,
            "table {ordinal} must be acknowledged exactly once per declared batch: {order:?}"
        );
        first_turns.push(
            order
                .iter()
                .position(|table| *table == ordinal)
                .expect("every table is admitted"),
        );
        last_turns.push(
            order
                .iter()
                .rposition(|table| *table == ordinal)
                .expect("every table completes its demand"),
        );
    }
    let latest_start = first_turns
        .iter()
        .max()
        .copied()
        .expect("seventeen tables declare turns");
    let earliest_finish = last_turns
        .iter()
        .min()
        .copied()
        .expect("seventeen tables declare turns");
    assert!(
        latest_start < earliest_finish,
        "no table may complete its whole demand before every peer has taken a \
         turn; the last table to start did so at {latest_start} and the first to \
         finish did so at {earliest_finish}: {order:?}"
    );

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
    let order = Arc::new(Mutex::new(Vec::<usize>::new()));
    let mut demands = Vec::with_capacity(ROTATION_TENANTS);
    for (ordinal, (client, table)) in equals.iter().enumerate() {
        demands.push(tokio::spawn(rotation_participant(
            Arc::clone(client),
            table.clone(),
            ordinal,
            // Tenant zero resumes at its second batch only if the append it
            // held inside the barrier was actually acknowledged.
            usize::from(ordinal == 0 && incumbent_first_admitted),
            Arc::clone(&order),
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

    let order: Vec<usize> = order
        .lock()
        .expect("the acknowledgement order is readable")
        .clone();
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
/// `order` records each acknowledgement in the order the pod granted it, so a
/// case can judge whose turn came when instead of only how many refusals it
/// took.
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
    order: Arc<Mutex<Vec<usize>>>,
) -> usize {
    let mut refusals = 0_usize;
    for batch in first_batch..ROTATION_BATCHES {
        let rows = rotation_batch(ordinal, batch);
        refusals += append_until_admitted(
            &client,
            &table,
            &rows,
            &format!("participant {ordinal} batch {batch}"),
        )
        .await;
        order
            .lock()
            .expect("the acknowledgement order is writable")
            .push(ordinal);
    }
    refusals
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
