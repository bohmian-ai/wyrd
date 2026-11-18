# Shared workload management

Status: finalized

The maintainer agreed this proposal and requested TASK-012-R1-allocation before
TASK-013 on 2026-11-16. This record is finalized design input, not implementation
approval. Numerical controller defaults below are selected starting values,
not measured results. The maintainer subsequently explicitly approved the
revision-16 specification amendment on 2026-11-16; implementation and evidence
remain outstanding.

## Problem

Gateway, verification, Bifrost and Monty child processes compete for a server's
resources. Existing local limits do not express their combined demand. Verification
also promises capacity for predicted worker/frame/media/checkpoint representations,
withholds capacity while it is unused, and adds sizing traversals.

## Desired outcome

One server and its managed children share capacity according to current demand.
Major retained owners remain accountable, idle subsystems do not hold charged
entitlements, tenant admission is fair, existing local ceilings remain effective,
and contention slows or refuses new work before it disrupts unrelated work.

## Constraints and non-goals

- User decisions: no new speculative peak precharging; consumption-based
  accounting; no allocation-by-allocation application bookkeeping.
- Preserve Bifrost's intentional staging, Parquet and compaction workspace
  reservations, governed pools, spill behavior, query policies and caches.
- Use conventional workload-management practice, not existing design documents
  as justification. Do not edit those documents during exploration.
- No production edits, benchmarks, database changes, branch changes or commits
  are authorized by this design discussion.
- One server plus its own Monty children is the control domain. PostgreSQL,
  provider infrastructure, other replicas and unrelated host processes are not
  controlled by this manager. Existing distributed tenant quotas remain separate.
- No replacement allocator, per-object tenant tagging, DRF implementation,
  operating-system resource isolation, strict CPU-time scheduler, or global
  allocator/RSS hard-cap guarantee.

## Proposed implementation

### One shared owner and existing execution owners

Extend the responsibility represented by `ProcessResources` with one workload
manager in `wyrd-runtime`. The server boot owner constructs and passes it to
gateway, verification and Bifrost. Keep byte charges and scratch custody in their
existing owners; do not add a second independently debited memory pool.

The manager owns a current pressure snapshot, per-engine and per-tenant coarse
usage attribution, fair admission, and CPU execution permits. It does not own
provider calls, query plans, subprocess protocol execution or durable queues.

Owners pass an internal context: tenant, initiating workload, and operation id.
Nested gateway calls inherit verification attribution for resource fairness;
gateway's security, model-call and commercial quota checks still run. Physical
allocations have one current owner; attribution changes do not add root usage.

### Memory budget, overhead and growth

Reuse existing discovery and explicit deployment caps. Let P be the configured
server-plus-managed-children memory ceiling, H the existing non-managed headroom,
and M = P - H the shared managed budget. An explicitly smaller Bifrost cap stays
as a local ceiling. Do not introduce another fixed overhead deduction or multiply
headroom by workers, tenants, executions or media occurrences.

H is capacity deliberately excluded from M, not bytes added to a charge. It is
not free: it reduces the managed working set that fits. The existing default is
1 GiB. Keep it fixed for a running deployment and configurable through the
existing setting; do not automatically shrink it to chase throughput. It covers
ordinary objects, native/allocator overhead and bounded measurement/allocation
lag, without promising those uses will always fit it. If measurement shows it
insufficient, increase headroom or reduce concurrency/maximum payload sizes.

There are no fixed engine partitions. All engines can grow toward M subject
to their existing local limits, actual ownership, and contention policy.

Retain existing Bifrost charging as its declared consumption contract. Elsewhere,
attach/resize one charge when a substantial buffer is materialized or retained,
using its allocated capacity where available. A buffer already materialized but
refused by admission is dropped and the operation returns its existing capacity
error. Incremental streaming uses bounded chunks. Do not predict maximum future
worker heap, frame capacity, repeated media encodings, or checkpoint variants.

