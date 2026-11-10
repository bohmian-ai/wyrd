---
id: SUPPLEMENT-verifier-execution-state-and-durability
revision: 3
status: approved
parent: SPEC-verifier-review-pipelines
---

# Verification execution state and durability

## Purpose and authority

One in-memory state object drives a verification execution. The engine attempts
to restore that state from Bifrost whenever it enters the execution, regardless
of whether its caller is synchronous verification, a queue delivery, or a
retry. The existing Bifrost shared writer persists progress and results.
PostgreSQL coordinates queued work; it is not a second recovery-state store.

This supplement records the agreed execution boundary and data flows for
[the existing specification](spec.md). It is the single place to resolve the
approved execution-state and durability decisions used by the revised tasks.
It does not create implementation tasks or claim implementation readiness.

The user has rejected the earlier durability architecture. In particular,
revision 17's PostgreSQL block checkpoints, separate direct-publication state,
and active-to-retained archival handoff are not an approved implementation
mandate for this work. Their presence in the parent spec, supplements,
architecture documents, or remediation packet does not settle these decisions.
The user explicitly approved decisions 1–5 and the clarified stale-worker
protection in revision 2. This contract governs the parent specification and
owning architecture; documentation reconciliation does not certify implemented
behavior. Task 001 establishes and proves the required Bifrost extensions.

Existing authoring, publication naming, tool arguments/results, immutable
content, reference selection, permissions, and resource limits remain in scope
under their existing contracts. This supplement changes where execution state
is owned and recovered, not those contracts.

## Scope and limits

The principal subject is everything inside **execute verification**. Entry
paths are shown to make that boundary unambiguous. Queue admission, polling,
claims, renewals, and retry scheduling keep their existing PostgreSQL owners.
Other surrounding evidence writes use the existing Bifrost shared writer.
Existing registry, authorization, and shared upload control-plane operations
retain their owners; this is not a replacement of all server PostgreSQL use.

No additional batching writer, workflow service, recovery database, application
WAL, direct object-storage checkpoint protocol, archival catalog, or
publication storage system is authorized. The approved changes extend existing
Bifrost records, recovery reads, and checkpoint acceptance. They introduce no
second lease system, separately selected checkpoint table, retention period,
or public resume API. PostgreSQL remains the queue lease owner.

The cost of recoverability is waiting for durable progress at consequential
boundaries. Batching shares the physical write work across executions; it does
not make an in-memory enqueue durable or remove all acknowledgement waits.

## Entry paths

### Synchronous `verify`

```text
caller: verify(request)
  -> existing validation, authorization, Verifier/input resolution
  -> execute verification(execution identity, resolved context)
  -> return the verification result or execution error
```

Synchronous verification does not enter the PostgreSQL queue. Its execution
uses the same state ownership and persistence rules as submitted work.
Fresh synchronous requests have distinct execution identities; this change
does not add concurrent same-execution direct resumption.
Persistent state does not by itself authorize background resumption after a
direct caller disconnects. This supplement adds no direct-request scheduler.

### Asynchronous `submit`

```text
caller: submit(request)
  -> existing admission and PostgreSQL queue record
  -> return submission/execution identity

poller:
  -> claim PostgreSQL queue record
  -> resolve the accepted execution context
  -> execute verification(the same execution identity, resolved context)
  -> complete, release, or schedule retry through the existing queue owner
```

The queue exists before the actual verification execution. No engine state is
created or checkpointed as a prerequisite to inserting the queue record.
Existing admission may retain request inputs through Bifrost before accepting
the queue record. Request-input custody is not an execution checkpoint; this
supplement does not reorder or duplicate that admission workflow.

A queue retry re-enters execute verification with the original execution
identity. It does not automatically create another verification run. A
deliberate new execution remains distinct under the existing request contract.
The optional application `run_id` is correlation context; it must not silently
replace `execution_id` as the verification recovery identity.

### Scheduled and group executions

Schedule/group activation, debounce, admission, queue claims, renewals and retry
scheduling keep their existing owners. They enter the same queued execution
path and the same engine state lifecycle. A retry preserves the original
scheduled window and pinned group inputs instead of resolving a new execution.
Group snapshot deduplication, previous-verdict comparison and Operator dispatch
must preserve their behavior when historical execution reads move to Bifrost;
this change does not serialize whole group executions or redesign activation.

