//! Boot, shard rotation replay, owner drop, and shutdown on a serve-task
//! panic.
//!
//! Module of the `scribe` group; shared fixtures live in `support.rs`.

use arrow::datatypes::{DataType, Field};
use std::time::Duration;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::file_list_writer::PublicationFenceBarrier;
use vala_bifrost_redux::scribe::persistence::PersistenceFaults;
use vala_bifrost_redux::scribe::routing::shard_for;
use vala_sdk::{CollectedQueryLimits, QueryClient};
use wyrd_server::config::BifrostTarget;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_testing::bifrost::{BifrostClusterSpec, BifrostTopology, WyrdTestCluster};

use super::support::*;

const ROTATION_PREFIX_TABLE: &str = "z_rotation_events";
const ROTATION_PREFIX_FQN: &str = "vala.bifrost.z_rotation_events";
const ROTATION_NON_PREFIX_TABLE: &str = "a_rotation_events";
const ROTATION_NON_PREFIX_FQN: &str = "vala.bifrost.a_rotation_events";

/// Proves one fully fenced acknowledgement survives node replacement exactly once.
///
/// The returned ACK is accepted only after its complete WAL slice set and
/// terminal commit are durable, the tenant-scoped control row and canonical
/// audit are committed, and the rows are visible to Scribe. Restart then
/// preserves that exact durable identity through publication and retirement.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn pg_bifrost_scribe_shard_rotation_replay_journey() {
    let faults = PersistenceFaults::default();
    let publication_barrier = PublicationFenceBarrier::for_table(ROTATION_NON_PREFIX_TABLE);
    faults.pause_next_publication(publication_barrier.clone());
    let mut cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(
        BifrostClusterSpec::three_mixed()
            .with_scribe_rotation_for_test(vala_bifrost_redux::scribe::ScribeRotationTestConfig {
                wal_rotation_bytes: 64 * 1024,
                memtable_rotation_bytes: 64 * 1024,
                memtable_max_age: Duration::from_millis(150),
            })
            .with_scribe_persistence_faults_for_test(faults.clone()),
    )
    .await
    .expect("restart journey cluster");
    let server = cluster.server(0).expect("restart journey server");
    let tenant = cluster.data_tenant_id();
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
        .await
        .expect("restart table");
    for table_name in [ROTATION_PREFIX_TABLE, ROTATION_NON_PREFIX_TABLE] {
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
            .await
            .expect("rotation cohort table");
    }
    let writer = bootstrap_transport(server, "restart-writer", &["admin"])
        .await
        .expect("restart writer");
    let (prefix_batch, non_prefix_batch, trigger_batch) =
        same_shard_batch_ids(tenant, ROTATION_PREFIX_TABLE, ROTATION_NON_PREFIX_TABLE);
    let target_shard = shard_for(
        tenant,
        &TableRef::new(BifrostNamespace::Bifrost, ROTATION_PREFIX_TABLE),
        prefix_batch,
    );
    assert_eq!(
        target_shard,
        shard_for(
            tenant,
            &TableRef::new(BifrostNamespace::Bifrost, ROTATION_NON_PREFIX_TABLE),
            non_prefix_batch,
        ),
        "production routing must place both tenant-qualified keys on one shard"
    );
    writer
        .send_frame(frame_for_table(
            ROTATION_PREFIX_FQN,
            prefix_batch.into_bytes(),
            &[101],
        ))
        .await
        .expect("age cohort prefix ACK");
    writer
        .send_frame(frame_for_table(
            ROTATION_NON_PREFIX_FQN,
            non_prefix_batch.into_bytes(),
            &[202],
        ))
        .await
        .expect("age cohort non-prefix ACK");
    tokio::time::sleep(Duration::from_millis(175)).await;
    writer
        .send_frame(frame_for_table(
            ROTATION_PREFIX_FQN,
            trigger_batch.into_bytes(),
            &[303],
        ))
        .await
        .expect("age rotation trigger ACK");
    tokio::time::timeout(
        Duration::from_secs(10),
        publication_barrier.wait_before_publication(),
    )
    .await
    .expect("age cohort publication barrier");
    let hot_reader = bootstrap_client(server, "rotation-hot-reader", &["admin"])
        .await
        .expect("rotation hot reader");
    for (table, expected) in [
        (ROTATION_PREFIX_FQN, vec![101, 303]),
        (ROTATION_NON_PREFIX_FQN, vec![202]),
    ] {
        let hot = QueryClient::new(&hot_reader)
            .collect_bounded(
                &fused_query_for_table(table),
                CollectedQueryLimits {
                    max_rows: 16,
                    max_encoded_bytes: 1024 * 1024,
                },
            )
            .await
            .expect("cohort hot read");
        assert_query_result(&hot, &expected);
    }
    let retained_cohort_wal = server
        .bifrost_scribe()
        .expect("rotation Scribe")
        .wal_bytes_on_disk();
    assert!(
        retained_cohort_wal > 0,
        "cohort WAL remains retained while a member is unpublished"
    );
    publication_barrier.release_before_publication();
    tokio::time::timeout(
        Duration::from_secs(10),
        publication_barrier.wait_after_publication(),
    )
    .await
    .expect("visible-before-retirement barrier");
    let immutable_during_visible_cut = server
        .bifrost_scribe()
        .expect("rotation Scribe")
        .memtable_stats()
        .expect("rotation immutable stats")
        .immutable_rows;
    assert!(
        immutable_during_visible_cut >= 1,
        "publication visibility must precede selected immutable retirement"
    );
    publication_barrier.release_after_publication();
    server.flush_bifrost().await.expect("finish age cohort");
    server
        .bifrost_scribe()
        .expect("rotation Scribe")
        .retire_committed_for_test()
        .await
        .expect("retire completed age cohort");
    for _ in 0..50 {
        if server
            .bifrost_scribe()
            .expect("rotation Scribe")
            .memtable_stats()
            .expect("retired cohort stats")
            .immutable_rows
            == 0
        {
            break;
        }
        server.flush_bifrost().await.expect("converge age cohort");
        server
            .bifrost_scribe()
            .expect("rotation Scribe")
            .retire_committed_for_test()
            .await
            .expect("converge age cohort retirement");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        server
            .bifrost_scribe()
            .expect("rotation Scribe")
            .memtable_stats()
            .expect("retired cohort stats")
            .immutable_rows,
        0,
        "cohort ownership retires only after its final member completes"
    );

    let size_batch = matching_batch_id(tenant, ROTATION_PREFIX_TABLE, target_shard, &[]);
    let size_trigger =
        matching_batch_id(tenant, ROTATION_PREFIX_TABLE, target_shard, &[size_batch]);
    let oversized_ids = (1_000_i64..12_000).collect::<Vec<_>>();
    writer
        .send_frame(frame_for_table(
            ROTATION_PREFIX_FQN,
            size_batch.into_bytes(),
            &oversized_ids,
        ))
        .await
        .expect("size cohort ACK");
    writer
        .send_frame(frame_for_table(
            ROTATION_PREFIX_FQN,
            size_trigger.into_bytes(),
            &[12_001],
        ))
        .await
        .expect("automatic size rotation trigger ACK");
    server.flush_bifrost().await.expect("finish size cohort");
    for _ in 0..100 {
        let published_rows: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(row_count), 0)::bigint
               FROM vala.file_list
              WHERE data_tenant_id=$1 AND namespace='vala.bifrost'
                AND table_name IN ($2, $3)",
        )
        .bind(tenant.as_uuid())
        .bind(ROTATION_PREFIX_TABLE)
        .bind(ROTATION_NON_PREFIX_TABLE)
        .fetch_one(cluster.pg_fixture().operator_pool().pool())
        .await
        .expect("size publication convergence");
        if published_rows == 11_004 {
            break;
        }
        server
            .flush_bifrost()
            .await
            .expect("continue size publication");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut expected_prefix = vec![101, 303];
    expected_prefix.extend(oversized_ids.iter().copied());
    expected_prefix.push(12_001);
    expected_prefix.sort_unstable();
    for (table, expected) in [
        (ROTATION_PREFIX_FQN, expected_prefix),
        (ROTATION_NON_PREFIX_FQN, vec![202]),
    ] {
        let sealed = QueryClient::new(&hot_reader)
            .collect_bounded(
                &query_for_table(table),
                CollectedQueryLimits {
                    max_rows: 16_384,
                    max_encoded_bytes: 8 * 1024 * 1024,
                },
            )
            .await
            .expect("cohort sealed read");
        assert_query_result(&sealed, &expected);
    }
    let cardinality: (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*)::bigint,
                COUNT(DISTINCT id)::bigint,
                COALESCE(SUM(row_count), 0)::bigint
           FROM vala.file_list
          WHERE data_tenant_id=$1
            AND namespace='vala.bifrost'
            AND table_name IN ($2, $3)",
    )
    .bind(tenant.as_uuid())
    .bind(ROTATION_PREFIX_TABLE)
    .bind(ROTATION_NON_PREFIX_TABLE)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("rotation file-list and audit cardinality");
    assert_eq!(
        cardinality.0, cardinality.1,
        "artifact identities are unique"
    );
    assert_eq!(
        cardinality.2, 11_004,
        "every cohort row publishes exactly once"
    );
    let mut cohort_conn = cluster
        .pg_fixture()
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("cohort audit tenant connection");
    let cohort_audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM vala.audit_outbox
          WHERE operation='bifrost.ingest_batch' AND resource IN ($1, $2)",
    )
    .bind(ROTATION_PREFIX_FQN)
    .bind(ROTATION_NON_PREFIX_FQN)
    .fetch_one(&mut **cohort_conn.transaction())
    .await
    .expect("cohort canonical audit cardinality");
    assert_eq!(cohort_audits, 5, "each accepted cohort batch audits once");
    cohort_conn
        .commit()
        .await
        .expect("commit cohort audit inspection");
    for _ in 0..100 {
        server
            .bifrost_scribe()
            .expect("rotation Scribe")
            .retire_committed_for_test()
            .await
            .expect("retire size cohort");
        if server
            .bifrost_scribe()
            .expect("rotation Scribe")
            .memtable_stats()
            .expect("size cohort retirement stats")
            .immutable_rows
            == 0
        {
            break;
        }
        server.flush_bifrost().await.expect("converge size cohort");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        server
            .bifrost_scribe()
            .expect("rotation Scribe")
            .memtable_stats()
            .expect("size cohort retirement stats")
            .immutable_rows,
        0,
        "completed size cohort retires before crash replay begins"
    );
    let batch_id = uuid::Uuid::now_v7();
    let retry_frame = frame(batch_id.into_bytes(), &[707]);
    writer
        .send_frame(retry_frame.clone())
        .await
        .expect("durable restart ACK");
    let before_duplicate = server
        .scribe_inspection_snapshot()
        .expect("pre-duplicate Scribe ownership")
        .ingress_lifecycle;
    let wal_before_duplicate = server
        .bifrost_scribe()
        .expect("pre-duplicate Scribe")
        .wal_bytes_on_disk();
    let visible_before_duplicate = server
        .bifrost_scribe()
        .expect("pre-duplicate Scribe")
        .memtable_stats()
        .expect("pre-duplicate memtable visibility");
    let visible_rows_before_duplicate =
        visible_before_duplicate.writable_rows + visible_before_duplicate.immutable_rows;
    writer
        .send_frame(retry_frame)
        .await
        .expect("duplicate client retry converges");
    let after_duplicate = server
        .scribe_inspection_snapshot()
        .expect("post-duplicate Scribe ownership")
        .ingress_lifecycle;
    assert_eq!(
        after_duplicate.materializations,
        before_duplicate.materializations + 1,
        "the duplicate is preprocessed into one current slice"
    );
    assert!(
        after_duplicate.materialized_bytes > before_duplicate.materialized_bytes,
        "the duplicate slice contributes its measured materialized bytes"
    );
    assert_eq!(
        after_duplicate.transfers, before_duplicate.transfers,
        "duplicate convergence must not append another WAL slice"
    );
    assert_eq!(
        after_duplicate.transferred_bytes, before_duplicate.transferred_bytes,
        "duplicate convergence must not transfer bytes into WAL"
    );
    assert_eq!(
        after_duplicate.reservations,
        before_duplicate.reservations + 1
    );
    assert_eq!(after_duplicate.releases, before_duplicate.releases + 1);
    assert_eq!(
        after_duplicate.shard_transfers,
        before_duplicate.shard_transfers + 1
    );
    let reserved_delta = after_duplicate.reserved_bytes - before_duplicate.reserved_bytes;
    assert!(reserved_delta > 0, "duplicate root reservation is measured");
    assert_eq!(
        after_duplicate.released_bytes - before_duplicate.released_bytes,
        reserved_delta
    );
    assert_eq!(
        after_duplicate.shard_transferred_bytes - before_duplicate.shard_transferred_bytes,
        reserved_delta
    );
    assert_eq!(after_duplicate.active_attempts, 0);
    assert_eq!(after_duplicate.active_reservations, 0);
    assert_eq!(after_duplicate.active_materializations, 0);
    assert_eq!(after_duplicate.active_shard_transfers, 0);
    assert_eq!(
        server
            .bifrost_scribe()
            .expect("post-duplicate Scribe")
            .wal_bytes_on_disk(),
        wal_before_duplicate,
        "duplicate retry adds no WAL record bytes"
    );
    let visible_after_duplicate = server
        .bifrost_scribe()
        .expect("post-duplicate Scribe")
        .memtable_stats()
        .expect("post-duplicate memtable visibility");
    assert_eq!(
        visible_after_duplicate.writable_rows + visible_after_duplicate.immutable_rows,
        visible_rows_before_duplicate,
        "duplicate retry adds no visible row"
    );
    let mut after_duplicate_conn = cluster
        .pg_fixture()
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("post-duplicate tenant connection");
    let durable_after_duplicate: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*)::bigint FROM vala.scribe_batch_commits
              WHERE logical_table_fqn=$1 AND batch_id=$2),
            (SELECT COUNT(*)::bigint FROM vala.audit_outbox
              WHERE operation='bifrost.ingest_batch' AND resource=$1),
            (SELECT COUNT(*)::bigint FROM vala.file_list
              WHERE namespace='vala.bifrost' AND table_name=$3)",
    )
    .bind(TABLE_FQN)
    .bind(batch_id)
    .bind(TABLE_NAME)
    .fetch_one(&mut **after_duplicate_conn.transaction())
    .await
    .expect("post-duplicate durable cardinality");
    after_duplicate_conn
        .commit()
        .await
        .expect("commit post-duplicate inspection");
    assert_eq!(
        durable_after_duplicate,
        (1, 1, 0),
        "duplicate retry adds no SQL fence, audit, or file-list row"
    );
    let node = cluster
        .configured_node_ids()
        .first()
        .copied()
        .expect("restart journey node");
    let mut tenant_conn = cluster
        .pg_fixture()
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("restart journey tenant connection");
    let durable_identity: (
        Vec<u8>,
        i32,
        uuid::Uuid,
        i64,
        i16,
        i64,
        i64,
        i64,
        uuid::Uuid,
    ) = sqlx::query_as(
        "SELECT slice_set_digest, slice_count, wal_node_id, wal_writer_epoch,
                    wal_shard_id, wal_segment_sequence, wal_lsn_min, wal_lsn_max, request_id
               FROM vala.scribe_batch_commits
              WHERE logical_table_fqn=$1 AND batch_id=$2",
    )
    .bind(TABLE_FQN)
    .bind(batch_id)
    .fetch_one(&mut **tenant_conn.transaction())
    .await
    .expect("ACKed batch control fence");
    assert_eq!(durable_identity.0.len(), 32);
    assert!(durable_identity.1 > 0, "ACK requires a nonempty slice set");
    assert_eq!(durable_identity.2, node.as_uuid());
    assert!(durable_identity.3 > 0);
    assert!(durable_identity.4 >= 0);
    assert!(durable_identity.5 >= 0);
    assert!(
        durable_identity.6 < durable_identity.7,
        "SLICE LSN must precede its terminal COMMIT LSN"
    );
    let ingest_audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM vala.audit_outbox
          WHERE request_id=$1 AND operation='bifrost.ingest_batch'
            AND resource=$2",
    )
    .bind(durable_identity.8.to_string())
    .bind(TABLE_FQN)
    .fetch_one(&mut **tenant_conn.transaction())
    .await
    .expect("ACKed batch canonical audit");
    assert_eq!(
        ingest_audits, 1,
        "control fence and canonical audit must commit before ACK"
    );
    let visible = server
        .bifrost_scribe()
        .expect("restart Scribe")
        .memtable_stats()
        .expect("post-ACK memtable visibility");
    assert_eq!(visible.writable_rows + visible.immutable_rows, 1);
    let ownership = server
        .scribe_inspection_snapshot()
        .expect("post-ACK Scribe ownership");
    assert_eq!(ownership.ingress_lifecycle.active_attempts, 0);
    assert_eq!(ownership.ingress_lifecycle.active_reservations, 0);
    assert_eq!(ownership.ingress_lifecycle.active_materializations, 0);
    assert_eq!(ownership.ingress_lifecycle.active_shard_transfers, 0);
    assert_eq!(
        ownership.ingress_lifecycle.active_shard_transferred_bytes,
        0
    );
    assert_eq!(
        ownership.ingress_lifecycle.reservations,
        ownership.ingress_lifecycle.releases
    );
    assert_eq!(
        ownership.ingress_lifecycle.reserved_bytes,
        ownership.ingress_lifecycle.released_bytes
    );
    assert_eq!(
        ownership.ingress_lifecycle.shard_transfers,
        ownership.ingress_lifecycle.reservations
    );
    assert_eq!(
        ownership.ingress_lifecycle.shard_transferred_bytes,
        ownership.ingress_lifecycle.reserved_bytes
    );
    assert!(
        ownership.ingress_lifecycle.transfers > 0
            && ownership.ingress_lifecycle.transfers < ownership.ingress_lifecycle.materializations,
        "WAL transfers are a positive strict subset when a duplicate is preprocessed"
    );
    assert!(
        ownership.ingress_lifecycle.transferred_bytes > 0
            && ownership.ingress_lifecycle.transferred_bytes
                < ownership.ingress_lifecycle.materialized_bytes,
        "duplicate materialized bytes never transfer into WAL"
    );
    assert!(ownership.ingress_lifecycle.succeeded >= 1);
    let unpublished: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(row_count), 0)::bigint FROM vala.file_list
         WHERE namespace = 'vala.bifrost' AND table_name = $1",
    )
    .bind(TABLE_NAME)
    .fetch_one(&mut **tenant_conn.transaction())
    .await
    .expect("restart pre-publication inspection");
    assert_eq!(unpublished, 0, "ACK precedes file-list publication");
    tenant_conn
        .commit()
        .await
        .expect("commit restart pre-replacement inspection");
    let roots = cluster
        .terminate_node_abruptly_for_test(node)
        .await
        .expect("abruptly terminate restart node");
    let wal_sizes = roots
        .wal_root
        .as_deref()
        .map(wal_file_sizes)
        .expect("restart retains a WAL root");
    assert!(
        wal_sizes.iter().any(|&size| size > 64),
        "acknowledged WAL must retain at least one complete record: {wal_sizes:?}"
    );
    let evidence = cluster
        .restart_terminated_node_at_new_address(node, roots.clone())
        .await
        .expect("restart node at a new address");
    assert_eq!(evidence.node_id, node);
    assert_ne!(evidence.http_addr, roots.previous_http_addr);
    assert_ne!(evidence.grpc_addr, roots.previous_grpc_addr);
    assert!(
        evidence
            .writer_epoch
            .zip(evidence.previous_writer_epoch)
            .is_some_and(|(new, old)| new > old),
        "restart must advance the production writer fence"
    );
    let replacement = cluster.server(0).expect("replacement restart server");
    for _ in 0..250 {
        let replayed_rows: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(row_count), 0)::bigint FROM vala.file_list
              WHERE data_tenant_id=$1 AND namespace='vala.bifrost' AND table_name=$2",
        )
        .bind(tenant.as_uuid())
        .bind(TABLE_NAME)
        .fetch_one(cluster.pg_fixture().operator_pool().pool())
        .await
        .expect("replay publication inspection");
        if replayed_rows == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut restarted_tenant_conn = cluster
        .pg_fixture()
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("restarted journey tenant connection");
    let restarted_identity: (
        Vec<u8>,
        i32,
        uuid::Uuid,
        i64,
        i16,
        i64,
        i64,
        i64,
        uuid::Uuid,
    ) = sqlx::query_as(
        "SELECT slice_set_digest, slice_count, wal_node_id, wal_writer_epoch,
                    wal_shard_id, wal_segment_sequence, wal_lsn_min, wal_lsn_max, request_id
               FROM vala.scribe_batch_commits
              WHERE logical_table_fqn=$1 AND batch_id=$2",
    )
    .bind(TABLE_FQN)
    .bind(batch_id)
    .fetch_one(&mut **restarted_tenant_conn.transaction())
    .await
    .expect("restarted batch control fence");
    assert_eq!(
        restarted_identity, durable_identity,
        "retry and restart must preserve every durable batch identity field"
    );
    let reader = bootstrap_client(replacement, "restart-reader", &["admin"])
        .await
        .expect("restart reader");
    let query = QueryClient::new(&reader)
        .collect_bounded(
            &closeout_query(),
            CollectedQueryLimits {
                max_rows: 16,
                max_encoded_bytes: 1024 * 1024,
            },
        )
        .await
        .expect("restart exact-once read");
    assert_query_result(&query, &[707]);
    let unrelated_tenant = cluster
        .add_tenant("rotation-unrelated")
        .await
        .expect("unrelated tenant");
    replacement
        .create_bifrost_table_for_test(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant: unrelated_tenant,
            physical_layout: None,
            audit: None,
        })
        .await
        .expect("unrelated tenant table");
    let unrelated_reader =
        bootstrap_client_for_tenant(replacement, unrelated_tenant, "rotation-unrelated-reader")
            .await
            .expect("unrelated tenant reader");
    let unrelated = QueryClient::new(&unrelated_reader)
        .collect_bounded(
            &closeout_query(),
            CollectedQueryLimits {
                max_rows: 16,
                max_encoded_bytes: 1024 * 1024,
            },
        )
        .await
        .expect("unrelated tenant query");
    assert_eq!(
        unrelated.rows, 0,
        "another tenant cannot observe replayed rows"
    );
    let source_epoch = roots
        .previous_writer_epoch
        .expect("acknowledged source epoch is retained");
    let recovered_identity: (uuid::Uuid, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT node_id, writer_epoch, wal_lsn_min, wal_lsn_max, COUNT(*)::bigint FROM vala.file_list
         WHERE namespace='vala.bifrost' AND table_name=$1
         GROUP BY node_id, writer_epoch, wal_lsn_min, wal_lsn_max",
    )
    .bind(TABLE_NAME)
    .fetch_one(&mut **restarted_tenant_conn.transaction())
    .await
    .expect("recovered source identity");
    assert_eq!(recovered_identity.0, node.as_uuid());
    assert_eq!(recovered_identity.1, source_epoch);
    assert_eq!(recovered_identity.2, durable_identity.6);
    assert!(
        recovered_identity.3 < durable_identity.7,
        "published slice range must precede its terminal COMMIT LSN"
    );
    assert_eq!(recovered_identity.4, 1);
    restarted_tenant_conn
        .commit()
        .await
        .expect("commit restart post-replacement inspection");
    replacement
        .bifrost_scribe()
        .expect("replacement Scribe")
        .retire_committed_for_test()
        .await
        .expect("retire source WAL segment");
    let source_wal_dir = roots
        .wal_root
        .as_deref()
        .expect("retained WAL root")
        .join(node.as_uuid().simple().to_string())
        .join(source_epoch.to_string());
    assert!(
        wal_file_sizes(&source_wal_dir).is_empty(),
        "durably published source WAL epoch must retire"
    );
    wait_forge_quiesce(&cluster)
        .await
        .expect("restart journey Forge quiesce");
    assert_drained_shutdown(
        cluster
            .shutdown_and_inspect()
            .await
            .expect("restart journey shutdown"),
    );
}

