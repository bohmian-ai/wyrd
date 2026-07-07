//! Client-tier Bifrost throughput bench: enqueue rate into MockSink.
//!
//! Measures the producer pool overhead (get-or-create, lock, enqueue) at
//! varying batch sizes. Informational only — no SLO enforcement. Stage 5 adds
//! the server-round-trip bench when the gRPC ingest transport is wired.
//!
//! Run with:
//! ```sh
//! cargo bench -p vala-sdk --features bench-bin
//! ```

use std::sync::Arc;

use arrow_schema::{DataType, Field, Schema, SchemaRef};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use vala_sdk::{Bifrost, ClientScope, SinkKind};
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpConfig;
use wyrd_queue::{MockSink, QueueConfig};
use wyrd_spec::reference::CardRef;

fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("payload", DataType::Utf8, false),
    ]))
}

fn card() -> CardRef {
    "prod/Service/bench@1.0.0".parse().expect("valid card ref")
}

fn row(i: usize) -> Vec<u8> {
    format!(r#"{{"id": {i}, "payload": "bench-payload"}}"#).into_bytes()
}

fn make_bifrost() -> Bifrost {
    let config = ClientConfig {
        http: HttpConfig {
            base_url: "http://bench.local".to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some("bench-key".to_owned().into()),
        ..ClientConfig::default()
    };
    let scope = ClientScope::from_config(&config).expect("scope");
    Bifrost::new(scope, Arc::new(MockSink::new()), QueueConfig::default())
}

fn bench_enqueue_throughput(c: &mut Criterion) {
    let schema = schema();
    let target = card();

    let mut group = c.benchmark_group("bifrost_enqueue");

    for batch_size in [100usize, 1_000, 10_000] {
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, &size| {
                b.iter(|| {
                    let bifrost = make_bifrost();
                    for i in 0..size {
                        bifrost
                            .insert(
                                SinkKind::Record,
                                "bench.prompts",
                                &schema,
                                row(i),
                                target.clone(),
                                None,
                            )
                            .expect("enqueue");
                    }
                    bifrost.dropped()
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_enqueue_throughput);
criterion_main!(benches);