## Inside execute verification

```text
enter with execution identity and accepted context
  -> attempt to load that execution's recoverable state from Bifrost
       absent, authoritatively: initialize fresh in-memory state
       present and compatible: restore in-memory state
       unavailable/incomplete/incompatible: fail or wait; never start fresh
  -> if terminal, return its recorded result/error without executing steps
  -> otherwise execute remaining work from the state
       inputs and dependency results are read from memory
       starts, settlements, decisions, and publications update the state
       recovery-relevant progress goes through the existing shared writer
  -> record the selected final outcome through that same writer
  -> return after the required acknowledgements
```

The engine does not take separate queue-retry and crash-recovery paths. Its
operation is load, restore or initialize, then continue. Queue retry policy
determines when it is called again; persisted execution state determines what
work remains eligible when it is called.

### What the state holds

| State information | Why it must survive |
| --- | --- |
| Execution and tenant identity, exact Verifier/configuration and accepted attribution | Resume the same execution against the same declaration and security boundary. |
| Pinned inputs and exact evidence references | Avoid resolving a different input set on retry. Large bytes remain with their existing storage owners. |
| Original deadline, spent/remaining budgets, invocation identities and attempts | Prevent retries from resetting time or increasing permitted work. |
| Complete action and requirement settlements, including downstream values and lineage | Reuse completed work without calling the provider or rereading a database between steps. |
| Fanout membership, iteration progress, conditional/speculative decisions and dispositions | Preserve the actual scheduling decisions as those capabilities are implemented. |
| Accepted publication identity, JSON or manifest, exact completed-file descriptors, and cumulative accounting | Preserve receipts, read exact bytes, and enforce limits across retries. |
| Execution status, cancellation/termination decision, selected result identity and final outcome | Preserve terminal decisions and avoid selecting an abandoned result. |
| Existing inspection/event ordering and progress needed to reproduce accepted history | Keep public inspection consistent with accepted execution progress. |

The existing engine ledger and logical settlement types are the starting
point. This does not mean serializing an engine instance. Connections,
credentials, futures, semaphores, scratch workspaces, and model conversations
are not the durable state object. The runtime reconstructs its capabilities
around recovered data. Recovery is not deterministic replay of engine code or
resumption at an arbitrary instruction inside an Agent call.

### How an update is recorded

```text
engine changes its in-memory run state
  -> represents a complete logical checkpoint as a Bifrost write
  -> reserves capacity in the existing ScribeOutbox
  -> outbox batches it with compatible writes from other executions
  -> Scribe syncs WAL and accepts the checkpoint under current ownership
  -> existing acknowledgement returns through the outbox
  -> engine releases the work waiting on that durable boundary
```

Each checkpoint contains the complete logical recovery state, with a format
version and accepted revision, in the existing canonical Bifrost verification
data. Large content remains referenced through existing storage. Complete
checkpoints are required at the boundaries below, not for every memory
mutation. Physical encoding remains implementation-owned within that contract;
no separate checkpoint table is selected.

| Boundary | Progress that must be acknowledged before continuing |
| --- | --- |
| Starting an external invocation | Its identity and started attempt, so an ambiguous call consumes the correct budget. |
| Releasing dependent work | The complete upstream settlement and applicable admission/control decision. |
| Returning a successful publication receipt | The accepted publication, exact content association and cumulative accounting. |
| Returning a terminal execution outcome | The selected outcome and every record required to retrieve that same result. |

Ordinary local computation does not require a database write for every memory
mutation. Concurrent independent starts or settlements can share a persistence
batch where their dependency boundaries permit it. Different executions share
the existing outbox's batching and backpressure. Recovery writes use reserved
capacity and receipt-bearing delivery; best-effort shedding is unacceptable.

### What acknowledgement means

The boundary is the existing **Scribe WAL acknowledgement**. It includes a
durable WAL append/batch fence and authoritative active rows. An outbox enqueue
is process memory; an in-memory table alone is not durability.

Checkpoint acknowledgement does not wait for object publication, Iceberg
promotion, or compaction. These remain Bifrost background operations. This
does not change file-upload completion: accepted file bytes must already have
completed through the existing upload machinery.

