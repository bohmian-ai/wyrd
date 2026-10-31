---
id: SPEC-benchmark-scaling
revision: 1
status: draft
---

# Benchmark scaling follow-up

**Draft awaiting human approval.** This revision authorizes no implementation,
benchmark execution, threshold changes, or replacement of architecture authority.
The decisions below must be resolved before approval and task planning.

## Problem and proposed change

Two performance gaps remain after the benchmark closeout at `92b62f9c0`.
First, learned hot Variant layouts exist, but real SQL can still reconstruct
whole values that a query never returns. Variant shredding extracts selected
paths into typed columns and keeps other values in a residual column. Its
benefit is lost when a later filter requires the whole value again. TASK-008
attempted expression projection and shared staged-file reading, then rolled
them back after incomplete functional proof and missed performance acceptance.

Second, gateway capacity crosses several fixed-sized stages. The final L1
64-client overhead p99 was 0.491 ms for 1 KiB buffered metadata calls and
0.239 ms for streamed metadata calls, meeting `<0.5 ms`. The 64 KiB metadata
case missed at 1.877 ms; payload capture missed at 156.762 ms while using
11.461 of 32 logical cores. The payload median overhead was 2.542 ms. The long
tail with spare aggregate CPU suggests waiting or serialization, but does not
identify a particular queue, lock, device, or connection pool as its cause.

This change proposes completing hot read behavior and investigating gateway
resource scaling through the existing owners. A gateway re-architecture is
warranted only where measured constraints cannot be removed within those owners.
The outcome is lower verified read/capture cost and usable additional capacity
when operators provide more resources, with bounded overload and unchanged
evidence semantics. It is not a promise of linear scaling or universal latency.

For example, `SELECT COUNT(*) FROM bifrost.traces.spans WHERE attributes ->>
'region' = 'eu-west'` must return the same count on canonical and learned
memtable, staged, and published data. On a learned source, a fully covered
typed predicate must not reconstruct the whole `attributes` value merely for
a leader-side recheck. A query returning all of `attributes` must still receive
the exact logical value, including residual fields.

The approach adds conversion, accounting, and diagnostic costs. Greater
concurrency can increase memory, database connections, WAL contention and
downstream pressure. Bigger queues alone can make the tail worse. Prefer
extending the current workflow over another runtime, scheduler, resource probe,
or capture pipeline. [Research](research.md) records history, source owners,
the constant inventory, alternatives, and limits of the evidence.

## Scope and relationship to existing packets

The hot part carries forward the unfinished behavioral outcomes of TASK-008:
real-SQL physical-leaf evaluation, shared staged/published reading on the data
owner, truthful scan facts, and controlled read/write cost comparisons. It does
not replay its reverted implementation. The current executable baseline is
`92b62f9c0` and later verified source, not a reset to `471f69ff9`.
The historical rollback to that older source remains evidence of what failed.

The gateway part builds on `changes/active/gateway-capacity/`: reuse its real
server, common startup, execution owner, tenant snapshot invalidation, bounded
listener recovery, capture attribution, stream lifecycle, and measurement
facilities. Its overhead/capacity harness, startup work, portable hosted worker,
and Verifier source-order change are excluded from reimplementation here.
Its specification explicitly excluded general performance optimization;
resource scaling is the new scope. This draft does not retire or edit that packet.

There is an authority conflict: gateway-capacity revision 5 requires
unreserved capture shedding without admission delay; current Wyrd design
requires reservation before charge/dispatch, deadline-bounded waiting, and
`WYRD_GATEWAY_429_CAPTURE_STAGING_FULL` on expiry. Operations still groups
captures with non-blocking shed producers. The proposed follow-up preserves the
current gateway-specific reservation contract. Resolving contradictory owner
prose is an approval prerequisite; this drafting request does not authorize
editing architecture. It must never be resolved by silently restoring shedding.

Non-goals are new public APIs, SDK behavior, Card kinds, persistent layouts,
sampling policy, query languages, threshold changes, a replacement benchmark
harness, general verification-runtime work, physical WAL multiplexing, production
autoscaling, dynamic resource resizing, decoded caches, retained staged Arrow,
or a second canonical-plus-shredded resident representation. L2 performance
execution is not required by this draft; the existing capacity harness may
inform later approved validation without copying it. No production code, tests,
architecture, or benchmark declarations change during this drafting work.

