//! The public write-then-read path: single tenant, multitenant,
//! post-compaction, distributed, and with a delayed fsync.
//!
//! Module of the `scribe` group; shared fixtures live in `support.rs`.

use arrow::datatypes::{DataType, Field};
use std::collections::BTreeSet;
use std::time::Duration;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::forge::ForgeLifecycleEvent;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::ScribePublicationEvent;
use vala_sdk::{BifrostGrpcTransport, CollectedQueryLimits, QueryClient};
use wyrd_server::config::BifrostTarget;
use wyrd_spec::DataTenantId;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, BifrostTopology, WyrdTestCluster, full_bifrost_topology,
};

use super::support::*;

#[tokio::test]
#[ignore = "requires the real three-pod Bifrost journey lane"]
async fn public_write_read_journey() {
    let cluster = WyrdTestCluster::start(3, full_bifrost_topology())
        .await
        .expect("three-pod WyrdTestCluster");
    let result = run_closeout_journey(&cluster).await;
    let shutdown = cluster.shutdown().await;
    shutdown.expect("three-pod cluster shutdown");
    result.expect("public Bifrost closeout journey");
}

/// Proves the public write/read contract on a production-configured Forge pod.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn public_write_compact_read_journey() {
    run_topology_public_journey(BifrostTopology::OnePod, true)
        .await
        .expect("public write/compact/read journey");
}

/// Proves that the public journey remains tenant-bound across compaction setup.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn public_multitenant_write_compact_read_journey() {
    run_multitenant_public_journey()
        .await
        .expect("public multitenant write/compact/read journey");
}

/// Proves public writes and reads on the exact role-separated six-node matrix.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn distributed_write_compact_read_journey() {
    run_topology_public_journey(BifrostTopology::ThreeServersThreeForgeWorkers, true)
        .await
        .expect("distributed write/compact/read journey");
}