A verification worker can recover using surviving Bifrost state. If the needed
state is still held by an unavailable Scribe's WAL, recovery may have to wait
for that Scribe and its persistent volume. Permanent loss of that disk before
background publication is not covered by the existing acknowledgement. No
remote WAL replication or stronger shared-storage acknowledgement is approved
by this supplement.

Checkpoint ownership/acceptance and authoritative recovery loading require the
approved extensions described below. The existing WAL durability boundary is
reused; it must not be mistaken for proof that those extensions already exist.

### How results are recorded

An invocation result first becomes a complete settlement in the in-memory
state. Its acknowledged persistence makes it available for recovery. The
running engine supplies that settlement directly to dependent steps.

The final verification result follows the existing result-building path:

```text
engine produces final VerificationOutcome
  -> retain the selected outcome/result identity in run state
  -> existing result builder produces detail and summary records
  -> existing outbox writes execution_outputs and results
  -> persist the state information identifying that selected outcome
  -> await all required acknowledgements
  -> return the same selected result
```

The existing `execution_inputs`, `execution_outputs`, and `results` meanings
remain intact. Persistence of selection is necessary because unsuccessful or
abandoned deliveries can leave other result records. A reader must not choose
the result with the latest timestamp. A retry after result persistence but
before queue completion returns the recorded terminal outcome and allows the
queue owner to finish its bookkeeping without another provider call.

The state must preserve stable identities across an ambiguous write/reply.
Cross-record partial delivery must be recoverable through the existing writer
and result-delivery lifecycle before terminal success is acknowledged. This
does not authorize a new result format or a second finalization service.
Direct verify is the exception at final success: it returns its final input,
output and result without waiting for their acknowledgement, while its
progress and publication boundaries keep theirs.

### Approved terminal settlement — revision 3

Every queued terminal path uses this order: select the terminal decision and
stop computation; record the complete canonical terminal state and required
result records through the existing Bifrost writer; receive its WAL/batch-commit
acknowledgement; only then settle the queue work fully. This applies to success,
error, timeout, exhaustion, cancellation, revocation and pre-engine/admission
refusal. Outbox enqueue or a logged write failure is not acknowledgement.

The existing PostgreSQL queue owns durable pending-delivery coordination until
that acknowledgement. It may retain the minimal sanitized terminal decision
needed to retry delivery, alongside stable execution/delivery identity, due/lease
coordination and delivery completion. This is an explicitly approved, narrow
terminal-delivery responsibility, not a second progress/checkpoint/result or File
payload store. Bifrost remains canonical for execution recovery and retained
history. No separate table family, writer, service or general event projector is
selected. Private schema and method choices remain implementation-owned.

Cancellation/revocation immediately and atomically removes computation
eligibility under existing security rules, independently of Bifrost availability.
That control commit retains pending terminal delivery durably. A crash after it
and before enqueue must leave discoverable work for an existing queue worker.
The worker retries only the original terminal delivery using server delivery
permission, never the cancelled principal's execution authority or a provider
call. A repeated request is not required to repair the record. Pre-engine and
refused work retain exact immutable identity without inventing accepted inputs
or runnable authority. Queue delivery can retire only after all records required
to retrieve the same terminal decision are acknowledged.

If a process dies after Bifrost acceptance but before queue settlement, the next
claim reads the same accepted decision and idempotently finishes settlement.
Stable execution/result/write identities, generic batch fences, existing bounded
admission and retry/lease ownership are reused. A temporary source/writer outage
keeps delivery pending; it cannot convert unknown state into exhaustion or a new
terminal decision. The computation deadline/budget never restarts; expiration
stops computation but does not discard already-required terminal delivery.
Independent runs continue under normal queue fairness and bounded writer capacity.

Direct successful verification retains its approved queue-free fast return;
this amendment adds no direct background scheduler or final-record ACK wait.
Suspension is not a terminal workflow state: TASK-007 applies the same ACK-before-
settlement order to its current queue delivery, saving the waiting checkpoint
before releasing all active resources. A later trigger admits a distinct queue
item for the same execution through SubmissionSink. This amendment does not
implement suspension or resolve its other open contracts.

