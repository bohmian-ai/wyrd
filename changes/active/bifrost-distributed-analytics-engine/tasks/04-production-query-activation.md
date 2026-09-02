---
id: BIFROST-R3-T04-PRODUCTION-ACTIVATION
title: Activate server-owned Analytical selection through the one public query contract
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 3
depends_on: [BIFROST-R3-T03-PHYSICAL-BASELINE]
requirements: [REQ-001, REQ-002, REQ-003, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008]
parent_task: BIFROST-R3-T2-PRODUCTION-ACTIVATION
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# Production query activation

## Outcome and value

The existing authenticated raw-SQL operation conservatively chooses
Interactive or Task 3's qualified Analytical path without caller hints.
Analytical selection occurs only after a supported distributed physical plan
with a real exchange and successful path admission; every later failure is
terminal. Rust, HTTP/gRPC, and internal scheduled callers receive the same
stream, selected-path terminal, errors, audit, cancellation, and cleanup.

Required execution skill: `$wyrd-implement`.

## Current-state amendment

Retain `OraclePlanner`'s optimized-plan classification, immutable
`PlannedSqlCut`, current authenticated query routes, audit-WAL acceptance,
terminal-safe stream, lifecycle cancellation service, and internal
`AppState::query_sql` seam. Do not create exact routing facts, public EXPLAIN,
path hints, a query-job schema, or automatic retry. Replace the current
Interactive-only dispatch after the qualified Analytical handle is integrated.

## Owners, scope, consumers, and non-goals

- `vala-bifrost-redux::Oracle` owns prepare, candidate classification, physical
  planning/validation, path admission, selection, execution, and settlement.
- `oracle/admission.rs` owns path counters/queues and the Interactive floor.
- `oracle/query_stream.rs` owns one terminal contract.
- `wyrd-spec::vala::api` and proto source own the closed terminal execution path;
  `wyrd-tonic`, `wyrd-server`, and `vala-sdk` project it.
- `wyrd-server` owns audit-WAL-before-rows, public/private listener composition,
  lifecycle cancellation, readiness, shutdown, and internal scheduled calls.
- `wyrd-testing` owns Rust/HTTP/gRPC and multi-process journeys.
- The existing test-tier `WyrdTestServer` owner gains one internal
  multi-node-Oracle composition option consumed unchanged by the Python,
  TypeScript, and MCP journey tasks; no language-specific cluster handle is
  exported.

Python, TypeScript, and MCP belong to Tasks 5–7. Do not repeat Task 3's physical
operator qualification here.

## Ordered implementation scenarios

### Scenario 1 — One request and one typed terminal path

**Behavior.** The request remains SQL, visibility, freshness, and optional
deadline only. A successful or failed started stream carries exactly one closed
`Interactive | Analytical` selected path; no graph, class, workers, plan, or
selector becomes public. Maps REQ-001, REQ-008, INV-001, INV-008, AC-006, AC-008.

**RED.** Add source/schema tests proving request closure and terminal path:
`vala::api::tests::query_terminal_projects_path_without_request_selector` in
`wyrd-spec`. Exact:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::api::tests::query_terminal_projects_path_without_request_selector)'
```

It fails because the current terminal has no selected path.

**GREEN.** Add `QueryExecutionPath::{Interactive, Analytical}` to the
language-neutral terminal source, derive required schema/proto conversions, and
thread the server-selected value through `vala-sdk`. Do not alter
`BifrostQueryRequest`. Pre-stream admission errors remain structured request
errors; started streams terminate explicitly and preceding failure frames never
form a successful result.

**REFACTOR.** Generated OpenAPI/schema/stubs come only from owners; no hand edits
or persisted query-path row.

### Scenario 2 — Conservative selection state machine

**Behavior.** Interactive is default. Existing classification nominates an
Analytical candidate; Task 3's supported physical plan plus real exchange and
successful Analytical admission irreversibly select it. Unsupported shape,
safe planning failure, or no exchange falls back before selection only. Maps
REQ-002, REQ-003, INV-001, INV-008, AC-002, AC-003, AC-008.

**RED.** Add
`oracle::tests::analytical_selection_requires_supported_physical_exchange`.
Mutate candidate, planner result, supported predicate, exchange presence, and
admission result; assert fallback only before admission/selection and no second
optimizer/facts owner. Exact:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::tests::analytical_selection_requires_supported_physical_exchange)'
```

**GREEN.** Implement on `Oracle` the closed transition
`Prepared -> InteractiveSelected` or `AnalyticalCandidate -> PhysicalPlanned ->
SupportedExchange -> AnalyticalAdmitted -> AnalyticalSelected`. Use
`PlannedSqlCut`'s existing classification inputs and Task 3's pure validator.
Planning/support/no-exchange errors may enter Interactive only while state is
pre-selection and Interactive can execute safely. Admission refusal and every
later error are Analytical failures.

**REFACTOR.** Keep transition state private and cohesive on Oracle; query class
does not become the execution-path authority.

### Scenario 3 — Path capacity and Interactive floor

**Behavior.** Separate path accounting sits under one aggregate root;
Analytical saturation never consumes the Interactive floor; refusal mutates no
grant/counter. Maps REQ-005, REQ-006, INV-005, INV-006, AC-004.

