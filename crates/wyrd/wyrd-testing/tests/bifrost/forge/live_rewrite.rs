use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use uuid::Uuid;

use vala_bifrost_redux::catalog::TenantTableBinding;
use vala_sql::row_types::forge_tasks::{ForgeClaimStrategy, ForgeTaskStrategy};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::RegisterOutcome;
use wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta;
use wyrd_testing::bifrost::{WyrdTestCluster, shared_process_telemetry_for_test};

use crate::public_support::{
    JourneyTable, ManagedRow, append_values, assert_tenant_scoped_not_found, canonical_order,
    enable_compaction, public_rows_returned, read_managed_rows, register_table, rows_digest,
    set_table_properties, tenant_client, unique_table,
};

/// Longest a journey waits for one production Forge attempt to return.
///
/// Every wait in this journey is a notification wait, never a poll: the bound
/// is a diagnostic ceiling so a stuck role fails loudly instead of hanging the
/// suite, and no assertion depends on how long a step took.
const ATTEMPT_BOUND: Duration = Duration::from_secs(15);

/// How long one released, unresolved attempt may take to let its claim go.
///
/// An attempt whose sibling plans are still exhausting their own publication
/// retry schedules against the refusing catalog is not released until the last
/// of them returns, so this bound covers the whole schedule rather than one
/// catalog call.
const RELEASE_BOUND: Duration = Duration::from_secs(120);

/// How many production scheduler passes one drain phase may take.
///
/// Planning is per table, and a promoted table then owes its own rewrite, so a
/// pod holding several tables needs more than one pass to owe nothing; the
/// budget bounds that without asserting how many passes it actually took.
const DRAIN_PASS_BUDGET: usize = 24;
/// Compaction interval, in seconds, of the recovery journey's owner table.
///
/// Two and a half days outlasts the journey's two one-day partition closes, so
/// no promotion makes the table due; the two-day advance after uncertainty is
/// armed is what crosses it.
const REWRITE_INTERVAL_SECS: &str = "216000";

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

/// Reads one exact rewrite operation's durable phase, when the row exists.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn rewrite_phase_of(cluster: &WyrdTestCluster, operation: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT phase FROM vala.forge_operation_state WHERE operation_id = $1",
    )
    .bind(operation)
    .fetch_optional(cluster.pg_fixture().operator_pool().pool())
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

/// Lapses the claim lease of every task this tenant still holds Running.
///
/// An owner that could not account for its own operation releases the attempt
/// and stops renewing its heartbeat, so the claim lease lapses on its own TTL
/// and the production reclaim pass takes the task back. That TTL is production
/// minutes, and a journey proves the recovery rather than the wait: this
/// advances the persisted deadline the reclaim reads, leaving the reclaim
/// itself to the real bounded transaction.
///
/// # Panics
///
/// Panics when the deadline update fails.
async fn expire_running_claims(cluster: &WyrdTestCluster, tenant: DataTenantId) {
    sqlx::query(
        "UPDATE vala.forge_tasks \
         SET claim_expires_at = statement_timestamp() - interval '1 millisecond' \
         WHERE data_tenant_id = $1 AND state = 'running'",
    )
    .bind(tenant.as_uuid())
    .execute(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge running-claim deadline lapse");
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
         WHERE state NOT IN ('succeeded', 'failed', 'cancelled')",
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
        let target = observer.attempts().saturating_add(1);
        let pending = pending_tasks(cluster).await;
        let mut owed = pending;
        for tenant in tenants {
            owed += hot_rows(cluster, *tenant).await;
        }
        if owed == 0 {
            return;
        }
        for tenant in tenants {
            release_retries(cluster, *tenant).await;
        }
        // Drain the admitted tasks before asking for another planning cycle;
        // each fresh cycle may legitimately enqueue another orphan scan.
        if pending == 0 {
            cluster.request_forge_scheduler_pass_for_test();
        }
        let _ =
            tokio::time::timeout(ATTEMPT_BOUND, observer.wait_for_attempts_at_least(target)).await;
    }
    panic!(
        "the pod's own Forge left work owed after {DRAIN_PASS_BUDGET} passes: {:?}",
        observer.returned_errors()
    );
}

/// The complete live cut of one table's current snapshot.
///
/// Data and delete attachments are held apart because the rewrite algebra
/// applies to them separately: a snapshot removes data files and *may* remove
/// delete files whose scope it consumed, and a cut compared only on data would
/// accept a publication that silently changed what a reader sees.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LiveCut {
    /// Snapshot the cut was read from.
    snapshot_id: i64,
    /// Live data object paths, in canonical storage form.
    data: BTreeSet<String>,
    /// Live position- and equality-delete object paths, in canonical storage form.
    deletes: BTreeSet<String>,
}

/// One rewrite snapshot's identity, summary, and exact manifest algebra.
///
/// Everything here is read from the catalog the pod actually published
/// through, never from `vala.file_list`: the question a recovery journey has
/// to answer is what the *table* says happened, and only the manifests say it.
/// The byte volumes are summed from the same complete live maps the path sets
/// are differenced from, so a snapshot summary property is only ever an
/// *actual* value under test here and never the expectation it is checked
/// against.
#[derive(Debug, Clone)]
struct RewriteSnapshot {
    /// Identity of the snapshot the rewrite published.
    snapshot_id: i64,
    /// Snapshot this one was committed onto.
    parent_snapshot_id: Option<i64>,
    /// The snapshot's `forge.*` summary properties.
    summary: BTreeMap<String, String>,
    /// Data paths this snapshot added, in canonical storage form.
    added_data: BTreeSet<String>,
    /// Data paths this snapshot removed, in canonical storage form.
    removed_data: BTreeSet<String>,
    /// Delete paths this snapshot removed, in canonical storage form.
    removed_deletes: BTreeSet<String>,
    /// Manifest-recorded byte volume of every data path this snapshot added.
    added_bytes: u64,
    /// Manifest-recorded byte volume of every data path this snapshot removed.
    removed_bytes: u64,
}

/// One live manifest entry, with every fact the rewrite algebra reasons about.
///
/// Held instead of a bare path because the volume claims a rewrite makes are
/// about bytes, not names: a summary that reported the right file *count* with
/// the wrong byte total would otherwise be indistinguishable from a correct
/// one. Content type and the sequence numbers travel with it so a delete
/// attachment is never counted as data and a replacement's inherited sequence
/// stays inspectable from the same projection.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LiveFile {
    /// Iceberg content type the manifest entry declared for the object.
    content_type: iceberg::spec::DataContentType,
    /// Physical size in bytes the manifest recorded for the object.
    size_bytes: u64,
    /// Data sequence number the live entry carries, when the manifest has one.
    sequence_number: Option<i64>,
    /// File sequence number the live entry carries, when the manifest has one.
    file_sequence_number: Option<i64>,
}

/// Every file one snapshot holds live, split by content and keyed by path.
///
/// This is the journey's single manifest projection: every path set and every
/// byte total below is derived from two of these, so the two can never
/// disagree about which entries they described.
#[derive(Debug, Clone, Default)]
struct SnapshotFiles {
    /// Live data entries of the snapshot, keyed by canonical storage path.
    data: BTreeMap<String, LiveFile>,
    /// Live position- and equality-delete entries, keyed by canonical path.
    deletes: BTreeMap<String, LiveFile>,
}

impl SnapshotFiles {
    /// Returns the canonical paths of every live data entry.
    fn data_paths(&self) -> BTreeSet<String> {
        self.data.keys().cloned().collect()
    }

    /// Returns the canonical paths of every live delete attachment.
    fn delete_paths(&self) -> BTreeSet<String> {
        self.deletes.keys().cloned().collect()
    }

    /// Sums the manifest-recorded size of the named data entries.
    ///
    /// # Panics
    ///
    /// Panics when a named path is not a live data entry of this snapshot,
    /// which would mean the caller differenced two unrelated projections.
    fn data_bytes(&self, paths: &BTreeSet<String>) -> u64 {
        paths
            .iter()
            .map(|path| {
                self.data
                    .get(path)
                    .unwrap_or_else(|| panic!("{path} is a live data entry of this snapshot"))
                    .size_bytes
            })
            .sum()
    }
}

/// Rewrites one file path into the tenant-object-prefix form.
///
/// The same physical object is named two ways in this system: the catalog
/// stores an absolute warehouse URI, while the durable Forge plan and Prepared
/// detail store the storage-relative key. Both contain the binding's own
/// object prefix verbatim, so anchoring on that prefix normalises either form
/// without the journey reconstructing a storage layout it does not own. A path
/// that does not contain the prefix is foreign to this table and is returned
/// unchanged so a comparison against it fails loudly rather than silently.
fn canonical_path(path: &str, binding: &TenantTableBinding) -> String {
    let prefix = binding.object_prefix.as_str();
    path.find(prefix)
        .map_or_else(|| path.to_owned(), |start| path[start..].to_owned())
}

