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
    use vala_bifrost_redux::scribe::registry::{ScribeHeartbeat, live_scribes};
    use vala_bifrost_redux::scribe::stream_identity::{NodeId, acquire_on_boot};
    use vala_sql::OperatorPool;
    use wyrd_dev_fixtures::pg::PgFixture;

    const TICK: Duration = Duration::from_millis(200);

    async fn setup() -> (PgFixture, OperatorPool) {
        let fixture = PgFixture::start().await.expect("fixture");
        let pool = OperatorPool::from(fixture.platform_admin_pool().clone());
        (fixture, pool)
    }

    async fn read_row(
        pool: &OperatorPool,
        node_id: NodeId,
    ) -> (i64, chrono::DateTime<chrono::Utc>) {
        let row: (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT fencing_token, heartbeat_at FROM vala.cluster_nodes WHERE node_id = $1",
        )
        .bind(node_id.as_uuid())
        .fetch_one(pool.pool())
        .await
        .expect("cluster_nodes row");
        row
    }

    #[tokio::test]
    async fn scribe_heartbeats_cluster_nodes_every_5s() {
        if std::env::var("WYRD_DATABASE_URL").is_err() {
            eprintln!("skipping: WYRD_DATABASE_URL unset");
            return;
        }
        let (_fixture, pool) = setup().await;
        let node_id = NodeId::generate();
        acquire_on_boot(&pool, node_id, "scribe", "127.0.0.1:9000")
            .await
            .expect("acquire");

        let (_, initial_heartbeat) = read_row(&pool, node_id).await;

        let heartbeat = ScribeHeartbeat::start(pool.clone(), node_id, TICK);

        // Two full ticks + slack — at least two `heartbeat_at` updates should
        // have run by now (loop skips its immediate first tick).
        tokio::time::sleep(TICK * 3).await;

        let (_, after_heartbeat) = read_row(&pool, node_id).await;
        heartbeat.shutdown();

        assert!(
            after_heartbeat > initial_heartbeat,
            "heartbeat_at must advance after ~{}ms of ticks (initial={initial_heartbeat}, after={after_heartbeat})",
            (TICK * 3).as_millis(),
        );
    }

    #[tokio::test]
    async fn scribe_heartbeat_does_not_bump_writer_epoch() {
        if std::env::var("WYRD_DATABASE_URL").is_err() {
            eprintln!("skipping: WYRD_DATABASE_URL unset");
            return;
        }
        let (_fixture, pool) = setup().await;
        let node_id = NodeId::generate();
        let identity = acquire_on_boot(&pool, node_id, "scribe", "127.0.0.1:9001")
            .await
            .expect("acquire");

        let (epoch_before, _) = read_row(&pool, node_id).await;
        assert_eq!(
            epoch_before,
            identity.writer_epoch.as_i64(),
            "sanity: acquire_on_boot returned the row's fencing_token",
        );

        let heartbeat = ScribeHeartbeat::start(pool.clone(), node_id, TICK);
        tokio::time::sleep(TICK * 4).await; // ≥3 heartbeat cycles
        let (epoch_after, _) = read_row(&pool, node_id).await;
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
        let (_fixture, pool) = setup().await;
        let stale_node = NodeId::generate();

        sqlx::query(
            "INSERT INTO vala.cluster_nodes
                (node_id, role, advertise_addr, fencing_token, started_at, heartbeat_at)
             VALUES ($1, 'scribe', $2, 7, now() - interval '30 seconds', now() - interval '30 seconds')",
        )
        .bind(stale_node.as_uuid())
        .bind("127.0.0.1:9002")
        .execute(pool.pool())
        .await
        .expect("insert stale");

        let live = live_scribes(&pool).await.expect("live_scribes");
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
        let (_fixture, pool) = setup().await;
        let node_id = NodeId::generate();
        let advertise_addr = format!(
            "127.0.0.1:{}",
            9100_u16 + u16::try_from(Uuid::now_v7().as_u128() % 500).expect("port offset < 500")
        );
        let identity = acquire_on_boot(&pool, node_id, "scribe", &advertise_addr)
            .await
            .expect("acquire");

        let live = live_scribes(&pool).await.expect("live_scribes");
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
