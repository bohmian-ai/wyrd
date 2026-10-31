---
id: TASK-009
task_revision: 2
kind: implementation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 24
requirements: [REQ-035, REQ-036, REQ-037, REQ-038, REQ-039, INV-001, INV-002, INV-003, INV-006, INV-007, INV-008, AC-015, AC-016, AC-017, AC-018, AC-019]
depends_on: []
revises: [TASK-006-R1]
---

# Consolidate queued verification and deliver the measured candidate

## Outcome and Value

Submitted, scheduled and grouped verification share one producer–consumer
pipeline: prepare the canonical input, publish through Scribe, receive its ACK,
accept the execution into the existing Postgres queue, and let the existing
distributed runner load Bifrost inputs and execute through the shared engine.
Scheduled/group work cannot be claimed before its selector is readable because
no runnable execution exists before that ACK. Inputs, detailed outputs and
payload-free result summaries remain in their separate Bifrost tables.

There is no temporary snapshot/preparation table and no alternate runner input
path. Existing schedule cursors and group pending rows remain until queue
acceptance. The maintainer accepts published-but-unaccepted input records and
repeated preparation after a prequeue crash; do not add durable machinery to
remove that trade-off. Once queued, execution retries use its fixed archived
input and existing lease/retry machinery.

This replaces the previous TASK-009 file, its revision-23 architecture and its
old readiness verdict. It is the single requested revision task, including
full-gate closure, all three engines' full L1 benchmarks, separate
benchmark-records publication and a code PR against main. No implementation,
gate, benchmark, commit or PR is claimed by this packet.

## What Changes and What Must Stay True

### Concrete producers and one shared handoff

Preparation stays on the existing owners. A closed enum carries submitted,
scheduled and grouped admission data; concrete owner methods prepare the common
work contract. This is not a boxed producer trait or a new execution framework.
The common shape is the canonical input record, execution identity and typed
admission metadata. Names and private field placement remain implementation-owned.

| Origin | Input preparation | Admission difference |
|---|---|---|
| Submitted Run | Supplied declarations and existing activation context | Existing caller authority, trace links, request-key idempotency and after-ACK readiness checks |
| Scheduled | Existing window query freezes observation IDs | Frozen binding/authority/Trigger/Operators plus expected schedule cursor and its transition |
| Grouped | Existing cutoff/source query freezes ordered execution/result references | Frozen binding/authority/Trigger/Operators, content digest and the selected pending activity generation |

Generalize the existing Submission payload and shared ACK-to-queue writer;
do not wrap them with a second lifecycle owner. Producer differences belong in
preparation and the exhaustive admission-data match. Batching, publication,
successful-ACK filtering, queue acceptance/replay, transaction commits and
execution remain common. One standard contract does not mean flattening valid
origin-specific fields into optional JSON or copying source values into SQL.

Extend the existing successful-input-ACK handoff with a small internal
acknowledged-work value. Only the Scribe success path constructs it; the common
admission entry point requires it. A stored queue_execution flag, arbitrary
prepared request or an output/summary ACK cannot manufacture that authority.
Keep it internal: no public receipt API, capability framework or new spec tier
dependency. Existing lower SQL methods remain owned by wyrd-sql; production
server admission uses the one ACK-gated entry point.

### Scheduling state changes only with acknowledged queue acceptance

During background preparation, keep the existing cursor and pending group
unchanged for candidates being queued. Existing no-execution skip/disarm/refusal
decisions keep their current transactional state transitions; do not manufacture
inputs, ACKs or queued work for those outcomes. Select/freeze evidence using the
current cutoff/order/null rules,
then release the preparation transaction before staging or awaiting publication.
Carry the frozen command through the existing bounded memory handoff: execution
ID, selector, original recorded_at, Verifier/version/subject/owner, binding,
Trigger/Operators, accepted authority/attribution, retry budget, window or group
digest and correlation. Within the attempt neither replay nor admission may
substitute new binding state, authority, timestamp, input selection or ID.

After the own-input ACK, the shared admission writer uses one tenant transaction
to insert/replay the ordinary execution and apply its scheduling transition:

- Schedule: preserve the existing binding/window occurrence key, advance only
  the expected cursor and do not overwrite newer/edited scheduling state.
- Group: preserve binding/group/selection-digest dedupe; consume only the pending
  activity generation that produced this candidate. Newer activity stays pending.
- Submitted: preserve existing idempotency conflicts/replays and refusal results.

Add only version metadata to the existing group pending table through a forward
migration. The activity generation must change on every acknowledged activity
upsert and must not be reused when a consumed row is recreated. Use an ordinary
Postgres-generated generation token in that existing row; no new table, global
lock or guard service. The existing activity upsert updates it in the same SQL
operation. Conditional consumption must also be replay-safe: accepting an
already-accepted candidate cannot consume later activity or repeat a cursor
transition. Identical logical work returns the original accepted execution;
distinct later group selections may coexist. The accepted execution keeps its
own acknowledged input record; a losing candidate cannot replace that input.

