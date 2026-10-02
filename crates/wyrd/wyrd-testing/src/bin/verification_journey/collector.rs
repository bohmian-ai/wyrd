//! The two local endpoints the server calls out to: the HTTP Operator each
//! tenant's failed Eval verdicts dispatch to, and the OTLP/gRPC trace
//! collector the server exports its own sampled spans to.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;
use wyrd_tonic::otlp::common::v1::any_value;
use wyrd_tonic::otlp::trace::v1::Span;
use wyrd_tonic::otlp::trace_service::trace_service_server::{TraceService, TraceServiceServer};
use wyrd_tonic::otlp::trace_service::{ExportTraceServiceRequest, ExportTraceServiceResponse};
use wyrd_tonic::tonic::{Request, Response, Status};

use crate::Result;

/// Path prefix of the Operator endpoint; the tenant slug follows.
const OPERATOR_PATH: &str = "/operator";

/// Phase spans every correlated attempt trace carries under its
/// `verification.attempt` root.
const ATTEMPT_PHASES: [&str; 4] = [
    "claim",
    "verification.load",
    "verification.engine",
    "verification.settle",
];

/// Every span the server exported, in arrival order.
#[derive(Clone, Default)]
struct SpanSink(Arc<Mutex<Vec<Span>>>);

#[wyrd_tonic::tonic::async_trait]
impl TraceService for SpanSink {
    /// Keeps every exported span.
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> std::result::Result<Response<ExportTraceServiceResponse>, Status> {
        let spans = request
            .into_inner()
            .resource_spans
            .into_iter()
            .flat_map(|resource| resource.scope_spans)
            .flat_map(|scope| scope.spans);
        self.0
            .lock()
            .map_err(|_| Status::internal("span sink poisoned"))?
            .extend(spans);
        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}

/// The running local endpoints.
pub struct Collector {
    /// Operator endpoint recording every delivery.
    operator: wiremock::MockServer,
    /// Spans the server exported.
    spans: SpanSink,
    /// `http://` address of the trace collector.
    trace_endpoint: String,
    /// Stops the trace collector.
    stop: CancellationToken,
}

impl Collector {
    /// Starts both endpoints on loopback ports the OS assigns.
    ///
    /// # Errors
    ///
    /// Returns a bind failure.
    pub async fn start() -> Result<Self> {
        let operator = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&operator)
            .await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let trace_endpoint = format!("http://{}", listener.local_addr()?);
        let spans = SpanSink::default();
        let stop = CancellationToken::new();
        let router = wyrd_tonic::tonic::transport::Server::builder()
            .add_service(TraceServiceServer::new(spans.clone()));
        let serving = stop.clone();
        tokio::spawn(async move {
            if let Err(error) =
                wyrd_tonic::server::serve_grpc_with_listener(router, listener, serving).await
            {
                eprintln!("trace collector stopped: {error}");
            }
        });
        Ok(Self {
            operator,
            spans,
            trace_endpoint,
            stop,
        })
    }

    /// The URL `slug`'s Operator Card posts to.
    pub fn operator_url(&self, slug: &str) -> String {
        format!("{}{OPERATOR_PATH}/{slug}", self.operator.uri())
    }

    /// The address the server's `WYRD_OTLP_ENDPOINT` names.
    pub fn trace_endpoint(&self) -> &str {
        &self.trace_endpoint
    }

    /// Operator deliveries received per tenant slug.
    pub async fn operator_posts(&self) -> BTreeMap<String, u64> {
        let mut posts = BTreeMap::new();
        for request in self.operator.received_requests().await.unwrap_or_default() {
            if let Some(slug) = request
                .url
                .path()
                .strip_prefix(OPERATOR_PATH)
                .and_then(|rest| rest.strip_prefix('/'))
            {
                *posts.entry(slug.to_owned()).or_default() += 1;
            }
        }
        posts
    }

    /// Stops the trace collector and summarizes what the server exported.
    pub fn finish(&self, tenant_ids: &[String], forbidden: &[String]) -> TraceSummary {
        self.stop.cancel();
        let spans = self
            .spans
            .0
            .lock()
            .map(|spans| spans.clone())
            .unwrap_or_default();
        TraceSummary::of(&spans, tenant_ids, forbidden)
    }
}