## Required behavior

### Hot reads and their write cost

**REQ-001 — Real SQL uses learned leaves.** Scalar projections and filter-only
aggregates use available physical leaves on both memtable and staged sources,
locally and across the existing private peer boundary. The promise includes
filters whose source column is absent from the output; correctness-only scalar
fixture tests cannot close it. Whole-value reconstruction occurs only when
semantics require it, including residual fallback and whole-value output.

**REQ-002 — One file-reading policy.** Staged and published Parquet use the
existing shared per-file planning and cached-reader policy on the node holding
the data. Real file size, rows, time bounds and table time axis drive pruning
and partition sizing. Storage and query-bound lease ownership may differ.
Discovery, listing, routing and publication cuts retain their existing owners.

**REQ-003 — Conversion has a measured total cost.** Compare the same key,
values, selectivity, row population and query shape with and without learned
shredding, separately for memory and staged data. Evaluate ingress conversion,
WAL/ACK latency, staging, CPU and peak charged memory as well as read latency.
A faster read cannot conceal an unbounded copy or an unacceptable ingest loss.

**REQ-004 — Scan facts describe executed work.** Memory and file scan evidence
must distinguish absent instrumentation from zero work, aggregate each source
once, survive remote completion, and not count refused/stale completions.
Memory bytes describe selected Arrow buffers, not disk IO; compare them with
like memory measurements, never equate them with compressed file bytes.
Unknown/inexact timestamp statistics must remain conservative and preserve
usable nanosecond UTC bounds without conversion warnings or false pruning.

### Gateway capacity

**REQ-005 — Resource geometry is coherent.** Resolve gateway execution capacity
from the operator's effective CPU, memory and connection allocation under one
documented policy, respecting other enabled roles and downstream budgets.
Execution slots, capture work, buffering and queues must fit that allocation
together; they must not independently multiply into an unsafe resident or
connection demand. Existing explicit limits remain hard upper bounds.

**REQ-006 — Added resources can become useful capacity.** Under the approved
resource/workload matrix, increasing an identified limiting allocation must
increase sustainable work or improve its waiting tail until another measured
resource or dependency becomes limiting. Report plateaus and their attribution.
Changing a constant without demonstrating its effect does not satisfy this
requirement. CPU use alone is neither success nor bottleneck proof.

**REQ-007 — Waiting is attributable and bounded.** Distinguish capture reservation,
outbox scheduling/batching, projection/encoding, Scribe submission/ACK, database
acquisition, upstream connection/response and downstream stream backpressure.
Explain the 64 KiB payload tail with reconciled elapsed/CPU and queue evidence.
Every queued, retrying and in-flight item stays charged until released; stalled
dependencies and slow readers cannot cause unbounded memory or connections.

**REQ-008 — Overload retains the gateway contract.** A capturing call reserves
capacity before charging or dispatching, waits only within its deadline and
uses the existing retryable capture-staging refusal on expiry. A reserved
capture is never shed. Projection/storage/deadline failure after admission is
reason-counted without changing an answered call. Audit remains non-blocking.
Increasing throughput must not starve other tenants or the process's audit,
verification and Bifrost obligations.

**REQ-009 — Defaults are classified, not indiscriminately enlarged.** For each
inventoried value, distinguish resource geometry, operator override, protocol
or security limit, retry/freshness policy and library default. Remove demonstrated
fixed bottlenecks while retaining justified bounds. Request sizes, token reuse,
retry semantics, listener liveness budgets and provider timeouts do not grow
automatically with core count. Effective allocation and limiting reasons must
be diagnosable without secrets or unbounded metric labels.

## Invariants and ownership

