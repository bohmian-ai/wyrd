//! Storage ownership: what a composed process owns, and what it does not share.

use std::sync::Arc;

use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

/// One composed node owns exactly one storage owner, and two nodes own two.
///
/// Everything the owner bounds — the node-wide concurrent-request ceiling, the
/// decoded-metadata budget charged to the Oracle memory root, and the single
/// shutdown — is a claim about a *node*. If two co-located roles each composed
/// their own owner, each would believe it alone was spending a ceiling the pair
/// actually shares, and the node would issue twice the concurrent object-store
/// work its configuration allows. If two simulated nodes shared one, a
/// per-node budget would silently become a cluster-wide one and the harness
/// would stop resembling the deployment it stands in for. Pointer identity is
/// the only assertion that separates those two failures from the passing case,
/// because every other observable — policy, backend, warehouse — is identical
/// by construction across nodes that share a test backend.
///
/// The cache allocation is checked alongside it: these nodes serve Oracle, so
/// the owner must have resolved a real metadata budget rather than silently
/// composing a disabled one.
///
/// # Panics
///
/// Panics when the cluster cannot start, when a composed node exposes no
/// storage owner, when co-located roles do not share one, or when two nodes
/// share one.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn each_composed_node_owns_exactly_one_storage_owner() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::two_mixed())
        .await
        .expect("the two-pod mixed cluster starts");

    let owners: Vec<_> = (0..2)
        .map(|index| {
            let server = cluster
                .server(index)
                .unwrap_or_else(|| panic!("node {index} is bound"));
            let state = server.state();
            let owner = state
                .bifrost_storage()
                .unwrap_or_else(|| panic!("node {index} composed a storage owner"));
            // Every co-located role reaches storage through the state's one
            // owner, so a second borrow on the same node must be the same
            // allocation, not an equal-looking copy.
            assert!(
                Arc::ptr_eq(
                    owner,
                    state
                        .bifrost_storage()
                        .expect("the owner is still composed"),
                ),
                "node {index} must publish one owner to every role"
            );
            assert!(
                owner.metadata_cache_enabled(),
                "an Oracle-serving node must resolve a metadata budget"
            );
            assert!(
                owner.policy().metadata_cache_bytes() > 0,
                "an enabled cache must have a nonzero budget"
            );
            Arc::clone(owner)
        })
        .collect();

    assert!(
        !Arc::ptr_eq(&owners[0], &owners[1]),
        "two simulated nodes must not share one storage owner"
    );
}

/// Published Int64 `value` column of one table, ascending, read through Oracle.
///
/// # Errors
///
/// Returns the scheduled-query or column-shape error.
async fn published_values(
    server: &wyrd_testing::WyrdTestServer,
    table: &str,
) -> Result<Vec<i64>, super::query::ServerJourneyError> {
    let mut batches = Vec::new();
    wyrd_server::query::scheduled::ScheduledQueryCaller::new(
        server.state().clone(),
        super::query::scheduled_context(server.data_tenant_id())?,
        tokio_util::sync::CancellationToken::new(),
    )
    .run_with(
        wyrd_spec::vala::api::BifrostQueryRequest {
            sql: format!("SELECT value FROM vala.bifrost.{table} ORDER BY value"),
            deadline_ms: Some(30_000),
        },
        |batch| {
            batches.push(batch);
            Ok(())
        },
    )
    .await?;
    let mut values = Vec::new();
    for batch in &batches {
        let column = batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .ok_or("the value column is not Int64")?;
        values.extend(column.values().iter().copied());
    }
    Ok(values)
}

/// Current `/readyz` status code and JSON body.
///
/// # Errors
///
/// Returns the HTTP or JSON decoding error.
async fn readyz(
    server: &wyrd_testing::WyrdTestServer,
) -> Result<(u16, serde_json::Value), super::query::ServerJourneyError> {
    let base = server.base_url().ok_or("missing HTTP URL")?;
    let response = reqwest::get(format!("{base}/readyz")).await?;
    Ok((response.status().as_u16(), response.json().await?))
}

/// Builder for the combined Scribe+Oracle+Forge target with the Eval worker,
/// over one durable data root so a restart replays the same WAL.
fn combined_target(data_root: &std::path::Path) -> wyrd_testing::WyrdTestServerBuilder {
    wyrd_testing::WyrdTestServer::builder()
        .with_verification_runtime_for_test()
        .with_durable_bifrost_data_root(data_root.to_path_buf())
}

