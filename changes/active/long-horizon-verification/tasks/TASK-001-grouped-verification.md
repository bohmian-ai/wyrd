---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-long-horizon-verification
spec_revision: 13
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-008, REQ-009, REQ-010, REQ-011, REQ-013, REQ-014, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-008, INV-001, INV-002, INV-003, INV-004, INV-005]
depends_on: []
---

# Grouped verification, end to end

## Outcome and Value

Ordinary verify/submit executions sharing group_id feed exact, isolated source
references; group_end schedules serial, immutable whole-group judgments and
verdict-change Operators. Ship J-001–004 and the grouping portion of J-005
through Rust/Python/TypeScript and agent HTTP/MCP, including failures and recovery.

## Owners, Scope, Consumers, and Prohibited Changes

Own REQ-001–005, 008–011, 013–014; REQ-006 discovery/snapshot bounds;
AC-001–004/008, AC-005 group correctness/Drift coexistence, and AC-006 source
resolution, isolation, snapshots, recovery and compatibility. TASK-003 owns
process staging/pressure/progress and AC-006 outbox admission; TASK-002 owns
scheduled samples. This task is verifiable at normal load without either.
The full J-005 combines this task and TASK-003 after integration; do not claim
its pressure proof from group-only tests. No dependency is added for that seam.

Use `$wyrd-implement`. All INV-001–005 apply. Paths are owner/consumer guidance,
not a private implementation allowlist. Keep server durability in Rust, SDKs
thin, tenant authorization/RLS intact, and audit non-blocking on ScribeOutbox.
Do not edit the approved spec, introduce compatibility aliases, or restore any
removed lifecycle, checkpoint, internal fan-out, media-processing or budget system.
Tasks 1–3 have no behavioral dependency and may run on isolated branches in
parallel. Shared files require integration, not duplication of owners. Run
Cargo-backed verification sequentially when sharing a target directory.

Group rows are analytical evidence; no producer Postgres lookup/write is added.
Copy available authenticated owner Service identity without IO, preserve ordinary
unbound invocation, and resolve participating bindings only in the background.
Do not require source binding_id provenance absent from recorded rows. Each
binding uses its own pinned Trigger durations. Only one outstanding execution
per binding/group; no revision table, replacement flags or parallel adoption.

## Source-backed Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Producer metadata | wyrd-client::Observe::{verify,submit}, VerificationRequest, server VerificationControl::{execute,enqueue,stage_direct}, Principal::card_ref | Python PyObserveHandle and support-desk-app keyword example; Rust verify_in_real_time; TS Observe | No group_id; owner payloads are None | Extend existing SDK/wire and ResultPayloadBuilder input/result paths with group_id and no-IO owner metadata | No new producer operation, transport or durable staging owner |
| Source resolution | wyrd-spec Reference/VerifierSpec validation; server components/cards/resolve.rs | verifier fixtures; three SDK verifier-building-block journeys | No cross-execution input strings or binding-local source lookup | Whole slots/outcomes, exact Ref/version resolution, group_id slot refusal, existing validation errors | No second reference language beyond the approved two source forms |
| Analytical evidence | ExecutionInputsTable/ResultsTable; server verification/results.rs | service direct staging, submissions/runner result publication; pg_verification_runtime | No group metadata or bounded source snapshot selection | Extend existing managed tables and reads, deduplicate identities, order/null-align, pin selections and enforce source ceiling | No membership/revision store; retain outbox loss and visibility semantics |
| Activation/serial execution | TriggerActivation, VerificationScheduler::schedule_tenant, VerifierExecutionQueue claims/complete | scheduled_drift_alerts_operator; pg_verification_runtime | Schedule only; fencing is execution-local | Binding cursor discovery, bounded snapshot reads, existing queue serialization and settlement per binding/group | Queue-specific group fencing is necessary; no new poller, queue or hot-path lifecycle |
| Checks/live tools | vala-verify assertion::evaluate_item_mode and engine::decide; HostedTools BuiltinTool::BifrostQuery | SDK verifier_building_blocks; validate.rs check_agent_tools restriction | Empty assertions pass; check-Agent validation currently forbids tools | Empty selection inconclusive; close approved J-003 live-query Agent-check validation/execution through existing hosted tools | No span materialization, status root or new tool; enabling the approved check needs actual validator closure |
| Reactions/surfaces | VerifierExecutionQueue::complete, existing Operators, verification routes/MCP tools | pg_operator_delivery; wyrd-mcp verification::pg_tests | No verdict-change baseline/comparison, group inspection | Extend serial settlement, on_verdict_change-only group bindings and bounded inspection | No second delivery engine or combined reaction lists |