/// Run the canonical public write/read assertions for a named real topology.
async fn run_topology_public_journey(
    topology: BifrostTopology,
    wait_for_forge: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let spec = match topology {
        BifrostTopology::OnePod => BifrostClusterSpec::one_mixed(),
        BifrostTopology::ThreeServersThreeForgeWorkers => {
            BifrostClusterSpec::three_servers_three_forge_workers()
        }
        _ => BifrostClusterSpec::one_mixed(),
    };
    let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(spec).await?;
    let mut server_node_ids = BTreeSet::new();
    let mut forge_worker_node_ids = BTreeSet::new();
    if topology == BifrostTopology::ThreeServersThreeForgeWorkers {
        let roles = cluster
            .servers()
            .map(wyrd_testing::WyrdTestServer::forge_process_role)
            .collect::<Vec<_>>();
        assert_eq!(roles[..3], [BifrostTarget::Server; 3]);
        assert_eq!(roles[3..], [BifrostTarget::ForgeWorker; 3]);
        assert!(
            cluster.servers()[..3]
                .iter()
                .all(|server| server.base_url().is_some() && server.grpc_url().is_some())
        );
        server_node_ids.extend(
            cluster.servers()[..3]
                .iter()
                .map(|server| server.node_id().as_uuid()),
        );
        forge_worker_node_ids.extend(
            cluster.servers()[3..]
                .iter()
                .map(|server| server.node_id().as_uuid()),
        );
        assert!(
            cluster.servers()[3..]
                .iter()
                .all(|server| server.base_url().is_none() && server.grpc_url().is_none())
        );
        assert!(
            cluster.servers()[..3]
                .iter()
                .all(|server| { server.forge_process_role() == BifrostTarget::Server })
        );
    }
    let scribe_observers = cluster
        .servers()
        .filter_map(wyrd_testing::WyrdTestServer::scribe_publication_observer)
        .collect::<Vec<_>>();
    let result = run_closeout_journey(&cluster).await;
    if result.is_ok() && wait_for_forge {
        let before_physical =
            physical_state(&cluster, cluster.data_tenant_id(), TABLE_NAME).await?;
        assert!(
            before_physical.files >= 2,
            "compaction journey requires multiple physical inputs"
        );
        for observer in &scribe_observers {
            tokio::time::timeout(
                Duration::from_secs(30),
                observer
                    .wait_for(|event| matches!(event, ScribePublicationEvent::Published { .. })),
            )
            .await?;
            let events = observer.events();
            assert_scribe_event_sequence(&events);
        }
        for server in cluster.servers() {
            server
                .forge_clock()
                .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))?;
        }
        cluster.request_forge_scheduler_pass_for_test();
        if let Some(observer) = cluster.forge_completion_observer() {
            let events = tokio::time::timeout(
                Duration::from_secs(30),
                observer.wait_for_lifecycle(|events| {
                    events
                        .iter()
                        .any(|event| matches!(event, ForgeLifecycleEvent::Planned { .. }))
                        && events
                            .iter()
                            .any(|event| matches!(event, ForgeLifecycleEvent::Claimed { .. }))
                        && events
                            .iter()
                            .any(|event| matches!(event, ForgeLifecycleEvent::Rewritten { .. }))
                        && events.iter().any(|event| {
                            matches!(event, ForgeLifecycleEvent::CatalogCommitted { .. })
                        })
                        && events
                            .iter()
                            .any(|event| matches!(event, ForgeLifecycleEvent::Terminal { .. }))
                }),
            )
            .await
            .map_err(|_| "Forge lifecycle observer timed out")?;
            let attribution = assert_forge_event_sequence(&events, None);
            if topology == BifrostTopology::ThreeServersThreeForgeWorkers {
                assert!(forge_worker_node_ids.contains(&attribution.claimed_worker));
                assert!(forge_worker_node_ids.contains(&attribution.terminal_worker));
                assert!(!server_node_ids.contains(&attribution.claimed_worker));
                assert!(!server_node_ids.contains(&attribution.terminal_worker));
            }
        }
        let after_physical = physical_state(&cluster, cluster.data_tenant_id(), TABLE_NAME).await?;
        assert_eq!(after_physical.rows, before_physical.rows);
        assert!(after_physical.compacted > before_physical.compacted);
        assert!(
            after_physical.files - after_physical.compacted
                < before_physical.files - before_physical.compacted
        );
        assert_eq!(after_physical.snapshot_count, 1);
        let reader_index = usize::from(topology == BifrostTopology::ThreeServersThreeForgeWorkers);
        let server = cluster
            .server(reader_index)
            .ok_or("missing post-Forge reader")?;
        let reader = bootstrap_client(server, "post-forge-reader", &["admin"]).await?;
        let post_forge = QueryClient::new(&reader)
            .collect_bounded(
                &closeout_query(),
                CollectedQueryLimits {
                    max_rows: 1_024,
                    max_encoded_bytes: 8 * 1024 * 1024,
                },
            )
            .await?;
        let scribe_count = cluster
            .servers()
            .filter(|server| server.bifrost_scribe().is_some())
            .count();
        let mut expected = vec![1, 2, 3, 4];
        expected.extend((0..scribe_count).map(|pod| 10 + i64::try_from(pod).unwrap_or(i64::MAX)));
        expected.push(99);
        assert_query_result(&post_forge, &expected);
        if let Some(observer) = cluster.forge_completion_observer() {
            let terminal_count = observer
                .lifecycle_events()
                .iter()
                .filter(|event| matches!(event, ForgeLifecycleEvent::Terminal { .. }))
                .count();
            for server in cluster
                .servers()
                .filter(|server| server.forge_process_role() != BifrostTarget::ForgeWorker)
            {
                let expected_passes = server.completed_forge_scheduler_passes_for_test() + 1;
                server.request_forge_scheduler_pass_for_test();
                tokio::time::timeout(
                    Duration::from_secs(30),
                    server.wait_for_forge_scheduler_passes_for_test(expected_passes),
                )
                .await?;
            }
            assert_eq!(
                physical_state(&cluster, cluster.data_tenant_id(), TABLE_NAME).await?,
                after_physical
            );
            assert_eq!(
                observer
                    .lifecycle_events()
                    .iter()
                    .filter(|event| matches!(event, ForgeLifecycleEvent::Terminal { .. }))
                    .count(),
                terminal_count,
                "a converged second scheduler pass must not execute another rewrite"
            );
        }
        if topology == BifrostTopology::ThreeServersThreeForgeWorkers {
            let server_a = cluster.server(0).ok_or("missing Server A")?;
            bootstrap_transport(server_a, "distributed-generation-two", &["admin"])
                .await?
                .send_frame(frame(uuid::Uuid::now_v7().into_bytes(), &[777]))
                .await?;
            server_a.flush_bifrost().await?;
            expected.push(777);
            expected.sort_unstable();
            let server_c = cluster.server(2).ok_or("missing Server C")?;
            let reader_c = bootstrap_client(server_c, "distributed-reader-c", &["admin"]).await?;
            let generation_two = QueryClient::new(&reader_c)
                .collect_bounded(
                    &closeout_query(),
                    CollectedQueryLimits {
                        max_rows: 1_024,
                        max_encoded_bytes: 8 * 1024 * 1024,
                    },
                )
                .await?;
            assert_query_result(&generation_two, &expected);

            let inspection = cluster.oracle_inspection().await?;
            assert_eq!(inspection.memberships.len(), 6);
            assert!(inspection.memberships.iter().all(|row| {
                row.ready
                    && row.fencing_token > 0
                    && cluster.servers()[..3]
                        .iter()
                        .any(|server| server.node_id() == row.node_id)
            }));
            assert_eq!(inspection.forge_active_claims, 0);
            assert_eq!(inspection.forge_active_attempts, 0);
            let role_metrics = cluster.telemetry().snapshot()?;
            for (node_ids, expected_role) in [
                (&server_node_ids, "server"),
                (&forge_worker_node_ids, "forge_worker"),
            ] {
                for node_id in node_ids {
                    let node_id = node_id.to_string();
                    assert!(
                        role_metrics.iter().any(|sample| {
                            sample.family == "bifrost_forge_role_node_started_total"
                                && sample.labels.get("role").map(String::as_str)
                                    == Some(expected_role)
                                && sample.labels.get("node_id").map(String::as_str)
                                    == Some(node_id.as_str())
                                && sample.value >= 1.0
                        }),
                        "node {node_id} lacks {expected_role} production role telemetry"
                    );
                }
            }
        }
        // Wait for every pod's read-audit relay to drain before asserting on the
        // durable read-audit row count. `audit_wal_records` is an exact pending
        // counter, so this converges the outbox without masking a real shortfall.
        let convergence_deadline = std::time::Instant::now() + Duration::from_secs(30);
        let reconciled = loop {
            let inspection = cluster.oracle_inspection().await?;
            if inspection.audit_wal_records == 0 {
                break inspection;
            }
            if std::time::Instant::now() >= convergence_deadline {
                return Err(format!(
                    "read-audit relay did not converge: {} WAL records pending (oldest {:?}) after 30s",
                    inspection.audit_wal_records, inspection.audit_oldest_age,
                )
                .into());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        let expected_reads = if topology == BifrostTopology::ThreeServersThreeForgeWorkers {
            6
        } else {
            3
        };
        assert_eq!(reconciled.read_audit_rows, expected_reads);
        assert!(reconciled.audit_rows >= reconciled.read_audit_rows);
        assert_eq!(reconciled.active_tail_fences, 0);
        for owner in ["scribe", "forge", "oracle"] {
            assert!(
                reconciled
                    .metric_families
                    .iter()
                    .any(|family| family.contains(owner)),
                "journey outcome requires {owner} telemetry"
            );
        }
    }
    wait_forge_quiesce(&cluster).await?;
    assert_drained_shutdown(cluster.shutdown_and_inspect().await?);
    result
}