/// What the sampled server traces show.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TraceSummary {
    /// Spans exported.
    pub spans: usize,
    /// Sampled `verification.attempt` roots.
    pub attempts: usize,
    /// Attempts whose trace holds every [`ATTEMPT_PHASES`] span.
    pub correlated: usize,
    /// Attempts whose trace also holds a `verification.evidence_read` span.
    pub evidence_reads: usize,
    /// Descriptions of spans leaking a tenant id, credential, or evidence.
    pub leaks: Vec<String>,
}

impl TraceSummary {
    /// Correlates `spans` by trace and scans their attributes.
    ///
    /// A `verification.*` span carrying any of `tenant_ids`, or any span
    /// carrying one of `forbidden` (credentials and the evidence marker),
    /// is a leak.
    fn of(spans: &[Span], tenant_ids: &[String], forbidden: &[String]) -> Self {
        let mut by_trace: BTreeMap<&[u8], Vec<&Span>> = BTreeMap::new();
        for span in spans {
            by_trace.entry(&span.trace_id).or_default().push(span);
        }
        let mut summary = Self {
            spans: spans.len(),
            ..Self::default()
        };
        for root in spans
            .iter()
            .filter(|span| span.name == "verification.attempt")
        {
            summary.attempts += 1;
            let trace = by_trace
                .get(root.trace_id.as_slice())
                .cloned()
                .unwrap_or_default();
            let has = |name: &str| trace.iter().any(|span| span.name == name);
            summary.correlated += usize::from(ATTEMPT_PHASES.iter().all(|phase| has(phase)));
            summary.evidence_reads += usize::from(has("verification.evidence_read"));
        }
        for span in spans {
            for attribute in &span.attributes {
                let Some(any_value::Value::StringValue(value)) = attribute
                    .value
                    .as_ref()
                    .and_then(|value| value.value.as_ref())
                else {
                    continue;
                };
                let tenant = span.name.starts_with("verification.")
                    && tenant_ids.iter().any(|id| value.contains(id.as_str()));
                if tenant
                    || forbidden
                        .iter()
                        .any(|secret| value.contains(secret.as_str()))
                {
                    summary
                        .leaks
                        .push(format!("{} attribute {}", span.name, attribute.key));
                }
            }
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
    use wyrd_tonic::otlp::trace::v1::Span;

    use super::TraceSummary;

    /// A span in trace `trace` named `name` with string attributes.
    fn span(trace: u8, name: &str, attributes: &[(&str, &str)]) -> Span {
        Span {
            trace_id: vec![trace; 16],
            name: name.to_owned(),
            attributes: attributes
                .iter()
                .map(|(key, value)| KeyValue {
                    key: (*key).to_owned(),
                    value: Some(AnyValue {
                        value: Some(any_value::Value::StringValue((*value).to_owned())),
                    }),
                    ..KeyValue::default()
                })
                .collect(),
            ..Span::default()
        }
    }

    /// An attempt is correlated only when its own trace holds every phase,
    /// and a tenant id on a verification span or a credential anywhere is a
    /// leak while a tenant id on another span is not.
    ///
    /// # Panics
    ///
    /// Panics when a count or leak is wrong.
    #[test]
    fn attempts_correlate_by_trace_and_leaks_are_found() {
        let mut spans = vec![span(1, "verification.attempt", &[("outcome", "completed")])];
        for phase in [
            "claim",
            "verification.load",
            "verification.engine",
            "verification.settle",
        ] {
            spans.push(span(1, phase, &[]));
        }
        spans.push(span(1, "verification.evidence_read", &[]));
        spans.push(span(2, "verification.attempt", &[("run_id", "tenant-a")]));
        spans.push(span(2, "claim", &[]));
        spans.push(span(
            3,
            "request",
            &[("tenant", "tenant-a"), ("auth", "wyrd_key_x")],
        ));
        let summary =
            TraceSummary::of(&spans, &["tenant-a".to_owned()], &["wyrd_key_x".to_owned()]);
        assert_eq!(summary.attempts, 2);
        assert_eq!(summary.correlated, 1);
        assert_eq!(summary.evidence_reads, 1);
        assert_eq!(
            summary.leaks,
            vec![
                "verification.attempt attribute run_id".to_owned(),
                "request attribute auth".to_owned()
            ]
        );
    }
}
