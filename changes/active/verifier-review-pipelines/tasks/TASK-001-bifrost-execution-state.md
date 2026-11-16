---
id: TASK-001
kind: architecture_refactor
spec: SPEC-verifier-review-pipelines
spec_revision: 18
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
status: complete
previous_id: TASK-001-R2
historical_parent: TASK-001@spec16
depends_on: []
replaces: TASK-001-R1 lifecycle scenarios 12-17
req: [REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-031, REQ-032, REQ-046, REQ-047, REQ-048, REQ-049, REQ-052, REQ-053, REQ-054, REQ-055, REQ-056, REQ-057, REQ-058, REQ-059]
inv: [INV-002, INV-003, INV-004, INV-005, INV-009, INV-010, INV-011, INV-015, INV-017, INV-018, INV-019]
ac: [AC-005, AC-006, AC-007, AC-008, AC-009, AC-024, AC-025, AC-039, AC-040, AC-041, AC-046, AC-047, AC-048, AC-049, AC-050, AC-051]
dur: [DUR-001, DUR-002, DUR-003, DUR-004, DUR-005, DUR-006, DUR-007, DUR-008, DUR-009, DUR-010]
---

# Recover verification execution state through Bifrost

## Outcome and Value

Refactor the implemented revision-16 foundation so both verification paths use
engine-owned in-memory state recovered from Bifrost. PostgreSQL coordinates the
queue instead of retaining a second execution-state copy. Complete this task
before Tasks 002–006 build on the foundation.

Before: a queued restart reads PostgreSQL invocation/publication records, while
direct verification has no checkpoint adapter. After: every engine entry loads
accepted Bifrost state or establishes its absence; settled results and exact
publications survive retry without repeating successful work.

This is architecture work under newly approved behavior, not a new verdict on
original review findings. The historical Task 001 and implemented portion of Task 001-R1
are the source baseline. Preserve their valid fixes; independently verify the
reported closure. The old scenarios 12–17 are superseded, including scenario
12's implemented no-direct-checkpoints assertion. Immutable reviews stay intact.

## Revision 1 — Preserve concurrency while checkpoints are pending

Requested 2026-11-05 against implementation candidate
`02e51db6150da543d426b93a05e4372113146b8c`. Status: pending implementation
and proof. This section supplements the original task under approved spec
revision 18 and durability supplement revision 2. It is not a task-review
verdict. Earlier implementation evidence below predates this revision.

### Diagnosis and intended outcome

Step execution is still concurrent. The bottleneck is `RunState::change` in
`crates/wyrd/wyrd-server/src/verification/state.rs`: callers hold the same
per-execution mutex while confirming the lease, encoding a complete state,
reserving outbox capacity and waiting for Scribe acknowledgement. A sibling
step can execute its action concurrently but cannot record its own progress
during that wait. Each mutation also clones and encodes the growing full state.

For example, factual check A and sensitive-information check B run in parallel;
check C combines their results. If A completes while its checkpoint is pending,
B must still be able to record its own completion in memory. C starts only once
an accepted checkpoint covers both results. If both changes are captured
together, one checkpoint acknowledges both. A crash restores accepted results;
an unsettled invocation may repeat under the existing attempt policy.

Preserve the current engine, complete-checkpoint format, exact accepted-state
recovery, expected-next-revision fence and takeover/replay guarantees. The
existing supplement explicitly permits independent changes to share a batch.
No new specification revision or weaker durability guarantee is required.

### Selected correction

1. Keep the existing per-execution `tokio::sync::Mutex<Held>` in `RunState`.
   Restrict its critical sections to validating and applying memory transitions,
   assigning local mutation tickets, capturing a consistent immutable snapshot,
   and updating accepted/pending checkpoint bookkeeping. Publication replay,
   name conflicts, budgets and terminal selection use the same memory authority.
   Release the lock before lease queries, capacity waits, serialization, staging
   IO or acknowledgement waits. Snapshot copying itself remains a measurable
   cost under the lock.
2. Separate local mutation tickets from persisted checkpoint revisions. A ticket
   identifies the memory change a caller needs preserved. A frozen checkpoint
   records the last ticket it includes and receives the next persisted revision.
   Allocate revisions per frozen checkpoint, rather than per memory mutation;
   tickets and coverage bookkeeping are private adapter state, not a wire or
   persisted-format extension.
3. Allow one frozen checkpoint per execution to be pending. Further transitions
   update live memory and accumulate for the next snapshot. Reuse one shared
   pending operation/receipt so callers awaiting covered changes share the same
   write. After acceptance, remaining callers checkpoint the accumulated state
   together. Clone and encode once per frozen checkpoint, rather than once per
   waiting caller. Do not retain an unbounded queue of full snapshots.
4. Keep persistence in the existing Scribe outbox. Waiting callers share and
   drive checkpoint preparation/acceptance through `RunState`; add no per-run
   background writer, actor queue or independent flush service. A dropped
   waiter must leave other waiters able to drive the pending operation. Once
   staged, immutable payload and retry identity remain owned by the outbox.
   If every waiter is cancelled, unaccepted progress cannot authorize an effect
   or silently resume direct execution in the background.