/// Correlate one public ACK through its exact immutable and published identities.
fn assert_scribe_event_sequence(events: &[ScribePublicationEvent]) {
    let (ack_index, batch_id) = events
        .iter()
        .enumerate()
        .find_map(|(index, event)| match event {
            ScribePublicationEvent::Acknowledged { batch_id, rows } if *rows > 0 => {
                Some((index, *batch_id))
            }
            _ => None,
        })
        .expect("positive Scribe acknowledgement");
    let (sealed_index, seal_id, file_list_row_id) = events
        .iter()
        .enumerate()
        .skip(ack_index + 1)
        .find_map(|(index, event)| match event {
            ScribePublicationEvent::Sealed {
                seal_id,
                file_list_row_id,
                batch_ids,
                wal_lsn_min,
                wal_lsn_max,
            } if batch_ids.contains(&batch_id) && wal_lsn_min <= wal_lsn_max => {
                Some((index, *seal_id, *file_list_row_id))
            }
            _ => None,
        })
        .expect("seal carrying acknowledged batch");
    let published = events
        .iter()
        .skip(sealed_index + 1)
        .any(|event| matches!(event, ScribePublicationEvent::Published {
            seal_id: observed_seal,
            file_list_row_id: observed_file,
            batch_ids,
            ..
        } if *observed_seal == seal_id && *observed_file == file_list_row_id && batch_ids.contains(&batch_id)));
    assert!(
        published,
        "the correlated seal must publish its exact file identity"
    );
}

