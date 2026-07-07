//! Ignored Rust e2e mirror for the happy-path and backpressure/drain journeys.
//!
//! These tests require a live `WyrdTestServer` (embedded Postgres + real server
//! socket). Run with:
//!
//! ```sh
//! cargo test -p vala-sdk --all-features -- --ignored --test-threads=1
//! ```
//!
//! The current write path drains into a `MockSink` — the gRPC ingest transport
//! is wired in a later stage. Until then these tests exercise the SDK lifecycle,
//! producer-pool identity, and the asymmetric backpressure contract against
//! real server credentials.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use arrow_schema::{DataType, Field, Schema, SchemaRef};
use async_trait::async_trait;
use secrecy::ExposeSecret;
use vala_sdk::{Bifrost, ClientScope, SinkKind, observe};
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpConfig;
use wyrd_queue::{BatchSink, MockSink, QueueConfig, SealedBatch, WyrdQueueError};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_testing::server::WyrdTestServer;

fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]))
}

fn card() -> CardRef {
    "prod/Service/genai_pipeline@1.0.0"
        .parse()
        .expect("valid card ref")
}

fn row(i: i64) -> Vec<u8> {
    format!(r#"{{"id": {i}, "value": "batch"}}"#).into_bytes()
}

fn client_config(srv: &WyrdTestServer) -> ClientConfig {
    ClientConfig {
        http: HttpConfig {
            base_url: srv.base_url().unwrap_or("").to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(srv.api_key().expose_secret().to_owned().into()),
        ..ClientConfig::default()
    }
}

/// A stall sink that parks forever once a batch arrives — used to saturate the
/// bounded producer channel for backpressure tests.
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
    let deadline = Instant::now() + Duration::from_secs(5);
    while !sink.started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "stall sink never started");
        std::thread::sleep(Duration::from_millis(5));
    }
}

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

/// Happy-path: insert via Bifrost handle + observe path, no drops, real server
/// credentials validate the scope construction path.
#[tokio::test]
#[ignore = "requires live WyrdTestServer — run with --ignored --test-threads=1"]
async fn observe_and_bifrost_roundtrip() {
    let srv = WyrdTestServer::start_bound()
        .await
        .expect("test server start");
    let config = client_config(&srv);
    let scope = ClientScope::from_config(&config).expect("scope");
    let bifrost = Bifrost::new(scope, Arc::new(MockSink::new()), QueueConfig::default());
    let schema = schema();
    let target = card();

    for i in 0..500i64 {
        bifrost
            .insert(
                SinkKind::Record,
                "test.roundtrip",
                &schema,
                row(i),
                target.clone(),
                None,
            )
            .expect("insert via Bifrost handle");
    }
    assert_eq!(
        bifrost.dropped(),
        0,
        "Bifrost handle: no drops on happy path"
    );

    for i in 0..500i64 {
        observe::record(
            &bifrost,
            SinkKind::Record,
            "test.roundtrip",
            &schema,
            row(i),
            target.clone(),
            None,
        );
    }
    assert_eq!(bifrost.dropped(), 0, "observe path: no drops on happy path");
    assert_eq!(bifrost.producer_count(), 1, "one producer for one table");

    srv.shutdown().await.expect("server shutdown");
}

/// Backpressure + drain: a saturated queue propagates WYRD_CLIENT_429_QUEUE_FULL
/// from the insert path; observe swallows and counts.
#[tokio::test]
#[ignore = "requires live WyrdTestServer — run with --ignored --test-threads=1"]
async fn backpressure_and_drain_no_silent_drops() {
    let srv = WyrdTestServer::start_bound()
        .await
        .expect("test server start");
    let config = client_config(&srv);
    let scope = ClientScope::from_config(&config).expect("scope");
    let stall = Arc::new(StallSink::default());
    let bifrost = Bifrost::new(scope, stall.clone(), saturating_config());
    let schema = schema();
    let target = card();

    // Prime the stall: first insert starts draining then parks forever.
    bifrost
        .insert(
            SinkKind::Record,
            "test.backpressure",
            &schema,
            row(0),
            target.clone(),
            None,
        )
        .expect("first accepted");
    wait_until_started(&stall);

    // Flood until rejection.
    let mut saw_queue_full = false;
    for i in 1..=100 {
        match bifrost.insert(
            SinkKind::Record,
            "test.backpressure",
            &schema,
            row(i),
            target.clone(),
            None,
        ) {
            Ok(()) => {}
            Err(WyrdQueueError::QueueFull) => {
                saw_queue_full = true;
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
    assert!(
        saw_queue_full,
        "saturated queue must propagate WYRD_CLIENT_429_QUEUE_FULL"
    );

    // Observe path swallows queue-full.
    let scope2 = ClientScope::from_config(&client_config(&srv)).expect("scope2");
    let bifrost2 = Bifrost::new(scope2, stall.clone(), saturating_config());
    observe::record(
        &bifrost2,
        SinkKind::Record,
        "test.backpressure.obs",
        &schema,
        row(0),
        target.clone(),
        None,
    );
    wait_until_started(&stall);
    for i in 1..=100 {
        observe::record(
            &bifrost2,
            SinkKind::Record,
            "test.backpressure.obs",
            &schema,
            row(i),
            target.clone(),
            None,
        );
    }
    assert!(
        bifrost2.dropped() > 0,
        "observe path: drops counted on saturation"
    );

    srv.shutdown().await.expect("server shutdown");
}
