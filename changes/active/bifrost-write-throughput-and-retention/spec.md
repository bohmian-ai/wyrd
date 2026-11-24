---
id: SPEC-bifrost-write-throughput-and-retention
revision: 4
status: approved
---

# Bounded Scribe write pipelining

## Problem and agreement

The serial shard loop leaves later WAL work waiting while an earlier group
awaits PostgreSQL acceptance. Interleave consecutive groups to keep writes
progressing during that wait, preserving durable completion and recovery.

The maintainer agreed to the [pipeline proposal](design/pipeline-implementation-proposal.md)
and explicitly requested this aligned specification and one implementation task.
Revision 4 records that agreement. It replaces the earlier serial scheduling and
benchmark-per-scenario delivery direction. Previous retention and CPU work is
existing context, not work to repeat in this pipeline task. Its contracts remain
protected by [Bifrost authority](../../../architecture/bifrost-design.md).
The prior packet is retained under `review/revision-3-packet/` as historical evidence,
not executable planning; its relative links retain their original packet context.

## Required behavior

**REQ-014 — Repeated bounded overlap.** Each shard has exactly two group slots,
including collection/preparation, one ordered WAL producer and one tenant-fencing
operation at a time. After N is synced, N+1 can write and sync while N fences.
After N settles, reuse its slot for subsequent groups whenever eligible input and
resources permit. Only the oldest group may fence or publish visibility. Two
occupied slots stop further dispatch through existing bounded admission.

**REQ-015 — Independent completion and collection.** Publish accepted rows and
inspection visibility and update the committed mirror before success ACK. N's
ACK never waits on N+1's unrelated IO. Service ready head completion before new
preparation; bounded mailbox/preparation work cannot starve it. Preserve tenant/
table fairness, per-table FIFO, existing byte limits and the 64-request cap.
For an overlapping slot, drain eligible requests; below a soft target of 32,
wait for arrivals only until 1 ms from the first collected request. Do not reset
that deadline. Continue draining ready requests above 32 to existing limits.
The soft target ends waiting for new arrivals, not draining already-ready input.
Dispatch once ready input is drained and the target is met, or on cap, deadline
or predecessor completion; idle dispatch has
no intentional delay. These are private initial constants, not public options.

**REQ-016 — Identity and durable acceptance.** Exclude an in-flight
`(tenant, table, batch_id)` from the other slot until its owner settles, preserving
per-table FIFO while other eligible tables proceed. Keep batched tenant SQL,
retained-version comparisons, conflict classification and cross-pod PostgreSQL
arbitration. Success requires synced SLICE/COMMIT segments, accepted fencing and
visibility. A synced but unfenced attempt remains provisional. No additional
normal ACK-path SQL, sync or cleanup wait is introduced.

**REQ-017 — Explicit failure ownership.** A known failure with no accepted or
uncertain batches drains submitted IO, refuses affected waiters and releases
settled ownership before allowing progress. Normal per-batch conflicts retain
existing rollback/reoffer behavior. If earlier tenants accepted before a later
tenant operational failure, or bounded SQL reconciliation cannot resolve
acceptance, stop dispatch and later fencing, retain both slots and charges,
drain submitted IO and use existing fail-closed restart recovery. Apply the
same retained-ownership rule to post-fence insertion failure. Recovery restores
canonical accepted batches, including lost responses, and suppresses unfenced
attempts. Never undo a prior group's visibility or ACK because a later group fails.

**REQ-018 — Lifecycle and resource barriers.** Charge both slots and conversion
ownership through existing global/tenant/table accounting until actual settlement
or transfer. Project pending generation bytes exactly once before later WAL
mutation. When rotation, selective sealing or pressure-driven generation mutation
requires a barrier, stop replenishing slots, settle dispatched ownership, then
mutate the generation through the existing owner. Shutdown drains submitted work
and preserves unresolved durable ownership for restart. Preserve existing FlushAll
queue semantics. Client cancellation and event-loop selection must not abandon
submitted operations. Receipt cleanup and recovery-safe frontier advancement
remain governed by actual durable custody, never provisional WAL progress.

**REQ-019 — Prove progress before performance qualification.** Deterministic
handshakes through the production coordinator and real WAL/SQL boundaries must
prove repeated slot reuse, later writes during earlier fencing, independent ACKs,
ordering, bounded work and failure recovery. No elapsed-time speed threshold or
quiet host is required for these tests. This establishes the pipeline's ability
to increase throughput by removing serial dependencies under controlled stage
blocking; it does not establish measured production throughput or CPU gains.

**REQ-020 — Maintainer-run benchmark is the final closeout step.** After all
implementation, correctness, journey and required format/lint checks are complete,
leave the task at verification complete, performance pending. The maintainer
independently runs `mise run benchmark:l1:bifrost-writes` as the last required
execution step. All eight routine cases must be VALID. Compare matching cases
only against [the designated baseline](design/l1-baseline-74034459d.md), with
repeatable concurrent-write improvement beyond observed variation and no repeatable
regression in single-client behavior, CPU efficiency or existing read protections.
Do not require intermediate benchmarks or a fresh baseline run before implementation.
Earlier metrics, latency claims and numerical targets are excluded. Mixed,
incompatible or inconclusive results leave qualification pending; a VALID exit
alone does not establish improvement. Recording and reviewing the final evidence
may follow the run; no additional implementation gate follows it. Required read
or sustained-load qualification, where applicable, occurs before this final run.

## Scope and implementation flow

Extend the existing shard coordinator and GroupWalState; split WAL durability
from fence resolution and visibility. Reuse the WAL IO lane and existing SQL
owner, retaining pending operations across coordinator polls. No duplicate
process_group loops, generic pipeline framework, new runtime, deeper pipeline,
extra shards, cross-group sync aggregation, WAL-only acceptance, public knobs,
wire/storage format changes or independent resource ledger.

Implementation owns helper layout and test hook shape. Update affected owning
architecture language from serial scheduling to bounded overlap while retaining
acceptance, custody and cleanup contracts. No redesign of retention or the prior
CPU optimization work is authorized.

## Acceptance and delivery

Exactly one task implements REQ-014–020 in ordered test-first scenarios: overlap,
collection/identity, failure/lifecycle, connected journeys and deferred closeout.
The task maintains the single scenario evidence list. Existing acceptance,
tenancy, recovery and permission safeguards remain mandatory.

Revision 4 is approved by the maintainer's explicit agreement and instruction to
write the aligned specification and plan. It authorizes planning, not automatic
implementation, staging or commits.
