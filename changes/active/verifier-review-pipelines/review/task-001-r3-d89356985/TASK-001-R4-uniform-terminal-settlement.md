---
id: TASK-001-R4
kind: remediation
status: proposed
spec: SPEC-verifier-review-pipelines
spec_revision: 21
approved_scope: terminal-settlement amendment and unchanged revision-18 obligations
supplement: SUPPLEMENT-verifier-execution-state-and-durability@3
parent_task: TASK-001
previous_remediation: TASK-001-R3
followup_remediation: TASK-001-R5
candidate: d89356985566ab3492699682942438c25c8322fd
original_base: 0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb
prior_candidate: cd905f4cd929785bda25696326d58b4a4ae88dee
depends_on: []
remediates: [FIND-TASK-001-16, FIND-TASK-001-17, FIND-TASK-001-19, FIND-TASK-001-22, FIND-TASK-001-23, FIND-TASK-001-26]
user_directed: [USER-TERMINAL-001]
req: [REQ-052, REQ-053, REQ-054, REQ-055, REQ-056, REQ-058, REQ-059]
inv: [INV-017, INV-018, INV-019, INV-027]
ac: [AC-046, AC-047, AC-048, AC-049, AC-050, AC-051, AC-061]
---

# Uniform terminal delivery before queue settlement

> **Implementation rejected; completion claims void.** The user rejected both
> `033aeccd0` and `0d317ecdf`, including the uncommitted `recorded`-flag fix.
> The evidence and `Status: IMPLEMENTED` below are historical claims, not
> acceptance evidence. Follow [R5: revert and rebuild one terminal workflow](../task-001-r4-0d317ecdf-working/TASK-001-R5-single-terminal-settlement.md).
> R4's approved outcomes and remaining acceptance obligations still apply;
> its rejected implementation must be reverted before rebuilding.

## Outcome and approval

Every queued terminal path uses exactly this sequence:

**Select terminal decision / stop computation → write canonical Bifrost state
and required result records → receive existing WAL/batch-commit ACK → settle
queue delivery.**

The user explicitly approved this mechanism after the immutable R3 review.
Cancellation/revocation removes computation eligibility immediately and atomically,
while the existing queue retains durable pending delivery until ACK. A restart
retries record delivery only, without requiring another cancel request, rerunning
a Verifier, renewing a computation budget or granting cancelled authority.

