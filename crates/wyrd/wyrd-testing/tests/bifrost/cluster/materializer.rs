//! Real Postgres smoke proof for the production-path qualification materializer.

#[cfg(feature = "bench")]
mod pg_tests {
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use tempfile::tempdir;
    use tokio_util::sync::CancellationToken;
    use wyrd_testing::bifrost::bench_dataset::{
        BifrostQualificationDataset, DatasetManifestFixture, DatasetShape,
    };
    use wyrd_testing::bifrost::bench_materializer::{
        BackpressurePolicy, BifrostDatasetMaterializer, MaterializationError,
    };
    use wyrd_testing::bifrost::{BifrostTopology, WyrdTestCluster};

    /// Under injected WAL pressure the materializer names its own binding ceiling.
    ///
    /// Trips the server WAL breaker so public admission returns retryable
    /// capacity rejections, then drives the locked two-tenant shape against a
    /// short 20-second deadline. The run cannot complete, so it fails with
    /// `SetupDeadlineExceeded`; the returned pressure evidence must carry the
    /// governor high-water captured from the server inspection snapshot before
    /// shutdown and, from that captured telemetry alone, name a binding ceiling
    /// with no ad-hoc instrumentation. This is the write-path proof for audit
    /// finding F5.
    #[tokio::test]
    #[ignore = "requires managed Postgres and the real public Gate cluster"]
    async fn pg_bifrost_materializer_names_binding_ceiling_under_wal_pressure() {
        let cluster = WyrdTestCluster::start(1, BifrostTopology::OnePod)
            .await
            .expect("cluster starts");
        let tenant = cluster
            .add_tenant("materializer-ceiling-tenant")
            .await
            .expect("second tenant starts");
        let dataset = BifrostQualificationDataset::new(
            DatasetShape::new(2, 160_000).expect("locked smoke shape"),
        )
        .expect("dataset builds");
        let run_root = tempdir().expect("run root");
        let server = cluster.server(0).expect("server exists");
        server
            .trip_bifrost_wal_disk_full_for_test()
            .expect("WAL breaker trips");
        let materializer = BifrostDatasetMaterializer::from_server(
            server,
            dataset,
            vec![cluster.data_tenant_id(), tenant],
            BackpressurePolicy::with_deadline(Instant::now() + Duration::from_secs(20)),
            run_root.path(),
            CancellationToken::new(),
        )
        .await
        .expect("materializer setup");
        let error = materializer
            .materialize()
            .await
            .expect_err("WAL pressure exhausts the setup deadline");
        drop(materializer);
        let pressure = match &error {
            MaterializationError::SetupDeadlineExceeded { pressure, .. } => pressure.clone(),
            other => {
                cluster
                    .shutdown()
                    .await
                    .expect("cluster stops after unexpected error");
                panic!("expected a setup-deadline failure under WAL pressure: {other:?}");
            }
        };
        let binding_ceiling = pressure.binding_ceiling();
        assert!(
            pressure.rejections > 0,
            "WAL pressure produced retryable capacity rejections"
        );
        assert!(
            pressure.governor_high_water.is_some(),
            "server governor high-water was captured before shutdown"
        );
        assert!(
            binding_ceiling.is_some(),
            "captured telemetry names the binding ceiling"
        );
        println!(
            "materializer WAL pressure rejections={} binding_ceiling={binding_ceiling:?} governor_high_water={:?}",
            pressure.rejections, pressure.governor_high_water,
        );
        cluster.shutdown().await.expect("cluster stops");
    }

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
        // AC7 assertion split. Anchor-invariant facts (per-day row counts) are
        // asserted against the checked-in canonical-anchor manifest; the
        // anchor-dependent recorded run anchor is asserted against the live
        // admission window, never against a checked-in absolute event time. The
        // generator-at-run-anchor equivalence (uniform event-time shift, unchanged
        // Q2/Q3/Q4 selections) is proven at the unit tier in `bench_dataset`.
        let manifest: DatasetManifestFixture = serde_json::from_slice(include_bytes!(
            "../../../fixtures/bifrost/qualification/dataset-manifest.json"
        ))
        .expect("checked-in dataset manifest");
        let shape = manifest.smoke.shape;
        assert_eq!(result.layout.len(), (shape.days as usize) * 2);
        for partition in &result.layout {
            let per_day = manifest
                .smoke
                .per_day
                .iter()
                .find(|day| day.day == partition.day)
                .expect("manifest per-day fact");
            assert_eq!(partition.rows, per_day.row_count);
        }
        // The recorded run anchor is UTC-midnight aligned and slides the whole
        // event-time axis into the live admission window (30 days past, 24 hours
        // future), so admission accepts every qualification row; completion above
        // with full per-day counts is the end-to-end zero-`EventTimeOutOfRange`
        // proof (AC6). It is never the checked-in canonical 2026-01-01 anchor.
        const MICROS_PER_DAY: i64 = 86_400_000_000;
        assert_eq!(result.run_anchor_micros.rem_euclid(MICROS_PER_DAY), 0);
        let now_micros = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("after epoch")
                .as_micros(),
        )
        .expect("micros fit i64");
        assert!(result.run_anchor_micros >= now_micros - 30 * MICROS_PER_DAY);
        assert!(
            result.run_anchor_micros + i64::from(shape.days) * MICROS_PER_DAY
                <= now_micros + MICROS_PER_DAY
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

    /// Prove the pressure seal drains ingress occupancy during ingest with no
    /// manual flush — the write-path falsifier of the historical livelock.
    ///
    /// The full smoke `pg_bifrost_materializer_smoke_shape` reads each day back
    /// through the Oracle query path to assert the materialized layout; that
    /// read-back is deliberately out of this test's scope. This sibling instead
    /// drives the identical two-tenant `DatasetShape(2, 160_000)` write path and
    /// polls the typed Scribe inspection snapshot mid-ingest to observe ingress
    /// occupancy cross the high-water mark and then fall back below the low-water
    /// mark — the coordinated pressure seal freeing capacity — while the client
    /// issues no manual flush (the materializer flushes only at a day boundary,
    /// after this test has already observed the drain and cancelled). It never
    /// reaches the day-boundary read-back, so it isolates the write-path drain
    /// from the Oracle read path. Before the pressure seal, occupancy pinned at the ingress
    /// ceiling and never fell (the livelock); with it, occupancy must oscillate within
    /// the hysteresis band.
    #[tokio::test]
    #[ignore = "requires managed Postgres and the real public Gate cluster"]
    async fn pg_bifrost_pressure_seal_drains_ingress_without_manual_flush() {
        let cluster = WyrdTestCluster::start(1, BifrostTopology::OnePod)
            .await
            .expect("cluster starts");
        let tenant = cluster
            .add_tenant("pressure-drain-tenant")
            .await
            .expect("second tenant starts");
        let dataset = BifrostQualificationDataset::new(
            DatasetShape::new(2, 160_000).expect("locked smoke shape"),
        )
        .expect("dataset builds");
        let run_root = tempdir().expect("run root");
        let token = CancellationToken::new();
        let materializer = BifrostDatasetMaterializer::from_server(
            cluster.server(0).expect("server exists"),
            dataset,
            vec![cluster.data_tenant_id(), tenant],
            BackpressurePolicy::with_deadline(Instant::now() + Duration::from_secs(180)),
            run_root.path(),
            token.clone(),
        )
        .await
        .expect("materializer setup");

        let drained = {
            let server = cluster.server(0).expect("server exists");
            let materialize_future = materializer.materialize();
            tokio::pin!(materialize_future);
            let evidence_deadline = tokio::time::Instant::now() + Duration::from_secs(150);
            let mut crossed_high = false;
            let mut peak_occupancy = 0_usize;
            let observed = loop {
                let snapshot = server
                    .scribe_inspection_snapshot()
                    .expect("Scribe inspection snapshot");
                peak_occupancy = peak_occupancy.max(snapshot.ingress_used_memory);
                // The watermark decision keys on ingress occupancy against the
                // ingress ceiling, not the larger Scribe child limit.
                if snapshot.ingress_high_water_memory > 0
                    && snapshot.ingress_used_memory >= snapshot.ingress_high_water_memory
                {
                    crossed_high = true;
                }
                // Once occupancy has crossed high-water, a coordinated pressure
                // seal (no manual flush) must drain it back below the low-water
                // mark. Observing that ordering proves the drain, not a lull.
                if crossed_high && snapshot.ingress_used_memory < snapshot.ingress_low_water_memory
                {
                    break true;
                }
                assert!(
                    tokio::time::Instant::now() < evidence_deadline,
                    "pressure seal never drained ingress below low-water \
                     (crossed_high={crossed_high}, peak_occupancy={peak_occupancy})"
                );
                tokio::select! {
                    result = &mut materialize_future => {
                        panic!(
                            "materializer reached the day-boundary read-back before the \
                             mid-ingest drain was observed: {result:?}"
                        );
                    }
                    _ = tokio::task::yield_now() => {}
                }
            };
            // Stop before the day-boundary Oracle read-back, which is out of this
            // write-path test's scope; the run directory is cleaned on cancel.
            token.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(30), &mut materialize_future).await;
            observed
        };
        drop(materializer);
        assert!(
            drained,
            "pressure seal drained ingress occupancy below low-water without manual flush"
        );
        cluster.shutdown().await.expect("cluster stops");
    }
}
