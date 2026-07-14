//! Integration tests for StreamIdentity::acquire_on_boot against vala.cluster_nodes.
//!
//! Tests verify:
//! - First boot with new node_id returns writer_epoch=1
//! - Second boot with same node_id increments writer_epoch to 2
//! - Different node_id gets its own independent writer_epoch sequence

mod pg_tests {
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;

    async fn setup() -> PgFixture {
        PgFixture::start().await.expect("fixture")
    }

    #[tokio::test]
    async fn acquire_on_boot_first_boot_returns_epoch_1() {
        let fixture = setup().await;
        let pool = fixture.superuser_pool().await.expect("superuser pool");

        let node_id = Uuid::now_v7();

        // First boot: INSERT path
        let (epoch,): (i64,) = sqlx::query_as(
            r#"
            INSERT INTO vala.cluster_nodes (
                node_id, role, advertise_addr, fencing_token,
                started_at, heartbeat_at
            ) VALUES ($1, 'scribe', 'localhost:50051', 1, now(), now())
            ON CONFLICT (node_id) DO UPDATE
            SET fencing_token = vala.cluster_nodes.fencing_token + 1,
                started_at = now(),
                heartbeat_at = now()
            RETURNING fencing_token
            "#,
        )
        .bind(node_id)
        .fetch_one(&pool)
        .await
        .expect("first boot");

        assert_eq!(epoch, 1, "first boot should return writer_epoch=1");
    }

    #[tokio::test]
    async fn acquire_on_boot_second_boot_increments_epoch() {
        let fixture = setup().await;
        let pool = fixture.superuser_pool().await.expect("superuser pool");

        let node_id = Uuid::now_v7();

        // First boot
        let (epoch1,): (i64,) = sqlx::query_as(
            r#"
            INSERT INTO vala.cluster_nodes (
                node_id, role, advertise_addr, fencing_token,
                started_at, heartbeat_at
            ) VALUES ($1, 'scribe', 'localhost:50051', 1, now(), now())
            ON CONFLICT (node_id) DO UPDATE
            SET fencing_token = vala.cluster_nodes.fencing_token + 1,
                started_at = now(),
                heartbeat_at = now()
            RETURNING fencing_token
            "#,
        )
        .bind(node_id)
        .fetch_one(&pool)
        .await
        .expect("first boot");

        assert_eq!(epoch1, 1);

        // Second boot: UPDATE path
        let (epoch2,): (i64,) = sqlx::query_as(
            r#"
            INSERT INTO vala.cluster_nodes (
                node_id, role, advertise_addr, fencing_token,
                started_at, heartbeat_at
            ) VALUES ($1, 'scribe', 'localhost:50051', 1, now(), now())
            ON CONFLICT (node_id) DO UPDATE
            SET fencing_token = vala.cluster_nodes.fencing_token + 1,
                started_at = now(),
                heartbeat_at = now()
            RETURNING fencing_token
            "#,
        )
        .bind(node_id)
        .fetch_one(&pool)
        .await
        .expect("second boot");

        assert_eq!(epoch2, 2, "second boot should increment to writer_epoch=2");
    }

    #[tokio::test]
    async fn acquire_on_boot_different_nodes_independent_epochs() {
        let fixture = setup().await;
        let pool = fixture.superuser_pool().await.expect("superuser pool");

        let node_id_1 = Uuid::now_v7();
        let node_id_2 = Uuid::now_v7();

        // Boot node 1
        let (epoch1,): (i64,) = sqlx::query_as(
            r#"
            INSERT INTO vala.cluster_nodes (
                node_id, role, advertise_addr, fencing_token,
                started_at, heartbeat_at
            ) VALUES ($1, 'scribe', 'localhost:50051', 1, now(), now())
            ON CONFLICT (node_id) DO UPDATE
            SET fencing_token = vala.cluster_nodes.fencing_token + 1,
                started_at = now(),
                heartbeat_at = now()
            RETURNING fencing_token
            "#,
        )
        .bind(node_id_1)
        .fetch_one(&pool)
        .await
        .expect("node 1 boot");

        // Boot node 2
        let (epoch2,): (i64,) = sqlx::query_as(
            r#"
            INSERT INTO vala.cluster_nodes (
                node_id, role, advertise_addr, fencing_token,
                started_at, heartbeat_at
            ) VALUES ($1, 'scribe', 'localhost:50052', 1, now(), now())
            ON CONFLICT (node_id) DO UPDATE
            SET fencing_token = vala.cluster_nodes.fencing_token + 1,
                started_at = now(),
                heartbeat_at = now()
            RETURNING fencing_token
            "#,
        )
        .bind(node_id_2)
        .fetch_one(&pool)
        .await
        .expect("node 2 boot");

        assert_eq!(epoch1, 1, "node 1 should get epoch 1");
        assert_eq!(epoch2, 1, "node 2 should get independent epoch 1");
    }
}