5. Release a caller only when an accepted snapshot covers its ticket. Changes
   made after a snapshot was frozen await the next checkpoint. Live state feeds
   execution; pending publication state must not produce an accepted replay
   receipt, downstream content release or terminal success. Retain the existing
   WAL barriers before external dispatch, dependent release, publication success
   and final success. Never wait for Iceberg or object publication.
6. Extend the existing server Scribe adapter for independent-run throughput:
   lane checkpoint writes by execution identity across the existing lanes, and
   acknowledge each checkpoint at its own frame's durable acceptance boundary,
   independently of unrelated frames and group scheduling. Preserve the existing
   result/group acknowledgement contracts. Prepare frozen checkpoint JSON/Arrow
   through the existing bounded writer preparation path. Stable checkpoint
   identity and exact-next acceptance survive retries; one pending checkpoint
   per execution avoids out-of-order same-execution submissions.
7. On refused or uncertain acceptance, stop that writer's consequential progress
   and recover through the existing authoritative claim/read path. Do not mark
   a shared checkpoint failed merely because one waiter was cancelled. Preserve
   original deadlines, lease cancellation, takeover fencing, reserved capacity,
   byte accounting, bounded backpressure and outbox shutdown/drain semantics.
   Bifrost retains generic checkpoint/fence responsibilities; graph scheduling
   and dependency decisions remain engine-owned.

### Ordered revision scenarios

#### R1.1 — Record sibling progress during a checkpoint wait

**Behavior.** Hold A's checkpoint acknowledgement. B records its own transition
and unrelated ready work continues without waiting for A's memory lock. A
separate execution can progress through another existing outbox lane. No caller
crosses its own durability boundary early.

**RED.** Add deterministic held-acknowledgement proof in the existing server
unit target. Show that the current mutex prevents B from recording progress.
Use controlled futures and acknowledgements, not timing sleeps or host load.

**GREEN.** Restrict locking to memory coordination and move durability waits
outside it. Exercise both direct and queued composition where the changed
adapter differs; preserve the existing engine concurrency limits.

**REFACTOR.** Keep one state authority and the existing checkpoint interface;
do not replace the engine or add another runtime task to hide lock contention.

#### R1.2 — Coalesce changes with exact acknowledgement coverage

**Behavior.** Freeze a snapshot containing A, then record B and other independent
changes while its acknowledgement is held. The first acceptance covers only A;
one subsequent checkpoint captures the accumulated changes. A snapshot frozen
after both A and B includes both. Concurrent duplicate publication/terminal
requests share the selected identity and await its durability. Cancellation of
one waiter leaves others able to finish; failure or timeout releases no success.

**RED.** Add coverage, checkpoint-count, replay-receipt and waiter-cancellation
assertions through the real outbox with controlled Scribe responses. Verify one
pending checkpoint per execution, bounded reservations and stable retry identity.
Record the exact focused selectors before running RED.

**GREEN.** Coalesce at `RunState`, preserve the current complete-state payload
and expected-next fence, and acknowledge checkpoint frames independently of
unrelated slice work. Keep accepted publication visibility separate from pending
memory changes. Retry the same frozen write without rebuilding it from newer
live state.

**REFACTOR.** Reuse existing outbox delivery, capacity, encoding and drain
owners. Remove per-caller full-state cloning/encoding and avoid a second batching
layer or a changed Bifrost acceptance protocol.

#### R1.3 — Recover the exact accepted batch and retain boundaries

**Behavior.** Run concurrent A/B followed by dependent C. Interrupt before any
checkpoint acceptance, after acceptance covering only A, and after acceptance
covering both. Recovery reuses exactly the accepted settlements and conserved
attempts; C never consumes unaccepted results. Lose an acknowledgement, cancel
the attempt, and replace the writer: stable identity, exact recovery and stale
write rejection remain intact. Publication and terminal replies remain gated
by their own accepted coverage.

**RED.** Extend the existing server integration/outbox seams to expose premature
coverage or stranded pending work under cancellation. Retain the existing exact
recovery and takeover/WAL-replay proofs as regression evidence; do not manufacture
RED for unchanged fence behavior.

**GREEN.** Propagate accepted coverage and failures through all checkpoint,
publication, inspection and terminal consumers, preserving existing attempt,
permission, lease and direct-disconnect semantics.

**REFACTOR.** Keep all recovery at the accepted Bifrost checkpoint. No second
recovery store, per-step delta format, background direct resumption or new lease
owner is introduced.

### Revision acceptance and proof

These observations extend scenarios 1/3/4/5/6 under REQ-052–057, INV-017–019
and DUR-001–004/006–010. A task-review PASS remains outstanding.

- Independent memory transitions proceed during a held checkpoint acknowledgement;
  no lock spans lease/storage IO, capacity waits or checkpoint acknowledgement.
- Checkpoint count follows frozen batches, with exact per-caller coverage and one
  pending immutable checkpoint per execution. Pending changes never masquerade
  as accepted publication receipts or terminal outcomes.
- Concurrent runs use existing lanes and bounded shared writer capacity. A
  checkpoint receipt does not wait for unrelated frame/group completion.
