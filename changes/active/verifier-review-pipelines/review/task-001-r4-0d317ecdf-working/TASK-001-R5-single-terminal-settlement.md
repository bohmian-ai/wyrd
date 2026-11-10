---
id: TASK-001-R5
kind: remediation
status: ready
spec: SPEC-verifier-review-pipelines
spec_revision: 21
approved_scope: terminal-settlement amendment and unchanged revision-18 obligations
supplement: SUPPLEMENT-verifier-execution-state-and-durability@3
parent_task: TASK-001
previous_remediation: TASK-001-R4
candidate: 0d317ecdf2d28d2e77b6f394ebe0f443d256d580
restart_baseline: d89356985566ab3492699682942438c25c8322fd
depends_on: []
user_directed: [USER-TERMINAL-001, USER-TERMINAL-002]
remediates: []
---

# Revert R4 and implement one terminal-delivery workflow

## Outcome and authority

Revert the rejected R4 implementation, then implement R4 through one existing
verification-runtime owner. Both an active attempt and a pending-delivery claim
must use the same terminal operation: **resolve the canonical decision → write
the required Bifrost records through the shared Scribe outbox → wait for their
WAL/batch-commit ACKs → settle the matching queue delivery**.

For example, an engine failure is recorded and acknowledged once, and its queue
termination commits with no further delivery owed. If that commit fails, a new
claim recovers the accepted failure through the same operation, runs no engine,
and finishes queue settlement. Cancellation stops computation immediately; its
durable pending delivery enters that same operation without a computation lease.

This is a user-directed correction, not an independent task-review verdict.
USER-TERMINAL-001 is the approved single-sequence requirement; USER-TERMINAL-002
is the user's rejection of both R4 commits and instruction that the implementor
revert its work. No new `FIND-*` IDs or review approval are claimed here.

