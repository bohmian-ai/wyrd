//! Reusable OTLP workload builders for integration tests and benchmarks.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use wyrd_tonic::otlp::common::v1::{AnyValue, InstrumentationScope, KeyValue, any_value};
use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
use wyrd_tonic::otlp::trace::v1::{
    ResourceSpans, ScopeSpans, Span as OtlpSpan, Status as OtlpStatus, span, status::StatusCode,
};
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

const MIN_SPANS_PER_TRACE: usize = 3;
const MAX_SPANS_PER_TRACE: usize = 12;
const DEFAULT_TRACE_START_TIME_NANOS: u64 = 1_700_000_000_000_000_000;

/// Seeded random OTLP trace generator.
///
/// The generator creates a batch with a realistic resource and instrumentation
/// scope, then fills it with one or more traces. Each trace has a random number
/// of spans, parent/child relationships, attributes, events, status, and links.
/// A seed makes a benchmark run reproducible while keeping IDs and topology
/// independent across requests.
#[derive(Debug, Clone)]
pub struct RandomTraceGenerator {
    rng: StdRng,
    start_time_nanos: u64,
}

impl RandomTraceGenerator {
    /// Create a reproducible generator from a caller-provided seed.
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        Self::from_seed_at(seed, DEFAULT_TRACE_START_TIME_NANOS)
    }

    /// Create a reproducible generator with an explicit event-time anchor.
    ///
    /// Benchmarks use a current-time anchor so their default bounded query
    /// window can verify the rows they just wrote; tests can keep the stable
    /// historical anchor from [`Self::from_seed`].
    #[must_use]
    pub fn from_seed_at(seed: u64, start_time_nanos: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            start_time_nanos,
        }
    }

    /// Generate one OTLP export request containing exactly `span_count` spans.
    ///
    /// The request uses multiple traces when the requested count is larger than
    /// the randomly selected size of one trace. The sequence is included in the
    /// resource instance attribute to make failed/replayed requests diagnosable.
    #[must_use]
    pub fn export_request(
        &mut self,
        tenant_index: usize,
        sequence: u64,
        span_count: usize,
    ) -> ExportTraceServiceRequest {
        let mut remaining = span_count;
        let mut resource_spans = Vec::new();
        let mut trace_index = 0usize;
        while remaining > 0 {
            let trace_span_count = self.next_trace_span_count(remaining);
            resource_spans.push(self.resource_spans(
                tenant_index,
                sequence,
                trace_index,
                trace_span_count,
            ));
            remaining -= trace_span_count;
            trace_index += 1;
        }
        ExportTraceServiceRequest { resource_spans }
    }

    fn next_trace_span_count(&mut self, remaining: usize) -> usize {
        if remaining <= MIN_SPANS_PER_TRACE {
            return remaining;
        }
        let upper = remaining.min(MAX_SPANS_PER_TRACE);
        self.rng.random_range(MIN_SPANS_PER_TRACE..=upper)
    }

    fn resource_spans(
        &mut self,
        tenant_index: usize,
        sequence: u64,
        trace_index: usize,
        span_count: usize,
    ) -> ResourceSpans {
        let trace_id = self.random_id::<16>();
        let mut spans = Vec::with_capacity(span_count);
        let mut span_ids = Vec::with_capacity(span_count);
        let base_time = self
            .start_time_nanos
            .saturating_add(sequence.saturating_mul(1_000_000_000));
        for span_index in 0..span_count {
            span_ids.push(self.random_id::<8>());
            let current_span_id = span_ids[span_index];
            let parent_span_id = span_index
                .checked_sub(1)
                .map_or_else(Vec::new, |parent| span_ids[parent].to_vec());
            let kind = self.span_kind(span_index);
            let failed = self.rng.random_bool(0.02);
            let start_time =
                base_time.saturating_add(u64::try_from(span_index).unwrap_or(u64::MAX) * 1_000_000);
            let linked_trace_id =
                (trace_index > 0 && span_index == 2).then(|| self.random_id::<16>());
            let links = linked_trace_id.map_or_else(Vec::new, |linked_trace_id| {
                vec![span::Link {
                    trace_id: linked_trace_id.to_vec(),
                    span_id: self.random_id::<8>().to_vec(),
                    trace_state: "vendor=otlp-bench".to_owned(),
                    attributes: vec![kv(
                        "link.type",
                        any_value::Value::StringValue("fanout".to_owned()),
                    )],
                    dropped_attributes_count: 0,
                    flags: 1,
                }]
            });
            spans.push(OtlpSpan {
                trace_id: trace_id.to_vec(),
                span_id: current_span_id.to_vec(),
                parent_span_id,
                trace_state: "vendor=otlp-bench".to_owned(),
                flags: 1,
                name: span_name(kind, failed),
                kind: kind as i32,
                start_time_unix_nano: start_time,
                end_time_unix_nano: start_time
                    .saturating_add(self.rng.random_range(100_000_u64..=2_000_000_u64)),
                attributes: vec![
                    kv(
                        "http.method",
                        any_value::Value::StringValue("GET".to_owned()),
                    ),
                    kv(
                        "http.route",
                        any_value::Value::StringValue("/checkout".to_owned()),
                    ),
                    kv(
                        "http.status_code",
                        any_value::Value::IntValue(if failed { 500 } else { 200 }),
                    ),
                    kv(
                        "tenant.index",
                        any_value::Value::IntValue(i64::try_from(tenant_index).unwrap_or(i64::MAX)),
                    ),
                    kv(
                        "benchmark.sequence",
                        any_value::Value::IntValue(i64::try_from(sequence).unwrap_or(i64::MAX)),
                    ),
                    kv("sampled", any_value::Value::BoolValue(true)),
                ],
                dropped_attributes_count: 0,
                events: vec![span::Event {
                    time_unix_nano: start_time.saturating_add(50_000),
                    name: if failed { "exception" } else { "cache.lookup" }.to_owned(),
                    attributes: vec![kv(
                        "cache.hit",
                        any_value::Value::BoolValue(!failed && self.rng.random_bool(0.8)),
                    )],
                    dropped_attributes_count: 0,
                }],
                dropped_events_count: 0,
                links,
                dropped_links_count: 0,
                status: Some(OtlpStatus {
                    message: if failed {
                        "checkout dependency failed".to_owned()
                    } else {
                        String::new()
                    },
                    code: if failed {
                        StatusCode::Error as i32
                    } else {
                        StatusCode::Ok as i32
                    },
                }),
            });
        }

        ResourceSpans {
            resource: Some(OtlpResource {
                attributes: vec![
                    kv(
                        "service.name",
                        any_value::Value::StringValue("checkout-api".to_owned()),
                    ),
                    kv(
                        "service.version",
                        any_value::Value::StringValue("2026.07".to_owned()),
                    ),
                    kv(
                        "deployment.environment",
                        any_value::Value::StringValue("benchmark".to_owned()),
                    ),
                    kv(
                        "service.instance.id",
                        any_value::Value::StringValue(format!("loadgen-{tenant_index}")),
                    ),
                ],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "wyrd.otlp.benchmark".to_owned(),
                    version: "1.0.0".to_owned(),
                    attributes: vec![kv(
                        "instrumentation.language",
                        any_value::Value::StringValue("rust".to_owned()),
                    )],
                    dropped_attributes_count: 0,
                }),
                spans,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }
    }

    fn span_kind(&mut self, span_index: usize) -> span::SpanKind {
        match span_index {
            0 => span::SpanKind::Server,
            1 => span::SpanKind::Internal,
            2 => span::SpanKind::Client,
            _ if self.rng.random_bool(0.25) => span::SpanKind::Producer,
            _ => span::SpanKind::Internal,
        }
    }

    fn random_id<const N: usize>(&mut self) -> [u8; N] {
        let mut bytes = [0_u8; N];
        self.rng.fill(&mut bytes);
        if bytes.iter().all(|byte| *byte == 0) {
            bytes[N - 1] = 1;
        }
        bytes
    }
}