/// Collects WAL file sizes below one retained node root for crash evidence.
fn wal_file_sizes(root: &std::path::Path) -> Vec<u64> {
    let mut sizes = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some("wal")
                && let Ok(metadata) = entry.metadata()
            {
                sizes.push(metadata.len());
            }
        }
    }
    sizes
}

#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn fresh_boot_provisions_redux_before_first_write() {
    let cluster = WyrdTestCluster::start(1, BifrostTopology::OnePod)
        .await
        .expect("fresh WyrdTestCluster boot");
    let server = cluster.server(0).expect("booted Bifrost server");
    let tenant = cluster.data_tenant_id();
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
        .await
        .expect("first Redux table resolution after boot");
    let transport = bootstrap_transport(server, "fresh-boot-writer", &["admin"])
        .await
        .expect("first writer after boot");
    transport
        .insert_batch(TABLE_FQN, uuid::Uuid::now_v7().into_bytes(), ipc(&[1]))
        .await
        .expect("first Redux write after boot");
    server.flush_bifrost().await.expect("first write flush");
    let rows: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(row_count), 0)::bigint FROM vala.file_list
         WHERE data_tenant_id = $1 AND namespace = 'vala.bifrost' AND table_name = $2",
    )
    .bind(tenant.as_uuid())
    .bind(TABLE_NAME)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("first Redux file-list read");
    assert_eq!(rows, 1);
    cluster.shutdown().await.expect("cluster shutdown");
}

