---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-long-horizon-verification
spec_revision: 13
requirements: [REQ-006, AC-005, AC-006, INV-001, INV-002, INV-003, INV-004, INV-005]
depends_on: []
---

# Bound outbox staging without blocking audit

## Outcome and Value

Producer overload is refused with typed retryable pressure before source
staging, process memory/work stays finite, and other runnable tenants progress.
Own REQ-006 staging/admission/progress, AC-005 pressure/progress and AC-006
outbox admission/retry/drain/loss portions. TASK-001 owns discovery/snapshot
bounds and the group portions of those ACs; neither task depends on the other.

## Owners, Scope, Consumers, and Prohibited Changes

Execute with `$wyrd-implement`. All INV-001–005 apply. Use ordinary current
verify/submit for standalone pressure/progress journeys; group_id is not a
prerequisite. After integration with TASK-001 rerun combined J-005 pressure
against actual grouped calls; this task owns that seam's overload assertions.

Keep one ScribeOutbox write path, non-blocking audit, existing retry identity,
per-tenant batching and shutdown/crash-loss semantics. Count/log unsuccessful
audit staging without failing or delaying the authorization decision. Source
invocation overload is a typed refusal; it cannot silently stage half of a
refused call or discard successful source staging merely to satisfy a bound.
Include queued, waiting, retrying and in-flight outbox work in finite process
accounting, not just the channel. No distributed quota ledger, strict fairness,
new durable outbox, platform-specific limiter or producer Postgres access.

## Source-backed Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Process staging | wyrd-runtime::outbox::{Outbox::new,stage}, writer dispatch/retry/shutdown | outbox::tests, including its unbounded-queue expectation; server ScribeOutbox | Unbounded channel/backlogs and no producer admission result | Finite staging in existing owner with whole pending-work accounting | No second buffer, writer, broker or durable receipt |
| Source refusal | VerificationControl::{execute,enqueue,stage_direct}; ScribeWrite::ExecutionInputs/Result | pg_verification_routes; verify_in_real_time SDK journeys | Producer success cannot distinguish exhausted staging | Existing typed error family and pre-staging producer admission | No new queue/lifecycle/SQL operation |
| Non-blocking audit | AuditStage and ScribeOutbox audit staging | existing audit::authorize path, ScribeWrite audit branch | Bounds must not turn audit failure into permission failure | Count/log failed audit staging without failing original operation | Preserve existing policy, not a durable audit relay |
| Tenant progress/recovery | ClaimLoop::claim_round; Outbox writer's tenant dispatch | outbox::tests::a_failing_tenant_does_not_delay_another_tenant; SDK server journeys | Need finite pressure proof and progress under one tenant's burst | Reuse current resource admission and tenant rounds | No new distributed share/quota scheduler |

## Approach

1. Add focused bound/retry/drain and non-blocking audit tests under existing owners.
2. Extend finite accounting/admission at the single outbox boundary and source call sites.
3. Preserve identical retry slices, tenant progress and shutdown/loss counters.
4. Prove refusal/recovery in ordinary SDK/HTTP/MCP journeys, then the integrated grouped J-005 variant.

## Ordered Implementation Scenarios

### Scenario 1 — Finite staging includes queued and writing work

**Behavior.** REQ-006/AC-006: stalled/failed sinks cannot grow staged bytes/items
without bound; overflow is detected before source staging, and capacity is
released on existing terminal consumption/shutdown, not on dequeue alone.
Retries reuse identical slices and do not double-charge pending work.
The bound covers the Scribe-ACK → SubmissionOutbox handoff
(`verification/submissions.rs:29–54`, `scribe_outbox.rs:921–930`), not only the
first channel. The active-work bound is enforced where the runner claims work
(`verification/runner.rs:227`, `claims.rs:140–165`); hosted step concurrency
alone does not bound execution tasks.

**RED.** Add planned outbox::tests::bounded_staging_covers_retrying_and_inflight_work
(exact command below). Fill the bound while work is waiting/in-flight/retrying;
assert overflow plus later recovery. Existing unbounded staging fails the bound.
Update the existing unbounded-queue expectation to the approved bounded behavior;
this is an intentional contract change, not disabling a failing test.

**GREEN.** Implement finite staging through the existing generic outbox/Scribe
owner and production producer admission. Keep retained successful work accounted.

**REFACTOR.** Reuse existing release/retry/shutdown ownership rather than a parallel
capacity service. Rerun retry/tenant/drain regressions after accounting changes.

### Scenario 2 — Refuse excess source calls without making audit blocking

**Behavior.** AC-005/006: ordinary verify/submit beyond process capacity gets
stable typed retryable pressure and retry timing before any source row is
staged. Successful source calls retain existing outbox durability. An unavailable
or full audit path is logged/counted and never changes authorization success
or failure, nor waits for persistence. Direct two-row staging cannot be partial
for a call refused by admission.

**RED.** Add planned outbox::tests::bounded_audit_staging_remains_nonblocking and
verification_staging_refuses_and_recovers in each SDK owner; include direct
HTTP and planned verification::pg_tests::staging_pressure_is_retryable through
the existing MCP journey target. Current stage cannot report
producer pressure and the queue has no finite ceiling.

**GREEN.** Close the error/admission path in Rust owners and client projections;
keep audit and authorization ordering exactly as the approved invariants require.