/// Reads every file one named snapshot holds live, partitioned by content.
///
/// This walks the snapshot's complete manifest list and keeps only alive
/// entries, which is the one definition of "live" the catalog itself uses. It
/// is deliberately not a diff: an Iceberg commit that empties a manifest drops
/// that manifest from the new list rather than rewriting it with `DELETED`
/// entries, so per-snapshot `ADDED`/`DELETED` scanning silently under-reports.
/// Every algebraic claim in this journey is instead a difference of two of
/// these complete live sets.
///
/// # Panics
///
/// Panics when the manifest list or any manifest it names cannot be read.
async fn snapshot_files(
    table: &iceberg::table::Table,
    snapshot: &iceberg::spec::SnapshotRef,
    binding: &TenantTableBinding,
) -> SnapshotFiles {
    let mut files = SnapshotFiles::default();
    let manifests = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("the journey manifest list");
    for manifest_file in manifests.entries() {
        let manifest = manifest_file
            .load_manifest(table.file_io())
            .await
            .expect("the journey manifest");
        for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
            let data_file = entry.data_file();
            let path = canonical_path(data_file.file_path(), binding);
            let content_type = data_file.content_type();
            let live = LiveFile {
                content_type,
                size_bytes: data_file.file_size_in_bytes(),
                sequence_number: entry.sequence_number(),
                file_sequence_number: entry.file_sequence_number,
            };
            if content_type == iceberg::spec::DataContentType::Data {
                files.data.insert(path, live);
            } else {
                files.deletes.insert(path, live);
            }
        }
    }
    files
}

/// Reads one table's current live cut through the pod's production catalog.
///
/// # Panics
///
/// Panics when the table or its manifests cannot be read, or when the table
/// carries no current snapshot, which a journey only reaches after a
/// successful publication.
async fn live_cut(cluster: &WyrdTestCluster, binding: &TenantTableBinding) -> LiveCut {
    let server = cluster.server(0).expect("the embedded pod is running");
    let table = server
        .bifrost_catalog()
        .iceberg_catalog()
        .load_table(&binding.table_ident())
        .await
        .expect("the journey table loads through the production catalog");
    let snapshot = table
        .metadata()
        .current_snapshot()
        .expect("a published journey table has a current snapshot")
        .clone();
    let files = snapshot_files(&table, &snapshot, binding).await;
    LiveCut {
        snapshot_id: snapshot.snapshot_id(),
        data: files.data_paths(),
        deletes: files.delete_paths(),
    }
}

/// Projects every snapshot this table carries that a Forge rewrite published.
///
/// Identified by the production summary property rather than by position or
/// timestamp, so an unrelated append between two rewrites cannot be miscounted
/// as one and a rewrite cannot be missed because something committed after it.
/// Each rewrite's algebra is the difference between its own complete live set
/// and its parent's, which is what the table actually gained and lost at that
/// commit regardless of how the catalog chose to encode it in manifests.
///
/// # Panics
///
/// Panics when the table or any manifest it names cannot be read, or when a
/// rewrite snapshot names a parent the metadata does not carry, which would
/// mean the rewrite committed onto a base the table has since forgotten.
async fn rewrite_snapshots(
    cluster: &WyrdTestCluster,
    binding: &TenantTableBinding,
) -> Vec<RewriteSnapshot> {
    let server = cluster.server(0).expect("the embedded pod is running");
    let table = server
        .bifrost_catalog()
        .iceberg_catalog()
        .load_table(&binding.table_ident())
        .await
        .expect("the journey table loads through the production catalog");
    let mut found = Vec::new();
    for snapshot in table.metadata().snapshots() {
        let summary = snapshot
            .summary()
            .additional_properties
            .iter()
            .filter(|(key, _)| key.starts_with("forge."))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>();
        if summary.get("forge.workflow").map(String::as_str) != Some("iceberg-rewrite") {
            continue;
        }
        let parent_snapshot_id = snapshot.parent_snapshot_id();
        let parent = parent_snapshot_id
            .and_then(|parent| table.metadata().snapshot_by_id(parent))
            .unwrap_or_else(|| {
                panic!(
                    "a rewrite snapshot names a base the table still carries: {:?}",
                    snapshot.snapshot_id()
                )
            });
        let own = snapshot_files(&table, snapshot, binding).await;
        let base = snapshot_files(&table, parent, binding).await;
        let own_data = own.data_paths();
        let base_data = base.data_paths();
        let added_data = own_data
            .difference(&base_data)
            .cloned()
            .collect::<BTreeSet<_>>();
        let removed_data = base_data
            .difference(&own_data)
            .cloned()
            .collect::<BTreeSet<_>>();
        found.push(RewriteSnapshot {
            snapshot_id: snapshot.snapshot_id(),
            parent_snapshot_id,
            summary,
            added_bytes: own.data_bytes(&added_data),
            removed_bytes: base.data_bytes(&removed_data),
            added_data,
            removed_data,
            removed_deletes: base
                .delete_paths()
                .difference(&own.delete_paths())
                .cloned()
                .collect(),
        });
    }
    found
}

/// The durable facts one Prepared rewrite operation committed to before its call.
///
/// This is the operation's own immutable promise: what it would remove, what
/// it would add, and which base it derived them from. Correlating the Iceberg
/// snapshot against *this* — rather than against whatever the table happens to
/// hold — is what makes the landed effect provably the operation's own.
#[derive(Debug, Clone)]
struct PreparedRewrite {
    /// Operation identity, which is also the publishing attempt's identity.
    operation_id: Uuid,
    /// Snapshot the replacement was derived against.
    base_snapshot_id: i64,
    /// Exact catalog paths the commit would delete, in storage form.
    input_paths: BTreeSet<String>,
    /// Exact rewritten object paths the commit would add, in storage form.
    output_paths: BTreeSet<String>,
}

/// Reads the durable Prepared detail of one named rewrite operation.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails, or when the row does not
/// carry the Prepared rewrite detail shape it is required to.
async fn prepared_rewrite(
    cluster: &WyrdTestCluster,
    operation: Uuid,
    binding: &TenantTableBinding,
) -> PreparedRewrite {
    let detail = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT prepared_detail FROM vala.forge_operation_state WHERE operation_id = $1",
    )
    .bind(operation)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge prepared-detail inspection");
    let paths = |field: &str| {
        detail
            .get(field)
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| panic!("the Prepared detail carries {field}: {detail}"))
            .iter()
            .map(|value| {
                canonical_path(
                    value.as_str().expect("a Prepared detail path is a string"),
                    binding,
                )
            })
            .collect::<BTreeSet<_>>()
    };
    PreparedRewrite {
        operation_id: detail
            .get("operation_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap_or_else(|| panic!("the Prepared detail names its operation: {detail}")),
        base_snapshot_id: detail
            .get("base_snapshot_id")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_else(|| panic!("the Prepared detail names its base: {detail}")),
        input_paths: paths("input_paths"),
        output_paths: paths("output_paths"),
    }
}

/// Reads the durable small-files task the given rewrite attempt belongs to.
///
/// Returns the task identity and the exact selected input set its immutable
/// plan bound, which is the only authority on what this rewrite was allowed to
/// consume.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails or the task is absent.
async fn rewrite_task_plan(
    cluster: &WyrdTestCluster,
    task_id: Uuid,
    binding: &TenantTableBinding,
) -> BTreeSet<String> {
    let plan = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT plan FROM vala.forge_tasks WHERE task_id = $1 AND strategy = 'small_files'",
    )
    .bind(task_id)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge rewrite-task inspection");
    plan.get("inputs")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("the durable rewrite plan binds its inputs: {plan}"))
        .iter()
        .map(|value| {
            canonical_path(
                value
                    .as_str()
                    .expect("a durable plan input is an object path"),
                binding,
            )
        })
        .collect()
}

/// Reads the durable lifecycle state of one Forge task.
///
/// This is the committed authority an attempt's reported result is compared
/// with, read independently of any production telemetry.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails or the task is absent.
async fn durable_task_state(cluster: &WyrdTestCluster, task_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT state FROM vala.forge_tasks WHERE task_id = $1")
        .bind(task_id)
        .fetch_one(cluster.pg_fixture().operator_pool().pool())
        .await
        .expect("Forge task state inspection")
}

/// Reads the durable canonical plan hash of one small-files Forge task.
///
/// Returned hex-encoded, which is exactly how publication renders it into the
/// snapshot's `forge.rewrite.plan_hash` property, so the comparison is against
/// the persisted bytes rather than against a hash this journey recomputed.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails or the task is absent.
async fn durable_plan_hash(cluster: &WyrdTestCluster, task_id: Uuid) -> String {
    let plan_hash = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT plan_hash FROM vala.forge_tasks WHERE task_id = $1 AND strategy = 'small_files'",
    )
    .bind(task_id)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge rewrite-task plan-hash inspection");
    assert_eq!(
        plan_hash.len(),
        32,
        "the durable rewrite task carries its canonical plan hash"
    );
    hex::encode(plan_hash)
}

/// Counts the plans one named attempt admitted and published under.
///
/// One attempt publishes each of its plans under its own operation identity,
/// and the production dispatch boundary records one evidence entry per plan.
/// Counting those entries is what bounds "one catalog commit per admitted
/// plan" to *this* attempt: the durable operation rows on the table also carry
/// whatever a successor opened after the release, so a phase count would
/// answer for more than one attempt.
fn attempt_plan_count(
    observer: &vala_bifrost_redux::forge::ForgeWorkerCompletionObserver,
    attempt_id: Uuid,
) -> usize {
    observer
        .rewrite_evidence_for_test()
        .iter()
        .filter(|record| record.attempt_id == attempt_id)
        .count()
}

