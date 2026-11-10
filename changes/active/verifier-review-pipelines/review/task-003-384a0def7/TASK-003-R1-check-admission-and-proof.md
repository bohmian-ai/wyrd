---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-verifier-review-pipelines
spec_revision: 22
parent_task: TASK-003
candidate: 384a0def73ed6eac4041dc1e428dc30e981514fe
base: e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355
remediates: [FIND-003-1, FIND-003-2, FIND-003-3, FIND-003-4]
depends_on: []
---

# Preserve check admission and settlement, and finish required proof

## Outcome and authority

Agent checks must obey the same capacity and recovery guarantees as actions.
A check waiting for capacity must not spend its invocation budget; a capacity
refusal from one check must let its already-dispatched siblings finish and
record their results. Complete the existing public speculative-order proof and
batch the mandatory rustdoc corrections with these changes.

For example, a one-slot execution running a code check can have an Agent check
waiting beside it. Restarting during that wait must leave the Agent's one-call
budget available. A different case has an already-running Agent check beside a
capacity-refused check: deferral must preserve the running check's accepted
settlement instead of cancelling it and charging another attempt on recovery.

This is direct remediation for `$wyrd-implement`, not a new design plan or spec
approval round. Authority is the [approved baseline and revision-22 amendment](../../spec.md),
[original TASK-003](../../tasks/TASK-003-generic-orchestration.md),
[shared public contracts](../../public-contracts.md), and
[durability supplement](../../execution-state-and-durability.md).
The parent spec remains draft only for unrelated TASK-007 decisions. Review
subject is the complete base-to-candidate range above; the validated findings
and selected correction boundaries are in [findings-validation.md](findings-validation.md).
The original [verdict](verdict.md) and discovery reports remain acceptance context.

## Diagnoses and selected corrections

### FIND-003-1 — A waiting check spends before it can run

`VerificationEngine::requirement` in
`crates/vala/vala-verify/src/blocks/engine.rs:1490–1514` spends the Agent budget
and accepts a begun invocation before `judge` acquires the execution/shared
permits at line 1660. With one code check occupying a one-slot execution and
one Agent check waiting, interruption leaves a durable Agent attempt although
its executor was never called. Recovery conserves that attempt through
`with_checkpoints` and `agent_attempts`; with a one-invocation ceiling the Agent
can become budget-exhausted without ever running. This violates Scenario 1's
no-pre-dispatch-spend requirement and REQ-008's cumulative invocation accounting.
The existing budget test exercises an interrupted dispatched action, not a
check waiting for capacity.

**Selected correction.** Reuse the existing check/action admission owner: obtain
check admission before spending or recording its invocation, retain admission
through execution/settlement, and remove the later duplicate acquisition.
There must be one acquisition, preserving run-before-shared permit ordering.
Both whole-Verifier and per-item/per-iteration checks use this shared requirement
boundary; fixing one caller alone leaves the other defective. Preserve
settled/missing-evidence fast paths, non-Agent checks, Agent-only charging,
acknowledged begin before external dispatch, Busy forgetting and genuinely
interrupted-attempt accounting.

The invalid record originates before storage. Refunding interrupted records,
adding dispatched markers or filtering recovery would weaken valid accounting
and introduce unnecessary persistence machinery. One local ordering correction
and a controlled checkpoint regression close the gap.

### FIND-003-2 — One Busy check drops running siblings

After a member's action succeeds, `instance_requirements` owns its concurrent
check futures. At `engine.rs:1441–1444`, it propagates `result?` directly. A
capacity-refused check forgets its own attempt, but the return drops the other
checks' futures and leaves their previously accepted begin records unsettled.
Outer `members`/`drive` drain on Busy, but cannot recover the inner futures once
dropped. Recovery can therefore repeat and charge an Agent check whose own
capacity never failed, or exhaust its budget. This violates the existing Busy
preservation contract and task admission/recovery obligations.

**Selected correction.** Apply the existing Busy-aware `settled` handling where
the inner check owner consumes completions, so it retains and settles siblings
before propagating the refusal. The other two `Tasks` consumers already use
this owner. Preserve ordinary non-Busy errors and outer cancellation/deadline
behavior; do not broaden draining to new error families or add a task registry,
store or retry policy. Moving admission for FIND-003-1 does not fix this distinct
future-lifetime defect. One existing-owner correction and a focused recovery
regression suffice.

