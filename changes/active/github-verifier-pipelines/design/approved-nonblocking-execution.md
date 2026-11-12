# Nonblocking verification execution

Status: finalized

The maintainer agreed this flow and requested revision of task 010 in the current
design conversation. The maintainer subsequently explicitly approved spec revision 7 recording
this design. Contradictory owning architecture still requires reconciliation
before affected production work.

## Outcome and scope

Direct and queued verification use the same stateful engine. Preserve
resource-scaling preparation, caches, ownership moves, batching, isolated capacity,
compact VerificationResult / retained VerificationDetail(s), and gateway memory
accounting/background capture. Necessary pre-execution cold loads remain ordinary
preparation; do not introduce cache-only refusal or a new authority cache.

This correction removes execution-time lease confirmations, outbox admission and
persistence waits. Leave Bifrost implementation, catalog, query semantics, durable
write acknowledgement and WAL/batch fencing unchanged. Agent access to Bifrost
continues through Agent tools; no new Verifier query surface is proposed.

## Connected flow

| Stage | Direct | Queued |
|---|---|---|
| Input/preparation | Existing authenticated request, authorization, validation and exact Verifier/subject resolution, including necessary cold loads | Existing admission, batched queue claim, pinned input/source/policy, accepted authority, original deadline and exact accepted recovery state |
| Execution | Shared engine, direct capacity, fresh live state | Same engine, queued capacity, prepared live state and local lease-loss token |
| Progress | Validate and mutate live state; release dependents immediately | Identical; no SQL confirmation, outbox reservation/staging or ACK in engine transitions |
| Finish | Select result in memory, return it and release execution resources | Select result in memory and release execution resources; background claim ownership continues |
| Persistence/completion | Background evidence publication under bounded existing outbox semantics | Retain the existing ClaimLoop permit and completed state, renew claim in background, retry persistence, then token-fenced batched SQL settlement |

The engine still waits for actual requested work, including model/tool/Python work
and validated physical file writes where needed. Provider response time counts
against the per-call budget. Permissions, publication identities/conflicts,
descriptors, deadlines, limits and source lineage remain enforced.

## Persistence and capacity

Execution marks progress in memory. Extend existing run-state/outbox lifecycle
owners to observe dirty state, freeze consistent coalesced snapshots, account
memory and encode/deliver off execution. Do not clone, count or serialize the
whole growing state per mutation, or hold its lock across encoding or IO. Reuse
the existing bounded encoding capacity and loss accounting; no second queue,
WAL, finalizer service or general event bus.

Queued finished state remains available for background persistence retry during
an outage; do not mark it completed merely because a terminal offer was shed.
Reuse the existing ClaimLoop permit, acquired before claiming, as the finite
bound on the whole running/completing claim lifetime. Retain it until settlement
or existing cancellation/lease-loss cleanup; release computation-owned workspace,
Python and model resources when computation ends. Full claim capacity stops new
claims while already-running work can finish. Preserve per-run/outbox byte limits;
no second tracking queue, semaphore or completion manager is needed.
Direct capacity remains isolated. Direct evidence remains bounded/best effort;
a direct response never promises retained details already exist.

The existing runner owns the claim through background persistence and settlement.
Compute permits have the shorter computation lifetime. On success, it retains/
renews the finished claim until canonical terminal evidence is acknowledged, then
settles it using existing queue fields and token-fenced batched completion.
Preserve existing error/cancellation/lease-loss handling and local tracking cleanup;
do not turn permanent rejection into endless transient retry or retain authority
after lease loss. This reuses existing transitions, not a new completion manager
or public failure/durability policy. This is live memory retention, not a durable
SQL result mirror or a guarantee that unpersisted results survive process death.
Completions remains bounded by those held claim permits. Keep that existing
lifetime rather than detaching it or duplicating completion orchestration. The
retained claim permit is lifecycle tracking, not computation capacity held over ACK.

## Lease, cancellation and SQL

Use the existing background LeaseRenewal owner. Remove synchronous confirmations
from engine mutations, publication replay and terminal delivery. Execution checks
local cancellation and the original deadline. Maintain background cancellation
responsiveness by retaining the existing cancellation-notification migration,
trigger and LeaseRenewal listener; do not replace them with polling or another
notification path. Already-issued external calls can finish after cancellation.

Admission, claims, renewal and completion use bounded tenant batches, short
TenantConn transactions and set-based statements. Acquire a connection to process
the ready batch, commit and release it; never hold it across compute, provider,
Bifrost, file or encoding waits. Preserve fairness, RLS and PostgreSQL-owned lease
timestamps. Normal low traffic can produce a one-row batch; do not implement a
connection/transaction per row by construction or degrade batched failure to
unbounded per-row retries.

## Recovery, fencing and parked work

A crash loses live state and unaccepted work. Direct requests do not resume.
Queued redelivery preserves ExecutionId, accepted source/policy/input and original
deadline, reuses the exact accepted compatible checkpoint and can repeat later
work/effects/spend. Missing/unavailable recovery is not authoritative absence.
Persisted terminal decisions are delivered without recomputation. Never reset
accepted budget consumption or promise conservation of lost spend.

Keep existing stable execution/revision identities, accepted-record conflict and
replay behavior, queue token/generation fences and current-source checks.
Background preparation/persistence/completion own SQL authority work. Do not
claim Scribe already validates queue tokens: the inspected ExecutionState write
carries no lease token. Preserve the documented allowed old frozen revision race
and exact accepted recovery without introducing a Bifrost redesign or stronger
instantaneous cross-replica revocation guarantee.

For task 005, validated pinned extraction is usable immediately by dependent
Prompt/workspace consumers from live state; accepted recovery reuses only its
actually persisted exact source/context. Remove the older extraction custody
wording that requires checkpoint/Scribe ACK before consumption.

For future tasks 007–009, a fully blocked execution releases compute resources
while background custody persists the continuation before retiring its current
delivery. Evidence/approval control receipts retain their durable contracts.
GitHub delivery consumes retained reports independently, validates current
authority, and never recomputes verification to retry a provider write.

## Gateway and file publication

Reuse resource-scaling gateway snapshots, in-memory ledger and background
settlement/capture. Remove capture-slot waiting for verification-originated calls
only. Trace their actual attribution path before changing a conditional registry
lookup; its mere existence is not proof that these calls use it. Preserve
standalone gateway behavior and provider/model capacity controls.

For server-produced publication files, keep validated object IO, exact descriptors,
tenant paths and authorization. Remove private external-upload-session SQL and
inventory-ACK dependencies from engine publication by extending the existing
storage owner, with inventory persistence handled in the background. External
upload contracts remain unchanged; no duplicated digest/descriptor logic.

## Schema and proof

Task 010 retains the explicit unreleased schema cleanup and approved GitHub/PR,
Task 004 extraction metadata and resource-scaling migrations. No new generic
execution table or SQL result/checkpoint mirror. Never rewrite deployed migration
history or reset user databases.

Proof exercises SDK → real server/Postgres → SDK direct and queued execution,
holds persistence/SQL replies independently, checks compute permits and memory
bounds, resumes persistence and observes retained results plus batched settlement.
Recovery/cancellation/takeover tests preserve accepted evidence and original
bounds. Compare matched full verification benchmarks against resource-scaling,
including p99 overhead below 500 microseconds at 64 clients and existing L2
limits. Preserve latency/throughput/CPU/memory baseline; no new 5% tolerance.

No production changes or runtime performance results are claimed. Approved spec revision 7 records this design; owning verification authority
still needs reconciliation before affected implementation.
