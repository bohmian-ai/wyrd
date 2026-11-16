---
id: TASK-003
spec: SPEC-verifier-review-pipelines
spec_revision: 22
status: proposed
previous_id: TASK-002
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
depends_on: [TASK-001-R5, TASK-002]
req: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-032, REQ-045, REQ-051, REQ-052, REQ-053, REQ-054, REQ-055, REQ-056, REQ-057]
inv: [INV-001, INV-005, INV-011, INV-016, INV-017, INV-018, INV-019, INV-027]
ac: [AC-003, AC-004, AC-009, AC-020, AC-023, AC-038, AC-043, AC-044, AC-045, AC-046, AC-047, AC-048, AC-049, AC-050]
---

# Generic step orchestration

## Outcome and Value

Implement fixed/array fanout, ordinary conditions, sequential repeat and optional speculative steps in the existing verification engine. After this task, the three example control-flow patterns can run with Task 002 tools.

## What Changes and What Must Stay True

The user approved the revision-22 scope correction on 2026-11-08. The approved
baseline remains revision 18 plus its approved amendments; unrelated draft
wait/approval decisions are excluded. Existing implementation evidence below is
historical, not new verification. The remaining executable work is scenarios 5
and 6. Previously correct behavior needs regression proof, not a manufactured
RED. TASK-003 remains incomplete until these scenarios and required checks pass.

Admission uses existing shared process capacity, per-execution bounds and tenant
claim rotation. It adds no per-tenant Agent quota, capacity reservation or
cluster-wide quota. Full capacity can make another tenant wait; cancellation or
completion releases capacity for queued work. No acceptance-time limits response
is added. Server recovery fault injection stays in Rust real-server journeys;
Python/TypeScript exercise their public contracts with existing harness controls.

Implemented TASK-001 supplies the baseline; TASK-001-R5 carries the existing R4 contract and owns remaining
corrections/proofs for engine-owned state, Bifrost checkpoint acceptance,
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
| **Speculation and admission** | speculative requires when and defers only guard dependencies. Pending guard permits ordinary-ready action work but withholds its content; true admits, false cancels/fences/discards, invalid/unavailable guard blocks. Persist disposition/budgets. Apply shared process and per-execution ceilings to actions/checks/retries; fanout never multiplies capacity, waits release unrelated permits and never replenish budgets. |

## Approach

1. Extend pure contracts, reference/dependency extraction and existing comparison validation.
2. Replace the existing stage barrier with bounded readiness progression and Task 001 checkpoint integration.
3. Implement count/items expansion, independent attempts and ordered collection.
4. Implement conditional joins and sequential repeat using the same action/check settlement path.
5. Implement speculative disposition and shared admission; project new fields/state through existing surfaces.

## Ordered Implementation Scenarios

Scenarios describe authored verification journeys. Declaration failures, timing
orders, limits and crashes are test cases beneath the relevant journey; they do
not become separate implementation phases or repeat Task 001's storage work.

### Scenario 1 — Run parallel reviewers and consolidate their work

**Behavior.** A developer declares repeated reviewers over the same immutable
evidence, or maps a frozen runtime array. Agents publish their accounts and a
consolidator consumes settled collections. Independent work runs within shared
limits; successful values retain input order and retries do not become extra
reviewers. A ready successor need not wait for an unrelated reviewer.