/// Execute independent public writes and strict reads for two authenticated tenants.
async fn run_multitenant_public_journey() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let spec = BifrostClusterSpec::one_mixed();
    let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(spec).await?;
    let server = cluster.server(0).ok_or("missing multitenant server")?;
    let tenant_a = cluster.data_tenant_id();
    let tenant_b = cluster.add_tenant("public-compact-tenant-b").await?;
    let tenant_tables = [
        (tenant_a, "closeout_events_a", 101_i64),
        (tenant_b, "closeout_events_b", 202_i64),
    ];
    for (tenant, table_name, expected) in tenant_tables {
        let table_fqn = format!("vala.bifrost.{table_name}");
        server
            .create_bifrost_table_for_test(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, table_name),
                user_fields: vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ],
                tenant,
                physical_layout: None,
                audit: None,
            })
            .await?;
        let client = bootstrap_client_for_tenant(server, tenant, "public-tenant-writer").await?;
        let transport = client
            .connect_grpc()
            .await
            .map(|connection| BifrostGrpcTransport::new(connection, Default::default()))?;
        for value in [expected, expected + 1] {
            transport
                .send_frame(frame_for_table(
                    &table_fqn,
                    uuid::Uuid::now_v7().into_bytes(),
                    &[value],
                ))
                .await?;
            server.flush_bifrost_for_tenant(tenant).await?;
        }
        let query = QueryClient::new(&client)
            .collect_bounded(
                &query_for_table(&table_fqn),
                CollectedQueryLimits {
                    max_rows: 16,
                    max_encoded_bytes: 1024 * 1024,
                },
            )
            .await?;
        assert_query_result(&query, &[expected, expected + 1]);
    }
    let tenant_a_client =
        bootstrap_client_for_tenant(server, tenant_a, "wrong-tenant-reader").await?;
    let denied = match QueryClient::new(&tenant_a_client)
        .query(&query_for_table("vala.bifrost.closeout_events_b"))
        .await
    {
        Ok(_) => return Err("tenant A resolved tenant B's table".into()),
        Err(error) => error,
    };
    assert_eq!(denied.status(), 404);
    let mut before_physical = Vec::new();
    for (tenant, table_name, _) in tenant_tables {
        before_physical.push((tenant, physical_state(&cluster, tenant, table_name).await?));
    }
    for server in cluster.servers() {
        server
            .forge_clock()
            .advance(chrono::Duration::days(1) + chrono::Duration::minutes(3))?;
    }
    cluster.request_forge_scheduler_pass_for_test();
    if let Some(observer) = cluster.forge_completion_observer() {
        tokio::time::timeout(
            Duration::from_secs(30),
            observer.wait_for_lifecycle(|events| {
                [tenant_a, tenant_b].iter().all(|tenant| {
                    let task_id = events.iter().find_map(|event| match event {
                        ForgeLifecycleEvent::Planned {
                            task_id,
                            tenant: observed,
                            ..
                        } if observed == tenant => Some(*task_id),
                        _ => None,
                    });
                    task_id.is_some_and(|expected| {
                        events.iter().any(|event| {
                            matches!(event, ForgeLifecycleEvent::Terminal { task_id, .. } if *task_id == expected)
                        })
                    })
                })
            }),
        )
        .await?;
    }
    if let Some(observer) = cluster.forge_completion_observer() {
        let events = observer.lifecycle_events();
        for tenant in [tenant_a, tenant_b] {
            assert_forge_event_sequence(&events, Some(tenant));
        }
    }
    let server = cluster
        .server(0)
        .ok_or("missing multitenant post-Forge reader")?;
    for (tenant, table_name, expected) in tenant_tables {
        let reader = bootstrap_client_for_tenant(server, tenant, "public-tenant-reader").await?;
        let post_forge = QueryClient::new(&reader)
            .collect_bounded(
                &query_for_table(&format!("vala.bifrost.{table_name}")),
                CollectedQueryLimits {
                    max_rows: 16,
                    max_encoded_bytes: 1024 * 1024,
                },
            )
            .await?;
        assert_query_result(&post_forge, &[expected, expected + 1]);
        let before = before_physical
            .iter()
            .find_map(|(observed, state)| (*observed == tenant).then_some(*state))
            .ok_or("missing tenant physical baseline")?;
        let after = physical_state(&cluster, tenant, table_name).await?;
        assert_eq!(after.rows, before.rows);
        assert!(after.compacted > before.compacted);
        assert!(after.files - after.compacted < before.files - before.compacted);
        let mut conn =
            wyrd_sql::TenantConn::acquire(cluster.pg_fixture().app_pool(), tenant).await?;
        let forge_audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM vala.audit_outbox WHERE data_tenant_id = wyrd.current_tenant() AND operation LIKE 'forge.%'",
        )
        .fetch_one(&mut **conn.transaction())
        .await?;
        assert!(
            forge_audits > 0,
            "tenant must retain its own Forge audit trail"
        );
    }
    let inspection = cluster.oracle_inspection().await?;
    assert_eq!(inspection.forge_active_claims, 0);
    assert_eq!(inspection.forge_active_attempts, 0);
    assert!(inspection.audit_rows >= 6);
    assert!(
        inspection
            .metric_families
            .iter()
            .any(|family| family.contains("forge"))
    );
    wait_forge_quiesce(&cluster).await?;
    assert_drained_shutdown(cluster.shutdown_and_inspect().await?);
    Ok(())
}

