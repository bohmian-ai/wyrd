//! Supervised ownership: every Forge call path runs under a supervised
//! owner, and attempt terminals settle exactly once.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.

use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{ForgeWorker, ForgeWorkerCompletionObserver, ForgeWorkerConfig};
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_tasks::{
    FORGE_TASK_PAYLOAD_VERSION, ForgeTaskEstimates, ForgeTaskLane, ForgeTaskPlan,
    ForgeTaskStrategy, ForgeTaskTableIdentity, NewForgeTask,
};
use wyrd_testing::WyrdTestServer;

use super::support::*;

/// Rejects private Forge ownership seams from the authoritative journey graph.
///
/// The guard discovers helpers transitively across the journey and bound
/// server/cluster harness, so cross-file indirection cannot hide a private
/// owner seam.
#[test]
fn authoritative_forge_call_tree_uses_only_supervised_owners() {
    let sources = [
        include_str!("support.rs"),
        include_str!("ownership.rs"),
        include_str!("expiry.rs"),
        include_str!("convergence.rs"),
        include_str!("orphan_gc.rs"),
        include_str!("dedicated_roles.rs"),
        include_str!("../../../src/server.rs"),
        include_str!("../../../src/bifrost/cluster.rs"),
        include_str!("../../../src/bifrost/forge_harness.rs"),
    ];
    let roots = [
        "pg_bifrost_forge_attempt_terminal_paths_settle_exactly_once",
        "pg_bifrost_forge_publication_recovery_and_orphan_gc",
        "pg_bifrost_forge_manifest_maintenance_precedes_expiry",
        "pg_bifrost_forge_small_files_converges_without_query_dependency",
    ];
    let forbidden = [
        "Forge::run",
        ".run_once(",
        "ForgeWorker::new",
        "execute_one_for_test",
        "claim_next_for_test",
        "claim_fair_for_test",
        "append_prepared",
        "reconcile_live_replacements_for_test",
        "run_orphan_gc_for_test",
    ];
    let mutation_forbidden = [
        "INSERT INTO vala.forge_tasks",
        "UPDATE vala.forge_tasks",
        "DELETE FROM vala.forge_tasks",
        "INSERT INTO vala.forge_operations",
        "UPDATE vala.forge_operations",
        "DELETE FROM vala.forge_operations",
    ];
    let violations =
        authoritative_forge_violations(&sources, &roots, &forbidden, &mutation_forbidden);
    assert!(
        violations.is_empty(),
        "authoritative Forge graph violations: {violations:?}"
    );
}

/// Traverses the closed journey facade and reports private ownership violations.
///
/// Qualified calls resolve by exact owner, the closed facade's receiver names
/// resolve to their concrete owners, `self` calls remain on the current owner,
/// and bare calls resolve only to module functions. This deliberately refuses
/// name-only cross-owner traversal, so same-named methods cannot alias one
/// another in the authority graph.
fn authoritative_forge_violations(
    sources: &[&str],
    roots: &[&str],
    forbidden: &[&str],
    mutation_forbidden: &[&str],
) -> Vec<String> {
    let local_functions = sources
        .iter()
        .enumerate()
        .flat_map(|(source_index, source)| {
            rust_functions(source)
                .into_iter()
                .map(move |function| (source_index, function))
        })
        .collect::<Vec<_>>();
    let mut pending = roots
        .iter()
        .map(|name| {
            local_functions
                .iter()
                .find(|(_, function)| function.name == *name)
                .cloned()
                .expect("authoritative root has an owner-qualified definition")
        })
        .collect::<Vec<_>>();
    let mut authoritative = std::collections::BTreeSet::new();
    let mut violations = Vec::new();
    while let Some((source_index, function)) = pending.pop() {
        if !authoritative.insert((source_index, function.start)) {
            continue;
        }
        let body = rust_function_body(sources[source_index], &function)
            .expect("guarded function must exist in the authoritative source graph");
        for (callee_source, callee) in &local_functions {
            let qualified = format!("{}::{}(", callee.owner, callee.name);
            let receiver = format!("self.{}(", callee.name);
            let module_call = format!("{}(", callee.name);
            let facade_receiver = authoritative_receiver(&callee.owner)
                .is_some_and(|receiver| body.contains(&format!("{receiver}.{}(", callee.name)));
            let is_called = body.contains(&qualified)
                || (callee.owner == function.owner && body.contains(&receiver))
                || facade_receiver
                || (callee.owner == "module" && body.contains(&module_call));
            if is_called && !authoritative.contains(&(*callee_source, callee.start)) {
                pending.push((*callee_source, callee.clone()));
            }
        }
        for token in forbidden {
            if body.contains(token) {
                violations.push(format!(
                    "{}::{} contains private owner seam {token}",
                    function.owner, function.name
                ));
            }
        }
        for token in mutation_forbidden {
            if body.contains(token) {
                violations.push(format!(
                    "{}::{} mutates durable owner table through {token}",
                    function.owner, function.name
                ));
            }
        }
    }
    violations
}