**REFACTOR.** Share existing typed refusal/error construction; no language-owned
admission policy or alternate audit queue. Preserve Scenario 1.

### Scenario 3 — A tenant burst drains and another tenant progresses

**Behavior.** AC-005/006: accepted B work completes while A continues offering
more calls. Local staging/work stays bounded; stop load and drain pending work.
Retry a refused enqueue after drain without duplicate execution. Sink retry,
shutdown and abrupt-crash loss boundaries remain truthful. Scale-out preserves
each process bound without asserting a deployment-wide quota.

**RED.** Extend the planned verification_staging_refuses_and_recovers journeys
with both tenants, replay, drain and API/worker recovery. Use normal controlled
request traffic, not host stress or a mandatory capacity benchmark. Compare
source rows to accepted/refused responses; current unbounded behavior cannot
supply the refusal/progress proof.

**GREEN.** Reuse existing batching and claim rounds with finite process admission.
Once TASK-001 is integrated, repeat this same journey with group_id to close
J-005; record that combined result separately from standalone proof.

**REFACTOR.** Remove duplicate process counters/temporary buffering and rerun
all scenario tests, including audit and shutdown negatives.

## Acceptance Criteria

- Whole pending work has a finite process bound, with retry/drain accounting and no growing hidden writer backlog.
- Source overload returns typed retryable errors before staging, including SDK/HTTP/MCP projections; successful calls retain existing durability, not stronger receipts.
- Audit remains non-blocking/countable; no audit-write error leaks into invocation results.
- B progresses during A's burst; accepted work drains and replay/recovery obey existing semantics. No strict share, replica-invariant quota or loss-free crash claim.
- Standalone ordinary-call proof passes independently; integrated grouped J-005 evidence is recorded when TASK-001 is available, without changing task dependencies.

## Expected Write Set and Consumer Closure

Likely owners: crates/shared/wyrd-runtime outbox/audit boundary; server
scribe_outbox, verification control, resource admission/config/telemetry and
existing auth/audit call sites; shared contract error catalog/client projection
only where necessary. SDK runtime tests exercise the actual language boundaries;
no durable behavior moves into clients. Audit/gateway consumers of the same
outbox need regression closure when affected. Paths are guidance, not a private
API or type allowlist. Coordinate TASK-001's producer metadata edits.

## Verification and Evidence

Planned unit selectors in the existing outbox::tests module:

```bash
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::bounded_staging_covers_retrying_and_inflight_work)'
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::bounded_audit_staging_remains_nonblocking)'
```

Existing, source-confirmed retry/progress/drain regressions:

```bash
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::a_retry_writes_the_identical_failed_slice_without_later_items) | test(=outbox::tests::a_failing_tenant_does_not_delay_another_tenant) | test(=outbox::tests::shutdown_flushes_items_that_recover_before_the_deadline) | test(=outbox::tests::shutdown_counts_items_unwritten_at_the_deadline_as_lost)'
```

Planned SDK journey selectors in existing verify-in-real-time owners; add the
HTTP and MCP refusal/discover-act-observe assertions to their actual server flow,
not an in-process replacement engine. Build native dependencies once using
mise run py:setup, ts:build, ts:build:testing before the owning runtime tests.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::verification_staging_refuses_and_recovers)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_verification_staging_refuses_and_recovers'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^verification_staging_refuses_and_recovers$"'
```

Planned MCP pressure journey in the existing verification::pg_tests owner:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::staging_pressure_is_retryable)"'
```

Confirm planned names in nextest list/pytest collection/Vitest output after
tests are added, with nonzero intended selection. Record exact commands for
any additional named audit/integration test in task evidence. No full suites.

```bash
mise run fmt
mise run lints
mise run check:deps
```

If public errors/generated contracts change run mise run codegen:check; SQL
changes require check:tenant-isolation. Changed SDK sources require their
format/lint/type tasks (py:format, py:lints, py:typecheck; ts:format, ts:lints,
ts:typecheck and ts:napi:check for native declarations). Static checks are
non-TDD proof; behavioral admission/recovery is never static-only evidence.
TASK-004 separately measures memory/throughput; no benchmark is required to
prove this task's deterministic finite-bound outcome.

Append the `$wyrd-implement` acceptance evidence table with source locations,
exact RED/GREEN/REFACTOR commands/results, selected counts/features, actual
write-set/non-goal audit and unresolved integrated evidence. Diagnose unexpected
failures with tracing per that skill. Implementation does not approve or merge.

## Material Stop Conditions

Do not change the approved spec or add aliases, another owner/queue/transport,
producer Postgres dependencies or unrelated policy. Resolve private mechanics
through `$wyrd-implement`; route a required material change through `$wyrd-spec`.
Do not add tasks to the owner-approved four-task split.

## Authority Links


- [Approved specification, revision 13](../spec.md).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md).
- [Wyrd design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx), [Bifrost design](../../../../architecture/bifrost-design.md).
- [Spec workflow](../../../../architecture/references/languages/spec-driven-development.md), [implementation execution](../../../../architecture/references/languages/implementation-execution.md), [testing workflows](../../../../architecture/references/languages/testing-workflows.md).
- Execute with [$wyrd-implement](../../../../.agents/skills/wyrd-implement/SKILL.md); implementation returns evidence, not independent task approval.
