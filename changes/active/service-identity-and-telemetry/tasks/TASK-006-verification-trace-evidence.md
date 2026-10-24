---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-service-identity-and-telemetry
spec_revision: 16
requirements: [REQ-014, REQ-015, REQ-040, REQ-043, REQ-045, REQ-046, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, AC-014, AC-017, AC-019, AC-020]
depends_on: [TASK-003]
---

## Outcome and Value

Verification judges the authorized trace visible to one attempt, including
other workloads' and untagged spans. Queued checks submitted before evidence
lands use bounded retries; direct checks still evaluate once. For example,
checkout verification can inspect fraud-service spans in the same trace while
its result remains owned by checkout's exact binding.

## What Changes and What Must Stay True

Extend the existing reader, engine and queue: remove implicit trace time bounds,
share each fetched trace within one attempt, and retry queued not-yet-landed
evidence. Preserve tenant/table authorization, binding/result attribution,
source-span identities, explicit projection, deterministic order, admission
limits and explicit 10,000-span overflow refusal. Retained history may cost more
to scan. Accepted execution identity, authority, inputs and idempotency survive
backoff/restart; only the final completed result is persisted. PostgreSQL owns
retry due times, and waiting releases the worker.

TASK-002 owns frozen authority and scheduled-account changes; TASK-003 supplies
catalog/schema compatibility; TASK-007 owns broader profiling/optimization.
No workload-only mode, new cache/service, absolute accepted-execution expiry,
completion protocol, historical replay guarantee, mutable completed result or
retry-until-pass behavior is added. SDKs keep server-owned semantics.

## Approach

1. Revalidate the reader → hosted engine → runner → queue path after TASK-003.
2. Read retained exact-ID spans without time/attribution narrowing; share one
   collection per trace per attempt, including concurrent checks.
3. Carry not-yet-landed evidence through existing retry/settlement machinery;
   on exhaustion complete requirement judgments through the normal decision rule.
4. Prove direct, queued, restart, failure and late-arrival behavior through owner
   tests and focused public-surface journeys.
5. Update the owning Verifier/query architecture with behavior; TASK-008 remains
   the final separate docsite task.

## Ordered Implementation Scenarios

### Scenario 1 — Read the complete visible authorized trace

**Behavior.** Authored selectors inspect all eligible spans, while the result
keeps its exact owner/subject and source spans keep their identities.

| Case | Expected result |
| --- | --- |
| Same trace across workloads, Cards and untagged spans | All visible authorized spans included. |
| Retained spans outside the former seven-day-before/one-day-after bounds | Included without an acceptance-relative predicate. |
| Other trace/tenant, or denied table | No unrelated evidence; ordinary refusal remains enforced. |
| 10,000 / 10,001 spans | Full ordered collection / explicit failure, never truncation. |

