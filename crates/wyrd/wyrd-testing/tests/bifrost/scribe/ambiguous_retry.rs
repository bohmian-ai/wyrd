//! An ambiguous ingest deadline keeps its batch until the server deduplicates it.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_client::bifrost::{
    Bifrost, BifrostClientError, BifrostGrpcTransport, BifrostIngestSink, BifrostTransportConfig,
    Correlation, IngestTransport, TableConfig,
};
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_queue::{
    ClientByteGuard, DurableBatchAck, QueueConfig, SealedBatch, SinkError, WyrdQueueError,
};
use wyrd_testing::WyrdTestServer;

use super::support::{register_table, sorted_values, unique_table, value_schema};

/// Real gRPC transport wrapper that records the batch identity at the sink seam.
///
/// It performs no retry or payload transformation: it forwards the exact
/// non-cloneable sealed batch to [`BifrostGrpcTransport`], so the journey can
/// prove the queue hands back the same owner after an ambiguous deadline.
struct RecordingTransport {
    /// The authenticated real gRPC transport under test.
    inner: BifrostGrpcTransport,
    /// Batch ids observed before each real attempt.
    attempts: Mutex<Vec<[u8; 16]>>,
}

impl RecordingTransport {
    /// Returns the batch ids seen at the sink boundary, in order.
    ///
    /// # Panics
    ///
    /// Panics if the attempt recorder mutex is poisoned.
    fn attempts(&self) -> Vec<[u8; 16]> {
        self.attempts.lock().expect("attempt recorder").clone()
    }
}

#[async_trait]
impl IngestTransport<ClientByteGuard> for RecordingTransport {
    /// Records then forwards the exact owned batch through the real gRPC path.
    ///
    /// # Errors
    ///
    /// Propagates the real transport outcome, including its retained owner for
    /// an ambiguous deadline.
    ///
    /// # Panics
    ///
    /// Panics if the attempt recorder mutex is poisoned.
    async fn insert_batch(
        &self,
        batch: &SealedBatch<ClientByteGuard>,
    ) -> Result<DurableBatchAck, SinkError> {
        self.attempts
            .lock()
            .expect("attempt recorder")
            .push(batch.batch_id);
        IngestTransport::insert_batch(&self.inner, batch).await
    }
}

/// An ambiguous post-receipt deadline keeps its evidence until reconciliation.
///
/// The server's WAL sync outlasts the client's 250 ms deadline, so the first
/// flush cannot know whether the append landed. The queue makes one sink
/// attempt, retains the owner and its bytes as a retry instead of a loss, and
/// its scheduled retry resends the same UUIDv7 until the server deduplicates it
/// against the already-durable append. Exactly one row reads back.
///
/// # Panics
///
/// Panics when the first flush is not ambiguous, the queue drops or releases
/// the ambiguous owner, a retry changes the batch identity, the retry never
/// acknowledges, or the table reads back other than the one row.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn ambiguous_deadline_retains_then_deduplicates_one_batch() {
    let server = WyrdTestServer::builder()
        .with_wal_sync_delay(Duration::from_millis(750))
        .start_bound()
        .await
        .expect("delayed-WAL server starts");
    let tenant = server.data_tenant_id();
    let fqn = register_table(
        &server,
        tenant,
        BifrostNamespace::Bifrost,
        &unique_table("ambiguous_retry"),
    )
    .await;
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, &unique_table("ambiguous_writer"), &["admin"])
        .await
        .expect("bootstrap the writer");
    let client = wyrd_client::WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().expect("gRPC URL"),
            timeout_ms: 250,
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().expect("HTTP URL").to_owned(),
            ..HttpConfig::default()
        },
        credential: Some(bootstrap.api_key().expect("machine API key").clone()),
        ..ClientConfig::default()
    })
    .expect("writer client");
    let recording = Arc::new(RecordingTransport {
        inner: BifrostGrpcTransport::connect_with_config(
            &client,
            BifrostTransportConfig::with_max_frame_retries(1),
        )
        .await
        .expect("connect the gRPC transport"),
        attempts: Mutex::new(Vec::new()),
    });
    let bifrost = Bifrost::with_sink(
        &client,
        Some(TableConfig::from_arrow(&fqn, value_schema()).expect("declared table")),
        Arc::new(BifrostIngestSink::new(recording.clone())),
        QueueConfig {
            linger_ms: 60_000,
            ..QueueConfig::default()
        },
    );

    bifrost
        .insert(
            br#"{"value": 41}"#.to_vec(),
            Correlation {
                card_ref: bootstrap.card_ref().cloned(),
                run_id: None,
            },
        )
        .expect("enqueue one row");
    let first = bifrost
        .flush()
        .await
        .expect_err("a deadline after server receipt is ambiguous");
    let BifrostClientError::Queue(WyrdQueueError::Sink(sink_error)) = &first else {
        panic!("the queue must receive an ambiguous sink result: {first}");
    };
    assert!(
        matches!(
            sink_error,
            wyrd_spec::error::WyrdError::ServiceUnavailable { .. }
        ),
        "the deadline is a retryable ambiguous result: {sink_error:?}"
    );
    assert_eq!(
        recording.attempts().len(),
        1,
        "the transport owns every resend, so the queue made one sink attempt"
    );
    let retained = bifrost.metrics();
    assert!(
        retained.owned_bytes > 0,
        "the ambiguous owner keeps its bytes"
    );
    assert_eq!(
        (
            retained.retry_entries,
            retained.pending_controls,
            retained.dropped_rows
        ),
        (1, 0, 0),
        "the owner is retained for retry, its control released, nothing lost"
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    while bifrost.metrics().retry_entries > 0 {
        assert!(
            Instant::now() < deadline,
            "the scheduled retry never reconciled: {:?}",
            bifrost.metrics()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let attempts = recording.attempts();
    assert!(attempts.len() >= 2, "the queue retried the retained owner");
    assert!(
        attempts.iter().all(|id| *id == attempts[0]),
        "every retry carries the original UUIDv7"
    );
    let settled = bifrost.metrics();
    assert_eq!((settled.live_batches, settled.dropped_rows), (0, 0));

    bifrost.flush().await.expect("nothing is left to flush");
    bifrost.shutdown().await.expect("settled producer shutdown");
    assert_eq!(
        bifrost.metrics().owned_bytes,
        0,
        "no client bytes stay reserved"
    );
    server.flush_bifrost().await.expect("publish the append");
    assert_eq!(
        sorted_values(&client, &fqn).await,
        vec![41],
        "the retry did not duplicate the durable row"
    );
    server.shutdown().await.expect("server shutdown");
}
