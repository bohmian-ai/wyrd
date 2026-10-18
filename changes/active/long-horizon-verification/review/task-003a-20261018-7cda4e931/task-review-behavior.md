# TASK-003a behavior acceptance review

**Result: FAIL.** Immutable range: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5...7cda4e93159ab328de2a74068d5e1978a2590818`; candidate HEAD confirmed at `7cda4e93159ab328de2a74068d5e1978a2590818`. This is the independent behavior reviewer. Source was not changed; no superseded assessment packet was read.

## Findings

### B-01 — INCORRECT — refused handoff consumes the only scheduling item

- **Location:** `crates/wyrd/wyrd-server/src/verification/scheduler.rs:373`, `:387`.
- **Obligation:** TASK-003a locked decision 7 explicitly says query/SQL/**refusal**/degraded/truncated failure rolls back and ends that tenant's pass; REQ-004 consumes work only after successful enqueue or identical-snapshot handoff. A1/A11.
- **Producer → consumer:** Scribe ACK produces a pending row through `ScribeSink::schedule_groups` → `VerifierExecutionQueue::record_group_activity`; scheduler locks it and `GroupPoller::poll` returns the queue outcome. `enqueue_group` (`verifier_executions.rs:1827`) returns `EnqueueOutcome::Refused` when `resolve_binding` finds unavailable/not-ready Verifier or unavailable subject. `hand_off_groups` treats every `Ok(outcome)` as a completed handoff, deletes the pending row and commits. There is no execution to own the rejected snapshot and, by design, no historical rescan to recreate the notification. Restoring readiness without new source activity cannot produce its judgment.
- **Sibling tracing:** ordinary cron `schedule_due` intentionally skips a refused occurrence and advances its cursor; manual enqueue returns refusal to its caller. Neither owns this pending-group retry contract. Snapshot query refusals already take the rollback path in `hand_off_groups`; SQL errors likewise roll back. The defect is the group's successful-Result/refused-outcome branch, not the shared queue's refusal rules.
- **Smallest correction:** in `hand_off_groups`, distinguish `Refused` from `Enqueued`/`AlreadyEnqueued`, log it, leave the transaction uncommitted and end that tenant's pass. Do not change cron skip behavior, add retry machinery, or weaken readiness. Add one focused server integration case: make an existing source-backed group's target temporarily unavailable, run a due pass, assert its pending row survives and no execution appears, restore eligibility, then assert a later pass hands off the same work. The Implementation Evidence's decision to consume Refused outcomes contradicts the approved instruction rather than authorizing a replacement.

### B-02 — INCORRECT / DRIFT — pending SQL silently shortens valid after durations

- **Location:** `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:195`, `:208`.
- **Obligation:** REQ-004 and task locked decisions 1/2 require the existing positive `TriggerDuration` and `due_at = statement_timestamp() + binding.after`, without a new authored/runtime budget. A1.
- **Producer → consumer:** `TriggerDuration::parse` (`crates/wyrd-spec/src/card/trigger.rs:92`) accepts up to nine digits in seconds/minutes/hours/days and preserves that exact authored duration. Registration freezes it into `GroupEndSettings`. The new SQL uses `LEAST(..., 3_155_760_000)` so `after: 40000d`, a representable roughly 110-year PostgreSQL interval, becomes 36,525 days. The stored Trigger and readback still say 40000d, but the scheduler can execute earlier than the declared quiet period.
- **Sibling tracing:** the cap is introduced only in `record_group_activity`; Trigger schema, parser, Card projection, and SDK declarations expose no corresponding maximum. It is not an existing `TriggerDuration` restriction. All grouped source input/result writers share this one wrong due-time producer.
- **Smallest correction:** remove the arbitrary 100-year clamp and its argument/constant, preserving exact representable authored durations. Keep out-of-range database arithmetic as an explicit failure rather than silently scheduling early; do not add an undocumented alternate duration policy. Add an exact SQL test asserting `due_at - statement_timestamp()` equals a valid duration beyond the removed cap, alongside an ordinary short-duration case. If limiting the public duration domain is desired instead, that requires an explicitly agreed contract decision.

### B-03 — MISSING — required shared conversation across actual language adapters

- **Location:** `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1335`; sibling `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:470`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:521`.
- **Obligation:** original task Scenario 1 explicitly requires at least one real shared conversation populated by Rust, Python and TypeScript under one server/tenant/owner/group. Spec AC-001 and the journey preamble require mixed casts. A1/A7/A10.
- **Evidence:** Rust C1 creates `Assistant::start`, emits four Rust verifies, one Rust submit and one HTTP enqueue. Python and TS C1 each use their own runtime/server fixture and make their own six-turn conversation. None invokes the other language adapters against that same server/owner/group. These are useful parity journeys, but their passing counts do not establish cross-runtime composition.
- **Consequence:** a required acceptance story is unproved: the combined pinned snapshot of evidence emitted by all three SDKs. This is a coverage finding, not a claim that production cross-runtime grouping is broken.
- **Smallest correction:** extend one existing support journey to pass the existing server URL/credentials/bundle and common group to the other two real runtime adapters, then inspect the one complete six-fact snapshot. Reuse current SDK support; add no second server/test framework or production mechanism. Preserve the separate-runtime negative tests.