Required proof crashes before enqueue, after WAL acceptance with a lost reply,
and after ACK before queue settlement, then restarts without the original caller.
The same terminal decision must eventually be retrievable independently of queue
and source Cards, with no renewed computation, duplicate result or repeated
completion effects. Exercise success, engine failure/timeout/exhaustion, explicit
and principal cancellation, inactive admission, pre-target and group refusal at
their actual owners. Preserve immediate authority retirement in outage cases.

## Recovery and retry

### One recovery sequence

1. Establish the existing execution identity and accepted authority. On queued
   takeover, shut out the previous writer through Bifrost before continuing.
2. Load recoverable state through Bifrost. Require an authoritative result
   before concluding there is no prior state.
3. Validate its format, pinned identity, original bounds, and termination state.
4. Rebuild the in-memory ledger and execution context from that data.
5. Reuse settled invocations and accepted publications. Apply existing retry
   policy to interrupted work; do not reset spent attempts or the deadline.
6. Continue through the same execution and persistence path used by a fresh run.

| Situation | Required behavior |
| --- | --- |
| No previous state, established authoritatively | Start fresh when the execution is entered. |
| A settled; B started but never settled | Restore A. Count B's attempt as interrupted/spent; start another attempt only if allowed. |
| A failed and its settlement was recorded | Restore the settled failure. A queue retry alone does not erase it or reset its retry policy. |
| Execution failed before it could settle/checkpoint work | A later invocation loads the last recoverable progress and handles the uncertain work under remaining bounds. |
| Terminal result persisted; caller or queue completion reply lost | Return the recorded terminal outcome; do not execute steps again. |
| Cancelled or original deadline elapsed | Preserve the decision and stop new work. Existing read/cancel contracts remain applicable. |
| Capacity refusal before actual invocation work | Preserve existing no-attempt-spend behavior. |
| State unavailable, partial, corrupt, or incompatible | Explicit failure/wait; no empty state fallback or silent input substitution. |

A verification failure may cause the caller's existing queue policy to retry.
That only causes another engine entry. The engine uses recorded state to
distinguish unfinished/retryable execution from a settled failure or terminal
outcome. A failed judgment is not automatically an unexecuted invocation.

If an external call took effect but its settlement was never acknowledged,
the call can repeat. Preserve existing stable call identities and idempotency
where the external system supports them. This architecture does not promise
exactly-once external effects. Checkpoint acceptance must enforce the approved
single-writer rule below; outbox ordering or batch deduplication alone does
not provide that guarantee.

### Queue leases and stale-worker protection

PostgreSQL owns queued work claims, lease expiry and renewal. Expiry makes work
eligible for a replacement claimant; it cannot retract a provider call that
the old worker already sent. Bifrost owns acceptance of recoverable progress,
using the execution authority established by the queue. These responsibilities
do not create two copies of execution state or two lease schedulers.

The current runtime defaults are a **10-minute queue lease** and a separate
**5-minute queued execution timeout**. The lease-renewal loop checks every
100 seconds and renews after at least one third of the lease has elapsed,
extending expiry by the full lease on the PostgreSQL clock. Expired work is
reclaimable on an eligible poller claim round. These are existing defaults,
not new timing decisions in this supplement.

| Question | Owner |
| --- | --- |
| Which worker may run this queued execution, and when may another claim it? | PostgreSQL queue lease. |
| How long may the verification run, including retries? | Original execution deadline, retained in run state. |
| Which checkpoint may advance recoverable state? | Bifrost acceptance under current execution authority. |

Lease loss cancels the existing runner when detected, but cannot guarantee
that an already-sent external request stops or that a delayed write vanishes.
Current PostgreSQL checkpoint writes check the lease before acceptance. Moving
state to Bifrost must preserve that protection at its acceptance boundary.
Synchronous verify has no PostgreSQL queue lease; fresh requests use distinct
execution identities and retain their existing request/deadline lifecycle.

```text
A holds the PostgreSQL queue lease and starts a slow call
  -> A loses its lease; B claims the same execution
  -> B shuts out A's checkpoint writes through Bifrost
  -> B loads the last accepted checkpoint and resumes
  -> A's late result cannot change recoverable state
```