/// Assert the durable production Forge lifecycle stages after observer completion.
fn assert_forge_event_sequence(
    events: &[ForgeLifecycleEvent],
    tenant: Option<DataTenantId>,
) -> ForgeWorkerAttribution {
    let (planned_index, task_id, inputs) = events
        .iter()
        .enumerate()
        .find_map(|(index, event)| match event {
            ForgeLifecycleEvent::Planned {
                task_id,
                tenant: observed,
                inputs,
                ..
            } if tenant.is_none_or(|expected| expected == *observed) => {
                Some((index, *task_id, inputs))
            }
            _ => None,
        })
        .expect("correlated Forge planned event");
    assert!(
        inputs.len() >= 2,
        "Forge plan must name every eligible input"
    );
    assert!(inputs.windows(2).all(|pair| pair[0] < pair[1]));

    let stage = |after: usize, predicate: &dyn Fn(&ForgeLifecycleEvent) -> bool| {
        events
            .iter()
            .enumerate()
            .skip(after + 1)
            .find_map(|(index, event)| predicate(event).then_some(index))
            .expect("correlated Forge lifecycle stage")
    };
    let claimed = stage(
        planned_index,
        &|event| matches!(event, ForgeLifecycleEvent::Claimed { task_id: observed, .. } if *observed == task_id),
    );
    let claimed_worker = match &events[claimed] {
        ForgeLifecycleEvent::Claimed { worker_id, .. } => *worker_id,
        _ => unreachable!("claimed index selected by typed predicate"),
    };
    let rewritten = stage(
        claimed,
        &|event| matches!(event, ForgeLifecycleEvent::Rewritten { task_id: observed, input_count } if *observed == task_id && *input_count == inputs.len()),
    );
    let committed = stage(
        rewritten,
        &|event| matches!(event, ForgeLifecycleEvent::CatalogCommitted { task_id: observed, snapshot_id } if *observed == task_id && *snapshot_id > 0),
    );
    let terminal = stage(
        committed,
        &|event| matches!(event, ForgeLifecycleEvent::Terminal { task_id: observed, worker_id } if *observed == task_id && *worker_id == claimed_worker),
    );
    assert!(terminal > committed);
    let terminal_worker = match &events[terminal] {
        ForgeLifecycleEvent::Terminal { worker_id, .. } => *worker_id,
        _ => unreachable!("terminal index selected by typed predicate"),
    };
    ForgeWorkerAttribution {
        claimed_worker,
        terminal_worker,
    }
}

