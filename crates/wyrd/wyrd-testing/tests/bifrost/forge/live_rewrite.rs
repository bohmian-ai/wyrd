use std::collections::BTreeSet;
use std::time::Duration;

use uuid::Uuid;

use vala_sql::row_types::forge_tasks::{ForgeClaimStrategy, ForgeTaskStrategy};
use wyrd_spec::DataTenantId;
use wyrd_testing::bifrost::{WyrdTestCluster, shared_process_telemetry_for_test};

use crate::public_support::{
    append_values, read_sorted_values, register_table, tenant_client, try_read, unique_table,
};

/// Longest a journey waits for one production Forge attempt to return.
///
/// Every wait in this journey is a notification wait, never a poll: the bound
/// is a diagnostic ceiling so a stuck role fails loudly instead of hanging the
/// suite, and no assertion depends on how long a step took.
const ATTEMPT_BOUND: Duration = Duration::from_secs(60);

/// How many production scheduler passes one drain phase may take.
///
/// Planning is per table, and a promoted table then owes its own rewrite, so a
/// pod holding several tables needs more than one pass to owe nothing; the
/// budget bounds that without asserting how many passes it actually took.
const DRAIN_PASS_BUDGET: usize = 24;

/// Reads the durable Forge operation phases for one tenant's live rewrites.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn rewrite_phases(cluster: &WyrdTestCluster, tenant: DataTenantId) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT phase FROM vala.forge_operation_state \
         WHERE data_tenant_id = $1 AND family = 'iceberg_rewrite' \
         ORDER BY prepared_at, operation_id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge operation-state inspection")
}

/// Identifies one tenant's rewrite operations that claim no terminal outcome.
///
/// Returned newest first, so the caller can name the operation the pod just
/// failed to settle without assuming it is the tenant's only one.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn unsettled_rewrites(cluster: &WyrdTestCluster, tenant: DataTenantId) -> Vec<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT operation_id FROM vala.forge_operation_state \
         WHERE data_tenant_id = $1 AND family = 'iceberg_rewrite' AND phase = 'prepared' \
         ORDER BY prepared_at DESC, operation_id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge unsettled-rewrite inspection")
}

/// Reads the durable phase of one named Forge rewrite operation.
///
/// The journey asserts on the operation it armed rather than on the tenant's
/// newest one: a tenant owns more than one table, so the pod may legitimately
/// open an unrelated rewrite while the armed operation is being settled.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails, or when the operation is
/// absent, which would mean a settled operation lost its durable record.
async fn rewrite_phase(cluster: &WyrdTestCluster, operation: Uuid) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT phase FROM vala.forge_operation_state WHERE operation_id = $1",
    )
    .bind(operation)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge operation-phase inspection")
}

/// Counts one tenant-owned table's hot and promoted `vala.file_list` rows.
///
/// Supplemental evidence only: the public read is the correctness oracle, and
/// this exists to say *which tier* currently owns the rows that read returned.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn file_tiers(cluster: &WyrdTestCluster, tenant: DataTenantId, table: &str) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT count(*) FILTER (WHERE compacted), count(*) FILTER (WHERE NOT compacted) \
         FROM vala.file_list WHERE data_tenant_id = $1 AND table_name = $2",
    )
    .bind(tenant.as_uuid())
    .bind(table)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge file-list inspection")
}

/// Makes every `retryable` Forge task for one tenant immediately eligible.
///
/// Backoff between attempts is real production time, which a bounded journey
/// cannot wait out. Only the eligibility clock moves; the attempt count and the
/// failure classification are untouched, so a task failing for a real reason
/// still exhausts its attempts and still reports why.
///
/// # Panics
///
/// Panics when the eligibility update fails.
async fn release_retries(cluster: &WyrdTestCluster, tenant: DataTenantId) {
    sqlx::query(
        "UPDATE vala.forge_tasks \
         SET ready_at = statement_timestamp(), \
             next_eligible_at = statement_timestamp() - interval '15 minutes' \
         WHERE data_tenant_id = $1 AND state = 'retryable'",
    )
    .bind(tenant.as_uuid())
    .execute(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge retry eligibility advance");
}

/// Collects the object paths one tenant's table currently owns, by tier.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn file_paths(
    cluster: &WyrdTestCluster,
    tenant: DataTenantId,
    table: &str,
) -> BTreeSet<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT file_path FROM vala.file_list WHERE data_tenant_id = $1 AND table_name = $2",
    )
    .bind(tenant.as_uuid())
    .bind(table)
    .fetch_all(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge file-path inspection")
    .into_iter()
    .collect()
}