R5 is the active correction handoff for this rejected implementation. Original TASK001 and
implemented R3 are historical context; neither is reopened for implementation.
The [review verdict](verdict.md) remains the verdict on d89356985 under the
pre-amendment authority. Its missing decision is resolved prospectively by
[approved supplement3](../../execution-state-and-durability.md#approved-terminal-settlement--revision-3).
[Spec21](../../spec.md) remains draft only for unrelated open wait/approval
contracts. This task uses its explicitly approved terminal amendment and unchanged
revision18 obligations, not those unrelated draft choices. [TASK007](../../tasks/TASK-007-durable-waits-and-approval.md)
owns implementation of suspension and its continuation surfaces.

## Diagnosis and selected corrections

### FIND16 — Missing group/owner in cancellation identity

`service/inspection.rs:703–719` builds fallback identity with group_id and
owner_card_uid set to None although `queue.execution_status` already supplies
both. Cancelling grouped/binding work before its first checkpoint therefore
loses public group/owner and group-input selection after queue removal.
Copy those existing fields into fallback identity and preserve accepted identity
precedence. No new lookup, payload store or per-reader fallback is needed.

### FIND17 — Terminal production, custody and recovery are incomplete

`runner.rs:390–457` returns target/SYSTEM errors before RunState, checks recovered
selected success but ignores recovered failure, and can dispatch after accepted
TerminalFailure. `state.rs:609–669` allows select after failure, producing both
terminal dispositions. `runner.rs:1375–1405` logs failed canonical failure writes
and still permits terminal queue settlement. Post-delivery terminal mappings
also need the common custody rule.

`inspection.rs:687–797` commits cancellation before a best-effort write; repeat
cancel does not retry it. `verifier_executions.rs:3174–3196` principal cancellation,
`fence_principals` admission and scheduler refused-group insertion only commit
SQL terminal facts. Existing runnable/claim predicates exclude terminal rows.
A crash before enqueue leaves no worker responsibility and no Bifrost record.
Healthy manual run.fail/readback proof does not exercise these actual producers.

Extend the existing queue/runner delivery lifecycle, not a new service. Retain
minimal sanitized terminal decision, exact immutable execution/delivery identity
and durable delivery eligibility until Bifrost ACK; use existing queue leases,
bounded claims, fairness/backoff and shared Scribe writer. Immediate control
cancellation and pending delivery are distinct: a cancelled run is ineligible
for computation but remains eligible for server-owned terminal record delivery.
The queue may retain that narrow pending-decision custody under the approved
amendment; no full progress, output, publication or result payload mirror.
Delivery completion must be recoverable after every process interruption.

Capture exact identity through existing canonical admission/lifecycle custody
while available, including no-checkpoint/pre-target/refused work. Preserve the
fact that refusal supplies neither accepted inputs nor execution authority.
Recover an accepted terminal success or failure before attempting computation or
replacing it with a fresh target failure. Enforce mutually exclusive terminal
selection at existing RunState memory authority. Match accepted state after a
lost ACK/failed queue settlement and retire only delivery, idempotently. Normal
lease/authority controls and the accepted rare delayed-old-worker gap remain.

This explicitly selected existing-owner extension closes the review's unresolved
custody boundary. A best-effort callback, reader-only SQL fallback, stale runner,
process-local retry or blanket scan of all terminal history is insufficient.
Delivery discoverability and bounded due/lease eligibility must survive restart;
queue coordination can retire only after all required canonical ACKs. Private
column names and local method layout remain implementation-owned. An ordered
migration may extend existing queue coordination; no execution-head table,
archive/finalizer, second queue or writer is permitted.

### FIND19 — Final-attempt recovery exhausts unknown completed state

`runner.rs:1036–1041` converts exhausted Retry/Defer to lease_expired even when
reserved outbox capacity or exact accepted state is temporarily unavailable.
That strands an already completed result after a failed queue COMMIT. The new
healthy final-attempt test misses that window. Keep exhausted non-computing
reconciliation on the existing no-settlement/lease-expiry reclaim path when
blocked; ordinary release refunds an attempt and is insufficient. Permanently
exhaust computation only after authoritative readable state proves unfinished
work, then deliver that terminal decision through the same ACK rule. Preserve
attempt counts, original computation deadline and all nonexhausted semantics;
required terminal delivery is not discarded when computation time expires.

### FIND22 — Checkpoint charge ignores normal variable results

`state.rs:379–403` charges entry counts and publication JSON, omitting variable
invocation settlements, captured actual/data/reason, final outcome/evidence and
identity/failure/object strings. A normal64KiB captured actual without publication
still charges1024 bytes. Part/ScribeSink/Outbox trust that charge, defeating byte
backpressure. Keep deferred bounded preparation and OnceLock. Conservatively
traverse complete retained values and prepared-representation overhead with
stable saturating accounting before admission; reuse existing value accounting
where sufficient. No eager JSON/Variant encoding, generic size framework,
second governor or constant-only per-entry estimate. Admission/release use the
same immutable charge across preparation and retries.

### FIND23 and FIND26 — Bounded mandatory static corrections

Add actual Errors documentation to `Checkpoint::prepare`, Panics to the changed
outbox test helper `execution_state`, and the LeaseRenewal::hold poison invariant
to `Leased::writer`. Reserve removed protobuf number10/name state in the existing
IngestCaptureRequest without restoring removed claims or compatibility code.
These are explicit gates; source inspection plus relevant existing checks is
the proof. No manufactured RED or separate style round.

## Boundaries preserved

Bifrost owns accepted recovery state and retained history; queue ownership is
admission, computation control and the approved narrow pending terminal delivery.
No Bifrost component writes Postgres queue rows: server queue/runner owners do.
No per-execution SQL head, revision ownership, strict new distributed fencing,
universal exactly-once external effect or remote WAL durability promise.

Keep short memory locks, coalescing/shared waiters, exact revision/ticket coverage,
per-frame ACKs, execution lanes, generic batch dedup/replay, original attempt and
computation limits, scoped authorization/audit, exact File custody, group and
Operator idempotent completion, independent runtime availability and direct
queue-free successful fast return. Do not add a direct resume scheduler or make
direct final result delivery wait for ACK. No retention policy or cleanup project.

Suspension is nonterminal. TASK007 must save and ACK its waiting checkpoint before
retiring its current queue delivery and releasing all active resources; the future
trigger admits a new delivery of the same workflow through SubmissionSink. R4
must preserve that shared boundary but does not implement wait/approval/trigger
features, choose their open public policies, or keep parked worker resources.

## Ordered scenarios and focused proof

Use existing test owners. New selectors below are planned and must be registered
and confirmed nonzero before RED. Preserve RED/GREEN evidence rather than claiming
historical code would have failed. All commands run serially through mise.

### 1. Canonical terminal decision survives settlement failure

**Behavior.** Accepted success and failure replay the same disposition after a
failed queue settlement; failure never reenters the engine and select cannot
supersede failure. Pre-target errors cannot replace an accepted terminal state.

**RED.** Extend existing failed-settlement seams; planned T1 exercises accepted
engine failure then termination COMMIT failure/reclaim and terminal exclusivity.
Record failure due to missing behavior, not setup.

**GREEN.** Complete common terminal recovery/selection and ACK-gated delivery
retirement at existing owners; retain same result/error identity.

**REFACTOR.** Reuse existing result/failure mapping and RunState authority; remove
caller-specific contradictory terminal logic without changing sibling controls.

### 2. Every terminal producer retains restartable delivery

**Behavior.** Success, engine error, timeout, exhaustion, user/principal cancel,
inactive admission, pre-target refusal and group refusal survive death before
enqueue, ambiguous acceptance and death after ACK before queue settlement.
Cancellation stops immediately during Bifrost outage; queued delivery survives
restart and eventually records exact sanitized state without computation.

**RED.** Planned T2 exercises these equivalent producer/interruption cases at
actual owners, including no checkpoint and no original caller after restart.
Check canonical identity/status/error after queue/source deletion once delivery
is accepted. Duplicate reply/request is not required for repair.

**GREEN.** Extend existing bounded queue/runner delivery eligibility and retry
coordination, canonical identity custody and stable shared-writer delivery; retire
pending delivery only after all required ACKs. Retain normal atomic revocation.

**REFACTOR.** Consolidate producer delivery through existing owners. Do not create
a general projector, extra writer, history scan or per-route reader workaround.

### 3. Grouped pre-claim cancellation preserves metadata

**Behavior.** Group/owner and existing canonical group-input selection remain
identical after cancellation, queue/Card removal and authorized retained read.

**RED.** Planned T3 extends current cancellation journey with grouped metadata;
retain a missing-grant negative case.

**GREEN.** Propagate already available metadata into existing immutable identity.

**REFACTOR.** Reuse the common retained reader and existing group-input lookup.

### 4. Exhausted reconciliation survives temporary unavailable state/capacity

**Behavior.** A final attempt whose result is durable but queue COMMIT failed
waits/retries delivery during separate source and reserved-share outages, then
completes with the same identity, unchanged attempts and zero engine entries.
Readable truly unfinished final computation still exhausts and records its error.

**RED.** Planned T4 extends the existing final-attempt proof with controlled
unavailability. Restore the dependency within its existing test deadline; do not
invent a new postdeadline computation policy.

**GREEN.** Preserve exhausted record-only retry through existing lease expiry;
never convert unavailable/incompatible knowledge into unfinished state.

**REFACTOR.** Keep nonexhausted refund/retry semantics and queue control owner.

### 5. Complete checkpoint charge follows retained payload growth

**Behavior.** A checkpoint with large actual/data/reason and terminal outcome,
without publications, charges their bytes; small configured admission backpressures
and releases the same charge. Preparation/retries retain identity and charge.

**RED.** Planned T5 extends current owned-memory proof, showing existing fixed
entry estimate ignores that ordinary payload.

**GREEN.** Conservatively account complete retained values and prepared capacity
without moving serialization out of the admitted bounded writer.

**REFACTOR.** Reuse typed value traversal/accounting; no framework or benchmark.

### 6. Parallel settled predecessors release their real dependent

**Behavior.** Hosted steps A/B record settlements concurrently while an ACK is
held; dependent C consumes both only after an accepted snapshot covers both.
Restart restores accepted downstream values without repeating settled calls.

**RED.** Planned T6 supplies the original missing observation. A verification-only
result is acceptable if the real hosted behavior already passes; do not replace
settlements with begin-only records or manufacture a production change.

**GREEN.** Correct only a demonstrated existing coverage/dependency gap.

**REFACTOR.** Keep existing walker/ledger/checkpoint/outbox owners.

## Implementation reference

| Existing owner | Missing behavior / selected extension |
|---|---|
| server verification/runner.rs execute/state/attempt/settle/retained/deliver; verification/completions.rs | Shared terminal failure recovery and ACK-before-retirement; bounded record-only claims/reconciliation reuse existing claim/settle lifecycle. Completions' process channel alone is not durable custody. |
| wyrd-sql queries/verifier_executions.rs claim/due/cancel/cancel_principal/fence_principals/refuse_group | Distinguish immediate computation ineligibility from durable pending terminal delivery at every writer; preserve tenant transactions, Postgres clock and atomic revoke. Extend existing queue, not execution payload tables. |
| server components/verification/service.rs and service/inspection.rs; auth/revoke.rs; scheduler/submissions | Actual direct/queued/control/admission producers and fallback metadata; no stale-worker writer. Capture available exact identity at canonical lifecycle/admission owner. |
| verification/state.rs RunState/Checkpoint; scribe_outbox.rs Part/ScribeSink; shared runtime Outbox | Canonical terminal exclusivity, stable complete memory charge, existing bounded preparation/ACK/retry mechanisms. |
| wyrd-spec verification/state.rs; canonical verification inputs/events | Existing identity/failure format; preserve no-authority refusal distinction and exact accepted history. No new independent journal. |
| pg_durable_verifier_work, pg_verification_runtime, pg_verifier_executions and state/outbox units | Existing real-server, failed settlement, cancellation, fixture restart, controlled writer and owned-memory seams. No new harness/target. |
| wyrd-tonic proto and existing generators; owning architecture/spec | Static tag/name retirement and consistent approved queue-delivery boundary. Never rewrite applied migration checksums or hand-edit generated artifacts. |

## Commands and acceptance evidence

Planned focused commands T1–T6 (existing packages/targets, new test names):

```bash
# T1
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::terminal_failure_reclaims_without_reentering_the_engine)"'
# T2
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::terminal_delivery_survives_every_commit_boundary)"'
# T3
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::preclaim_group_cancellation_preserves_retained_identity)"'
# T4
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::exhausted_reconciliation_survives_source_and_capacity_outage)"'
# T5
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::checkpoint_charge_tracks_variable_settlements)'
# T6
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::concurrent_settlements_release_their_hosted_dependent)"'
```

| Obligation | Required evidence |
|---|---|
| USER-TERMINAL-001; FIND17; REQ055/058; INV027; AC061 | T1/T2 actual producer/interruption matrix; ACK before queue retirement, immediate stop, restartable record-only delivery, accepted decision exclusive and unchanged. |
| FIND16; REQ056; AC048 | T3 retained group/owner/exact source selection and current permission negatives. |
| FIND19; REQ058; AC049 | T4 no false exhaustion under unknown state, no new attempt/computation, eventual original completion. |
| FIND22; Revision1 admission invariant | T5 variable payload growth, actual byte backpressure, stable release/preparation/retry charge. |
| FIND23/26 | Source-to-rustdoc and reservation inspection; existing format/lint/proto/codegen checks for actual write set, no runtime test. |
| Original dependent proof | T6 actual hosted settlement-consuming dependent, parallel progress and exact restored values. |

After focused scenarios, run only affected state/outbox units and exact changed
queue/migration regressions needed by the final write set. Record each named test's
exact selector and result. Run repository fmt/lints and applicable codegen,
dependency/tenant/docs checks for touched owners; no broad gate, full language
journeys, synthetic host load or throughput project. Review's existing green
runs are historical context, not closure evidence for this task. A new applied
queue migration needs existing apply/idempotence and an actual interrupted
pending-delivery upgrade/claim proof. Preserve older payload custody guards.

## Handoff and limits

Implement this single task with wyrd-implement when the caller requests execution;
this document authorizes no automatic implementation or Git operation. Then review
closure against d89356985 and this approved amendment, retaining prior accepted
boundaries rather than restarting discovery. No finding is closed by drafting.

If implementation requires full execution-state custody in SQL, a separate
finalizer/queue/writer, stronger old-worker fencing, weakened revocation, new
public API or unresolved TASK007 policy, stop and identify the exact conflict.
Private queue coordination shape and helper layout are ordinary implementation
choices under the approved terminal delivery contract.

## Packet preparation evidence

Independent read-only comprehension passed on the amended spec/supplement,
architecture owners, R4 and TASK007: exact ACK-before-retirement mechanism,
immediate computation stop, durable record-only retry, preserved direct return
and accepted old-worker limitation, six finding mappings and six complete TDD
blocks. Existing test packages/targets/features/Postgres wrapper were checked;
new selectors remain planned until registered. Local links, whitespace, metadata,
dependency map and git diff --check passed. No implementation or runtime tests
were performed by this packet update; no finding is closed by these checks.

## Implementation evidence

Commits: `033aeccd0` (terminal delivery, recovery, FIND19/22/23/26),
`0d317ecdf` (delivery tenant scan, T1–T4/T6, updated durable-result proof).
Every PG command ran serially through
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=<selector>)"'`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND17 failure never reenters the engine; select cannot supersede failure | `verification/state.rs` `Decision`, `select`/`fail` via `decide`; `verification/runner.rs` `execute` (`Decision::of` before computing), `decided_transition` | T1 `pg_verification_runtime::terminal_failure_reclaims_without_reentering_the_engine`; `verification::state::tests::a_recorded_failure_is_never_superseded`, `duplicate_selections_share_one_accepted_result` | PASS |
| FIND17 pre-target refusal cannot replace an accepted decision | `runner.rs` `refused`, `system` | Code inspection; covered by the shared `decided_transition` path T1 exercises | PASS |
| FIND17 ACK before queue retirement; restartable record-only delivery for every queued terminal writer | migration `20261108000000_verifier_execution_terminal_delivery.sql`; `wyrd-sql` `INSERT_SUBMISSIONS_SQL`, `INSERT_GROUP_ERROR_SQL`, `TERMINATE_SQL`, `CANCEL_SQL`, `CANCEL_PRINCIPAL_SQL` set `delivery_due_at`; `claim_deliveries`/`delivered`/`redeliver`; `RUNNABLE_TENANTS_SQL` `owing`/`delivering`; `runner.rs` `RunnerClaim::Delivery`, `deliver_owed`; `RunState::deliver` | T2 `pg_durable_verifier_work::terminal_delivery_survives_every_commit_boundary` (queue-only termination, undelivered cancel, abandoned zero-lease delivery claim, restart, read back after queue and Card deletion) | PASS |
| Interrupted pending-delivery claim and migration apply/idempotence | same | T2 abandoned-claim step; `wyrd-sql pg_migration::pg_tests::migrations_apply_and_are_idempotent` (every wrapper run) | PASS |
| FIND16 group/owner in the cancel identity | `PendingDelivery` (group, owner = binding owner else owning root); `inspection.rs` `deliver_cancellation` | T3 `pg_durable_verifier_work::preclaim_group_cancellation_preserves_retained_identity` (group retained, owner as the queue holds it, outsider cancel/read 403) | PASS |
| FIND19 blocked exhausted claim never falsely exhausts | `runner.rs` `attempt`: exhausted `Release`/`Retry`/`Defer` → `LeaseLost` | T4 `pg_verification_runtime::exhausted_reconciliation_survives_source_and_capacity_outage` | PASS |
| Selected result is never timed out or retried after selection | `runner.rs` `deliver`: drop/timeout → `Defer`, ACK wait floored at `DELIVERY_TIMEOUT` | `pg_verification_runtime::completion_waits_for_the_durable_result` (updated to the R4 contract) | PASS |
| FIND22 checkpoint charge follows variable payloads | `state.rs` `Checkpoint::charge`/`weight` | T5 `scribe_outbox::tests::checkpoint_charge_tracks_variable_settlements` (`cargo nextest run --locked -p wyrd-server --lib -E 'test(=…)'`) | PASS |
| Parallel settled predecessors release their dependent | existing `RunState` checkpoint owner, unchanged | T6 `pg_durable_verifier_work::concurrent_settlements_release_their_hosted_dependent` (verification-only) | PASS |
| FIND23/26 static | `Checkpoint::prepare` `# Errors`; `execution_state` `# Panics`; `Leased::writer` poison invariant; `wyrd.v1.proto` `IngestCaptureRequest` `reserved 10` / `"state"` | `mise run codegen:check` | PASS |
| Regressions | — | `pg_durable_verifier_work` + `pg_verification_runtime` 72 tests (71 pass; the 72nd is the durable-result proof, updated above and passing); `wyrd-sql` `verifier_executions` 39/39; `wyrd-testing --test server -P journey verification_runtime::cancel_and_revocation_fence_unfinished_work`; state/outbox units | PASS |
| Format, lints, boundaries | — | `mise run fmt`; `cargo clippy --locked -p wyrd-server -p wyrd-sql -p wyrd-testing --features wyrd-server/test-support --tests -- -D warnings`; `mise run check:tenant-isolation`; `mise run docs:check`; `git diff --check` | PASS |