| ID | Promise and failure prevented |
|---|---|
| INV-001 | Exact Variant types, values, missing versus null, mixed/nested values and residuals survive all tiers. Layout selection changes performance only. Canonical Arrow wire and public schemas stay unchanged. |
| INV-002 | Learned layouts stay bounded, process-local and keyed by exact tenant/table/version; each generation pins one layout, cold generations remain canonical, and restart/eviction changes no result. WAL stays canonical with its logical fingerprint/digest. No new sampling policy is authorized. |
| INV-003 | ACK still requires durable WAL, tenant fence and authoritative insertion. Staged authority remains durable before Arrow retirement; publication, recovery and generation selection never duplicate or omit rows. |
| INV-004 | Full schema and signed assignment authority are validated before source IO; tenant proof, row tripwires, peer identity and fail-closed decode stay intact. Narrow output cannot weaken full-schema validation. |
| INV-005 | The leader never plans peer files; no footer crosses the network. Captures, staged leases and admission survive exactly as long as their query stream; cancellation, deadline, drop and shutdown release their resources. |
| INV-006 | Gateway authorization, SSRF/DNS screening, TLS, credential isolation, redaction, limits/budgets, pricing, retry/fallback and accounting settlement keep their existing semantics. Capacity is not a permission or rate-limit bypass. |
| INV-007 | One process Scribe outbox remains the owner for audit, captures, spans and verification evidence. Tenant-scoped batches, verified SUB/ACT attribution, identical-slice retries, idempotency and required detail-before-summary ordering remain valid across concurrency changes. |
| INV-008 | Resource discovery and enforcement work identically on macOS arm64, Linux x86_64 and Linux arm64. No cgroups, namespaces, `/proc`, platform gating or synthetic host-load calibration. Resource geometry is fixed at controlled startup for this proposal. |
| INV-009 | Benchmark workloads, sample floors, timing boundaries, default coverage, validity rules and thresholds remain intact. VALID is measurement integrity, not target attainment or approval to keep a regression. |

Scribe owns layout lifetime, conversion, staging and append durability; Oracle
owns SQL planning, source projection and query completion; Forge keeps its
existing promotion/compaction contracts. Gateway owns governed invocation and
its lifecycle; Skald's provider transport remains the egress owner; SQL owns
tenant-bound database plumbing; Bifrost/runtime retain outbox and resource
ownership. Compare these complete workflows before proposing a boundary move.
No new public interface, persisted contract, service, or durable publication
ledger is proposed. A required change to one returns this draft for revision.

## Acceptance and evidence

Future validation uses existing repository lanes and real SDK → WyrdTestServer
with repository Postgres → SDK journeys on affected surfaces, plus owning
unit/integration proof. Task planning supplies exact selectors after approval.
Nothing in this section requests executing load during this drafting session.

| ID / obligations | Situation and expected observation |
|---|---|
| AC-001 / REQ-001, INV-001–002 | Run the actual attribute-filter COUNT and scalar/whole-value queries over canonical, learned, mixed-layout, staged and published sources, locally/remotely. Results match; physical-work evidence shows the covered filter-only case does not rebuild the unused root. Missing/null, type conflict, residual-only keys and cold/restarted layouts remain correct. Establish a failing real-SQL case before changing its owner. |
| AC-002 / REQ-002, INV-003–005 | Read repeated staged and published sources with known file facts, including timestamp bounds, publication races and cancellation. Shared pruning/cache policy is observed, unknown bounds keep candidate data, invalid assignments cause no source IO, and leases/charges return after termination. Concurrent retirement never deletes an in-use run. |
| AC-003 / REQ-003–004 | Controlled same-key comparisons isolate shredding from reader changes. Record read latency/CPU, reconstructed leaves, scan work, ingest ACK/CPU and peak memory including conversion scratch/captures. Native/remote facts reconcile, empty and unavailable remain distinct. Gains have an explicit cost and keep/revert recommendation. |
| AC-004 / REQ-005–007, REQ-009, INV-008 | On the human-approved small/reference/larger allocation matrix, keep workload and dependencies comparable and record effective CPU/memory/connection budgets. Queue/CPU/IO evidence identifies each limiting stage; greater limiting resources produce the agreed improvement or an explained dependency plateau. Test low allocations and invalid/overflowing budgets without OS controls. |
| AC-005 / REQ-008, INV-006–007 | Exercise concurrent tenants, paused Scribe, full reservation budget, slow reader, cancellation, deadline and shutdown through real gateway paths. Capacity is bounded, refusal precedes charge/dispatch, reserved captures are retained, audit does not block, recovery drains, identities/accounting stay exact and no tenant monopolizes progress. |
| AC-006 / INV-009 | Produce clean, full, unprofiled L1 records with all repetitions, integrity checks and source attribution for hot, parquet, writes-parquet and gateway, plus affected verification regression evidence. Profiles and allocation diagnostics are separate evidence. Report every existing target as met, missed or unsettled; disclose incomparable historical controls. A remaining miss requires explicit maintainer disposition after causal evidence, never threshold retuning. |