Transfer a charge with the physical buffer; shared immutable buffers retain one
charge until their final reference drops. Actual simultaneous independent copies
are separate retained owners. No per-string or per-field guard is required.
Small internal object graphs remain outside fine-grained attribution and are
covered by aggregate pressure and headroom. JSON serialized length is not a
claim about their exact heap footprint.

### Aggregate pressure observation

Maintain separate observations, never add them as if they were disjoint:

- L: the existing root ledger, including intentional Bifrost reservations.
- A: sampled allocator-committed bytes for the parent plus managed workers.

Use q = max(L / M, A / P). Allocator commitment is not exact RSS and can stay
high after logical objects are freed. Keep observed commitment, ledger usage,
and attributable owner bytes separate in metrics. Do not subtract L from A and
call the result exact untracked memory.

The server already uses mimalloc; its locked dependency provides aggregate
statistics. Use allocator-native sampling at the binary/worker boundary, not
new per-allocation parent hooks or request-side heap walks. The installed
release allocator retains essential committed-memory statistics; do not enable
expensive detailed allocation statistics for this controller. Verify this
sampling path and reclamation behavior on every supported platform before
shipping. Use a narrow dependency/API bridge only in the server binary if needed;
SDK embedders retain their own allocators and supply the existing abstract
resource observation contract rather than linking mimalloc through a library.

Recommend 100 ms background sampling. Monty reports current live bytes and
allocator commitment on a bounded, coalescing telemetry extension to its framed
worker protocol, independent of completing an interpreter turn. Serialize frame
writes through one writer. No IPC in allocation/refund hooks. Parent retains the
last sample until a new sample or confirmed worker exit, including idle workers.
Samples older than 500 ms prevent new work for that worker and new worker starts
until visibility recovers; sample loss does not free its remembered usage.

This requires a real worker-protocol integration, not merely reading the existing
child counter from the parent. Parent/worker ship as the same executable. Lost
telemetry must not corrupt protocol frames or block interpreter allocation.

### Fair shares and contention

Use three equal default workload weights: gateway, verification and Bifrost.
An absent workload receives no charged allocation. Bifrost preserves its own
interactive/analytical/maintenance policies inside that outer workload. Tenant
admission within each workload is round-robin, using existing tenant identity.
Do not introduce configurable weight matrices in the first implementation.

Memory shares are soft targets, not reservations or permission to allocate bytes
that do not exist. At contention, divide the 0.95 * M admission operating target
among demanding workloads by weight,
redistributing portions limited by an existing local cap. Include a holder of
non-reclaimable borrowed memory in demand until it releases that memory.
Within a workload, apply the same equal-share principle to its demanding tenants
where owner attribution exists. Unattributed private/native memory is protected
by node pressure and per-operation limits, not invented tenant byte estimates.

Below q = 0.95, unused capacity is borrowable by all eligible work. At q >= 0.95,
pause new operations belonging to workloads/tenants above their soft target;
eligible below-target work may start only while the existing root budget and
local limits permit. A share grants neither a byte precharge nor an exemption
from process pressure. Reconsider admission on release, completion or sampling.
If allocator pressure triggers before ledger pressure and cannot be attributed
to tracked owners, pause new memory-growing work globally; a nominally
below-target owner cannot bypass unexplained process pressure. Existing work can
still finish and release memory. The 5% gap is a launch-control margin, not a
charged allocation or extra hard cap; existing bounded growth can consume it.

At q >= 1, stop new work regardless of share, keep cleanup/persistence flowing,
and invoke available existing reclamation: idle-worker eviction and supported
cache reclamation, with Bifrost still deciding its own spill/retry behavior.
If critical pressure persists after reclamation, cancel one cancellable operation
from the workload most above its target, choosing its largest attributable
operation; if all are at target, choose the largest cancellable attributable
operation. Reassess after settlement or the next 100 ms sample before choosing
another victim. Never select already-acknowledged durable work or mandatory
audit/result settlement as a victim. Report resource exhaustion through the
owner's existing capacity/error classification, not a failed verification claim.

