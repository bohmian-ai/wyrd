//! OTel tracing example - outputs spans to stdout (no infra required).
//!
//! Run with: cargo run -p wyrd --example tracing_stdout --features otel

fn main() {
    use opentelemetry::global;
    use opentelemetry_sdk::trace::TracerProvider;
    use opentelemetry_stdout::SpanExporter;

    let exporter = SpanExporter::default();
    let provider = TracerProvider::builder()
        .with_simple_exporter(exporter)
        .build();
    global::set_tracer_provider(provider);

    use std::sync::Arc;
    use wyrd_observe::{OtelObserver, set_global};

    set_global(Arc::new(OtelObserver::new()));
    wyrd::init();

    println!("OtelObserver installed. Run an agent to see spans on stdout.");
    println!("Requires valid provider credentials to produce real spans.");
}
