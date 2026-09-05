---
id: BIFROST-R6-T05-R02-OTEL-JOURNEY
title: Prove actual OTEL trace debugging over MCP
kind: remediation
status: review
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 6
parent_task: BIFROST-R5-T05-MCP
depends_on: [BIFROST-R6-T05-R01-MCP-QUERY-CORRECTNESS]
requirements: [REQ-010, REQ-012]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-009]
acceptance: [AC-003, AC-004, AC-006, AC-009]
reviewed_candidate: 910cbcd98
remediates: [FIND-BIFROST-R5-T05-MCP-6]
---

# Actual trace debugging journey

Required execution skill: `$wyrd-implement`.

## Outcome, owners, and scope

Close the validated gap in `reviews/05-closeout-review.md`: a real MCP agent
reads an actual error span from `vala.traces.spans`, not a generic id/value table.
Modify only the existing `agent_debugs_otel_error_trace_through_mcp` journey in
`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs` and its execution evidence.
The existing HTTP OTLP handler, Scribe built-in registration and flush, Oracle,
MCP handler, and runtime remain the production owners. No new helper, fixture
framework, dependency, contract, protocol, or production behavior is required.

## Ordered scenario — discover, query, and settle real error spans

**Behavior.** Bootstrap an authorized server caller with the existing
`WyrdTestServer::bootstrap_service` and `exchange_api_key`, ingest two valid
OTLP JSON spans through authenticated `POST /v1/traces` using the already
installed `reqwest`, and `flush_bifrost` before reading PublishedOnly. Use
current timestamps, fixed nonzero trace/span IDs, a service resource attribute,
one OK and one ERROR status, and distinct span names. An actual MCP client
lists `vala.traces.spans`, describes its `name` and `status` fields, then submits
a bounded time-filtered SELECT for the ERROR status. Assert ordered columns,
exact positional values for the error span, Interactive success, complete
freshness, and matching row count.

**RED.** Add the real trace-table discovery assertion to the existing journey
while it still seeds the generic fixture; the table-discovery assertion must
fail. Use this exact command:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey -E 'test(=query::pg_tests::agent_debugs_otel_error_trace_through_mcp)' --run-ignored=all"
```

**GREEN.** Replace generic fixture setup in this journey with the real OTLP
HTTP setup above. Select the existing non-null Utf8 `name` and `status` fields;
use the discovered table for both the bounded ERROR query and the two-row
ceiling/cancellation queries. Preserve canonical too-large errors with no rows,
the existing post-schema stall and lifecycle observer, and released admission,
memory, peer slots, and tail fences before disconnect. Keep the generic fixture
for the separate malformed-query journey unchanged.

**REFACTOR.** Keep setup local to this one existing test. Retain its real MCP
transport and deterministic cancellation synchronization. Add no sleeps or
production changes.

## Verification and completion

Run the focused command, `mise run test:bifrost:journey:mcp`, `mise run fmt`,
`mise run lints`, and `git diff --check`. Record RED/GREEN and command outcomes.
Reuse the correctness remediation's broader engine/Oracle/boundary evidence
because this task changes only one MCP journey.

Authority: `AGENTS.md`; `architecture/agent-rules.md`;
`architecture/wyrd-design.md` Doctrine 20;
`architecture/bifrost-design.md` Public surface;
`architecture/wyrd-security-posture.md`;
`architecture/references/languages/{implementation-execution,testing-workflows,agent-harness}.md`;
and approved `spec.md` revision 6 Journey E / AC-009.

If the existing OTLP production path cannot ingest valid spans or Oracle cannot
serve the required trace query, preserve the failing evidence and return for
bounded plan revision; do not hide the failure with a generic table or direct
in-process ingestion.

## Readiness

Independent `wyrd-task-readiness` reviewer returned READY with no blockers.

## Execution evidence

RED: the exact focused command above failed at the actual OTEL table discovery
assertion (1 failed, 5 skipped; 3.324s), with only the generic fixture registered.

GREEN attempt exposed a production dependency failure before MCP discovery:
`POST /v1/traces` returns `WYRD_VALA_413_PAYLOAD_TOO_LARGE`, reporting
8193 bytes against a 2440-byte limit for two valid small spans. The exact
focused command failed (1 failed, 5 skipped; 2.489s). The journey retains the
structured response in its assertion so this refusal is diagnosable.

`TASK_REVISION_REQUIRED`: the test-only plan cannot complete. Scribe's
`material_plan.rs::OtlpCounts::finish` estimates current material from request
bytes, value bytes, and managed projection; `ingress.rs` passes that estimate
as the direct projection material limit. The exact Arrow plus IPC requirement
checked by `otlp_managed.rs::OtlpManagedMaterialPlan::admitted_bytes` exceeds
that estimate. Correcting the pre-admission sizing in the Scribe owner requires
a production material-planning task and its allocation/admission evidence;
increasing an arbitrary limit or padding the test request is not a valid fix.
The source already has exact trace/log/metric projection sizing, which should
be examined for reuse before choosing a conservative bound or shared exact
planning. Existing ownership, pre-reservation, bounded materialization, WAL,
and replay guarantees must remain intact. No production workaround was made.

Final test-only candidate verification: `mise run fmt` and `mise run lints`
PASS (workspace linting 37.16s). `mise run test:bifrost:journey:mcp` FAIL:
5 passed, 1 failed (the real OTLP journey, same HTTP 413 sizing mismatch).
The original discovery/authentication/malformed-input MCP journeys remain green.

Production dependency remediation `05-remediation-otlp-material.md` restores
GREEN: the exact OTLP MCP selector passed (2.551s) after correcting Scribe
projection accounting. The earlier TASK_REVISION_REQUIRED condition is resolved
by that independently reviewed task; final cumulative verification follows.

Final closure proof: exact real OTLP MCP journey PASS (2.628s), full MCP lane
PASS (6/6), full Oracle lane PASS (23/23), Bifrost integration PASS (965/965),
format and workspace lints PASS. Both trace coverage finding 6 and its Scribe
admission dependency finding 7 are ready for cumulative review.