The ownership change and checkpoint acceptance must have one authoritative
ordering: if A's checkpoint is accepted first, B inherits it; if B's takeover
is accepted first, A's checkpoint is rejected. Cancellation, lease validity
and the original execution deadline remain enforced; a stale or expired
worker cannot gain acceptance merely because a replacement has not yet run.

Implemented ordering: B's takeover is its PostgreSQL claim, and A's writes stop
at their next lease confirmation, which precedes freezing each revision. A
revision is accepted by its Scribe batch fence, whose identity derives from the
execution and revision, so of A's and B's writes of one revision only the first
fenced is accepted and the other is refused. The accepted rare interval is a
revision A froze just before the claim: it may be accepted ahead of B's, B then
refuses as stale, and the next claim inherits it. No PostgreSQL execution-head or
ownership table closes this interval; `vala.scribe_state_heads` was rejected and
retired by a new migration, with the applied ones unchanged.

Extend the existing Bifrost write/fence machinery to enforce the current writer
and expected accepted revision together. Retrying the identical accepted write
recovers its acknowledgement. A conflicting or stale write cannot advance the
accepted position. Recovering readers obtain that position and load its exact
checkpoint; delayed visibility means wait/retry, never an older checkpoint.

Rejected writes must remain rejected after WAL restart/replay. The present
batch fence is not an execution ownership fence, and existing restoration of
an unfenced WAL commit must not promote rejected progress. Accepted, rejected
and ambiguous delivery outcomes must remain distinguishable through recovery
within the existing Bifrost lifecycle. This is required Bifrost work, not an
additional application WAL or PostgreSQL checkpoint adapter.

### Publications within recovered state

Publishing and naming remain defined by [output publication](output-publication.md)
and the [original publication backend task](review/task-001-r1-365bfc94b/TASK-001-original-durable-agent-work.md),
refined by [Task 001](tasks/TASK-001-bifrost-execution-state.md) and
[Task 002](tasks/TASK-002-agent-tooling.md). Agent arguments remain `name` plus
exactly one of `path` or `value`; successful response remains `{name, kind}`.
Names remain unique within the existing producer-attempt scope. This file
introduces no new naming, receipt, or reference API.

```text
stateless Agent calls existing publish tool
  -> existing hosted tool execution uses existing shared File upload machinery
       for a file/directory, or validates the supplied JSON
  -> completed publication information returns to the verification engine
  -> engine retains accepted publication information in its run state
  -> existing shared writer persists the acceptance
  -> after acknowledgement the Agent receives the existing receipt
```

The Agent holds no verification state. The publication tool does not become a
state owner or recovery service. File bytes use the same non-Card File
upload/completion machinery used by Artifact storage; no Artifact Card
intermediary or duplicate uploader is added. Exact completed descriptors stay
with accepted state so the next step can read/mount them without discovering
them again through an inventory query.

A restored publication must retain its producer, call identity, argument
identity, accepted content, and exact descriptors. Same-call replay within
the original attempt returns the existing receipt; conflicting reuse refuses.
A new invocation attempt has its own publication scope. Publications from an
interrupted/failed attempt remain attributed to that attempt and do not become
the successful output of another attempt.

Uploaded bytes without acknowledged acceptance are not an accepted publication.
A crash after acceptance but before the response must preserve receipt replay.
A crash after publication acceptance but before invocation settlement preserves
the publication while leaving the invocation interrupted. Existing storage
retention and orphan handling remain responsible for bytes; this supplement
adds no cleanup mechanism.

## Required outcomes and acceptance evidence