RED (one build with the mechanisms reverted, runner restored afterwards):
- With failed-decision recovery, the exhausted → `LeaseLost` mapping and delivery claims all reverted, T4 failed: `status: "errored", error_code: Some("lease_expired")`.
- T1 failed: `timed out waiting for the delivery to retire`.
- T2 failed: the deliveries were never retired.
- T1 still terminated with the recorded error once recovery was reverted, because `select` refuses to supersede a recorded failure. Its RED therefore isolates delivery retirement, not engine re-entry.
- T5 RED came from the HEAD state charge (the fixed per-entry estimate).
- T3 and T6 have no RED: T3 needs the new delivery API, and T6 is verification-only by design.

### Diagnosis

- **Runnable-tenant scan.**
  - *Symptom:* every runner claim round failed with `syntax error at or near ")"`.
  - *Cause:* a stray `)` closing the `waiting` CTE in `RUNNABLE_TENANTS_SQL`.
  - *Fix site:* the query itself; its only caller is `tenants_with_runnable_executions`.
- **Plan test.**
  - *Symptom:* `runnable_scan_walks_the_claim_indexes_however_large_the_backlog` saw a `Bitmap Heap Scan`.
  - *Cause:* the grouped `delivering` aggregate scanned every owed row.
  - *Fix:* replaced with the same recursive skip scan the queued and leased CTEs use. The plan assertion now also admits `verifier_executions_delivery_due`.
- **Refused-shape test.**
  - *Symptom:* `group_snapshots_are_idempotent_and_react_in_settlement_order` tripped `verifier_executions_delivery_shape` before the refused-shape constraint it targets.
  - *Fix:* dropped the new CHECK, since only the terminal writers set the column.

### Limits

- **Executions settled before the migration owe no delivery.** They keep their queue record only.
- **Every queued terminal is delivered through the delivery claim,** including failures already recorded by the attempt. This costs one idempotent state read per failed execution (`RunState::deliver` keeps an existing decision).
- **T3 seeds the group id with SQL** on a manual submission. A manual row cannot carry `owner_card_uid` without a binding, so the owner is asserted as the queue derives it.
- **T4 exercises the source outage only.** The capacity refusal maps through the same exhausted `Defer` → `LeaseLost` arm, but the harness cannot fill the outbox.

Non-goals stayed excluded: no SQL execution head, second queue, writer or finalizer; no applied migration edited; the direct successful verify path still returns without an ACK wait. The `architecture/*` edits and this packet stay uncommitted user work.

Status: IMPLEMENTED.
