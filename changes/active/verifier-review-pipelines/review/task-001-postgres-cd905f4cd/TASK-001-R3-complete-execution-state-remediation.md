---
id: TASK-001-R3
kind: architecture_refactor
spec: SPEC-verifier-review-pipelines
spec_revision: 18
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
status: proposed
original_task: changes/active/verifier-review-pipelines/tasks/TASK-001-bifrost-execution-state.md
candidate: cd905f4cd929785bda25696326d58b4a4ae88dee
base: 0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb
findings: [FIND-TASK-001-16, FIND-TASK-001-17, FIND-TASK-001-19, FIND-TASK-001-21, FIND-TASK-001-22, FIND-TASK-001-23]
user_directed: [USER-PG-001, USER-DURABILITY-001]
subsumed_by_removal: [FIND-TASK-001-24, FIND-TASK-001-25]
req: [REQ-052, REQ-053, REQ-054, REQ-055, REQ-056, REQ-057, REQ-058, REQ-059]
depends_on: []
---

# Complete TASK-001 execution-state remediation

## Outcome and value

Keep live verification state in the existing engine/RunState and recover it
from canonical Bifrost records through the existing writer, WAL, batch/source
metadata and read owners. Remove `vala.scribe_state_heads` and every production
claim/read/advance dependency on it. PostgreSQL retains its approved queue and
control-plane roles; it does not own execution checkpoint position, historical
result selection, publication custody or the sole retained terminal/error body.

For example, a queued run resumes its accepted Bifrost checkpoint without a SQL
execution-head lookup. A completed direct or queued run remains inspectable
using its recorded identity after source Card deletion; deleting a queued parent
cannot erase retained evidence. A final-attempt run whose result is already
durable finishes queue bookkeeping without running the model again.

This is the single active remediation task for all remaining findings from the
[original review](../task-001-bifrost-cd905f4cd/findings-validation.md) and
[PostgreSQL audit](verdict.md), consolidated at the user's direction on
2026-11-06. It replaces the narrower PostgreSQL-only R3 handoff and includes
findings21–23 below. Findings24/25 are addressed by verified removal of the
rejected owner;18 remains withdrawn and20's stronger ownership protocol remains
superseded. Historical review tasks are evidence, not parallel implementation
packets. Route this task to `$wyrd-implement`; no new execution architecture or
additional finding discovery is prescribed.

TASK-001 is implemented. Its original document supplies historical requirements
and evidence, not a destination for new work. All TASK-001 follow-ups from the
durable-workflow research belong in this remediation, including their proof and
measurements. USER-DURABILITY-001 records the user's agreement to those bounded
follow-ups; it is not a new independently discovered finding. New durable wait
and human-approval actions are separate TASK-007 scope and are not prerequisites
for closing this remediation.

## Authority and current user decisions

Read the approved revision-18 baseline of the [spec](../../spec.md),
[durability supplement](../../execution-state-and-durability.md),
[original task including Revision 1](../../tasks/TASK-001-bifrost-execution-state.md),
applicable architecture and AGENTS.

The parent now carries a revision-20 draft for new wait/approval actions. Its
proposals do not change this task's baseline or require waiting for that draft's
approval. This task also owns the user-approved research proof below.

The current user directions recorded in [subject.md](subject.md) take
precedence over contradictory older prose:

- USER-PG-001: the new SQL head table/ownership/accepted-position authority is
  rejected. Moving it into background Scribe, renaming it, adding equivalent
  queue columns or replacing it with a separate authoritative sidecar/cache
  does not satisfy removal. This is human-directed scope, not retroactive
  independent-review credit.
- The user accepts the specified rare interval where an old delayed worker/write
  can overlap a replacement. Preserve ordinary queue leases, renewal,
  cancellation, revocation and original deadlines, but do not add a new SQL
  ownership or queue-to-Scribe acceptance protocol to close that interval.
- Direct final input/output/result delivery remains asynchronous: returning the
  direct result must not acquire a new Bifrost final-record acknowledgement
  wait. Withdrawn finding18 creates no work. Existing consequential progress/
  publication boundaries remain governed separately.

The prior finding20 specification-decision stop and finding25's relocation-only
correction are superseded. The approved Bifrost authoritative-reader extension
is missing implementation work, not a reason to request a new SQL checkpoint
index or another approval gate.

## Diagnosis and selected correction

### USER-PG-001 — remove the rejected SQL authority completely

`RunState::claim` acquires a Vala tenant connection, writes `scribe_state_heads`
and commits before reading the revision it returns. Direct `fresh` uses the
same path. Public `accepted` queries that head before reading Bifrost. At
checkpoint acceptance, `scribe_batch_commits::record` invokes `advance`, so
SQL determines writer ownership and which checkpoint revision is authoritative.
Revision zero in SQL authorizes fresh state. The table has no payload, but it
still owns execution recovery and acceptance. Background relocation leaves that
same violation intact.