/// Builds a public Oracle query that includes the exact fenced live tail.
fn fused_query_for_table(table: &str) -> BifrostQueryRequest {
    BifrostQueryRequest {
        sql: format!("SELECT id, value FROM {table} ORDER BY id"),
        visibility: VisibilityMode::Fused,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: None,
    }
}

/// Finds three distinct batch identities whose tenant-qualified table keys route together.
///
/// The search calls the production Scribe hash and therefore remains valid if
/// the routing implementation changes while the fixed shard-count contract is
/// preserved.
///
/// # Panics
///
/// Panics only if the bounded deterministic UUID search cannot find matches,
/// which would violate the fixed sixteen-shard routing contract.
fn same_shard_batch_ids(
    tenant: DataTenantId,
    prefix_table: &str,
    non_prefix_table: &str,
) -> (uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    let prefix = TableRef::new(BifrostNamespace::Bifrost, prefix_table);
    let non_prefix = TableRef::new(BifrostNamespace::Bifrost, non_prefix_table);
    for _ in 0..256 {
        let prefix_id = uuid::Uuid::now_v7();
        let target = shard_for(tenant, &prefix, prefix_id);
        let non_prefix_id = matching_batch_id(tenant, non_prefix_table, target, &[]);
        let trigger_id = matching_batch_id(tenant, prefix_table, target, &[prefix_id]);
        if shard_for(tenant, &non_prefix, non_prefix_id) == target {
            return (prefix_id, non_prefix_id, trigger_id);
        }
    }
    panic!("fixed Scribe routing must yield a same-shard cohort");
}