## Approach

1. Extend producer metadata and shared registration/contracts with all SDK/wire/schema consumers.
2. Resolve exact source bindings and bounded whole-slot/outcome snapshots through existing evidence owners.
3. Extend binding-cursor scheduling and the execution queue for idle/max_age/finality and serial re-judgment.
4. Close empty-assertion, source-ceiling, trace-ID/live-query and verdict-change behavior.
5. Prove each journey, isolation/negative flows and cross-replica recovery; update affected authority/docs/schema consumers with the approved contract.

## Ordered Implementation Scenarios

Each scenario is executable behavior and requires TDD. Planned journey selectors
below are new tests to add under the inspected existing targets, not claims that
those tests already exist. Run each scenario's exact selectors from Verification.

### Scenario 1 — Resumable support and registration boundaries

**Behavior.** AC-001/008 and REQ-001/005/011/013/014: chat backend, human console
and order producer use normal calls and shared group_id, with no producer IO
added. Named/UID calls with matching recorded tuples have the same eligibility;
ownerless/wrong-owner calls still execute but are excluded. Reserve group_id
for typed and reference-string input declarations; do not reserve other Python
positional/collector names. End on idle using pinned settings.

**RED.** Add grouped_support_resumes in the three existing verify-in-real-time
journey owners (commands below). Assert metadata readback, six ordered values,
field-specific registration refusal, replay/conflict, permission denial and
other-owner exclusion. Current contracts have no group_id/source mechanism.

**GREEN.** Deliver those producer, registration, background lookup and idle-review
paths across SDK/HTTP/MCP using existing owners; no append/create/close operation.

**REFACTOR.** Keep error construction, SDK projections and metadata staging shared;
rerun this scenario after tightening source/binding resolution.

### Scenario 2 — Refund correction, serial history and reactions

**Behavior.** AC-002, REQ-003/004/010: a passed promise review is followed by a
whole-group failed review on day-3 overdue rows; both result IDs remain history.
Hold prior work outstanding, coalesce arrivals, and enqueue the next snapshot
only after settlement. Finality filters new activity without rejecting calls
or deleting data. First failed result is baseline/no dispatch; changes dispatch
once, unchanged verdicts do not, and ordinary on_failure remains unchanged.

**RED.** Add grouped_refund_changes_verdict in the three journey owners. Cover
burst coalescing, no parallel review, stale settlement/replay and correct Operator
context/delivery. Registration with on_failure on group_end must fail by field.
Current scheduler/settlement lacks these group behaviors.

**GREEN.** Extend existing Trigger/queue/Operator settlement and bounded inspection
for these outcomes. Source result arrival is new activity; no pending-source wait.

**REFACTOR.** Consolidate serial settlement inside its current owner, preserving
ordinary scheduling/Operator behavior and Scenario 1.

### Scenario 3 — Tool-heavy coding and explicit live span reads

**Behavior.** AC-003/006, REQ-002/009/013: non-traced prompt/tool/edit/test/diff
calls produce per-turn judgments and a whole-group result. Only declared source
slots/outcomes enter aligned lists; absent values are null. Source child paths,
reason/status roots, missing/ambiguous bindings and cycles are refused. Checks
navigate whole lists using existing selectors. Trace slots yield IDs only;
a declared Agent check uses existing bifrost.query with ordinary tool/query
permissions, live visibility and no span snapshot.

**RED.** Add grouped_coding_uses_declared_sources in each journey owner. Assert
clean/failed coding outcomes, failed source inclusion, absent results, immutable
snapshot values, no automatic span fetch, live late spans and denied tool/query
reads. Include empty Assert each in ordinary inline and step requirements too.
Current source resolution is absent, empty assertions pass, and check tools
are refused by current validation.