Remove the head owner and its runtime dependencies through the existing server
adapter and Bifrost owners. Fresh unique direct identities initialize in memory;
queued retry and retained reads obtain accepted execution state through Bifrost.
Retain the current short memory lock, coalescing/tickets/shared waiters, existing
outbox capacity/byte accounting, execution lanes and own-frame acknowledgements.
Normal HeldLease checks remain ordinary queue control, not a replacement head.

Complete the already approved authoritative Bifrost recovery path at the same
time. Existing generic batch fences describe tenant/table/batch/version/digest
and exact node/epoch/shard/segment/LSN attempts; file-list/read-cut/publication
metadata describes published stream/range coverage; existing live/staged readers
and retained-volume WAL replay supply unpublished data. Extend these existing
owners to reconcile the table's required durable sources with published and
reachable live/WAL coverage, then resolve the execution's canonical checkpoint
revision. Only complete coverage may establish accepted state or authoritative
absence. An uncovered/unavailable required owner must return typed retryable
unavailability, never an older snapshot or fresh state.

An ordinary latest-visible-row query is insufficient: live roster discovery
can omit an already unavailable owner holding a later acknowledged checkpoint.
Do not add an execution-specific SQL index/source registry to repair that query.
The existing generic Bifrost metadata tables remain legitimate infrastructure;
new per-execution head/writer/position fields in them are prohibited.

Required removal closure includes:

- Head query/export/schema and its state-advance hook in batch recording.
- RunState/StateScribe constructors, claim/accepted/read and direct/runner/
  inspection/hosted/testing consumers that assume SQL head existence.
- SQL-specific propagated state authority in Redux contracts/ingress/
  preprocessing/shards, builtin state-fenced definitions, refused-state branches,
  replay assumptions, frame/chunk/IPC and peer protocol plumbing.
- Unused head-only protocol fields/types/errors and their generated projections;
  retire protobuf names/numbers safely where fields are removed and regenerate
  from their owners. Keep useful Bifrost checkpoint/revision payload metadata.
- Head-manipulating fixtures/assertions and authority prose that excuses the
  table. Replace rejected SQL-specific proof with the retained behavior's proof
  at the Bifrost owner, rather than discarding recovery/tenant/replay coverage.

Preserve generic batch deduplication, accepting definition/version, ambiguous
commit reconciliation, exact WAL-attempt and published-range handling, sibling
batch progress and accepted canonical evidence. Reconcile replay's SQL-head
assumptions explicitly; neither blindly restoring formerly refused progress nor
skipping valid acknowledged checkpoints is acceptable. Do not restore the old
SQL invocation/publication/event stores or weaken their custody guard.

Applied migrations are immutable. Retire an installed rejected table with an
ordered migration while removing runtime reliance; do not rewrite an applied
checksum or erase accepted Bifrost data to make migration pass. Historical
migration references can remain historical with an explicit superseding removal
explanation. If retained candidate-era data needs handling, establish its actual
accepted batch/WAL custody before removal; do not assume every database/volume
is disposable.

This removal also subsumes the old head-query ordinal-decoding correction24
and direct request-owned claim25 once verified. Do not spend work repairing or
relocating queries that must be deleted.

### FIND-TASK-001-17 — canonicalize terminal facts, not queue payloads

`TerminalSelection` represents only a completed VerificationOutcome. Engine and
pre-engine errors, queue refusal/exhaustion, retry termination and user/principal
cancellation can write only queue status/full error. Public execution status
then returns that SQL payload as historical truth. A completed failed verdict
is not proof that an errored/timed-out/cancelled execution is retained correctly.

Complete the existing versioned canonical lifecycle/terminal representation
and connect its existing terminal producers through the existing reserved
shared-writer path. Record exact identity and existing sanitized typed status/
error, including outcomes with no prior accepted checkpoint. Engine failures
use their existing execution owner; queue-owned decisions use their actual
lifecycle owner rather than authorizing a stale runner or adding a finalizer.
Use existing stable identities/retry delivery and preserve queue control behavior;
no atomic queue-to-Scribe ownership protocol is added.

Keep only SQL status/retry diagnostics/result references justified by the legal
coordination uses in the audited inventory. They cannot be the sole retained
outcome, a second recovery payload, or the authority for the historical API.
Do not indiscriminately drop result references/group verdicts/dispatch context
that support idempotent completion and independent delivery. Preserve existing
public errors and result/input/output table meanings. Evolve the existing state
format safely; incompatible data must fail visibly without silent fresh reset.

### FIND-TASK-001-16 — resolve retained evidence from recorded identity

The canonical state does not retain all exact references needed by the readers.
Queued GET looks up live subject/Verifier Cards before returning its result;
direct GET returns not-found if either is deleted. Invocation/publication/content
inspection first requires a SQL queue row, although direct runs have none. The
no-queue fallback selects only direct-origin inputs, so retained queued evidence
also cannot survive removal of its coordination parent.

