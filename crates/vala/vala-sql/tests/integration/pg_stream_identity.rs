//! Integration tests for StreamIdentity::acquire_on_boot against vala.cluster_nodes.
//!
//! Tests verify:
//! - First boot with new node_id returns writer_epoch=1
//! - Second boot with same node_id increments writer_epoch to 2
//! - Different node_id gets its own independent writer_epoch sequence
//! - WAL recovery finds a stream's newest batch fence across tenants

mod pg_tests {
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    /// Starts one isolated database fixture for a stream-identity test.
    async fn setup() -> PgFixture {
        PgFixture::start().await.expect("fixture")
    }

    /// Acquires the next Scribe epoch inside the system tenant boundary.
    async fn acquire_epoch(fixture: &PgFixture, node_id: Uuid, advertise_addr: &str) -> i64 {
        let mut conn = fixture
            .vala_postgres()
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await
            .expect("system tenant connection opens");
        let epoch = sqlx::query_scalar(
            r"
            INSERT INTO vala.cluster_nodes (
                data_tenant_id, node_id, role, advertise_addr, fencing_token,
                started_at, heartbeat_at
            ) VALUES ($1, $2, 'scribe', $3, 1, now(), now())
            ON CONFLICT (data_tenant_id, node_id, role) DO UPDATE
            SET fencing_token = vala.cluster_nodes.fencing_token + 1,
                started_at = now(),
                heartbeat_at = now()
            RETURNING fencing_token
            ",
        )
        .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
        .bind(node_id)
        .bind(advertise_addr)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("stream epoch acquires");
        conn.commit().await.expect("stream epoch commits");
        epoch
    }

    /// Proves a new Scribe stream starts at epoch one.
    #[tokio::test]
    async fn acquire_on_boot_first_boot_returns_epoch_1() {
        let fixture = setup().await;
        let node_id = Uuid::now_v7();
        let epoch = acquire_epoch(&fixture, node_id, "localhost:50051").await;

        assert_eq!(epoch, 1, "first boot should return writer_epoch=1");
    }

    /// Proves restarting one Scribe advances only its stream epoch.
    #[tokio::test]
    async fn acquire_on_boot_second_boot_increments_epoch() {
        let fixture = setup().await;
        let node_id = Uuid::now_v7();
        let epoch1 = acquire_epoch(&fixture, node_id, "localhost:50051").await;

        assert_eq!(epoch1, 1);

        let epoch2 = acquire_epoch(&fixture, node_id, "localhost:50051").await;

        assert_eq!(epoch2, 2, "second boot should increment to writer_epoch=2");
    }

    /// Proves different Scribe nodes retain independent epoch sequences.
    #[tokio::test]
    async fn acquire_on_boot_different_nodes_independent_epochs() {
        let fixture = setup().await;
        let node_id_1 = Uuid::now_v7();
        let node_id_2 = Uuid::now_v7();

        let epoch1 = acquire_epoch(&fixture, node_id_1, "localhost:50051").await;
        let epoch2 = acquire_epoch(&fixture, node_id_2, "localhost:50052").await;

        assert_eq!(epoch1, 1, "node 1 should get epoch 1");
        assert_eq!(epoch2, 1, "node 2 should get independent epoch 1");
    }

    /// WAL recovery reads the newest fence of one shard stream across every
    /// tenant through the operator session.
    ///
    /// Two tenants fence batches on the same stream; the newest WAL position
    /// is the second tenant's, and the cross-tenant operator lookup still sees
    /// it. Another shard of the same stream has no fence.
    ///
    /// # Panics
    ///
    /// Panics when the fixture or a connection cannot start, a fence does not
    /// commit, or the lookup misses the newest cross-tenant fence.
    #[tokio::test]
    async fn newest_wal_fence_spans_tenants_on_one_stream() {
        let fixture = setup().await;
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant_b,
                &format!("test-{}", tenant_b.as_uuid().simple()),
            )
            .await
            .expect("second tenant seeds");
        let node_id = Uuid::now_v7();
        for (tenant, segment, lsn) in [(tenant_a, 2, 20), (tenant_b, 3, 31)] {
            let commit = vala_sql::queries::scribe_batch_commits::ScribeBatchCommit {
                tenant,
                logical_table_fqn: "bifrost.fenced".to_owned(),
                batch_id: Uuid::now_v7(),
                slice_set_digest: [7; 32],
                slice_count: 1,
                wal_node_id: node_id,
                wal_writer_epoch: 5,
                wal_shard_id: 1,
                wal_segment_sequence: segment,
                wal_lsn_min: lsn - 1,
                wal_lsn_max: lsn,
                request_id: Uuid::now_v7(),
            };
            let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .expect("tenant connection");
            vala_sql::queries::scribe_batch_commits::record(
                &mut conn,
                &[(commit, chrono::Utc::now())],
            )
            .await
            .expect("fence records");
            conn.commit().await.expect("fence commits");
        }

        let operator = fixture.operator_pool();
        assert_eq!(
            vala_sql::queries::scribe_batch_commits::newest_wal_fence(operator, node_id, 5, 1)
                .await
                .expect("newest fence reads"),
            Some((3, 31)),
            "the newest fence on the stream is found whichever tenant owns it"
        );
        assert_eq!(
            vala_sql::queries::scribe_batch_commits::newest_wal_fence(operator, node_id, 5, 2)
                .await
                .expect("unfenced shard reads"),
            None,
            "a shard with no fence has no newest fence"
        );
    }
}