### FIND-003-3 — Required rustdoc is missing or attached to the wrong item

Root AGENTS and `agent_docs/rust.md` make changed-item rustdoc, including panic
preconditions on private/test helpers, a merge gate. This finding does not claim
that supported inputs currently panic. The validated locations are:

- `engine.rs`: `members` (1229), `member` (1287), `instance_requirements` (1422),
  `ready` (1929), `missing_requirements` (1990), `Node::id` (2136): direct
  declaration indexing without the corresponding panic/precondition docs.
- `crates/wyrd-spec/src/card/verifier/validate.rs:2648`, `controlled`: its object
  `expect` precondition is undocumented.
- `crates/vala/vala-bifrost-redux/src/scribe/shards.rs:4472–4500`: the inserted
  `contradicts_retained` received the old slice-popping rustdoc, leaving
  `next_prepared_slice` undocumented and describing the predicate incorrectly.
- `sdks/wyrd-sdk-rust/tests/integration/verifier_orchestration.rs`: `section_turns`
  (2079) and `mapped` (2096) propagate fixture/read panics without documentation.

**Selected correction.** Restore documentation attachment and add concise
precondition/panic sections matching the actual bodies and nearby patterns.
Do not add defensive runtime checks, change signatures, refactor the helpers or
write tests for this documentation-only correction. Batch it with this task;
there is no separate style-only round.

### FIND-003-4 — Public speculative order is inferred from delays

Rust's speculative journey assigns one/three-second provider delays to make the
action or guard finish first (`verifier_orchestration.rs:89–94,237–273`). Its two
ADMIT cases have identical final assertions at 1103–1251 and never observe the
relative order. Runtime/tool/checkpoint work can consume the delay margin, so
both cases can pass while exercising the same order. Other excluded-content
cases can flake when their assumed order reverses. Python, TypeScript and MCP
use equivalent delay-based fixtures. The deterministic engine test is useful
local proof, but does not replace the separately required public proof-7 journey.

**Selected correction.** Keep the existing public journeys, declarations,
providers and outcome/read-authority assertions. In the Rust proof, explicitly
hold/release the relevant provider replies and observe bounded accepted
invocation milestones through the existing public invocation inspection:

- Action first: keep comparison pending until extra action success is accepted,
  then release comparison and observe admission/exclusion.
- Decision first: observe the extra provider request, hold its answer, release
  comparison and observe the comparison accepted while extra still runs. For
  discard, observe the recorded disposition before releasing extra's cancelled
  reply. For admit, the engine records the admission only once the admitted step
  settles, so observe extra settle `admitted` after its answer is released.
- Known false: hold bootstrap eligibility until comparison is accepted, then
  release it and assert no extra call.

Assert these milestones as well as existing final results. Preserve the mirrors'
required cases, controls, statuses and manifests. Make their response gating
deterministic only where needed for those existing observations; do not demand
a second full timing-proof matrix on every language.

Use existing public `Execution::invocations` and its Python/TS/MCP projections,
which expose accepted action settlement/speculation, rather than a production
hook. The TS local HTTP receiver currently takes a synchronous callback; a small
fixture-only async response adaptation may be necessary. Implementation owns
the smallest appropriate local synchronization. Do not add server lifecycle
controls, a generalized orchestration harness, dependencies or larger delays.
Unrelated simulated provider latency, timeout and deliberately stalled-upstream
stimuli remain legitimate. A new post-hoc order assertion beside the same
uncontrolled delays alone is insufficient under the deterministic test rule.

## Preserved behavior and non-goals

Keep one engine, Scope, checkpoint store and retry/admission authority. Preserve
checkpoint-before-external-dispatch, accepted settlements and File custody,
original deadlines, cancellation, real interrupted work budgets, current grants,
tenant isolation, speculative finality and Scribe per-append conflict isolation.
The existing accepted rare old-worker frozen-revision overlap remains unchanged.
No migration, persisted format, new quota/reservation, limits response,
authorization policy, production API or dependency change is required.
Python/TypeScript still do not stop workers, expire leases or start replicas.
Do not manufacture historical RED or rewrite the recorded original TDD exceptions.
If the minimal correction actually requires changing any approved boundary,
stop and report the evidence instead of inventing that decision.

