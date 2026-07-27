//! Vala client SDK — the client-tier Bifrost ingest surface.
//!
//! `vala-sdk` ships buffered JSON rows to the Wyrd ingest service as Record
//! batches. It owns three things:
//!
//! - [`BifrostIngestSink`] — the `wyrd-queue` [`BatchSink`] that maps one sealed
//!   batch onto the ingest RPC (`table`, `wyrd_batch_id`, Arrow IPC frames),
//!   preserving `batch_id` so the server's commit dedup holds across retries.
//! - [`Bifrost`] — the write handle that pools one [`Producer`] per destination,
//!   keyed by `(ClientScope, SinkKind, table)`.
//! - [`observe`] — fire-and-forget telemetry that never breaks its caller.
//!
//! [`ClientScope`] is a credential fingerprint the **token-opaque** client tier
//! computes without ever decoding a JWT: `(server_url, SHA-256 of the resolved
//! credential's secret material)`. Backpressure is asymmetric — [`Bifrost::insert`]
//! propagates queue-full to the caller, while [`observe::record`] swallows it,
//! counts the drop, and warns.
//!
//! [`BatchSink`]: wyrd_queue::BatchSink
//! [`Producer`]: wyrd_queue::Producer

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod grpc;
pub mod handle;
pub mod observe;
#[cfg(feature = "python")]
pub mod python;
pub mod scope;
pub mod sink;

pub use grpc::{
    BifrostFrame, BifrostGrpcTransport, BifrostTransportConfig, MAX_FRAME_BYTES, MAX_FRAME_RETRIES,
    PROTO_FRAME_OVERHEAD_BYTES,
};
pub use handle::{Bifrost, schema_from_json_schema};
pub use scope::{ClientScope, SinkKind};
pub use sink::{BifrostIngestSink, IngestTransport};

// C4a forward schema helpers, re-exported so SDK users build the user Arrow
// schema from a `FieldSpec` set or a JSON-Schema value without reaching into
// `wyrd-queue` directly.
pub use wyrd_queue::{fieldspec_to_arrow, json_schema_to_arrow};

/// Server-free unit lane for `vala-sdk`: scope identity, producer pooling, and
/// the asymmetric backpressure contract — all driven through mock/stall sinks.
#[cfg(test)]
mod sdk {
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use arrow_schema::{DataType, Field, Schema, SchemaRef};
    use async_trait::async_trait;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::HttpConfig;
    use wyrd_queue::{BatchSink, MockSink, QueueConfig, SealedBatch};
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::reference::CardRef;

    use crate::{Bifrost, BifrostIngestSink, ClientScope, IngestTransport, SinkKind, observe};

    fn config_with_key(url: &str, key: &str) -> ClientConfig {
        ClientConfig {
            http: HttpConfig {
                base_url: url.to_owned(),
                ..HttpConfig::default()
            },
            api_key: Some(key.to_owned().into()),
            ..ClientConfig::default()
        }
    }