Retain immutable exact execution references and required reconstruction facts
once in existing canonical input/state/lifecycle records. Adapt InputResolver,
accepted-state and shared status/inspection readers to those records for every
origin and to the canonical selected result identity, not a SQL result pointer
or timestamp-selected result. Keep SQL live queue/Operator projections in their
coordination roles. Authorize/audit the current caller's exact feature grants
against the recorded Verifier identity before exposing data; a narrow SYSTEM
serving capability must not grant user permissions.

Preserve stable page cuts, exact producer/attempt and File descriptor selection,
content/read/download behavior and required-source unavailability. Do not fetch
live/deleted registry Cards as a historical execution identity authority or add
tombstones, direct queue rows, archive services or per-route fallback stores.

### FIND-TASK-001-19 — reconcile selected results before final exhaustion

Queue CLAIM_SQL permanently errors expired final attempts before the runner can
load the already selected Bifrost terminal result. A fully durable result followed
by failed queue-completion COMMIT can therefore become an exhaustion error.
The existing failed-settlement proof covers attempt1 to attempt2, not the ceiling.

Extend the existing queue-to-runner/load/deliver/complete path to reconcile
canonical selected work before permanently exhausting unfinished computation.
SQL must not query Bifrost. Preserve actual exhausted-work behavior, original
provider attempts/deadline, cancellation and stable selected identity; completing
already durable bookkeeping does not authorize another model call or a new
budget. This task does not choose a new delivery-beyond-deadline policy for
results that were not already durable.

### FIND-TASK-001-21 — confirm active publication replay authority

An already accepted publication can return its receipt through the covered-ticket
fast path without confirming the execution's current lease. After cancellation,
expiry or takeover, the old RunState may still have `broken = false`; JSON replay
and file preflight replay can therefore report success as the first stale
operation. The existing replay test first attempts a new stale write, which
marks the state broken and masks this path.

Reuse the existing HeldLease confirmation at the common active publication
success/replay boundary, outside the memory lock, including already covered
tickets. Both JSON acceptance/replay and file preflight receipt lookup must
respect it. A valid replay reuses the original receipt and adds no checkpoint.
Historical inspection remains independent of an active lease, and direct work
retains its queue-free path. This ordinary tool-authority check does not add the
rejected distributed checkpoint-ownership protocol or close the accepted rare
delayed-write interval.

### FIND-TASK-001-22 — bound all checkpoint preparation

`StateScribe::write` currently converts the complete state to JSON and builds its
Variant before outbox capacity admission on the polling request/runner task.
Only subsequent Arrow chunk encoding reaches the existing bounded writer
preparation path. Moving the mutex did not bound or move this full-state CPU
and allocation work.

Extend the existing Scribe preparation owner to perform frozen JSON/Variant
preparation as well as Arrow encoding under its existing bounded delivery and
resource accounting. Encode once per frozen checkpoint and preserve stable
retry identity, one pending snapshot, exact coverage, reservation/byte accounting,
deadlines, per-frame receipts and drain behavior. Reuse its preparation mechanism;
do not add an actor, per-run background task, unbounded per-caller blocking spawn,
new queue or format-optimization project. Coordinate this correction with SQL-head
removal so checkpoint payload preparation has one final owner.

### FIND-TASK-001-23 — finish mandatory rustdoc on surviving items

The original validation identified missing intent/Errors/Panics documentation
on the new state-format test and state/SQL/lifecycle/server test helpers. Add
precise rustdoc to the surviving or changed items while making the behavioral
corrections; no new checker or manufactured failing behavioral test is needed.
The bounded source inventory is:

- `wyrd-spec/src/verification/state.rs::only_the_current_format_decodes`: intent
  and panic contract for format/malformed-state assertions.
- `vala-sql/tests/integration/pg_scribe_batch_commits.rs::claim_state`: panic
  contract if it survives; delete obsolete head-only proof with its owner.
- `wyrd-server/tests/integration/pg_durable_verifier_work.rs::Leased::writer`:
  panic contract for its query/renewal setup.
- `wyrd-testing/tests/bifrost/scribe/lifecycle.rs::take_over`: propagated errors.
- `wyrd-server/src/verification/state.rs::tests::held_state`: panic contract
  for IDs/runtime-spawning outbox construction.

Paths are under `crates/` at their existing owning family locations. Recheck
actual surviving bodies after removal rather than documenting deleted code.
Every other newly changed item remains subject to normal rustdoc rules; this
finding does not authorize an unrelated documentation sweep.

### USER-DURABILITY-001 — prove the existing durable execution contract

Preserve the approved repeat-call policy: a provider may have completed a call
whose settlement was not acknowledged, so recovery may repeat it. Preserve the
same logical operation identity across that uncertain recovery and propagate
existing provider idempotency support where available. An intentional new call
has a distinct logical identity. Verify the actual provider adapter path rather
than assuming that checkpoint identity automatically reaches its HTTP request.
First establish whether any inspected adapter actually supports this behavior.
If none does, record the bounded evidence and possible-repeat limitation; do
not invent provider support or new headers to satisfy a hypothetical test.
Do not promise universal provider support, exactly-once external effects, or a
new cross-provider deduplication service.

