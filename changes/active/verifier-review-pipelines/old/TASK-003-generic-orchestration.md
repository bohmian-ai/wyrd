---
id: TASK-003
spec: SPEC-verifier-review-pipelines
spec_revision: 18
status: proposed
previous_id: TASK-002
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
depends_on: [TASK-001, TASK-002]
req: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-045, REQ-051, REQ-052, REQ-053, REQ-054, REQ-055, REQ-056, REQ-057]
inv: [INV-001, INV-005, INV-011, INV-016, INV-017, INV-018, INV-019]
ac: [AC-003, AC-004, AC-020, AC-023, AC-038, AC-043, AC-044, AC-045, AC-046, AC-047, AC-048, AC-049, AC-050]
---

# Generic step orchestration

## Outcome and Value

Implement fixed/array fanout, ordinary conditions, sequential repeat and optional speculative steps in the existing verification engine. After this task, the three example control-flow patterns can run with Task 002 tools.

## What Changes and What Must Stay True

Task 001 supplies the engine-owned run state, Bifrost checkpoint acceptance,
authoritative recovery and exact File descriptors. Both direct and queued paths use
that lifecycle. This task extends its state with fanout/repeat/guard/disposition
progress; it adds no PostgreSQL checkpoint owner or terminal archive handoff.

| Implement | Required implementation logic |
|---|---|
| **Declarations and references** | Extend Step and existing validator/planner for foreach, repeat, any_succeeded, speculative and spec.limits (concurrency, max_agent_invocations, timeout_seconds). Preserve old list depends_on, output/status, code JSON and Prompt/Verdict schemas. Reject cycles across every potential action/guard edge and invalid known-schema paths. |
| **Ready-step execution** | Extend the existing walker/ledger so each settlement can release ready dependents immediately, without waiting for unrelated peers. Ordinary action/named bindings remain hard prerequisites. any_succeeded is eligible after one predecessor fully succeeds; all settled without success prevents dispatch. |
| **Fanout and identity** | For count, create positive independent calls without Prompt inputs. For items+max_items, validate/freeze the entire ordered array before dispatch, persisting its identity for shared run-state recovery; duplicates retain distinct indices. Use Task 001 identities and give each attempt private conversation/history/scratch. Aggregate successful final values in input order; failed children prevent a successful aggregate. |
| **Conditions and settled joins** | Evaluate normal when before bindings/expansion. False intentionally skips; missing/wrong-type/failed comparison is an error. Collection mounts wait for producer requirements/decisions and retain status/partial work; strict names require successful admitted production. Foreach named member selection refuses; repeat selects its final successful iteration only. |
| **Sequential repeat** | Run one iteration per item at a time, with max_iterations and until. Expose iteration.output, previous (initial null) and index; settle requirements before until/next iteration. Until true stops; false continues; exhaustion, invalid data or action failure cannot succeed. Restore iteration checkpoints without extra iterations. |
| **Speculation and admission** | speculative requires when and defers only guard dependencies. Pending guard permits ordinary-ready action work but withholds its content; true admits, false cancels/fences/discards, invalid/unavailable guard blocks. Persist disposition/budgets. Apply deployment/tenant/execution ceilings to actions/checks/retries; fanout never multiplies capacity, waits release unrelated permits and never replenish budgets. |

## Approach

1. Extend pure contracts, reference/dependency extraction and existing comparison validation.
2. Replace the existing stage barrier with bounded readiness progression and Task 001 checkpoint integration.
3. Implement count/items expansion, independent attempts and ordered collection.
4. Implement conditional joins and sequential repeat using the same action/check settlement path.
5. Implement speculative disposition and shared admission; project new fields/state through existing surfaces.

## Ordered Implementation Scenarios

### 1. Validate declarations

**Behavior.** Reject count zero, both count/items, absent/excess bounds, unknown refs, named foreach selections and cyclic action/guard/any_succeeded edges. Known output schemas reject impossible selections. Existing document declarations still validate.

