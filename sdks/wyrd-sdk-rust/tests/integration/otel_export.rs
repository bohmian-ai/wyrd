//! A stock OpenTelemetry exporter, configured only by `OTEL_EXPORTER_OTLP_*`
//! the way a deployment points one at Wyrd's gRPC address with an
//! `x-wyrd-api-key` header, lands spans Bifrost reads back.
//!
//! The exporter reads that configuration only from its process environment,
//! which a multithreaded test cannot set soundly. So this story, the one Rust
//! journey about ambient configuration, exports from a child process: the
//! test re-runs itself with the `OTEL_EXPORTER_OTLP_*` variables set, and the
//! child prints the exported identities on one stdout line.

use std::process::Command;

use opentelemetry::trace::{Span as _, Status, TraceContextExt, Tracer, TracerProvider as _};
use opentelemetry::{Context, InstrumentationScope, KeyValue};
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::trace::SdkTracerProvider;
use serde::Deserialize;
use serde_json::{Value, json};
use wyrd_sdk::Bifrost;

use crate::support::Deployment;

/// The exact name of the one test in this story.
const TEST: &str = "otel_export::stock_exporter_span_reads_back_through_bifrost";

/// Environment variable marking the exporting child process.
const CHILD: &str = "WYRD_OTEL_EXPORT_CHILD";

/// Prefix of the stdout line on which the child prints the exported
/// identities.
const EXPORTED: &str = "WYRD_OTEL_EXPORTED ";

/// One `vala.traces.spans` row the story reads, identities as hex.
#[derive(Debug, PartialEq, Deserialize)]
struct SpanRow {
    /// The span name.
    name: String,
    /// The trace identity.
    trace_id: String,
    /// The span identity.
    span_id: String,
    /// The parent span identity; `None` for a root span.
    parent_span_id: Option<String>,
    /// The OTLP status code.
    status_code: i32,
    /// The status message.
    status_message: Option<String>,
    /// `gen_ai.request.model`.
    gen_ai_request_model: Option<String>,
    /// `gen_ai.usage.input_tokens`.
    gen_ai_usage_input_tokens: Option<i64>,
}

/// Export an errored `answer` span with GenAI attributes and its child
/// `retrieve` through the stock exporter, and print the `answer` span's
/// trace and span identities after [`EXPORTED`].
///
/// # Panics
/// Panics when the exporter does not build or the export fails.
fn export_spans() {
    let exporter = SpanExporter::builder()
        .with_tonic()
        .build()
        .expect("the stock exporter builds from the environment");
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();
    let tracer = provider.tracer_with_scope(
        InstrumentationScope::builder("wyrd.tests.otel.trace")
            .with_version("1.0.0")
            .build(),
    );
    let mut answer = tracer
        .span_builder("answer")
        .with_attributes([
            KeyValue::new("gen_ai.request.model", "claude-opus-5"),
            KeyValue::new("gen_ai.usage.input_tokens", 4096_i64),
        ])
        .start(&tracer);
    answer.set_status(Status::error("refused"));
    let answer_context = answer.span_context().clone();
    let parent = Context::current().with_remote_span_context(answer_context.clone());
    tracer.start_with_context("retrieve", &parent).end();
    answer.end();
    provider.force_flush().expect("spans export");
    provider.shutdown().expect("the provider stops");
    let exported = json!({
        "trace_id": answer_context.trace_id().to_string(),
        "span_id": answer_context.span_id().to_string(),
    });
    println!("{EXPORTED}{exported}");
}

/// Run [`TEST`] again as a child process whose only configuration is the
/// stock OTLP exporter pointed at `endpoint` with `key` as its
/// `x-wyrd-api-key` header, and return the identities it exported.
///
/// # Panics
/// Panics when the child cannot run, fails, runs no test, or prints no
/// identities.
async fn export_in_child(endpoint: &str, key: &str) -> Value {
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .args([TEST, "--exact", "--include-ignored", "--nocapture"])
        .env(CHILD, "1")
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", endpoint)
        .env(
            "OTEL_EXPORTER_OTLP_HEADERS",
            format!("x-wyrd-api-key={key}"),
        );
    let output = tokio::task::spawn_blocking(move || command.output())
        .await
        .expect("child joins")
        .expect("child starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // libtest exits 0 when `--exact` selects nothing; "1 passed" proves the
    // child actually ran.
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "exporting child failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let line = stdout
        .lines()
        .find_map(|line| line.strip_prefix(EXPORTED))
        .unwrap_or_else(|| panic!("the child printed no identities:\n{stdout}"));
    serde_json::from_str(line).expect("exported identities are JSON")
}

/// The exported spans read back with their identities, parentage, status,
/// and GenAI attributes.
///
/// # Panics
/// Panics when the export or read-back fails or a row differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn stock_exporter_span_reads_back_through_bifrost() {
    if std::env::var_os(CHILD).is_some() {
        export_spans();
        return;
    }
    let deployment = Deployment::start().await;
    let key = deployment.key("otel_exporter", &["admin"]).await;
    let grpc = deployment
        .server()
        .grpc_url()
        .expect("bound server has a gRPC URL");

    let exported = export_in_child(&grpc, &key).await;
    deployment
        .server()
        .flush_bifrost()
        .await
        .expect("spans publish");

    let rows: Vec<SpanRow> = Bifrost::connect(&deployment.admin())
        .await
        .expect("Bifrost connects")
        .sql_as(
            "SELECT name, encode(trace_id, 'hex') AS trace_id, encode(span_id, 'hex') AS span_id, \
                    encode(parent_span_id, 'hex') AS parent_span_id, status_code, status_message, \
                    gen_ai_request_model, gen_ai_usage_input_tokens \
               FROM vala.traces.spans WHERE scope_name = 'wyrd.tests.otel.trace' ORDER BY name",
            &[],
        )
        .await
        .expect("spans read back");
    let trace_id = exported["trace_id"].as_str().expect("trace id").to_owned();
    let span_id = exported["span_id"].as_str().expect("span id").to_owned();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(
        rows[0],
        SpanRow {
            name: "answer".to_owned(),
            trace_id: trace_id.clone(),
            span_id: span_id.clone(),
            parent_span_id: None,
            status_code: 2,
            status_message: Some("refused".to_owned()),
            gen_ai_request_model: Some("claude-opus-5".to_owned()),
            gen_ai_usage_input_tokens: Some(4096),
        }
    );
    assert_eq!(
        (
            rows[1].name.as_str(),
            &rows[1].trace_id,
            rows[1].parent_span_id.as_ref()
        ),
        ("retrieve", &trace_id, Some(&span_id))
    );
    deployment.shutdown().await;
}