Validate existing checkpoint/File/evidence retention together: accepted state
and its referenced content must remain recoverable for the documented supported
recovery interval. Record the actual retention/cleanup rules and their boundary
behavior. Do not invent an interval, extend retention, or claim indefinite
recovery. A missing or contradictory supported interval is a concrete policy
decision to report, not permission to silently select one.

Keep full snapshots. Measure their preparation and acknowledgement costs as
part of this remediation's evidence; neither incremental journals nor
compaction are selected. Checkpoint compatibility uses the existing persisted
format mechanism, not another user-visible engine version or deployment-pinning
system. These are proof obligations of existing approved behavior: already
correct paths require verification-only evidence, not a manufactured RED.

## Allowed PostgreSQL uses to preserve

The three discovery inventories and [validation](findings-validation.md) give
actual operations/fields/callers; they are the implementation reference, not a
blanket module exemption. Preserve these verified categories:

| Allowed owner | Permitted purpose / limit |
| --- | --- |
| Queue/admission/poller/leases | Identity/security/routing/idempotency, accepted initial input custody, claims, attempts/backoff, original deadline, renewal/confirm, cancel/revoke, release/retry and idempotent completion; not checkpoint head or retained outcome selection. |
| Schedules/groups/bindings | Due cursors/windows/debounce/generation/snapshot dedup and independent previous-verdict comparison; preserve existing source-input and dispatch behavior. |
| Operator delivery | Existing frozen bounded failure context, dispatch attempts/errors/leases and connection/auth lifecycle; not execution recovery custody. |
| Auth/registry/readiness/baselines | Existing current permission/principal/tenant/registration/preparation operations; not historical execution identity reconstruction from live Cards. |
| Shared File control | Existing upload/session/backend/digest/length/type/completion machinery; accepted publication association remains canonical Bifrost data. |
| Generic Bifrost internals | Existing catalog/table definitions, immutable batch/WAL identity/fences, file/source publication metadata, read cuts and node/physical-source coordination; no replacement per-execution SQL ownership/position authority. |

## Ordered implementation scenarios

### 1. Remove SQL-head authority and recover through Bifrost

**Behavior.** Fresh direct execution and queued recovery/read paths perform no
head claim/read/advance. Complete canonical state or authoritative absence comes
from Bifrost's existing source/read owners. A required owner unavailable before
discovery produces typed failure, never old/fresh state. Accepted data survives
retained-volume restart and ordinary identical-batch retry.

Also accept a checkpoint but drop its acknowledgement, then retry/reconcile
with the same batch identity and restart with retained WAL. Recovery returns
the accepted state without a duplicate logical settlement or rerunning an
already recorded invocation. Distinguish this from an external call whose
settlement never reached acceptance, which remains eligible for repetition.

**RED.** Use existing SQL migration and server/Bifrost seams to expose the head
table/dependency and ordinary partial-source false absence; record exact focused
selectors before RED. Do not introduce an artificial runtime checker for a
property a schema/source assertion and existing behavior test can cover.

**GREEN.** Remove the rejected owner and all dependent paths, extend complete
source coverage and adapt protocol/replay/migrations coherently. Preserve generic
batch control and the accepted rare timing limit, not rejected strict-takeover
assertions tied to the SQL table.

**REFACTOR.** Delete unused head-only exports/types/fixtures and keep shared
outbox/read/replay owners; no replacement schema, actor, writer or sidecar.

### 2. Retain full terminal/error facts independently of queue payload

**Behavior.** Engine/pre-engine error plus queue-origin refusal/cancellation/
exhaustion retain exact sanitized terminal facts in canonical data, including no
prior checkpoint. Queue control/diagnostics remain functional and are not the
only historical source. Direct final delivery gains no acknowledgement wait.

Exercise every prior checkpoint format this change promises to support and the
new terminal representation. Unsupported or malformed records refuse recovery
explicitly, without dispatch, fresh reset, or alteration of retained evidence.
Use the existing format/version owner; compatibility does not require pinning
each execution to a server release.

**RED.** Extend existing negative-flow integration seams to assert canonical
readback without relying on the queue's error payload; distinguish completed
failed verdicts from execution failures and record exact selectors before RED.

**GREEN.** Complete canonical terminal producers/readers through their existing
lifecycle and shared-writer owners; preserve repeated cancel/revocation,
retryability, ordinary lease/deadline controls and version-safe custody.

**REFACTOR.** Remove obsolete payload-dependent API assembly; reuse existing
sanitized error/result projections without another finalization service.

### 3. Inspect direct/queued retained data after source/parent removal

**Behavior.** Nonempty direct and queued executions with accepted publications
retain exact result/references/pages/content after source Card deletion; retained
queued work also reads without a queue parent. Current scoped/cross-tenant
permission negatives and source-unavailable distinctions still apply.