The unchanged recovery objectives include parquet point c64 p99 `<5 ms`, span
peak efficiency `≥30 wire MiB/CPU-s`, and gateway c64 per-request overhead p99
`<0.5 ms` for each declared case/handoff. At `92b62f9c0`, the first two missed
at 11.698 ms and 15.852 wire MiB/CPU-s. Hot shredding principally targets
attribute read cost; it is not asserted to fix unrelated point lookups or the
WAL-limited write path. These are protected end-to-end outcomes and explicit
remaining misses, not an invented causal link. Hot historical recovery figures
(861 queries/CPU-s at point c1 and ≤4.31 ms at c64) remain comparison context
until a human decides whether to carry them as additional acceptance targets.

## Material decisions awaiting human approval

1. **Capture authority reconciliation.** Confirm preserving reserved capture
   admission and arrange correction of conflicting operations/old-packet prose
   in its owning workflow. Restoring shedding would change the product contract
   and is outside this proposal. No architecture edits are made here.
   Also reconcile gateway-capacity revision 7's reference to an absent revision-6
   amendment before relying on any obligations attributed to that amendment.
2. **Resource policy and allocation ownership.** Proposed: startup resolution
   from explicit operator allocations with portable defaults, coherent per-role
   shares and hard deployment connection ceilings. Decide whether to extend the
   existing resource model or require a distinct gateway allocation boundary;
   specify configuration precedence, public configuration changes, reservation
   shares and tenant fairness guarantees before approval. Automatic live tuning
   is excluded. Existing Bifrost ownership alone does not authorize taking its
   protected resources for gateway work.
3. **Quantitative scaling and regression acceptance.** Choose allocation sizes,
   controlled dependency budgets, improvement/regression tolerances and evidence
   confidence for AC-003/004. Decide whether the historical hot recovery targets
   become obligations. Existing L1 thresholds stay unchanged; a target waiver
   cannot waive functional hot-leaf or shared-reader behavior. A strict zero
   regression rule would need explicit agreement given measurement variation.
4. **Point/write remediation boundary.** Proposed: measure and protect point
   latency and span write efficiency here; allow only corrections within the
   existing hot-read/gateway/outbox boundaries. Decide whether closing those
   independent misses requires a separate durability/write-scaling change.
   Cross-shard WAL redesign, persistent-format changes or new service ownership
   need their own authority rather than being inferred from this draft.

Approval must resolve these material decisions in a revised draft. This is a
behavioral proposal, not an implementation-ready packet or permission to run
benchmarks. Ordinary private algorithms, helpers and module choices remain
implementation-owned.

## Authority and revision history

- [Wyrd doctrine](../../../architecture/wyrd-design.md#doctrine),
  [gateway admission and identity](../../../architecture/wyrd-design.md#principal-model),
  [runtime context](../../../architecture/wyrd-design.md#stock-provider-sdk-context)
  and [delegation/fallback](../../../architecture/wyrd-design.md#cross-service-delegation).
- [Bifrost Variant shredding](../../../architecture/bifrost-design.md#variant-shredding),
  [durability](../../../architecture/bifrost-design.md#durability-and-visibility),
  [query authority](../../../architecture/bifrost-design.md#immutable-planning-and-source-authority),
  [nested reads](../../../architecture/bifrost-design.md#nested-field-reads)
  and [resource invariants](../../../architecture/bifrost-design.md#resource-and-failure-invariants).
- [Operations](../../../architecture/operations/README.md),
  [deployment budgets](../../../architecture/operations/deployment-and-release.md#database-logins-and-connection-budget),
  [capacity and overload](../../../architecture/operations/reliability-and-recovery.md#capacity-and-overload),
  [security posture](../../../architecture/wyrd-security-posture.md#tenant-and-data-isolation).
- [Specification workflow](../../spec-driven-development.md),
  [benchmark methodology](../../../benchmarks/README.md), and
  [source/history research](research.md). The latter preserves immutable source
  references because the benchmark-experiments packet is being retired elsewhere.
- 2026-10-31 — Revision 1 drafted from repository authority, source, historical
  diffs and retained records. Awaiting human approval; no implementation tasks.
