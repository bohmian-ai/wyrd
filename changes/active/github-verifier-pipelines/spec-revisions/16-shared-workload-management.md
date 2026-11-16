---
id: SPEC-github-verifier-pipelines
revision: 16
status: approved
approved_on: 2026-11-16
amends: ../spec.md
design: ../design/shared-workload-management.md
---

# Shared workload management — revision 16 amendment

The maintainer agreed the [finalized design](../design/shared-workload-management.md)
on 2026-11-16 and requested TASK-012-R1-allocation before TASK-013. This amendment
records that agreement and was explicitly approved by the maintainer on
2026-11-16. It is incorporated into the current specification as revision 16.
All unaffected pipeline behavior, public contracts and historical task verdicts
remain unchanged. Approval does not claim implementation or verification.

## Problem, outcome and scope

Gateway, verification, Bifrost and managed Monty children share a server's memory
and CPU. Existing local ceilings do not coordinate combined demand. Verification
also charges predicted worker/frame/media/checkpoint copies before consumption.
A small verification can therefore exclude unrelated work despite low actual
consumption, and idle workers can retain speculative capacity.

Extend existing process resources into one workload manager. Capacity grows with
demand; unused engine entitlements are not charged. Preserve Bifrost's refined
memory management, including intentional staging, Parquet and compaction
workspace reservations. Coordinate CPU admission without replacing execution
engines or promising preemption. Implement this before TASK-013 measures the
canonical mixed workload. TASK-014 retains throughput optimization.

## Required behavior

**REQ-019 — Shared domain and consumption.** One manager covers the server and
its managed Monty children. Extend `ProcessResources` and existing owners;
there is no second independently debited memory pool, allocator, durable queue,
SQL coordination or public workload API. Internal context carries tenant,
initiating workload and operation identity through nested calls. Verification-
originated gateway work retains verification attribution for resource sharing;
gateway permission, provider and commercial quota checks remain unchanged.
Independent buffers count while retained; a shared physical buffer counts once
until its last reference drops. Ownership transfer is not another root charge.
Account substantial retained buffers at their owners, using allocated capacity
where available. Small object graphs use aggregate pressure/headroom, not
per-field guards or another serialization pass solely for memory sizing.

Remove speculative Monty heap/frame envelopes, media copy multipliers and
checkpoint future-copy estimates. New non-Bifrost buffers are accounted when
materialized/retained; discard a refused new buffer and use existing capacity
errors. Bounded streaming chunks limit allocation-before-observation overlap.
Preserve actual retained-memory custody, local payload limits and per-session
Monty heap ceilings. Do not require every allocation to be individually attributed.

**REQ-020 — Capacity and observation.** Reuse existing discovery and settings.
For ceiling P covering parent and managed children, retain existing headroom H
and managed budget M = P - H; preserve an explicitly smaller Bifrost cap.
Headroom is excluded capacity, not a charge, and is fixed during a deployment;
keep its existing 1 GiB minimum/default and increase-only configuration.
Do not add per-worker/request overhead deductions or automatically shrink H.
Engine shares grow online within these ceilings; configuration changes require
restart. Other replicas, PostgreSQL and provider infrastructure are outside
this manager's domain; existing distributed tenant quotas remain separate.

Observe ledger L and parent-plus-worker allocator commitment A separately. Use
q = max(L/M, A/P); do not add overlapping observations or label commitment exact
RSS. Use allocator-native aggregate statistics at the binary boundary, without
new per-allocation parent hooks or enabling expensive detailed allocator stats.
The installed mimalloc observation API/necessary narrow binary feature bridge
is in scope; do not link its allocator into SDK libraries. Worker telemetry is
bounded/coalescing and independent of interpreter-turn completion, serialized
through the worker protocol writer, with no IPC from allocation hooks.
Sample every 100 ms. At 500 ms staleness, stop new work on that worker and new
worker starts until visibility recovers. Retain its last usage through idle,
stale/cancelled states until a newer sample or confirmed process exit.

**REQ-021 — Borrowing and pressure.** Gateway, verification and Bifrost have equal
default weights, with no charged entitlement or fixed engine partitions. Keep
Bifrost's inner class/tenant policies. Below q=0.95, eligible work can borrow
unused capacity within local ceilings. Under contention, distribute the 0.95*M
admission target among demanding workloads by weight, redistributing portions
limited by local ceilings; include borrowers holding live memory in demand.
Apply equal soft targets among demanding tenants where attribution exists.
Pause additional work above target; below-target work remains subject to root
and local limits. If unattributed allocator pressure triggers first, pause new
memory-growing work globally. These targets grant neither precharges nor
instantaneous ownership of other operations' non-reclaimable memory.

At q>=1 stop new work, allow bounded settlement/cleanup, and use supported
reclamation, including idle-worker eviction. Bifrost retains its own spill/retry
decisions. If critical pressure persists, cancel one cancellable operation from
the workload furthest above target, selecting its largest attributable operation;
if all are at target, select the largest cancellable attributable operation.
Reassess after settlement or the next 100 ms sample before another victim.
Never select acknowledged durable work or mandatory settlement/audit as victims.
Use existing capacity-error semantics, not a failed verification judgment.
Exit contention after q<=0.90 for two samples. These selected defaults need
measurement; the launch margin is not a charged allocation or another hard cap.