### B-04 — MISSING — C13's cron isolation assertion schedules no cron work

- **Location:** `crates/wyrd/wyrd-server/tests/integration/pg_verification_groups.rs:699`, `:703`.
- **Obligation:** task C13 specifically requires a blocked group query not to prevent ordinary cron work **being scheduled**, inspecting queue state and query invocations, not timing alone. A11/A10.
- **Evidence:** `Support::start` registers only `support.yaml`, whose bindings are one ordinary `turn-check` and one `group_end` conversation Verifier; there is no schedule binding. The final block waits for the group schema stall, calls `batched.pass`, and only asserts that its `Result` is `Ok`. It never asserts a cron tick/execution and the due cron list is empty. Calling `group_pass` and `pass` manually in `join!` also does not itself test `VerificationScheduler::run`'s loop composition.
- **Consequence:** C13 passes even if no ordinary cron occurrence is admitted during the blocked snapshot. The production `tokio::join!` loops look separate, but the required regression proof is vacuous.
- **Smallest correction:** register/use an existing scheduled fixture, activate it, make a real occurrence due, then while the group read is stalled assert that the cron occurrence's execution is committed and readable. Use the actual scheduler loop when proving loop independence; reuse existing fault/harness controls. Also count or fault actual query invocations when asserting the no-read/full-capacity cases; an empty handoff vector alone cannot prove no Bifrost query occurred.

### B-05 — MISSING — Python/TS pressure recovery does not read back the retried eligible group

- **Location:** `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:1031`, `:1051`; sibling `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:1066`, `:1084`.
- **Obligation:** Scenario 7/C11 requires grouped pressure/refusal, retry after drain and readback of the resulting group execution through each shipped SDK. A8/A10/A11; AGENTS §11 runtime-owned journeys.
- **Evidence:** Python pressures/retries `assistant`'s `answer-check` with group `staged`, which has no group-end binding. Only afterwards it registers the unrelated support group binding, creates a new random group and verifies a fresh `turn-check`. TS similarly retries the original assistant call, then appends a fresh support submission. Their group readback does not contain an execution that was refused and retried under pressure. Rust C11 correctly establishes the support binding before pressure and retries its exact refused grouped verify; it is the reusable sibling pattern, not part of this defect.
- **Consequence:** these runtime journeys prove a grouped keyword survives refusal and an independent group works later, but do not prove the required bound group's retry/drain/pending/snapshot seam for Python/TS. No production failure is inferred.
- **Smallest correction:** use the bound support source as the pressured grouped verify/submit and retry that same call after drain, asserting the group contains the accepted retry once and excludes refused attempts. Reuse the existing Rust scenario shape and existing fixtures; remove the unrelated happy-path tail if it becomes redundant.

## Acceptance matrix