    fn test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]))
    }

    fn card() -> CardRef {
        "prod/Service/alpha@1.0.0".parse().expect("valid card ref")
    }

    fn row() -> Vec<u8> {
        br#"{"id": 1}"#.to_vec()
    }

    /// Config that saturates fast: a stalled sink parks the drain, the tiny channel
    /// fills, and further enqueues return queue-full.
    fn saturating_config() -> QueueConfig {
        QueueConfig {
            channel_capacity: 2,
            staging_capacity: 8,
            flush_max_rows: 1,
            flush_interval_ms: 0,
            flush_timeout_ms: 60_000,
            max_message_bytes: 4 * 1024 * 1024,
        }
    }

    /// A sink whose `send` parks forever after signalling it started — stalls the
    /// producer's drain so the bounded channel cannot empty.
    #[derive(Default)]
    struct StallSink {
        started: AtomicBool,
    }

    #[async_trait]
    impl BatchSink for StallSink {
        async fn send(&self, _batch: SealedBatch) -> Result<u64, WyrdError> {
            self.started.store(true, Ordering::SeqCst);
            std::future::pending::<()>().await;
            unreachable!()
        }
    }

    fn wait_until_started(sink: &StallSink) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !sink.started.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "stall sink never started");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn same_url_and_credential_collapse_to_one_scope() {
        let a = ClientScope::from_config(&config_with_key("http://x", "secret")).expect("scope");
        let b = ClientScope::from_config(&config_with_key("http://x", "secret")).expect("scope");
        assert_eq!(a, b, "same (url, credential) must produce one scope");
        assert_eq!(a.credential_fingerprint(), b.credential_fingerprint());
    }

    #[test]
    fn different_credential_yields_different_scope() {
        let a = ClientScope::from_config(&config_with_key("http://x", "secret-a")).expect("scope");
        let b = ClientScope::from_config(&config_with_key("http://x", "secret-b")).expect("scope");
        assert_ne!(a, b, "differing secrets must not collide");
        assert_ne!(a.credential_fingerprint(), b.credential_fingerprint());
        assert_eq!(a.server_url(), b.server_url(), "same base URL is shared");
        // Proof of token-opacity: the fingerprint is a raw SHA-256 hex of the secret
        // bytes (64 hex chars), never a decoded JWT claim.
        assert_eq!(a.credential_fingerprint().len(), 64);
        assert!(
            a.credential_fingerprint()
                .bytes()
                .all(|c| c.is_ascii_hexdigit())
        );
    }

    #[test]
    fn identical_key_reuses_one_pooled_producer() {
        let scope =
            ClientScope::from_config(&config_with_key("http://x", "secret")).expect("scope");
        let bifrost = Bifrost::new(scope, Arc::new(MockSink::new()), QueueConfig::default());
        let schema = test_schema();

        bifrost
            .insert(SinkKind::Record, "ns.tbl", &schema, row(), card(), None)
            .expect("first accepted");
        bifrost
            .insert(SinkKind::Record, "ns.tbl", &schema, row(), card(), None)
            .expect("second accepted");
        assert_eq!(
            bifrost.producer_count(),
            1,
            "identical (scope, kind, table) reuses the pooled producer"
        );

        bifrost
            .insert(SinkKind::Record, "ns.other", &schema, row(), card(), None)
            .expect("distinct table accepted");
        assert_eq!(bifrost.producer_count(), 2, "a new table is a new producer");
    }

    #[test]
    fn insert_propagates_queue_full() {
        let sink = Arc::new(StallSink::default());
        let scope =
            ClientScope::from_config(&config_with_key("http://x", "secret")).expect("scope");
        let bifrost = Bifrost::new(scope, sink.clone(), saturating_config());
        let schema = test_schema();

        bifrost
            .insert(SinkKind::Record, "ns.tbl", &schema, row(), card(), None)
            .expect("first enqueue accepted");
        wait_until_started(&sink);

        let mut rejected = 0;
        for i in 0..40 {
            let payload = format!(r#"{{"id": {}}}"#, i + 1).into_bytes();
            if let Err(err) =
                bifrost.insert(SinkKind::Record, "ns.tbl", &schema, payload, card(), None)
            {
                assert_eq!(
                    err.code(),
                    "WYRD_CLIENT_429_QUEUE_FULL",
                    "insert must surface queue-full to the caller"
                );
                rejected += 1;
            }
        }
        assert!(
            rejected > 0,
            "a saturated queue must reject on the write path"
        );
    }

    #[test]
    fn observe_record_swallows_and_counts_overflow() {
        let sink = Arc::new(StallSink::default());
        let scope =
            ClientScope::from_config(&config_with_key("http://x", "secret")).expect("scope");
        let bifrost = Bifrost::new(scope, sink.clone(), saturating_config());
        let schema = test_schema();

        // Prime the stall, then flood the same saturated producer via the telemetry
        // path. `record` returns unit — the caller is never handed an error.
        observe::record(
            &bifrost,
            SinkKind::Record,
            "ns.tbl",
            &schema,
            row(),
            card(),
            None,
        );
        wait_until_started(&sink);

        for i in 0..40 {
            let payload = format!(r#"{{"id": {}}}"#, i + 1).into_bytes();
            observe::record(
                &bifrost,
                SinkKind::Record,
                "ns.tbl",
                &schema,
                payload,
                card(),
                None,
            );
        }
        assert!(
            bifrost.dropped() > 0,
            "overflow on the telemetry path is counted, never surfaced"
        );
    }

    struct RecordingTransport {
        seen: Mutex<Vec<[u8; 16]>>,
    }

    #[async_trait]
    impl IngestTransport for RecordingTransport {
        async fn insert_batch(
            &self,
            _table: &str,
            batch_id: [u8; 16],
            _frames: Vec<u8>,
        ) -> Result<(), WyrdError> {
            self.seen.lock().expect("poisoned").push(batch_id);
            Ok(())
        }
    }

    #[test]
    fn ingest_sink_forwards_batch_id_unchanged() {
        let transport = Arc::new(RecordingTransport {
            seen: Mutex::new(Vec::new()),
        });
        let sink = BifrostIngestSink::new(transport.clone());
        let batch = SealedBatch {
            table: "ns.tbl".to_owned(),
            batch_id: [7u8; 16],
            frames: vec![1, 2, 3],
            rows: 1,
        };

        let rt = wyrd_runtime::runtime();
        rt.block_on(sink.send(batch.clone())).expect("first send");
        // A retry re-sends the identical batch — the sink forwards batch_id verbatim.
        rt.block_on(sink.send(batch)).expect("retry send");

        let seen = transport.seen.lock().expect("poisoned");
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], [7u8; 16]);
        assert_eq!(
            seen[1], [7u8; 16],
            "retry preserves batch_id for server dedup"
        );
    }

    #[test]
    fn schema_from_json_schema_builds_arrow_schema() {
        let json = serde_json::json!({
            "type": "object",
            "properties": { "id": { "type": "integer" } },
            "required": ["id"],
        });
        let schema = crate::schema_from_json_schema(&json).expect("schema builds");
        assert_eq!(schema.fields().len(), 1);
        assert_eq!(schema.field(0).name(), "id");
    }
}
