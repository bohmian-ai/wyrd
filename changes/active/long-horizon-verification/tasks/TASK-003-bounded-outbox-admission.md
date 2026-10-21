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
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../AGENTS.md).
- [Wyrd design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md), [Bifrost design](../../../../architecture/bifrost-design.md).
- [Spec workflow](../../../../changes/spec-driven-development.md), [implementation execution](../../../../changes/implementation-execution.md), [testing workflows](../../../../TESTING.md).
- Execute with [$wyrd-implement](../../../../.agents/skills/wyrd-implement/SKILL.md); implementation returns evidence, not independent task approval.

## Implementation Evidence

Result: `IMPLEMENTED` (commits 2348d34d8, f9cabf895, 2bf02899d, f3c5244fd,
c0e4da4de, a7578202d, 11c061743, c7471e119, 09ce35d62). Task review pending.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Finite process bound over queued, waiting, retrying and in-flight work; retry and drain accounting; no hidden writer backlog | `wyrd-runtime/src/outbox.rs` `Admission` (reserved and unreserved shares, released on write, loss or shutdown, not on dequeue); `Reservation` RAII slot; config `scribe.outbox_capacity` (`config.rs`, default 65 536, ≥ 1); runner cap `RuntimeLimits::execution_slots` enforced in `ClaimLoop` (`claims.rs`) | `outbox::tests::bounded_staging_covers_retrying_and_inflight_work`, `bounded_audit_staging_remains_nonblocking`; `pg_verification_runtime::the_runner_claims_no_more_than_its_execution_slots` | PASS |
| Typed retryable refusal before staging; SDK, HTTP and MCP projections; no stronger receipts | `WYRD_VERIFICATION_429_STAGING_FULL` (`wyrd-spec/src/error.rs`), `details.retry_after_seconds=1` and `Retry-After: 1` (`http/error.rs`); direct verify and submit reserve before staging (`components/verification/service.rs`); submission slot carried through the Scribe-ACK handoff (`submissions.rs`, `scribe_outbox.rs`); runner reserves its result slot before the engine runs and defers without an attempt charge when full (`runner.rs`) | Rust, Python and TS `verification_staging_refuses_and_recovers`; MCP `verification::pg_tests::staging_pressure_is_retryable` (EXECUTE and ENQUEUE refused, no `execution_inputs` row); `http::error` Retry-After test; codegen:check | PASS |
| Audit non-blocking and counted; no audit error in invocation results | Unreserved `stage` never blocks or fails; past its share an item is counted lost and logged (`outbox.rs`); span cap protects that share (`scribe_outbox.rs` `TENANT_SPAN_PERMITS`) | `bounded_audit_staging_remains_nonblocking`; journeys log `outbox is full; the staged item was lost` for setup audits while every call succeeds | PASS |
| Tenant B progresses during A's burst; accepted work drains; replay and recovery unchanged | Reserved share per call; per-tenant writer dispatch unchanged | All three SDK journeys: the neighbor's queued answer-check passes while the burst's judgments are held; recovery after flush; exactly 3 `execution_inputs` rows; `a_failing_tenant_does_not_delay_another_tenant`, `a_retry_writes_the_identical_failed_slice_without_later_items`, both shutdown tests | PASS |
| Standalone ordinary-call proof; integrated grouped J-005 recorded when TASK-001 is available | Standalone journeys need no TASK-001 code; `depends_on: []` unchanged | Standalone: PASS. Grouped J-005 rerun with `group_id`: **open**, to run on the integration branch once TASK-001 lands | PASS (standalone) / OPEN (integrated) |

### Commands

- Six exact `wyrd-runtime` outbox selectors (the two new tests and four regressions): 6 passed.
- Rust journey (`-p wyrd-ai --test integration -P journey`, `=verify_in_real_time::verification_staging_refuses_and_recovers`): passed.
- Python `test_verify_in_real_time.py::test_verification_staging_refuses_and_recovers`: passed. The whole file: 20 passed (shared fixtures changed).
- TS `-t "^verification_staging_refuses_and_recovers$"`: passed. The whole file: 20 passed. `verifier-building-blocks` and `scheduled-drift-alerts-operator` (affected by the provider scope): 26 passed.
- MCP `=verification::pg_tests::staging_pressure_is_retryable`: passed.
- wyrd-server `--test integration -E 'test(/^(pg_verification_runtime|pg_verification_routes)::/)'`: 51 passed. Lib `verification::runner`, `scribe_outbox`, `components::verification`, `http::error`: 20 passed.
- `mise run fmt`, `lints`, `check:deps`, `codegen:check`, `docs:check`, `py:lints`, `py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck`, `ts:napi:check`, `git diff --check`: all passed.

### Diagnoses

1. **Rust journey: judge never received its calls** (`admitted=4 limit=4`). Evidence: the trace showed the reserve check counting unreserved audit and span items, and the request's own audit was staged before `reserve`. Cause: one shared total for reservable and unreserved work. Fix site: `outbox.rs` `Admission` split into reserved (3/4) and unreserved shares, plus the runner result reservation. A diagnostician confirmed the cause.
2. **Python journey: baseline not ready** for the neighbor tenant. Evidence: `fitter: drift baseline fitted tenant=<neighbor>`, but `WyrdTestServer::wait_for_baseline` (`wyrd-testing/src/server.rs:3885`) reads only the fixture tenant. Fix site: `conftest.py` `registered_assistant` waits only when given `fitted_by`. Then "Card scope does not cover the verified subject": the neighbor needed its own Service key (`support.py` `service_key(..., client)`), as the Rust `in_tenant` does. TS had the same key cause.
3. **TS staging test fails only in its full file.** Evidence: no judge arrivals; executions admitted. Cause, independently confirmed: Vitest 4 caches file-scoped fixtures by name, so the staging test reused the file's first `provider`. Fix site: `tests/support/server.ts` scopes `provider` like `server`. Other `perTest` files rerun: 26 passed. (Those five failures were first caused by an unbuilt `target/release/wyrd-monty-worker`; `cargo build --release -p wyrd-monty-worker` fixed that. The TS integration lane does not build it.)
4. **Clippy `large_futures`** across the Rust journey `Assistant::start` callers. Cause: the new builder field pushed `start_on`'s future (it holds the builder across the judge mock await) past the limit. Fix site: `Box::pin` at the two `start_on` callers.
5. **`every_tenant_is_claimed_in_the_first_round` (`claimed=64`) and `two_hundred_released_runs_complete_on_their_first_attempt` (timeout).** Cause, independently confirmed: the task-required `execution_slots` cap (64) makes more than 64 held runs impossible. The tests' expectations were out of date, not production code. Fix site: the tests give the runner a slot per execution (70 and 200), keeping their unlimited-tenant-round and pool proofs; new `the_runner_claims_no_more_than_its_execution_slots` proves the cap.

### Non-goal and write-set audit

No distributed quota, strict fairness, new durable outbox, queue, transport, alias, platform-specific limiter, or producer Postgres access. One ScribeOutbox write path. The spec and task split are unchanged. All 31 files changed since ef3a57b07 are in the expected owners (runtime outbox, server verification, config and error, spec error catalog, docs and design, test harness knobs, SDK journeys and support). No untracked files.

### Risks and limits

- At tiny test capacities, setup audits, spans and captures are counted lost (by design, never blocking).
- The unreserved share is a strict 1/4 of capacity, and reserved work cannot borrow from it.
- The runner defers a completed engine run only before it starts, never after.
- Production defaults: capacity 65 536 items and 64 execution slots. TASK-004 measures memory and throughput.