| Test case within this journey | Expected observation and focused proof |
| --- | --- |
| Invalid count/items/bounds, impossible or unknown references and dependency cycles | Field-specific refusal before dispatch; old declarations still work. [Validation cases](#proof-1). |
| A settles before B; ordinary and any_succeeded dependencies | Release only after required settlements; all-failed/skipped candidates start no successor and missing ordinary evidence still blocks. [Readiness cases](#proof-2). |
| Fixed sweeps with identical scratch names and a retry | Original item indices, independent attempt scratch/history, identical pinned inputs and no extra reviewer. [Public sweep journey](#proof-3). |
| Empty/duplicate/non-array/over-limit items and reversed completion | Honest empty collection, distinct indices, zero calls on invalid input, ordered values and no successful partial aggregate. [Array cases](#proof-4). |
| Capacity held by one tenant, blocked IO and recovery | Bounded active/pending work, competing-tenant progress when shared capacity is released, no pre-dispatch spend, no replenished budgets and accepted references reused directly. [Admission cases](#proof-9). |
| Configured long-running active work, renewal, cancellation and restart | Existing shared process and per-execution limits remain authoritative; lease renewal sustains permitted work, recovery keeps the original deadline and budgets, cancellation releases capacity. Use controlled time and existing admission/lifecycle seams rather than an hours-long sleep or synthetic host load. [Admission cases](#proof-9) and [control-state cases](#lifecycle-proof). |
| Non-repository document pipeline using array work and refinement | Same public controls, publications and verdicts without a code-review mode. [Document journey](#proof-10). |
| Restart or incompatible state | Frozen control membership/settlements and original bounds survive, or explicit refusal without dispatch/erasure. [Control-state cases](#lifecycle-proof). |

**RED.** Run the [public sweep journey](#proof-3) and supporting cases to assert
actual Agent calls, immutable mounts, ordered results and consolidation. The
current engine cannot expand the new work or release ready successors correctly.
Confirm planned selectors before RED; pure ordering tests support rather than
replace the public journey.

**GREEN.** Extend existing validation, dependency extraction, walker/ledger and
Scope for bounded readiness and frozen count/array expansion. Use Task 002's
tools and Task 001's state; preserve successful-producer and partial-status rules.

**REFACTOR.** Keep one invocation path, planner and state owner. Share fixed/array
execution without another scheduler, graph store or mapped Agent loop.

### Scenario 2 — Select conditional specialists and consolidate honest outcomes

**Behavior.** A planner returns decisions selecting distinct specialist Agents.
True runs the specialist; false skips it before binding its workspace. The
consolidator consumes settled successful, empty, skipped and failed-partial
collections without treating a failure as a clean review. An authored named-report
requirement still refuses a missing report; no applicable requirements is
inconclusive.

**RED.** Run the [conditional-specialist journey](#proof-5) with neither/either/both
specialists selected, plus null/missing/failed decisions, failed producers and
missing named evidence. Assert exact calls, collection status/reasons and final
verdict. The current conditional/collection path cannot express the full journey.

**GREEN.** Extend existing comparison/Scope/requirement reduction and conditional
readiness; decide before resolving branch inputs and retain decisions/status in
the common run state. Preserve strict successful output/name selection.

**REFACTOR.** Share existing missing-evidence and condition causes; no special
reviewer types, quorum or report schema. Keep the parallel-review journey green.

### Scenario 3 — Refine an output through bounded sequential iterations

**Behavior.** A developer asks an Agent to refine an output until the declared
condition is met, then consumes the final successful report. Each iteration sees
its recorded previous value and index; only one iteration per item runs at once.
Until true stops, false continues within the bound, and invalid decision, failure
or exhaustion cannot fabricate success. The same behavior composes with fanout.

**RED.** Add the [public refinement journey](#sequential-refinement-journey) and
support it with [iteration/recovery cases](#proof-6). Assert final report selection,
previous/index values, stop/exhaustion behavior and call counts across interruption.
The new repeat contract and durable iteration progression are missing.

**GREEN.** Extend the existing action/requirement settlement path with sequential
iteration state; evaluate until only after both settle. Restore iterations through
Task 001 without repeating accepted work or resetting attempts/deadline. The
[document journey](#proof-10) also exercises array-plus-repeat composition.

**REFACTOR.** Reuse ordinary invocation/checkpoint machinery rather than nested
graphs or another retry loop; keep prior journey proofs green.

### Scenario 4 — Start an extra review early and admit or discard it

**Behavior.** In the speculative-review configuration, the first successful
bootstrap makes an extra review eligible while comparison is pending. Once the
comparison settles, admit the extra contribution or cancel/discard it. Consolidation
waits for action and decision; retained receipts alone never grant admission.

| Test case within this journey | Expected observation and focused proof |
| --- | --- |
| Guard known false, pending/true/false/invalid; action finishes before or after it | No known-false call; pending output withheld; true admitted, false excluded, invalid blocked. Ordinary consumers never receive excluded bytes. [Public speculative journey](#proof-7). |
| Crash at publication, guard acceptance or cancellation; stale completion | Recorded disposition remains final and rejected work cannot resurrect. [Speculative recovery cases](#proof-8) and [control-state cases](#lifecycle-proof). |
| Debugger inspects retained excluded content | Existing execution-read grants permit inspection without changing downstream eligibility. Covered in the public journey and Task 006's inspection cases. |

**RED.** Drive the [public speculative journey](#proof-7) across decision/completion
orders and the recovery cases above. Assert actual calls, ordinary mounted entries,
final report and authorized debugging. Withholding/admission is currently missing.

**GREEN.** Record action settlement separately from guard disposition in the same
run state and apply both before dependency release. Reuse Task 001's acceptance,
cancellation and recovery contracts rather than new persistence machinery.

**REFACTOR.** Keep speculation an ordinary Step timing option and one collection
resolver; no event subscriptions or speculative descendant workflow.

### Scenario 5 — Waiting on a cancelled execution reports cancellation

**Behavior.** A user submits work, cancels it through the public SDK and waits
on the same execution. The wait raises the shared catalog error
`WYRD_SDK_409_VERIFICATION_CANCELLED`, status 409, with the execution ID and
`cancelled` status in details. It returns no result and starts no more work.
Status reads continue to expose the existing stored `cancelled` code. Rust,
Python and TypeScript must report the same wait error; a caller wait timeout
retains its existing separate error.

**RED.** Extend the existing [SDK cancellation journeys](#cancelled-wait-proof)
to assert the exact catalog code, status and execution identity. Replace the
Python assertion that merely accepts `details.original_code == cancelled`.
The expected missing-behavior failure is the current upstream-failure error,
not a fixture/setup failure. Confirm nonzero test selection before RED.

**GREEN.** Declare the cancellation error in the existing `wyrd-spec` catalog
and translate terminal cancellation once in the shared Rust client. Reuse
ordinary language error projection and regenerate consumed catalog artifacts.
Keep stored server codes and response shapes unchanged. Rerun these focused
journeys and the existing cancellation/readback regression.

**REFACTOR.** Keep terminal interpretation in the shared execution owner, not
separate Python/TypeScript exception rules or a generic alias for non-catalog
transport codes. Preserve deadline, authorization and unknown-error behavior.

### Scenario 6 — Cancellation survives competing state writers

**Behavior.** Cancellation races a worker checkpoint with the server's terminal
record delivery for the same execution. Force the competing writes rather than
relying on scheduling luck. Cancellation must stay retained; pending terminal
delivery must reach Bifrost acknowledgement and retire; later reads remain
cancelled with no successful result; accepted reports remain inspectable; no
cancelled work resumes; unrelated work sharing the Scribe lane completes.
Exercise both writer arrival orders as timing cases of this one race.

**RED.** Add the [planned cancellation-race regression](#cancellation-race-proof)
in the existing server integration owner, using deterministic synchronization
at the actual checkpoint/delivery boundary. Assert the complete workflow above,
not merely `BatchConflict`. List its actual selector before running it. If the
existing implementation already passes, record verification-only regression
proof; do not manufacture RED or change production code to justify the test.
If it fails, record which terminal decision or delivery is lost/stranded.

**GREEN.** First reuse the existing terminal selection, fenced recovery and
pending delivery owners. Change production behavior only if the forced race
exposes a defect, repairing its owning shared path without a second writer or
retry system. Rerun the forced race, the existing terminal-delivery and
retained-readback tests, and the affected real SDK cancellation journey.

**REFACTOR.** Keep synchronization private to Rust test support. Preserve Scribe
batch conflict isolation and ACK-before-delivery-retirement. Do not add SDK
fault controls, new coordination stores, migrations or a cancellation service.

## Acceptance Criteria

| Obligation | Required proof |
|---|---|
| REQ-001–003/009; INV-001/011; AC-020/023 | Validation/readiness plus the explicit document journey prove old extraction and a non-repository pipeline using array fanout, repeat, publications and ordinary decisions. Proof 10 closes public array behavior and AC-023; Proof 4 isolates engine ordering/bounds. |
| REQ-004–007; INV-005; AC-003–004/006/045 | Fixed/array, conditional/join proofs establish frozen identities/order, per-instance requirements, honest partial collections and strict missing evidence. |
| REQ-045; AC-038 | Repeat proof covers stopping/exhaustion/invalid values, combination with foreach and real restart. |
| REQ-051; INV-016; AC-043–044 | Speculation and recovery proofs cover readiness, all decision/completion orders, no-success dispatch and durable exclusion. |
| REQ-008; AC-009 contribution | Admission proof covers shared process/per-execution ceilings, queue/retry waits, cumulative spend and competing-tenant progress when capacity becomes available; no limits response or tenant reservation. |
| REQ-032; public cancellation/error closure | Scenario 5 proves exact shared cancellation errors on Rust/Python/TypeScript, unchanged status payloads, no result and no resumed work. An upstream error or language-specific mapping fails this proof. |
| REQ-055/057; AC-049 contribution; AC-009 | Scenario 6 forces both cancellation-writer arrival orders. Retained cancellation, accepted reports, terminal ACK/retirement and unrelated completion are required; a stranded delivery or resurrected success fails. |
| Revision-18 durability obligations in metadata; DUR consumer seam | Lifecycle proof closes this capability against the shared Bifrost state foundation with unchanged identities, bounds, current permissions and exact evidence. R5 carries R4’s remaining loading, acceptance, replay and old-consumer corrections/proofs; original TASK-001 is the implemented baseline. |

## Implementation Reference

VerificationEngine::with_checkpoints, walk/RunLedger and Scope already own
logical scheduling/settlements. Implemented TASK-001 introduced the Bifrost host;
R5 carries R4’s remaining authoritative loading/acceptance corrections.
Extend that same state with control progress; do not add another graph store.

| Inspected owner / consumers | Reuse and precise gap |
|---|---|
| verifier::{Step,validate}; verification::plan_stages; reference parser | Add control fields and all possible dependency edges to these pure owners. Existing SCC witnesses and Prompt/code validation remain authoritative. |
| vala-verify::walk/RunLedger; blocks::VerificationEngine/Scope/Settled | Current walk exposes settlements only after a stage barrier and drops partial stage settlements on failure. Extend this lifecycle for readiness/checkpoints, rather than another graph runtime. |
| blocks/engine/tests.rs scripted ActionExecutor/CheckExecutor | Existing call-order/attempt/mount recording supports deterministic readiness, expansion and iteration tests without providers. Real SDK journeys complement these tests. |
| HostedBlocks; HostedTools::code_budget; VerifierRunner and Task 001 checkpoints | Current per-execution constant/process-local TaskId budget must become effective bounded invocation accounting with durable spend. Existing OperatorPermits is delivery-only; do not reuse it as a separate Agent budget. |

The remaining-work reuse map is source-backed against the current checkout:

- `wyrd-client::verify::Execution::wait_until` calls
  `error::code_to_wyrd_error` for terminal `cancelled`; the latter only rebuilds
  catalogued codes and otherwise returns `UpstreamFailure`. The catalog has a
  wait-timeout error but no cancelled-wait error. Extend this execution/catalog
  boundary; do not teach every generic transport mapper a server-state alias.
- Python's orchestration journey currently asserts only `original_code`;
  TypeScript asserts cancel/status without waiting. The existing Rust
  admission journey owns cancellation/capacity-release closure. Extend these
  callers with exact error assertions using ordinary SDK error classes.
- `pg_durable_verifier_work` owns real Postgres/Bifrost terminal-delivery and
  retained-readback tests. `VerifierRunner` terminal selection/delivery and the
  existing `RunState` checkpoints are the competing production paths. Existing
  tests do not force their same-revision cancellation contention. Extend this
  integration owner with a deterministic race; retain the Scribe isolation fix.
- Likely changes: `crates/wyrd-spec/src/error.rs`, shared client execution owner,
  consumed generated error artifacts, the existing orchestration journeys in
  Rust/Python/TypeScript, and the server integration test/support owners.
  Python stubs or native wrappers change only if required by actual projection;
  no harness lifecycle API is part of this outcome.

## Verification and Evidence

Proof numbers identify supporting tests, not additional implementation scenarios.
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
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::repeat_stops_or_exhausts_without_overlapping)"'
```

### Proof 7

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::speculative_disposition_controls_consumer_admission)"'
```

### Proof 8

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::speculative_recovery_cannot_resurrect_content)"'
```

### Proof 9

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::admission_bounds_fanout_across_tenants)"'
```

### Proof 10

Supporting non-repository SDK → real server → SDK journey: extract an uploaded
document, map its declared array work (including duplicate/empty variants), have
Agents publish accounts, conditionally run a specialist and sequentially refine
before consolidation. Assert ordered values, exact manifests/source citations,
settled status, one active iteration and original repeat bounds, plus ordinary
passed/failed/inconclusive final requirements. Reuse the same declarations on
public surfaces without repository identity, GitHub or a document-only engine.
This is the cross-domain proof for the orchestration journeys, not another
implementation phase.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::document_pipeline_uses_array_repeat_and_publications)"'
```

### Lifecycle proof

The Rust real-server journey below already exists and drives takeover through
`Desk::take_over`, reusing bound-worker cancellation, tenant-scoped lease expiry
and `Deployment::replica`. Retain its assertions on accepted control state and
public SDK reads after recovery. Do not add worker-stop, lease-expiry or replica
controls to Python/TypeScript harnesses, or duplicate fault injection in MCP.
Their public-contract closure remains below. Historical results are recorded in
the evidence table; planning claims no new runtime proof.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::logical_control_state_resumes_from_bifrost)"'
```

### Sequential-refinement journey

Planned public SDK → real server → SDK journey. Assert the final successful
report, previous/index values, stop/exhaustion/error outcomes and conserved
iteration identity across interruption; the cluster test in proof 6 isolates
non-overlap and recovery timing. Confirm the selector before RED.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::sequential_refinement_returns_final_admitted_report)"'
```

### Public surface closure

Mirror authored controls, branch manifest/status, strict missing report and cancellation; use the same Verifier declarations. The Rust journey above covers the same server contract; add HTTP/OpenAPI assertions at existing route tests and CLI assertions through its in-process command surface. Planned language/MCP selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_orchestration.py::test_conditions_and_collections_preserve_missing_evidence'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-orchestration.test.ts -t "^conditions_and_collections_preserve_missing_evidence$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all --test-threads=1 -E "test(=verification::pg_tests::verifier_orchestration_conditions_and_collections_preserve_missing_evidence)"'
```

### Cancelled-wait proof

Use the existing Rust admission journey's cancellation/capacity-release case
and the Python/TypeScript conditional journeys' cancellation cases. Add public
SDK cancel/wait assertions to those cases. The Rust selector is recorded once
under [proof 9](#proof-9); Python/TypeScript exact commands are under
[public surface closure](#public-surface-closure). Do not rerun a selector as
separate proof obligations. These selectors are confirmed from current source.
Use setup prerequisites in [shared verification](verification.md#existing-targets-and-setup)
once before the affected languages. Expected result: all three waits raise the
same catalog error with status and identity, while retained status stays
cancelled. The retained-readback regression below also remains green.

### Cancellation-race proof

The first selector is **planned**, in `wyrd-server`'s existing `integration`
target. Its module requires the existing `test-support` feature. Confirm the
actual selector after adding it and before execution; a zero selection is not
proof. The following two selectors already exist and cover acknowledged
terminal delivery and retained cancellation after queue/Card deletion.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::cancellation_writer_race_preserves_terminal_delivery)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::terminal_delivery_survives_every_commit_boundary)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::cancellation_reads_back_from_state_after_the_queue_and_cards_are_gone)"'
```

The forced race must assert retained cancellation, preserved accepted reports,
no resumed work or success, ACK before delivery retirement, and completion of
unrelated work in the shared lane. The retained-readback test must still pass
once source queue rows and Cards are removed. The linked SDK cancellation selectors
supply public journey regression alongside this server-boundary proof.

### Static proof of the revision-22 correction

TDD is not applicable to the document-only scope correction. Check this task,
the spec, shared contracts and packet index together: no new tenant quota,
acceptance-time limits snapshot or language-harness fault controls are required;
existing enforcement, tenancy, recovery and public-surface proof remain owned.
Historical evidence is not a new PASS. Scenarios 5 and 6 remain UNVERIFIED until
implementation records the focused results and applicable shared owner checks.

Apply only the [shared owner checks](verification.md#owner-checks) for this write set; record real command/results during implementation.

## Material Stop Conditions

No nested dynamic graphs, streams, publication subscriptions, built-in findings/quorum or alternate engine. Any further change to condition errors, decide, speculative admission, the approved cancelled-wait contract or terminal durability requires spec authority.

Long-running proof does not raise default timeouts or deployment ceilings.
Report incompatible advertised duration and configured limits explicitly.
Durable wait/human-approval actions are owned by TASK-007 under the revision-20 extension;
ordinary async I/O and DAG readiness in this task do not establish suspension.

## Authority Links

[Approved baseline and revision-22 amendment](../spec.md) · [Durability decisions 1–5](../execution-state-and-durability.md) · [Task ownership/dependencies](README.md) ·
[Publication contract](../output-publication.md) · [PR
extraction](../pull-request-extraction.md) ·
[Orchestration](../example-orchestration.md).

[Wyrd design](../../../../architecture/wyrd-design.md) · [Bifrost design](../../../../architecture/bifrost-design.md) · [Security](../../../../architecture/wyrd-security-posture.md) · [Implementation execution](../../../implementation-execution.md) · [Testing](../../../../TESTING.md). Apply nested AGENTS.md at each implementation owner.

## Obligations

Historical line numbers refer to this file before the obligations section was appended; revision-22 corrections cite the current named sections.

| ID | Source | Observation | Planned assertion |
|---|---|---|---|
| O1.1 | L30, L72 | `foreach` with zero count, both or neither of count/items refused with field path before dispatch | validation unit test: rule/path per case |
| O1.2 | L30, L72 | `items` without positive `max_items`, unknown/impossible items reference refused | same test |
| O1.3 | L30, L72 | cycles across every action/guard/`any_succeeded` edge refused | same test, graph rule |
| O1.4 | L30, L72 | `any_succeeded` must be nonempty, unique, known; `speculative` requires `when`; repeat needs positive `max_iterations`; limits positive | same test |
| O1.5 | L30, L72 | old list `depends_on`, `output`/`status`, code JSON and Prompt/Verdict declarations still validate and run | existing engine/validation suites stay green |
| O1.6 | L109 | foreach named-member selection refused; repeat named selection allowed | validation test |
| O2.1 | L31, L73 | a successor of A starts while unrelated B still runs | engine `ready_successors_do_not_wait_for_unrelated_steps` |
| O2.2 | L31, L73 | `any_succeeded` successor starts after first success | same |
| O2.3 | L31, L73 | all candidates failed/skipped: no dispatch, cause preserved | same |
| O2.4 | L31, L73 | `any_succeeded` does not waive an ordinary binding | same |
| O3.1 | L32, L74 | count fanout makes N independent calls with item indices and no injected inputs | engine fanout test + `fixed_sweeps_have_private_attempts` |
| O3.2 | L32, L74 | each attempt has private scratch/history; identical scratch names do not collide | `fixed_sweeps_have_private_attempts` |
| O3.3 | L74 | every item sees identical pinned mounts | same |
| O3.4 | L74 | a retry is an attempt of the same item, not an extra reviewer | engine fanout test + journey |
| O3.5 | L32 | successful aggregate output is input-ordered array | engine fanout test |
| O4.1 | L75 | empty items: `[]`, empty collection, zero calls, not a clean verdict | `array_fanout_freezes_and_orders_items` |
| O4.2 | L75 | duplicate items keep distinct indices | same |
| O4.3 | L75 | non-array items: zero calls, step failure | same |
| O4.4 | L75 | over-limit items: zero calls, step failure, no truncation | same |
| O4.5 | L75 | reversed completion keeps input order | same |
| O4.6 | L32, L75 | a failed child prevents successful aggregate; statuses retained | same |
| O4.7 | L32 | frozen items persisted for recovery; restore reuses them | lifecycle proof |
| O5.1 | L33, L91-95 | true condition runs specialist; false skips before binding workspace | `conditions_and_collections_preserve_missing_evidence` |
| O5.2 | L33 | null/missing/wrong-type/failed decision is an error, not false | same + engine test |
| O5.3 | L33 | collection mounts wait for producer requirements and retain skipped/failed status and partial work | same |
| O5.4 | L33, L92 | strict named report missing refuses consumer | same |
| O5.5 | L92 | no applicable requirements is inconclusive | same |
| O6.1 | L34, L111 | one iteration per item at a time | `repeat_stops_or_exhausts_without_overlapping` + engine repeat test |
| O6.2 | L34 | `iteration.previous` null then previous value; `iteration.index` zero-based; `iteration.output` only in until | engine repeat test + `sequential_refinement_returns_final_admitted_report` |
| O6.3 | L34 | until true stops successfully; false continues | same |
| O6.4 | L34 | exhaustion, invalid until data, action failure cannot succeed | same |
| O6.5 | L34 | step requirements settle before until and next iteration | engine repeat test |
| O6.6 | L111 | repeat composes with foreach: independent per item, ordered final values | engine test + document journey |
| O6.7 | L34, L118 | recovery restores iterations without extra iterations, attempts or deadline reset | proof 6 + lifecycle |
| O6.8 | L109 | repeat named selection reads final successful iteration only | engine test |
| O7.1 | L35 | speculative pending guard: action runs, content withheld | `speculative_disposition_controls_consumer_admission` + engine test |
| O7.2 | L135 | known-false guard makes no call | same |
| O7.3 | L135 | true admits; false cancels/discards; invalid blocks | same |
| O7.4 | L135 | ordinary consumers never receive excluded bytes; consolidation waits for action and decision | same |
| O7.5 | L135 | action finishing before/after guard both honoured | same |
| O7.6 | L136 | recorded disposition is final across crash; stale completion cannot resurrect | proof 8 + lifecycle |
| O7.7 | L137 | retained excluded content inspectable with execution-read grants | public speculative journey |
| O7.8 | L35 | true guard + failed action stays visible as failed | engine test |
| O9.1 | L35, L76 | active work bounded by shared process and per-execution ceilings; fanout never multiplies; `limits.concurrency` and `foreach.concurrency` only lower | engine admission test + proof 9 |
| O9.2 | L35, L104 | `max_agent_invocations` counts actions/checks/retries; persisted spend never replenished | engine test + proof 9 |
| O9.3 | L77 | `limits.timeout_seconds` lowers the deadline; recovery keeps original deadline; cancellation releases capacity | proof 9 + lifecycle |
| O9.4 | L76; revision-22 correction | competing tenant progresses when shared capacity is released; no reserved tenant capacity | proof 9 |
| O9.5 | L104; revision-22 correction | acceptance-time limits snapshot retired; enforcement retained | static contract check; proof 9 retains enforcement |
| O10 | L78, L249-261 | non-repository document pipeline: array, conditional, repeat, publications, ordinary verdicts | proof 10 |
| OL | L79, L264-276 | logical control state (membership, settlements, decisions, bounds) resumes from Bifrost; incompatible state refused without dispatch | lifecycle proof |
| OC | Revision 22; scenario 5 | cancelled wait raises exact shared catalog error across SDKs; stored status unchanged | cancelled-wait proof |
| OR | Revision 22; scenario 6 | forced cancellation-writer contention preserves retained cancellation and completes terminal delivery/unrelated work | cancellation-race proof |
| OP | L289-299 | Python/TS/MCP/HTTP/CLI mirror conditions/collections/strict missing report/cancellation | public surface closure selectors |

## Implementation Evidence

Implementation status: **IMPLEMENTED** at `384a0def7`. Every ledger obligation is PASS except O9.5, which revision 22 retires by explicit user approval. A fresh read-only checker over this task, its ledger and `git diff main...HEAD` found no obligation without a proving assertion; the two stale citations it named (OL, O-L158) are re-anchored. Routes to `$wyrd-task-review`.

Commits: `cfb6ce607` (engine, contracts, server), `5d7088339`…`048820be9`, `3c9073b79`, `c6c7c8ed8`, `345648d74`, `b4e532c90`, `1e3fbb617`, `2eac9f3af`, `23ae55ade`, `a915dc877`, `2beec832d`; revision 22: `c6ddfb417` (scenario 5), `43005c0f9`, `50b063e4d` (scenario 6).
Abbreviations: VV = `crates/vala/vala-verify/src/blocks/engine/tests.rs`; VS = `crates/wyrd-spec/src/card/verifier/validate.rs`; RJ = `sdks/wyrd-sdk-rust/tests/integration/verifier_orchestration.rs`; TS = `sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-orchestration.test.ts`; PY = `sdks/wyrd-sdk-python/tests/integration/test_verifier_orchestration.py`; MCP = `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs`; J(name) = `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::<name>)"'`; L = `mise exec -- cargo nextest run --locked -p vala-verify -p wyrd-spec --lib` (833/833 pass).

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| OC: exact cancelled-wait error | `wyrd-spec` `error.rs:1420` `SdkVerificationCancelled` (409); `wyrd-client` `verify/execution.rs:323` translates terminal `cancelled` once in `wait_until`; generated `error-codes.ts`, `api/errors.md`, `llms-full.txt`; wait docs in `verify.rs`, `.pyi`, `native/src/verification.rs`, `index.ts`, `wyrd-design.md`, the run-a-verification guide (`c6ddfb417`) | RJ:1029-1048 (code, 409, `execution_id`, `cancelled`); RJ:1049-1062 (status keeps `cancelled`, no result); RJ:1063-1068 (no further work); PY:171-185 (same error, status unchanged, no result, no resume); TS:435-447 (same error, status unchanged, no result, no further work); wait timeout and authorization unchanged: `verify_in_real_time.rs:966`, PY `test_verify_in_real_time.py:349`, TS `verify-in-real-time.test.ts:339` (another tenant's wait raises `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT`); unknown codes unchanged: `wyrd-client` `error.rs:329-347` (catch-all keeps `original_code`), and `wait_until` adds only the `cancelled` branch | J(conditions_and_collections_preserve_missing_evidence): 1 passed; `another_tenant_cannot_resume_an_execution` in Rust, Python and TypeScript: 1 passed each; `error::tests::unknown_code_preserved_in_catch_all`: 1 passed; PY and TS whole files 3 passed each; `cancellation_reads_back_from_state_after_the_queue_and_cards_are_gone`: 1 passed | PASS |
| OR: forced cancellation-writer race | verification-only regression: existing terminal selection, fenced recovery and pending delivery owners; test-only `FirstWriter`, `cancellation_race`, `owed_until`, `outbox_lane` (`43005c0f9`, `50b063e4d`, `384a0def7`) | `pg_durable_verifier_work.rs`: 2727-2733 (both arrival orders); 2800-2831 (checkpoint confirmed and held before the cancel; cancel commits); 2832-2847 (delivery held before Scribe stays owed); 2737-2762, 2848-2862 (an unrelated execution is submitted while the delivery is held and pinned to its outbox lane through the public `OutboxSink::lane`; `scribe_outbox.rs:2680-2681` pins an execution's inputs and state revisions to one lane); 2864-2884 (each order released; only a checkpoint admitted first is accepted); 2885-2903 (every delivery retires, so the refused delivery was redelivered); 2904-2920 (the unrelated execution in that lane is `completed` with its result retained); 2922-2950 (cancelled, no terminal result, exact attempts, accepted report remains); 2951-2954 (no resumed work); 2955-2972 (public read cancelled, `result` null); 2973-2992 (public publication listing still holds the report). ACK before retirement once Scribe accepted: 3863-3866 (an acknowledged decision stays owed while its retirement fails) and 3877 (recorded once) in `terminal_delivery_survives_every_commit_boundary` | `cancellation_writer_race_preserves_terminal_delivery` (selection 1 test): 1 passed; trace shows the delivery frame refused and redelivered with the checkpoint first, and the worker frame refused with the delivery first; `terminal_delivery_survives_every_commit_boundary`, `cancellation_reads_back_from_state_after_the_queue_and_cards_are_gone`, `preclaim_group_cancellation_preserves_retained_identity`, `cancelled_work_keeps_checkpoints_and_fences_stale_writes`, `batched_checkpoints_recover_exactly_the_accepted_batch`: 6 passed; J(conditions_and_collections_preserve_missing_evidence): 1 passed | PASS |
| O1.1: zero count, both/neither count/items refused with path | `wyrd-spec` `card/verifier/validate.rs` foreach rules | VS:2719-2728, VS:2818 | L `step_controls_refuse_invalid_shapes` | PASS |
| O1.2: items without positive max_items, unknown/impossible items refused | same | VS:2733-2758, VS:2818 | same | PASS |
| O1.3: cycles across action/guard/any_succeeded refused | validate graph edges; `plan_stages` | VS:2829-2846 (ordinary and speculative guard cycles, any_succeeded cycle); VV:3412-3413, VV:3421-3422 | L `step_controls_refuse_invalid_shapes`, `invalid_step_controls_reject_before_dispatch` | PASS (`cf376e411`) |
| O1.4: any_succeeded nonempty/unique/known; speculative needs when; repeat positive; limits positive | same | VS:2763-2788, VS:2820-2826 (each limit at 0) | L `step_controls_refuse_invalid_shapes` | PASS (`cf376e411`) |
| O1.5: old depends_on/output/status/code/Prompt/Verdict still validate and run | unchanged owners | whole existing suites | L 833/833; `wyrd-sql --lib` 96/96; 9 server cancel/renewal tests | PASS |
| O1.6: foreach named member refused; repeat named allowed | validate `fanout_member` | VS:2846-2858, VS:2859-2867 | L | PASS |
| O2.1: successor of A starts while B runs | `walk.rs` readiness | VV:2774, VV:2786 | L `ready_successors_do_not_wait_for_unrelated_steps` | PASS |
| O2.2: any_succeeded starts after first success | same | VV:2778, VV:2782-2784 | same | PASS (`cf376e411`) |
| O2.3: all candidates failed/skipped → no dispatch, cause kept | same | VV:2794-2801 | same | PASS |
| O2.4: any_succeeded keeps ordinary bindings | same | VV:2794-2799 | same | PASS |
| O3.1: count fanout N calls, item indices, no injected inputs | engine foreach | VV:2911-2921 (`counted` outputs `[{}, {}]`, one call per index); RJ:701-709 | L `array_fanout_freezes_and_orders_items`; J(fixed_sweeps_have_private_attempts) | PASS (`d48630e2f`) |
| O3.2: private scratch/history per attempt | hosted attempt workspaces | RJ:723-734 (every conversation, including reviewer 0's retry, holds only its own tool calls); RJ:737-741, RJ:747-751 (empty and private scratch) | J(fixed_sweeps_have_private_attempts) | PASS (`603398744`) |
| O3.3: identical pinned mounts | same | RJ:742, RJ:754 | same | PASS |
| O3.4: retry is an attempt of the same item | engine attempts | VV:2977-2989; RJ:770-777 | L; J(fixed_sweeps…) | PASS |
| O3.5: input-ordered aggregate | engine aggregation | VV:2878-2881 | L `array_fanout…` | PASS |
| O4.1: empty items → `[]`, empty collection, zero calls, not a clean verdict | engine items | VV:2897-2901 (`empty` output `[]`, Succeeded); VV:2936-2943 (`empty` zero calls); VV:2902-2910 (`empty-covers` Inconclusive); VV:2333-2342 (empty successful collection) | L `array_fanout_freezes_and_orders_items`, `publication_references_resolve_to_attempts_and_statuses` | PASS (`d48630e2f`) |
| O4.2: duplicates keep distinct indices | same | VV:2878-2881 (`a` at 1 and 2), VV:2886-2896 | same | PASS |
| O4.3: non-array → zero calls, failure | same | VV:2922-2935, VV:2936-2943 | same | PASS |
| O4.4: over-limit → zero calls, failure, no truncation | same | VV:2922-2935, VV:2936-2943 | same | PASS |
| O4.5: reversed completion keeps order | same | VV:2882-2885, VV:2878-2881 | same | PASS |
| O4.6: failed child prevents successful aggregate | same | VV:2944-2955, VV:2956-2976 (ledger keeps items 0/2 succeeded, 1 failed) | same | PASS (`cf376e411`) |
| O4.7: frozen items persisted and reused | `ControlRecord::Items` | VV:2995, VV:3016; RJ:1678-1685, RJ:1730-1733 | L; J(logical_control_state_resumes_from_bifrost) | PASS |
| O5.1: true runs, false skips before binding | condition before bindings; the stylist mounts `steps.security.publications.account` | RJ:900-909 (a skipped stylist's reason names its condition, never the unbound account); RJ:915-918 (calls) | J(conditions_and_collections_preserve_missing_evidence); Python/TS/MCP mirrors rerun green on the changed fixture | PASS (`e42e88ee9`) |
| O5.2: null/missing/wrong-type/failed decision is error | comparison causes | RJ:865-878 (null, missing), RJ:886-892 (UNPLANNED: failed decision fails `security`), RJ:919-928 (reason text); VV:3740-3750 (missing, wrong type) | J(conditions…); L `condition_errors_and_admitted_failures_stay_failures` | PASS |
| O5.3: collections wait and keep skipped/failed/partial | collection mounts | VV:2329-2332 (`check:review-ok` before `action:consume`); RJ:935-950 (BOTH `failed` + `draft`, skipped collections) | L `publication_references_resolve_to_attempts_and_statuses`; J(conditions…) | PASS (`060e4bc20`) |
| O5.4: strict named report missing refuses consumer | strict publication selection; `hosted/workspace.rs` named mount | RJ:879-885 (`SILENT`: security succeeds without `account`, quote Failed with `published no \`account\``, no `QUOTE:` call); RJ:919-928 | J(conditions…) | PASS (`e42e88ee9`) |
| O5.5: no applicable requirement → inconclusive | decide | RJ:995-1007 | J(conditions…) | PASS |
| O6.1: one iteration per item at a time | engine repeat | VV:3107-3115, VV:3159-3165; RJ:1909-1913 | L `repeat_refines_one_iteration_at_a_time`; J(repeat_stops_or_exhausts_without_overlapping) | PASS |
| O6.2: previous null then value; zero-based index; output only in until | iteration scope | VV:3103-3106, VS:2810-2814; RJ:1308, RJ:1310-1313 | L; J(sequential_refinement_returns_final_admitted_report) | PASS |
| O6.3: until true stops; false continues | same | VV:3103-3115; RJ:1289-1296, RJ:1302-1306 | same | PASS |
| O6.4: exhaustion/invalid until/action failure cannot succeed | same | VV:3137-3154; RJ:1343-1385 | same | PASS |
| O6.5: step requirements settle before until | same | VV:3116-3126 (check `judged#k` before `refine#k+1`; stopping iteration judged) | L | PASS (`cf376e411`) |
| O6.6: repeat composes with foreach | `document-pipeline.yaml` `polish` (foreach items + repeat) | VV:3155-3165; RJ:1458-1490 `document_pipeline…` `polish` output in input order and per-item rounds `[0, 1]` | L; J(document_pipeline_uses_array_repeat_and_publications) | PASS (`8307a3f1d`) |
| O6.7: recovery restores iterations, no extra iteration/attempt/deadline reset | checkpoint iterations; claim `COALESCE(deadline_at)` | RJ:1734-1763, RJ:1847-1857, RJ:1869-1873, RJ:1909-1913 | J(logical_control_state…); J(repeat_stops…) | PASS |
| O6.8: repeat named selection reads final iteration | Scope selection | VS:2859; RJ:1329-1333 | L; J(sequential_refinement…) | PASS |
| O7.1: pending guard: action runs, content withheld | engine speculation | VV:3236-3243, VV:3274; RJ:1175-1203 | L `speculative_steps_withhold_content_until_admitted`; J(speculative_disposition_controls_consumer_admission) | PASS |
| O7.2: known-false guard makes no call | same | VV:3273; RJ:1180-1184 | same | PASS |
| O7.3: true admits, false discards, invalid blocks | same | VV:3302-3305; RJ:1199-1203 | same | PASS |
| O7.4: consumers never get excluded bytes; wait for action+decision | same | VV:3257-3272 (`dropped-consumer`, `blocked-consumer` not Succeeded, no output, never called); VV:3240-3243 (consumer waits for the decision); RJ:1199-1203 | L `speculative_steps_withhold_content_until_admitted`; J(speculative_disposition…) | PASS (`249e9f126`) |
| O7.5: action before/after guard both honoured | same | VV:3236-3239 (`extra` finishes before the guard); VV:3248-3256 (`late` finishes after the true guard, then admitted to `late-consumer`); VV:3303 (`late` Admitted) | L `speculative_steps_withhold_content_until_admitted` | PASS (`249e9f126`) |
| O7.6: recorded disposition final across crash; no resurrection | `Disposition` control; `attempted` invocations (`345648d74`) | VV:3308-3325 (a recorded disposition wins over a re-evaluated guard: stale result cannot resurrect); VV:3327-3349 (crash after decisions, a cancelled attempt kept, nothing reruns); VV:3351-3389 (crash after publication/completion and guard acceptance, before the decision commits: nothing reruns, decided the same way); RJ:1934-2062 (crash after publication while the discarded/blocked review is mid-cancel; the replica calls neither comparer nor extra, and the stalled worker resurrects nothing) | L `speculative_steps_withhold_content_until_admitted`; J(speculative_recovery_cannot_resurrect_content) | PASS |
| O7.7: excluded content inspectable with read grants | publication speculation projection; `verifier:read` authorization | RJ:1158-1166 (`verifier:read`-only key); RJ:1212-1235 (discarded and blocked accounts inspected); RJ:1236-1247 (disposition unchanged after inspection) | J(speculative_disposition_controls_consumer_admission) | PASS (`a17f1d5e3`) |
| O7.8: true guard + failed action visible as failed | engine | VV:3751-3766 | L `condition_errors…` | PASS |
| O9.1: active work bounded by shared process and per-execution ceilings; fanout never multiplies; concurrency only lowers | `HostedLimits::max_workspaces`, `limits.concurrency`, `foreach.concurrency`, Busy drain (`b4e532c90`) | VV:3478-3498; RJ:2170-2185 (2 workspaces across tenants) | L `run_and_fanout_bounds_only_lower_engine_capacity`, `capacity_refusal_lets_dispatched_work_settle`; J(admission_bounds_fanout_across_tenants) | PASS (scope corrected: shared and per-execution bounds only; J(admission_bounds_fanout_across_tenants) and the two L tests re-run: 1 and 2 passed) |
| O9.2: max_agent_invocations counts actions/checks/retries; never replenished | durable attempt accounting | VV:3592-3611, VV:3636-3646 (an Agent check spends the budget), VV:3678-3683 (recovery); RJ:2197-2201, RJ:2209-2218 | L `agent_budget_counts_retries_and_is_never_replenished`; J(admission…) | PASS (`cf376e411`) |
| O9.3: timeout_seconds lowers deadline; recovery keeps deadline; cancellation releases capacity | runner deadline; cancel notify (`1e3fbb617`) | RJ:2273-2291; RJ:1847-1857; RJ:2249-2252, RJ:2268-2271 | J(admission…); J(repeat_stops…) | PASS |
| O9.4: competing tenant progresses when shared capacity is released | claim-loop tenant rotation; Busy defer without spend | RJ:2182-2185, RJ:2209-2218, RJ:2268-2271 | J(admission…) | PASS (scope corrected: B may wait while capacity is occupied and completes once it is released; J(admission_bounds_fanout_across_tenants) re-run: 1 passed) |
| O9.5: acceptance-time limits snapshot | unchanged enqueue response | revision-22 amendment removes reporting only; enforcement stays | static packet review | RETIRED by explicit user approval, not an implementation PASS |
| O10: non-repository document pipeline | fixtures `document-pipeline.yaml`, `polisher.yaml`, `samples/clean-report.json` | RJ:1425-1618 (flagged escalation fails with its note; unflagged one is skipped with its false-condition reason, never called, mounted as an empty `skipped` collection, its requirement `not_applicable`); public mirrors of the flagged report (array, repeat, publications) at TS:599-693, PY:251-329, MCP:4581-4778; the unflagged skip is Rust-only (RJ:1588-1616), as proof 10 names the Rust journey | J(document_pipeline_uses_array_repeat_and_publications): 1 passed; TS/PY whole files 3 passed; MCP 3 passed | PASS |
| OL: control state resumes from Bifrost; incompatible state refused without dispatch | `EXECUTION_STATE_FORMAT` 3, controls | RJ:1716-1788 `logical_control_state_resumes_from_bifrost` (resume from Bifrost); `wyrd-spec` `state.rs:301-309` (formats 1-2, before controls, refused); `pg_durable_verifier_work.rs:3229-3234`, `:3243-3247` (reclaim refuses, no dispatch); VV:3021-3047 (recovered items beyond `max_items` fail the step and dispatch nothing) | J(logical_control_state…); `wyrd-spec` `only_the_current_format_decodes`; L `array_fanout_freezes_and_orders_items`; `pg_durable_verifier_work::state_format_changes_preserve_existing_custody` | PASS (`8c56958dd`) |
| OL.P: server recovery proof ownership | existing Rust `Desk::take_over`; existing SDK contract mirrors | Rust real-server journeys cover recovery: restart and resume from Bifrost (RJ:1695-1729, `logical_control_state_resumes_from_bifrost`), and `Desk::take_over` (RJ:1845-1857 repeat, RJ:1989-2008 speculative); OP.py/ts/mcp cover public contracts | [Lifecycle proof](#lifecycle-proof), [proof 6](#proof-6), [proof 8](#proof-8), [public closure](#public-surface-closure) | PASS (scope corrected: Rust `Desk::take_over` owns recovery proof; J(logical_control_state_resumes_from_bifrost), J(repeat_stops_or_exhausts_without_overlapping), J(speculative_recovery_cannot_resurrect_content) re-run: 3 passed; OP.py/ts/mcp unchanged and passing) |
| OP.py: Python mirror incl. cancellation | `sdks/wyrd-sdk-python/tests/integration/test_verifier_orchestration.py`, `conftest.py` `orchestration_turn` | PY:54-106 case table (incl. `SILENT`: quote fails with ``published no `account` ``), PY:134-185 | whole file: 3 passed | PASS |
| OP.ts: TS mirror incl. cancellation | `sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-orchestration.test.ts` | TS:298-448 (case table incl. `SILENT` at TS:347-356, quote reason asserted at TS:392-395) | whole file: 3 passed | PASS |
| OP.mcp: MCP mirror incl. cancellation | `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs` | MCP:4369-4553 (incl. `SILENT`, quote reason at MCP:4467-4475) | MCP `verifier_orchestration_` selectors: 3 passed | PASS |
| OP.http: HTTP/OpenAPI at existing route tests | `pg_openapi_contract.rs`; RJ `http` helper; existing cancel route test | `pg_openapi_contract.rs` L1732-1764 (schemas); RJ:982-992 (HTTP status and publications match SDK and CLI, every branch incl. `SILENT`); `pg_durable_verifier_work.rs` L2639-2683 (`POST …/cancel` answers 200 `cancelled`, reads back cancelled, an outsider is 403) | J(conditions_and_collections_preserve_missing_evidence); `wyrd-server --test integration -E test(=pg_openapi_contract::verification_contract_publishes_eleven_typed_operations_and_the_local_upload)`; `-E test(=pg_durable_verifier_work::cancellation_reads_back_from_state_after_the_queue_and_cards_are_gone)`: 1 passed | PASS (`d027ff8ab`) |
| OP.cli: CLI in-process status/publications/cancel | RJ `cli` helper | RJ:961-966, RJ:967-981, RJ:2242-2252 | J(conditions…), J(admission…) | PASS |
| O-L65: blocked IO; accepted references reused directly under admission/recovery | admission Busy defer; recovery from accepted state | RJ:2176-2185 (items blocked on held IO occupy the two shared workspaces and nothing starts beside them); RJ:2229-2271 (stalled `c0`/`c1` stay stalled until cancelled, and B progresses once capacity is released); RJ:2197-2201 (no pre-dispatch spend); VV:3678-3683 (no replenished budget); RJ:1721-1729, RJ:1778-1788 `logical_control_state…` (accepted publications reused, no section reviewer reruns) | J(admission…); J(logical_control_state…); L budget test | PASS (scope corrected: progress after capacity release, no tenant reservation; J(admission…), J(logical_control_state…) and L `agent_budget_counts_retries_and_is_never_replenished` re-run: all passed) |
| O-L66: lease renewal sustains permitted work; recovery keeps budgets | `leases.rs` (renewal, notify on cancel `1e3fbb617`) | `pg_verification_runtime.rs:4336` (`renewal_keeps_a_long_run_and_a_taken_token_cancels_it`: renewal sustains a run past its lease, a taken token cancels it); `pg_durable_verifier_work.rs:2145-2168` (a reclaim keeps the original deadline); VV:3678-3683 (recovery keeps the spent Agent budget); RJ:2242-2271 admission cancel (cancellation releases capacity at once) | the two server tests above (2 passed); L budget test; J(admission…) | PASS (existing seams; no new production code) |
| O-L35: waits release unrelated permits; queue/retry waits | `admit()` takes the run's bound before the shared permit; Busy defer (`b4e532c90`) | VV:3510-3539 (`waiting_blocks_release_unrelated_permits`; fails when `admit()` takes the shared permit first, mutation-checked); RJ:2176-2185 (a deferred item holds no workspace while waiting) | L `waiting_blocks_release_unrelated_permits`; J(admission…) | PASS (`c46d24e86`) |
| O-L33: strict names require successful admitted production (discarded/blocked speculative producer refuses named selection) | strict publication selection | VV:3202-3203 (`dropped-named`, `blocked-named`); VV:3257-3272 (never called, not Succeeded, no output); RJ:879-885 (`SILENT` producer without the name) | L `speculative_steps_withhold_content_until_admitted`; J(conditions…) | PASS (`c3cb31220`) |
| O-L136: inspection does not change downstream eligibility | same | RJ:1236-1247 | J(speculative_disposition_controls_consumer_admission) | PASS (`a17f1d5e3`) |
| O-L158: recovery under current permissions | shared Bifrost state foundation (TASK-001); control records in the same state | `pg_durable_verifier_work.rs:3725-3738, 3750-3785` (a revocation fences the live lease, and the restarted runtime records `principal_revoked` without computing); `pg_durable_verifier_work.rs:2145-2168` (unchanged bounds); RJ:1716-1788 `logical_control_state_resumes_from_bifrost` (unchanged identities and exact evidence, no settled work reruns) | `pg_durable_verifier_work::terminal_delivery_survives_every_commit_boundary` (1 passed); J(logical_control_state…) | PASS (existing seams) |
| O-L274: public mirrors of foreach/repeat/speculative authored controls | TS, Python, MCP mirrors over the same fixtures | TS:489-586 and PY:207-240 and MCP:4803-4929 (every speculative case, the called extra review, the consolidator manifest, retained dispositions; TS/PY a `verifier:read` inspection keeps them); TS:599-693, PY:251-329, MCP:4581-4778 (array `foreach`, per-item `repeat`, summarizer `repeat`, publications) | TS/PY whole files 3 passed each; MCP 3 passed | PASS |
| O-L264: refinement conserves iteration identity across interruption | proof 6, lifecycle | RJ:1734-1763, RJ:1869-1913 | J(repeat_stops…), J(logical_control_state…) | PASS |

### RED records

- Genuine RED before fix: a `when` over a missing path skipped rather than failed (O5.2); a wrong-type equality decided false (O5.2); a discarded/blocked speculative step lost the attempts it began (`invocations: []`, O7.6, J(speculative_recovery…)); a Busy refusal dropped dispatched siblings (`[("map",1,false)]`, VV `capacity_refusal_lets_dispatched_work_settle`, O9.1/O9.2); a cancelled run kept both workspaces and tenant B's execution stayed `pending` for 60s (O9.3, J(admission…)).
- Mutation RED (the server already met the contract): TS (expected style `[]`, expected cancel `completed`), MCP (quote `succeeded`), OpenAPI (`speculation` for `speculative`). Python RED: planner failed before the scripted turns existed.
- Process exception: the `cf376e411` assertions (O1.3, O1.4, O2.2, O4.6, O6.5, O9.2) were added after the checker's review and passed on first run against existing behavior; they have no missing-behavior RED.
- Process exception: the `d48630e2f` (O3.1, O4.1) `e42e88ee9` (O5.1, O5.4) `060e4bc20` (O5.3) `249e9f126` (O7.4, O7.5), `603398744` (O3.2) `a17f1d5e3` (O7.7, O-L136) `c3cb31220` (O-L33), `8307a3f1d` (O6.6), `8c56958dd` (OL), `d027ff8ab` (OP.http) and `c46d24e86` (O-L35, mutation-checked rather than RED), `7c85d599d` (O7.6 pre-decision crash), `eff4a5a51` (O10 unflagged) and `1f92a5800` (OP mirrors' `SILENT`) assertions passed on first run against existing behavior; no missing-behavior RED.
- Genuine RED (scenario 5): J(conditions_and_collections_preserve_missing_evidence) with the new wait assertion failed with `WYRD_SPEC_502_UPSTREAM_FAILURE` (`original_code: cancelled`) before `SdkVerificationCancelled` existed. The Python and TypeScript assertions went green on the same shared-client change without a separate RED run.
- Verification-only (scenario 6): `cancellation_writer_race_preserves_terminal_delivery` passed on first run against the existing owners; no production code changed, as the task directs.
- Process exception: most production code (`cfb6ce607`) preceded its tests; most engine and journey tests passed on first run, so they have no missing-behavior RED. The inspection speculation projection changed without a prior RED.

### Diagnoses

- **Proof 6 finished before the stall.** Symptom: passed in 11s with a 20s hold. Evidence: trace `serve task did not drain within the teardown budget; aborting`. Cause: `TEARDOWN_BUDGET` (2s) aborts the stalled worker. Fix site: test only; `STALLED` = 600s and the test claims non-overlap only while the stalled worker lives.
- **Speculative invocations dropped.** Symptom: discarded manifest `invocations: []`. Cause: the cancel/withhold path built the settlement from settled attempts only. Fix site: `blocks/engine.rs` `attempted`, the union of restored and begun attempts, used by drive cancel and recorded-disposition paths.
- **Busy dropped siblings.** Symptom: one item settled instead of two. Cause: returning `EngineError::Busy` dropped the `Tasks` set. Fix site: `engine.rs` `settled` drains running tasks via `walk.rs` `Tasks::drain`, used by `drive` and `members`.
- **Proof 9 order.** Symptom: `[(1,1),(0,1),(2,1)]`. Cause: publications list in commit order. Fix site: test helper `mapped` sorts.
- **Cancellation kept capacity.** Symptom: RJ:2262-2267 timed out, `last_status: pending`. Evidence: no runner claim rounds after the refusal (`claims.rs` `paused`); no `lease was taken` warning; renewal ticks at `lease / 6` = 100s (`leases.rs`, `mod.rs` default lease 10 min). Cause: a committed cancel only fenced Postgres, and the holder learned of it on its next renewal tick. An independent read-only diagnostician confirmed the cause and fix site; `wyrd-design.md` requires cancellation to stop computation immediately. Fix site: migration `20261108000100_verifier_execution_cancel_notify.sql` notifies `wyrd_verification_cancelled` on `running → cancelled`; `LeaseRenewal::run` listens through the existing `NotificationListener` and renews at once. Both producers are covered (`cancel`, `cancel_principal` revocation). The claim loop is unchanged. Regression: 9 `pg_durable_verifier_work`/`pg_verification_runtime` cancel and renewal tests pass.
- **Lifecycle.** An interrupted attempt is spent, never replenished, and runs as attempt 2 (RJ:1759-1763).

- **Sweep journey consolidator lookup.** Symptom: `fixed_sweeps_have_private_attempts` panicked `no entry found for key` at the consolidator manifest read. Evidence: the scripted `CONSOLIDATE:` reply names its read `c-accounts` (`c-{collection}`), and the test indexed `c-manifest`. Cause: a test key mismatch introduced in `89c3473c5`; a fresh diagnostician confirmed the test panicked at every commit since, so earlier PASS citations of this journey were wrong. Fix site: the test lookup (`603398744`). The other `c-manifest` users (`hosted_agent_tools.rs`) run their own script and are unaffected. All seven other Rust orchestration journeys were re-run by exact selector afterwards and pass.

- **CLI status lacked `result`.** Symptom: in the conditions journey, the CLI status of `SECURITY-ONLY` and then `BOTH` read `{"status":"completed","dispatches":[]}`. Evidence: `run_direct` returns after the fenced terminal selection and stages the result rows on the Scribe outbox without waiting (`components/verification/service.rs:616-638`), and `get_execution` omits `result` until those rows are visible (`verification/inputs.rs:409-452`). Cause: documented eventual consistency (`wyrd-spec` `verification/execution.rs:169-170`: "`completed` may briefly lack a visible result"). It started losing the race once the per-iteration HTTP reads added audit rows to the same outbox. A fresh diagnostician confirmed this. Fix site: the test waits with the documented `Execution::wait` (`desk.settled`) before the status reads (`d027ff8ab`). The server is unchanged.

- **Scribe contradiction poisoned a lane.** Symptom: in the whole TS and Python orchestration files, the speculative test got 503 "the verification execution state cannot be recorded now"; each test passed alone. Evidence: 19 `retained Scribe batch ID was reused with contradictory payload identity` lines. Cause (fresh read-only diagnostician): on cancel the worker's leased RunState and the delivery writer stage the same revision batch ID with different payloads. `write_group` (`scribe/shards.rs`) reads `memtable.rs` `retained_identity_rows` first, which returned `ScribeError::Internal` and failed the whole group. The outbox retries Internal as Unavailable, so the group retried forever and head-of-line blocked its lane. Fix site: `retained_identity_rows` returns `BatchConflict`, and `write_group` asks `contradicts_retained` before staging any slice and refuses only that append through `refuse_conflict`, so the group commits (`bifrost-design.md`: a contradiction fails closed for that batch). Callers checked: `retained_batch_rows` has the one production caller. RED: `scribe::shards::tests::contradictory_retained_batch_refuses_only_itself` failed with the Internal group error. GREEN: that test, `wal_closeout::contradictory_batch_retry_is_rejected_before_second_wal_append` (now asserts `BatchConflict`), every `scribe::` unit and pg test, the 9 Rust orchestration journeys, the two server cancel/delivery tests, and the TS and Python whole files.
- **Python DISCARD-EARLY unrecorded.** Symptom: the cancelled extra review never appeared in the receiver. Cause: `serving()` without a delay recorded only after a successful write, and the cancelled caller drops the connection. Fix site: `conftest.py` `serving` records in `finally`; the test waits on the receiver's condition for the called request.

### Mechanical deviations

- Proofs 6, 8 and 9 run in `wyrd-ai --test integration verifier_orchestration::*`, not `wyrd-testing --test server verification_runtime::*`. That target needs the real SDK, a bound replica and a second tenant. Commands are J(repeat_stops_or_exhausts_without_overlapping), J(speculative_recovery_cannot_resurrect_content) and J(admission_bounds_fanout_across_tenants).
- The Rust cancelled-wait assertion is in `conditions_and_collections_preserve_missing_evidence`, not the admission journey the cancelled-wait proof names: that journey's held `HELD` execution is the Rust case mirrored by the Python and TypeScript conditional journeys, and the admission journey cancels through the CLI. Same observation, same `wyrd-ai` target.
- The HTTP cancel assertion is the existing route test `pg_durable_verifier_work::cancellation_reads_back_from_state_after_the_queue_and_cards_are_gone`; no new HTTP test was added.

### Owner checks

Revision-22 remaining work (`c6ddfb417`, `43005c0f9`): `mise run fmt`, `lints`, `codegen:check`, `check:deps`, `check:tenant-isolation`, `py:lints`, `py:typecheck`, `ts:format:check`, `ts:lints`, `ts:typecheck`, `ts:napi:check` (after committing the regenerated `index.d.ts`/`index.d.cts`), `docs:generate` (no drift), `docs:check:commands`, `docs:linkcheck`: all exit 0. `git diff --check` is clean.

Historical implementation results, before the revision-22 remaining work:

`mise run fmt`, `lints`, `codegen:check`, `check:deps`, `check:tenant-isolation`, `py:lints`, `py:typecheck`, `ts:format:check`, `ts:lints`, `ts:typecheck`, `ts:napi:check`: all exit 0. The queue-geometry test `runnable_scan_walks_the_claim_indexes_however_large_the_backlog` passes. `git diff --check` is clean. The tracked diff audit leaves only the pre-existing `.agents/skills/*` edits and untracked `changes/` uncommitted. Non-goals stayed excluded: no nested graphs, streams, subscriptions, quorum or alternate engine.

### Open and risks

- The revision-22 amendment resolves the former scope blockers O9.1, O9.4,
  O9.5, O-L65 and OL.P without new quota, limits-response or SDK harness
  machinery; their cited proofs were re-run for this candidate. O9.5 is
  retired by user approval and is not an implementation PASS.
- Fresh checkers flagged the bystander's lane, public report inspection, the wait-timeout proof and stale citations; each is resolved above (`50b063e4d`, `384a0def7`).
- Scenario 6 needed no production change: the losing writer's frame is refused
  on its own, and a refused delivery is redelivered on the newer revision.