The contract is the [original R4 task](../task-001-r3-d89356985/TASK-001-R4-uniform-terminal-settlement.md),
the [approved terminal amendment](../../execution-state-and-durability.md#approved-terminal-settlement--revision-3),
and the approved baseline obligations described in [spec revision 21](../../spec.md).
The parent spec remains draft for unrelated wait/approval decisions; those are
not reopened. R5 replaces R4's rejected implementation approach, not its other
acceptance obligations. Route this task directly to `$wyrd-implement`.

## Diagnosis and rejected alternatives

The inspected subject is HEAD above plus the uncommitted runner and runtime-test
edits. The rejected commits are `033aeccd0` and `0d317ecdf`.

| Source evidence at handoff | Defect and consequence |
|---|---|
| `verification/runner.rs::retained`, lines 1568–1601; `RunState::fail/decide`, `verification/state.rs:776–814` | The attempt records its failure and waits for the state ACK on its own path. |
| `wyrd-sql/src/queries/verifier_executions.rs::TERMINATE_SQL`, lines 584–592; exhausted `VerifierExecutionQueue::retry`; `runner.rs::deliver_owed`, lines 531–580 | Termination nevertheless owes another delivery. A later claim independently resolves/records the same decision and retires it. An accepted failure avoids another state write today, but still incurs another claim, SYSTEM lookup, authoritative state read and settlement transaction. This is duplicate workflow ownership even when no second canonical row is written. |
| Dirty `runner.rs::settle`, lines 967–969, plus `execute/refused/decided_transition/busy/retained/attempt` | `false && recorded` disables the attempted suppression. Enabling it would retain both workflows and add ACK bookkeeping across them; it does not satisfy the requested correction. |
| `state.rs::RunState::deliver`, lines 605–607; `runner.rs::deliver`, lines 1306–1344; `components/verification/service/inspection.rs::deliver_cancellation`, lines 728–763 | Pending delivery returns an accepted selected outcome as though all records were delivered, bypassing the normal result builder/output/summary ACK path. Cancellation after selection but before result ACK can retire delivery with required records missing. A common operation must close this gap as part of defining completion once. |
| Dirty `pg_verification_runtime.rs::retryable_engine_failures_exhaust_to_errored`; existing failure-reclaim test | Polling until delivery becomes clear, or sampling a terminal row after a worker may clear it, cannot establish that the terminal transaction itself owed no second delivery. Existing eventual-state proof permits the rejected overlap. |

Deleting the inline write and leaving the second workflow is also invalid: R4's
failed-termination-commit scenario requires the canonical failure to be accepted
before the attempt's queue termination, so reclaim cannot rerun computation.
Keep that ordering by moving the responsibility into the common operation.

## Required rollback before implementation

1. Inventory the dirty tree and preserve unrelated user work. Reverse the source,
   test and protocol changes from both rejected commits, newest first, together
   with the uncommitted `recorded` plumbing and racy assertion. The restart point
   is the R3 candidate above. Do not merely remove `false &&`, retain the two
   workflows under a new name, or cherry-pick their orchestration into the rebuild.
2. Preserve applied-migration history. Treat
   `20261108000000_verifier_execution_terminal_delivery.sql` as immutable: retain
   its current bytes/checksum, including its index. An unused additive column is
   not a reason to drop data or rewrite migration history during source rollback.
   Rebuild may use that already-approved narrow queue coordination. If the
   correction actually needs a schema change, use a new ordered migration and
   prove upgrade of pending rows; never edit the retained migration.
3. Preserve the approved architecture/specification documents, this packet and
   historical review evidence. They are not rejected implementation. Mark the
   prior R4 PASS/IMPLEMENTED claims void; do not present them as evidence for R5.
4. Before rebuilding, record the rollback diff and confirm that the affected
   production/test/protocol files match R3, except the explicitly retained
   migration. Reversing these commits also removes valid R4 changes such as
   checkpoint accounting and protobuf reservation. Re-establish every R4
   obligation and its proof during the rebuild; rollback alone is not completion.

This is a local implementation instruction. Do not reset the whole worktree,
delete unrelated files, erase evidence, commit or push without caller direction.

## Selected correction and preserved boundaries

Extend the existing server verification runtime's terminal-delivery operation;
do not create a finalizer, service, second queue, writer or generic settlement
framework. `RunState` continues to own canonical decision selection/recovery and
state persistence. The runtime continues to own required result delivery and
Postgres queue settlement. Share the complete operation, including its ACK and
settlement rule, rather than sharing only a `state.fail` call between two flows.

An attempt hands its terminal decision and existing state/context to that owner.
A pending-delivery claim supplies its durable decision/identity and recovers
state through the same owner. A request-side cancellation shortcut, if retained,
must delegate to that operation; it must not independently call `RunState::deliver`
and clear the queue. Exact private signatures, helpers and module layout remain
implementation-owned. There is no separate `recorded` flag passed through the
execution stack to compensate for duplicate paths.

The common operation has these obligations:

- Recover authoritative accepted state before replacing a decision. Accepted
  success or failure wins over a later target error or cancellation delivery.
  Keep terminal selection mutually exclusive and preserve stable identity.
- For failure, acknowledge its canonical state before settling an attempt's
  terminal queue transition. For selected success, also use the existing
  `ResultPayloadBuilder::build_as`, selected result ID, output/summary staging
  and required-result ACKs. A selected checkpoint alone is insufficient.
- Retire only the matching delivery after all required ACKs. An acknowledged
  attempt terminal transition must commit without creating another owed pass.
  Queue mutation and retirement share the applicable tenant transaction/fence;
  a stale computation settlement cannot clear a competing cancellation's work.
- Cancellation/revocation's control commit still stops computation atomically
  and retains minimal durable pending delivery before Bifrost is available.
  This is immediate control, not completed record delivery. Every terminal
  producer must leave discoverable responsibility across death before enqueue,
  lost ACK and failed queue commit. No original request or live worker is needed.
- Unavailable state, capacity refusal, rejected/dropped writes and missing ACKs
  retain delivery responsibility. Recovery uses original decisions/bounds and
  performs no provider calls. It never treats unavailable state as absent.
- Retry only genuinely unfinished computation within its original attempts and
  deadline. Preserve exhausted record-only recovery, bounded fair claims,
  backoff, lease/authority fences, tenant isolation, internal delivery authority,
  stable Scribe replay/dedup and idempotent group/Operator completion.

The guarantee is one production workflow, not one network send under every
fault. Lost ACKs can require replay of identical sealed batches; do not remove
legitimate retries to make a write-count assertion pass. No new exactly-once
external-effect or stricter distributed-fencing promise is introduced.
Direct synchronous success retains its approved queue-free fast return.
Suspension/wait/approval remains TASK-007's scope.

## Ordered implementation scenarios

Use existing test owners and controlled Scribe/SQL fault seams. Planned names
below must be registered and confirmed to select a nonzero test before RED.

### 1. One acknowledged failure needs no second delivery pass

**Behavior.** Exercise engine-terminal error, timeout and final retry exhaustion.
Hold the failure's Scribe ACK and observe that attempt delivery has not settled.
After release, observe the actual terminal transaction with delivery clear and
no later owed-delivery claim for that failure. Nonterminal retry still retries.

**RED.** Planned P1 must fail on the rejected two-path implementation. Prevent
the delivery worker from repairing the row before observation, or capture the
transactional fact at commit. Assert the canonical failure and matching identity,
not merely an arbitrary event-frame count. Eventual polling alone is invalid.

**GREEN.** Route attempt failures through the common operation and settle their
ACKed decision once. Remove both superseded orchestration and flag plumbing.

**REFACTOR.** Reuse RunState/outbox/queue owners without duplicating their rules.

### 2. Failed queue commit recovers the accepted decision

**Behavior.** Accept a failure, fail its termination commit, restart/reclaim and
settle the same error with zero engine entries. Exercise accepted success under
the equivalent commit failure, preserving its result identity and completion
effects. Temporary source/capacity outage must not become false exhaustion.

**RED.** Strengthen existing P2 to prove the failure was canonically accepted
before the injected commit failure and retirement needs no repair pass. Keep
the existing success/outage regression P3; extend its capacity case as R4 requires.

**GREEN.** Recover both dispositions through the same operation; keep pending
delivery when authoritative recovery or required ACKs are unavailable.

**REFACTOR.** Preserve original bounds and existing replay/completion owners.

### 3. Every non-attempt producer reaches the same operation after restart

**Behavior.** Exercise user cancel, principal revoke, inactive admission,
pre-target refusal and refused group activation at their actual producers,
including no checkpoint. Stop after control/custody commit before enqueue,
restart without the caller, then recover exact status/error/identity without
engine entry. Separately interrupt after ACK before queue retirement. Missing
SYSTEM authority or unavailable state leaves delivery owed until restored.

**RED.** Extend P4 into an explicit producer/interruption case table. The current
test's two seeded rows do not prove all cases in its broad name. Actual revocation
must prove immediate atomic computation stop while Bifrost is unavailable.

**GREEN.** Feed durable pending claims through the common terminal operation;
retain each producer's existing control/security transaction.

**REFACTOR.** Remove independent request-side delivery/retirement logic; reuse
existing internal permissions and exact identity custody.

### 4. Accepted selection still owes output and summary ACKs

**Behavior.** Hold result delivery after selected-state ACK; cancel or revoke,
then stop the original attempt and restart delivery. Until all required result
records ACK, delivery stays owed. Afterwards the same selected result is retained
with its required records, no engine entry and no duplicate completion effects.
Also race a stale attempt settlement against cancellation; stale work must not
retire cancellation's delivery.

**RED.** Planned P5 must fail the current `RunState::deliver` early-return path.
Observe both state and required result frames/ACKs and the queue retirement
boundary. Cover failed/dropped result ACK and the stale-settlement race using
controlled barriers, not timing sleeps.

**GREEN.** Make the common operation complete the existing selected-result route
for both callers and enforce matching queue settlement.

**REFACTOR.** Reuse the current result builder and stable replay identity; do not
mirror result payloads in SQL or add another builder.

## Acceptance and implementation reference

| Obligation | Closure proof |
|---|---|
| USER-TERMINAL-002: rejected implementation reverted first | Recorded rollback diff against R3, migration exception explained, unrelated user work preserved; prior completion claims explicitly void. |
| USER-TERMINAL-001 and R4 FIND17: one complete operation | P1–P5 plus a source call-path inventory showing every attempt/control/admission/recovery producer reaches the same ACK/settlement owner. No duplicate recording workflow or compensating flag. |
| R4 FIND19: final-attempt recovery conserves work | P2/P3; accepted decision survives failed commit and dependency outages, with no new computation or refunded/increased budget. |
| R4 FIND16/22/23/26 and dependent proof | Re-run original R4 T3/T5/T6 and static documentation/protobuf obligations after rollback/rebuild, using the exact commands in R4. No historical PASS substitutes for new evidence. |
| Existing queued/direct success semantics | P6 and original R4 affected regressions; direct fast return remains outside queued ACK settlement. |

Inspected owners and scope:

- `verification/runner.rs`: execute/refused/retained/decided_transition/attempt,
  result/deliver/settle/deliver_owed; existing normal result path is the required
  reuse boundary, not a model for a second failure workflow.
- `verification/state.rs`: Decision, accepted/select/fail/decide/deliver and
  checkpoint charge; keep memory authority and shared Scribe checkpoint ACKs.
- `components/verification/service{.rs,/inspection.rs}`: cancellation and direct
  verification consumers; `auth/revoke.rs`, scheduler/submission/group owners and
  SQL admission/cancel/refusal writers supply pending decisions, not new writers.
- `wyrd-sql/src/queries/verifier_executions.rs`: terminate/retry/cancel,
  cancel_principal/fence_principals/refuse_group, claim_deliveries/delivered/
  redeliver and tenant scanning. Existing TenantConn transaction composition and
  Postgres-owned deadlines remain required. Retain immutable migration history.
- `verification/results.rs`, completions/groups and `scribe_outbox.rs`: existing
  result builder, completion effects, stable identities, bounded batching/ACKs.
- Existing runtime/durable-work integration modules, SQL queue tests, Bifrost
  runtime test consumer and `wyrd.v1.proto` are in the rejected commits' write set.
  Reconcile their R4 obligations too; no SDK/wire expansion is authorized.

No new dependency, feature, export, public status or durable payload store is
needed. Read nested SQL AGENTS.md and the owning architecture before changes.

## Verification and evidence

Execute focused commands serially with `WYRD_LOG=info,wyrd_server=debug`.
Package, integration target, test-support feature and existing module selectors
were confirmed from manifests and `tests/integration/main.rs`. P1/P5 are planned.

```bash
# P1 — planned deterministic failure ordering/no-second-pass proof
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::terminal_failures_use_one_acknowledged_settlement)"'
# P2 — strengthen existing failed termination/reclaim proof
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::terminal_failure_reclaims_without_reentering_the_engine)"'
# P3 — existing exhausted success recovery; complete required outage coverage
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::exhausted_reconciliation_survives_source_and_capacity_outage)"'
# P4 — extend existing real producer/restart proof
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::terminal_delivery_survives_every_commit_boundary)"'
# P5 — planned selected-result cancellation/restart and stale settlement proof
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::cancelled_selected_delivery_waits_for_all_required_acks)"'
# P6 — existing normal queued success ACK regression
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::completion_waits_for_the_durable_result)"'
mise run fmt
mise run lints
mise run check:tenant-isolation
git diff --check
```

Run original R4 T3/T5/T6, its applicable proto/codegen checks and the exact changed
SQL regressions as described in its Commands section. Register actual selectors
and record exact focused commands for any additional changed tests before RED.
Keep the rustdoc gate. Do not run broad gate/full language suites for this task.
Do not claim RED from rollback/setup errors or weaken a test to fit the new path.

Record rollback evidence, scenario RED/GREEN results, canonical record and queue
observations, final call-path inventory and requirement-to-proof closure. Inspect
tracked and untracked diffs. The implementation is ready for independent review
only when all original R4 obligations and these corrections have fresh evidence.

## Material stop conditions and handoff limits

Stop for a specific authority conflict or a required new public/persisted
contract outside the approved narrow queue extension. Private function shape
is not a blocker. Do not request fresh approval for the already-approved single
sequence or user-directed rollback. Existing pending rows must remain recoverable;
an incompatible upgrade needs explicit migration proof, never silent deletion.

This packet is based on source inspection, not runtime execution. It does not
certify the existing working-source review as complete or independently validate
its reports. No production code was changed and no Cargo tests were run while
authoring this remediation.

Handoff check: one independent, read-only document reader reviewed the approved
terminal context and R4/R5 together and found no material ambiguity or
contradiction. Local document targets, four complete behavioral scenario blocks,
new-file whitespace and `git diff --check` passed. The two dirty Rust files'
SHA-256 hashes were unchanged across packet authoring.

## Implementation evidence

Status: IMPLEMENTED (one gap stated under Limits). Uncommitted working tree on
`0d317ecdf`; no commit. Every R4 `PASS`/`IMPLEMENTED` claim recorded in
TASK-001-R4 is void and is not evidence for R5.

### Rollback

- R4 saved before rollback: `git diff d89356985 0d317ecdf` (both rejected
  commits) and the uncommitted runner/runtime-test `recorded` edits.
- `git checkout d89356985 --` on every R4-touched source, test and proto file
  (`service.rs`, `service/inspection.rs`, `scribe_outbox.rs`, `runner.rs`,
  `state.rs`, `pg_durable_verifier_work.rs`, `pg_verification_runtime.rs`,
  wyrd-sql `verifier_executions.rs`, `pg_verifier_executions.rs`,
  wyrd-testing `verification_runtime.rs`, `wyrd.v1.proto`). Afterwards
  `git diff --stat d89356985 -- crates` showed only the retained migration.
- Migration exception: `20261108000000_verifier_execution_terminal_delivery.sql`
  keeps its committed bytes (`git diff HEAD -- crates/wyrd/wyrd-sql/migrations`
  is empty). Its `delivery_due_at` column and index are the approved narrow
  queue coordination R5 rebuilds on; no new migration.
- Architecture/spec documents, packets and review evidence were not reverted.
- Rebuild re-established R4's valid obligations: `scribe_outbox.rs` checkpoint
  charge (T5) and the `IngestCaptureRequest` `reserved 10` / `"state"` protobuf
  reservation are byte-identical to R4's; neither is a terminal workflow.

### One terminal operation (call-path inventory)

`runner.rs::conclude` is the only place a terminal decision is resolved and
written: `Offer::Accepted` keeps the accepted decision; `Offer::Failed` →
`RunState::fail` (state ACK); `Offer::Selected` → `RunState::select`, then
`result` (existing `ResultPayloadBuilder` path, selected result ID) and
`deliver` (stage on the shared Scribe slot, wait for the result ACK). Its
`Transition` is settled by `settle` (attempt, lease-fenced) or
`settle_delivery` (delivery claim, fenced on the claimed `delivery_due_at`).

| Producer | Path to `conclude` | Queue settlement |
|---|---|---|
| Engine completed / terminal / final-attempt retry / timeout | `attempt` → `execute` → `conclude` | `settle`: ACKed → `terminate(.., Acknowledged)`; unACKed failure → `Owe` → `terminate(.., Owed)` |
| Exhausted claim, accepted state | `execute`: `Offer::Accepted`, no engine | same |
| Exhausted claim, nothing decided | `execute`: `Offer::Failed(lease_expired)` | same |
| Target refused at claim | `execute` → `record` → `conclude` | `settle` |
| User cancel (`inspection.rs::cancel`) | `CANCEL_SQL` stops computation, delivery due now; no request-side state write | delivery claim → `deliver_owed` → `record` → `conclude` → `settle_delivery` |
| Principal revoke (`fence_principals` → `cancel_principal`) | same as cancel | same |
| Inactive admission / pre-target refusal (`INSERT_SUBMISSIONS_SQL` errored rows) | delivery due now | same |
| Refused group activation (`INSERT_GROUP_ERROR_SQL`) | delivery due now | same |
| Attempt whose ACK or termination commit was lost | `TERMINATE_SQL` owed, or row still `running` → reclaim → `execute` with `Offer::Accepted` | `settle` / `settle_delivery` |
| Exhausted SQL `retry` | delivery due now | delivery claim |

Removed: R4 `deliver_owed`'s own recording workflow, `RunState::deliver`'s
selected early return, `inspection.rs::deliver_cancellation` /
request-side retention, and the `recorded` flag plumbing.

### RED (R4 candidate) and GREEN (R5)

RED tree: scratch worktree at `0d317ecdf` plus R4's uncommitted runner edit,
with the R5 server test files copied in. Compile-only adaptations to R4's
older SQL API, made in the scratch tree only: removed the
`TerminalDelivery::Acknowledged` argument (3 call sites) and added
`.is_some()` to `cancel` (3 call sites). R4's SQL test file was kept there
because the migrate step compiles it. Commands: the packet's exact commands,
with `CARGO_TARGET_DIR` set to a scratch dir and `WYRD_LOG=info,wyrd_server=debug`.

| Proof | RED on R4 | GREEN on R5 |
|---|---|---|
| P1 `pg_verification_runtime::terminal_failures_use_one_acknowledged_settlement` | FAIL `no commit owes a delivery: [.., ("errored", true), ("errored", true)]` | PASS |
| P2 `pg_verification_runtime::terminal_failure_reclaims_without_reentering_the_engine` | FAIL `no commit owes a delivery: [.., ("errored", true), ("errored", true), ("errored", false)]` | PASS |
| P3 `pg_verification_runtime::exhausted_reconciliation_survives_source_and_capacity_outage` | PASS (regression, expected) | PASS |
| P4 `pg_durable_verifier_work::terminal_delivery_survives_every_commit_boundary` | FAIL `every decision is owed`: the user cancel was already retired at request time | PASS |
| P5 `pg_verification_runtime::cancelled_selected_delivery_waits_for_all_required_acks` | FAIL `the delivery writes the result: Elapsed(())` (`RunState::deliver` early return) | PASS |
| P6 `pg_verification_runtime::completion_waits_for_the_durable_result` | PASS (regression, expected) | PASS |
| T3 `pg_durable_verifier_work::preclaim_group_cancellation_preserves_retained_identity` | not run (R4 had no RED either) | PASS |
| T5 `scribe_outbox::tests::checkpoint_charge_tracks_variable_settlements` (`mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=…)'`) | not re-run (code identical to R4) | PASS |
| T6 `pg_durable_verifier_work::concurrent_settlements_release_their_hosted_dependent` | not run (verification-only per R4) | PASS |
| SQL `pg_verifier_executions::terminal_deliveries_are_settled_only_by_their_latest_claim` (`scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=…)'`) | none: uses R5's `TerminalDelivery` argument | PASS |
| `pg_durable_verifier_work::cancellation_reads_back_from_state_after_the_queue_and_cards_are_gone` (adapted) | — | PASS |
| Modules after the final edit: `pg_verification_runtime`, `pg_durable_verifier_work`, `pg_verification_groups`, wyrd-sql `pg_verifier_executions` | — | PASS: wyrd-server 83/83 (`-E "test(/^pg_verification_runtime::/) \| test(/^pg_durable_verifier_work::/) \| test(/^pg_verification_groups::/)"`), wyrd-sql 34/34 (`-E "test(/^pg_verifier_executions::/)"`) |

Checks after the final edit: `mise run fmt`, `mise run lints`,
`mise run check:tenant-isolation`, `mise run codegen:check` and
`git diff --check` all pass. `cargo check -p wyrd-testing --tests --all-features`
compiles.

### Requirement closure

| Obligation | Implementation | Proof | Result |
|---|---|---|---|
| USER-TERMINAL-002 rollback first | Rollback above | `git diff --stat d89356985 -- crates` = migration only | PASS |
| USER-TERMINAL-001 / FIND17 one operation, no flag | `conclude` → `settle`/`settle_delivery`; inventory above | P1, P2, P4, P5 | PASS |
| An ACKed attempt failure owes no second pass | `settle` `Terminate` → `TerminalDelivery::Acknowledged` | P1 (commit log: no owed commit, exactly one terminal commit) | PASS |
| A failed termination commit recovers the accepted failure, no engine | `execute` `Offer::Accepted` | P2 (accepted before reclaim, winner entered 0) | PASS |
| FIND19 exhausted recovery under outage | unchanged `state_failure` / `busy` mapping | P3 | PASS (capacity: see Limits) |
| Every producer reaches the operation after restart; missing SYSTEM or state leaves it owed | delivery claims → `deliver_owed` → `record` | P4 (cancel, revoke, fenced admission, refusal, refused group; Scribe grant revoked; SYSTEM hidden; retirement fault) | PASS |
| Revocation stops computation atomically while Bifrost is down | `cancel_principal` | P4 (`terminate` on the live lease → `StaleLease` with the grant revoked) | PASS |
| A selected cancellation waits for all result ACKs; a stale attempt cannot retire it | `conclude` Selected → `deliver`; claim fence | P5, SQL fence test | PASS |
| FIND16 group/owner identity | `PendingDelivery` custody | T3 | PASS |
| FIND22 checkpoint charge | `scribe_outbox.rs` / `state.rs` charge | T5 | PASS |
| FIND23/26 static | rustdoc `# Errors`/`# Panics`; `reserved 10` / `"state"` | `codegen:check`, inspection | PASS |
| Dependent release | unchanged | T6 | PASS |
| Queued and direct success | unchanged direct fast return | P6 | PASS |

### Diagnoses

- **P1 final-retry case.** Symptom: the row went `errored` while the EVENTS frame
  was held. Evidence: the row settled through `retry` → `RetryOutcome::Exhausted`
  with delivery owed. Cause: the test raised `max_attempts` after the claim, so
  the attempt saw a nonfinal Retry and SQL `retry` exhausted it. That is the
  owed branch, not the final-attempt path. Fix site: the test. It now reaches the
  final attempt naturally (two unheld retries, expire between). No assertion
  was weakened.
- **P5 durable row count.** Symptom: 2 RESULTS / 3 OUTPUTS rows where the test
  asserted 1. Cause: each separate delivery stages new batches; Scribe dedup
  covers only replays of identical sealed batches. This is existing behavior,
  and the existing reclaim test also sends twice. The packet says not to remove
  legitimate retries to pass a write-count assertion. Fix site: the assertion
  now requires every sent and durable row to carry the selected result ID.
- **`cancellation_reads_back…`.** Symptom: "the state holds the cancellation"
  failed. Cause: R5 removes the request-side retention the test relied on.
  Fix site: the test runs the delivery runtime before reading state.
- **P4 enqueue 504.** Cause: revoking the Scribe grant before enqueue blocked
  the enqueue's own ACK. Fix: the test's outage covers only revocation and
  delivery.
- **P4 SYSTEM suspension.** Cause: `auth_service_accounts_system_active_check`
  forbids suspending SYSTEM. Fix: the test hides it via `principal_kind`.
- **P4 stuck delivery after the retirement fault.** Cause: the failed claim's
  fence holds for the default delivery lease. Fix: the test expires
  `delivery_due_at`.
- **P4 group verifier.** Cause: the group refusal carries the group-end
  binding's Verifier. Fix: the test compares against the row's frozen
  `verifier_card_uid`.
- **Lints after GREEN.** (1) `too_many_arguments` on `execute`: R5 had added an
  `abandon` argument down `execute` → `record` → `conclude` → `deliver`. Both
  callers already race `abandon` in a biased select, and `HeldLease::lost` is
  its child, so the argument was redundant. It was removed rather than
  suppressed, which also removed `conclude`'s existing `#[expect]`.
  (2) `cast_sign_loss` in P1: replaced with a literal claim count. All GREEN
  modules were re-run after this edit.

### Local choices

- Delivery claims take at most `limit.div_ceil(2)` per pass.
- An unACKed or dropped result defers (`RESULT_UNDELIVERED`); a delivery waits
  at most `DECISION_TIMEOUT` (30 s).
- Missing SYSTEM or identity → `Owe`; unreadable state or a full reserved share
  → defer/redeliver.
- A delivery reads state as SYSTEM.
- A delivery of an accepted selection retires delivery and leaves the queue
  status.
- A refused target goes through `record`.
- Tests: a test-only commit-log trigger (`public.queue_commits`), and
  `pg_verification_groups::Support` made `pub(crate)` for the P4 group case.

### Non-goals kept out

No new finalizer, service, queue, writer, migration, dependency, feature,
export, wire field or public status. No change to direct synchronous success,
suspension/wait/approval (TASK-007), or Scribe dedup semantics. No commit.

### Limits

- **P3 capacity case not exercised.** The outbox reserved share is about
  1 GiB / 1M items (`SCRIBE_BYTES`, `SCRIBE_CAPACITY`) with no test seam. A
  refused reservation maps to `busy()`/defer in `execute` and `record`
  (source-inspected), but no test fills it. Closing this needs a new
  test-support capacity seam, which the packet does not authorize.
- Several deliveries of one selected result write several Scribe rows under
  one result ID (observation, existing behavior; see the P5 diagnosis).
