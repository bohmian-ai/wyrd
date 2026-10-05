# Telemetry-first tiered-test debugging

- Change: `telemetry-first-tiered-test-debugging`
- Specification: `SPEC-telemetry-first-tiered-test-debugging`, approved revision 1
- Completed: 2026-10-01
- Reviewed target: `7d96c30066425e0cde2290842d5801307843283d`
- Delivery reference: not supplied

Tiered Bifrost tests now install production-shaped telemetry capture and use traces, metrics, and structured events as the first debugging evidence. Multi-process capture remains available while a child or control request is blocked, and panic reporting emits a bounded correlated snapshot without exposing secrets or unbounded payloads.

Missing evidence is repaired at the production telemetry owner so the same signal helps operators and tests. Repository checks reject ad hoc direct-print debugging outside the narrow, documented evidence-reporting boundary, and both supported agent harnesses share the same debugging workflow rule.

Acceptance closed through blocked-child telemetry recovery, capture-once tests, actionable direct-print check failures, synchronized workflow instructions, and existing tiered journey verification. The change did not alter product behavior, test assertions, process topology, or tier ownership; no material deviation remains.

Current owners and evidence:

- [Testing map](../../../TESTING.md), [agent rules](../../../architecture/agent-rules.md), and [Bifrost design](../../../architecture/bifrost-design.md).
- [Telemetry runtime](../../../crates/shared/wyrd-runtime/src/otel.rs), [Bifrost test capture](../../../crates/wyrd/wyrd-testing/src/bifrost/telemetry.rs), and [process harness](../../../crates/wyrd/wyrd-testing/src/bifrost/cluster.rs).
- [Repository checks](../../../scripts/checks).