/// A Scribe WAL fault withdraws only Scribe; the combined target keeps serving.
///
/// A WAL sync failure after the record bytes land leaves their durability
/// unknown. That write must get no ACK, and Scribe must refuse every later
/// write, report unready, and drop its cluster fence, while WAL files stay on
/// disk. The same process keeps `/readyz` at 200 because Oracle, Forge, and
/// the verification worker are healthy; the body still names Scribe's fault.
/// Oracle keeps answering published reads. A restart over the same data root
/// replays the WAL before Scribe reports ready and accepts writes again. A
/// shared-governor poison, by contrast, still ends the process.
///
/// # Errors
///
/// Returns the first claim that broke.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_wal_fault_is_role_local() -> Result<(), super::query::ServerJourneyError> {
    let data_root = tempfile::tempdir()?;
    let server = combined_target(data_root.path()).start_bound().await?;
    super::query::await_server_ready(server.base_url().ok_or("missing HTTP URL")?).await?;
    let table = format!("wal_fault_{}", uuid::Uuid::now_v7().simple());
    server
        .create_bifrost_table_for_test(vala_bifrost_redux::catalog::CreateTableRequest {
            table: vala_bifrost_redux::catalog::TableRef::new(
                vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
                &table,
            ),
            user_fields: vec![arrow::datatypes::Field::new(
                "value",
                arrow::datatypes::DataType::Int64,
                false,
            )],
            tenant: server.data_tenant_id(),
            physical_layout: None,
            audit: None,
        })
        .await?;
    let fqn = format!("vala.bifrost.{table}");
    server.seed_bifrost_rows(&fqn, &[1, 2, 3]).await?;

    server.trip_bifrost_wal_sync_fault_for_test()?;
    assert!(
        server.seed_bifrost_rows(&fqn, &[4]).await.is_err(),
        "a write whose WAL sync failed must not be acknowledged"
    );
    let scribe = server
        .state()
        .bifrost
        .scribe()
        .ok_or("the combined target selects Scribe")?;
    assert!(scribe.wal_faulted() && !scribe.is_ready());
    assert!(
        server.seed_bifrost_rows(&fqn, &[5]).await.is_err(),
        "a faulted Scribe admits no later write"
    );

    // The monitor withdraws the fence and the readiness loop republishes; both
    // are asynchronous, so poll the observable outcome within a bound.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let (status, body) = loop {
        let cluster = scribe.cluster();
        cluster.refresh_snapshot().await?;
        let fenced_out = cluster.snapshot().live_scribes().is_empty();
        let (status, body) = readyz(&server).await?;
        if fenced_out && body["checks"]["scribe"]["reason"] == "scribe_wal_faulted" {
            break (status, body);
        }
        if std::time::Instant::now() > deadline {
            return Err(format!("Scribe was not withdrawn: fenced_out={fenced_out} {body}").into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };
    assert_eq!(status, 200, "a combined target stays ready: {body}");
    assert_eq!(body["status"], "ok");
    assert_eq!(body["checks"]["oracle"]["reason"], "ok");
    assert_eq!(body["checks"]["verification"]["reason"], "ok");
    assert_eq!(published_values(&server, &table).await?, vec![1, 2, 3]);
    let wal_root = server
        .scribe_wal_root_for_test()
        .ok_or("the combined target composes a WAL")?;
    assert!(
        std::fs::read_dir(wal_root)?.next().is_some(),
        "a faulted Scribe keeps its WAL files"
    );

    let server = server
        .restart_bound(combined_target(data_root.path()))
        .await?;
    super::query::await_server_ready(server.base_url().ok_or("missing HTTP URL")?).await?;
    let (_, body) = readyz(&server).await?;
    assert_eq!(
        body["checks"]["scribe"]["reason"], "ok",
        "replay precedes ready: {body}"
    );
    server.seed_bifrost_rows(&fqn, &[6]).await?;
    let recovered = published_values(&server, &table).await?;
    assert!(
        recovered.starts_with(&[1, 2, 3]) && recovered.ends_with(&[6]) && !recovered.contains(&5),
        "restart keeps acknowledged rows and serves writes: {recovered:?}"
    );

    server
        .state()
        .bifrost
        .resource_health()
        .ok_or("a data role reports resource health")?
        .poison(vala_bifrost_redux::resources::BifrostResourcePoisonReason::Accounting);
    let terminal = server
        .await_terminal_failure_for_test(std::time::Duration::from_secs(60))
        .await?;
    assert!(
        terminal.contains("bifrost_resource_health"),
        "shared-governor poison stays process-terminal: {terminal}"
    );
    Ok(())
}

