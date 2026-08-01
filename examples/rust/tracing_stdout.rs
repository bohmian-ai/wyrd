//! OTel tracing example - outputs spans to stdout (no infra required).
//!
//! Run with: cargo run -p wyrd-rust-examples --bin tracing_stdout

fn main() {
    use opentelemetry::global;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use opentelemetry_stdout::SpanExporter;

    let exporter = SpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter)
        .build();
    global::set_tracer_provider(provider);

    use skald_observer::{OtelObserver, set_global};
    use std::sync::Arc;

    set_global(Arc::new(OtelObserver::new()));
    wyrd::init();

    println!("OtelObserver installed. Run an agent to see spans on stdout.");
    println!("Requires valid provider credentials to produce real spans.");
}