**RED.** Add planned `visible_trace_spans_preserve_binding_ownership` ([J1](#journey-commands)); the current reader omits out-of-window spans. Assert the case table through real writes, verification and reads.

**GREEN.** Extend `CallerTraces` using TASK-003's projection, preserving ordinary query authorization, ordering and overflow detection.

**REFACTOR.** Remove unused anchor/lookback plumbing; reuse the current transport and span decoder. Keep J1 green.

### Scenario 2 — Share one attempt's collection and preserve completed results

**Behavior.** Concurrent/sequential checks and different slots naming the same
trace fetch once; different traces stay distinct, and retries/new executions
fetch afresh. Nonempty evidence is evaluated immediately; late spans cannot
revise or rerun a completed result, including a failed judgment.

**RED.** Add planned `trace_checks_share_one_attempt_read` ([U1](#owner-test-commands)) with fetch-count and consistent-selector assertions, and `late_trace_spans_require_new_verification` ([J2](#journey-commands)) showing only a new execution judges late spans. Current per-check fetches violate U1; J2 may be verification-only if already correct.

**GREEN.** Extend the current attempt lifecycle to share fetched collections and retain durable completed results; rerun U1, J1 and J2.

**REFACTOR.** Keep reuse scoped to the existing engine attempt; add no cross-execution cache or snapshot protocol.

### Scenario 3 — Retry absent queued evidence under the same execution

**Behavior.** Zero rows or a missing span table defer the same accepted execution
without publishing an intermediate result. Preserve three total attempts,
30/60-second backoff and configured per-attempt timeout, five minutes by default;
restart preserves authority, inputs, owner/subject and idempotency, and a later
attempt rereads and evaluates arriving evidence without an absolute lifetime expiry.

**RED.** Add planned `missing_trace_retries_same_execution` ([I1](#owner-test-commands)), covering both absence cases, later arrival and restart using the real engine. Assert persisted attempt/due times and unchanged accepted data; current behavior completes on the first absence.

**GREEN.** Route only not-yet-landed evidence into existing queue retry, releasing workers and using PostgreSQL due times. Rerun I1 and earlier scenarios.

**REFACTOR.** Reuse claim/retry/settlement and existing bounded test controls; do not resubmit, mint tokens or refresh authority.

### Scenario 4 — Exhaustion completes requirement judgments normally

**Behavior.** The third unavailable attempt produces one completed result with
an explicit inconclusive trace requirement. Trace-only is inconclusive; mixed
requirements keep normal `all`/`pass_rate` semantics, including definite failure
or threshold pass, without an execution-wide inconclusive override.

**RED.** Add planned `missing_trace_exhaustion_keeps_decision_rule` ([I2](#owner-test-commands)), using a compact trace-only/mixed-failure/threshold-pass table. Assert three attempts, 30/60-second delays and one final result; current trace handling finishes immediately and generic retry exhaustion errors.

**GREEN.** Complete exhausted trace availability through existing aggregation and lease-fenced publication, preserving generic retry exhaustion. Rerun I2 and prior scenarios.

**REFACTOR.** Retain one judgment/decision implementation and one durable completion path.

### Scenario 5 — Preserve direct execution and failure classification

**Behavior.** Direct absence yields one explicit inconclusive trace requirement,
no queue submission, and the configured lifetime (60 seconds by default).
Failed judgments, invalid evidence, denied reads, oversized traces and engine
timeouts retain their existing handling and never enter missing-evidence retry;
queued default/overridden per-attempt limits remain intact.

**RED.** Add planned `trace_unavailability_preserves_execution_mode` ([I3](#owner-test-commands)) with a direct-absence/queued-negative case table and assertions for no hidden enqueue, exact typed errors where applicable, defaults and timeout classification. Report already-correct cases as regression proof.

**GREEN.** Preserve mode and typed failure distinctions, routing only queued not-yet-landed evidence to availability retry; rerun I3 and prior scenarios.

**REFACTOR.** Reuse existing limits/errors; add no timeout setting, settling delay or inferred completeness flag.

## Acceptance Criteria

| Obligations | Observation and violating result |
| --- | --- |
| REQ-014–015, 040; INV-001–007; AC-014, 019–020 | I1/I2 preserve execution, accepted authority, inputs and binding attribution across retry/restart; fallback authority, changed ownership or intermediate results violate the contract. |
| REQ-043, 045; INV-001–008; AC-017, 019 | J1/U1 include all retained authorized exact-ID spans once per attempt; hidden narrowing, cross-tenant evidence or truncation violate it. |
| REQ-045; AC-019 | J2 judges nonempty evidence immediately and preserves old results after late arrival; only a new verification sees new spans. |
| REQ-046; AC-020 | I1/I2 show eventual success or final requirement-level inconclusive after three attempts/30–60-second backoff; lifetime expiry or erased mixed judgments violate it. |
| REQ-043, 045–046; AC-017, 020 | I3 preserves direct semantics, query refusal, ceiling and timeout handling; broad retry classification or hidden enqueue violates it. |
| REQ-040, 045–046; INV-006; AC-014, 019–020 | P1/T1/M1 project whole-trace, direct-absence and queued-arrival behavior through each shipped surface without client-owned retry. TASK-007 adds broader performance evidence. |

## Implementation Reference

No `.codegraph/` exists. Inspected owners and consumers support extending the
whole current workflow; these paths guide ownership, not private structure.

| Inspected owner/call path/tests | Gap and reuse |
| --- | --- |
| `wyrd-server/src/verification/hosted/traces.rs::CallerTraces::{statement,fetch}` ← `hosted/blocks.rs::HostedBlocks::engine` | Reader already queries as caller, decodes `SPAN_COLUMNS`, orders and requests 10,001 rows. Remove time bounds, retain failure limits and consume TASK-003 schema; no replacement transport/store. |
| `vala-verify/src/blocks/engine.rs::VerificationEngine::{run,trace}` → `trace_source.rs::{TraceSource,project_spans}`; `blocks/engine/tests.rs` | Each check fetches and catches all trace errors as inconclusive. Extend attempt sharing and distinguish missing evidence for queued policy, preserving selectors/decision rules. |
| `wyrd-server/src/verification/runner.rs::VerifierRunner::{execute,dispatch,settle}`; `verification/mod.rs::RuntimeLimits`, `hosted/mod.rs::HostedLimits` | Runner restores accepted caller and sets each attempt's deadline; `EvidenceUnavailable` already retries. Extend absence/final-budget seam without changing direct or unrelated errors. |
| `wyrd-sql/src/queries/verifier_executions.rs::VerifierExecutions::{claim,retry,complete}`, `RetryPolicy::{default,delay_after}` | Queue owns frozen inputs, leases and database due times; generic exhaustion errors. Reuse it with trace-specific final judgment, consuming TASK-002 authority contract. |
| `wyrd-server/tests/integration/pg_verification_runtime.rs`; `wyrd-testing/tests/bifrost/server/verification_runtime.rs` | Existing runtime controls and real delegated trace/restart journeys support proof. Availability tests must reach real engine/read logic, not script their expected outcome. |
| Python `tests/integration/test_verifier_building_blocks.py`; TS `wyrd/tests/integration/verifier-building-blocks.test.ts`; MCP `tests/bifrost/mcp/verification.rs::pg_tests` | Existing public trace authoring/OTel and server fixtures, `serverTest`, and MCP execute/enqueue/result paths cover shipped callers. Extend their fixtures with minimal mirrored trace cases; add no client retry. |

Update Verifier behavior in `architecture/wyrd-design.md`, evidence/query prose
in `architecture/bifrost-design.md`, and contradictory owner rustdoc. TASK-003
owns schema production; TASK-008 owns docsite/examples. Revalidate constructors
and callers after removing unused acceptance-anchor plumbing.

## Verification and Evidence

New names below are **planned**; confirm their actual exact selectors after
adding tests, before RED. Packages, targets, module roots, runner, features and
setup were checked in manifests, source and `mise.toml`; no tests ran during
planning. Run Cargo sequentially and record selected count, expected RED,
GREEN assertions and final evidence. These are focused commands, not full suites.

Before any server-backed command below, run `mise run build:server` once after
production changes. `WyrdTestServer` selects the same-profile `wyrd-server` worker
(`wyrd-testing/src/server.rs`), so a stale binary is not valid proof.

### Owner test commands

U1 proves attempt read consistency; I1 retry/recovery; I2 final decision; I3 direct/failure classification. Integration uses the existing `test-support` module and all-schema Postgres setup.

```bash
# U1
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::trace_checks_share_one_attempt_read)'
# I1
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::missing_trace_retries_same_execution)"'
# I2
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::missing_trace_exhaustion_keeps_decision_rule)"'
# I3
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::trace_unavailability_preserves_execution_mode)"'
```

### Journey commands

J1/J2 prove whole-trace attribution and late arrivals through the Rust SDK and
real server. P1/T1/M1 each add one mirrored public journey with three cases:
whole visible cross-workload trace, direct absence, and queued absence followed
by landed evidence under the same returned execution. Extend existing checked-in
fixtures and harness controls; internal timing/fetch-count mathematics stay in
Rust owner tests. These are TDD projections of scenarios 1/3/5: observe the same
missing behavior at RED, extend server owners at GREEN and simplify existing
surface fixtures at REFACTOR while all remain green.

```bash
# J1
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::visible_trace_spans_preserve_binding_ownership)"'
# J2
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::late_trace_spans_require_new_verification)"'
# Python setup, then planned P1
mise run py:setup
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_trace_evidence_keeps_whole_trace_and_waits_when_submitted'
# TypeScript setup, then planned T1
mise run ts:build
mise run ts:build:testing
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t "^trace evidence keeps whole trace and waits when submitted$"'
# Planned M1; existing MCP target/profile owns real-server lifecycle
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::trace_evidence_keeps_whole_trace_and_waits_when_submitted)"'
```

### Existing regressions and owner checks

Retain generic retry exhaustion as errored and delegated trace-read authority/
audit assertions across bounded retries. Owner module checks cover aggregation;
SQL arithmetic runs if its owner changes. Prose uses static proof, not artificial RED.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::retryable_engine_failures_exhaust_to_errored)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::delegated_input_reads_keep_history)"'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(/^blocks::engine::tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::verifier_executions::tests::retry_delays_double_and_cap)'
mise run fmt
mise run lints
mise run check:tenant-isolation
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
```

## Material Stop Conditions

Stop for spec authority if correctness requires hidden narrowing, new row access
policy, refreshed/fallback authority, absolute expiry, completeness claims,
mutable completed results or changed retry/timeout defaults. Coordinate slow
retained-history reads with TASK-007 without reducing evidence. Resolve projection
incompatibility at TASK-003 rather than introducing a duplicate reader; routine
private fixture/lifecycle/error plumbing remains implementation-owned.

## Authority Links

- [Approved revision 16](../spec.md), [whole-trace/availability contract](../supplements/table-storage-and-correlation.md#whole-trace-verification-decision), [auth](../supplements/auth-system.md), [signal schemas](../supplements/telemetry-schemas.md).
- [Verifier](../../../../architecture/wyrd-design.md#verifier), [Bifrost](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [SQL rules](../../../../crates/wyrd/wyrd-sql/AGENTS.md).
- [Spec workflow](../../../spec-driven-development.md), [execution](../../../implementation-execution.md), [testing](../../../../TESTING.md), [Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md), [TypeScript rules](../../../../sdks/wyrd-sdk-ts/AGENTS.md).

## Implementation Evidence

Revalidated owners: `CallerTraces` already queried as the accepted caller
with `SPAN_COLUMNS`, ordering, and the 10,001-row probe; the gap was the
acceptance-anchored time predicate. `VerificationEngine` fetched per check and
caught every trace error as inconclusive; the gap was per-attempt sharing and
a deferral seam. The runner already mapped `EngineError::EvidenceUnavailable`
to `EngineOutcome::Retry` (`evidence_unavailable`) through the existing
three-attempt, 30/60-second PostgreSQL-due retry; the gap was choosing
deferral only for a queued attempt before its last. No new error, setting,
cache, table, or retry path was added.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-043/045, AC-017/019: all retained authorized exact-ID spans, cross-workload and untagged, outside the former window; other trace/tenant excluded; denied query refused; 10,000 judged / 10,001 explicit | `verification/hosted/traces.rs::CallerTraces::statement` (no time bound; anchor/lookback/chrono removed) | J1 `verification_runtime::visible_trace_spans_preserve_binding_ownership` | PASS |
| One read per trace per attempt, shared by concurrent checks; deferral only on a deferring engine | `vala-verify/src/blocks/engine.rs::{run, read, trace}`, `with_unlanded_traces_deferred` | U1 `blocks::engine::tests::trace_checks_share_one_attempt_read`, `unlanded_traces_defer_only_a_deferring_engine`; all 24 `blocks::engine::tests::` | PASS |
| REQ-014/015/040, AC-014/020: missing table and zero rows retry the same execution across restart; 30/60 s; no intermediate result; accepted authority, inputs, attribution, idempotency unchanged; third attempt judges arrived spans (one 30 days old) | `verification/runner.rs` (`defer_unlanded_traces: run.attempt < run.max_attempts`), `hosted/blocks.rs::Execution` | I1 `pg_verification_runtime::missing_trace_retries_same_execution` | PASS |
| REQ-046, AC-020: exhaustion completes under `decide` (trace-only inconclusive, mixed `all` failed, `pass_rate` passed) | existing aggregation; last attempt not deferred | I2 `pg_verification_runtime::missing_trace_exhaustion_keeps_decision_rule` | PASS |
| REQ-043/045/046, AC-017/020: direct absence inconclusive with nothing enqueued; failed judgment, denied read, oversized trace, invalid evidence settle on attempt 1; 5 min / 60 s defaults | `components/verification/service.rs` (`defer_unlanded_traces: false`) | I3 `pg_verification_runtime::trace_unavailability_preserves_execution_mode` | PASS |
| REQ-045, AC-019: nonempty evidence judged on attempt 1; late spans never revise a completed result; a new execution sees them | unchanged completion path | J2 `verification_runtime::late_trace_spans_require_new_verification` | PASS |
| REQ-040/045/046, INV-006, AC-014/019/020: whole trace, direct absence, queued absence then landed evidence under the same execution, no client retry, on every surface | `fixtures/cards/verifier_building_blocks/whole-trace.yaml`; shared `wyrd_testing::bifrost::canonical_signals::trace_spans` | P1 `test_trace_evidence_keeps_whole_trace_and_waits_when_submitted`; T1 `trace evidence keeps whole trace and waits when submitted`; M1 `verification::pg_tests::trace_evidence_keeps_whole_trace_and_waits_when_submitted` | PASS |
| Delegated trace reads keep accepted history across bounded retries | journey updated to make each retry due and require three attempts and a decision per attempt | `verification_runtime::delegated_input_reads_keep_history` | PASS |
| Generic retry exhaustion and SQL backoff unchanged | no change | `pg_verification_runtime::retryable_engine_failures_exhaust_to_errored`; `queries::verifier_executions::tests::retry_delays_double_and_cap`; `pg_hosted_artifacts::` (12) | PASS |
| Architecture and rustdoc agree | `architecture/wyrd-design.md` (Verifier checks), `architecture/bifrost-design.md` (query service); rustdoc on changed items | static review; `rg` for lookback/anchor/window prose | PASS |
| Format, lints, boundaries | — | `mise run fmt`, `lints`, `check:tenant-isolation`, `py:format`, `py:lints`, `py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck`, `ts:napi:check`, `git diff --check` | PASS |

Limits: RED was not executed separately for U1, J1, or the I-tests; the
production change was written before the tests, so their failure against the
prior reader/engine is argued from source (the 30-day span and the per-check
fetch), not observed. P1 and T1 wait the real 30-second PostgreSQL backoff
(about 35 s each) because the Python and TypeScript harnesses expose no
retry-due control, and adding one was not needed for the proof.

Non-goals stayed excluded: no workload-only mode, cache/service, absolute
expiry, completion protocol, mutable completed result, client-owned retry,
or retry-until-pass behavior.

### Diagnoses

- **Symptom:** I2 panicked in `sent_result_id` ("one batch names one
  result"). **Evidence:** three runs' results arrived in fewer submitted
  batches. **Cause:** the Scribe outbox group-commits results, so one batch
  can carry several executions' rows. **Fix site:** the test's `verdicts`
  helper reads `(result_id, verdict)` per row; the write-count assertion only
  requires every write to be a result.
- **Symptom:** I1's precondition "no span table yet" failed.
  **Evidence:** `WyrdTestServer::retained_records` rustdoc: "A table the
  tenant has never published reads as no rows." **Cause:** the inspection
  helper cannot distinguish a missing table. **Fix site:** dropped the
  precondition; the harness's `start_with(false)` creates no built-in
  tables, so the first attempt still exercises the missing-table path.
- **Symptom:** I3's "denied read" judged `failed`, not inconclusive.
  **Evidence:** `query/service.rs::QueryAuthority::admit_capability` admits on
  `BifrostQuery/Read` alone. **Cause:** the case granted `bifrost_query_read`,
  which is sufficient, so the read succeeded. **Fix site:** the case submits
  with no permissions; `trace_reader` grants only `bifrost_query_read`.
- **Symptom:** J1's denial assertion found `retrieve` in the response.
  **Evidence:** the response echoes the authored `check.expected`.
  **Cause:** the assertion matched authored text, not leaked evidence.
  **Fix site:** assert each reason names the `BifrostQuery/Read` refusal.