| ID | Required outcome | Situation and expected observation |
| --- | --- | --- |
| DUR-001 | Entry paths remain distinct outside execution and share state handling inside it. | Exercise verify, submit/poller, and queue redelivery. Verify has no queue admission; redelivery uses the same execution identity and loader. |
| DUR-002 | The run state drives dependencies in memory. | Complete A and run dependent B while instrumenting storage access. B receives the retained complete value without a dependency-result database read. |
| DUR-003 | Existing shared batching persists progress. | Interleave multiple runs, fill capacity, retry a failed batch, and lose a reply. Writes remain bounded and identifiable; no required checkpoint is shed and no second writer appears. |
| DUR-004 | Acknowledgement precedes consequential continuation. | Pause acknowledgement at invocation start, settlement, publication acceptance and terminal delivery. The corresponding call, dependent work, tool success or final success does not escape early; direct verify's final input/output/result return is the stated exception. |
| DUR-005 | Resume conserves settled work and bounds. | Interrupt mixed settled/running/failed work. Settlements and publications survive; only eligible uncertain work repeats within original attempts/deadline. |
| DUR-006 | Failed reads never become fresh executions. | Make Bifrost state or a required WAL owner unavailable. Observe explicit failure/wait, not absence or a repeated already-settled call. |
| DUR-007 | Final selection survives partial delivery. | Crash between final record acknowledgements and queue completion. Recover the same selected result and finish delivery without new model work. |
| DUR-008 | Publication contracts and storage reuse survive recovery. | Drive real Agent file/directory/JSON/null publication, lose replies, retry calls and inspect after recovery. Existing receipts, names, exact bytes, attempt attribution and conflicts remain unchanged. |
| DUR-009 | Ownership, authority and tenant boundaries survive retries. | Exercise concurrent/stale entry, revoked authority, cancellation and cross-tenant reads/writes. No unauthorized or stale accepted progress or disclosure occurs. |
| DUR-010 | Takeover and WAL replay preserve the accepted checkpoint. | Race A's checkpoint against B's takeover, delay the accepted checkpoint's visibility, and restart Scribe after rejecting A's late write. B inherits progress accepted before takeover; later A writes never become recoverable, including after replay, except one A froze under its confirmed lease before the claim, which is accepted or refused whole; unavailable exact state causes wait/retry. |

Evidence must include pure engine tests, real Bifrost/WAL and shared-storage
integration, PostgreSQL queue integration, and the existing Rust/Python/
TypeScript/MCP/HTTP journeys affected by the task revisions. Named tests and
exact commands belong in those revised tasks. No fresh tests or throughput
benchmarks have been run for this supplement. The reported request throughput
does not establish queued verification or checkpoint throughput; performance
proof must measure the actual state sizes, batching, acknowledgement latency,
and concurrent executions without weakening durability.

## Approved decisions 1–5

The user approved these decisions and the plain-language takeover explanation.

1. **Persist complete logical run state.** Use a versioned state payload in the
   existing canonical Bifrost verification data. Checkpoint at consequential
   boundaries, combine independent changes where permitted, and keep large
   bytes in existing storage. Preserve the selected final outcome in that state.
2. **Reuse the existing shared writer and WAL acknowledgement.** No new writer
   or object-publication wait. Durability covers the existing retained-volume
   Scribe recovery model; it does not add remote WAL replication.
3. **Make recovery loading authoritative.** Return complete compatible state,
   authoritatively absent state, or explicit unavailability/incompleteness.
   Wait or fail retryably when required state is unavailable. Extend the
   existing Bifrost reader using its durable source/batch metadata; add no
   execution-side source registry or second recovery store.
4. **Allow only the current writer to advance recoverable state.** PostgreSQL
   remains the queued-work lease owner. A replacement shuts out the old writer
   through Bifrost before resuming; late writes cannot supersede accepted
   progress. Extend existing Bifrost acceptance/fence and replay behavior as
   specified above. Fresh synchronous requests remain distinct executions;
   no new concurrent direct-resume API or second lease system is introduced.
5. **Preserve execution and publication semantics.** Restore settled work,
   retry eligible interrupted work within original bounds, retain existing
   publication naming/receipts/descriptors, and use the existing result builder
   and tables. Direct disconnect does not silently schedule background work.
   Preserve replay/dedup, group comparisons, binding inspection and independent
   delivery behavior while removing their obsolete PostgreSQL execution-state
   dependencies. No new tombstones, group serialization, archive handoff or
   cleanup mechanism is authorized as a shortcut.

### Required implementation validation

These decisions close the architectural choices listed in revision 1. Their
implementation and evidence are still required. In particular, ordinary Bifrost
query success currently permits omission of an offline Scribe, and existing
batch deduplication does not fence competing execution writers. Neither is a
valid substitute for the approved contracts. Prove authoritative loading,
accepted ordering, stale-write rejection through WAL replay, complete state
reconstruction and safe adaptation of old consumers before calling the
implementation ready. If existing owners cannot satisfy an approved contract,
report the specific conflicting evidence for a spec revision rather than add
an unapproved mechanism.