The execution and scheduling-state transition commit or roll back together.
Publication failure creates no runnable execution and leaves source work due;
SQL failure rolls back and the common handoff retries. Existing permanent
pre-admission rejection and source-incompleteness refusal behavior stays
explicit and produces no accepted input claim, verdict or Operator. Preserve
typed errors and authorization boundaries. A no-Scribe/transport/infrastructure
failure is not a successful ACK and not an auth refusal.

### Batch commits and distributed workers

The scheduler prepares/stages a bounded batch without waiting for one ACK
before preparing the next item. The existing shared admission writer groups
ACKed items of one tenant into a transaction, including mixed origins and their
cursor/group updates. Do not create per-origin writers or force one commit per
execution. Existing conflict/refusal results remain per item; transient SQL
failure rolls back/retries the batch. Reuse deterministic lock ordering and
existing capacity/poll/retry limits, with no new concurrency or timing knob.

Use bounded process-local in-flight ownership under the existing scheduler to
avoid repeatedly querying the same due item while its handoff is outstanding.
Release it on completion, failure or cancellation. It is not a durable lease or
cross-replica guarantee; other replicas may prepare duplicate candidates, which
the existing queue uniqueness rules arbitrate. Source state is retained until
acceptance, so process loss needs no preparation-table recovery scanner.

After commit, retain the existing local notification only as a latency hint.
Every eligible runner replica discovers work through the shared Postgres queue
and claims it with the existing database lease. No correctness depends on a
notification, producer/worker colocation or a singleton process. Keep one queue
runner implementation, renewable lease/cancellation, fairness, retries and
Operator settlement. Scheduler/group due polling is producer work, not another
execution queue. Existing multiple workers/replicas and queue batching remain.

### One engine and three Bifrost payload owners

Every queued origin loads its own input record from Bifrost and resolves its
kind through the existing InputResolver and drift preparation. Selecting IDs is
producer preparation; resolving their retained values is worker preparation.
All call the existing HostedBlocks/VerificationEngine. No special PG-input
branch, extra Bifrost readiness probe, current-data reselection on execution
retry or independent input loader is introduced.

Keep execution_inputs by execution ID, execution_outputs by result ID and
payload-free results by result ID. Reuse ResultPayloadBuilder and canonical
result readback. The input marker queue_execution becomes true for all queued
origins, false for direct; existing submitted activation semantics and background
selector shapes remain. Background frozen coordination data stays in the memory
command then queue state, not a duplicated Bifrost activation map. This is a
server-owned marker, not an ingestion permission or a queue trigger by itself.

Queued inputs already exist before execution, so normal result recording does
not write them again. Build queued result rows once and stage only behind the
existing applied-lease fence, with detailed output before summary. A shared
builder/writer must not stage a stale worker's result. Keep asynchronous result
settlement; no result ACK-before-completion or durable result spool is added.

Direct verify retains existing authorization, capacity reservations, request
inputs and immediate execution. It stages its input plus result afterward and
does not await Scribe. Keep existing registry reads; add no direct/submit-request/
capture/audit coordination writes, queue claims or publication waits. Window
submit still records observations. Keep 50 ms Scribe linger, dense batching,
trace/missing-evidence handling and bounded backpressure. Scribe outages may
eventually exhaust staging capacity; do not promise unlimited execution through
an outage or serialize normal producer preparation behind per-item ACK waits.

### Limits, retirement and migration

Scribe publication and Postgres acceptance are not one atomic transaction. A
crash before acceptance may leave an orphan input and the producer can select
again. Existing Bifrost-based group source selection can see unaccepted evidence;
do not call those rows harmless garbage or add a queue-acceptance join to change
that contract. Existing caller staging/handoff loss, source retention, async
result loss and at-least-once provider execution remain. Types constrain the
shared code path; they cannot promise cross-store atomicity or indefinite data
availability. Do not introduce rare-failure ledgers, duplicate record stores,
historical repair or a physical exactly-once batch protocol.

Discard all obsolete snapshot-table/hold/infinity/activation/terminal-plus-ACK
cleanup work. The previously rejected four tracked staging-workaround files
were already restored and its untracked migration removed; verify current state
rather than blanket-resetting the checkout. Revalidate retained dirty gateway,
Bifrost cache/WAL/audit-barrier and SDK three-table/staging fixes; retain only
needed changes. Regenerate llms-full.txt and other generated outputs from their
source. work/ diagrams are not product/PR deliverables.

