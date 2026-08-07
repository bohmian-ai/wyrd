mod pg_tests {
    //! Integration proof that the D84 governor and memtable gauges move during a
    //! real Postgres-backed ingest.
    //!
    //! Unit tests prove each gauge family carries only closed labels; this test
    //! proves the production steady-state path ([`ScribeImpl::check_age`]) exports
    //! live, non-zero values once rows are buffered. It appends a batch that stays
    //! resident in the writable memtable (no seal), drives one age-scan tick under
    //! a scoped [`wyrd_bench::BenchmarkRecorder`], and asserts the per-child
    //! occupancy, the D83 ingress watermarks, and the memtable gauges reflect the
    //! buffered state rather than sitting at zero.
    //!
    //! Skipped when `WYRD_DATABASE_URL` is unset (credential-free default suite).

    use arrow::array::{RecordBatch, TimestampMicrosecondArray, UInt64Array};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use chrono::DateTime;
    use opendal::services::Memory;
    use sqlx::types::Uuid;
    use std::sync::Arc;
    use vala_bifrost_redux::catalog::TableRef;
    use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
    use vala_bifrost_redux::scribe::ScribeImpl;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;

    /// Stand up a tenant-seeded Postgres fixture and an embedded Scribe over an
    /// in-memory object store, mirroring the seal integration harness.
    async fn setup() -> (PgFixture, DataTenantId, ScribeImpl) {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("test-{}", tenant.as_uuid().simple()),
            )
            .await
            .unwrap();

        let operator = Arc::new(
            opendal::Operator::new(Memory::default())
                .expect("memory backend")
                .finish(),
        );

        let temp_dir = tempfile::tempdir().expect("temp WAL dir");
        let mut node_id_bytes = *Uuid::now_v7().as_bytes();
        // The seal filename seam accepts a PodId string while file_list stores the
        // same value as UUID; pick a UUID whose first hex nibble satisfies the
        // PodId grammar, exactly as the seal harness does.
        node_id_bytes[0] = 0xa0 | (node_id_bytes[0] & 0x0f);
        let node_id = Uuid::from_bytes(node_id_bytes);
        let wal = Arc::new(
            vala_bifrost_redux::scribe::wal::WalWriter::new(
                temp_dir.path(),
                node_id_bytes,
                1,
                vala_bifrost_redux::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        // Leak the temp dir so WAL segments survive for the test lifetime.
        std::mem::forget(temp_dir);

        let scribe = ScribeImpl::new_for_embedded_with_deps(operator, wal, &node_id.to_string(), 1);
        (fixture, tenant, scribe)
    }

    /// Build a single-day Arrow batch of monotonically increasing rows.
    fn make_batch(row_count: usize, base_time_micros: i64) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("value", DataType::UInt64, false),
        ]));
        let timestamps: Vec<i64> = (0..row_count)
            .map(|i| base_time_micros + (i64::try_from(i).expect("bounded row index") * 1000))
            .collect();
        let values: Vec<u64> = (0..row_count)
            .map(|i| u64::try_from(i).expect("bounded row index"))
            .collect();
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(TimestampMicrosecondArray::from(timestamps)),
                Arc::new(UInt64Array::from(values)),
            ],
        )
        .expect("batch")
    }

    /// A user principal scoped to the seeded tenant.
    fn principal_for_tenant(tenant: DataTenantId) -> Principal {
        Principal {
            id: PrincipalId::new(Uuid::now_v7()),
            kind: PrincipalKind::User,
            tenant_id: tenant,
            roles: vec![],
            effective_permissions: PermissionSet::new(),
        }
    }

    /// Read a gauge value from the recorder snapshot, defaulting to zero.
    fn gauge(snapshot: &wyrd_bench::BenchmarkMetricSnapshot, key: &str) -> f64 {
        snapshot.gauges.get(key).copied().unwrap_or(0.0)
    }

    /// The governor and memtable gauges report live occupancy during an ingest.
    ///
    /// Appends a resident (unsealed) batch, then drives one production age-scan
    /// tick under a scoped recorder and asserts the D84 governor gauges
    /// (per-child used/limit and the D83 ingress watermarks) and the memtable
    /// gauges reflect the buffered rows rather than sitting at zero. This is the
    /// AC2 evidence: steady-state occupancy only arises through a real ingest.
    #[tokio::test]
    async fn governor_gauges_move_during_pg_ingest() {
        let (_fixture, tenant, scribe) = setup().await;
        let base_time = DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
            .expect("time")
            .timestamp_micros();
        // A modest batch stays resident in the writable memtable (no size seal),
        // so the ingress reservation and memtable bytes remain charged when the
        // age-scan tick reads them.
        let batch = make_batch(5_000, base_time);
        let fingerprint = SchemaFingerprint::from_arrow_schema(batch.schema().as_ref());
        let req = ScribeAppend {
            principal: principal_for_tenant(tenant),
            table: TableRef::new(BifrostNamespace::Bifrost, "events"),
            rows: batch,
            schema_fingerprint: fingerprint,
            request_id: RequestId::now_v7(),
            batch_id: uuid::Uuid::now_v7(),
            measured_wire_bytes: 0,
        };
        scribe.append(req).await.expect("append");

        // `check_age` emits the governor and memtable gauges before it requests any
        // flush, so the snapshot reflects the still-buffered ingest.
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            scribe.check_age(std::time::Instant::now());
        });
        let snapshot = recorder.snapshot();

        // Per-child limits are structural and always positive.
        assert!(
            gauge(&snapshot, "bifrost_memory_limit_bytes{consumer=\"scribe\"}") > 0.0,
            "scribe child limit gauge must be exported and positive"
        );
        assert!(
            gauge(&snapshot, "bifrost_memory_limit_bytes{consumer=\"parent\"}") > 0.0,
            "parent limit gauge must be exported and positive"
        );
        // The buffered ingest charges the Scribe child, so used bytes moved.
        assert!(
            gauge(
                &snapshot,
                "bifrost_memory_reserved_bytes{consumer=\"scribe\"}"
            ) > 0.0,
            "scribe reserved bytes must move while rows are buffered"
        );
        // The D83 ingress watermarks: limit is the denominator (positive) and
        // occupancy tracks the live charge (moved by the ingest).
        assert!(
            gauge(
                &snapshot,
                "bifrost_scribe_ingress_watermark_bytes{mark=\"limit\"}"
            ) > 0.0,
            "ingress watermark limit must be exported and positive"
        );
        assert!(
            gauge(
                &snapshot,
                "bifrost_scribe_ingress_watermark_bytes{mark=\"occupancy\"}"
            ) > 0.0,
            "ingress watermark occupancy must move while rows are buffered"
        );
        // The memtable gauges reflect the resident writable rows.
        assert!(
            gauge(&snapshot, "bifrost_scribe_active_memtable_bytes") > 0.0,
            "active memtable bytes must move while rows are buffered"
        );
    }
}