/// Selects the publication evidence one named attempt produced.
///
/// The three remaining rewrite fingerprints are execution evidence rather than
/// persisted task columns, so the only authority on their expected values is
/// the immutable evidence the production dispatch boundary recorded for this
/// exact task, attempt, and operation. Selecting by all three identities is
/// what stops an unrelated rewrite the same pod ran from answering.
///
/// # Panics
///
/// Panics when no evidence, or more than one, was recorded for the identities.
fn attempt_evidence(
    observer: &vala_bifrost_redux::forge::ForgeWorkerCompletionObserver,
    task_id: Uuid,
    attempt_id: Uuid,
    operation_id: Uuid,
) -> vala_bifrost_redux::forge::ForgeRewriteEvidence {
    let recorded = observer.rewrite_evidence_for_test();
    let mine = recorded
        .iter()
        .filter(|record| {
            record.task_id == task_id
                && record.attempt_id == attempt_id
                && record.operation_id == operation_id
        })
        .collect::<Vec<_>>();
    assert_eq!(
        mine.len(),
        1,
        "exactly one admitted attempt produced evidence for \
         task {task_id}/attempt {attempt_id}/operation {operation_id}: {recorded:?}"
    );
    mine[0].evidence.clone()
}

/// The canonical Forge audit resource one tenant-owned table is audited under.
///
/// Derived from the binding the journey registered rather than read off the
/// snapshot being inspected, so correlating an audit row to this table is an
/// independent statement and not a restatement of what the snapshot claims.
fn forge_audit_resource(binding: &TenantTableBinding) -> String {
    format!(
        "bifrost://{}/{}/{}",
        binding.tenant, binding.table_ref.namespace, binding.table_ref.name
    )
}

/// Reads the durable phase `vala.forge_operation_state` holds for one operation.
///
/// Forge transitions evaluate no principal permission, so this projection — not
/// audit — is the settlement authority. Read-only: the journey inspects the
/// ledger the production owners wrote and never appends to it.
///
/// # Panics
/// Panics when the read-only diagnostic query fails or the operation row is
/// absent, which would mean a Forge settlement lost its lineage.
async fn rewrite_operation_phase(cluster: &WyrdTestCluster, operation: Uuid) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT phase FROM vala.forge_operation_state WHERE operation_id = $1",
    )
    .bind(operation)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge settlement inspection")
}

/// Asserts one tenant's public read returns exactly the rows it acknowledged.
///
/// Both the ordered identity vector and its canonical digest are compared. The
/// vector is what names the defect when this fails; the digest is what makes
/// the comparison a single stable fact the completion record can quote, and
/// what would still catch an encoding difference the vector comparison
/// normalised away.
///
/// # Panics
///
/// Panics when the public read differs from the acknowledged rows in any way.
async fn assert_public_rows(
    client: &wyrd_client::WyrdClient,
    table: &JourneyTable,
    expected: &[ManagedRow],
    cut: &str,
) -> String {
    let read = read_managed_rows(client, &table.qualified).await;
    assert_eq!(
        read, expected,
        "{cut}: the public read returns exactly the acknowledged rows of {}",
        table.qualified
    );
    let digest = rows_digest(&read);
    assert_eq!(
        digest,
        rows_digest(expected),
        "{cut}: the public read's canonical digest matches the acknowledged rows"
    );
    digest
}

/// Forge families the journey and recovery windows print as evidence.
const FORGE_FAMILIES: [&str; 10] = [
    "bifrost_forge_active_tasks",
    "bifrost_forge_tasks_created_total",
    "bifrost_forge_task_attempts_total",
    "bifrost_forge_task_failures_total",
    "bifrost_forge_input_files_total",
    "bifrost_forge_input_bytes_total",
    "bifrost_forge_output_files_total",
    "bifrost_forge_output_bytes_total",
    "bifrost_forge_compaction_debt_files",
    "bifrost_forge_compaction_debt_bytes",
];

/// Durable rewrite facts one recovered operation must be able to prove.
///
/// Every field is read out of Postgres or the Iceberg manifests before this
/// struct is built, so the telemetry assertion compares production signals to
/// independently established truth rather than to another telemetry reading.
struct RecoveryTelemetry {
    /// Durable Forge task the landed snapshot named as its owner.
    task_id: Uuid,
    /// Durable `vala.forge_tasks.state` of that task after recovery settled.
    task_state: String,
    /// Attempt identity the uncertain publication committed under.
    attempt_id: Uuid,
    /// Plans the attempt admitted, each of which publishes independently and
    /// is therefore the ceiling on how many catalog commits it may submit.
    plans: usize,
    /// Live data files the recovered operation promised to remove, plus those
    /// the recovering dispatch's own later rewrites removed.
    input_files: u64,
    /// Managed data files the recovered operation promised to add, plus those
    /// the recovering dispatch's own later rewrites added.
    output_files: u64,
    /// Byte volume the landed snapshot and the later rewrites recorded as
    /// removed.
    input_bytes: u64,
    /// Byte volume the landed snapshot and the later rewrites recorded as
    /// added.
    output_bytes: u64,
    /// Rows public appends acknowledged inside the journey window.
    acknowledged_rows: u64,
    /// Rows public reads really returned inside the window.
    returned_rows: u64,
}

/// Reads one numeric Iceberg snapshot summary property.
///
/// # Panics
///
/// Panics when the property is absent or is not a `u64`, which would mean the
/// versioned rewrite property set the snapshot claims is incomplete.
fn summary_u64(summary: &BTreeMap<String, String>, key: &str) -> u64 {
    summary
        .get(key)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("the landed snapshot carries a numeric {key}: {summary:?}"))
}

/// Sums the window delta of one production counter series.
///
/// Selection is by exact family plus exact label equality, so an unrelated
/// series in the same family cannot contribute to the answer.
fn counter_delta(delta: &BifrostTelemetryDelta, family: &str, labels: &[(&str, &str)]) -> f64 {
    delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == family
                && labels
                    .iter()
                    .all(|(key, value)| sample.labels.get(*key).map(String::as_str) == Some(*value))
        })
        .map(|sample| sample.value)
        .sum()
}

/// Reads one scrubbed span attribute.
fn attribute<'a>(span: &'a wyrd_telemetry::CapturedSpan, key: &str) -> Option<&'a str> {
    span.attributes.get(key).map(String::as_str)
}