- Waiter cancellation, lost replies, writer takeover and shutdown cannot strand
  another caller's required checkpoint or release unaccepted effects. Existing
  exact-next fencing, retained-volume replay and complete recovery stay intact.

Use focused new tests in the existing server library and integration targets.
The following module/prefix selectors are planned: confirm actual registration
and record every scenario's exact test selector and RED/GREEN result before
claiming implementation evidence. Empty selections do not count.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::state::tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^scribe_outbox::tests::/)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(/^pg_durable_verifier_work::batched_checkpoints_/)"'
```

Rerun the existing [exact-recovery](#exact-recovery-proof),
[takeover/replay](#takeover-replay-proof),
[invocation-recovery](#invocation-recovery-proof),
[publication-state](#publication-state-proof) and
[terminal/inspection](#terminal-inspection-proof) proofs for affected behavior,
plus format/lints and the narrow checks covering the actual write set. Full
language journeys and aggregate gates remain integrated change-review work.
Record actual snapshot sizes/counts, encoded bytes, acknowledgement latency and
concurrent-run progress using the existing bounded workload/test facilities;
report observations rather than claiming zero throughput cost or generating
synthetic host load. Full snapshots still grow with run history; this revision
does not claim unlimited per-run scalability.

## What Changes and What Must Stay True

- Keep the existing engine/walker/ledger, logical settlements, File upload,
  shared Scribe outbox, result builder, query and queue owners. No second writer,
  recovery store, Agent loop, archive service or lease scheduler.
- Persist complete versioned logical checkpoints at consequential boundaries.
  Large bytes remain in existing storage. Complete state includes exact pinned
  context, attempts, all settled outcomes, budgets/deadline, accepted publication
  identity/private descriptors and selected terminal result. Do not serialize
  runtime futures, credentials or model conversations.
- Extend existing Bifrost read and acceptance/fence owners. Recovery loads the
  exact accepted compatible checkpoint, authoritative absence, or an explicit
  retryable failure. A successful ordinary query that omitted an offline owner
  cannot authorize starting fresh. Takeover shuts out the prior writer, and
  rejected WAL records never become accepted after restart.
- Reuse current WAL acknowledgements and reserved shared-writer capacity. No
  object-publication/Iceberg wait and no acknowledgement of process-memory
  enqueue. Batch across executions without losing individual acceptance results.
- Keep PostgreSQL queue claims/leases/renewal and initial input custody. Original
  deadlines survive retries; lease renewal does not extend them. Fresh direct
  requests remain distinct and queue-free; disconnect stops request work without
  background resumption. Schedule/group activation remains unchanged.
- Preserve existing publication names, {name,kind}, attempt scopes, call replay,
  byte/count limits, exact descriptors and successful-producer selection. This
  task owns the backend; Task 002 owns real Agent tool exposure.
- Preserve public status/error/cancel/page contracts and feature permissions.
  Remove old PostgreSQL execution-payload dependencies without replacing them
  with permanent tombstones, unsynchronized group comparisons, whole-group
  serialization or another cleanup/handoff protocol.
- Preserve existing evidence/session retention, independent control/delivery
  lifetimes, typed errors, tenant isolation and non-blocking audit. Bifrost's
  internal catalog/fence SQL is not an application recovery-state mirror.

The material cost is durable checkpoint acknowledgement and complete-state
encoding. Use bounded existing batching and backpressure; do not claim a QPS
improvement or weaken correctness to match a request benchmark.

## Approach

1. Map the existing logical state and public consumers onto the approved
   Bifrost representation, preserving selected identities and format safety.
2. Extend existing authoritative recovery read, accepted-position and writer
   fence/replay behavior; prove unavailable and stale cases before engine use.
3. Connect both direct and queued execution to the same state/checkpoint path
   and existing reserved outbox delivery.
4. Move accepted publication/replay/accounting and downstream resolution into
   run state while retaining the complete shared upload/read workflow.
5. Preserve terminal delivery, inspection, cancellation, group/replay/binding
   and independent Operator consumers under one execution-state source.
6. Reconcile old stored state and worker-format compatibility without erasing
   custody or silently executing a previously progressed run as fresh; remove
   obsolete steady-state PostgreSQL recovery writes/reads.
7. Verify focused behavior and original-fix regressions; record remaining
   integrated proof obligations for change review.

## Ordered Implementation Scenarios

### 1. Use one logical state for direct and queued runs

**Behavior.** Execute equivalent direct and queued verification with a dependency
A → B. Both enter the same loader and checkpoint path; direct creates no queue
record. B consumes A's complete in-memory result. The queue retries with the
original execution identity. Fresh direct calls remain distinct.

**RED.** Add [shared-state proof](#shared-state-proof), checking actual state
loads/writes, result values, queue admission counts and dependency reads. Current
direct has no adapter and queued startup reads PostgreSQL checkpoints. Confirm
the planned selector before RED.

**GREEN.** Consolidate existing ledger/checkpoint state and bind both paths to
its Bifrost host adapter. Preserve input custody, authority and original bounds.

**REFACTOR.** Delete redundant state copies/private loaders. Keep the existing
engine and request/poller entry owners.

### 2. Load exactly the accepted state or fail closed

**Behavior.** A complete checkpoint restores all required fields through WAL,
staging and publication. An absent execution starts fresh only when absence is
authoritative. Stop a required Scribe before discovery or delay the accepted
record: recovery waits/fails without older/empty fallback. Corrupt or incompatible
state refuses without alteration. Permanent unpublished-volume loss is outside
the supported recovery guarantee; do not silently lower it to absence.

**RED.** Add [exact-recovery proof](#exact-recovery-proof) using existing Scribe
restart/source controls. Assert exact accepted revision/values or typed failure.
Current ordinary reads can return successful partial coverage and current
verification records lack complete recovery state.

**GREEN.** Extend existing Bifrost recovery reading and canonical state payloads,
using durable source/batch metadata and exact accepted position. Preserve normal
analytical query semantics; do not make all queries execution-state lookups.

**REFACTOR.** Reuse existing catalog/live/replay owners; no execution-side source
registry, object listing protocol or alternate store.

### 3. Fence late checkpoints through takeover and WAL replay

**Behavior.** Race A's checkpoint against B's takeover. Progress accepted first
is inherited; once takeover wins A cannot advance recoverable state. Reject
expired/cancelled/revoked authority as required. Lose acceptance replies and
restart Scribe after rejection: accepted identity remains stable and rejected
progress never resurrects as an unfenced WAL commit.

**RED.** Add [takeover/replay proof](#takeover-replay-proof). Assert both race
orders, exact accepted position, ambiguous replay and stale-write exclusion after
restart. Existing transport-batch fences do not establish execution ownership.

**GREEN.** Extend declared Bifrost acceptance/fence/replay boundaries for current
writer and expected accepted revision, preserving the PostgreSQL queue's lease
owner. Per-checkpoint acceptance results remain correct inside shared batches.

**REFACTOR.** Keep one existing write owner. No second lease scheduler, private
cross-tier SQL import or transaction held across provider/storage IO.

### 4. Resume settled work and conserve interrupted attempts

**Behavior.** Recover settled actions and requirements, including settled
failures. Count started-but-unsettled work as spent; retry only if policy allows.
Preserve original deadline, cumulative limits, cancellation and no-spend capacity
refusal. Pause durable start/settlement acknowledgements: neither the external
call nor its dependent escapes early.

**RED.** Add [invocation-recovery proof](#invocation-recovery-proof) against real
Bifrost state and no PostgreSQL invocation payload reads/writes. Assert exact
provider call counts, values and bounds. Existing pure recovery regression alone
is not proof of the new storage seam.

**GREEN.** Reuse existing begin/forget/settle semantics and error classification
through the common state owner and reserved outbox acknowledgements.

**REFACTOR.** Keep the existing logical recovery algorithm and pure regression;
remove the PostgreSQL checkpoint adapter rather than running both adapters.

### 5. Recover publication acceptance and exact File references

**Behavior.** Use trusted hosted calls to publish files, directories, JSON/null.
A same-call replay survives lost reply and removed scratch; conflicting reuse
refuses and new attempts retain separate name scope. Restore exact private File
descriptors and cumulative accounting, then mount/read without inventory lookup.
A crash after upload but before acceptance exposes no accepted partial content;
acceptance before invocation failure survives with the original attempt status.

**RED.** Add [publication-state proof](#publication-state-proof). Assert exact
bytes/receipt/descriptor identity, limits and stale refusals after recovering
Bifrost state with no PostgreSQL publication payload rows. Existing storage
completion works, but acceptance/readback currently queries PostgreSQL.

**GREEN.** Keep shared freeze/upload/reconcile/inventory/read machinery. Retain
its accepted publication result in engine state and checkpoint before receipt.
Direct backend support uses the same lifecycle; actual Agent tool wiring remains
Task 002, not a new publication service in this task.

**REFACTOR.** Delete duplicate descriptor custody and SQL rediscovery; do not
replace complete upload orchestration or introduce an Artifact Card intermediary.

### 6. Preserve terminal result and public inspection

**Behavior.** Lose replies or crash between detail, summary, selected terminal
state and queue completion. Recovery returns the same selected result and finishes
required delivery without provider reruns. Read all outcomes/full errors, stable
invocation/publication pages, exact downloads and repeated terminal cancellation
without legacy PostgreSQL payload rows; unavailable sources never look deleted.

**RED.** Add [terminal/inspection proof](#terminal-inspection-proof), asserting
selected identity, exact response/cursor membership, current read/run grants,
source Card deletion and negative/cross-tenant cases. Current inspection and
selection depend on PostgreSQL execution payloads.

**GREEN.** Reuse result builder/outbox and public reader contracts with accepted
Bifrost state. Preserve stable physical write identities and cancellation
semantics; partial delivery is recoverable before terminal success.

**REFACTOR.** Remove phase-based archive selection and event backlog dependence.
No new result format, public backend parameter or separate finalization service.

### 7. Preserve queue, group and independent delivery behavior

**Behavior.** Retry scheduled/group work with unchanged original window and pinned
inputs. Replay matching/conflicting admission keys, deduplicate group snapshots,
compare same-binding/same-group previous verdicts, and change a verdict while
Operator delivery is held. Binding summaries remain correct and delivery retry
never enters the engine or depends on a removed payload parent.

**RED.** Add [consumer-continuity proof](#consumer-continuity-proof). Assert
baseline/unchanged/changed/inconclusive comparison and exactly the existing
logical dispatch behavior, concurrent completion ordering, replay conflicts and
lookup failure. Current historical joins depend on execution rows.

**GREEN.** Adapt each existing owner to canonical execution identity/state while
retaining admission serialization, group completion semantics and independent
queue/delivery/config lifetimes. Queue retries still use the original identity.

**REFACTOR.** Remove obsolete joins/FKs only after their behavior is covered. No
permanent terminal tombstones, whole-group execution serialization, copied
queue or archive cleanup protocol.

### 8. Preserve old custody and close original regressions

**Behavior.** Existing/incompatible stored execution formats do not become fresh
runs or lose accepted settlements/publications during the refactor. Compatible
recovery preserves custody; incompatible workers refuse without mutation. The
original 15 fixes remain satisfied after their persistence owner changes.

**RED.** Add [format/custody proof](#format-custody-proof) for the Bifrost state
boundary. Assert that legacy progressed state is never interpreted as absent.
Retain the existing original-fix tests as regression or verification-only proof;
do not manufacture RED for already-correct behavior.

**GREEN.** Use existing worker-format and versioned-contract owners to preserve
custody and refuse unsafe reinterpretation. Remove obsolete steady-state SQL
payload paths only when consumer/recovery proofs pass. Record how each original
finding's invariant is covered; this is not an automatic PASS on historical work.

**REFACTOR.** Remove dead adapters/queries after safe replacement; no permanent
compatibility shim or second recovery source in the completed architecture.

## Acceptance Criteria

| Obligations | Proof and expected observation |
| --- | --- |
| REQ-052–053; INV-017/019; AC-046–047; DUR-001–002 | Scenarios 1/4 plus SDK journey: common state path, memory-fed dependencies, original identity/bounds; direct remains queue-free. |
| REQ-054–057; INV-018; AC-048–049; DUR-003–004/006–007/009–010 | Scenarios 2/3/6: exact authoritative loading, ordered acceptance/takeover, rejection surviving replay, stable result and permissions; unavailable state never creates fresh work. |
| REQ-010–015/031/046–049/058; INV-004/005/015; AC-005–009/024–025/039–041/050; DUR-005/008 | Scenarios 4/5/6/8: original replay, limits, complete settlement/descriptor, deadlines, stable pages, visibility and negative-case regressions remain. Task 002 supplies actual Agent-tool exposure. |
| REQ-059; AC-051; DUR-001/009 | Scenario 7: original schedule/group inputs and comparison/dedup/dispatch behavior, binding summaries and independent delivery remain; no new serialized execution policy. |
| INV-002/003/009/010/011 | Scenarios 2/3/6/8 and focused owner checks: isolation, reduced/current authority, non-blocking audit, portable implementation and safe formats. |
| REQ-032 public projection contribution | Focused SDK journey and unchanged-schema/reader checks here; existing per-surface cases and integrated change review exercise shared read/cancel/publication behavior. No new SDK lifecycle or API. |

## Implementation Reference

Source baseline inspected at `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb`.
Paths guide ownership, not a private implementation allowlist.

| Inspected owner and consumers | Existing workflow, gap and selected extension |
| --- | --- |
| `vala-verify::VerificationEngine::with_checkpoints/begin/commit`, `walk::RunLedger`, `Scope`; `blocks/engine/tests.rs` | Already restore complete logical settlements and feed dependencies in memory. Consolidate recoverable state at this owner and retain current attempt semantics. |
| Server `verification/runner.rs::ExecutionCheckpoints`, `components/verification/service.rs::run_direct/stage_direct` | Queued adapter opens SQL per update; direct has no adapter and stages without receipts. Replace host composition with one Bifrost state flow; keep request/poller owners. |
| `scribe_outbox.rs::ScribeOutbox/ScribeWrite/ScribeSink::frames/write`, runtime `Outbox::reserve_until` | Existing bounded batching, retry identities and replies. Extend checkpoint projection and acceptance receipts; no additional writer or generic drop-on-full staging. |
| `vala-bifrost-redux` contracts, Scribe `append_and_sync_batch_commits/commit_batch_control_fences` and WAL replay; `vala-sql::scribe_batch_commits` | Existing WAL/fence/active insertion supplies durability. Unfenced WAL can currently restore; batch dedup does not fence execution owners. Extend declared acceptance and replay under approved contract without cross-owner private imports. |
| Oracle source selection and existing Scribe/catalog/batch metadata; `wyrd-testing` lifecycle/distributed journeys | Current analytical discovery can omit offline owners. Add authoritative exact execution recovery semantics at the existing owner, retaining unrelated analytical behavior and restart machinery. |
| Hosted `publications.rs::path/complete/receipt/accept`, `evidence.rs::resolve`, shared File completion and inventory | Existing full upload/read workflow is correct reuse; acceptance/accounting/descriptor lookup is SQL-backed. Move custody into run state and reuse returned exact descriptors. |
| `runner.rs::result/deliver`, result payload builder, `inspection.rs`, public Execution client | Preserve detail/summary and API selectors/grants; remove SQL-selected outcome and invocation/publication reads in favor of accepted state. |
| Queue `claim/renew/complete`, group snapshot and previous-verdict queries, bindings, Operator dispatch claims | Retain SQL queue/control ownership. Historical state joins, result selection and delivery-parent coupling need adaptation; no extra group scheduler or tombstones. |
| Existing worker-format and original replay/budget/page/revocation/deadline tests | Preserve logical invariants, replace architecture-specific PG-only assertions and add Bifrost seams. Historical test success is not evidence of the new architecture. |

## Verification and Evidence

All new selectors below are **planned**, in confirmed existing Cargo targets.
Add the smallest suitable tests/modules, list their actual selectors before RED,
and update the command if names change; zero selected tests is not proof.
Server integration uses existing `pg_durable_verifier_work` registration;
Scribe uses existing `lifecycle`; SDK uses existing `verify_in_real_time`.
Repository-managed Postgres setup and current owner checks are in
[shared verification](verification.md). Run Cargo-backed commands sequentially.

### Shared-state proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::direct_and_queued_use_bifrost_run_state)"'
```