/// Every `.wal` segment under `root` with its bytes, sorted by path.
///
/// # Errors
///
/// Returns the directory-walk or read error.
fn wal_segments(
    root: &std::path::Path,
) -> Result<Vec<(std::path::PathBuf, Vec<u8>)>, super::query::ServerJourneyError> {
    let mut segments = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|extension| extension == "wal") {
                let bytes = std::fs::read(&path)?;
                segments.push((path, bytes));
            }
        }
    }
    segments.sort();
    Ok(segments)
}

/// A WAL that fails restart replay leaves only Scribe unready.
///
/// A WAL sync fault keeps the segments on disk; corrupting the final CRC byte
/// of each segment that holds records makes the next boot's replay fail. The
/// restarted combined target must still boot and report `/readyz` 200 with
/// Scribe named unready, keep answering published Oracle reads, refuse writes
/// without an ACK, and leave every WAL byte as it found it for operator
/// repair.
///
/// # Errors
///
/// Returns the first claim that broke.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn failed_wal_replay_is_role_local() -> Result<(), super::query::ServerJourneyError> {
    let data_root = tempfile::tempdir()?;
    let server = combined_target(data_root.path()).start_bound().await?;
    super::query::await_server_ready(server.base_url().ok_or("missing HTTP URL")?).await?;
    let table = format!("wal_replay_{}", uuid::Uuid::now_v7().simple());
    server
        .create_bifrost_table_for_test(vala_bifrost_redux::catalog::CreateTableRequest {
            table: vala_bifrost_redux::catalog::TableRef::new(
                vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
                &table,
            ),
            user_fields: vec![arrow::datatypes::Field::new(
                "value",
                arrow::datatypes::DataType::Int64,
                false,
            )],
            tenant: server.data_tenant_id(),
            physical_layout: None,
            audit: None,
        })
        .await?;
    let fqn = format!("vala.bifrost.{table}");
    server.seed_bifrost_rows(&fqn, &[1, 2, 3]).await?;
    server.trip_bifrost_wal_sync_fault_for_test()?;
    assert!(
        server.seed_bifrost_rows(&fqn, &[4]).await.is_err(),
        "a write whose WAL sync failed must not be acknowledged"
    );
    let wal_root = server
        .scribe_wal_root_for_test()
        .ok_or("the combined target composes a WAL")?
        .to_path_buf();

    let mut corrupted = 0;
    for (path, mut bytes) in wal_segments(&wal_root)? {
        if bytes.len() > 64 {
            let last = bytes.len() - 1;
            bytes[last] ^= 0xff;
            std::fs::write(&path, &bytes)?;
            corrupted += 1;
        }
    }
    assert!(
        corrupted > 0,
        "the faulted Scribe kept segments with records"
    );
    let before = wal_segments(&wal_root)?;

    let server = server
        .restart_bound(combined_target(data_root.path()))
        .await?;
    super::query::await_server_ready(server.base_url().ok_or("missing HTTP URL")?).await?;
    let (status, body) = readyz(&server).await?;
    assert_eq!(status, 200, "the other roles keep the target ready: {body}");
    assert_eq!(
        body["checks"]["scribe"]["reason"], "scribe_wal_faulted",
        "{body}"
    );
    assert_eq!(body["checks"]["oracle"]["reason"], "ok", "{body}");
    assert_eq!(published_values(&server, &table).await?, vec![1, 2, 3]);
    assert!(
        server.seed_bifrost_rows(&fqn, &[5]).await.is_err(),
        "a Scribe whose replay failed admits no write"
    );
    assert_eq!(
        wal_segments(&wal_root)?,
        before,
        "a failed replay leaves every WAL byte for operator repair"
    );
    server.shutdown().await?;
    Ok(())
}