Only the existing group pending row's generation metadata needs a new migration.
Preserve data and initialize existing rows safely; never alter an applied
migration or reset a non-disposable database. Update Wyrd/Bifrost owner sections
and conflicting old task instructions before behavior. Existing queued work
continues through the same Bifrost reader. Lost old selectors retain existing
unavailable/error behavior; do not fabricate inputs. This task does not claim
mixed-version handoffs are supported: migrate additively, drain/stop old
scheduler/runner processes, then start the revised producers/consumers before
accepting new work. New generation updates must not be bypassed by old activity
writers. No compatibility alias or second active admission protocol is added.

Keep both original work-count regressions unchanged: one scheduled wait/attempt
and four mixed-case attempts; group queries remain exactly 4 + 2*5 = 14. Remove
the prior task's 12-query target. Retire a stale test only when its promised
behavior was superseded and replacement proof is green, not to bypass failure.
Non-goals include L2, unrelated performance redesign, stronger submit/result
durability, a public prequeue state/API, merge and release. Existing unrelated
benchmark obligations remain with their original tasks.

## Approach

1. Reconcile dirty work, replace conflicting owner text and add the minimal
   existing-group generation migration; preserve the approved no-table scope.
2. Generalize the current submission handoff into canonical input plus closed
   typed admission variants, with a restricted successful-ACK transition.
3. Route concrete schedule/group preparation through that SAME publisher/writer;
   keep source work pending and carry frozen command data across the handoff.
4. Batch acknowledged admission and source-state transitions per tenant; retain
   current dedupe, group-generation protection, distributed claims and bounded
   in-flight producer work without a durable spool.
5. Keep the Bifrost input reader, shared engine, direct costs and fenced result
   recording; complete scenarios and consumer regressions and remove stale work.
6. Repair/prove the final full gate, complete required immutable reviews and
   freeze a clean candidate for measurement with no competing work.
7. Run full L1 Verification/Bifrost/Gateway, publish benchmark-records, regenerate
   the report and create/update the code PR to main with actual evidence.

## Ordered Implementation Scenarios

### Scenario 1 — Every queued origin requires its own input ACK

**Behavior.** Submitted, schedule and group candidates all enter the same input
publisher. Paused/missing/failed input ACK produces no pending queue row or
execution attempt. Source scheduling state stays due. Successful own-input ACK
reaches the common admission owner; another row's ACK cannot substitute. Direct
verify can still return while archival is paused and staging capacity remains.