**RED.** Add
`oracle::admission::tests::analytical_saturation_preserves_interactive_floor`.
Fill all non-floor slots, race an Analytical refusal with an Interactive grant,
and assert aggregate/path counts plus existing grants before/after. Exact:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::admission::tests::analytical_saturation_preserves_interactive_floor)'
```

**GREEN.** Extend the existing atomic admission owner with closed path queues and
counters; reserve `interactive_floor_slots` before any Analytical grant. Let
Interactive use idle non-floor capacity. Both paths retain one query envelope
and the same aggregate memory/scratch roots. Fail startup for invalid finite
floor/total configuration using the existing config owner.

**REFACTOR.** No per-path memory roots or duplicate admission service.

### Scenario 4 — Public UI and distributed Rust journeys

**Behavior.** A real UI query selects Interactive while Analytical capacity is
full; a real data-scientist query selects Analytical without a hint; exact
results and selected terminals return with zero ownership. Maps REQ-001,
REQ-002, REQ-003, REQ-006, REQ-008, REQ-009, REQ-011, AC-002–AC-007.

**RED.** Add
`analytical_public::public_query_selects_both_paths_and_preserves_interactive_floor`
to the Oracle journey target. Use one-Oracle/one-Scribe for the UI case and the
qualified three-Oracle/one-Scribe fixture for Analytical; assert raw request
has no path field, terminals differ, trusted result parity, UI service under
pressure, and all production gauges/owners return to baseline. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_public::public_query_selects_both_paths_and_preserves_interactive_floor)' --run-ignored=all"
```

**GREEN.** Wire the selected path into the existing public Oracle stream; reuse
Task 2's admitted guard and Task 3's handle. Extend only existing production
telemetry for candidate/selection/fallback/path terminal and ensure all active
gauges settle through the joined owner.

**REFACTOR.** Do not copy the physical operator matrix; consume Task 3 evidence.

### Scenario 5 — Pre-selection fallback versus post-selection failure

**Behavior.** Unsupported/no-exchange candidates may run Interactive; injected
peer/transport/resource/cancellation/deadline/cleanup failure after selection
returns one failed Analytical terminal, no rerun, and no successful partial
rows. Maps REQ-002, REQ-007, REQ-008, INV-003, INV-004, AC-003, AC-004.

**RED.** Add
`analytical_public::fallback_is_preselection_only_and_failure_is_terminal`.
Drive a no-exchange candidate and an injected post-selection peer loss; assert
path/attempt/terminal/audit/ownership. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_public::fallback_is_preselection_only_and_failure_is_terminal)' --run-ignored=all"
```

**GREEN.** Carry the locked selection into `query_stream`; remove every error
edge from selected Analytical to Interactive. Await joined cleanup before the
failed terminal. Preserve stable Wyrd error mapping at HTTP/gRPC/Rust boundaries.

**REFACTOR.** One terminal constructor serves both paths; only the selected path
and typed outcome vary.

### Scenario 6 — Audit, internal scheduled caller, and lifecycle cancellation

**Behavior.** One audit WAL acceptance fsyncs before rows; stages append none.
The server-owned scheduled caller uses the same query operation, selects
Analytical for a supported global query, and cancellation/deadline settles the
whole graph. Maps REQ-008, REQ-009, REQ-011, INV-001, INV-002, AC-003, AC-005,
AC-007.

**RED.** Add
`query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup`
to the server journey. Assert one accepted/relayed logical read, no stage audit,
generated gRPC terminal path/error, same `AppState::query_sql` route for the
scheduled caller, cancellation, private peer isolation, readiness, and zero
ownership. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey -E 'test(=query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup)' --run-ignored=all"
```

**GREEN.** Preserve the existing audit acceptance before either engine can emit
rows; pass the authenticated context, permission digest, pinned cut, request ID,
and deadline unchanged into Analytical. Route internal calls through the same
Oracle method and lifecycle cancellation service. Keep peer services private
and included in readiness/ordered shutdown. In the test tier only, compose the
existing multi-node Oracle fixture behind the current `WyrdTestServer` handle
and one Rust-native configuration option; downstream runtimes consume this
fixture without editing its lifecycle or exporting a cluster class.

**REFACTOR.** No alternate internal analytical method or audit writer.

## Cross-scenario decisions and authority

Selection is request-lifecycle state, not durable SQL state. The real-exchange
check occurs on the built physical plan. Production telemetry labels use closed
path/reason/outcome values only; IDs and SQL stay scrubbed traces.

Authority: `architecture/wyrd-design.md`, `architecture/bifrost-design.md`,
`architecture/wyrd-security-posture.md`,
`architecture/references/domain/olap-serving.md`,
`architecture/references/languages/errors.md`, and
`architecture/references/languages/testing-workflows.md`.

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:server
mise run test:bifrost:journey:sdk
mise run test:e2e
mise run codegen:check
mise run check:proto-drift
mise run check:client-tier
mise run check:bifrost-resource-governance
git diff --check
```

## Completion evidence

- Source-generated request/terminal diff proving no selector or EXPLAIN.
- Selection mutation matrix and path admission/floor snapshots.
- UI, data-scientist, scheduled, failure, cancellation, and pressure journeys.
- Single audit acceptance/no stage audit evidence.
- Production telemetry deltas and zero owner/readiness/shutdown snapshots.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if activation requires a path hint, EXPLAIN,
automatic retry, exact routing-facts subsystem, broader operator promise,
durable query job, new listener, or changed tenant/audit semantics.