Leave contention after q <= 0.90 for two consecutive samples. Initial values
are explicit proposed defaults and must be validated under real mixed workloads.
Do not copy these numbers into every owner or implement separate controllers.

Live non-reclaimable memory is not forcibly transferred. Borrowers lose the
opportunity to start more work; capacity returns when owners release it, or
through supported cancellation/reclamation. Therefore a new workload's target
is not an instantaneous guaranteed memory allocation.

### CPU and execution admission

Reuse effective CPU C. Add one fair, non-preemptive CPU admission gate for major
CPU execution phases. There are C concurrent managed CPU lanes; idle workloads
hold no lanes. All lanes can be borrowed by one demanding workload.

Select eligible queued work by least normalized service among demanding
workloads, then use the same service accounting among that workload's tenants. Accumulate
completed busy lane time divided by workload weight; include elapsed time of
currently running leases when comparing service. Rebase an idle/new workload
to the current virtual service floor so idle time does not accumulate credit.
Blocked local owners are skipped; do not let their queue heads block others.

One permit represents one managed running lane, not one request or query.
Parallel work needs a permit per running lane. Permit handoff happens at existing
CPU job/turn boundaries; release before asynchronous IO, downstream admission,
or waiting for memory. Never reacquire a second permit recursively while holding
the first on a single running lane. CPU work may retain its actual buffers while
waiting for a permit; that memory remains charged/observed.

Integration must cover actual CPU dispatch, not only public query stream polling:
Monty runnable turns; substantial gateway/verification encoding or decoding;
Scribe's existing CPU job lanes; Oracle/Forge execution tasks, including their
spawned parallel work. Preserve each owner's worker pools and algorithms.
An outer query-lifetime semaphore is insufficient and must not be represented
as CPU accounting. Extend the existing `QuerySpanJoinSetTracer`, rather than
installing a competing global tracer, to propagate resource context and wrap
spawned work. The pinned DataFusion fork routes its `JoinSet` and `SpawnedTask`
futures and blocking closures through this hook. Wrap the entry execution too.
Async admission applies to each actual future poll and releases the lane on
return, including Pending. Nested polls on one thread inherit the running lane;
they must not recursively acquire. Blocking jobs acquire on their CPU worker,
never by blocking a Tokio async worker; mixed blocking IO/CPU jobs require their
owner to expose the CPU boundary. Preserve span instrumentation. Verify parallel
repartition/spill/sort execution, not just serial scans, to catch a missed hook.

This is a necessary Bifrost integration surface in the recommended proposal.
It changes CPU dispatch but does not redesign Bifrost memory,
workspace reservations, compaction sizes, query plans or local class policy.
Strict proportional CPU time and preemption of arbitrary long phases are excluded.
A long Monty turn or blocking native phase can delay another workload until its
current turn/job ends; existing deadlines still apply. Measure longest phases
and gateway latency before claiming that the resulting isolation is sufficient.

Waiting queues retain existing owner bounds and deadlines. The manager holds
small queue entries and references to admitted owner jobs, not copied payloads
or a second durable queue. Every CPU submission must be covered by an existing
bounded execution/lane queue; missing bounds must be made explicit before spec.
Cancellation removes a waiting entry and returns its permit exactly once.

### Per-area allocation and local limits

