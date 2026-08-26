mod pg_tests {
    //! Integration tests for Scribe cluster-node heartbeat and live-scribe
    //! discovery.
    //!
    //! Verifies (per plan §Invariants):
    //! - Heartbeat cadence updates `heartbeat_at` on this pod's row.
    //! - Heartbeats never bump `fencing_token` (epoch only advances on boot).
    //! - `live_scribes()` filters out rows whose `heartbeat_at` is > 15 s old.
    //! - `live_scribes()` projects `(node_id, writer_epoch, advertise_addr)` —
    //!   the CONTRACTS §8 discovery query shape.
    //!
    //! Skipped when `WYRD_DATABASE_URL` is unset (credential-free default suite).
    //!
    //! Tests use a 200 ms cadence override (production is 5 s) to keep wall
    //! clock small; the loop logic under test is identical either way.

    use std::time::Duration;

    use sqlx::types::Uuid;
    use vala_bifrost_redux::contracts::ScribeError;
    use vala_bifrost_redux::scribe::registry::{ScribeHeartbeat, heartbeat_tick, live_scribes};
    use vala_bifrost_redux::scribe::stream_identity::{NodeId, acquire_on_boot};
    use vala_sql::ValaPostgres;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    const TICK: Duration = Duration::from_millis(200);

    async fn setup() -> (PgFixture, ValaPostgres) {
        let fixture = PgFixture::start().await.expect("fixture");
        let postgres = fixture.vala_postgres().clone();
        (fixture, postgres)
    }

    async fn read_row(
        postgres: &ValaPostgres,
        node_id: NodeId,
    ) -> (i64, chrono::DateTime<chrono::Utc>) {
        let mut conn = postgres
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await
            .expect("system tenant connection");
        let row: (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT fencing_token, heartbeat_at FROM vala.cluster_nodes \
             WHERE data_tenant_id = $1 AND node_id = $2",
        )
        .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
        .bind(node_id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("cluster_nodes row");
        conn.commit().await.expect("commit row read");
        row
    }

    #[tokio::test]
    async fn scribe_heartbeats_cluster_nodes_every_5s() {
        if std::env::var("WYRD_DATABASE_URL").is_err() {
            eprintln!("skipping: WYRD_DATABASE_URL unset");
            return;
        }
        let (_fixture, postgres) = setup().await;
        let node_id = NodeId::generate();
        acquire_on_boot(&postgres, node_id, "scribe", "127.0.0.1:9000")
            .await
            .expect("acquire");

        let (_, initial_heartbeat) = read_row(&postgres, node_id).await;

        let heartbeat = ScribeHeartbeat::start(postgres.clone(), node_id, TICK);

        let mut samples = vec![initial_heartbeat];
        let deadline = std::time::Instant::now() + TICK * 8;
        while std::time::Instant::now() < deadline {
            tokio::time::sleep(TICK / 2).await;
            let (_, latest) = read_row(&postgres, node_id).await;
            if samples.last().copied() != Some(latest) {
                samples.push(latest);
            }
            if samples.len() >= 3 {
                break;
            }
        }
        heartbeat.shutdown();

        assert!(
            samples.len() >= 3,
            "expected at least 2 heartbeat updates producing 3 distinct heartbeat_at values within {}ms; got {samples:?}",
            (TICK * 8).as_millis(),
        );
    }

    #[tokio::test]
    async fn scribe_heartbeat_errors_when_row_missing() {
        let (fixture, postgres) = setup().await;
        let node_id = NodeId::generate();
        acquire_on_boot(&postgres, node_id, "scribe", "127.0.0.1:9003")
            .await
            .expect("acquire");

        let superuser = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query("DELETE FROM vala.cluster_nodes WHERE data_tenant_id = $1 AND node_id = $2")
            .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
            .bind(node_id.as_uuid())
            .execute(&superuser)
            .await
            .expect("delete cluster_nodes row");

        match heartbeat_tick(&postgres, node_id).await {
            Err(ScribeError::Internal { detail }) => {
                assert!(
                    detail.contains("cluster_nodes row missing"),
                    "unexpected missing-row detail: {detail}"
                );
            }
            Ok(()) => panic!("heartbeat_tick must reject a missing cluster_nodes row"),
            Err(other) => panic!("unexpected heartbeat error: {other}"),
        }
    }

    #[tokio::test]
    async fn scribe_heartbeat_does_not_bump_writer_epoch() {
        if std::env::var("WYRD_DATABASE_URL").is_err() {
            eprintln!("skipping: WYRD_DATABASE_URL unset");
            return;
        }
        let (_fixture, postgres) = setup().await;
        let node_id = NodeId::generate();
        let identity = acquire_on_boot(&postgres, node_id, "scribe", "127.0.0.1:9001")
            .await
            .expect("acquire");

        let (epoch_before, _) = read_row(&postgres, node_id).await;
        assert_eq!(
            epoch_before,
            identity.writer_epoch.as_i64(),
            "sanity: acquire_on_boot returned the row's fencing_token",
        );

        let heartbeat = ScribeHeartbeat::start(postgres.clone(), node_id, TICK);
        tokio::time::sleep(TICK * 4).await; // ≥3 heartbeat cycles
        let (epoch_after, _) = read_row(&postgres, node_id).await;
        heartbeat.shutdown();

        assert_eq!(
            epoch_after, epoch_before,
            "heartbeats must NOT bump fencing_token (epoch only advances on boot)",
        );
    }

    #[tokio::test]
    async fn scribe_stale_row_is_not_live() {
        if std::env::var("WYRD_DATABASE_URL").is_err() {
            eprintln!("skipping: WYRD_DATABASE_URL unset");
            return;
        }
        let (_fixture, postgres) = setup().await;
        let stale_node = NodeId::generate();

        let mut conn = postgres
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await
            .expect("system tenant connection");
        sqlx::query(
            "INSERT INTO vala.cluster_nodes
                (data_tenant_id, node_id, role, advertise_addr, fencing_token, started_at, heartbeat_at)
             VALUES ($1, $2, 'scribe', $3, 7, now() - interval '30 seconds', now() - interval '30 seconds')",
        )
        .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
        .bind(stale_node.as_uuid())
        .bind("127.0.0.1:9002")
        .execute(&mut **conn.transaction())
        .await
        .expect("insert stale");
        conn.commit().await.expect("commit stale row");

        let live = live_scribes(&postgres).await.expect("live_scribes");
        assert!(
            !live.iter().any(|s| s.node_id == stale_node),
            "row with heartbeat_at 30s old must be filtered by the 15s liveness window (got: {live:?})",
        );
    }

    #[tokio::test]
    async fn scribe_live_scribes_returns_writer_epoch() {
        if std::env::var("WYRD_DATABASE_URL").is_err() {
            eprintln!("skipping: WYRD_DATABASE_URL unset");
            return;
        }
        let (_fixture, postgres) = setup().await;
        let node_id = NodeId::generate();
        let advertise_addr = format!(
            "127.0.0.1:{}",
            9100_u16 + u16::try_from(Uuid::now_v7().as_u128() % 500).expect("port offset < 500")
        );
        let identity = acquire_on_boot(&postgres, node_id, "scribe", &advertise_addr)
            .await
            .expect("acquire");

        let live = live_scribes(&postgres).await.expect("live_scribes");
        let mine = live
            .iter()
            .find(|s| s.node_id == node_id)
            .expect("this pod's row must be live immediately after acquire_on_boot");

        assert_eq!(
            mine.writer_epoch, identity.writer_epoch,
            "projection must alias fencing_token to writer_epoch",
        );
        assert_eq!(
            mine.advertise_addr, advertise_addr,
            "projection must include advertise_addr",
        );
    }
}