### Exact-recovery proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all --test-threads=1 -E "test(=lifecycle::execution_recovery_requires_exact_accepted_state)"'
```

### Takeover-replay proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all --test-threads=1 -E "test(=lifecycle::execution_takeover_rejects_late_writes_after_replay)"'
```

### Invocation-recovery proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::invocations_resume_from_bifrost_with_original_bounds)"'
```

### Publication-state proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::publications_recover_exact_descriptors_from_run_state)"'
```

### Terminal-inspection proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::terminal_selection_and_inspection_use_run_state)"'
```

### Consumer-continuity proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::run_state_preserves_groups_replay_and_operator_delivery)"'
```

### Format-custody proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::state_format_changes_preserve_existing_custody)"'
```

### Public SDK journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verify_in_real_time::verify_and_submit_share_durable_execution_state)"'
```

Drive direct/submit/read/reconnect/cancel with a real SDK and server; assert
results, original retry identity, typed source failures and no direct queue
admission. This journey closes scenarios 1/4/6 at a user-facing boundary.

### Existing pure semantic regression

Existing selector confirmed from source; run after changed invocation behavior.

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::checkpoints_restore_settled_blocks_and_count_interrupted_attempts)'
```

Use the original finding inventory as regression obligations; record each actual
current selector before execution rather than assuming old planned tests exist.
Purely static wiring/stub/rustdoc corrections and already-correct behavior use
static/regression proof, not manufactured RED. Apply only changed-owner checks
from shared verification; complete language/capability suites run once at final
change review. No tests or benchmark results are claimed by this plan.

## Material Stop Conditions

Return to specification authority if the approved existing-owner extensions
cannot provide exact authoritative recovery or acceptance/replay ordering;
do not add another database, registry, writer or lease scheduler as a fallback.
Do not choose a new public API, incompatible data-loss policy, retention period,
whole-group serialization or private cross-tier transaction to make tests pass.
Legacy state must never be silently erased or interpreted as fresh. A safe
migration/format incompatibility discovery is material when it changes approved
behavior; ordinary lossless schema/wiring work remains implementation-owned.

## Authority Links

[Approved spec revision 18](../spec.md) ·
[Approved durability supplement revision 2](../execution-state-and-durability.md) ·
[Publication contract](../output-publication.md) · [Public contracts](../public-contracts.md) ·
[Historical R1](../review/task-001-r1-365bfc94b/TASK-001-R1-durable-execution-guarantees.md) ·
[Task map](README.md).

[Wyrd design](../../../../architecture/wyrd-design.md) ·
[Bifrost design](../../../../architecture/bifrost-design.md) ·
[Security](../../../../architecture/wyrd-security-posture.md) ·
[Implementation execution](../../../implementation-execution.md) ·
[Testing](../../../../TESTING.md). Apply nested AGENTS.md at each implementation owner.

## Implementation Evidence — Before Revision 1

Commits: `a9f0b6261` (Scribe state fence), `36f16ef16` (execution state in
Bifrost), `3e40b5873`, `2ede5f271`, `02e51db61` (proofs). Every server and
journey command ran under `scripts/postgres/with-test-postgres.sh` after
`db:migrate:all:inner`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scenario 1, REQ-052–053 / AC-046–047: direct and queued runs share one state path; direct is queue-free; fresh direct ids are distinct | `wyrd-server/src/verification/state.rs::RunState` (`claim`, `fresh`, `Checkpoints` impl); `runner.rs::execute/state`; `service.rs::run_direct` | `pg_durable_verifier_work::direct_and_queued_use_bifrost_run_state`; `wyrd-ai verify_in_real_time::verify_and_submit_share_durable_execution_state` | PASS |
| Scenario 2, REQ-054–057 / DUR-003–004: load exactly the accepted revision or fail closed; never fall back to fresh or older state | `RunState::claim/read` (head revision r → exact `sequence = r` row, missing → `Unavailable`); `runner.rs::state_failure` (Unavailable → Defer) | `scribe lifecycle::execution_recovery_requires_exact_accepted_state` (head ahead of the visible row → Unavailable; exact snapshot after takeover and after a WAL-replay restart) | PASS |
| Scenario 3, DUR-006–007: takeover fences the old writer; a refused write never resurrects through WAL replay | `vala.scribe_state_heads` claim/advance (`a9f0b6261`); writer = lease token, generation = attempt, `not_after` = original deadline | `scribe lifecycle::execution_takeover_rejects_late_writes_after_replay` (after restart, sequences are exactly `[1, 2]`); `pg_durable_verifier_work::publication_replay_preserves_attempt_identity` | PASS |
| Scenario 4: settled/begun attempts resume and interrupted ones stay spent, under the original deadline; no PostgreSQL invocation payload | `Checkpoints for RunState` (begin/forget/settle); migration `20261107000000_verifier_execution_state_in_bifrost.sql` drops the invocation, publication and event tables | `pg_durable_verifier_work::invocations_resume_from_bifrost_with_original_bounds`; `wyrd-sql pg_verifier_executions::reclaims_keep_the_first_deadline`; `vala-verify blocks::engine::tests::checkpoints_restore_settled_blocks_and_count_interrupted_attempts` | PASS |
| Scenario 5: publications recover exact descriptors, replay receipts, limits and stale refusal from state | `RunState::accept_publication/publication_receipt/publications`; `hosted/publications.rs`, `hosted/evidence.rs` | `pg_durable_verifier_work::publications_recover_exact_descriptors_from_run_state`, `publication_limits_survive_reclaim`, `file_and_json_publications_are_immutable`, `directory_snapshot_is_complete_or_rejected`, `publication_mounts_preserve_status_and_lineage` | PASS |
| Scenario 6: the terminal selection is checkpointed before delivery; recovery re-delivers the same result id; inspection reads from state | `RunState::select`; `results.rs::build_as`; `service/inspection.rs` pages from state | `pg_durable_verifier_work::terminal_selection_and_inspection_use_run_state`; `pg_verification_runtime::reclaimed_attempt_after_a_failed_settlement_redelivers_the_selected_result` | PASS |
| Scenario 7: groups, replay and Operator delivery continue | Queue, group and delivery owners unchanged, apart from the removal of the history projector | No dedicated `run_state_preserves_groups_replay_and_operator_delivery` test; covered by the existing suites `pg_verification_groups::*`, `pg_operator_delivery::*` and `pg_verification_runtime::*` (94/94 together with the suites above) | PASS (existing selectors) |
| Scenario 8: old and incompatible formats keep custody and never become fresh | `WORKER_FORMAT = 2`; the claim skips newer formats; the migration RAISEs while format-1 invocation or publication rows exist; `ExecutionState::decode` refuses other formats (Incompatible → Defer) | `pg_durable_verifier_work::state_format_changes_preserve_existing_custody`; `pg_durable_verifier_work::cancelled_work_keeps_checkpoints_and_fences_stale_writes`; `wyrd-sql pg_verifier_executions::newer_worker_formats_keep_their_records`; `wyrd-spec` `only_the_current_format_decodes` | PASS |
| Format, lints, contracts, boundaries | `StateRefused` error; regenerated TS error codes and the docs error catalog | `mise run fmt`, `lints`, `codegen:check`, `docs:check`, `check:deps`, `check:tenant-isolation`, `git diff --check` | PASS |

Updated existing tests (their contract changed under REQ-013/scenario 6):
- `reclaimed_attempt_after_a_failed_settlement_selects_its_own_result` is
  renamed `…_redelivers_the_selected_result`. A reclaim now re-delivers the
  selected identity and does not mint a new one.
- `queued_origins_require_their_own_input_ack`: a direct verify can no longer
  finish while Scribe holds an ingest, because its state must be accepted. It
  now asserts the call completes once the held write is released.
- `sustained_scribe_outage_caps_tenant_spans`: the outage loop drives the
  span cap with traced reads. The direct-result assertion moved to after
  recovery, since memory-only staging no longer counts as success.
- Deleted tests that covered removed mechanisms: the history-projector
  journey `events_replay_without_hiding_live_progress` and the multi-row
  `ExecutionEvents` frame-identity unit test.

Diagnoses:
- Forged-format test row rejected `invalid`. Cause: the test used a random
  Card UID and Scribe denied it by card scope. Fixed in the test by using the
  execution's Cards.
- Fourth claim found nothing claimable. Cause: `max_attempts` was exhausted.
  After the restart the test reads the state without claiming.

Limits and ponytail notes:
- Each execution's revisions are serialized under one mutex held across the
  Scribe ack.
- Generation = attempt.
- The direct result write is staged after its selection is accepted; its own
  ack is not awaited.
- Not proven here: the A → B in-memory dependency through a hosted block
  Verifier, and lost-ack replay of a state revision.
- RED was not shown before GREEN: the proofs were written after the
  implementation.

Historical status: IMPLEMENTED at `02e51db61`. Revision 1 remains pending
implementation and proof; the evidence above does not establish its acceptance.

## Implementation Evidence — Revision 1

Candidate: `cd905f4cd` on `02e51db61`.

Reuse map: `RunState` stays the one memory owner. The existing outbox,
reservation, Scribe state fence and Arrow chunk encoding on the blocking pool
are reused. Gap: `RunState::change` held its mutex across lease, staging and
ack IO, and the outbox acked revisions only after whole-slice and group
scheduling. The extension is a per-execution ticket/coverage counter, one
`futures_util::Shared` pending revision, per-frame acks and lanes keyed by
`execution_id`. No new owner, queue, task or format.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Sibling memory transitions proceed during a held ack; no lock spans IO | `verification/state.rs` `RunState::change`/`durable` (lock covers only bookkeeping and the frozen clone; lease confirm, staging and ack run outside it) | `verification::state::tests::siblings_record_progress_while_a_revision_is_held` (RED on `02e51db61`: memory locked; GREEN) | PASS |
| Checkpoints follow frozen batches with exact per-caller coverage; one pending per execution | `Held::{changes, covered, pending}`, `Pending::covers` | `verification::state::tests::held_changes_share_the_next_revision_across_cancelled_waiters` (RED; GREEN: 3 changes → 2 revisions, first holds only A) | PASS |
| Waiter cancellation does not strand others or retract a revision | `Acceptance = Shared<…>` driven by any waiter; lease confirm moved to the freezing caller so a parked revision holds no connection | same test; `pg_durable_verifier_work::batched_checkpoints_recover_exactly_the_accepted_batch` | PASS |
| Pending publication/terminal state never answers as accepted | `select`, `publication_receipt`, `accept_publication` await coverage of the current ticket before answering | `verification::state::tests::duplicate_selections_share_one_accepted_result` | PASS |
| Refused/uncertain revision releases nothing and stops the writer | `durable` sets `broken` on any failed revision, lease or capacity | `verification::state::tests::a_refused_revision_releases_nothing` | PASS |
| Per-frame ack, independent of unrelated frames and group scheduling | `scribe_outbox.rs` `ScribeSink::tally` acks `ExecutionState` at its frame; end-of-slice ack now covers `Result` only | `scribe_outbox::tests::a_revision_is_acknowledged_at_its_own_frame` | PASS |
| Revisions lane by execution identity | `ScribeSink::lane` keys `ExecutionState` by `execution_id` | `scribe_outbox::tests::verification_items_share_execution_lanes_and_executions_spread` | PASS |
| Exact recovery of the accepted batch; C never sees unaccepted B; replaced writer refused | unchanged fence and claim/read path | `pg_durable_verifier_work::batched_checkpoints_recover_exactly_the_accepted_batch` | PASS |
| Existing recovery, takeover/replay, invocation, publication, terminal and format proofs | unchanged | all 12 `pg_durable_verifier_work::*`; 68-test run including `pg_verification_runtime::*` and `pg_verifier*` was green except this test's earlier version | PASS |

Commands:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::state::tests::/) | test(/^scribe_outbox::tests::/)'   # 25 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(/^pg_durable_verifier_work::/)"'   # 12 passed
mise run fmt && mise run lints   # clean
git diff --check                 # clean
```