| Owner | Shared memory treatment | Execution and local limits |
|---|---|---|
| Gateway | Materialized request/response, retained stream/capture buffers; aggregate allocator pressure covers small protocol objects. Byte bounds complement frame counts. | Existing request/token/money/concurrent-call quotas remain; active streams count through completion. Network wait holds a call permit, no CPU lane. |
| Verification engine | Retained dominant inputs/results have aggregate owners; private state graphs use aggregate pressure rather than JSON-times-copy-count. Materialized checkpoint/result buffers transfer to outbox custody. | Existing direct/durable execution slots, deadlines, fanout and payload limits stay. Direct and queued work use the same resource manager. |
| Agent media | Raw/encoded buffers counted while retained, immutable sharing where supported. Remove six-copy multiplier and template traversal solely for sizing. | Existing evidence/media bounds and shared model-call permits. Provider wait holds retained buffers, no CPU lane. |
| Monty | Current child consumption/commitment plus actual parent frames, not twice the configured heap ceiling. Idle workers retain only observed usage. | Existing per-session heap ceiling, operation timeout and agent budget remain. Actual running turns use CPU lanes; suspended workers release their lane. Bound sessions by existing admitted executions/fanout, not a new peak envelope. |
| Bifrost | Keep current root, workspace reservations, pools, Arrow/cache accounting and spill. Root usage appears once in shared ledger. | Keep existing query/tenant/class admission, per-query ceilings and Scribe/Forge limits. Add only shared pressure/CPU-dispatch integration required for cross-engine coordination. |
| Outbox and settlement | Retained batches/buffers remain counted until consumed or discarded; ownership transfer is not another charge. | Drain under pressure; do not hold CPU while waiting for persistence. Preserve durable custody and audit non-blocking semantics. |
| Scratch/workspaces | Retain existing root disk accounting and per-execution limits; release only after confirmed deletion. | Existing spill/extraction/file limits remain. Disk occupancy is separate from memory and does not reserve RAM. |

Enforce each applicable ceiling independently. Never sum configured tenant,
execution, worker or query maxima into root usage. Do not create per-tenant byte
limits for unmeasured private heaps. Existing business quotas may be distributed;
this manager's fairness is per replica, not a new cluster-wide tenant guarantee.

### Progress, failure and transitions

Work carries one resource context through direct/queued verification, nested
gateway calls and staging. On publication, physical custody moves to the outbox;
the original operation no longer owns that buffer. A materialized replacement
temporarily overlaps its original only while both physical owners exist.

Stopping or cancelling admission does not drop host settlement. Charges stay
with the surviving owner until actual completion, disposal, confirmed deletion,
or worker exit. Deadline expiry removes a waiter but never fabricates freed
capacity. Session reset reports the child's new consumption; it does not assume
that configured session capacity was physically released.

The controller must not gate accepted work's mandatory persistence/cleanup as
new external work, or make a CPU holder wait for another permit needed by its
own settlement. Existing bounded drain lanes keep progressing under pressure.
This does not grant those lanes an unbounded memory exemption.

Keep existing configuration names for this change; resolve one internal process
plan from them. Do not introduce aliases, parallel resource settings, new SQL
coordination, public workload APIs or stored data. Process-budget settings change
on restart; owner consumption and shares change online without config changes.

## Evidence and rationale

Inspected at HEAD `0ae00f429`, plus current working-tree status. Existing dirty
packet files are unrelated and were not edited. Relevant current owners:

- `crates/shared/wyrd-runtime/src/resources.rs`: shared ledgers and custody.
- `crates/vala/vala-bifrost-redux/src/resources.rs`: discovery, headroom, root,
  DataFusion growth and CPU-derived local capacities.
- `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs`: existing bounded
  CPU jobs; `oracle/query_stream.rs`: lifecycle/streaming, not proof of CPU control.
- `crates/wyrd/wyrd-server/src/main.rs`: binary-only mimalloc integration.
- `verification/hosted/{sandbox,worker,admission,blocks,evidence}.rs` under server:
  speculative envelope, existing child counter, local limits and media owners.
- `crates/vala/vala-verify/src/blocks/agent.rs`: media multiplier/template count.
- Server `verification/state.rs`: retained JSON proxy and checkpoint multiplier.
- Server `components/gateway/owner.rs` and gateway adapter streaming: commercial
  admission and bounded transport do not imply aggregate byte/CPU fairness.
