# Canonical OpenTelemetry signal storage

- Change: `bifrost-canonical-otel-signals`
- Specification: `SPEC-bifrost-canonical-otel-signals`, approved revision 12
- Completed: 2026-10-01
- Reviewed target: `7d96c30066425e0cde2290842d5801307843283d`
- Delivery reference: not supplied

Bifrost stores traces, logs, and metrics in the canonical `vala.traces.spans`, `vala.logs.records`, and `vala.metrics.points` tables. Each table owns its logical schema, field identities, OTLP projection, Arrow validation, and physical fingerprint. OTLP and canonical Arrow writes converge before WAL preparation and use the same Scribe durability and Oracle SQL read path.

Span rows retain ordered events and links, status, resource and scope data, nanosecond timestamps, lossless attributes, and the approved promoted service and GenAI scalars. Logs and every supported metric point kind retain their complete supported OTLP values. GenAI data remains in its originating OTel signal table; stale trace-child and `vala.genai.*` physical tables and the typed observation read surface were removed.

Records are atomic. Mixed OTLP exports return exact standard partial-success counts, accepted records keep request order and contiguous ordinals, and all-invalid requests create no empty durable batch. Principal identity is mandatory while Card correlation is optional and resolved only from trusted signed scope.

Acceptance closed through table projection and schema tests, OTLP gRPC and HTTP protobuf/JSON journeys, partial-success and refusal journeys, Arrow-to-SQL round trips in Rust/Python/TypeScript, MCP SQL reads, publication/recovery coverage, and stale-surface checks. The approved revision history refined complete signal fidelity, optional Card correlation, unified clients, and canonical SQL as the sole read contract; no material deviation remains.

Current owners and evidence:

- [Bifrost design](../../../architecture/bifrost-design.md).
- [Trace schema](../../../crates/vala/vala-bifrost-redux/src/tables/traces/spans.rs), [trace projection](../../../crates/vala/vala-bifrost-redux/src/tables/traces/projection.rs), [log schema](../../../crates/vala/vala-bifrost-redux/src/tables/logs/records.rs), and [metric schema](../../../crates/vala/vala-bifrost-redux/src/tables/metrics/points.rs).
- [Public OTLP journeys](../../../crates/wyrd/wyrd-testing/tests/bifrost/otlp).

