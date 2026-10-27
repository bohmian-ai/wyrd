//! Read one trace's span tree through `Bifrost::trace_timeline`.
//!
//! Spans are written to the tenant's `bifrost.traces.spans` through the public
//! Arrow batch door with chosen trace ids, span ids, start times, and links,
//! then read back as canonical span records: the seed trace plus each trace
//! linking directly to it, deduplicated and in a total order.

use std::collections::BTreeMap;
use wyrd_client::bifrost::BifrostOptions;

use wyrd_client::bifrost::{Bifrost, TableConfig};
use wyrd_spec::vala::ids::TraceId;
use wyrd_testing::bifrost::canonical_signals::{Cell, Row, batch};
use wyrd_testing::server::WyrdTestServer;

use super::pg_tests::service_client;

/// The span table every timeline reads.
const SPANS: &str = "bifrost.traces.spans";
/// 2026-10-14T00:00:00Z in Unix nanoseconds, the first span's start.
const ANCHOR_NANOS: i64 = 1_791_936_000_000_000_000;

/// The trace id whose sixteen bytes are all `byte`.
fn trace(byte: u8) -> TraceId {
    TraceId::from_bytes([byte; 16]).expect("a non-zero trace id")
}

/// One span row of trace `[trace; 16]` and span `[span; 8]`, starting
/// `offset` nanoseconds after the anchor and linking to each trace in `links`.
fn span(trace: u8, span: u8, offset: i64, links: &[u8]) -> Row {
    BTreeMap::from([
        ("trace_id", Cell::Bytes(vec![trace; 16])),
        ("span_id", Cell::Bytes(vec![span; 8])),
        ("name", Cell::Text(format!("span-{trace:02x}-{span:02x}"))),
        ("kind", Cell::Int32(1)),
        ("start_time_unix_nano", Cell::Int64(ANCHOR_NANOS + offset)),
        (
            "end_time_unix_nano",
            Cell::Int64(ANCHOR_NANOS + offset + 1_000),
        ),
        ("duration_nano", Cell::Int64(1_000)),
        ("scope_name", Cell::Text("wyrd.timeline".to_owned())),
        (
            "service_name",
            Cell::Text("wyrd.timeline.service".to_owned()),
        ),
        (
            "links",
            Cell::Rows(
                links
                    .iter()
                    .map(|linked| {
                        BTreeMap::from([
                            ("trace_id", Cell::Bytes(vec![*linked; 16])),
                            ("span_id", Cell::Bytes(vec![*linked; 8])),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

/// A Bifrost client on a fresh server whose tenant span table holds `rows`,
/// published and readable.
///
/// # Panics
///
/// Panics when the server, the span table, the write, or the flush fails.
async fn spans_written(rows: &[Row]) -> (WyrdTestServer, Bifrost) {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    srv.ensure_builtin_table_for_test(srv.data_tenant_id(), "traces", "spans")
        .await
        .expect("the span table exists");
    let client = service_client(&srv, "timeline-reader", &["admin"]).await;
    let bifrost = Bifrost::connect_with(&client, BifrostOptions::default())
        .await
        .expect("Bifrost connects");
    let described = TableConfig::describe_with(&client, SPANS)
        .await
        .expect("the span table describes");
    bifrost
        .write_batch(SPANS, &batch(described.user_schema(), rows))
        .await
        .expect("the spans are admitted");
    srv.flush_bifrost().await.expect("the spans publish");
    (srv, bifrost)
}

/// The `(trace byte, span byte)` of each span in a timeline, in order.
fn ids(timeline: &[wyrd_spec::vala::trace::SpanRecord]) -> Vec<(u8, u8)> {
    timeline
        .iter()
        .map(|span| (span.trace_id.as_bytes()[0], span.span_id.as_bytes()[0]))
        .collect()
}

/// The seed trace's spans and every span of a trace linking directly to it
/// are read; an unrelated trace is not.
///
/// # Panics
///
/// Panics when the read fails or returns other spans.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn returns_seed_and_directly_linked_traces() {
    let (_srv, bifrost) = spans_written(&[
        span(0x0a, 0x01, 0, &[]),
        span(0x0a, 0x02, 10, &[]),
        span(0x0b, 0x01, 20, &[0x0a]),
        span(0x0b, 0x02, 30, &[]),
        span(0x0c, 0x01, 40, &[]),
    ])
    .await;

    let timeline = bifrost.trace_timeline(trace(0x0a)).await.expect("reads");

    assert_eq!(
        ids(&timeline),
        [(0x0a, 0x01), (0x0a, 0x02), (0x0b, 0x01), (0x0b, 0x02)]
    );
    assert_eq!(timeline[2].links[0].trace_id, trace(0x0a));
}

/// A trace linking only to a linked trace is a second hop and is not read.
///
/// # Panics
///
/// Panics when the read fails or returns the second-hop trace.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn ignores_second_hop_links() {
    let (_srv, bifrost) = spans_written(&[
        span(0x0a, 0x01, 0, &[]),
        span(0x0b, 0x01, 10, &[0x0a]),
        span(0x0c, 0x01, 20, &[0x0b]),
    ])
    .await;

    let timeline = bifrost.trace_timeline(trace(0x0a)).await.expect("reads");

    assert_eq!(ids(&timeline), [(0x0a, 0x01), (0x0b, 0x01)]);
}

/// A span stored twice reads once, keeping its earliest copy even when its
/// copies start apart and another span sorts between them, and spans starting
/// together order by trace id, then span id.
///
/// # Panics
///
/// Panics when the read fails, repeats a span, or orders a tie otherwise.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn deduplicates_and_orders_ties_by_trace_and_span() {
    let (_srv, bifrost) = spans_written(&[
        span(0x0b, 0x01, 0, &[0x0a]),
        span(0x0a, 0x02, 0, &[]),
        span(0x0a, 0x01, 0, &[]),
        span(0x0a, 0x01, 20, &[]),
        span(0x0a, 0x04, 10, &[]),
        span(0x0a, 0x03, -10, &[]),
    ])
    .await;

    let timeline = bifrost.trace_timeline(trace(0x0a)).await.expect("reads");

    assert_eq!(
        ids(&timeline),
        [
            (0x0a, 0x03),
            (0x0a, 0x01),
            (0x0a, 0x02),
            (0x0b, 0x01),
            (0x0a, 0x04)
        ]
    );
}

/// A malformed or all-zero trace id is refused by the typed id before any
/// request can be made.
#[test]
fn invalid_trace_id_fails_locally() {
    for invalid in ["not-a-trace-id", "00000000000000000000000000000000"] {
        let error = TraceId::from_hex(invalid).expect_err("the id is refused");
        assert_eq!(error.code(), "WYRD_SPEC_400_VALIDATION", "{invalid}");
    }
}

/// A trace no span names reads empty, both before the tenant's first span
/// export creates the span table and after.
///
/// # Panics
///
/// Panics when either read fails or returns a span.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn unseen_trace_returns_empty() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let client = service_client(&srv, "timeline-reader", &["admin"]).await;
    let bifrost = Bifrost::connect_with(&client, BifrostOptions::default())
        .await
        .expect("Bifrost connects");
    assert!(
        bifrost
            .trace_timeline(trace(0x0a))
            .await
            .expect("reads")
            .is_empty()
    );

    let (_srv, bifrost) = spans_written(&[span(0x0b, 0x01, 0, &[])]).await;
    assert!(
        bifrost
            .trace_timeline(trace(0x0a))
            .await
            .expect("reads")
            .is_empty()
    );
}