/// Counts one tenant's still-hot `vala.file_list` rows across every table.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn hot_rows(cluster: &WyrdTestCluster, tenant: DataTenantId) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND NOT compacted",
    )
    .bind(tenant.as_uuid())
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge hot-row inspection")
}

/// Counts the Forge tasks the pod has not yet driven to a terminal state.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn pending_tasks(cluster: &WyrdTestCluster) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM vala.forge_tasks \
         WHERE state NOT IN ('succeeded', 'failed', 'cancelled', 'unschedulable')",
    )
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge pending-task inspection")
}

/// Drives production passes until the pod owes no promotion and holds no task.
///
/// Both halves of the exit condition are durable and neither is a count of
/// passes or completions: a pass that claimed nothing and a completion that
/// moved some other table must not be mistaken for this backlog draining. The
/// task half matters as much as the row half, because the worker claims a ready
/// task on its own — a journey that arms something against "the next commit"
/// with a task still pending would arm it against that task instead.
///
/// Each iteration waits on a notification, never a sleep, and a lapsed wait is
/// not a failure on its own: the durable check at the top of the next iteration
/// is the verdict.
///
/// # Panics
///
/// Panics when the backlog is still owed after the pass budget.
async fn drain_forge_backlog(
    cluster: &WyrdTestCluster,
    observer: &vala_bifrost_redux::forge::ForgeWorkerCompletionObserver,
    tenants: &[DataTenantId],
) {
    for _ in 0..DRAIN_PASS_BUDGET {
        let mut owed = pending_tasks(cluster).await;
        for tenant in tenants {
            owed += hot_rows(cluster, *tenant).await;
        }
        if owed == 0 {
            return;
        }
        for tenant in tenants {
            release_retries(cluster, *tenant).await;
        }
        let target = observer.attempts().saturating_add(1);
        cluster.request_forge_scheduler_pass_for_test();
        let _ =
            tokio::time::timeout(ATTEMPT_BOUND, observer.wait_for_attempts_at_least(target)).await;
    }
    panic!(
        "the pod's own Forge left work owed after {DRAIN_PASS_BUDGET} passes: {:?}",
        observer.returned_errors()
    );
}