/// Finds one valid unused UUIDv7 batch identity for a production Scribe shard.
///
/// # Panics
///
/// Panics if no identity maps to `target_shard` in the bounded search, which
/// would contradict the production hash's fixed sixteen-shard topology.
fn matching_batch_id(
    tenant: DataTenantId,
    table_name: &str,
    target_shard: usize,
    excluded: &[uuid::Uuid],
) -> uuid::Uuid {
    let table = TableRef::new(BifrostNamespace::Bifrost, table_name);
    (0..65_280)
        .map(|_| uuid::Uuid::now_v7())
        .find(|candidate| {
            !excluded.contains(candidate) && shard_for(tenant, &table, *candidate) == target_shard
        })
        .expect("fixed Scribe routing must yield a matching batch identity")
}

/// Proves the sole coordination-runtime owner is released without panicking on
/// an async frame, and only after the Scribe role has drained.
///
/// Reproduces the production teardown shape rather than the convenient harness
/// one. In production `BoundServer::run(mut self)` consumes the only `AppState`
/// and returns into `async fn main`, so the final drop of the composed graph
/// lands on an async frame. `WyrdTestServer::shutdown` cannot reproduce that: it
/// routes its own last clone through `spawn_blocking`, a legal blocking context.
/// So this journey uses the in-place seam — cancel, join the serve task, keep
/// the harness — and then drops the harness inside the test's own async frame,
/// which is where a blocking `Runtime::drop` would panic.
///
/// `scribe_drained` comes from the report `BoundServer::run` produced during the
/// real production drain, so the assertion is that the Scribe shutdown path
/// completed before the drop statement below releases the executor. Owner
/// liveness at that point is not asserted; it is a borrow-scope fact.
///
/// # Panics
///
/// Panics when boot, table creation, ingest, or the in-place teardown fails, or
/// when the Scribe role did not report a completed drain.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn coordination_runtime_owner_drops_without_panic() {
    let mut server = wyrd_testing::WyrdTestServer::builder()
        .with_forge_process_role_for_test(BifrostTarget::Scribe)
        .start_bound()
        .await
        .expect("bound Bifrost server with a live Scribe role");
    let tenant = server.data_tenant_id();
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
        .await
        .expect("coordination-runtime journey table");
    let transport = bootstrap_transport(&server, "coordination-runtime-writer", &["admin"])
        .await
        .expect("coordination-runtime journey writer");
    transport
        .insert_batch(
            TABLE_FQN,
            uuid::Uuid::now_v7().into_bytes(),
            ipc(&[1, 2, 3]),
        )
        .await
        .expect("real ingest through the composed Scribe shard lanes");
    drop(transport);

    let report = server
        .cancel_and_join_for_test()
        .await
        .expect("production serve task joins after cancellation");
    assert!(
        report.scribe_drained,
        "the composed Scribe role must complete its bounded drain before teardown"
    );

    // The harness now holds the last `AppState` and the sole coordination-runtime
    // owner. Dropping it here — on this async frame, not through
    // `spawn_blocking` — is the production shape; a `Runtime` reachable from the
    // cloned state graph would abort the test process at this statement.
    drop(server);
}

/// Proves a panicking serve task fails `shutdown()` instead of finishing green.
///
/// The panic unwinds on a `tokio-runtime-worker` thread that libtest never
/// attributes to a test, so a discarded join result lets the run pass while the
/// process printed a panic. This journey forces that exact stack and requires
/// the teardown seam to surface it.
///
/// # Panics
///
/// Panics when the bound server cannot start or when `shutdown()` reports
/// success despite a panicking serve task.
#[tokio::test]
#[ignore = "requires the real Postgres-backed Bifrost journey lane"]
async fn serve_task_panic_fails_shutdown() {
    let server = wyrd_testing::WyrdTestServer::builder()
        .with_forge_process_role_for_test(BifrostTarget::Scribe)
        .with_serve_task_panic_for_test()
        .start_bound()
        .await
        .expect("bound server with an injected serve-task panic");
    let error = server
        .shutdown()
        .await
        .expect_err("a panicking serve task must fail shutdown");
    assert!(
        error.to_string().contains("join failed"),
        "serve-task panic must surface as a join failure, got: {error}"
    );
}
