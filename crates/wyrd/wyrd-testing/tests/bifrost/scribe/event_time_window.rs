//! Negative public-ingest journey for the D85/T42 event-time acceptance window.
//!
//! Proves that a caller-supplied `wyrd_event_time` outside the server
//! acceptance window is REJECTED end-to-end through the public native Arrow IPC
//! ingest path, and that the stable `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE`
//! contract surfaces to the caller on the wire (gRPC `InvalidArgument` plus the
//! canonical code in `google.rpc.ErrorInfo.reason`) with no durable row
//! written. The native surface is used because it is the only public ingest
//! shape that carries a caller-stamped `wyrd_event_time`: the projected OTLP
//! path never presents one (spans project to user columns only and the server
//! stamps the managed event time at the seam), so an out-of-range value can
//! only originate from a native caller. Uniform seam enforcement across the
//! native and projected arms is proven by the `vala-bifrost-redux`
//! `execution_lanes` unit tests; this journey proves the public contract a real
//! native caller observes.

// Wrapped in `mod pg_tests`: the test boots a `start_bound` server (PgFixture),
// so the fast family lane skips it via `--skip pg_tests`; `mise run test:e2e`
// (Postgres up) runs the whole wyrd-testing crate including this.
mod pg_tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use arrow::array::{Int64Array, StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use wyrd_testing::{Bootstrap, WyrdTestServer};
    use wyrd_tonic::tonic::transport::Channel;
    use wyrd_tonic::tonic::{Code, Request};
    use wyrd_tonic::tonic_types::StatusExt;
    use wyrd_tonic::wyrd::v1::InsertBatchRequest;
    use wyrd_tonic::wyrd::v1::bifrost_ingest_service_client::BifrostIngestServiceClient;

    /// Registered table for the journey; only `[id, value]` are user columns.
    const TABLE_NAME: &str = "event_time_events";
    /// Fully-qualified name the public gRPC ingest frame targets.
    const TABLE_FQN: &str = "vala.bifrost.event_time_events";

    /// Connects a raw Bifrost ingest gRPC client, retrying until the mounted
    /// server accepts the channel or a fixed deadline elapses.
    ///
    /// A raw client (rather than the SDK transport) is used so the assertion can
    /// read the verbatim wire `Status` and its `google.rpc.ErrorInfo`, which is
    /// exactly what a headless/agent caller observes.
    async fn connect(url: &str) -> BifrostIngestServiceClient<Channel> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match Channel::from_shared(url.to_owned())
                .expect("endpoint parses")
                .connect()
                .await
            {
                Ok(channel) => return BifrostIngestServiceClient::new(channel),
                Err(error) => {
                    assert!(
                        Instant::now() < deadline,
                        "gRPC channel never connected within 5s: {error}"
                    );
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Encodes one native Arrow IPC frame carrying the registered `[id, value]`
    /// user columns plus a caller-stamped managed `wyrd_event_time` column set
    /// to `event_micros` for every row.
    ///
    /// The schema fingerprint excludes `wyrd_*` columns, so a batch presenting
    /// an explicit `wyrd_event_time` still resolves against a `[id, value]`
    /// table. The column uses the T38-validated physical type
    /// `Timestamp(Microsecond, Some("UTC"))` so it passes native type
    /// validation and reaches the acceptance-window check at the decode/stamp
    /// seam.
    fn ipc_with_event_time(ids: &[i64], event_micros: i64) -> Vec<u8> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("value", DataType::Utf8, false),
            Field::new(
                wyrd_spec::vala::WYRD_EVENT_TIME,
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
        ]));
        let rows = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(ids.to_vec())),
                Arc::new(StringArray::from(vec!["journey"; ids.len()])),
                Arc::new(
                    TimestampMicrosecondArray::from(vec![event_micros; ids.len()])
                        .with_timezone("UTC"),
                ),
            ],
        )
        .expect("valid event-time batch");
        let mut bytes = Vec::new();
        let mut writer =
            StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("IPC writer builds");
        writer.write(&rows).expect("IPC batch writes");
        writer.finish().expect("IPC stream finishes");
        bytes
    }

    /// Current wall-clock time in epoch microseconds, saturating into `i64`.
    ///
    /// # Panics
    ///
    /// Panics only if the system clock is before the Unix epoch, which cannot
    /// happen on a booted test host.
    fn now_micros() -> i64 {
        let micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after Unix epoch")
            .as_micros();
        i64::try_from(micros).unwrap_or(i64::MAX)
    }

    /// A caller `wyrd_event_time` older than the default 30-day past bound is
    /// rejected through the public native ingest path with the stable
    /// `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE` code, and no durable row is
    /// written.
    ///
    /// The value is stamped 31 days in the past, one day beyond the default
    /// past bound, so admission must reject it (reject, never clamp) before any
    /// Scribe write. The assertion reads the verbatim gRPC `Status`: the code
    /// class must be `InvalidArgument` (the 400 projection) and the attached
    /// `ErrorInfo.reason` must be the canonical Wyrd code — the exact contract a
    /// headless caller receives. A post-rejection flush then proves the
    /// `vala.file_list` row count for the table remains zero.
    #[tokio::test]
    async fn native_event_time_out_of_range_rejected_through_public_ingest() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("bound Bifrost server");
        let tenant = srv.data_tenant_id();
        srv.create_bifrost_table_for_test(CreateTableRequest {
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
        .expect("event-time journey table resolves");

        let jwt = match srv
            .bootstrap_user("event-time-writer", &["admin"])
            .await
            .expect("bootstrap ingest user")
        {
            Bootstrap::User { jwt, .. } => jwt,
            other => panic!("expected user bootstrap, got {other:?}"),
        };

        let grpc = srv.grpc_url().expect("grpc url");
        let mut client = connect(&grpc).await;

        // 31 days in the past: one day beyond the default 30-day past bound.
        let stale_micros = now_micros() - 31 * 24 * 60 * 60 * 1_000_000;
        let mut request = Request::new(InsertBatchRequest {
            table: TABLE_FQN.to_owned(),
            arrow_ipc: ipc_with_event_time(&[1], stale_micros).into(),
            wyrd_batch_id: uuid::Uuid::now_v7().as_bytes().to_vec().into(),
        });
        request.metadata_mut().insert(
            "x-wyrd-access-token",
            format!("Bearer {jwt}").parse().expect("metadata value"),
        );

        let status = client
            .insert_batch(request)
            .await
            .expect_err("out-of-range wyrd_event_time must be rejected");
        assert_eq!(
            status.code(),
            Code::InvalidArgument,
            "out-of-range event time is a 400/InvalidArgument rejection; got {status:?}"
        );
        let info = status
            .get_details_error_info()
            .expect("rejection carries google.rpc.ErrorInfo");
        assert_eq!(
            info.reason, "WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE",
            "the stable event-time code must surface to the caller"
        );

        // The rejected batch must not have produced any durable file-list row.
        srv.flush_bifrost().await.expect("post-rejection flush");
        let rows: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(row_count), 0)::bigint FROM vala.file_list
             WHERE data_tenant_id = $1 AND namespace = 'vala.bifrost' AND table_name = $2",
        )
        .bind(tenant.as_uuid())
        .bind(TABLE_NAME)
        .fetch_one(srv.pg_fixture().operator_pool().pool())
        .await
        .expect("post-rejection file-list read");
        assert_eq!(rows, 0, "a rejected event time must write no durable rows");

        srv.shutdown().await.expect("server shutdown");
    }
}