## Acceptance and proof

| Finding | Required observation |
|---|---|
| FIND-003-1 | A capacity-waiting Agent check has no accepted invocation and no executor call. After interruption/recovery with one Agent invocation available, it runs exactly once. Dispatched interrupted work still counts, and permit ordering remains run-before-shared. |
| FIND-003-2 | Busy is not propagated until a dispatched sibling is allowed to settle and its settlement is accepted. The refused check leaves no retained attempt. Recovery invokes only the refused check and does not charge the settled Agent again. |
| FIND-003-3 | All identified rustdoc is attached to its correct item and describes actual panic/precondition behavior; production/test behavior and signatures are unchanged. |
| FIND-003-4 | Rust's public journey observes the declared accepted milestones for both admitted orders, completed/in-flight exclusion and known false, with original retained inspection and final outcome assertions. Changed mirrors retain all required contract/case coverage without duplicate timing matrices or server fault controls. |

## Focused verification

New engine regressions below are **planned names** in the confirmed existing
`vala-verify --lib` target, `blocks::engine::tests`. Confirm actual names/nonzero
selectors after adding them, before RED. Both check-lifecycle bugs require
meaningful missing-behavior RED and GREEN evidence. The existing scripted
executor/checkpoint owner supports controlled capacity, accepted records and
recovery; no new provider/server harness is required for those local gaps.
The ordering-fixture correction applies the same missing-proof discipline;
retain honest verification-only proof when underlying behavior already works.
Documentation has static proof and requires no manufactured RED.

### Waiting-check regression — planned

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::waiting_checks_spend_only_after_admission)'
```

### Nested Busy regression — planned

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::capacity_refusal_lets_dispatched_checks_settle)'
```

### Existing accounting, Busy and permit regressions

Names confirmed from current source; run each exact selector.

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::agent_budget_counts_retries_and_is_never_replenished)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::capacity_refusal_lets_dispatched_work_settle)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::waiting_blocks_release_unrelated_permits)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::speculative_steps_withhold_content_until_admitted)'
```

### Public speculative proof — Rust required; changed mirrors only

Use existing [shared setup](../../tasks/verification.md#existing-targets-and-setup),
Postgres wrapper and migrations; do not start overlapping Cargo commands.
These existing selectors were confirmed from source and their current owners.
Run Rust for the required public ordering correction; run Python/TS/MCP only
when their fixtures or assertions change. Do not run full journey aggregates.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verifier_orchestration::speculative_disposition_controls_consumer_admission)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_orchestration.py::test_speculative_disposition_controls_consumer_admission'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-orchestration.test.ts -t "^speculative_disposition_controls_consumer_admission$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all --test-threads=1 -E "test(=verification::pg_tests::verifier_orchestration_speculative_disposition_controls_consumer_admission)"'
```

### Owner checks and evidence