Diagnosis (integration RED during GREEN):

- **Symptom:** `batched_checkpoints_…` timed out waiting for the held revision to reach Scribe.
- **Evidence:** the trace showed `renew_verifier_leases` with `elapsed=10.14s`, finishing at teardown.
- **Cause:** the shared revision future began with `lease.confirm()`, which is Postgres IO, and ran only while polled. After one poll it parked mid-query holding a pooled connection.
- **Independent diagnostician:** confirmed this cause and the production hazard, and recommended spawning the write. Spawning was rejected because the revision forbids a per-run background writer.
- **Fix site:** `RunState::durable`. The freezing caller now confirms the lease outside the lock, before freezing. The shared future holds only outbox capacity and the staged write. Every `durable` caller (`select`, the publication paths, `begin`/`forget`/`settle`) routes through it.

Measurements (observations from the bounded tests, no synthetic load):
- **Revisions:** 3 concurrent changes under a held ack take 2 revisions; A+B with a held ack take 2.
- **Ack latency:** one in-process Scribe ack was about 22 ms (`bifrost_scribe_ack_seconds_sum`).
- **Lock hold:** the mutex is held only to apply a change or clone the frozen state. That clone remains the cost under the lock.
- **Not measured:** encoded bytes per revision and throughput across concurrent runs.

Limits:
- **Snapshot size:** full snapshots still grow with run history.
- **JSON encoding:** checkpoint JSON is serialized once per frozen revision outside the lock, but not on the outbox's blocking pool. Only the Arrow encoding is.
- **Cancellation:** a revision whose every waiter was cancelled waits unpolled, holding no connection, until the next waiter or `RunState` drop.
- **Not rerun:** `vala-verify` is unchanged, so its engine regression was not rerun.

Non-goals stayed excluded: no new state format, wire contract, Bifrost acceptance change, background writer, or second recovery store. No `architecture/` or packet file is staged.

Status: IMPLEMENTED (Revision 1); task review outstanding.
