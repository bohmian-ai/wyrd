Reviewed `99944fefb..f36e0fbb5bfcdc97330ac51f69fc2bcc86fdeeb7` and the untracked Python action-timeline test. Static review only; no builds or tests run.

## F1 — Queued ingest disappears from the tenant timeline

- **Category:** plan
- **File:line:** `crates/shared/wyrd-telemetry/src/export.rs:273`; `crates/vala/vala-bifrost-redux/src/gate/mod.rs:1155`.
- **What breaks:** For ordinary queued `insert`/`observe` calls in every SDK, the background producer sends producer contexts as links but has no active caller context to inject as `traceparent` (`wyrd-queue/src/producer.rs:456`). The server request therefore has no remote parent, receives no tenant route, and its CONSUMER span remains operator-only. Adding links afterwards cannot establish a route. A tenant calling `trace_timeline` sees its enqueue side but cannot see the linked ingest trace, even after flush. Without an operator endpoint that consumer evidence is never retained anywhere. The Rust action journey checks consumer links only in the operator capture (`action_timeline.rs:308`), missing AC-002 / Scenario 13's tenant-timeline obligation.
- **Minimal fix:** Make authenticated ingest with sampled producer contexts eligible for tenant export, retaining a separate consumer trace and producer links. Reconcile REQ-007's blanket exclusion of requests without `traceparent` with this required linked-ingest case; assert the consumer appears in the tenant timeline, including without an operator endpoint. Do not parent a mixed batch on an arbitrary producer. This follows the [OTel batch messaging model](https://opentelemetry.io/docs/specs/semconv/messaging/messaging-spans/).

## F2 — Rust telemetry leaves standard propagation disabled

- **Category:** otel
- **File:line:** `sdks/wyrd-sdk-rust/src/otel.rs:112`.
- **What breaks:** `start_telemetry` installs the tracer but never installs a W3C propagator. OpenTelemetry 0.31's global propagator defaults to `NoopTextMapPropagator`. A Rust service using Wyrd's telemetry setup and an HTTP/gRPC instrumentation layer that injects through the standard global propagator sends no trace headers: its next service, including Wyrd gateway HTTP ingress, starts another trace. The Wyrd SDK's handwritten header injection masks this for its own transport; the Rust action journey also manually constructs `traceparent`, so neither proves the default standard-instrumentation path. Python and TypeScript install their propagators; Rust does not satisfy REQ-006 / Scenario 11.
- **Minimal fix:** Install `opentelemetry_sdk::propagation::TraceContextPropagator` through `opentelemetry::global::set_text_map_propagator` on successful Rust telemetry initialization. Prove outbound headers through the standard propagator in the journey instead of manually formatting them. See [OTel context propagation](https://opentelemetry.io/docs/concepts/context-propagation/).

BLOCKERS