**GREEN.** Extend shared sources/check handling only as required by the approved
journey, using existing hosted tools and error families; no implicit data injection.

**REFACTOR.** Reuse Reference/selector and hosted authorization owners rather than
parallel parsers/readers; rerun earlier scenarios and non-trace checks.

### Scenario 4 — Twenty sampled frames and bounded source reads

**Behavior.** AC-004, REQ-008/009: caller samples JPEG/PNG and submits twenty
independent frame executions under event_id as group_id. Event-fact discovery
anchors the zero-frame variant. Group Assert each judges whole frame outcomes;
zero selection is inconclusive/empty_selection, one failed frame fails, and
ceiling+1 gives inconclusive/source_execution_limit_exceeded without truncation.

**RED.** Add grouped_sampled_frames_are_bounded in each journey owner. Cover
exactly twenty versus twenty-one, replay/physical-row deduplication, wrong owner,
zero frames, unauthorized media and one worker death after nineteen completed
frames. Current grouping/ceiling is absent and empty comparison wrongly passes.

**GREEN.** Use existing per-file submit and per-execution retry; only the running
frame repeats. Retain group snapshots/history and both explicit inconclusive
reasons. Add no codecs, step fan-out, budgets or checkpoints.

**REFACTOR.** Keep source ceiling detection bounded before payload collection;
rerun exact tests for replay, null alignment and empty assertions.

### Scenario 5 — Concurrent groups, publication lag and agent surfaces

**Behavior.** AC-005 group portions/006/008: many group_ids coexist with Window/
Drift; cursor retries and replicas do not duplicate outstanding reviews. Bounded
discovery/read failure defers work without partial judgments. Outbox loss/lag
is not disguised as durable receipts. Agent HTTP/MCP discovery → invoke → inspect
exposes exactly the same metadata, errors and isolation as SDKs.

**RED.** Add grouped_conversations_recover in each journey owner and the planned
MCP test below. Exercise cross-owner/tenant data, no-new-IO attribution, newer
registrations versus pinned lookup, failed/truncated reads, snapshot retry and
scheduler/worker restart. Current runtime has no group execution identity/fence.

**GREEN.** Finish normal-load group concurrency and recovery using existing queue,
Bifrost and route/tool owners. Keep process-pressure work with TASK-003.

**REFACTOR.** Remove duplicate projections and stale source/binding assumptions;
rerun all task journeys. Do not add a catch-all orchestration owner.

## Acceptance Criteria

- All five scenarios satisfy their full mapped spec journeys/negatives across the shipped surfaces; every requirement above has retained evidence, not only unit proof.
- Input sources are whole slots/outcomes only; exact-version participating-binding ambiguity/cycle checks and recorded-tuple isolation hold without producer reads/writes.
- Source ceiling and empty each yield the two specified inconclusive reasons, never truncation/vacuous pass; existing nonempty decide behavior stays intact.
- Serial reviews, finality, immutable snapshots, live span-query trade-off and verdict-change baseline/dispatch behavior match revision 13.
- Shared AC split: this task owns AC-005 group/Drift correctness and AC-006 registration/read isolation, snapshots, existing retry/outbox-loss semantics and compatibility. TASK-003 alone owns new staging refusal/bounds/progress proof; combined J-005 is rerun after integration.

## Expected Write Set and Consumer Closure

Likely owners: crates/wyrd-spec (verification wire, Verifier references/validation,
Trigger and errors), crates/shared/wyrd-client (Observe/verify),
crates/vala/vala-verify, crates/vala/vala-bifrost-redux managed verification tables,
crates/wyrd/wyrd-server (registration, control, scheduler, runner/hosted checks,
results, HTTP/MCP and inspection), crates/wyrd/wyrd-sql existing queue/migrations,
wyrd-mcp tool projections, and all three SDKs including authored Python stubs,
TypeScript source-generated declarations and N-API boundary. Existing SDK
integration targets and fixture/schema/example/docs consumers close the change.
Update applicable architecture authority to the approved decision; no unrelated
UI redesign or Scribe/Oracle replacement. Per the spec's pre-release schema
constraint, change the `execution_inputs`/`results` definitions in place; add no
upgrade path, and do not weaken fingerprint or physical-schema validation. TASK-002 shares Verifier/binding files;
TASK-003 shares producer staging call sites: integrate their changes, never revert.