Record and exercise the existing retention/cleanup boundary for canonical state
and referenced Files/evidence. Within the supported interval, accepted
references remain usable; after legitimate expiry, readers report the existing
honest availability state rather than fabricating an empty successful output.
No new retention duration or deletion exemption is introduced.

**RED.** Extend existing terminal/inspection and publication recovery proof with
these lifecycle cases; current live-Card and queue prerequisites fail them.
Record actual exact selectors before RED.

**GREEN.** Write required recorded identity once and adapt shared origin-neutral
canonical resolution/authorization. SQL pointers may remain lawful control
bookkeeping but cannot choose the historical result.

**REFACTOR.** Consolidate at existing readers, avoiding route-specific fallbacks,
registry tombstones or direct queue admission.

### 4. Finish durable result bookkeeping on the final attempt

**Behavior.** On the final permitted attempt, accept the selected result and all
required queued result records, then inject queue-completion COMMIT failure;
reconciliation returns that same result with zero provider/engine reruns, while
truly unfinished final attempts still exhaust under the original bounds.

**RED.** Adapt the existing failed-settlement runtime regression to the ceiling
case; it currently errors before the canonical loader can run. Use the exact
focused command below and retain the unfinished-exhaustion negative.

**GREEN.** Reconcile through the existing claim/runner/load/deliver/complete
owners before final computation exhaustion, without replenishing attempts or
extending computation/delivery policy.

**REFACTOR.** Reuse stable selected identity and existing bookkeeping; add no SQL
Bifrost query, second recovery source or finalizer.

### 5. Refuse stale-first publication replay without writing again

**Behavior.** Accept JSON and file publications, then cancel or replace the
queue lease and make identical replay the first stale operation. Both active
tool paths refuse; an otherwise identical live replay returns the original
receipt without another checkpoint. Authorized retained reads still work.

**RED.** Extend the existing publication replay fixture with this ordering;
do not precede replay with a fresh stale write that poisons the state. Record
the exact focused selectors before RED.

**GREEN.** Apply the existing current-lease confirmation at the shared active
publication receipt/success boundary outside the state lock, covering JSON
acceptance and file preflight while preserving valid replay and direct behavior.

**REFACTOR.** Keep one ordinary publication-authority check at its existing
owner; no SQL execution head, historical-read lease gate or new takeover protocol.

### 6. Prepare checkpoints through the bounded writer

**Behavior.** Frozen full-state JSON/Variant and Arrow preparation use the
existing bounded writer mechanism; holding preparation/admission does not hold
the RunState mutex or stop independent execution memory progress. Coalescing,
receipts and retry identity remain exact under cancellation and backpressure.

**RED.** Extend the existing controlled writer/state seams to observe the
preparation boundary and bounded admission; current inline JSON conversion
occurs before that boundary. Record the exact focused selectors before RED,
using controlled signals rather than sleeps or synthetic host load.

**GREEN.** Move full snapshot preparation to the existing admitted writer path,
encode once per frozen checkpoint and preserve capacity/byte/lifetime ownership
and stable retries across local/peer delivery.

**REFACTOR.** Consolidate preparation with the existing frame/chunk machinery;
add no unbounded blocking task stream or separate writer. Record actual state/
encoded bytes, revision counts and bounded concurrent-run observations without
claiming a machine-independent throughput gain.

Record preparation time and acknowledgement latency alongside those bytes and
counts in this task's evidence. State the workflow shape, concurrency, storage
configuration and observation method. Use representative existing journeys and
controlled test inputs; no synthetic host load, unsupported throughput promise,
delta-journal implementation or separate benchmark project is required.

### 7. Preserve external-call identity across uncertain recovery

**Behavior.** An interrupted logical call retains its identity when recovery
repeats it. A provider integration with existing idempotency support receives
the same key on that recovery; intentional distinct calls use distinct keys.
A provider without that support may execute again, and an acknowledged
settlement is reused without calling it again.

**RED.** Verify logical identity through the existing checkpoint recovery seam.
Separately inspect actual provider adapters for supported idempotency behavior;
do not treat publication/state tests as proof of provider HTTP requests. Name
the supported adapter and its real proving target, then record the exact focused
selector before running it. If required existing propagation/restoration is
missing, expose it through that adapter's controlled seam. If correct, record
verification-only evidence. If no supported adapter is established, record that
limitation; no artificial RED or new provider capability is required.

**GREEN.** Correct only missing identity restoration/propagation at existing
invocation and supported provider owners; conserve original bounds and do not
add unsupported provider headers or a new idempotency protocol.

**REFACTOR.** Reuse existing logical identity and adapter capability handling;
retain the explicit possible-repeat contract for unsupported providers.

Rustdoc correction23 uses static inspection of surviving items and normal
format/lint checks alongside these scenarios; it requires no manufactured RED.

## Authority and proof reconciliation

Align the active documents with the already supplied user directions while
preserving historical evidence and independent report provenance:

- Spec REQ-057/INV-018/AC-049; supplement recovery/stale-worker and decision4,
  DUR-009/010; Bifrost checkpoint acceptance paragraph; Wyrd execution ownership;
  operations reliability/recovery: reject the new SQL authority and qualify universal stale-worker
  promises with the accepted rare interval. Preserve normal controls, complete
  accepted recovery and retained generic batch/replay guarantees.
- Spec REQ-055; supplement terminal/result ACK flow and DUR-004; Wyrd final-success
  wording: explicitly preserve the direct
  final input/output/result fast-return exception, without weakening separately
  governed progress/publication boundaries or queued delivery.
- The original task's generic internal-fence-SQL exception must not be read as
  approval for `scribe_state_heads`. Preserve historical implementation tables
  as historical, with current supersession clearly recorded. Do not fabricate
  retroactive approval or remove the rejected design from history by rewriting
  applied migrations/review evidence.
- Preserve implemented TASK-001, including Revision1/scenario3, takeover proof
  and selected correction5, as historical evidence. Record their superseding
  instructions and all new proof here; do not reopen or amend that task to
  assign the current work.

These are bounded reconciliations of explicit user authority, not permission
for a new persistent ownership decision. If an actual remaining requirement
cannot be met through these existing owners, report the concrete source
conflict; do not solve it with another prohibited SQL mechanism.

## Acceptance and verification

| Work / finding | Required observation |
| --- | --- |
| USER-PG-001; deletion closure for24/25 | Final installed schema and production local/peer/read/replay paths have no head table/replacement authority or claim/read/advance dependency; accepted Bifrost evidence and migration checksums are preserved. |
| USER-PG-001 recovery | Complete accepted checkpoint/absence or typed source failure, including an owner unavailable before discovery, publication handover and retained-volume replay; no ordinary latest-visible fallback. |
| FIND-17 | Full sanitized terminal/error/status/identity from canonical data for existing terminal producers; no sole SQL historical error source, no new service/handoff or direct final ACK wait. |
| FIND-16 | Exact canonical selected result and origin-neutral inspection/content survive source/queue-parent removal with current grants/tenant isolation and stable cut/File selection. |
| FIND-19 | Last-attempt durable selected result completes after failed queue COMMIT without computation; unfinished-work exhaustion/budgets/deadline/cancel remain. |
| FIND-21 | Stale-first JSON/file replay refuses through the ordinary active authority check; valid same-call replay adds no revision and retained inspection needs no running lease. |
| FIND-22 | All frozen snapshot preparation is owned by the existing bounded writer path; independent memory progress, exact coverage, byte/reservation accounting and retry/drain semantics remain. |
| FIND-23 | Surviving verified tests/helpers and all newly changed Rust items have required intent/Errors/Panics documentation; deleted head-only items require no repair. |
| Approved SQL / adjacent behavior | Inventory-preserved queue/admission/renewal, File, group/comparison/Operator and generic batch/source roles remain; existing short-lock/coverage/receipt/lane behavior is preserved. |
| USER-DURABILITY-001 identity | Uncertain retries retain logical identity and supported provider idempotency keys; distinct calls remain distinct, acknowledged settlements are reused, unsupported providers retain possible-repeat semantics. |
| USER-DURABILITY-001 recovery/format | Lost acceptance replies reconcile through the original write identity across restart; supported checkpoint formats recover and incompatible/malformed formats refuse without reset or dispatch. |
| USER-DURABILITY-001 retention/cost | Actual existing retention boundaries are recorded and exercised; snapshot bytes/counts, preparation time and ACK latency have contextual evidence here, with no new storage format or unsupported performance claim. |