/// Proves the production telemetry tells the same story the durable facts do.
///
/// The journey window carries every span the whole route emitted, so span
/// selection is by durable identity: the catalog commit is found by the attempt
/// the operation actually published under, and the executing task by the task
/// the landed snapshot named. The recovery window is opened immediately before
/// the reconciliation loop so the recovered operation's own counted volume is
/// not contaminated by the rewrites earlier drain phases legitimately made.
///
/// Metric identity is also asserted negatively: production Forge series must
/// stay fixed-cardinality, so no sample may carry a tenant, table, task,
/// attempt, operation, or object-path label.
///
/// Attempt results are compared with what each settlement committed: the
/// released attempt committed nothing and so reports no result, the task's
/// last settled execution reports its durable state, every settled
/// execution's span result is counted exactly once in
/// `bifrost_forge_task_attempts_total`, and a successful execution trace
/// carries no routine claim or settle event.
///
/// # Panics
///
/// Panics when a required stage is missing, when a stage carries an identity
/// other than the durable one, when an attempt result disagrees with durable
/// state or with its counter, when a routine success event reappears, when the
/// recovered volume does not reconcile to the manifest-derived volume, or when
/// a Forge sample carries an unbounded label.
fn assert_recovery_telemetry(
    journey: &BifrostTelemetryDelta,
    recovery: &BifrostTelemetryDelta,
    facts: &RecoveryTelemetry,
) {
    // 1. The pod's own scheduler drove the route and said so.
    assert!(
        journey.spans.iter().any(|span| {
            span.name == "bifrost.forge.scheduler.pass"
                && attribute(span, "result") == Some("succeeded")
                && attribute(span, "role") == Some("server")
        }),
        "the production scheduler reported at least one successful pass: {:?}",
        journey
            .spans
            .iter()
            .map(|span| span.name.clone())
            .collect::<BTreeSet<_>>()
    );

    // 2. The attempt submitted at most one catalog commit per admitted plan —
    //    per-plan publication, never a retried one — and every commit it did
    //    submit reported the non-success the injected seam actually produced.
    //    Fewer commits than plans is production authority, not a lost outcome:
    //    once one plan's effect has landed, a sibling's planned base and inputs
    //    no longer hold and it is refused before any catalog call, leaving its
    //    operation open for the same durable reconciliation pass.
    let commits = journey
        .spans
        .iter()
        .filter(|span| {
            span.name == "bifrost.forge.catalog.commit"
                && attribute(span, "attempt_id") == Some(facts.attempt_id.to_string().as_str())
        })
        .collect::<Vec<_>>();
    assert!(
        !commits.is_empty() && commits.len() <= facts.plans,
        "the uncertain attempt submitted between one and one-per-plan catalog \
         commits, never more: {} of {} plans: {commits:?}",
        commits.len(),
        facts.plans
    );
    for commit in &commits {
        assert_eq!(
            attribute(commit, "task_id"),
            Some(facts.task_id.to_string().as_str()),
            "the commit span names the durable task the landed snapshot named: {commit:?}"
        );
        assert_eq!(
            attribute(commit, "strategy"),
            Some("iceberg_rewrite"),
            "the commit span names the managed rewrite strategy: {commit:?}"
        );
        assert_eq!(
            attribute(commit, "role"),
            Some("forge_worker"),
            "the commit span names the owning production role: {commit:?}"
        );
        assert_eq!(
            attribute(commit, "result"),
            Some("failed"),
            "a submission whose acceptance was never learned reports no success: {commit:?}"
        );
    }

    // 3. The task ran under the strategy the plan selected, and the publishing
    //    attempt ran it exactly once. How many executions follow is not this
    //    assertion's business: an open operation is recovered by whichever
    //    owner next reconciles the table from durable state, which may be a
    //    further execution of this task or the table-wide pass a maintenance
    //    task carries. That the recovery happened at all is asserted below,
    //    against the production counters of the recovery window itself.
    let executions = journey
        .spans
        .iter()
        .filter(|span| {
            span.name == "bifrost.forge.task.execute"
                && attribute(span, "task_id") == Some(facts.task_id.to_string().as_str())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        executions
            .iter()
            .filter(
                |span| attribute(span, "attempt_id") == Some(facts.attempt_id.to_string().as_str())
            )
            .count(),
        1,
        "exactly one execution ran under the published attempt: {executions:?}"
    );
    for span in &executions {
        assert_eq!(
            attribute(span, "role"),
            Some("forge_worker"),
            "every execution of the task named the owning role: {span:?}"
        );
        assert!(
            attribute(span, "strategy").is_some_and(|value| value.contains("small_files")),
            "every execution of the task named the planned rewrite strategy: {span:?}"
        );
    }

    // 3b. A result is what a settlement committed, never a Rust `Ok`. The
    //     released attempt wrote nothing durable, so its trace names no
    //     result.
    for span in executions
        .iter()
        .filter(|span| attribute(span, "attempt_id") == Some(facts.attempt_id.to_string().as_str()))
    {
        assert_eq!(
            attribute(span, "result"),
            None,
            "the released attempt committed nothing, so it reports no result: {span:?}"
        );
        for commit in &commits {
            assert_eq!(
                (commit.trace_id.as_str(), commit.parent_span_id.as_str()),
                (span.trace_id.as_str(), span.span_id.as_str()),
                "each catalog commit is a child of its attempt's task trace: {commit:?}"
            );
        }
    }
    let settled_result = executions
        .iter()
        .rev()
        .find_map(|span| attribute(span, "result"));
    // The leader owns a dispatched task's retry: the lapsed claim is closed by
    // reclaim rather than re-run, and the recovery settles under the leader's
    // next dispatch, whose success 3c observes.
    assert_eq!(
        (settled_result, facts.task_state.as_str()),
        (None, "cancelled"),
        "the released task is closed by reclaim, not settled by a second owner: {executions:?}"
    );

    // 3c. Each settled small-files execution is one trace and one counted
    //     attempt under the same result, and settlement released every
    //     claim-time active count. A registered zero cannot satisfy this: the
    //     recovery window must hold at least the recovered success.
    let recovered_executions = recovery
        .spans
        .iter()
        .filter(|span| {
            span.name == "bifrost.forge.task.execute"
                && attribute(span, "strategy").is_some_and(|value| value.contains("small_files"))
        })
        .filter_map(|span| attribute(span, "result"))
        .collect::<Vec<_>>();
    assert!(
        recovered_executions.contains(&"succeeded"),
        "the recovery window settled a small-files execution: {recovered_executions:?}"
    );
    for result in [
        "succeeded",
        "retry",
        "failed",
        "cancelled",
        "refused",
        "uncertain",
    ] {
        let traced = recovered_executions
            .iter()
            .filter(|traced| **traced == result)
            .count();
        let counted = counter_delta(
            recovery,
            "bifrost_forge_task_attempts_total",
            &[("task_type", "small_files"), ("result", result)],
        );
        assert!(
            (counted - traced as f64).abs() < f64::EPSILON,
            "small-files {result} attempts counted {counted} but traced {traced}"
        );
    }
    for sample in recovery
        .gauge_final
        .iter()
        .filter(|sample| sample.family == "bifrost_forge_active_tasks")
    {
        assert!(
            sample.value.abs() < f64::EPSILON,
            "settlement released every claimed attempt's active count: {sample:?}"
        );
    }

    // 3d. A settled task tells its story once, in its own trace: the routine
    //     claim and settle events are gone and nothing replaced them.
    for span in &journey.spans {
        for event in &span.events {
            assert!(
                ![
                    "Forge task claimed",
                    "Forge task settled",
                    "Forge recovery claimed an unfinished cleanup cursor",
                ]
                .contains(&event.name.as_str()),
                "a routine Forge success event was exported again: {span:?}"
            );
        }
    }

    // 4. The terminal reconciliation counted itself as a recovery.
    let recovered = counter_delta(
        recovery,
        "bifrost_forge_task_attempts_total",
        &[("task_type", "small_files"), ("result", "succeeded")],
    );
    assert!(
        recovered >= 1.0,
        "the reconciliation settled the recovered small-files attempt: {recovered}"
    );

    // 5. The counted recovery volume is the manifest-derived volume: the
    //    recovered operation counted once, plus whatever the recovering
    //    dispatch then rewrote under its own operations.
    for (family, expected) in [
        ("bifrost_forge_input_files_total", facts.input_files),
        ("bifrost_forge_input_bytes_total", facts.input_bytes),
        ("bifrost_forge_output_files_total", facts.output_files),
        ("bifrost_forge_output_bytes_total", facts.output_bytes),
    ] {
        let observed = counter_delta(recovery, family, &[("task_type", "small_files")]);
        assert!(
            (observed - expected as f64).abs() < f64::EPSILON,
            "{family} counted the recovered operation's own volume: {observed} vs {expected}"
        );
    }

    // 6. The rest of the shipped route reported itself in the same window.
    // Scribe and Oracle are proved by the quantity they moved, not by the
    // presence of their metric families: a registered family carrying only
    // zero-valued samples would satisfy a presence check while proving that
    // this journey never traversed either production owner.
    let accepted = counter_delta(journey, "bifrost_scribe_memtable_rows_inserted_total", &[]);
    // The counter is process-wide and carries no table label, and this process
    // also publishes its own retained audit history through the same Scribe, so
    // the journey's rows are a floor rather than the whole count. Row-for-row
    // fidelity of this journey's own data is proved by the public reads above,
    // which compare identities and a digest, not a volume.
    assert!(
        accepted >= facts.acknowledged_rows as f64,
        "the production Scribe inserted at least the rows public ingest acknowledged: \
         {accepted} vs {}",
        facts.acknowledged_rows
    );
    let returned = ["interactive", "analytical"]
        .into_iter()
        .map(|class| counter_delta(journey, "oracle_query_rows_total", &[("class", class)]))
        .sum::<f64>();
    assert!(
        (returned - facts.returned_rows as f64).abs() < f64::EPSILON,
        "the production Oracle returned exactly the rows the public reads streamed: \
         {returned} vs {}",
        facts.returned_rows
    );
    assert!(
        facts.acknowledged_rows > 0 && facts.returned_rows > 0,
        "the journey really drove both production owners: {} acknowledged, {} returned",
        facts.acknowledged_rows,
        facts.returned_rows
    );

    // 7. Forge series stay fixed-cardinality: no durable identity leaks into a
    //    label key or value.
    let forbidden_keys = [
        "tenant",
        "data_tenant_id",
        "table",
        "task_id",
        "attempt_id",
        "operation_id",
        "path",
    ];
    let forbidden_values = [facts.task_id.to_string(), facts.attempt_id.to_string()];
    for sample in journey.metrics.iter().chain(recovery.metrics.iter()) {
        if !sample.family.starts_with("bifrost_forge_") {
            continue;
        }
        for (key, value) in &sample.labels {
            assert!(
                !forbidden_keys.contains(&key.as_str()),
                "Forge metric {} carries an unbounded label {key}",
                sample.family
            );
            assert!(
                !forbidden_values.contains(value) && !value.contains('/'),
                "Forge metric {} carries an unbounded label value for {key}",
                sample.family
            );
        }
        assert!(
            APPROVED_FORGE_FAMILIES.contains(&sample.family.as_str()),
            "Forge emitted {} outside the approved public catalog",
            sample.family
        );
    }
}

/// The exact public Forge metric catalog this journey is allowed to observe.
///
/// Membership is asserted against every captured `bifrost_forge_` sample so a
/// family reintroduced outside the catalog fails here rather than reaching an
/// operator dashboard.
const APPROVED_FORGE_FAMILIES: &[&str] = &[
    "bifrost_forge_tasks_created_total",
    "bifrost_forge_active_tasks",
    "bifrost_forge_task_attempts_total",
    "bifrost_forge_task_duration_seconds",
    "bifrost_forge_task_failures_total",
    "bifrost_forge_input_files_total",
    "bifrost_forge_input_bytes_total",
    "bifrost_forge_output_files_total",
    "bifrost_forge_output_bytes_total",
    "bifrost_forge_deleted_objects_total",
    "bifrost_forge_snapshots_expired_total",
    "bifrost_forge_compaction_debt_files",
    "bifrost_forge_compaction_debt_bytes",
];

/// Promoted rows survive a rewrite whose acceptance the committer never learned.
///
/// The route is the shipped one end to end: two tenants register the same table
/// name, append through authenticated public gRPC, and read back through the
/// public query route. The pod's own Scribe publishes their
/// objects, the pod's own Forge scheduler and worker promote them, and the
/// settled table then plans its own rewrite. That rewrite's commit is accepted
/// by the real catalog which then loses the response, so the worker claims
/// nothing — the operation stays open even though the replacement is already
/// live. The successor recognises its predecessor's own snapshot from retained
/// evidence, settles that one operation, and publishes nothing a second time.
///
/// The customer oracle is the public read, and it is exact: at every one of the
/// four cuts — before promotion, after promotion, across the uncertain commit,
/// and after recovery — each tenant's public read must return the exact
/// `(batch_id, value)` multiset it acknowledged, with the matching
/// canonical digest, and the neighbouring tenant's identically named table must
/// be neither read, rewritten, nor disturbed.
///
/// Everything after that is correlated platform evidence for *why* the rows
/// stayed exact: the one Iceberg snapshot the uncertain commit really landed,
/// its manifest algebra against the operation's own Prepared promise, the
/// durable Prepared→Recovered settlement of that same operation, and the
/// production telemetry stages carrying those same identities.
///
/// # Panics
///
/// Panics when the cluster cannot start, a public append or read fails, a
/// production attempt misses its diagnostic bound, a tenant observes another
/// tenant's rows, the landed snapshot does not match the operation that
/// promised it, or the uncertain commit is settled or republished more than
/// once.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn forge_promoted_files_rewrite_and_remain_exact_across_recovery() {
    let (_telemetry_guard, telemetry) =
        shared_process_telemetry_for_test().expect("process production telemetry");
    let checkpoint = telemetry
        .checkpoint()
        .expect("production telemetry baseline");
    // The pod's maintenance interval paces its scheduler passes. It has to be
    // long enough that every phase below observes the uncertain state it arms,
    // and short enough that the recovery phase can wait for one pass rather
    // than for a redeploy.
    let cluster =
        WyrdTestCluster::start_embedded_forge_uncertainty_for_test(Duration::from_secs(30))
            .await
            .expect("one bound embedded Bifrost pod starts");
    cluster.lead_forge_for_test().await;
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
    // The owner's table compacts on RisingWave's interval rule alone, so its
    // promotions accumulate commits and the rewrite becomes due only when this
    // journey moves the Forge clock past the interval, after arming uncertainty.
    set_table_properties(
        server,
        &shared.binding,
        &[
            ("wyrd.forge.enable-compaction", "true"),
            ("wyrd.forge.compaction-interval-sec", REWRITE_INTERVAL_SECS),
        ],
    )
    .await;
    assert_eq!(
        shared.qualified, neighbour_shared.qualified,
        "both tenants must be registering the identical table name"
    );
    assert_ne!(
        shared.binding.object_prefix, neighbour_shared.binding.object_prefix,
        "one logical name must resolve to two disjoint physical tables"
    );
    let neighbour_only = register_table(server, neighbour, &unique_table("neighbour_only")).await;
    let owner_client = tenant_client(server, owner).await;
    let neighbour_client = tenant_client(server, neighbour).await;

    // Every batch identity is constructed here, before the server has said
    // anything, so the expected rows are the caller's own facts rather than
    // something read back out of the system under test.
    let mut owner_expected: Vec<ManagedRow> = Vec::new();
    let owner_values: Vec<i64> = (0..24).collect();
    let neighbour_values: Vec<i64> = (1_000..1_024).collect();
    // Each neighbour table has one file, so it owes no independent small-file
    // rewrite while we assert its exact cut survives the owner's recovery.
    let neighbour_shared_expected = canonical_order(
        append_values(
            &neighbour_client,
            &shared.qualified,
            Uuid::now_v7(),
            &neighbour_values,
        )
        .await,
    );
    let neighbour_only_expected = canonical_order(
        append_values(
            &neighbour_client,
            &neighbour_only.qualified,
            Uuid::now_v7(),
            &neighbour_values,
        )
        .await,
    );
    for half in 0..2 {
        let span = 12 * half..12 * (half + 1);
        owner_expected.extend(
            append_values(
                &owner_client,
                &shared.qualified,
                Uuid::now_v7(),
                &owner_values[span],
            )
            .await,
        );
        server
            .flush_bifrost()
            .await
            .expect("the pod publishes its staged rows");
    }
    owner_expected = canonical_order(owner_expected);

    // Cut 1 — before promotion.
    let owner_digest =
        assert_public_rows(&owner_client, &shared, &owner_expected, "before promotion").await;
    assert_public_rows(
        &neighbour_client,
        &neighbour_shared,
        &neighbour_shared_expected,
        "before promotion",
    )
    .await;
    assert_tenant_scoped_not_found(
        &owner_client,
        &neighbour_only.qualified,
        neighbour,
        "before promotion",
    )
    .await;

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

    // Cut 2 — after promotion.
    assert_eq!(
        assert_public_rows(&owner_client, &shared, &owner_expected, "after promotion").await,
        owner_digest,
        "promotion changed which tier owns the rows, not the rows"
    );
    assert_public_rows(
        &neighbour_client,
        &neighbour_shared,
        &neighbour_shared_expected,
        "after promotion",
    )
    .await;
    let (owner_promoted, owner_hot) = file_tiers(&cluster, owner, &shared.name).await;
    assert!(
        owner_promoted >= 2 && owner_hot == 0,
        "the owner's objects all moved to the promoted tier: {owner_promoted}/{owner_hot}"
    );
    let neighbour_before = file_paths(&cluster, neighbour, &shared.name).await;
    let neighbour_cut_before = live_cut(&cluster, &neighbour_shared.binding).await;
    let neighbour_only_cut_before = live_cut(&cluster, &neighbour_only.binding).await;

    // A third accepted round gives the rewrite more than one input to consume.
    owner_expected.extend(
        append_values(
            &owner_client,
            &shared.qualified,
            Uuid::now_v7(),
            &(24..36).collect::<Vec<i64>>(),
        )
        .await,
    );
    owner_expected = canonical_order(owner_expected);
    server
        .flush_bifrost()
        .await
        .expect("the pod publishes the third accepted round");
    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("the third round's partition closes");
    drain_forge_backlog(&cluster, &observer, &[owner, neighbour]).await;
    let promoted_digest = assert_public_rows(
        &owner_client,
        &shared,
        &owner_expected,
        "after the third promotion",
    )
    .await;

    // The complete promoted cut, captured before uncertainty is armed. Every
    // statement about what the rewrite removed and added is made against this.
    let pre_rewrite = live_cut(&cluster, &shared.binding).await;
    let rewrites_before = rewrite_snapshots(&cluster, &shared.binding).await.len();

    uncertainty.fail_after_next_commit();
    let errors_before = observer.returned_errors().len();
    release_retries(&cluster, owner).await;
    server
        .forge_clock()
        .advance(chrono::Duration::days(2))
        .expect("the owner's compaction interval elapses");
    let mut unsettled = Vec::new();
    // Roster discovery can admit legitimate sibling maintenance first, so this
    // waits for *this* owner's unsettled rewrite rather than for any attempt.
    // An attempt that cannot account for its own operation returns nothing: it
    // is released with the operation left open, so the durable open row — not a
    // returned failure — is what says the uncertainty landed.
    let observed = tokio::time::timeout(ATTEMPT_BOUND, async {
        loop {
            unsettled = unsettled_rewrites(&cluster, owner).await;
            tokio::time::sleep(Duration::from_millis(250)).await;
            // The attempt has drained once its open rows stop appearing: every
            // plan reached the refusing seam, and the released attempt left the
            // whole set open for whoever recovers it.
            if !unsettled.is_empty() && unsettled_rewrites(&cluster, owner).await == unsettled {
                break;
            }
        }
    })
    .await;
    if observed.is_err() {
        server.state().shutdown_token.cancel();
        panic!(
            "uncertain rewrite left no open operation; last unsettled={unsettled:?}, attempts={}, errors={:?}, phases={:?}",
            observer.attempts(),
            observer.returned_errors(),
            rewrite_phases(&cluster, owner).await
        );
    }
    assert_eq!(
        observer.returned_errors().len(),
        errors_before,
        "a released attempt returns no failure while its operation is open: {:?}",
        observer.returned_errors()
    );

    // Each of the attempt's plans publishes independently, so an attempt the
    // injected uncertainty stops leaves one operation per plan behind: the one
    // whose commit reached the catalog without a learnable answer, and the
    // siblings that never called it. Exactly one of them may name a snapshot.
    let published_after = rewrite_snapshots(&cluster, &shared.binding).await;
    let landed_uncertain = unsettled
        .iter()
        .copied()
        .filter(|operation| {
            published_after.iter().any(|snapshot| {
                snapshot.summary.get("forge.operation_id") == Some(&operation.to_string())
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        landed_uncertain.len(),
        1,
        "an uncertain rewrite claims no outcome: {:?}",
        observer.returned_errors()
    );
    let uncertain = landed_uncertain[0];
    // Each plan owns its own operation, so the attempt leaves a mixture: the
    // one whose commit landed without a learnable answer stays open, because
    // only proof about that exact operation may close it, while every sibling
    // the attempt can prove never reached the catalog is reset. The open row is
    // therefore identified by operation, not by position.
    let phases = rewrite_phases(&cluster, owner).await;
    assert_eq!(
        rewrite_phase_of(&cluster, uncertain).await.as_deref(),
        Some("prepared"),
        "the operation the pod could not settle is the one it just attempted: {phases:?}"
    );
    assert_eq!(
        phases.iter().filter(|phase| *phase == "prepared").count(),
        unsettled.len(),
        "the release leaves open every operation the owner cannot account for, \
         and exactly one of them named a snapshot: {phases:?}"
    );

    // Cut 3 — across the uncertain commit. The customer answer first; the
    // catalog is only consulted once the rows have already been proven exact.
    assert_eq!(
        assert_public_rows(
            &owner_client,
            &shared,
            &owner_expected,
            "across the uncertain commit"
        )
        .await,
        promoted_digest,
        "a rewrite whose acceptance was never learned changed no row"
    );
    assert_public_rows(
        &neighbour_client,
        &neighbour_shared,
        &neighbour_shared_expected,
        "across the uncertain commit",
    )
    .await;

    // The operation's own immutable promise, and the one snapshot that kept it.
    let prepared = prepared_rewrite(&cluster, uncertain, &shared.binding).await;
    assert_eq!(
        prepared.operation_id, uncertain,
        "the Prepared detail names the operation the pod could not settle"
    );
    let published = rewrite_snapshots(&cluster, &shared.binding).await;
    let landed = published
        .iter()
        .filter(|snapshot| {
            snapshot.summary.get("forge.operation_id") == Some(&uncertain.to_string())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        landed.len(),
        1,
        "the uncertain commit landed exactly one rewrite snapshot: {published:?}"
    );
    let landed = landed[0].clone();
    // One attempt publishes each of its plans under its own operation identity,
    // so the attempt that committed is named by the snapshot rather than being
    // the operation itself. Every later statement about the publishing attempt
    // is made against the identity the snapshot itself carries.
    let landed_attempt = landed
        .summary
        .get("forge.attempt_id")
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(|| panic!("the landed snapshot names its publishing attempt: {landed:?}"));
    assert_ne!(
        landed_attempt, uncertain,
        "an attempt's plan commits under a per-plan operation, not the attempt's own id"
    );
    assert_eq!(
        landed
            .summary
            .get("forge.rewrite.version")
            .map(String::as_str),
        Some("1"),
        "the landed snapshot carries the versioned rewrite property set: {landed:?}"
    );
    assert_eq!(
        landed
            .summary
            .get("forge.rewrite.base_snapshot_id")
            .and_then(|value| value.parse::<i64>().ok()),
        Some(prepared.base_snapshot_id),
        "the snapshot's base is the base the operation derived against"
    );
    assert_eq!(
        landed.parent_snapshot_id,
        Some(prepared.base_snapshot_id),
        "the rewrite committed directly onto its promised base"
    );
    assert_eq!(
        pre_rewrite.snapshot_id, prepared.base_snapshot_id,
        "the promised base is the cut the promotion left current"
    );
    let landed_task = landed
        .summary
        .get("forge.task_id")
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(|| panic!("the landed snapshot names its durable task: {landed:?}"));

    // Three authorities, narrowing: the durable plan bounds what the task was
    // ever allowed to touch; the Prepared detail is the exact promise the
    // attempt re-derived against the live cut and wrote down before calling
    // the catalog; the snapshot is what actually happened. Each must be
    // contained by the one before it, and the last two must be identical.
    // The snapshot's plan hash is the durable task's own bytes, and the other
    // three rewrite fingerprints are the exact evidence this attempt produced.
    let plan_hash = durable_plan_hash(&cluster, landed_task).await;
    assert_eq!(
        landed.summary.get("forge.rewrite.plan_hash"),
        Some(&plan_hash),
        "the landed snapshot carries the durable task's canonical plan hash: {landed:?}"
    );
    let evidence = attempt_evidence(&observer, landed_task, landed_attempt, uncertain);
    assert_eq!(
        landed.summary.get("forge.rewrite.selection_fingerprint"),
        Some(&evidence.selection_fingerprint),
        "the landed snapshot carries the attempt's own selection receipt: {landed:?}"
    );
    assert_eq!(
        landed.summary.get("forge.rewrite.debt_fingerprint"),
        Some(&evidence.debt_fingerprint),
        "the landed snapshot carries the attempt's own debt summary: {landed:?}"
    );
    assert_eq!(
        landed.summary.get("forge.rewrite.policy_fingerprint"),
        Some(&evidence.policy_fingerprint),
        "the landed snapshot carries the attempt's own policy identity: {landed:?}"
    );
    assert_eq!(
        evidence.base_snapshot_id, prepared.base_snapshot_id,
        "the attempt's evidence was resolved against the base it promised"
    );

    let selected = rewrite_task_plan(&cluster, landed_task, &shared.binding).await;
    assert!(
        selected.is_subset(&pre_rewrite.data),
        "the rewrite planned only files the promoted cut held live: {selected:?}"
    );
    assert!(
        !prepared.input_paths.is_empty() && prepared.input_paths.is_subset(&selected),
        "the operation promised to remove a non-empty part of its durable plan: {:?} vs {selected:?}",
        prepared.input_paths
    );
    assert!(
        prepared.input_paths.is_subset(&pre_rewrite.data),
        "the operation promised to remove only files the promoted cut held live: {:?}",
        prepared.input_paths
    );
    assert!(
        prepared.output_paths.is_disjoint(&pre_rewrite.data),
        "the operation promised managed outputs no earlier cut already held: {:?}",
        prepared.output_paths
    );
    assert_eq!(
        landed.removed_data, prepared.input_paths,
        "the snapshot removed exactly the operation's promised inputs"
    );
    assert_eq!(
        landed.added_data, prepared.output_paths,
        "the snapshot added exactly the operation's promised managed outputs"
    );
    assert_eq!(
        landed.removed_deletes,
        BTreeSet::new(),
        "this journey's table carries no deletes, so the rewrite removed none"
    );
    assert_eq!(
        pre_rewrite.deletes,
        BTreeSet::new(),
        "this journey's promoted cut carries no delete attachments"
    );
    assert_eq!(
        landed
            .summary
            .get("forge.rewrite.removed_data_files")
            .and_then(|value| value.parse::<usize>().ok()),
        Some(landed.removed_data.len()),
        "the snapshot's removed count matches its manifests"
    );
    assert_eq!(
        landed
            .summary
            .get("forge.rewrite.added_data_files")
            .and_then(|value| value.parse::<usize>().ok()),
        Some(landed.added_data.len()),
        "the snapshot's added count matches its manifests"
    );
    assert_eq!(
        landed
            .summary
            .get("forge.rewrite.removed_delete_files")
            .and_then(|value| value.parse::<usize>().ok()),
        Some(0),
        "the snapshot's removed-delete count matches its empty manifest set"
    );
    assert_eq!(
        landed
            .summary
            .get("forge.rewrite.retained_delete_files")
            .and_then(|value| value.parse::<usize>().ok()),
        Some(0),
        "the snapshot retained no delete attachment because there was none"
    );
    assert!(
        landed.removed_bytes > 0 && landed.added_bytes > 0,
        "the rewrite really moved volume: {} removed, {} added",
        landed.removed_bytes,
        landed.added_bytes
    );
    assert_eq!(
        summary_u64(&landed.summary, "forge.rewrite.removed_bytes"),
        landed.removed_bytes,
        "the snapshot's removed byte volume matches its own parent manifests"
    );
    assert_eq!(
        summary_u64(&landed.summary, "forge.rewrite.added_bytes"),
        landed.added_bytes,
        "the snapshot's added byte volume matches its own manifests"
    );

    // The live cut is exactly the algebra the snapshot describes.
    let rewritten_cut = live_cut(&cluster, &shared.binding).await;
    let expected_cut = pre_rewrite
        .data
        .difference(&landed.removed_data)
        .chain(landed.added_data.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        rewritten_cut.data, expected_cut,
        "the live cut is the promoted cut minus the removals plus the additions"
    );
    assert_eq!(
        rewritten_cut.deletes,
        BTreeSet::new(),
        "the rewrite left no delete attachment behind"
    );
    assert_eq!(
        rewritten_cut.snapshot_id, landed.snapshot_id,
        "the rewrite snapshot is the authoritative cut"
    );

    let rewrite_snapshot_id = landed.snapshot_id;
    let rewrites_after_commit = published.len();
    assert_eq!(
        rewrites_after_commit,
        rewrites_before + 1,
        "the uncertain commit added exactly one rewrite snapshot"
    );

    // A second window opens here so the recovered outcome's own volume can be
    // isolated from every rewrite the drain phases legitimately performed.
    let recovery_mark = telemetry
        .checkpoint()
        .expect("production telemetry recovery checkpoint");
    // Recovery may only start once the owner has actually let the attempt go.
    // A release writes nothing durable, so the observer's own record of it is
    // the only signal that this pod is no longer publishing under the claim;
    // lapsing the lease before that would manufacture a second owner for an
    // attempt still in flight, which is not what a lost process leaves.
    tokio::time::timeout(RELEASE_BOUND, async {
        while !observer.released_attempts_for_test().contains(&landed_task) {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .expect("the owner releases the attempt it cannot account for");
    // Recovery is a fresh leader dispatch that reconciles from retained
    // evidence and then publishes its own rewrite, so the lost-response fault
    // ends with the attempt it simulated; left armed, it would refuse that
    // rewrite too.
    uncertainty.resolve_uncertainty();

    let mut settled = false;
    for _ in 0..DRAIN_PASS_BUDGET {
        let target = observer.attempts().saturating_add(1);
        if rewrite_phase(&cluster, uncertain).await != "prepared" {
            settled = true;
            break;
        }
        // The released attempt stopped renewing its claim, so recovery starts
        // from a lapsed lease rather than from anything this process still
        // remembers. The deadline is advanced and the production reclaim pass
        // then takes the task back, exactly as it would after a kill.
        expire_running_claims(&cluster, owner).await;
        server
            .reclaim_expired_forge_attempts_for_test(16)
            .await
            .expect("the production reclaim pass runs");
        release_retries(&cluster, owner).await;
        let _ =
            tokio::time::timeout(ATTEMPT_BOUND, observer.wait_for_attempts_at_least(target)).await;
    }
    assert!(
        settled,
        "the pod never settled its one uncertain operation in {DRAIN_PASS_BUDGET} passes: {:?}",
        observer.returned_errors()
    );

    // The durable recovery transition precedes the retry's release and span
    // export. Observe that task's completion before checking terminal telemetry.
    drain_forge_backlog(&cluster, &observer, &[owner, neighbour]).await;

    // Cut 4 — after recovery. Customer answer first, again.
    assert_eq!(
        assert_public_rows(&owner_client, &shared, &owner_expected, "after recovery").await,
        promoted_digest,
        "recovery returned the rows to nobody's surprise: exactly the same ones"
    );
    assert_public_rows(
        &neighbour_client,
        &neighbour_shared,
        &neighbour_shared_expected,
        "after recovery",
    )
    .await;
    assert_public_rows(
        &neighbour_client,
        &neighbour_only,
        &neighbour_only_expected,
        "after recovery",
    )
    .await;
    assert_tenant_scoped_not_found(
        &owner_client,
        &neighbour_only.qualified,
        neighbour,
        "after recovery",
    )
    .await;

    let audit_resource = forge_audit_resource(&shared.binding);
    assert_eq!(
        landed.summary.get("forge.group"),
        Some(&audit_resource),
        "the landed snapshot is audited under this table's own resource: {landed:?}"
    );
    let phase = rewrite_operation_phase(&cluster, uncertain).await;
    assert_eq!(
        phase, "recovered",
        "the successor settles its predecessor's own operation as recovered"
    );
    // Recovery is the leader's next dispatch: it settles the landed operation
    // from evidence and may then compact the table's current files under its
    // own operation. What it must never do is publish the landed rewrite again.
    let recovered = rewrite_snapshots(&cluster, &shared.binding).await;
    let still = recovered
        .iter()
        .find(|snapshot| snapshot.snapshot_id == rewrite_snapshot_id)
        .expect("the recovered snapshot is the one the uncertain commit landed");
    assert_eq!(
        still.added_data, landed.added_data,
        "the landed rewrite's own output is unchanged by recovery"
    );
    assert!(
        recovered
            .iter()
            .filter(|other| other.snapshot_id != rewrite_snapshot_id)
            .all(|later| later.added_data.is_disjoint(&landed.added_data)
                && later.summary.get("forge.operation_id")
                    != landed.summary.get("forge.operation_id")),
        "recovery never republishes the landed operation: {recovered:?}"
    );
    // Rewrites the recovering dispatch published after the landed one; their
    // volume is counted in the recovery window alongside the recovered one's.
    let successors = recovered
        .iter()
        .filter(|later| {
            published
                .iter()
                .all(|before| before.snapshot_id != later.snapshot_id)
        })
        .collect::<Vec<_>>();

    // Tenant isolation across the whole recovery, at the platform layer too.
    assert_eq!(
        file_paths(&cluster, neighbour, &shared.name).await,
        neighbour_before,
        "the neighbouring tenant's objects were never replaced"
    );
    assert_eq!(
        live_cut(&cluster, &neighbour_shared.binding).await,
        neighbour_cut_before,
        "the neighbour's identically named table kept its exact snapshot and cut"
    );
    assert_eq!(
        live_cut(&cluster, &neighbour_only.binding).await,
        neighbour_only_cut_before,
        "the neighbour-only table kept its exact snapshot and cut"
    );
    assert!(
        neighbour_cut_before.data.is_disjoint(&rewritten_cut.data),
        "the two tenants' objects never overlap"
    );

    let recovery_window = telemetry
        .delta_since(&recovery_mark)
        .expect("production telemetry recovery delta");
    let journey_window = telemetry
        .delta_since(&checkpoint)
        .expect("production telemetry delta");
    eprintln!(
        "evidence forge phase=journey durable_unsettled_tasks={} samples: {}",
        pending_tasks(&cluster).await,
        journey_window.evidence(&FORGE_FAMILIES)
    );
    for span in recovery_window
        .spans
        .iter()
        .filter(|span| span.name.starts_with("bifrost.forge."))
    {
        eprintln!(
            "evidence forge_trace trace={} span={} parent={} name={} duration_nanos={} \
             result={:?} task_type={:?} event_levels={:?}",
            span.trace_id,
            span.span_id,
            span.parent_span_id,
            span.name,
            span.duration_nanos,
            attribute(span, "result"),
            attribute(span, "task_type"),
            span.events
                .iter()
                .filter_map(|event| event.attributes.get("level"))
                .collect::<Vec<_>>()
        );
    }
    eprintln!(
        "evidence forge phase=recovery landed_task={landed_task} attempt={landed_attempt} \
         removed_files={} added_files={} removed_bytes={} added_bytes={} samples: {}",
        landed.removed_data.len(),
        landed.added_data.len(),
        landed.removed_bytes,
        landed.added_bytes,
        recovery_window.evidence(&FORGE_FAMILIES)
    );
    assert_recovery_telemetry(
        &journey_window,
        &recovery_window,
        &RecoveryTelemetry {
            task_id: landed_task,
            task_state: durable_task_state(&cluster, landed_task).await,
            attempt_id: landed_attempt,
            plans: attempt_plan_count(&observer, landed_attempt),
            input_files: (landed.removed_data.len()
                + successors
                    .iter()
                    .map(|s| s.removed_data.len())
                    .sum::<usize>()) as u64,
            output_files: (landed.added_data.len()
                + successors.iter().map(|s| s.added_data.len()).sum::<usize>())
                as u64,
            input_bytes: landed.removed_bytes
                + successors.iter().map(|s| s.removed_bytes).sum::<u64>(),
            output_bytes: landed.added_bytes
                + successors.iter().map(|s| s.added_bytes).sum::<u64>(),
            acknowledged_rows: (owner_expected.len()
                + neighbour_shared_expected.len()
                + neighbour_only_expected.len()) as u64,
            returned_rows: public_rows_returned(),
        },
    );
}

/// Reads the file target every settled Iceberg rewrite of one table recorded.
///
/// The rewrite detail carries the one target the attempt's policy resolved,
/// so this is the durable evidence of what Forge actually compacted toward.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn rewrite_targets(cluster: &WyrdTestCluster, binding: &TenantTableBinding) -> Vec<u64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT (current_detail->>'target_file_size_bytes')::bigint \
         FROM vala.forge_operation_state \
         WHERE data_tenant_id = $1 AND resource = $2 AND family = 'iceberg_rewrite' \
           AND phase = 'committed'",
    )
    .bind(binding.tenant.as_uuid())
    .bind(forge_audit_resource(binding))
    .fetch_all(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge rewrite-target inspection")
    .into_iter()
    .map(|bytes| u64::try_from(bytes).expect("a recorded target is positive"))
    .collect()
}

/// Drives production passes until every table has a committed rewrite.
///
/// Promotion settles first; the rewrite that packs the promoted files is
/// planned by a later pass. Each iteration waits on a completion notification,
/// never a sleep, and the durable rewrite rows are the verdict.
///
/// # Panics
///
/// Panics when a table still has no committed rewrite after the pass budget.
async fn await_committed_rewrites(
    cluster: &WyrdTestCluster,
    observer: &vala_bifrost_redux::forge::ForgeWorkerCompletionObserver,
    bindings: &[&TenantTableBinding],
) {
    for _ in 0..DRAIN_PASS_BUDGET {
        let mut settled = true;
        for binding in bindings {
            settled &= !rewrite_targets(cluster, binding).await.is_empty();
        }
        if settled {
            return;
        }
        for binding in bindings {
            release_retries(cluster, binding.tenant).await;
        }
        let target = observer.attempts().saturating_add(1);
        if pending_tasks(cluster).await == 0 {
            cluster.request_forge_scheduler_pass_for_test();
        }
        let _ =
            tokio::time::timeout(ATTEMPT_BOUND, observer.wait_for_attempts_at_least(target)).await;
    }
    panic!(
        "the pod's own Forge committed no rewrite after {DRAIN_PASS_BUDGET} passes: {:?}",
        observer.returned_errors()
    );
}

/// Reads one table's explicit Iceberg file-target property, if any.
///
/// # Panics
///
/// Panics when the table cannot be loaded or the property is not a byte count.
async fn declared_file_target(
    cluster: &WyrdTestCluster,
    binding: &TenantTableBinding,
) -> Option<u64> {
    cluster
        .server(0)
        .expect("the embedded pod is running")
        .bifrost_catalog()
        .iceberg_catalog()
        .load_table(&binding.table_ident())
        .await
        .expect("the journey table loads through the production catalog")
        .metadata()
        .properties()
        .get("write.target-file-size-bytes")
        .map(|bytes| bytes.parse().expect("the declared target is a byte count"))
}

/// A caller-declared compaction target is stored, described, fenced, and used
/// by Forge, while an undeclared table compacts toward the deployment default.
///
/// Both tables are registered through the public client. The declared table
/// carries its target as its only explicit Iceberg property; re-registering it
/// with the same value or with none is idempotent, and a different value is
/// refused without changing it. Each table then receives two flushed public
/// appends, and the pod's own Forge promotes and rewrites them: the declared
/// table's committed rewrite records the declared target, the undeclared
/// table's records the approximately 1 GiB deployment default, the rewrite
/// outputs replace the promoted inputs, and both read back exactly.
///
/// # Panics
///
/// Panics when the pod cannot start, a public call fails or is not refused as
/// described, Forge leaves work owed, or a target or row set differs.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn compaction_target_registers_describes_and_steers_forge_rewrites() {
    let cluster = WyrdTestCluster::start_with_embedded_forge_observer()
        .await
        .expect("one bound embedded Bifrost pod starts");
    cluster.lead_forge_for_test().await;
    let observer = cluster
        .forge_completion_observer()
        .expect("the journey pod carries a Forge completion observer");
    let server = cluster.server(0).expect("the embedded pod is running");
    let tenant = cluster.data_tenant_id();
    let client = tenant_client(server, tenant).await;
    let target = 256 * 1024 * 1024_u64;
    let schema = std::sync::Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("value", arrow::datatypes::DataType::Int64, false),
    ]));
    let journey_table = |name: &str| JourneyTable {
        qualified: format!("vala.datasets.{name}"),
        name: name.to_owned(),
        binding: TenantTableBinding::resolve((
            tenant,
            vala_bifrost_redux::catalog::TableRef::new(
                vala_bifrost_redux::namespaces::BifrostNamespace::Datasets,
                name,
            ),
        ))
        .expect("the journey table resolves to its physical binding"),
    };
    let declared = journey_table(&unique_table("declared_target"));
    let undeclared = journey_table(&unique_table("default_target"));
    let register = |table: &JourneyTable, bytes: Option<u64>| {
        let config =
            wyrd_client::bifrost::TableConfig::from_arrow(&table.qualified, schema.clone())
                .expect("the journey schema is a table config");
        let config = match bytes {
            Some(bytes) => config.with_compaction_target_file_size_bytes(bytes),
            None => config,
        };
        let client = &client;
        async move {
            wyrd_client::Bifrost::connect_with_table(client, config)
                .await
                .expect("the public Bifrost client connects")
                .register()
                .await
        }
    };
    assert_eq!(
        register(&declared, Some(target))
            .await
            .expect("declared registration"),
        RegisterOutcome::Created
    );
    assert_eq!(
        register(&undeclared, None)
            .await
            .expect("undeclared registration"),
        RegisterOutcome::Created
    );
    enable_compaction(server, &declared.binding).await;
    enable_compaction(server, &undeclared.binding).await;
    let described = |table: &JourneyTable| {
        let client = &client;
        let fqn = table.qualified.clone();
        async move {
            wyrd_client::bifrost::TableConfig::describe(client, &fqn)
                .await
                .expect("the table describes")
                .compaction_target_file_size_bytes()
        }
    };
    assert_eq!(described(&declared).await, Some(target));
    assert_eq!(described(&undeclared).await, None);
    assert_eq!(
        declared_file_target(&cluster, &declared.binding).await,
        Some(target)
    );
    assert_eq!(
        declared_file_target(&cluster, &undeclared.binding).await,
        None,
        "an omitted target writes no Iceberg property"
    );
    assert_eq!(
        register(&declared, Some(target))
            .await
            .expect("same-value registration"),
        RegisterOutcome::AlreadyExists
    );
    assert_eq!(
        register(&declared, None)
            .await
            .expect("omitted-on-existing registration"),
        RegisterOutcome::AlreadyExists
    );
    let conflict = register(&declared, Some(target * 2))
        .await
        .expect_err("a different target is refused");
    let wyrd_client::bifrost::BifrostClientError::Transport(conflict) = conflict else {
        panic!("the conflict is a typed server refusal: {conflict}");
    };
    assert_eq!(
        (conflict.code(), conflict.status()),
        ("WYRD_VALA_409_BIFROST_COMPACTION_TARGET_MISMATCH", 409),
        "the conflict is the typed mismatch refusal"
    );
    assert_eq!(described(&declared).await, Some(target));
    assert_eq!(
        described(&undeclared).await,
        None,
        "omission on an existing table leaves it undeclared"
    );

    let mut expected = BTreeMap::new();
    for table in [&declared, &undeclared] {
        let mut rows = Vec::new();
        for half in 0..2_i64 {
            let values: Vec<i64> = (half * 8..half * 8 + 8).collect();
            rows.extend(append_values(&client, &table.qualified, Uuid::now_v7(), &values).await);
            server
                .flush_bifrost()
                .await
                .expect("the pod publishes its staged rows");
        }
        expected.insert(table.name.clone(), canonical_order(rows));
    }
    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("the written partition closes");
    drain_forge_backlog(&cluster, &observer, &[tenant]).await;
    let promoted = [
        live_cut(&cluster, &declared.binding).await,
        live_cut(&cluster, &undeclared.binding).await,
    ];
    await_committed_rewrites(
        &cluster,
        &observer,
        &[&declared.binding, &undeclared.binding],
    )
    .await;

    for ((table, recorded), promoted) in [
        (&declared, target),
        (
            &undeclared,
            vala_bifrost_redux::forge::ForgeConfig::default().default_target_file_size_bytes,
        ),
    ]
    .into_iter()
    .zip(promoted)
    {
        let targets = rewrite_targets(&cluster, &table.binding).await;
        assert!(
            !targets.is_empty() && targets.iter().all(|bytes| *bytes == recorded),
            "{} rewrites compact toward {recorded}: {targets:?}",
            table.qualified
        );
        assert_ne!(
            live_cut(&cluster, &table.binding).await.data,
            promoted.data,
            "{} rewrite outputs replaced its promoted inputs",
            table.qualified
        );
        assert_public_rows(&client, table, &expected[&table.name], "after compaction").await;
    }
}

/// A Forge rewrite attempt refused by a full shared memory root fails only that
/// attempt, and the durable task retries once memory returns and publishes one
/// complete rewrite snapshot, never a partial one.
///
/// Two flushed public appends are promoted by the pod's own Forge. The shared
/// Bifrost root is then filled exactly through a Forge view, so the rewrite the
/// next pass plans is refused its governed growth and returns a failure. While
/// the root is full the table's live cut, rewrite snapshots, and committed
/// rewrites are unchanged. Once the occupant drops, the same pod's Forge
/// retries and its committed groups consume every promoted input exactly once,
/// each landing one complete snapshot, and the public read returns exactly the
/// acknowledged rows.
///
/// # Panics
///
/// Panics when the pod cannot start, a public call fails, the full root does
/// not fail a rewrite attempt, the failed attempt publishes anything, or the
/// retry does not consume every promoted input exactly once with exact rows.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn failed_memory_attempt_retries_without_partial_publication() {
    let cluster = WyrdTestCluster::start_with_embedded_forge_observer()
        .await
        .expect("one bound embedded Bifrost pod starts");
    cluster.lead_forge_for_test().await;
    let observer = cluster
        .forge_completion_observer()
        .expect("the journey pod carries a Forge completion observer");
    let server = cluster.server(0).expect("the embedded pod is running");
    let tenant = cluster.data_tenant_id();
    let table = register_table(server, tenant, &unique_table("memory_retry")).await;
    enable_compaction(server, &table.binding).await;
    let client = tenant_client(server, tenant).await;
    let mut expected = Vec::new();
    for half in 0..2_i64 {
        let values: Vec<i64> = (half * 8..half * 8 + 8).collect();
        expected.extend(append_values(&client, &table.qualified, Uuid::now_v7(), &values).await);
        server
            .flush_bifrost()
            .await
            .expect("the pod publishes its staged rows");
    }
    let expected = canonical_order(expected);
    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("the written partition closes");
    drain_forge_backlog(&cluster, &observer, &[tenant]).await;
    assert!(
        rewrite_targets(&cluster, &table.binding).await.is_empty(),
        "promotion alone committed no rewrite yet"
    );
    let promoted = live_cut(&cluster, &table.binding).await;
    let snapshots_before = rewrite_snapshots(&cluster, &table.binding).await.len();

    let occupant = server
        .state()
        .bifrost_resources()
        .and_then(|resources| resources.forge())
        .expect("the embedded pod hosts Forge")
        .occupy_root_for_test();
    let errors_before = observer.returned_errors().len();
    for _ in 0..DRAIN_PASS_BUDGET {
        if observer.returned_errors().len() > errors_before {
            break;
        }
        let target = observer.attempts().saturating_add(1);
        if pending_tasks(&cluster).await == 0 {
            cluster.request_forge_scheduler_pass_for_test();
        }
        let _ =
            tokio::time::timeout(ATTEMPT_BOUND, observer.wait_for_attempts_at_least(target)).await;
    }
    let failures = observer.returned_errors()[errors_before..].to_vec();
    assert!(
        !failures.is_empty(),
        "a full shared root failed no Forge rewrite attempt"
    );
    assert_eq!(
        live_cut(&cluster, &table.binding).await,
        promoted,
        "the refused attempt published nothing: {failures:?}"
    );
    assert_eq!(
        rewrite_snapshots(&cluster, &table.binding).await.len(),
        snapshots_before,
        "the refused attempt landed no rewrite snapshot"
    );
    assert!(
        rewrite_targets(&cluster, &table.binding).await.is_empty(),
        "the refused attempt committed no rewrite"
    );

    drop(occupant);
    await_committed_rewrites(&cluster, &observer, &[&table.binding]).await;
    // The retry may pack the promoted inputs as several independent groups,
    // each committing its own complete snapshot; completeness is that every
    // promoted input is consumed by exactly one committed group.
    let committed: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT current_detail FROM vala.forge_operation_state \
         WHERE data_tenant_id = $1 AND resource = $2 AND family = 'iceberg_rewrite' \
           AND phase = 'committed'",
    )
    .bind(tenant.as_uuid())
    .bind(forge_audit_resource(&table.binding))
    .fetch_all(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge committed-operation inspection");
    let mut consumed = Vec::new();
    for detail in &committed {
        for input in detail["input_paths"].as_array().expect("recorded inputs") {
            consumed.push(canonical_path(
                input.as_str().expect("an input path"),
                &table.binding,
            ));
        }
    }
    let consumed_set: BTreeSet<String> = consumed.iter().cloned().collect();
    assert!(
        !committed.is_empty()
            && consumed.len() == consumed_set.len()
            && consumed_set == promoted.data,
        "the retry consumed every promoted input exactly once: {committed:#?}"
    );
    assert_eq!(
        rewrite_snapshots(&cluster, &table.binding).await.len(),
        snapshots_before + committed.len(),
        "each committed group landed exactly one rewrite snapshot"
    );
    assert!(
        live_cut(&cluster, &table.binding)
            .await
            .data
            .is_disjoint(&promoted.data),
        "no promoted input survives beside the rewrite outputs"
    );
    assert_public_rows(&client, &table, &expected, "after the retried rewrite").await;
}