- Server `oracle/telemetry.rs` under Bifrost already installs
  `QuerySpanJoinSetTracer`; the actual locked DataFusion fork at `5f4b171` routes
  `JoinSet` and `SpawnedTask` futures/closures through its tracing wrapper.
- Locked `mimalloc 0.1.52` / `libmimalloc-sys 0.1.49`: essential committed-memory
  stats and extended observation APIs exist; runtime overhead is not measured.

Primary references:

- [DataFusion memory pools](https://docs.rs/datafusion/latest/datafusion/execution/memory_pool/trait.MemoryPool.html): large-owner accounting and explicit untracked overhead; not every allocation.
- [Velox memory management](https://facebookincubator.github.io/velox/develop/memory.html): capacity arbitration depends on actual reclamation/spill; not a drop-in manager for live prompts.
- [Seastar scheduling groups](https://github.com/scylladb/seastar/blob/master/doc/tutorial.md): CPU isolation needs execution scheduling, not byte counters.
- [Netflix concurrency limits](https://github.com/Netflix/concurrency-limits): delay-driven admission is useful but is not a memory hard cap or CPU scheduler.

No latency, throughput, allocation-cost or mixed-workload measurements have been
performed for this proposal. Performance benefits and recommended thresholds
remain hypotheses until measured.

## Decisions and alternatives

The maintainer accepted the described mechanics, scope and numerical starting
values on 2026-11-16. They are carried into the revision-16 amendment without
claiming measurements or stricter CPU/RSS guarantees.

Recommend a small shared controller over existing owners, rather than importing
Velox, constructing a custom allocator, or implementing DRF. A common CPU gate
is justified only by cross-engine runnable-work coordination. New telemetry is
justified by child consumption being invisible to the parent today. Headroom and
sampling accept bounded observation lag instead of pretending all allocations
are exactly tracked and enforceable.

Reject fixed per-engine memory partitions: they strand capacity. Reject charged
minimum entitlements: they reintroduce speculative precharging. Reject six-copy
or whole-session envelope models outside the preserved Bifrost mechanisms.
Reject a second serialization pass purely for size estimation.

If strict CPU time shares or instantaneous memory guarantees are required, this
proposal is insufficient. Those are different guarantees requiring stronger
execution/allocator control; do not silently add them during implementation.

## Standards conflicts

Existing resource module comments describe before-allocation promises, and
verification contracts/review recommendations prescribed precharging. The user
has explicitly rejected extending that approach for verification and asked not
to use design documents as the basis for this proposal. The later specification
must reconcile affected contracts without altering Bifrost's intentional
workspace policies. No standards or approved task contracts were edited here.

## Verification uncertainties

- Platform and mixed-workload evidence for sampling, commitment reclamation,
  transient overshoot, CPU phase length and headroom sufficiency.

## Handoff

Finalized design. [Approved revision 16](../spec-revisions/16-shared-workload-management.md)
records the agreement; [TASK-012-R1](../tasks/012-R1-allocation.md) carries the
implementation and proof before TASK-013. Specification approval was recorded
on 2026-11-16; no implementation completion is claimed.
Implementation must prove that production execution paths participate. Ordinary
private naming/module choices belong to implementation; strict CPU/RSS
guarantees do not.

Proposed proof: small Monty/media operations consume their measured/retained
bytes rather than configured maxima; idle engines hold zero charged entitlement;
borrowing works until contention; tenant and workload queues progress; nested
calls are not double-counted; root handoffs and cancellation preserve custody;
slow streams are byte-bounded; Bifrost spill/workspace behavior is preserved;
drain work progresses at critical pressure; stale worker telemetry never frees
usage. Real mixed workloads compare gateway tail latency, engine throughput,
worker churn, queue delay, CPU service, ledger/commitment, spills and refusals
against the unchanged baseline, using the repository's full appropriate benchmark
lanes rather than synthetic host load or smoke runs.