PASS below means the audited source implements the obligation and the recorded test maps to it; it does not mean this reviewer reran the recorded command. FAIL means a required behavior/proof is missing, rather than a demand for an accepted extra safeguard.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| A1: after-only required Trigger, old fields refused | `wyrd-spec/src/card/trigger.rs` tagged strict enum; registration `BindingProjector` | contract unit tests; C7 served OpenAPI | PASS |
| A1: exact after controls SQL due time | `record_group_activity` clamps to 100 years | C13 exercises short duration only; B-02 | FAIL |
| A1: six-fact resumable support and group/owner identity | C1 real SDK paths → outbox → pending → group runner | C1 three separate runtimes, C7 | PASS |
| A1/A7: one common mixed-runtime support conversation | C1 emits one runtime's evidence per server | B-03, Scenario 1 proof absent | FAIL |
| A1/A7: request paths add no group SQL or binding lookup | existing control paths unchanged; Scribe activity SQL is background only | retained no-IO owner scenarios; C1/C11 | PASS |
| A2: visible inputs/results rather than PG acceptance | `Sources::read` exact owner/subject/Verifier/group UNION ALL; no queue selection joins | C8 manually stages visible result before source settlement | PASS |
| A2: null aligned outcomes and retained failed values | `Visible::executions`, `Payloads::value`, shared list per tuple | C3, C6, C8; media failure | PASS |
| A2: physical identity dedup, earliest ingest, deterministic selection/order | `Earliest::keep`, `Visible::executions` | selection unit test; C8 duplicate replay; media equal-ingest handling | PASS |
| A2: late evidence, immutable prior inputs | pin once into queue JSON, digest identity; retries use Grouped inputs | C6/C8 | PASS |
| A3: same snapshot idempotent independent of status | `PinnedGroup::new` JCS/SHA; unique group snapshot index; exact digest conflict lookup | C8/C9; digest test | PASS |
| A3: distinct snapshots overlap | no outstanding gate; queue admits distinct digest | C6/C9 | PASS |
| A3: no coverage/finality/discovery machinery | old fields/types/indexes/cursor and discovery removed in full diff | static diff; late-evidence journey | PASS |
| A4: settled-order baseline/equal/changed/recovery/replay | binding-row completion lock, `PREVIOUS_GROUP_SQL`, newly-applied check, dispatch uniqueness | C2 all SDKs; C9 | PASS |
| A5: coding without tracing, original files/commands remain caller-owned | coding sources; unchanged Vala ordinary check evaluation; no new producer executor | C3 all SDKs | PASS |
| A5: optional authorized Agent live trace reads | source maps trace declaration to ID only; existing hosted query path retained | C3 delayed spans and revoked permissions | PASS |
| A6: media 20/21, failed and empty; normal per-item retry | ceiling code/config removed; ordinary grouped evaluation and Assert each retained | C4 all SDKs; C10; Rust per-item retry | PASS |
| A7: independent groups/tenants/owners/source versions | exact tuple filter + TenantConn; pending SQL tuple match; authenticated owner | C5 all SDKs and retained owner-boundary journeys | PASS |
| A7: ordinary ungrouped invocation survives | group metadata remains optional; no pending activity without group | C1/C5/C11 | PASS |
| A8: TASK-002 every scheduled samples/check requirement | sample declarations and scoring unchanged by diff; group and cron loops separate | C12 all SDKs with parameterized pass/fail/refusal | PASS |
| A8: pre-staging refusal, audit non-blocking, tenant B progress | existing bounded staging and ClaimLoop tenant rounds retained; shared capacity | C11; retained owner checks reported | PASS |
| A8/A11: Python/TS bound group pressure retry/drain/readback | independent fresh support tail instead of bound retry | B-05 | FAIL |
| A9: minimal GroupSnapshot/public projections | sources-only contract, execution `group`, authored Python stub, generated TS, OpenAPI | C7/C10; recorded codegen/typing checks | PASS |
| A9: documentation states ACK/visibility/post-ACK-loss tradeoffs | updated design and how-verification-works; runtime-limits ceiling removed | static docs check recorded | PASS |
| A10: all original functional scenarios have passing required evidence | main C1–C13 results recorded, but mandatory composed stories/proofs absent | B-03/B-04/B-05 | FAIL |
| A10: benchmark explicitly remains pending, no new driver/framework | no new benchmark machinery; old builder compatibility retained | existing normal build checks reported; no performance PASS | PASS |
| A11: input/result ACK updates pending before reserved submit handoff | `ScribeSink::write` → `schedule_groups` before `reserved.stage`; retryable SQL failure | C8/C13; lower-owner outbox tests recorded | PASS |
| A11: one forced-RLS scheduling row, FK/cascade/indexes | new pending migration, exact tuple activity SQL | fresh DB migrations/C13 | PASS |
| A11: snapshot+enqueue+delete atomic; failed query/SQL rollback | caller-owned TenantConn held across bounded read and committed once | C8 failure, C13 concurrency | PASS |
| A11: refused enqueue preserves pending | `hand_off_groups` consumes Refused | B-01 | FAIL |
| A11: source arrival racing handoff survives | locked row until enqueue/delete/commit; blocked upsert recreates row | C13 concurrent direct activity SQL | PASS |
| A11: 64 tenants/32 items, sequential group passes, shared active capacity | scheduler constants and loops; one Arc<Semaphore> composed into runner/group; ClaimLoop holds owned permit | C13 32-item/capacity cases plus source inspection | PASS |
| A11: blocked snapshot permits real cron scheduling | separate loops in production, test no scheduled binding/occurrence | B-04 | FAIL |
| Constraints/non-goals: no new evidence store, queue, recovery service, quota, engine, dependency or platform mechanism | existing queue/outbox/SQL owners extended; only authorized pending table added | full range/file-list inspection | PASS |
| INV-001/003/004/005: Rust ownership, exact identities, immutable inputs, live optional trace, non-blocking audit | contracts/server/shared client boundaries retained; pinned inputs; tenant query authorization | C1–C12 mapping above; no contrary source path found | PASS |
| INV-002: ordinary finite active work, ordinary query bounds | shared semaphore; Reader rejects terminal != Success; no ceiling bypass | C8/C11/C13; benchmarking intentionally separate | PASS |