**RED.** Pure contract/engine tests assert field-path errors and zero dispatched calls for each invalid declaration. Expected RED: the new controls lack field-specific declaration validation. Run [proof 1](#proof-1); names are planned until added/listed.

**GREEN.** Extend the current Step/reference validator and existing DAG cycle detection; keep old fields and schemas unchanged.

**REFACTOR.** Delete duplicate schema/graph checks. Keep this and earlier scenario tests green.

### 2. Release ready successors

**Behavior.** Hold reviewer B while A settles requirements. A dependent on A starts; any_succeeded starts after A succeeds, not merely after its action returns. If all candidates fail/skip, no dependent starts. A missing ordinary mount still blocks.

**RED.** Use controlled engine futures and call-order assertions; the present stage barrier delays the eligible successor. Expected RED: the stage barrier delays a ready successor. Run [proof 2](#proof-2); names are planned until added/listed.

**GREEN.** Settle each ready task independently in the existing walker and evaluate success dependencies against fully settled state.

**REFACTOR.** Keep one walker/ledger lifecycle. Keep this and earlier scenario tests green.

### 3. Run fixed sweeps

**Behavior.** foreach count 3 runs the same Agent three times with identical immutable inputs and independent scratch/history. Each writes the same scratch filename safely; a retry changes attempt, not item identity or reviewer count.

**RED.** Drive the public Agent workflow and inspect three original indices, separate attempts, source digests and call histories. Expected RED: fixed-count invocations/private-attempt attribution do not exist. Run [proof 3](#proof-3); names are planned until added/listed.

**GREEN.** Bind Task 001 identity/private workspace for each admitted call and use Task 002 tools; freeze no artificial Prompt inputs.

**REFACTOR.** Reuse the normal Agent executor rather than a mapped Agent loop. Keep this and earlier scenario tests green.

### 4. Expand array items

**Behavior.** Table: empty array → []/empty collection, not passed by itself; duplicate values → distinct indices; non-array/over-limit → zero calls; reverse completion → input-order values; failed item → no successful partial output.

**RED.** Assert item/index bindings, dispatch counts and final arrays in engine tests; public journey checks the same typed outcome. Expected RED: runtime array expansion and ordered aggregation do not exist. Run [proof 4](#proof-4); names are planned until added/listed.

**GREEN.** Validate/freeze the full expansion once in the shared ledger, checkpoint it
for recovery in the shared run state, dispatch within limits and reduce only settled successful child
values.

**REFACTOR.** Use the same invocation path as fixed sweeps. Keep this and earlier scenario tests green.

### 5. Select specialists and join work

**Behavior.** True flag runs its distinct Agent; false skips before resolving that branch's bad input; null/missing/failed decision errors. Consolidation reads succeeded-empty/skipped/failed-partial collections honestly. Missing named final report blocks judging; no applicable requirements is inconclusive.

**RED.** Public journey asserts exact branch calls, manifest status/reasons and final verdict for the case table. Expected RED: settled publication joins cannot represent optional/failed branches. Run [proof 5](#proof-5); names are planned until added/listed.

**GREEN.** Extend existing comparison/Scope/requirement reduction for status-bearing settled collections, preserving strict output/name semantics.

**REFACTOR.** Share condition and missing-evidence causes with existing checks. Keep this and earlier scenario tests green.

### 6. Repeat sequentially

**Behavior.** Until true stops; false repeats to bound; invalid until/failure/exhaustion cannot succeed. Foreach+repeat keeps each item sequential. Crash between committed action/requirements/iterations: restore previous/index and budget without repeating committed work.

**RED.** Controlled engine and real recovery cases assert one active iteration per item, exact calls, final selection and unchanged remaining bounds. Expected RED: bounded sequential iteration checkpoints do not exist. Run [proof 6](#proof-6); names are planned until added/listed.

**GREEN.** Persist iteration boundaries through Task 001 and evaluate until only after action plus requirements settle.

**REFACTOR.** Reuse invocation settlements, not nested graphs. Keep this and earlier scenario tests green.

### 7. Resolve speculative work

**Behavior.** Table: known false → no call; pending plus ready prerequisites → may start; true → admit; false → cancel/discard; null/failed guard → block. Vary whether action finishes before/after guard; consumers wait for both and never receive excluded bytes.

**RED.** Assert dispatch and ordinary mounted entries for each timing/decision case; authorized execution readers can inspect retained excluded content. Expected RED: a pending guard cannot currently start withheld speculative work. Run [proof 7](#proof-7); names are planned until added/listed.

**GREEN.** Track action settlement separately from durable admission/disposition and apply it during collection resolution.

**REFACTOR.** Keep speculation a Step timing option, not a new action type. Keep this and earlier scenario tests green.

### 8. Recover speculative decisions

**Behavior.** Crash at publication, guard commit and cancel fence, then reclaim on another replica. Previous discard/block remains final; stale worker commits fail and retained receipts never become an admission grant.

**RED.** Cluster test asserts disposition history, ordinary collection contents and rejected stale commits after each interruption. Expected RED: there is no durable speculative admission/discard record. Run [proof 8](#proof-8); names are planned until added/listed.

**GREEN.** Commit guard/disposition under Task 001 fences and recover before dispatch/release.

**REFACTOR.** Use the same checkpoint and cancellation owner. Keep this and earlier scenario tests green.

### 9. Enforce cumulative limits

**Behavior.** Hold Agent/provider/tool capacity with one tenant and admit another.
Active calls respect effective ceilings; no workspace/future is created for every
pending item. Capacity/pre-dispatch deferral spends no Agent attempt; accepted
publications use committed references without inventory rediscovery, and legitimate
other waits release unrelated permits; timeout includes waits and restart retains spend.

**RED.** Assert active/pending counters, second-tenant progress, calls/attempts and published effective limits with deterministic stalled dependencies. Expected RED: new expanded invocations are not charged to persistent shared ceilings. Run [proof 9](#proof-9); names are planned until added/listed.

**GREEN.** Extend hosted admission and cumulative accounting for actions/checks/tools without adding independent caps or resetting state on recovery.

**REFACTOR.** Reuse existing queue/capacity owners; no new scheduler service. Keep this and earlier scenario tests green.

### 10. Verify generic document controls

**Behavior.** Use a non-repository document Verifier: extract an uploaded document, array-fanout its declared work (including duplicate/empty cases), have Agents publish their accounts, conditionally run a specialist and sequentially refine before consolidation. No GitHub integration or code-review mode exists. Public results preserve item order, repeat bounds, source citations, settled status and ordinary final requirements.

**RED.** SDK → server → SDK assertions check the ordered array and retained manifests, one iteration at a time and passed/failed/inconclusive behavior. Initially the server cannot execute the new foreach/repeat/publication references even though old document extraction still works. Run [proof 10](#proof-10).

**GREEN.** Run the new controls through the existing document extractor/Agent/check engine, Task 001 evidence and Task 002 tools. Reuse the same declarations on each surface; no repository identity is required.

**REFACTOR.** Use the same engine implementation as PR verification; no document-only orchestration path. Keep earlier scenario proofs green.

### 11. Restore logical control state and retain final outcomes

**Behavior.** Run bounded controls directly and queued. Interrupt queued work
between action/check/iteration/guard acknowledgements, then restore from Bifrost
on another worker. Frozen membership, settled failures, decisions and original
budgets/deadline survive. Incompatible state refuses without dispatch or erasure.
Direct work uses the same checkpoints while staying queue-free; terminal pages
and publications remain identical without PostgreSQL invocation payloads.

**RED.** Extend the existing engine/public journey with exact restored values,
call counts and authoritative source failures. Current recovery is PostgreSQL-only
and plain direct has no checkpoint adapter. Run [lifecycle proof](#lifecycle-proof).

**GREEN.** Extend the foundation state at the existing walker/ledger/Scope and
checkpoint seams. Persist/restore control decisions before dispatch or release;
use the selected terminal state rather than a handoff or timestamp lookup.

**REFACTOR.** Keep one readiness/settlement path and shared writer; no extra
workflow actor, event-code replay, private queue or direct background scheduler.

## Acceptance Criteria

| Obligation | Required proof |
|---|---|
| REQ-001–003/009; INV-001/011; AC-020/023 | Validation/readiness plus the explicit document journey prove old extraction and a non-repository pipeline using array fanout, repeat, publications and ordinary decisions. Proof 10 closes public array behavior and AC-023; Proof 4 isolates engine ordering/bounds. |
| REQ-004–007; INV-005; AC-003–004/006/045 | Fixed/array, conditional/join proofs establish frozen identities/order, per-instance requirements, honest partial collections and strict missing evidence. |
| REQ-045; AC-038 | Repeat proof covers stopping/exhaustion/invalid values, combination with foreach and real restart. |
| REQ-051; INV-016; AC-043–044 | Speculation and recovery proofs cover readiness, all decision/completion orders, no-success dispatch and durable exclusion. |
| REQ-008; AC-009 contribution | Admission proof covers shared ceilings, effective limit reporting, queue/retry waits, cumulative spend and competing tenants. |
| Revision-18 durability obligations in metadata; DUR consumer seam | Lifecycle proof closes this capability against the shared Bifrost state foundation with unchanged identities, bounds, current permissions and exact evidence. Task 001 owns loading, acceptance, replay and old-consumer replacement. |

## Implementation Reference

VerificationEngine::with_checkpoints, walk/RunLedger and Scope already own
logical scheduling/settlements. Task 001 changes their durable host from
PostgreSQL to Bifrost and adds authoritative checkpoint loading/acceptance.
Extend that same state with control progress; do not add another graph store.

| Inspected owner / consumers | Reuse and precise gap |
|---|---|
| verifier::{Step,validate}; verification::plan_stages; reference parser | Add control fields and all possible dependency edges to these pure owners. Existing SCC witnesses and Prompt/code validation remain authoritative. |
| vala-verify::walk/RunLedger; blocks::VerificationEngine/Scope/Settled | Current walk exposes settlements only after a stage barrier and drops partial stage settlements on failure. Extend this lifecycle for readiness/checkpoints, rather than another graph runtime. |
| blocks/engine/tests.rs scripted ActionExecutor/CheckExecutor | Existing call-order/attempt/mount recording supports deterministic readiness, expansion and iteration tests without providers. Real SDK journeys complement these tests. |
| HostedBlocks; HostedTools::code_budget; VerifierRunner and Task 001 checkpoints | Current per-execution constant/process-local TaskId budget must become effective bounded invocation accounting with durable spend. Existing OperatorPermits is delivery-only; do not reuse it as a separate Agent budget. |

## Verification and Evidence

All new names below are **planned**. Existing packages, targets, runners and Postgres setup are listed in [shared verification](verification.md). After adding a test, list/confirm its actual selector before RED; zero selected tests is not proof. The RED paragraph states the missing assertion, not a claim that a test has run. Use focused owner tests plus real public journeys; no credentialed provider or synthetic host load.

### Proof 1

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::invalid_step_controls_reject_before_dispatch)'
```

### Proof 2

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::ready_successors_do_not_wait_for_unrelated_steps)'
```

### Proof 3

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::fixed_sweeps_have_private_attempts)"'
```

### Proof 4

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::array_fanout_freezes_and_orders_items)'
```

### Proof 5

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::conditions_and_collections_preserve_missing_evidence)"'
```

### Proof 6

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::repeat_stops_or_exhausts_without_overlapping)"'
```

### Proof 7

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::speculative_disposition_controls_consumer_admission)"'
```

### Proof 8

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::speculative_recovery_cannot_resurrect_content)"'
```

### Proof 9

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::admission_bounds_fanout_across_tenants)"'
```

### Proof 10

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::document_pipeline_uses_array_repeat_and_publications)"'
```

### Lifecycle proof

New selector is **planned**. Use the existing target/setup below, add/list the actual
test before RED, and record a nonzero selection plus the missing-behavior failure and
closure result. Extend the task's existing Python/TS/MCP public-closure case table with
its recovery outcomes rather than adding full suites; Task 006 uses existing
combined-surface cases. No runtime proof is claimed during planning.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::logical_control_state_resumes_from_bifrost)"'
```

### Public surface closure

Mirror authored controls, branch manifest/status, strict missing report and cancellation; use the same Verifier declarations. The Rust journey above covers the same server contract; add HTTP/OpenAPI assertions at existing route tests and CLI assertions through its in-process command surface. Planned language/MCP selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_orchestration.py::test_conditions_and_collections_preserve_missing_evidence'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-orchestration.test.ts -t "^conditions_and_collections_preserve_missing_evidence$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all --test-threads=1 -E "test(=verification::pg_tests::verifier_orchestration_conditions_and_collections_preserve_missing_evidence)"'
```

Apply only the [shared owner checks](verification.md#owner-checks) for this write set; record real command/results during implementation.

## Material Stop Conditions

No nested dynamic graphs, streams, publication subscriptions, built-in findings/quorum or alternate engine. Any change to condition errors, decide or speculative admission requires spec authority.

## Authority Links

[Approved spec, revision 18](../spec.md) · [Durability decisions 1–5](../execution-state-and-durability.md) · [Task ownership/dependencies](README.md) ·
[Publication contract](../output-publication.md) · [PR
extraction](../pull-request-extraction.md) ·
[Orchestration](../example-orchestration.md).

[Wyrd design](../../../../architecture/wyrd-design.md) · [Bifrost design](../../../../architecture/bifrost-design.md) · [Security](../../../../architecture/wyrd-security-posture.md) · [Implementation execution](../../../implementation-execution.md) · [Testing](../../../../TESTING.md). Apply nested AGENTS.md at each implementation owner.