/// Maps every stateful owner in the closed authoritative facade to its receiver.
fn authoritative_receiver(owner: &str) -> Option<&'static str> {
    match owner {
        "WyrdTestServer" => Some("server"),
        "WyrdTestCluster" => Some("cluster"),
        "ForgeFixture" => Some("fixture"),
        "ForgeObjectStoreControl" => Some("controls"),
        "JourneyMaintenance" => Some("maintenance"),
        _ => None,
    }
}

/// One owner-qualified function definition in an inspected source.
#[derive(Clone, Debug)]
struct RustFunction {
    owner: String,
    name: String,
    start: usize,
}

/// Returns every owner-qualified function and its exact definition offset.
fn rust_functions(source: &str) -> Vec<RustFunction> {
    let impls = rust_impl_ranges(source);
    let mut functions = Vec::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("fn ") {
        let start = offset + relative;
        let tail = &source[start + 3..];
        let name = tail
            .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .next()
            .unwrap_or_default();
        if !name.is_empty() {
            let owner = impls
                .iter()
                .find(|implementation| implementation.start < start && start < implementation.end)
                .map_or("module", |implementation| implementation.owner.as_str())
                .to_owned();
            functions.push(RustFunction {
                owner,
                name: name.to_owned(),
                start,
            });
        }
        offset = start + 3 + name.len();
    }
    functions
}

/// One lexical `impl` block and its concrete owner.
struct RustImplRange {
    /// Concrete type named by the implementation.
    owner: String,
    /// Opening-brace offset of the implementation.
    start: usize,
    /// Closing-brace offset of the implementation.
    end: usize,
}

/// Returns the lexical ranges of concrete inherent implementation blocks.
fn rust_impl_ranges(source: &str) -> Vec<RustImplRange> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("impl ") {
        let declaration = offset + relative;
        let tail = &source[declaration + 5..];
        let owner = tail
            .split(|character: char| character == '{' || character.is_whitespace())
            .next()
            .unwrap_or_default();
        let Some(open_relative) = tail.find('{') else {
            break;
        };
        let open = declaration + 5 + open_relative;
        if !owner.is_empty()
            && let Some(end) = matching_brace(source, open)
        {
            ranges.push(RustImplRange {
                owner: owner.to_owned(),
                start: open,
                end,
            });
        }
        offset = open + 1;
    }
    ranges
}

/// Finds the closing brace paired with `open`.
fn matching_brace(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (relative, byte) in source.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + relative);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract one Rust function body for the static authoritative-owner guard.
///
/// # Panics
///
/// Panics when the named function is absent or its braces are unbalanced.
fn rust_function_body<'a>(source: &'a str, function: &RustFunction) -> Option<&'a str> {
    let open = source[function.start..]
        .find('{')
        .map(|offset| function.start + offset)
        .expect("guarded function must have a body");
    let mut depth = 0_usize;
    for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth
                    .checked_sub(1)
                    .expect("function braces remain balanced");
                if depth == 0 {
                    return Some(&source[open..=open + offset]);
                }
            }
            _ => {}
        }
    }
    panic!("guarded function body must close")
}

/// Proves a same-named method cannot conceal a forbidden call in another owner.
#[test]
fn owner_guard_distinguishes_same_named_methods() {
    let source = "impl Closed { fn start(&self) {} } fn root() { helper(); } fn helper() { Hidden::start(); } impl Allowed { fn start(&self) {} } impl Hidden { fn start(&self) { ForgeWorker::new(); } }";
    let violations =
        authoritative_forge_violations(&[source], &["root"], &["ForgeWorker::new"], &[]);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].starts_with("Hidden::start "));
}