/// Promoted rows survive a rewrite whose acceptance the committer never learned.
///
/// The route is the shipped one end to end: two tenants register the same table
/// name, append through authenticated public gRPC, and read back through the
/// strict fused public query route. The pod's own Scribe publishes their
/// objects, the pod's own Forge scheduler and worker promote them, and the
/// settled table then plans its own rewrite. That rewrite's commit is accepted
/// by the real catalog which then loses the response, so the worker claims
/// nothing — the operation stays open even though the replacement is already
/// live. The successor recognises its predecessor's own snapshot from retained
/// evidence, settles that one operation, and publishes nothing a second time.
///
/// What must hold at every step — before promotion, after promotion, across the
/// uncertain commit, and after recovery — is that a public read returns exactly
/// the rows that tenant acknowledged, and that the neighbouring tenant's
/// identically named table is neither read, rewritten, nor disturbed.
///
/// # Panics
///
/// Panics when the cluster cannot start, a public append or read fails, a
/// production attempt misses its diagnostic bound, a tenant observes another
/// tenant's rows, or the uncertain commit is settled more than once.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn forge_promoted_files_rewrite_and_remain_exact_across_recovery() {
    let (_telemetry_guard, telemetry) =
        shared_process_telemetry_for_test().expect("process production telemetry");
    let checkpoint = telemetry
        .checkpoint()
        .expect("production telemetry baseline");
    let cluster =
        WyrdTestCluster::start_embedded_forge_uncertainty_for_test(Duration::from_hours(1))
            .await
            .expect("one bound embedded Bifrost pod starts");
    let observer = cluster
        .forge_completion_observer()
        .expect("the journey pod carries a Forge completion observer");
    let uncertainty = cluster
        .commit_uncertainty_catalog()
        .expect("the journey pod carries the uncertainty catalog");
    let server = cluster.server(0).expect("the embedded pod is running");
    assert!(
        server.base_url().is_some() && server.grpc_url().is_some(),
        "a public journey needs a bound HTTP and gRPC surface"
    );

    let owner = cluster.data_tenant_id();
    let neighbour = server
        .seed_tenant("forge-rewrite-neighbour")
        .await
        .expect("the neighbouring tenant is seeded");
    let shared_name = unique_table("rewrite_recovery");
    let shared = register_table(server, owner, &shared_name).await;
    let neighbour_shared = register_table(server, neighbour, &shared_name).await;
    assert_eq!(
        shared, neighbour_shared,
        "both tenants must be registering the identical table name"
    );
    let neighbour_only = register_table(server, neighbour, &unique_table("neighbour_only")).await;
    let owner_client = tenant_client(server, owner).await;
    let neighbour_client = tenant_client(server, neighbour).await;

    // Two sealed objects per tenant: one is the smallest set a rewrite can
    // merge, and the sentinel ranges are disjoint so a crossed read is visible
    // as a value rather than only as a count.
    let owner_rows: Vec<i64> = (0..24).collect();
    let neighbour_rows: Vec<i64> = (1_000..1_024).collect();
    for half in 0..2 {
        let span = 12 * half..12 * (half + 1);
        append_values(&owner_client, &shared, &owner_rows[span.clone()]).await;
        append_values(&neighbour_client, &shared, &neighbour_rows[span.clone()]).await;
        append_values(&neighbour_client, &neighbour_only, &neighbour_rows[span]).await;
        server
            .flush_bifrost()
            .await
            .expect("the pod publishes its staged rows");
    }

    assert_eq!(
        read_sorted_values(&owner_client, &shared).await,
        owner_rows,
        "a public read before promotion returns exactly the accepted rows"
    );
    assert_eq!(
        read_sorted_values(&neighbour_client, &shared).await,
        neighbour_rows,
        "the neighbouring tenant reads exactly its own rows from the shared name"
    );
    assert!(
        try_read(&owner_client, &neighbour_only).await.is_err(),
        "one tenant must not resolve a table only its neighbour registered"
    );

    // Closing the written partition is what makes the pod's own planner see a
    // compactable group; nothing durable is fabricated.
    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("the written partition closes");

    drain_forge_backlog(&cluster, &observer, &[owner, neighbour]).await;
    assert!(
        observer
            .completed_strategies()
            .contains(&ForgeClaimStrategy::Known(
                ForgeTaskStrategy::ScribePromotion
            )),
        "the server-owned worker ran the production promotion strategy"
    );

    assert_eq!(
        read_sorted_values(&owner_client, &shared).await,
        owner_rows,
        "a public read after promotion returns exactly the accepted rows"
    );
    assert_eq!(
        read_sorted_values(&neighbour_client, &shared).await,
        neighbour_rows,
        "promotion did not move a row across the tenant boundary"
    );
    let (owner_promoted, owner_hot) = file_tiers(&cluster, owner, &shared_name).await;
    assert!(
        owner_promoted >= 2 && owner_hot == 0,
        "the owner's objects all moved to the promoted tier: {owner_promoted}/{owner_hot}"
    );
    let neighbour_before = file_paths(&cluster, neighbour, &shared_name).await;

    // A third accepted round the owner alone writes. Draining it leaves that
    // tenant owing exactly one rewrite over promoted files from two promotion
    // generations, and leaves its neighbour owing nothing, so the next commit
    // the pod makes is the one this journey is about to render uncertain.
    let owner_rows: Vec<i64> = (0..36).collect();
    append_values(&owner_client, &shared, &owner_rows[24..]).await;
    server
        .flush_bifrost()
        .await
        .expect("the pod publishes the third accepted round");
    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("the third round's partition closes");
    drain_forge_backlog(&cluster, &observer, &[owner, neighbour]).await;
    assert_eq!(
        read_sorted_values(&owner_client, &shared).await,
        owner_rows,
        "the promoted cut returns every accepted row"
    );

    // The catalog accepts the replacement and then loses its answer, which is
    // the one outcome a committer cannot resolve on its own.
    uncertainty.fail_after_next_commit();
    let attempts = observer.attempts();
    release_retries(&cluster, owner).await;
    cluster.request_forge_scheduler_pass_for_test();
    tokio::time::timeout(
        ATTEMPT_BOUND,
        observer.wait_for_attempts_at_least(attempts + 1),
    )
    .await
    .expect("the uncertain rewrite attempt returned");

    let unsettled = unsettled_rewrites(&cluster, owner).await;
    assert_eq!(
        unsettled.len(),
        1,
        "an uncertain rewrite claims no outcome: {:?}",
        observer.returned_errors()
    );
    let uncertain = unsettled[0];
    assert_eq!(
        rewrite_phases(&cluster, owner)
            .await
            .last()
            .map(String::as_str),
        Some("prepared"),
        "the operation the pod could not settle is the one it just attempted"
    );
    let rewritten = file_paths(&cluster, owner, &shared_name).await;
    assert_eq!(
        read_sorted_values(&owner_client, &shared).await,
        owner_rows,
        "the rewritten cut reads exactly the rows the promoted cut did"
    );
    assert_eq!(
        read_sorted_values(&neighbour_client, &shared).await,
        neighbour_rows,
        "an uncertain rewrite of one tenant leaves its neighbour's rows exact"
    );

    // The successor settles its predecessor's own snapshot from retained
    // evidence rather than publishing a second replacement.
    let mut settled = false;
    for _ in 0..DRAIN_PASS_BUDGET {
        if rewrite_phase(&cluster, uncertain).await != "prepared" {
            settled = true;
            break;
        }
        release_retries(&cluster, owner).await;
        server
            .reclaim_expired_forge_attempts_for_test(16)
            .await
            .expect("the production reclaim pass runs");
        let target = observer.attempts().saturating_add(1);
        cluster.request_forge_scheduler_pass_for_test();
        let _ =
            tokio::time::timeout(ATTEMPT_BOUND, observer.wait_for_attempts_at_least(target)).await;
    }
    assert!(
        settled,
        "the pod never settled its one uncertain operation in {DRAIN_PASS_BUDGET} passes: {:?}",
        observer.returned_errors()
    );
    assert_eq!(
        rewrite_phase(&cluster, uncertain).await,
        "recovered",
        "the successor settles its predecessor's own operation as recovered"
    );
    assert_eq!(
        file_paths(&cluster, owner, &shared_name).await,
        rewritten,
        "recovery rewrote nothing a second time"
    );
    assert_eq!(
        read_sorted_values(&owner_client, &shared).await,
        owner_rows,
        "the public read after recovery still returns exactly the accepted rows"
    );
    assert_eq!(
        read_sorted_values(&neighbour_client, &shared).await,
        neighbour_rows,
        "recovery never touched the neighbouring tenant's identically named table"
    );
    assert_eq!(
        file_paths(&cluster, neighbour, &shared_name).await,
        neighbour_before,
        "the neighbouring tenant's objects were never replaced"
    );
    assert_eq!(
        read_sorted_values(&neighbour_client, &neighbour_only).await,
        neighbour_rows,
        "the neighbour-only table is unchanged by the owner's rewrite"
    );
    assert!(
        try_read(&owner_client, &neighbour_only).await.is_err(),
        "the tenant boundary still refuses a neighbour-only table after recovery"
    );

    let delta = telemetry
        .delta_since(&checkpoint)
        .expect("production telemetry delta");
    let families = delta
        .metrics
        .iter()
        .map(|sample| sample.family.clone())
        .collect::<BTreeSet<_>>();
    assert!(
        families.contains("bifrost_forge_operations"),
        "the production route reported its operation telemetry: {families:?}"
    );
}