**RED.** Add the planned real-engine integration test in [V1](#v1-shared-ack-gate).
Current HEAD fails because schedule/group execution rows exist before input ACK.
Assert direct completion with archival paused, not only a fast-path latency.
Do not use a scripted engine that skips input preparation.

**GREEN.** Generalize the existing submitted handoff/ACK boundary, restrict
admission to successful acknowledged work and keep direct delivery unchanged.
Run V1 and V7–V10. No pre-ACK held execution or new table is an acceptable fix.

**REFACTOR.** Reuse current builders and owners; remove producer-specific
publication/acceptance and unneeded dynamic dispatch rather than wrapping it.

### Scenario 2 — Queue acceptance and scheduling transitions are atomic

**Behavior.** For an ACKed schedule/group candidate, queue acceptance and its
cursor/pending transition commit together. Rollback leaves work pending. Same
occurrence/content contenders accept once; delayed/replayed ACKs cannot change
the selected accepted input, advance a newer/edited cursor or consume newer group
activity, including when a pending row was consumed and recreated.

**RED.** Add the planned Postgres integration test in [V2](#v2-atomic-scheduling-admission).
It fails because current insertion/consumption precedes ACK and there is no
non-reused group generation protection. Exercise real competing acceptance,
rollback, changed cursor and both updated/recreated group pending rows.

**GREEN.** Move pending execution insertion into the shared after-ACK writer,
apply source transitions in its transaction and preserve existing uniqueness.
Use generation-scoped group consumption and replay-safe transitions. Run V1–V2,
V7–V9 and migration/RLS checks.

**REFACTOR.** Consolidate transaction ownership under existing SQL/admission
owners; remove separate scheduler enqueue writers and all input-hold states.

### Scenario 3 — Asynchronous preparation feeds per-tenant batch commits

**Behavior.** Multiple producers can prepare/stage while earlier inputs await
ACK. A writer batch accepts multiple ACKed items, including mixed origins, with
their scheduling updates in one tenant transaction. A failed commit rolls back
the batch and retries through the same owner; item conflicts retain existing
typed outcomes. Pending in-flight work does not cause repeated same-process
source queries or starve another due item/tenant. Nothing crosses tenant scope.

**RED.** Add the planned integration test in [V3](#v3-batched-admission).
Current HEAD cannot admit schedule/group commands through the submitted batch
owner. Coordinate multiple staged inputs and prove multiple admissions share
a commit while later preparation proceeds before ACK release.

**GREEN.** Extend existing admission batching and deterministic transaction
ordering to the enum variants. Preserve bounded capacity, in-flight release and
normal SQL retries; do not add serial per-item ACK waits. Run V1–V3 and V7–V8.

**REFACTOR.** Reuse existing batched SQL and outbox lanes; remove duplicate
flushers, per-origin queue services and redundant per-item round trips.

### Scenario 4 — Failure retries existing source state without stronger delivery

**Behavior.** Publication failure or process loss before queue acceptance leaves
existing scheduling work due. It can be re-prepared; orphan input rows are an
accepted outcome. A SQL acceptance retry within one handoff retains identical
input ID/content/timestamp and frozen authority, Trigger/Operators and metadata
despite live binding changes. After acceptance, worker restart/retries use the
accepted Bifrost input, never a fresh population. No expired lease can stage a
result. Do not assert physical exactly-once prequeue publication.

**RED.** Add the planned integration test in [V4](#v4-failure-and-retry-semantics).
Current HEAD consumes background scheduling state before acknowledged acceptance.
Assert retained due state at the failure boundary, fixed in-flight metadata and
fixed accepted membership after later source data arrives. Include the existing
terminal pre-admission refusal cases as regressions, not newly invented states.

**GREEN.** Retry through existing source polling/handoff, preserving metadata
within a candidate and queue dedupe across candidates. Keep existing unavailable
source behavior and refused-item semantics. Run V1–V4 and V9–V10.

**REFACTOR.** Delete temporary-table recovery/cleanup/TTL mechanisms and orphan
suppression probes. Keep only ordinary pending-state, idempotency and lease rules.

### Scenario 5 — Distributed queue discovery does not depend on notifications

**Behavior.** An eligible replica can claim committed work from another
producer/replica with the local notification suppressed. Competing replicas do
not both own the same valid lease. There is one queue implementation for all
origins and no colocation or singleton requirement. Preparation/outbox pressure
releases local in-flight ownership and leaves durable source state for retry.

**RED.** Add the planned integration test in [V5](#v5-distributed-discovery).
Exercise the newly shared background admission from a separate producer with
notification delivery suppressed. Add the origin coverage missing from existing
distributed-claim proof; do not manufacture a RED for already-correct polling.

**GREEN.** Preserve shared Postgres polling/claims, current lease fencing and
optional local hints; admit new origins through the same writer. Run V1–V5 and
V9 plus existing multi-replica regression V17.

**REFACTOR.** Remove any direct dispatch-to-worker dependency, separate origin
poller for core execution or new notification infrastructure.

### Scenario 6 — Typed admission and source transitions remain tenant-isolated

**Behavior.** Wrong-tenant commands cannot accept, replay or consume another
tenant's schedule/group state, and ACK association cannot be substituted by
another execution's input. Accepted caller/background authority and source-read
scope remain unchanged. Generation metadata and queue operations retain forced
RLS and existing connection boundaries; credentials never enter records.

**RED.** Add the planned integration test in [V6](#v6-admission-isolation).
Use real tenant/platform logins and the new mixed-origin handoff; verify denied
cross-tenant state mutation and incorrect ACK association. Existing submitted
tenant tests are regression proof, not substitutes for new origin coverage.

**GREEN.** Extend typed queue commands under TenantConn/OperatorPool and current
error/authorization owners. Keep source validation and no public token/receipt
surface. Run V1–V6 and V11–V12.

**REFACTOR.** Remove untyped catch-all admission JSON, raw pool access, manual
tenant predicates and copied authority envelopes; keep the closed enum explicit.

## Acceptance Criteria

| Obligations | Observable acceptance and proof |
|---|---|
| REQ-035, AC-015 | V1: all queued origins use own-input ACK before pending insertion; direct still returns with archive paused. Source review confirms a common handoff and restricted acknowledged-work construction. |
| REQ-036, AC-016 | V2/V4: queue and source transitions commit together, duplicates select one accepted record, in-flight metadata stays fixed and newer/edited/recreated source state survives. No preparation table or stronger prequeue guarantee is introduced. |
| REQ-036/037, AC-017 | V3/V5/V17: mixed-origin per-tenant batch commits, asynchronous bounded preparation, failed-batch retry, distributed polling without notification dependence and exclusive valid leases. |
| REQ-037, AC-018, INV-001/003 | V6/V9–V11: real isolation/source permissions, same engine and canonical reader, stale-worker fence, separate Bifrost payload owners and unchanged direct costs/API behavior. |
| REQ-038, AC-018/019 | V7 stays one scheduled wait/attempt and four total; V8 remains exactly 14 queries. V12 and integrated V13 prove migration, generated artifacts, owner cleanup and the entire gate without exclusions. |
| REQ-039, AC-019, INV-006/007/008 | V14–V16: five full VALID same-SHA records, honest target outcomes, separate benchmark-records commit/push and actual main-based code PR linking gate and history evidence. |

## Implementation Reference

| Existing owner and inspected seam | Missing behavior and selected extension |
|---|---|
| `components/verification/service.rs::enqueue`/`run_direct`; `verification/results.rs::InputRecord` and `ResultPayloadBuilder::inputs` | Submitted input already carries the ACK handoff; direct already reserves and stages afterward. Reuse standard input shapes/builders, extend queued marker/origin handling and preserve direct timing. No second encoder is needed. |
| `scribe_outbox.rs::ScribeWrite::ExecutionInputs`, `ScribeSink::write`; `verification/submissions.rs::SubmissionSink::write` and existing outbox | Current command is caller-only and sink stages it only after a non-lost input ACK. Generalize that payload/ACK gate and writer for all origins, retaining success filtering, batching and post-commit notification. Existing typed ownership is the shared pipeline, not a new orchestration service. |
| `wyrd-sql/.../verifier_executions.rs::Submission`, `accept_submissions`, `enqueue_scheduled`, `enqueue_group`, `schedule_due`, `consume_group`, `ResolvedTarget` | Existing queue owner freezes authority/binding/Trigger/Operators and owns uniqueness. Carry that frozen command through ACK and move origin-specific queue/source writes into common tenant-batched admission. Do not call mutable binding resolution again after ACK or hard-code manual origin for all variants. |
| `VerificationScheduler`, `GroupPoller::poll`, existing due cursor/group queries and `record_group_activity` | Existing scheduler currently inserts before publication and consumes the pending item. Leave source state pending until ACK acceptance; attach expected cursor/generation, add minimal non-reused generation metadata, batch preparation and retain bounded local in-flight ownership. No durable preparation store or extra recovery scanner is justified. |
| `verification/inputs.rs::InputResolver::record/evidence/slots/result`; `DriftEngine`; `HostedBlocks::engine`; runner `dispatch/recorded` | These already load/resolve Bifrost inputs and call one engine. Keep the all-origin Bifrost route, factor only real duplicate setup and preserve selected membership, source errors and canonical readback. No producer-specific executor/trait or PG-selector fast path. |
| Runner `settle`, `Completions::commit/stage`, `VerifierExecutionQueue::claim/complete_many` | Existing stale-lease fence and distributed claims already own correctness. Preserve them while sharing payload construction; local wake remains optional and results remain asynchronously recorded. |
| Runtime integration harness/PublicationFault; real-client `verification_runtime` ScanEvidence; SDK verify-in-real-time stories | Extend real seams for ACK gating, transactional acceptance, mixed-origin batches, generation concurrency and distributed origin coverage. Preserve exact original work-count assertions and public negative/readback stories. |
| `mise.toml`, TESTING.md and benchmarks/README.md; existing benchmark-records worktree | Reuse exact proof and full L1/history machinery. No new benchmark framework, record schema, threshold changes or benchmark CI dependency. |

Update grouped/scheduled/submit/runner ownership in architecture/wyrd-design.md
and the verification input/activation contract in architecture/bifrost-design.md.
They currently require early background enqueue and submitted-only ACK activation.
Keep current security/tenancy and the explicit orphan/source-retention/result-loss
limits. Paths are guidance, not a private helper/type/file allowlist.

## Verification and Evidence

All commands run from the repository root with sequential Cargo work. V1–V6
and V18 are planned tests, not existing passing proof. Existing wyrd-server
integration and wyrd-testing server manifests, test roots and mise owners
establish their targets/features/setup. Confirm each actual nextest selector
after adding it and before RED; a zero-test result is not proof. TDD applies to
new/changed executable behavior. Existing correct polling, shared engine,
documentation, generated artifacts, measurements and delivery use regression,
static or verification-only proof, without manufactured production changes.

### V1: Shared ACK gate

Planned real-engine integration test for all queued origins and paused-archive
direct delivery; also run the public journey V18:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::queued_origins_require_their_own_input_ack)"'
```

### V2: Atomic scheduling admission

Planned real-Postgres concurrent/rollback/generation test:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::acknowledged_admission_preserves_scheduling_state)"'
```

### V3: Batched admission

Planned test of mixed-origin per-tenant commits, asynchronous preparation and
bounded in-flight ownership with deterministic coordination:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::queued_origins_share_batched_admission)"'
```

### V4: Failure and retry semantics

Planned failure-boundary test of retained source state, fixed in-flight metadata
and immutable accepted inputs, allowing orphan prequeue inputs:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::failed_handoffs_retry_without_consuming_source_work)"'
```

### V5: Distributed discovery

Planned origin-complete notification-suppression/distributed-claim regression:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::accepted_origins_run_without_local_notification)"'
```

### V6: Admission isolation

Planned mixed-origin tenant/ACK-association negative proof with real RLS:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::queued_admission_preserves_tenant_and_input_identity)"'
```


### V7: Exact runtime work counts

Existing selector; keep one scheduled queue wait/attempt and four total attempts:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::attempts_record_queue_wait_phases_terminal_latency_and_one_trace)"'
```

Historical evidence: this unchanged test passed once at `edc1f2eac^`
(`432d186bb`) in this investigation; it is context, not final-candidate proof.

### V8: Exact group lookup and SDK evidence

Existing real-client/server selector and target; preserve its exact count of
14 (4 + 2*5), including each group's canonical input read, ordering/null/verdict
and source-occurrence assertions:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run build:server && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::group_lookup_preserves_source_occurrences)"'
```

Its old exact count of 14 also passed once at `432d186bb`; this supports the
linger/race explanation but is not a timing or new-workflow proof.

### V9: Stale lease protection

Existing regression; consolidation must not publish a stale worker's result:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::expired_lease_is_reclaimed_and_the_stale_holder_is_fenced)"'
```

### V10: Canonical payload ownership

Existing regression; preserve distinct input/output/summary records and readback:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::verification_records_have_one_payload_owner)"'
```

### V11: Public journeys and consolidation regression

These are existing, focused SDK journeys, not new TDD scenarios. Preserve their
typed evidence/verdict and negative behavior. Add the new ACK-gate
assertion through the planned real-client server journey V18;
do not put raw SQL or private APIs into SDK stories.

Rust schedule/group coexistence (package name is `wyrd-ai`):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run build:server && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=verify_in_real_time::grouped_conversations_coexist_with_drift)"'
```

Python schedule/group coexistence:

```bash
mise run py:setup
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q tests/integration/test_verify_in_real_time.py::test_grouped_conversations_coexist_with_drift'
```

TypeScript schedule/group coexistence:

```bash
mise run ts:build
mise run ts:build:testing
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^grouped conversations coexist with drift$"'
```

Inspect the complete direct/submitted stories and HTTP/MCP consumers for the
changed owner, and run any additional specifically named regression by its
confirmed exact selector. Their complete surface coverage runs once at V13.
Source review proves one engine rather than a new delegation framework; direct
L1 in V14 measures its throughput/overhead. Do not manufacture a RED for code
already correctly shared, documentation or delivery operations.

### V12: Focused static and migration checks

Run only applicable checks during implementation; V13 owns their final aggregate:

```bash
mise run test:sql
mise run check:tenant-isolation
mise run check:deps
mise run codegen:check
mise run fmt
mise run lints
mise exec -- git diff --check
```

Add the owning Python/TypeScript formatting/lint checks when changing those
stories. If generated output is stale, update its source and regenerate; do not
edit `llms-full.txt` or declarations by hand. Static proof also inventories
removed hold/infinity/activation logic, test retirement, one engine, no new
direct coordination IO, shared ACK admission, batching, generation protection and the approved migration.

### V13: Integrated full gate and final source

The maintainer explicitly requires full-gate closure in this single revision
task, overriding ordinary task-level aggregate exclusion. This is final
integrated proof, not another per-scenario lane. Run once on the integrated
candidate with all lanes allowed to finish and Cargo work serialized:

```bash
mise run -c -j 1 gate
```

Record candidate SHA, environment, exact invocation, overall exit and every
lane's result with a log path. Fix red lanes at the owning code/test/setup;
rerun failed proof after fixes, and obtain a complete green gate for the final
candidate. Do not call a partially green shard matrix a full pass, exclude a
lane, weaken an assertion, or stop at proving a failure predates this change.
Any final source/test/config change requires affected proof and renewed final
gate evidence. Do not repeat already-covered component suites as a checklist.

Complete the independent task/change reviews required by the repository
workflow against an immutable candidate. Approval is not merge permission.
Commit the approved code, contract/documentation updates and retained test fixes
using configured Git identity. Freeze its clean source SHA before V14; keep
large raw logs outside tracked files. This task authorizes implementation-time
code/history commits, pushes and PR creation, not those operations during
planning. Use the current code branch `change/benchmark-rev19` unless its
caller-owned branch arrangement has changed; never force-push or rewrite it.

### V14: Full L1 measurements

No TDD applies: these measure the finished candidate. All three commands run
sequentially with no competing tests, builds, servers or benchmarks, using the
same clean source SHA and default full plans:

```bash
mise run bench:l1:verification
mise run bench:l1:bifrost
mise run bench:l1:gateway
```

Do not pass `--allow-dirty`, `--steps`, `--profile` or `--smoke` for required
history proof. Bifrost produces three records (writes-parquet, hot, parquet),
Verification one (direct-verify) and Gateway one (overhead): five in total.
Each required record is schema 3, mode full, clean, unprofiled, full coverage and
VALID with source SHA, all repetitions and integrity facts present. Reports
retain every target as met, missed or unsettled; target misses alone do not
block completion and do not permit retuning. Retain eligible INVALID attempts;
correct the validity cause and obtain VALID replacements. If code changes,
freeze a new candidate and repeat final gate plus all five required measurements.
Never run benchmark workloads as part of gate or in parallel with its repairs.

### V15: Benchmark branch publication

`benchmarks/history/` already exists as the `benchmark-records` worktree in this
checkout. Verify that fact and its clean baseline before recording. Do not
recreate/delete it, switch the code checkout to benchmark-records, overwrite
other records or commit record JSON on the code branch. Follow
`benchmarks/README.md` if the worktree was moved by another authorized task.

Regenerate the self-contained report:

```bash
mise run bench:results
mise exec -- git -C benchmarks/history status --short
```

Inspect the five new record paths for source attribution, complete coverage,
validity, criteria, privacy and canonical schema. Stage exactly this task's
generated records (eligible INVALID attempts included), commit inside that
worktree and push `benchmark-records` to origin. Use the existing identity and
normal non-forced push. Record history commit SHA and each final VALID path;
do not stage another contributor's records or commit raw logs/profiles/HTML.

### V16: PR against main

Push the measured code branch and create or update its PR with base `main` on
the configured `origin` (`bohmian-ai/wyrd` at authoring). Inspect any existing
PR for this head and update it rather than opening a duplicate. Use a prepared
body file for multiline text and `gh ... --body-file`, or a structured GitHub
tool. The task authorizes PR creation; it does not authorize merging.

The description leads with the scheduling race and resulting execution flow,
then links: final source SHA; full gate command/result/log evidence; five VALID
record paths and their benchmark-records commit; direct/Bifrost/Gateway target
outcomes; asynchronous result/source-retention/at-least-once limits. Confirm
base/head and the returned PR URL. No fabricated green badges or attribution
footers. If a credential or branch-protection permission genuinely prevents
publication, finish local evidence and report the exact blocked operation;
never claim that the PR/history push exists.

### V17: Existing distributed lease regression

Keep the existing multi-replica execution proof in addition to new origin
coverage in V5:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::two_replicas_claim_each_queued_run_exactly_once)"'
```

### V18: Public shared-admission journey

Planned real SDK -> WyrdTestServer/Postgres/Bifrost -> SDK journey. Use actual
Run submit, scheduled Window observations and grouped source inputs to prove
the same ACK gate and fixed input readback; direct verify still returns with
archival paused while capacity exists. Reuse existing publication controls or
the smallest owning harness extension, not private SDK APIs/raw SQL in stories.
The focused SDK coexistence regressions in V11 retain each language surface;
the final gate owns complete HTTP/MCP/language closure.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run build:server && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::queued_origins_share_ack_gated_admission)"'
```

## Material Stop Conditions

- Stop for specification authority if implementing the approved pipeline
  requires a temporary/prequeue input table, durable publication ledger, PG
  worker-input branch, boxed producer framework or another execution mechanism.
- Do not put supplied/source values, resolved group arrays, outputs or
  credentials on execution rows or elsewhere in Postgres. No new input kind,
  public receipt, stronger submit/result delivery or provider exactly-once
  guarantee is approved.
- Preserve tenant/authority, window/cutoff/order, idempotency, retry and Operator
  semantics. The group generation migration is approved coordination metadata;
  other material persisted/public/dependency/feature changes need authority.
- Migration must preserve existing data. Do not edit applied checksums, reset
  non-disposable deployments or pretend old activity writers can safely ignore
  the new generation contract. No missing historical selection may be invented.
- No gate/count weakening, admission-readiness probes or benchmark retuning can
  substitute for correct shared flow. Local factoring, in-scope gate repairs,
  test setup and bounded proof corrections remain implementation-owned.
- Unavailable external publication credentials may block history push/PR after
  unaffected work is complete; report the exact operation and never fabricate
  delivery or passing evidence.

## Authority Links

- [Approved spec, revision 24](../spec.md#revision-24-supplement--one-producerconsumer-verification-pipeline)
- [Spec-driven workflow](../../../spec-driven-development.md) and
  [implementation execution](../../../implementation-execution.md)
- [Wyrd verification and distributed runner](../../../../architecture/wyrd-design.md)
  and [Bifrost verification tables](../../../../architecture/bifrost-design.md)
- [Rust closed-enum/owner guidance](../../../../agent_docs/rust.md),
  [security posture](../../../../architecture/wyrd-security-posture.md), and
  [SQL owner rules](../../../../crates/wyrd/wyrd-sql/AGENTS.md)
- [Testing commands](../../../../TESTING.md) and
  [benchmark methods/history](../../../../benchmarks/README.md)
- [Prior storage task, superseded only within this revision](TASK-006-R1-simplify-verification-data-plumbing.md)


## Implementation Evidence

Implementation `4778abf71` plus the outbox fix `e9e3d95cb` on `change/benchmark-rev19`. Every command ran serially
through `scripts/postgres/with-test-postgres.sh` against the repository Postgres.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-035, AC-015: own-input ACK before any queued row; direct unaffected | `scribe_outbox.rs::ScribeWrite::ExecutionInputs` carries `Queued` for every origin; `verification/submissions.rs::SubmissionSink::write` admits only `Acknowledged` work | V1 `pg_verification_runtime::queued_origins_require_their_own_input_ack`; V18 `verification_runtime::queued_origins_share_ack_gated_admission` | PASS |
| REQ-036, AC-016: cursor/generation move with admission; fixed in-flight metadata | `wyrd-sql` `accept_scheduled` (`ADVANCE_EXPECTED_CURSOR_SQL`), `accept_grouped` (`CONSUME_GROUP_GENERATION_SQL`) | V2 `pg_verifier_executions::acknowledged_admission_preserves_scheduling_state`; V4 `pg_verification_groups::failed_handoffs_retry_without_consuming_source_work` | PASS |
| REQ-036/037, AC-017: per-tenant mixed-origin batches, distributed polling, exclusive leases | `admission_lane`: a tenant's scheduled and group candidates share lane 0; submissions hash key or execution id | V3 `pg_verification_groups::queued_origins_share_batched_admission`; V5 `pg_verification_runtime::accepted_origins_run_without_local_notification`; V17 `two_replicas_claim_each_queued_run_exactly_once` | PASS |
| REQ-037, AC-018, INV-001/003: isolation, stale fence, payload owners, SDK behavior | unchanged runner fence and Bifrost owners | V6 `pg_verifier_executions::queued_admission_preserves_tenant_and_input_identity`; V9; V10; V11 Rust `wyrd-ai`, Python pytest, TypeScript vitest | PASS |
| REQ-038, AC-018/019: exact counts, static checks, gate | no count or assertion changed | V7 four attempts, one scheduled wait; V8 exactly 14 queries; V12 fmt, lints, `test:sql`, `check:tenant-isolation`, `check:deps`, `codegen:check`, `git diff --check` all exit 0; V13 see below | PASS |

Selector deviations: V2 and V6 live in `wyrd-sql/tests/integration/pg_verifier_executions.rs`
(lane `test:sql`) because they drive the SQL owner directly; V3 and V4 live in
`wyrd-server/tests/integration/pg_verification_groups.rs` beside the group
fixtures. Names are unchanged; run with `test(=pg_verifier_executions::<name>)` or
`test(=pg_verification_groups::<name>)` in the same targets.

Full-file regression: `pg_verification_runtime::` and `pg_verification_groups::`
58 run, 58 passed at `4778abf71`. An earlier run (5 failures) preceded
`51bc7d183`, which aligned those pre-existing scheduler/group tests with
acknowledged admission (superseded promise: insert before publication).

Non-goals held: no temp/prequeue table, no inputs on execution rows, no hold or
infinity scheme, no recovery machinery, the 50 ms linger unchanged.

V18 `verification_runtime::queued_origins_share_ack_gated_admission` (journey,
`-p wyrd-testing --test server`) passed. V7–V11 and V17 passed by exact selector
at `4778abf71`; V11 on Rust, Python and TypeScript.

### V13 gate

| Candidate | Invocation | Result | Log |
|---|---|---|---|
| `4778abf71` | `mise run -c -j 1 gate` | exit 100: only `test:bifrost:gate` red, 275/276; `forge live_rewrite::compaction_target_registers_describes_and_steers_forge_rewrites` | outside the tree |
| `e9e3d95cb` | `mise run -c -j 4 gate` (maintainer-directed `-j 4`) | exit 0, every lane green | outside the tree |

Diagnosis of the red lane:

- **Symptom:** `flush_bifrost` at `live_rewrite.rs:2276` returned
  `1 staged Scribe writes did not settle within 60s`; it passed alone and in
  four further lane runs.
- **Evidence:** the dump at failure showed one outbox item pending, no write
  failure or retry, and no further write started. `OutboxWriter::run` called
  `dispatch` and `next_wake`, each reading `Instant::now()` itself.
- **Cause:** a lane whose linger deadline passed between the two reads was
  not ready for `dispatch` and was filtered as already due by `next_wake`,
  which then returned `None`; the writer awaited `pending()` until another
  item or completion arrived. Introduced by `edc1f2eac` (lane linger), not by
  this task's diff.
- **Fix site:** `wyrd-runtime/src/outbox.rs`, the shared owner of
  `ScribeSink` (audit, captures, spans, verification inputs/results) and
  `SubmissionSink`; the loop now passes one instant to both. Retry deadlines
  share the same instant.
- **Diagnosticians:** a read-only Claude diagnostician located the stall
  before Scribe ingest without naming a cause; a read-only Codex diagnostician
  named the lost wakeup, confirmed in source.
- **Proof:** `outbox::tests::a_lane_short_of_its_linger_keeps_a_wake_at_its_deadline`
  fails with the old second clock read and passes with the fix; outbox tests
  20/20, `test:shared` 823/823, lints, fmt, and the green gate above.