**REQ-022 — Non-preemptive CPU admission.** Reuse effective CPU C for C managed
running lanes shared across major CPU work. Idle workloads hold no lanes; one
workload can borrow all available lanes. Select least normalized service among
demanding workloads, then their tenants, using busy-lane duration divided by
weight and including elapsed running leases. Rebase returning workloads/tenants
to the current virtual-service floor; idle time earns no credit. Skip locally
blocked work without blocking eligible peers. A lane is running work, not a
query-lifetime permit. Parallel lanes need separate permits. Release before IO,
memory/downstream admission and provider waits; nested work on the same running
lane must not recursively acquire. Keep existing bounded owner queues/deadlines;
manager entries retain references, not copied payloads or another durable queue.

Integrate Monty runnable turns, substantial encoding/decoding, Scribe CPU jobs
and actual Oracle/Forge execution, including spawned parallel work. Extend the
existing DataFusion tracing wrapper for context and admission, preserving spans;
cover entry execution too. Async polling releases its lane on return/Pending;
blocking CPU jobs run on existing CPU workers. An outer query-stream wrapper
alone does not prove coverage. Preserve owner pools and algorithms. This is
non-preemptive admission: no exact CPU-time percentage, instantaneous lane
reclamation or arbitrary native-code interruption is promised.

**REQ-023 — Existing limits and progress.** Tenant quotas, execution/query/heap
ceilings, payload limits, concurrency and shared pressure all apply independently;
never charge the sum/product of configured maxima. Streams keep their active
call and retained-buffer custody until completion/cancellation. Only queued
verification is resumable. Direct/queued work share this manager. Preserve
deadlines, Busy deferral, host settlement, token fencing, evidence acceptance
and audit non-blocking semantics. Charges survive cancelled waiters until their
real owners finish/dispose; scratch releases only on confirmed deletion.
Publication moves custody to the outbox. Mandatory bounded drain work keeps
progressing under pressure without an unbounded allocation exemption or a CPU
permit held while awaiting persistence.

**REQ-024 — Compatibility and qualification.** Preserve Bifrost pools, memory/
workspace policy, caches, query semantics, catalog and WAL/fence/durability
contracts. Only shared pressure and CPU-dispatch integration is added there.
Keep existing configuration names, wire/SDK contracts and distributed quotas;
no schema/migration, DRF scheduler, new allocator, OS isolation, strict RSS cap
or exact private-heap tenant accounting. Retain REQ-018's no-regression standard:
full clean L1 verification/gateway/Bifrost against compatible existing baselines,
with current correctness/validity and no invented tolerance. TASK-013 owns the
subsequent full L2 comparison and 100-tenant claim fairness regression. Do not
claim measured throughput benefits from deleting estimates alone.

## Acceptance

| ID | Situation | Expected observation | Requirements |
|---|---|---|---|
| AC-015 | Small Monty/media work, idle worker, retained buffers and outbox transfer | Consumption follows real retained/observed owners; no maximum-envelope/multiplier charges, phantom idle entitlement or double root debit | 019,020,023 |
| AC-016 | Competing workloads/tenants, borrowing, pressure escalation/recovery, stale worker telemetry | Online soft sharing and selected thresholds; no bypass of unattributed pressure, fabricated release or victimization of acknowledged work; bounded drain progresses | 020,021,023 |
| AC-017 | CPU-heavy work across engines with nested calls, parallel tasks, IO suspension and cancellation | C managed lanes, work-conserving sharing, normalized-service admission and correct tenant context; real DataFusion/Scribe/Monty paths participate; no recursive-permit deadlock | 019,022,023 |
| AC-018 | Public verification calls with gateway/model exchange and Bifrost work on one server, then existing functional and full L1 qualification | Connected consumption/admission/custody, preserved Bifrost behavior and local limits, no benchmark regression; reviewed output is the TASK-013 input | 019–024 |

## Authority and implementation order

This approved amendment supersedes conflicting before-allocation peak/copy
requirements in TASK-012 review/remediation contracts and narrows REQ-017's
"Bifrost implementation unchanged" to permit the stated shared pressure/CPU
integration. It does not waive preserved memory/workspace/durability behavior or
REQ-018. The 100-tenant TASK-013 regression remains separate from resource-manager
tenant fairness. Historical reviews remain historical, not evidence for this work.

Implement through [TASK-012-R1](../tasks/012-R1-allocation.md), after TASK-012 and
before TASK-013; TASK-014 follows TASK-013. Reconcile owning resource comments/
authority with this selected behavior during implementation, without using old
design documents to reinstate rejected speculative accounting. A missing
observation/execution integration must be reported, not replaced with forbidden
reservations or an unapproved dependency/runtime redesign.