Run `mise run fmt` and `mise run lints` for changed Rust owners, then applicable
Python/TypeScript checks from [shared owner checks](../../tasks/verification.md#owner-checks)
only when those owners change. Inspect corrected rustdoc against every listed
body. No schema/codegen/tenancy gate expansion is justified by test/document
changes alone; add only checks required by the actual write set. Finish with
`git diff --check` and inspect the remediation diff for unintended changes.
Record exact selections, assertions, RED/GREEN or static proof and commands in
this task's execution evidence. Preserve the immutable original review and task;
route the cumulative candidate back to `$wyrd-task-review` for closure and
changed-path review. Full integrated surface/platform suites run at change
review, not as a new remediation requirement.

## Obligations

| ID | Source | Planned assertion |
|---|---|---|
| R1.1a | L171 | A capacity-waiting Agent check has no accepted invocation record and no executor call at interruption. |
| R1.1b | L171 | After recovery with one Agent invocation available, the waiting Agent check runs exactly once and judges. |
| R1.1c | L171 | Dispatched interrupted work still counts (the interrupted code check's record is retained; the existing interrupted-action budget assertions stay green). |
| R1.1d | L171, L59 | One admission acquisition, run permit before shared permit (`admit` reused; `waiting_blocks_release_unrelated_permits` stays green). |
| R1.2a | L172 | Busy from a refused step check is not returned while a dispatched sibling check is still running. |
| R1.2b | L172 | Before Busy returns, the sibling's settlement is accepted and the refused check leaves no retained attempt. |
| R1.2c | L172 | Recovery invokes only the refused check and does not charge the settled Agent again (one-invocation ceiling). |
| R1.3 | L173, L97-106 | Rustdoc attached to the right item with actual panic/precondition text for all ten listed items; no behavior or signature change. |
| R1.4a | L174, L130-131 | Rust public journey, action-first ADMIT: comparison held until extra's success is accepted in public invocations, then released; admission observed. |
| R1.4b | L174, L132-136 | Rust, decision-first: extra's provider request observed and held, comparison released and accepted while extra runs. Discard: disposition recorded before extra's reply is released. Admit: extra settles `admitted` after release (the admission is recorded at settlement). |
| R1.4c | L174 | Rust, completed and in-flight exclusion cases observe their milestones. |
| R1.4d | L174, L135-136 | Rust, known false: bootstrap held until comparison accepted, then released; no extra call. |
| R1.4e | L174 | Existing retained-inspection and final-outcome assertions are kept. |
| R1.4f | L174, L139-141 | Changed mirrors keep all required cases without duplicate timing matrices or server fault controls. |
| R1.5 | L228-233 | fmt, lints, `git diff --check`, remediation diff audit. |

## Execution evidence

Remediation range `384a0def7..b54624d86` (6 commits): `9c7bf4e13` (FIND-003-1/2),
`b084482d9` (FIND-003-3), `f04384dbf` Rust, `464f90279` MCP, `48be2b52a` Python,
`b54624d86` TypeScript (FIND-003-4), `f74a57634` (R1.1c checker gap). The cumulative candidate for task review is
`e6fe5fbdd..b54624d86`.

Abbreviations: ENG = `crates/vala/vala-verify/src/blocks/engine.rs`, ET =
`crates/vala/vala-verify/src/blocks/engine/tests.rs`, RJ =
`sdks/wyrd-sdk-rust/tests/integration/verifier_orchestration.rs`, MCP =
`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs`, PY =
`sdks/wyrd-sdk-python/tests/integration/test_verifier_orchestration.py`, TS =
`sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-orchestration.test.ts`, GC =
`crates/wyrd/wyrd-testing/src/gateway_capture.rs`.

### Owner and gap (reuse)

- FIND-003-1: the existing `admit` (ENG:494) is now acquired once in the prepared
  requirement future (ENG:1507) before `spend`/`begin`; `judge` no longer acquires
  (the only remaining acquisitions are ENG:1507 and the action's ENG:1579). No marker,
  refund or recovery filter.
- FIND-003-2: `instance_requirements` consumes completions through the existing
  Busy-aware `settled` (ENG:1455), like `members` and `drive`.
- FIND-003-4: wiremock answers every request on one current-thread runtime
  (`wiremock-0.6.5/src/mock_server/bare_server.rs:75-83`) after a fixed delay, so it
  cannot wait for a test event, and blocking a responder stalls all upstream traffic.
  The existing sibling `PacedUpstream` (GC) exists for the same limitation.
  `HeldUpstream` (GC:579) is an axum forwarder in front of the scripted mock. It uses
  axum, reqwest and wiremock, which `wyrd-testing` already depends on, so no
  dependency was added. It is shared by the Rust SDK and MCP journeys. Python reuses
  its threaded local model with `threading.Event`, the existing `HOLD_ARRIVED`
  pattern. TypeScript's `Receiver.start` now accepts a promised reply, the fixture-only
  adaptation the task anticipated. Milestones read existing public invocation
  listings (`Execution::invocations`, `verification.list_invocations`, Python/TS
  `Execution.invocations`). There is no production hook and no server fault control.

### RED

- `blocks::engine::tests::waiting_checks_spend_only_after_admission`, before the fix:
  `assertion failed: only the dispatched check is recorded; left:
  [("judged", 1, false), ("slow", 1, false)] right: [("slow", 1, false)]`. The waiting
  Agent check was recorded before admission.
- `blocks::engine::tests::capacity_refusal_lets_dispatched_checks_settle`, before the
  fix: `the refusal waits for the running check`. Busy returned while the sibling was
  still held.
- After the fix, both assertions passed. A later outcome assertion failed only because
  a hosted pass needs a reachable citation (`passed without a citation of reachable
  original evidence`). The scripted verdicts now cite `/text`; the behavioural
  assertions are unchanged.
- FIND-003-4: not missing behaviour. The production speculation already worked, so
  there is no product RED; this is verification-only proof. The first GREEN attempt
  surfaced two facts that are now asserted:
  - Invocations project `speculation: none` until a disposition is recorded, never
    `pending`.
  - An admitting decision for an in-flight speculative step records its disposition
    only once that step settles (ENG `drive`, the
    `Some(Done::Step(done)) if done.speculation == Pending` /
    `None if … !speculation.admits()` arms). Only a discard records while the step
    runs, because it cancels the step.
- FIND-003-3: documentation only, static proof, no RED.

### Mechanical deviations

- R1.4b, decision-first ADMIT: the task asks to "observe recorded disposition before
  allowing completion". The approved engine semantics above never record an admission
  before the admitted step settles.
  - The Rust milestone is therefore: the extra request arrived and its answer is held;
    the comparison is accepted (`Action{succeeded}`) while extra is still `started`
    with nothing recorded; extra is then released and settles `admitted`.
  - DISCARD-EARLY observes the recorded `discarded` disposition while extra is still
    `started`, before its held reply is released.
  - This is the observable order the contract needs; no approved behaviour changed.
- RJ waits on submit plus `Desk::settled` (the public wait) instead of `verify`, so
  milestones can be observed while the execution runs.
- `HELD` stays in RJ for the admission journey's section latency, an unrelated
  legitimate stimulus, with its doc narrowed to that use. `BRIEF` and the speculative
  `HELD`, `HELD_REPLY`/`BRIEF_REPLY`, `HELD_SECONDS`/`BRIEF_SECONDS` and
  `HELD_MS`/`BRIEF_MS` are removed.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| R1.1a waiting Agent check has no record and no call | ENG:1507 admit before spend/begin; `judge` no longer acquires | ET:3597 (`calls == ["check:slow"]`), ET:3602 (ledger only `slow` unsettled) | `waiting_checks_spend_only_after_admission` RED then PASS | PASS |
| R1.1b recovery with one Agent invocation runs it exactly once | same | ET:3619 (`["check:slow","check:judged"]`), ET:3621 (judged `passed`, not BUDGET_SPENT), ET:3629 (`judged` attempt 1 settled) | same | PASS |
| R1.1c dispatched interrupted work still counts | unchanged `recorded`/`agent_attempts` | ET:3629 (`slow` keeps interrupted attempt 1); ET:3689 and ET:3693 in `interrupted_agent_checks_keep_their_invocation` (an Agent check interrupted after admission leaves recovery with no call and `BUDGET_SPENT`); existing interrupted-action budget section | selectors PASS (new test is characterization: the behaviour already held, so there is no RED) | PASS |
| R1.1d one acquisition, run before shared | `admit` ENG:494 reused at ENG:1507; acquisitions only at ENG:1507/1579 | `waiting_blocks_release_unrelated_permits` (existing) | PASS | PASS |
| R1.2a Busy waits while a sibling runs | ENG:1455 `settled` | ET:3687 (5 s timeout elapses with the run still pending while `running` is held) | `capacity_refusal_lets_dispatched_checks_settle` RED then PASS | PASS |
| R1.2b sibling settlement accepted, refused attempt absent | same | ET:3702 (`running` 1 settled, `work` settled, no `refused`) | same | PASS |
| R1.2c recovery invokes only the refused check, no re-charge under a one-invocation cap | same | ET:3719 (`["check:refused"]`), ET:3722 (both `passed`) | same | PASS |
| R1.3 rustdoc attached and accurate | ENG `members`/`member`/`instance_requirements`/`ready`/`missing_requirements`/`Node::id` Panics sections; `validate.rs` `controlled` Panics; `shards.rs` slice-pop doc moved back onto `next_prepared_slice`, `contradicts_retained` keeps only its own contract; RJ `section_turns`/`mapped` Panics | static inspection of each body against its doc; diff `b084482d9` is doc-only | `mise run fmt`, `mise run lints` clean | PASS |
| R1.4a action-first ADMIT/DISCARD/BLOCK: comparison held until extra success accepted | RJ `Race::ActionFirst`; GC `HeldUpstream` | RJ:1248 (extra `Action{succeeded}`, `speculation none` before compare is released) | RJ selector PASS ×2 (13.9 s, 13.8 s) | PASS |
| R1.4b decision-first, reworded with user approval | RJ `Race::DecisionFirst` | `stalled_in("EXTRA:")` (request held); RJ:1262 (comparison accepted while extra held); RJ:1279 (extra `started`, with `discarded` recorded for DISCARD-EARLY and nothing recorded yet for ADMIT-EARLY); RJ:1307/1313 and the retained `[Admitted]` listing (ADMIT-EARLY settles admitted) | RJ selector PASS | PASS |
| R1.4c completed and in-flight exclusion | DISCARD-LATE action-first, DISCARD-EARLY decision-first | RJ:1248 / RJ:1279, plus retained `[Discarded]` vs `[]` | same | PASS |
| R1.4d known false: bootstrap held until comparison accepted, no extra call | RJ `Race::Known` | RJ:1291 (compare accepted while bootstrap held), RJ:1295 (no EXTRA request yet), RJ:1313 (`called: false`) | same | PASS |
| R1.4e retained inspection and final outcomes kept | unchanged assertions | RJ:1307 (status), RJ:1313 (called), manifest/entries, retained listing, `verifier:read` inspection, RJ:1377 (inspection leaves disposition) | same | PASS |
| R1.4f mirrors keep all cases, ordered without delays, no duplicate timing matrix or fault controls | MCP `Race` + `reached` MCP:4307 on `HeldUpstream`; PY `hold`/`reached` (PY:225/239) + conftest `HOLDS`; TS `hold`/`reached` (TS:580) + promised `Receiver` reply | MCP:5012, PY:316, TS:652 plus each mirror's existing status/called/manifest/inspection assertions; mirrors only wait for milestones and don't re-assert them | MCP selector PASS (11.7 s); MCP desk siblings `conditions_and_collections…`, `document_pipeline…` PASS; PY selector PASS (13.9 s); TS selector PASS, and the whole TS file 3/3 PASS | PASS |
| R1.5 owner checks and audit | — | — | `mise run fmt` 0, `mise run lints` 0, `py:lints` 0, `py:typecheck` 0, `ts:lints` 0, `ts:typecheck` 0, `ts:format:check` clean, `git diff --check 384a0def7..HEAD` clean; diff touches only the 13 listed files | PASS |

### Commands

- `mise exec -- cargo nextest run --locked -p vala-verify --lib -E` with each of the
  six exact selectors from *Focused verification*: 6/6 PASS. The whole `vala-verify
  --lib` run: 72/72 PASS.
- The four *Public speculative proof* commands, exactly as listed: Rust, MCP, Python
  and TypeScript each 1/1 PASS.
- Extra checks, not required, run only to cover fixture reach:
  - MCP `verifier_orchestration_conditions_and_collections_preserve_missing_evidence`
    and `verifier_orchestration_document_pipeline_uses_array_repeat_and_publications`:
    2/2.
  - The whole `verifier-orchestration.test.ts` file: 3/3.

### Non-goals and limits

- No migration, persisted format, production API, limits response, authorization or
  dependency change.
- Python and TypeScript still do not stop workers, expire leases or start replicas.
- Other Rust SDK journeys keep the plain wiremock upstream. Only the speculative
  journey goes through `HeldUpstream`. MCP's `OrchestrationDesk` routes all three of
  its journeys through it, and all three pass.
- Material risk: `HeldUpstream` withholds answers indefinitely while a hold is set.
  Every journey releases all holds before it waits for the final result: Rust in its
  flow, Python and TypeScript in `finally`. A failed milestone in Rust or MCP fails
  the test, and dropping the upstream closes held connections.

### Checker result

A fresh read-only checker received only the task file, the ledger and the diff. It
named two obligations:
- **R1.1c:** only a code check was interrupted. Closed by
  `interrupted_agent_checks_keep_their_invocation` (`f74a57634`).
- **R1.4b, ADMIT-EARLY:** as first worded, the obligation asked for an admission
  recorded before extra completes. The approved engine never records one: `drive` at
  ENG:902-906 records an admission only after the admitted step settles. With the
  user's approval, the obligation (L132-136 and the ledger row) was reworded to what
  the engine can show. The existing assertions prove the reworded obligation, and no
  engine behaviour changed.

Status: IMPLEMENTED. Every obligation is PASS.