## Consequences for the task and remediation packet

The task packet is reconciled against this approved supplement.
[Task 001](tasks/TASK-001-bifrost-execution-state.md) replaces the obsolete
lifecycle work and must complete before Tasks 002–006. This file alone neither revises their status
nor certifies the implementor's reported fixes. The reported original fixes
remain evidence to assess against the resolved architecture, not proof that
the rejected architecture should remain. Scenarios 13–17 are not accepted
durability decisions.

| Existing artifact | Outcome assigned under approved revision 2 |
| --- | --- |
| Task 001 (historical Task 001/R1 baseline) | Replace PostgreSQL execution-recovery ownership and rejected handoff/direct-publication mechanisms with the agreed state flow; retain valid safety/publication requirements and independently verify claimed fixes. |
| Task 002 | Preserve publish/fs.write contracts and naming; use the existing upload owner and engine state, with no stateful publication service or extra writer. |
| Task 003 | Put fanout/repeat/condition/speculation progress in the same recoverable run state and preserve the existing invocation semantics. |
| Task 004 | Keep webhook/admission/delivery queue retries distinct from verification state recovery; remove invalid dependencies on retained execution payload rows. |
| Task 005 | Preserve pinned extraction/evidence and exact descriptors across state restoration using existing library/storage owners. |
| Task 006 | Teach and inspect the resolved state flow through unchanged public contracts; remove phase/source explanations inherited only from the rejected architecture. |
| Task index, proof inventory, parent spec and supplements, owning architecture sections | Align requirements, dependencies, proofs and authority statements with the approved resolution. Do not treat the current revision-17 durability text as competing authority. |

The parent spec, focused supplements, owning architecture sections and task
packet are consolidated against revision 2. This is documentation/planning work,
not implementation. Historical review reports retain their original verdicts.

## Source grounding

- [Parent specification](spec.md), [publication contract](output-publication.md),
  [public contracts](public-contracts.md), and [task map](tasks/README.md).
- [Original Task 001 review](review/task-001-r1-365bfc94b/verdict.md) and
  [historical remediation packet](review/task-001-r1-365bfc94b/TASK-001-R1-durable-execution-guarantees.md):
  implementation/review context, not approval of the rejected architecture.
- [Wyrd doctrine and Verifier ownership](../../../architecture/wyrd-design.md#verifier),
  [Bifrost durability/visibility](../../../architecture/bifrost-design.md#durability-and-visibility),
  [tenant isolation](../../../architecture/wyrd-security-posture.md#tenant-and-data-isolation),
  and [specification workflow](../../spec-driven-development.md).
- `crates/vala/vala-verify/src/walk.rs` and `src/blocks/engine.rs`:
  existing in-memory ledger and logical attempt/settlement restoration.
- `crates/wyrd/wyrd-server/src/scribe_outbox.rs`: existing process-wide
  batching, reserved capacity, retries and receipt-bearing writes.
- `crates/wyrd/wyrd-server/src/verification/runner.rs`: queued state loading,
  current PostgreSQL checkpoint adapter, result builder and acknowledged delivery.
- `crates/wyrd/wyrd-server/src/verification/hosted/publications.rs`:
  current File completion/reconciliation, exact descriptors and accepted receipts.

## Revision history

| Revision | Status | Change |
| --- | --- | --- |
| 1 | draft | Records the agreed submit/verify boundary, engine-owned recoverable state, existing shared-writer/WAL flow, results and publication custody, retry semantics, unresolved guarantees, and required task reconciliation. |
| 2 | approved | Records explicit user approval of decisions 1–5, including complete logical checkpoints, existing WAL acknowledgement, authoritative recovery reads, Bifrost stale-writer protection, unchanged publication semantics, and scheduled/group reuse. Implementation validation and packet reconciliation remain required. |

| 3 | approved | User explicitly approves uniform queued terminal settlement: canonical Bifrost record and batch-commit ACK before queue delivery retirement; immediate cancellation/revocation retains durable pending delivery in the existing queue. Direct fast return remains. TASK-007 applies the ordering to suspended queue deliveries when implemented. |