/// Return one rendered staging-fold duration count for a closed task result label.
fn rendered_staging_fold_task_duration_count(rendered: &str, result: &str) -> Option<f64> {
    rendered
        .lines()
        .filter(|line| line.starts_with("bifrost_forge_task_duration_seconds_count{"))
        .find(|line| {
            line.contains("strategy=\"staging_fold\"")
                && line.contains(&format!("result=\"{result}\""))
        })
        .and_then(|line| line.rsplit_once(' ').map(|(_, value)| value))
        .and_then(|value| value.parse().ok())
}

/// Prove a real superseded worker attempt records the durable `cancelled` metric result.
///
/// The stale task returns `Ok(())` after cancellation, so this regression fails if worker
/// telemetry regresses to deriving the metric result from the Rust return value.
///
/// # Panics
///
/// Panics when the real server, scheduler, worker, durable task state, or production Prometheus
/// exposition does not establish the cancellation contract.
#[tokio::test]
async fn superseded_worker_records_cancelled_duration_from_durable_state() {
    superseded_worker_cancelled_settlement_journey().await;
}

/// Drives one real superseded worker through its durable cancelled settlement.
///
/// # Panics
///
/// Panics when cancellation fails to settle the task, audit, or bounded
/// telemetry evidence through the production worker path.
async fn superseded_worker_cancelled_settlement_journey() {
    let (server, telemetry) = start_telemetry_maintenance_server().await;
    let fixture = native_forge_group(&server, "durable_cancelled_metric").await;
    commit_journey_staging_snapshot(&server, &fixture).await;
    let current_snapshot = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("committed staging table")
        .metadata()
        .current_snapshot_id()
        .expect("committed staging snapshot");
    assert_ne!(current_snapshot, 0, "fresh task base must be stale");

    let tasks = ForgeTasks::new(fixture.operator_pool.clone());
    let envelope = journey_envelope(&fixture, 1, 1);
    let task_id = tasks
        .enqueue(&NewForgeTask {
            data_tenant_id: fixture.tenant,
            table_ref: ForgeTaskTableIdentity::new(
                "wyrd-redux",
                &fixture.binding.logical_namespace,
                &fixture.binding.table_name,
            )
            .expect("metric task identity"),
            strategy: ForgeTaskStrategy::StagingFold,
            lane: ForgeTaskLane::Ordinary,
            base_snapshot_id: 0,
            plan: ForgeTaskPlan {
                version: FORGE_TASK_PAYLOAD_VERSION,
                inputs: vec!["telemetry-superseded.parquet".to_owned()],
                parameters: serde_json::json!({"kind":"staging_fold"}),
            },
            plan_hash: [0xA5; 32],
            estimates: ForgeTaskEstimates {
                envelope: Some(envelope),
                files: 1,
                bytes: 1,
                parallelism: envelope.reader_permits,
                memory_bytes: envelope.memory_bytes().expect("journey resident total"),
                spill_bytes: envelope.scratch_bytes().expect("journey scratch total"),
                large_ceiling_bytes: 1,
            },
            ready_at: chrono::Utc::now(),
        })
        .await
        .expect("enqueue stale metric task");
    let before =
        rendered_staging_fold_task_duration_count(&telemetry.render(), "cancelled").unwrap_or(0.0);
    let worker = ForgeWorker::new(
        Arc::clone(&fixture.forge),
        ForgeWorkerConfig::default(),
        uuid::Uuid::now_v7(),
    )
    .expect("validated production worker");
    assert!(
        worker
            .execute_one_for_test(&CancellationToken::new())
            .await
            .expect("execute stale task"),
        "the production worker must claim the stale task"
    );
    let after = rendered_staging_fold_task_duration_count(&telemetry.render(), "cancelled")
        .expect("rendered result=cancelled task duration count");
    assert!(
        after > before,
        "the real superseded completion must render result=cancelled rather than result=succeeded"
    );
    let state: String = sqlx::query_scalar(
        "SELECT state FROM vala.forge_tasks WHERE task_id = $1 AND data_tenant_id = $2",
    )
    .bind(task_id)
    .bind(fixture.tenant.as_uuid())
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("durable cancelled task state");
    assert_eq!(state, "cancelled");
    server.shutdown().await.expect("telemetry server shutdown");
}