/// Ordinary shutdown joins a dedicated Forge worker's drain before any Bifrost settlement.
///
/// The bound dedicated-worker topology keeps its live worker in the serve
/// handle, so `WyrdTestServer::shutdown` must cancel and join that worker
/// rather than drain Bifrost directly: a direct drain finds Forge supervision
/// still running, takes Bifrost's abort path, and closes the node's storage
/// owner underneath the worker. The worker is held after durably claiming a
/// seeded task, so cancellation leaves it a real drain obligation, releasing
/// that claim to `retryable`. A router-only replica keeps the Postgres fixture
/// alive across the worker's shutdown so the released row stays readable.
///
/// # Errors
///
/// Returns the first startup, seeding, shutdown, or read failure.
///
/// # Panics
///
/// Panics when the worker never claims the seeded task, when shutdown aborted
/// storage before the worker joined, or when the claim was not released.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn dedicated_forge_worker_shutdown_drains_its_claim_before_storage_settles()
-> Result<(), super::query::ServerJourneyError> {
    let observer = vala_bifrost_redux::forge::ForgeWorkerCompletionObserver::new();
    observer.hold_after_claims_for_test(1);
    let worker = wyrd_testing::WyrdTestServer::builder()
        .with_forge_process_role_for_test(wyrd_server::BifrostTarget::ForgeWorker)
        .with_forge_completion_observer_for_test(observer.clone())
        .start_bound()
        .await?;
    let keeper = worker
        .start_replica(wyrd_testing::WyrdTestServer::builder())
        .await?;
    let pool = keeper.pg_fixture().superuser_pool().await?;
    let task_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,\
         namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,\
         estimated_bytes,state,ready_at,next_eligible_at,updated_at) \
         VALUES ($1,$2,'wyrd-redux','vala.bifrost','dedicated_shutdown','small_files',4242,\
         '{\"version\":1,\"inputs\":[\"a.parquet\"],\"parameters\":{\"kind\":\"live_rewrite\"}}'::jsonb,\
         decode(repeat('33',32),'hex'),1,100,'ready',statement_timestamp()-interval '1 hour',\
         statement_timestamp()-interval '1 hour',statement_timestamp())",
    )
    .bind(task_id)
    .bind(uuid::Uuid::from(worker.data_tenant_id()))
    .execute(&pool)
    .await?;
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        observer.wait_for_claims_for_test(),
    )
    .await
    .expect("the dedicated worker durably claims the seeded task");

    let storage = Arc::clone(
        worker
            .state()
            .bifrost_storage()
            .ok_or("the dedicated worker composes a storage owner")?,
    );
    let cancelled = worker.state().shutdown_token.clone();
    let release = tokio::spawn({
        let observer = observer.clone();
        async move {
            cancelled.cancelled().await;
            observer.release_claims_for_test();
        }
    });
    worker.shutdown().await?;
    release.await?;

    assert_eq!(
        storage.inspect().lifecycle,
        vala_bifrost_redux::storage::StorageLifecycle::Open,
        "ordinary shutdown settled storage before the dedicated worker joined"
    );
    let state: String = sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
        .bind(task_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        state, "retryable",
        "the joined worker released its cancelled claim before shutdown returned"
    );
    assert!(
        observer.returned_errors().is_empty(),
        "the worker drained cleanly: {:?}",
        observer.returned_errors()
    );
    keeper.shutdown().await?;
    Ok(())
}

/// Ordinary shutdown of a router-only in-process server settles Bifrost itself.
///
/// No serve task exists to drain Bifrost, so `WyrdTestServer::shutdown`
/// cancels the shared shutdown token and settles Bifrost before the Postgres
/// fixture is dropped. Storage is the last owner that settlement closes, after
/// the Oracle, so a closed storage owner proves the Oracle reader epoch no
/// longer outlives the database.
///
/// # Errors
///
/// Returns the startup or shutdown failure.
///
/// # Panics
///
/// Panics when the token is left uncancelled or storage is left open.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn router_only_shutdown_settles_bifrost_before_fixture_release()
-> Result<(), super::query::ServerJourneyError> {
    let server = wyrd_testing::WyrdTestServer::start_in_process().await?;
    let storage = Arc::clone(
        server
            .state()
            .bifrost_storage()
            .ok_or("the default target composes a storage owner")?,
    );
    let cancelled = server.state().shutdown_token.clone();
    server.shutdown().await?;
    assert!(
        cancelled.is_cancelled(),
        "shutdown cancels the shared token"
    );
    assert_eq!(
        storage.inspect().lifecycle,
        vala_bifrost_redux::storage::StorageLifecycle::Closed,
        "router-only shutdown settles Bifrost before the fixture is released"
    );
    Ok(())
}