/// Exact worker attribution returned by a correlated Forge lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ForgeWorkerAttribution {
    /// Durable owner that claimed the correlated task.
    claimed_worker: uuid::Uuid,
    /// Durable owner that completed the correlated task.
    terminal_worker: uuid::Uuid,
}

/// Durable physical state used only to corroborate public logical equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PhysicalState {
    /// Published Scribe input file count.
    files: i64,
    /// Input files marked replaced by Forge.
    compacted: i64,
    /// Logical rows retained across replacement.
    rows: i64,
    /// Distinct replacement snapshots committed by Forge.
    snapshot_count: i64,
}

/// Read exact file-list replacement evidence for one tenant-bound table.
///
/// # Errors
///
/// Returns a PostgreSQL error when the corroborating durable state is unavailable.
async fn physical_state(
    cluster: &WyrdTestCluster,
    tenant: wyrd_spec::DataTenantId,
    table_name: &str,
) -> Result<PhysicalState, sqlx::Error> {
    let row: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT count(*)::bigint,
                count(*) FILTER (WHERE compacted)::bigint,
                COALESCE(sum(row_count), 0)::bigint,
                count(DISTINCT committed_snapshot_id)::bigint
           FROM vala.file_list
          WHERE data_tenant_id = $1 AND namespace = 'vala.bifrost' AND table_name = $2",
    )
    .bind(tenant.as_uuid())
    .bind(table_name)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await?;
    Ok(PhysicalState {
        files: row.0,
        compacted: row.1,
        rows: row.2,
        snapshot_count: row.3,
    })
}

#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn public_write_read_journey_delayed_fsync_drains_without_loss() {
    let cluster = WyrdTestCluster::start_with_wal_sync_delay(
        1,
        BifrostTopology::OnePod,
        Duration::from_millis(10),
    )
    .await
    .expect("one-pod delayed-fsync WyrdTestCluster");
    let result = run_delayed_fsync_journey(&cluster).await;
    let shutdown = cluster.shutdown().await;
    shutdown.expect("delayed-fsync cluster shutdown");
    result.expect("delayed-fsync Bifrost closeout journey");
}

/// Exercises delayed WAL fsync through flush and exact public query readback.
///
/// The flush boundary must retain all eight acknowledged frames before the
/// terminal-safe query validates their exact ordered contents. Returning early
/// may leave already acknowledged test writes durable until cluster shutdown.
///
/// # Errors
///
/// Returns an error when table creation, client bootstrap, ingest, flush, or
/// bounded query collection fails.
///
/// # Cancellation
///
/// Cancelling this future stops the active operation but does not retract
/// acknowledged WAL appends or completed flush work. The caller retains the
/// cluster and must perform bounded shutdown.
async fn run_delayed_fsync_journey(
    cluster: &WyrdTestCluster,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tenant = cluster.data_tenant_id();
    let server = cluster.server(0).ok_or("missing delayed-fsync pod")?;
    server
        .create_bifrost_table_for_test(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await?;

    let transport = bootstrap_transport(server, "delayed-fsync-admin", &["admin"]).await?;
    for id in 0..8_i64 {
        transport
            .insert_batch(TABLE_FQN, uuid::Uuid::now_v7().into_bytes(), ipc(&[id]))
            .await?;
    }
    server.flush_bifrost().await?;

    let query_client = bootstrap_client(server, "delayed-fsync-query", &["admin"]).await?;
    let query = QueryClient::new(&query_client)
        .collect_bounded(
            &closeout_query(),
            CollectedQueryLimits {
                max_rows: 1_024,
                max_encoded_bytes: 8 * 1024 * 1024,
            },
        )
        .await?;
    assert_query_result(&query, &[0, 1, 2, 3, 4, 5, 6, 7]);
    Ok(())
}
