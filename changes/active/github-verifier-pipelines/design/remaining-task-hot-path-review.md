# Remaining-task hot-path review

Scope: tasks 005, 007, 008 and 009, their approved runtime amendment and existing
owners. This is a static design/source review, not a benchmark or implementation
verdict. Concurrent remediation implementation is outside this review's write
scope. No production source, task requirements or architecture were changed.

## Result

The remaining tasks do not require new PostgreSQL waits on ordinary direct
verification, gateway or Bifrost execution. They also preserve memory-only engine
transitions and background checkpoint/park/completion work. However, a blanket
claim that queued verification adds no database work would be incorrect:

1. Task 005 connects an existing SQL-backed library cache to the new extraction
   action. Its metadata operations explicitly acquire a tenant transaction and
   commit per protocol operation. This is existing code becoming reachable from
   queued extraction, not proof that the execution path already paid its cost.
2. Task 008 originally left approval-expiry time acquisition ambiguous. The
   maintainer resolved this by removing separate approval expiration: all pending
   approvals use the execution's one existing expiration timestamp. No new clock
   anchor, timer or per-step timestamp query is needed.

## Wait and lock inventory

| Task | Work and placement | Assessment |
|---|---|---|
| 005 — extraction | Resolve frozen source/baseline; run the independent ReviewPipeline; GitHub/object-store IO and snapshot/context metadata calls; then put validated output into live state | Real new reachability of database IO during the queued extraction workflow. Consumers must not additionally wait for Wyrd checkpoint/Scribe acceptance. Does not belong on ordinary direct execution. |
| 007 — deferred evidence | Publication endpoint validates and waits for durable evidence acceptance; running dependencies consume accepted facts in memory; background custody persists parked state and retires the queue delivery; preparation/recovery reads accepted evidence | These are separate accepted control operations, preparation or background lifecycle work. No per-step SQL polling, lease confirmation or checkpoint ACK is required for live independent branches. |
| 008 — approvals | Live action creates pending occurrence under the existing execution expiration; separate decision endpoint performs authorization and waits for durable acceptance; background park/resume uses 007 | No separate approval deadline, clock anchor or per-step timestamp query. Checkpoint/recovery preserves the original execution expiration. |
| 009 — report/delivery | Pure schema/report helpers and normal step outputs; provider worker reads accepted reports, checks current access/generation and records reconciled delivery progress | Provider IO, authority checks and progress persistence belong to independent delivery after/outside computation. No GitHub receipt or provider-progress ACK should become a verification-completion wait. |

Durable publication/decision receipts deliberately wait for canonical acceptance;
they are not permission to add the same wait to an executing graph. Permission
checks remain sanctioned. PostgreSQL remains coordination/metadata, not payload
or evidence custody. Bifrost append acknowledgement remains its existing durable
WAL/fence contract. Existing batching/group commit must be preserved.

## Confirmed Task 005 metadata behavior

[Task 005](../tasks/005-pull-request-extraction.md) calls Task 004's pipeline
using pinned source and baseline, then exposes validated extraction to dependent
steps without checkpoint acceptance. Task 004 already implemented the metadata
traits, but production `ReviewPipeline` wiring for the new extraction action is
still future work: inspected production references are the adapter definitions;
the current pipeline constructions are in integration tests.

[`CodeReviewMetadata::tenant`](../../../../crates/wyrd/wyrd-server/src/components/github/review_cache.rs)
opens a tenant connection, awaits one operation, and commits it (228–240).
Snapshot pointer lookup, pointer publication and record touches use this owner
(271–342); `upsert_context` does the same (410–428). SQL includes pointer
insert/update, record upsert and context upsert. Writes have normal row/index
contention even though the cache query file adds no explicit advisory lock or
`FOR UPDATE` clause. This path is transaction-per-protocol-call, not a demonstrated
group-commit implementation.

The transaction encloses metadata SQL only and is released before returning to
the library's GitHub/object-store work. No transaction across network/object IO
is required. Object bytes remain outside PostgreSQL. These cache protocols are
explicitly owned by Task 004's storage amendment and
`architecture/wyrd-design.md` → Code-review source cache; that approved custody
does not establish zero latency or batching on the eventual extraction path.

Task 005 should not be qualified as adding no hot-path SQL merely because its
adapters were completed earlier. The maintainer subsequently directed extending
`CodeReviewMetadata` with immediate-drain batching/group commit, modeled on
verification completions; Task 005 now records that work. The existing adapter
has not yet been replaced or measured. No replacement database or generic batch
service is selected.

## Locks and Task 008 timing

The existing queued `RunState` uses a per-execution memory mutex. Mutation work
and background snapshot freezing share it; persistence IO happens outside it.
Tasks 007/008 extend that live state, so existing locks remain and their critical
sections may grow. The task requirements do not mandate a new global mutex,
advisory lock or a lock held across IO. Their performance is not proven by this
static review.

[Task 008](../tasks/008-approval-steps.md) now uses only the execution's original
expiration timestamp. This resolves the earlier timing ambiguity without adding
a query or clock mechanism. Pending approvals expire with the execution;
checkpoint/recovery and continuation never reset that timestamp. The existing
execution deadline's PostgreSQL coordination semantics remain unchanged.

## Required proof and limits

Use the already required held-persistence and independent-branch journeys to
show live progress and immediate compute release at park. Trace the eventual
extraction metadata calls and transaction lifetimes; document their actual
count/placement rather than predict a fixed warm/cold query count from adapter
interfaces alone. Check that neither a state mutex nor a tenant transaction is
held over GitHub, object-store, model or Scribe IO.

Keep provider delivery retry independent of computation and preserve its existing
authorization fences. Continue the agreed L1-only, no-regression qualification
for verification, gateway and all three Bifrost experiments. Those L1 workloads
do not themselves execute a PR extraction or human-approval journey, so they do
not replace the feature's SQL/lock trace and correctness tests.

No tests, benchmarks or consequential experiments ran. Actual contention and
future metadata call counts remain unmeasured.