fn span_name(kind: span::SpanKind, failed: bool) -> String {
    match kind {
        span::SpanKind::Server => "GET /checkout".to_owned(),
        span::SpanKind::Client => "SELECT checkout".to_owned(),
        span::SpanKind::Producer => "publish checkout.updated".to_owned(),
        _ if failed => "checkout.failure".to_owned(),
        _ => "checkout.process".to_owned(),
    }
}

fn kv(key: &str, value: any_value::Value) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue { value: Some(value) }),
    }
}

#[cfg(test)]
mod tests {
    use vala_bifrost_redux::gate::collector::project_resource_spans;
    use wyrd_tonic::prost::Message;

    use super::RandomTraceGenerator;

    #[test]
    fn seeded_generator_emits_exact_span_count() {
        let mut generator = RandomTraceGenerator::from_seed(7);
        let request = generator.export_request(2, 11, 25);
        let spans = request
            .resource_spans
            .iter()
            .flat_map(|resource| resource.scope_spans.iter())
            .flat_map(|scope| scope.spans.iter())
            .collect::<Vec<_>>();
        assert_eq!(spans.len(), 25);
        assert!(spans.iter().all(|span| span.trace_id.len() == 16));
        assert!(spans.iter().all(|span| span.span_id.len() == 8));
        assert!(spans.iter().any(|span| !span.events.is_empty()));
        assert!(request.encoded_len() > 1_000);
    }

    #[test]
    fn seeded_generator_is_reproducible() {
        let mut left = RandomTraceGenerator::from_seed(42);
        let mut right = RandomTraceGenerator::from_seed(42);
        assert_eq!(
            left.export_request(0, 1, 20),
            right.export_request(0, 1, 20)
        );
    }

    #[test]
    fn generated_batches_keep_one_stable_arrow_schema() {
        let mut generator = RandomTraceGenerator::from_seed(11);
        let one = project_resource_spans(&generator.export_request(0, 0, 1))
            .expect("one-span request projects")
            .batch
            .expect("one-span request has a batch");
        let many = project_resource_spans(&generator.export_request(0, 1, 10))
            .expect("multi-span request projects")
            .batch
            .expect("multi-span request has a batch");
        assert_eq!(one.schema(), many.schema());
    }
}
