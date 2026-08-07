//! Real Postgres smoke proof for the production-path qualification materializer.

#[cfg(feature = "bench")]
mod pg_tests {
    use std::time::{Duration, Instant};

    use tempfile::tempdir;
    use tokio_util::sync::CancellationToken;
    use wyrd_testing::bifrost::bench_dataset::{BifrostQualificationDataset, DatasetShape};
    use wyrd_testing::bifrost::bench_materializer::{
        BackpressurePolicy, BifrostDatasetMaterializer, MaterializationError,
    };
    use wyrd_testing::bifrost::{BifrostTopology, WyrdTestCluster};

    /// Materialize the locked two-tenant smoke shape through public Gate and Oracle.
    ///
    /// The 180-second absolute deadline is intentionally part of this fixture's
    /// contract: the test measures production backpressure rather than changing
    /// Scribe lifecycle controls.
    #[tokio::test]
    #[ignore = "requires managed Postgres and the real public Gate cluster"]
    async fn pg_bifrost_materializer_smoke_shape() {
        let cluster = WyrdTestCluster::start(1, BifrostTopology::OnePod)
            .await
            .expect("cluster starts");
        let tenant = cluster
            .add_tenant("materializer-smoke-tenant")
            .await
            .expect("second tenant starts");
        let dataset = BifrostQualificationDataset::new(
            DatasetShape::new(2, 160_000).expect("locked smoke shape"),
        )
        .expect("dataset builds");
        let run_root = tempdir().expect("run root");
        let materializer = BifrostDatasetMaterializer::from_server(
            cluster.server(0).expect("server exists"),
            dataset,
            vec![cluster.data_tenant_id(), tenant],
            BackpressurePolicy::with_deadline(Instant::now() + Duration::from_secs(180)),
            run_root.path(),
            CancellationToken::new(),
        )
        .await
        .expect("materializer setup");
        let result = materializer.materialize().await;
        drop(materializer);
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                cluster
                    .shutdown()
                    .await
                    .expect("cluster stops after smoke failure");
                panic!("smoke materialization completes: {error:?}");
            }
        };
        assert_eq!(result.layout.len(), 4);
        assert!(
            result
                .layout
                .iter()
                .all(|partition| partition.rows == 160_000)
        );
        assert!(result.rows_per_second > 0.0);
        assert!(result.bytes_per_second > 0.0);
        println!(
            "materializer throughput rows_per_second={:.2} bytes_per_second={:.2} duration_ms={} pressure_rejections={} pressure_waited_ms={}",
            result.rows_per_second,
            result.bytes_per_second,
            result.duration.as_millis(),
            result.pressure.rejections,
            result.pressure.waited.as_millis(),
        );
        let cancellation_tenant = cluster
            .add_tenant("materializer-cancel-tenant")
            .await
            .expect("cancellation tenant starts");
        let token = CancellationToken::new();
        let cancellation_root = tempdir().expect("cancellation run root");
        let cancellation_materializer = BifrostDatasetMaterializer::from_server(
            cluster.server(0).expect("server exists"),
            dataset,
            vec![cancellation_tenant],
            BackpressurePolicy::with_deadline(Instant::now() + Duration::from_secs(180)),
            cancellation_root.path(),
            token.clone(),
        )
        .await
        .expect("cancel materializer setup");
        let cancelled = {
            let cancellation_server = cluster.server(0).expect("server exists");
            let materialize_future = cancellation_materializer.materialize();
            tokio::pin!(materialize_future);
            let evidence_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            loop {
                let snapshot = cancellation_server
                    .scribe_inspection_snapshot()
                    .expect("Scribe inspection snapshot");
                let admitted = snapshot.scribe_used_memory > 0
                    || snapshot.wal_disk_bytes > 0
                    || snapshot.writable_bucket_count > 0
                    || snapshot.immutable_bucket_count > 0;
                if admitted {
                    token.cancel();
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < evidence_deadline,
                    "typed Scribe inspection never observed public batch admission"
                );
                tokio::select! {
                    result = &mut materialize_future => {
                        panic!("materializer finished before active Scribe admission: {result:?}");
                    }
                    _ = tokio::task::yield_now() => {}
                }
            }
            tokio::time::timeout(Duration::from_secs(30), &mut materialize_future)
                .await
                .expect("active cancellation drains")
                .expect_err("cancellation is reported")
        };
        drop(cancellation_materializer);
        assert!(matches!(cancelled, MaterializationError::Cancelled { .. }));
        assert_eq!(
            std::fs::read_dir(cancellation_root.path())
                .expect("run root exists")
                .count(),
            0
        );
        cluster.shutdown().await.expect("cluster stops");
    }
}