/// Proves cancellation, qualified failure, claim expiry, and successor reclaim settle once.
///
/// Each phase drives a real PostgreSQL-backed production worker boundary and
/// asserts durable task, audit, telemetry, publication, and resource-release
/// evidence before its fixture shuts down.
///
/// # Panics
///
/// Panics when any terminal path retains ownership, duplicates publication, or
/// diverges from its exact durable and resource settlement.
#[tokio::test]
#[ignore = "gated journey: real Forge terminal paths and exact settlement"]
async fn pg_bifrost_forge_attempt_terminal_paths_settle_exactly_once() {
    supervised_same_tenant_fifo_journey().await;
    supervised_worker_claim_expiry_journey().await;
    supervised_forge_panic_recovery_scenario().await;
    dedicated_unschedulable_admission_journey().await;
    supervised_temporary_pressure_defers_without_ownership().await;
}

/// Proves two durable ready tasks for one tenant are claimed in FIFO order.
///
/// # Panics
///
/// Panics when owner-backed enqueue, supervised claim observation, or orderly
/// server shutdown fails to preserve the two distinct readiness instants.
async fn supervised_same_tenant_fifo_journey() {
    let observer = ForgeWorkerCompletionObserver::new();
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .with_forge_completion_observer_for_test(observer.clone())
        .start_bound()
        .await
        .expect("bound same-tenant FIFO server");
    let first = native_forge_group(&server, "journey_fifo_first").await;
    let second = native_forge_group(&server, "journey_fifo_second").await;
    let tasks = ForgeTasks::new(first.operator_pool.clone());
    let readiness = chrono::Utc::now();
    let first_id = enqueue_stale_fifo_task(
        &tasks,
        &first,
        readiness - chrono::Duration::seconds(2),
        0x31,
    )
    .await;
    let second_id = enqueue_stale_fifo_task(
        &tasks,
        &second,
        readiness - chrono::Duration::seconds(1),
        0x32,
    )
    .await;
    let events = tokio::time::timeout(
        Duration::from_secs(30),
        observer.wait_for_lifecycle(|events| {
            events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        vala_bifrost_redux::forge::ForgeLifecycleEvent::Claimed { task_id, .. }
                            if *task_id == first_id || *task_id == second_id
                    )
                })
                .count()
                == 2
        }),
    )
    .await
    .expect("two supervised same-tenant claims");
    let claimed = events
        .iter()
        .filter_map(|event| match event {
            vala_bifrost_redux::forge::ForgeLifecycleEvent::Claimed { task_id, .. }
                if *task_id == first_id || *task_id == second_id =>
            {
                Some(*task_id)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        claimed,
        [first_id, second_id],
        "one tenant's durable ready queue must claim by ready_at then task_id"
    );
    server.shutdown().await.expect("same-tenant FIFO shutdown");
}

/// Enqueues one intentionally stale task whose supervised claim settles without publication.
async fn enqueue_stale_fifo_task(
    tasks: &ForgeTasks,
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    ready_at: chrono::DateTime<chrono::Utc>,
    plan_byte: u8,
) -> uuid::Uuid {
    let envelope = journey_envelope(fixture, 1, 1);
    tasks
        .enqueue(&NewForgeTask {
            data_tenant_id: fixture.tenant,
            table_ref: ForgeTaskTableIdentity::new(
                "wyrd-redux",
                &fixture.binding.logical_namespace,
                &fixture.binding.table_name,
            )
            .expect("FIFO task table identity"),
            strategy: ForgeTaskStrategy::StagingFold,
            lane: ForgeTaskLane::Ordinary,
            base_snapshot_id: 1,
            plan: ForgeTaskPlan {
                version: FORGE_TASK_PAYLOAD_VERSION,
                inputs: vec![format!("fifo-{plan_byte}.parquet")],
                parameters: serde_json::json!({"kind":"staging_fold"}),
            },
            plan_hash: [plan_byte; 32],
            estimates: ForgeTaskEstimates {
                envelope: Some(envelope),
                files: 1,
                bytes: 1,
                parallelism: envelope.reader_permits,
                memory_bytes: envelope.memory_bytes().expect("FIFO resident total"),
                spill_bytes: envelope.scratch_bytes().expect("FIFO scratch total"),
                large_ceiling_bytes: 1,
            },
            ready_at,
        })
        .await
        .expect("enqueue same-tenant FIFO task")
}