Use the existing targets below, narrowing to exact changed/proving tests after
registration; record each scenario's exact command and RED/GREEN output before
claiming evidence. These recipes are not a requirement to run every unrelated
case or the full language journeys. Existing named regression selectors are
confirmed; new selectors remain implementer-owned and must not select zero tests.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::state::tests::/) | test(/^scribe_outbox::tests::/)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(/^pg_durable_verifier_work::/)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::reclaimed_attempt_after_a_failed_settlement_redelivers_the_selected_result) | test(=pg_verification_runtime::retryable_engine_failures_exhaust_to_errored)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-sql --test integration --test-threads=1 -E "test(/^pg_scribe_batch_commits::/) | test(/^pg_stream_identity::/)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all --test-threads=1 -E "test(/^lifecycle::/)"'
```

Retarget obsolete head-specific assertions with the source/authority diagnosis
above; preserve meaningful generic batch/recovery/publication/tenant coverage.
Use only the exact changed lifecycle/replay selectors needed for closure. The
repository setup runs both migration apply/idempotence checks; additionally
exercise installed-head retirement and old payload custody refusal where the
migration write set requires it. Keep CPU/IO work and shared admission bounded.

Run format/lints and applicable codegen/generated-doc, dependency and tenant
checks for the actual Rust/proto/SQL/generated write set, plus `git diff --check`.
No broad `gate`, full journey suite, throughput claim or synthetic host load is
required. Missing rustdoc on changed items remains a merge gate; do not document
or refactor rejected code solely to keep it alive.

## Evidence to record during implementation

Keep all new research-follow-up evidence here, not in implemented TASK-001.
Record exact selectors/results for lost-ACK recovery, compatible/incompatible
formats, external-call identity/provider support and retention boundaries.
For preparation observations, record workflow shape, configuration, snapshot
and encoded bytes, revision count, preparation duration, acknowledgement latency
and the concurrent-progress observation. No measurements have been run by
drafting this addition; historical tests are not its proof.

## Completion boundaries

This is the one active handoff for USER-PG-001 and every remaining finding:
16,17,19,21,22,23 plus deletion closure for24/25. Finding18 is withdrawn and20's
stronger protocol is superseded. There is no separate follow-up packet for21–23.
Historical task/review artifacts remain evidence only; implement this consolidated
R3 task and review its complete result against the original task and current
user decisions.

The remediated result still needs independent acceptance against original
TASK-001's obligations and integrated change proof. All remaining work is owned
here. No finding is closed by consolidation or deletion plans alone.
No source, migration or tests have been changed by drafting this task;
historical green runs are not closure evidence for this remediation.

## Implementation evidence

Commits: `74293e219` (USER-PG-001, FIND-21, FIND-22), `63d96450d` (FIND-19),
`591a2fa13` (FIND-17, FIND-16), plus a rustdoc placement fix in the final
working-tree diff. Authority-document reconciliation is in the working tree
(`architecture/*`, `spec.md`, `execution-state-and-durability.md`) and is not
committed.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| USER-PG-001; deletion closure for 24/25 | New migration `vala-sql/migrations/20261108000000_retire_scribe_state_heads.sql` drops the table; applied migrations are unchanged. `RunState::accepted_revision` probes `revision_fence(tenant, execution, n)` = the outbox digest batch id of `revision_batch(execution, n)` in chunks of 256 and takes the contiguous fenced prefix. Head queries, `committed_batches` head reads, and `Conflicted` are deleted (`BatchConflict` keeps the generic role). | `pg_migration::*` for both crates (apply + idempotence); `pg_durable_verifier_work::batched_checkpoints_recover_exactly_the_accepted_batch`; `lifecycle::execution_recovery_requires_exact_accepted_state`; `pg_scribe_batch_commits::*`, `pg_stream_identity::*` 10/10 | PASS |
| USER-PG-001 recovery | Absence = revision 0 fresh; a fenced but invisible revision is `StateError::Unavailable` (wait/retry), never an older one. | `lifecycle::execution_recovery_requires_exact_accepted_state`, `lifecycle::execution_takeover_rejects_late_writes_after_replay`, `lifecycle::scribe_shutdown_drains_or_preserves_replay` (lifecycle 8/8) | PASS |
| FIND-17 | Format 2 `ExecutionState.identity: ExecutionIdentity` + `failure: TerminalFailure`. Producers: runner `retained()` (engine Terminate, final-attempt Retry, exhausted claim); direct `verify` error path (`service.rs`, `state.fail`); user cancel (`inspection.rs::retain_cancellation` after the queue commit, as the canceller, identity from state or else the registry Cards). `get_execution` takes status from `Retained::status()` and error from `state.failure`. | `pg_durable_verifier_work::failures_read_back_from_state_instead_of_the_queue_payload` (state error `engine_failed` beats queue `queue_only`; later failure does not replace it); `pg_durable_verifier_work::cancellation_reads_back_from_state_after_the_queue_and_cards_are_gone` (no prior checkpoint); `wyrd-spec verification::state` format-1 refusal | PASS |
| FIND-16 | `VerificationControl::retained` reads the queue row optionally, then the accepted state, and authorizes `verifier:read` on the queue row's Verifier or the recorded identity's. `get_execution` builds the result from `state.terminal.result_id` and recorded refs; `InputResolver::direct`/`DirectResult` and the Card lookups are deleted. | Both tests above delete the queue row and both Cards, then GET returns 200 for a `viewer` and 403 for a user with no role; `terminal_selection_and_inspection_use_run_state`; `direct_and_queued_use_bifrost_run_state` | PASS |
| FIND-19 | CLAIM_SQL claims expired final-attempt leases with `exhausted = true` and does not count an attempt; the runner delivers the selected terminal or settles `lease_expired`. | `pg_verification_runtime::final_attempt_after_a_failed_settlement_completes_instead_of_exhausting`, `…reclaimed_attempt_after_a_failed_settlement_redelivers_the_selected_result`, `…retryable_engine_failures_exhaust_to_errored`; `pg_verifier_executions::terminal_failures_and_expired_final_attempts_carry_no_verdict` (33/33) | PASS |
| FIND-21 | `receipt_once_accepted` confirms the held lease before answering a replay. | `pg_durable_verifier_work::publication_replay_preserves_attempt_identity` (stale JSON replay is the first op; file replay and new write follow; live replay adds no revision) | PASS |
| FIND-22 | `Checkpoint` freezes the revision; `Part::Checkpoint` prepares its row once (`OnceLock`) on the outbox's spawn_blocking encode pool, charged at the stable `weight()`. | `scribe_outbox::tests::*`, `verification::state::tests::*` (25/25); `state_format_changes_preserve_existing_custody` | PASS |
| FIND-23 | Rustdoc on every new or changed item; the `failure()` doc split by the FIND-19 insert was rejoined. | `cargo clippy -p wyrd-server -p wyrd-testing -p wyrd-spec -p wyrd-sql --all-targets -D warnings` clean | PASS |
| Approved SQL / adjacent | Queue admission, renewal, cancel, revoke and release are unchanged except the exhausted-claim column. | `pg_verification_runtime::*` and `pg_durable_verifier_work::*` 76/76 | PASS |
| USER-DURABILITY-001 identity | `skald-providers/src/clients/{anthropic,openai,google,vertex,jev,embeddings}.rs` send no idempotency header; the only headers built are content type/length and auth (`clients/mod.rs`). No adapter supports provider idempotency, so none is sent: an uncertain recovery may repeat a provider call under the same invocation identity (possible-repeat semantics). Acknowledged settlements are reused from state. | `invocations_resume_from_bifrost_with_original_bounds`, `publication_replay_preserves_attempt_identity` | PASS (limitation recorded) |
| USER-DURABILITY-001 recovery/format | Lost replies reconcile through the deterministic revision batch id; format 1 (never shipped) and newer formats refuse as `Incompatible` without reset. | `state_format_changes_preserve_existing_custody`, `lifecycle::scribe_tick_finishes_a_claim_that_failed_after_its_commit`, `lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates` | PASS |
| USER-DURABILITY-001 retention/cost | See retention and measurements below. | Measurement probe (not committed) | RECORDED |

### Retention boundaries (existing, unchanged)

- `vala.scribe_batch_commits` fences: no delete or prune path exists in `vala-sql`; they are kept as long as the database is.
- `wyrd.verifier_executions`: no delete path; cancel, revoke and settle update the row in place.
- `bifrost.verification.execution_events`: no row-level retention. Forge snapshot expiry (`forge/expiry_policy.rs`) only removes replaced snapshots older than the retention window, and that window is never shorter than the maximum query deadline. Compaction keeps live rows.
- The supplement names no supported recovery interval, and none was chosen. Recovery holds as long as the fences and the current table snapshot exist, and nothing currently removes them. This gap belongs to policy and is reported here; it is not closed.

### Preparation and acknowledgement measurements

- Shape: one in-process `WyrdTestServer` with the repository Postgres. One leased execution makes 100 sequential `begin` revisions, plus a 4 KiB JSON publication every 10th revision (100 invocations and 10 publications at the end). Debug build, default `HostedLimits`.
- Results:
  - Final snapshot: 54,428 JSON bytes.
  - `Checkpoint::weight()` charges 138,972 bytes, so the estimate is conservative (about 2.6×).
  - One `Checkpoint::row()` preparation of the final snapshot took 1.48 ms.
  - Per-revision ACK latency: p50 68.6 ms, p95 75.1 ms, max 108.1 ms. This is dominated by the Scribe flush cadence and does not measure throughput.
- Concurrent progress: covered by `cd905f4cd`'s sibling-step test and by `batched_checkpoints_recover_exactly_the_accepted_batch`. No new claim is made.

### Diagnosis

- **Symptom:** after the head table was removed, PG recovery read `None` at revision ≥1.
- **Evidence:** `accepted_revision` probed `revision_batch(execution, n)`, but Scribe fences the outbox digest `batch_id(tenant, table, (key, 0, 0), 0)`.
- **Cause:** the code probed the logical key instead of the fenced id.
- **Fix site:** `scribe_outbox::revision_batch_id`, used through `state::revision_fence` by recovery and the lifecycle test. Nothing else probes revision fences.

### Limitations

- Failures before a Target exists (Verifier unavailable, or no SYSTEM principal) have no recorded identity, so only the queue holds them.
- Principal-revocation bulk cancellation (`cancel_principal`) is not mirrored into state; only the queue holds it.
- A cancellation's state record is best-effort after the commit. A failure is logged, and the cancel still stands.
- A registry-fallback identity for an execution cancelled before its first claim has no group or owner.
- Direct-verify failure recording waits for state acceptance on the error path. Success returns are unchanged.
- The two new FIND-17/16 tests passed on their first run. RED is by construction: before the change, `get_execution` required the queue row (404 once it was deleted) and returned the queue's error payload.
- The rare delayed-old-worker interval is accepted and documented in REQ-057 and the supplement.

## Successor after independent review and user approval

R3 is implemented historical context at d89356985. The independent cumulative
review and subsequent explicitly approved uniform ACK-before-queue-settlement
mechanism are packaged in [TASK-001-R4](../task-001-r3-d89356985/TASK-001-R4-uniform-terminal-settlement.md),
the sole active remediation handoff. Preserve the evidence above; new closure
evidence belongs in R4. Original TASK001 is not reopened.