## Coverage and caller expansion

Read the original task from base, revision-17 spec and immutable diff/file inventory. Applied `.agents/skills/wyrd-task-review/SKILL.md`, AGENTS, agent-rules, reference router, spec-driven development, maintainer/testing/evaluation guidance and relevant design/doctrine/Bifrost authority. Used CodeGraph before locating the changed owners.

Expanded the starting map through:

- public SDK verify/submit and HTTP replay → authenticated owner input/result projection → Scribe frames/ACK/lost-item selection → pending SQL → same-transaction group snapshot handoff;
- binding registration/source projection → exact frozen tuples → owner accepted query authority → UNION read → logical identity/result selection → JCS digest → immutable queue inputs → ordinary runner → settlement/Operator intent;
- scheduler runtime builder → shared capacity → ClaimLoop runner/operator siblings → cron/group loops → cancellation/transaction ownership;
- C1–C13 source, including support/refund/coding/media/coexistence/late/pressure journeys in Rust/Python/TS, MCP schema and media, served OpenAPI, SQL snapshot/settlement integration and server pending tests;
- changed migrations, Python authored inspection stub, generated TS declarations, fixture timing/selection changes, public documentation and ceiling removal.

No additional review entity, recovery inventory, visibility barrier, source settlement filter, serialization/adoption logic, strict fairness/quota, or benchmark was demanded. Fixture/helper naming leftovers and prose style are not behavioral blockers in this report.

## Verification and diagnosis assessment

The task records: Rust nine scenario selections passed; Python eleven parameter cases passed; TS six plus five parameter cases passed; C7 one, C8/C13 two, C9 one, C10 two; format/lints/codegen/dependency/tenant/Python/TS/docs checks passed. Recorded feature/target/selector choices align with the test owners; no zero-test selector was found. These are implementer records, not independently rerun results or raw logs.

The two recorded failure diagnoses are plausible against source: scheduled-query fault handling now mirrors the existing collect path by marking/holding the schema stall; MCP frames in one Scribe batch can share ingest time, so deriving outcome order from pinned execution IDs is correct and avoids an invalid submission-order assertion. Neither justifies weakening actual source membership/value checks. The recorded Refused-consumption decision is contradicted by locked decision 7 (B-01).

The orchestrator subsequently supplied `verification.md`: C13 was independently rerun with tracing through the repository-managed Postgres/migration wrapper, selected exactly one test, and passed (4.480 s; run `c3092534-4f3b-4f26-a2f5-4bd1db09517f`). This corroborates execution of the current assertions and does not fill the non-vacuous cron/query-count gap in B-04.

No Cargo-heavy suite, SDK build or database test was run by this reviewer, avoiding uncoordinated resource contention. Static source inspection suffices for the reported branches and missing assertions; focused reproduction commands should be coordinated by the orchestrator. REQ-015/AC-009 performance qualification remains intentionally unchecked and is not a finding.

Residual risk: unexecuted runtime/SQL failure paths and the missing composed acceptance proofs above remain; this report claims no independent green build.