## Verification and Evidence

Current roots confirmed: Rust package wyrd-ai, tests/integration/main.rs target
integration with module verify_in_real_time; Python integration module below;
TS existing verify-in-real-time.test.ts; MCP target mcp, verification::pg_tests.
Add the planned tests there or record an equally focused, confirmed replacement
command if owner-local test layout changes. No new target is necessary.

Build changed native SDK dependencies once before their runtime tests:

```bash
mise run py:setup
mise run ts:build
mise run ts:build:testing
```

Scenario 1 exact planned selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_support_resumes)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_grouped_support_resumes'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^grouped_support_resumes$"'
```

Scenario 2 exact planned selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_refund_changes_verdict)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_grouped_refund_changes_verdict'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^grouped_refund_changes_verdict$"'
```

Scenario 3 exact planned selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_coding_uses_declared_sources)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_grouped_coding_uses_declared_sources'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^grouped_coding_uses_declared_sources$"'
```

Scenario 4 exact planned selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_sampled_frames_are_bounded)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_grouped_sampled_frames_are_bounded'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^grouped_sampled_frames_are_bounded$"'
```

Scenario 5 exact planned selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_conversations_recover)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_grouped_conversations_recover'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^grouped_conversations_recover$"'
```

Planned MCP journey; include SDK-equivalent grouped HTTP calls in Scenario 5:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::grouped_verification_discover_act_observe)"'
```

Confirm Rust selectors with nextest list after adding tests; pytest collection
and Vitest output must show the intended case. Add exact commands for every
additional named supporting test to task evidence. Build native SDK dependencies
once per changed build, not once per case. Use normal local provider fixtures,
WyrdTestServer and repository Postgres, never a scripted replacement engine for
journey proof. Advance fixture times/cursors rather than sleeping for days.

Run affected contract/format/boundary checks:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:deps
mise run check:tenant-isolation
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
```

For touched docs under docs/, also `mise run docs:check`. For the changed served OpenAPI schema, add planned
pg_openapi_contract::grouped_verification_contract_is_served and run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_openapi_contract::grouped_verification_contract_is_served)"'
```

Codegen alone cannot prove served routes.
Generated outputs are regenerated from their owners. No full journey suite or
repository aggregate runs in this task; integrated review owns them.

Append implementation evidence using the `$wyrd-implement` acceptance table:
criterion → source → exact command/result. Record RED, GREEN and REFACTOR,
selected features, actual selected-test counts and non-goal/diff audit. Never
count an empty selector, skipped journey or missing environment as PASS. For
unexpected failures, re-run with WYRD_LOG/RUST_LOG and follow that skill's
diagnosis rules. Private fixture choices remain implementation-owned.

## Material Stop Conditions

Route a necessary approved-behavior change through `$wyrd-spec`; do not invent
retention, a producer durability receipt, extra source syntax, parent-Service
lookup, private group tables, a new queue, distributed quotas or a revised
Operator reaction rule. Existing check-Agent tool prohibition must be reconciled
with the explicit approved live-query outcome, not silently bypassed or used to
omit J-003. Local helper/test plumbing stays with `$wyrd-implement`. Keep the
four-task split; report an actual unresolved boundary instead of creating tasks.

## Authority Links

- [Approved specification, revision 13](../spec.md).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md).
- [Wyrd design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx), [Bifrost design](../../../../architecture/bifrost-design.md).
- [Spec workflow](../../../../architecture/references/languages/spec-driven-development.md), [implementation execution](../../../../architecture/references/languages/implementation-execution.md), [testing workflows](../../../../architecture/references/languages/testing-workflows.md).
- Execute with [$wyrd-implement](../../../../.agents/skills/wyrd-implement/SKILL.md); implementation returns evidence, not independent task approval.
