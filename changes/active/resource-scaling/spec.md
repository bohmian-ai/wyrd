---
id: SPEC-resource-scaling
revision: 5
status: approved
---

# Shared resources and lower allocation through Gateway, verification and Scribe

Every Bifrost consumer in a process should size itself from one resource
configuration resolved at startup, so an operator's CPU cap or explicit worker
setting means the same thing everywhere. The process Scribe outbox keeps main's
fixed concurrency, and its delivery limit holds even when a waiter is cancelled.

Revision 2 also scaled outbox writers and ordered lanes with CPU. Measured on
the Gateway L1 benchmark, that change reduced throughput while raising CPU and
memory. This revision withdraws it: the outbox keeps main's geometry, and any
future outbox concurrency change must first show the outbox is the bottleneck.

Revision 4 retains that resource behavior and adds integration of the learned-layout
read work and removal of avoidable payload rebuilding from Gateway through Scribe
publication. The goal is substantially lower allocation and memory growth under
sustained payload capture, without weakening evidence durability or learning.
Revision 4 was approved by the maintainer on 2026-11-07.

Revision 5 records the maintainer's explicit 2026-11-09 scope instruction:
TASK-005 must implement both Gateway and direct verification copy removal.
Direct verification must reuse original evidence through execution and staging,
avoid read-only comparison copies and redundant step-output ownership, and hash
captured values without a disposable serialized buffer. Both Gateway and
Verification L1 are required; neither side may be left as a design or audit.
Other approved behavior and acceptance remain unchanged.

## Problem

On main `5234961d51798581a026834cfffd5efaadc226b4`, the existing Bifrost
resource owner resolves CPU, memory and scratch once and composes Scribe,
Oracle and Forge from one root. Two gaps remain:

- **Divergent CPU sizing.** Omitted Scribe coordination, ingress and
  persistence worker counts are derived by probing the host's CPU count
  separately from the resource owner. An effective CPU cap therefore sizes
  Oracle from the capped value while Scribe workers still follow the host.
- **Uncounted delivery work.** Each outbox writer allows up to eight concurrent
  frame deliveries (encode, submit to Scribe, wait for acknowledgement), four
  writers in all, so up to 32 per process. When a delivery's waiter is
  cancelled, blocking encoding that already started keeps running but no
  longer counts against that limit, so retries can accumulate encoding work
  beyond it.

## Why revision 2's scaling is withdrawn

Revision 2 set writers and lanes per tenant to `W = max(4, C)` with `8 × W`
deliveries. On a 32-CPU host that is 32 writers and 32 lanes instead of 4 and
4. Gateway L1, same host, same workload, median (min..max) of three
repetitions:

| Step | Main | Revision 2 |
|---|---|---|
| buffered 1 KiB metadata, 64 clients | 53,368 req/s (51,131..53,408) | 49,372 req/s (48,750..50,705) |
| streamed 1 KiB metadata, 64 clients, equal paced rate | 1.49 cores, 1,446 MiB peak RSS | 2.67 cores, 3,367 MiB |
| buffered 64 KiB metadata, 64 clients | 18,269 req/s | 17,018 req/s |
| buffered 64 KiB payload, 64 clients | 4,783 req/s, 11.1 cores, 8,882 MiB | 5,110 req/s, 16.8 cores, 14,053 MiB |

Each lane batches independently behind its own 50 ms linger. More lanes split
the same writes into more, smaller slices, and every slice pays fixed encode,
Scribe ingress, WAL and memtable-batch costs. This is the batching trade-off
Kafka documents for `linger.ms` and `batch.size`. The evidence never showed
lane concurrency limiting throughput. The large-payload step improved, but the
benchmark did not record why its slices were flushed, so the gain is not
attributed to any batching mode, and it cost 1.5× CPU and 1.6× memory.

Coalescing several idle lanes into one slice could recover batch sizes, but it
would make several lanes one frozen retry unit, block their successors
together, and require new fairness and frame-identity rules. That adds
scheduling and failure behavior only to regain efficiency main already has,
so it is rejected for this change.

## Proposed behavior

Resolve one immutable resource configuration per process at startup, before
any consumer starts, from that process's own observations. Let C be the
existing effective CPU (the observed CPU, which `WYRD_BIFROST_EFFECTIVE_CPU`
may only lower):

| Setting | Policy |
|---|---|
| Scribe coordination, ingress and persistence workers, when omitted | Existing formulas and floors, using C from the shared configuration: with `B = max(C − 2, 2)`, ingress `max(B / 3, 1)`, persistence `max(B − ingress, 1)`, coordination `max(C, 2)` (integer division) |
| Scribe workers, when set explicitly | The configured value and its existing validation |
| Oracle, Forge, storage/cache and transport sizing | Existing policies, using the shared configuration |
| Bifrost managed memory and scratch | Existing headroom, cap and free-space-floor policies |
| Outbox writers | 4, fixed, not derived from C and not configurable |
| Ordered outbox lanes per tenant | 4, fixed |
| Concurrent frame deliveries per writer | 8 |
| Concurrent frame deliveries per process | 32 |
| Outbox item/byte capacity | Existing configured limits and defaults |

Example: a 32-CPU pod with `WYRD_BIFROST_EFFECTIVE_CPU=8` today sizes Oracle
from 8 CPUs but derives Scribe workers from 32. After this change Scribe
derives 8 coordination, 2 ingress and 4 persistence workers from the same 8,
unless an operator set one explicitly. The outbox runs 4 writers, 4 lanes and
32 deliveries on every host, as on main.

## Requirements

- **REQ-001 (fixed outbox geometry):** the process Scribe outbox runs 4
  writers and 4 ordered lanes per tenant on every host and CPU setting, with
  two delivery limits: 8 concurrent deliveries within one writer's slice, and
  32 across the process. The process limit is enforced on its own, not only as
  4 × 8, because cancelled work can outlive the writer slot that started it
  (REQ-002). No CPU-derived outbox geometry and no lane-count setting exist.
- **REQ-002 (counted deliveries):** a delivery counts against the process
  limit from encoding through submission and the wait for its Scribe
  acknowledgement, including blocking encoding that continues after its waiter
  is cancelled. A cancelled delivery is not submitted; its count is released
  when its started encoding finishes. Retries cannot start uncounted encoding
  beyond the limit. The count bounds concurrency, not bytes. This applies to
  the Scribe outbox; the verifier submission outbox is unchanged.
- **REQ-003 (preserved outbox semantics):** one outbox, tenant/request
  ordering, identical retry identities, detail-before-summary,
  ACK-before-handoff, admission/loss policy and graceful shutdown are
  unchanged.
- **REQ-004 (diagnostics):** existing startup diagnostics report the resolved
  CPU and its source, memory, headroom, scratch, Scribe worker counts,
  explicit worker overrides, outbox counts, configured queue limits and
  existing dependency caps, without a new public product API. These are
  structured operator log fields, not a stable machine contract.
- **REQ-005 (one configuration):** the existing Bifrost resource owner
  resolves one immutable configuration at server startup. Scribe, Oracle,
  Forge, storage/cache, transport and the outbox use it or capabilities derived
  from it. No consumer constructs a sibling resource root or probes the host
  for its own sizing.
- **REQ-006 (shared CPU):** automatic CPU-derived defaults throughout Bifrost
  use the shared C; explicit worker overrides remain effective. A configured
  effective CPU cap never leaves a consumer sized from an independent probe.

## Payload allocation behavior

A physical layout stores selected Variant fields as typed leaves and the remaining
content as residual data. Logical values stay the same across layouts. Keep these
learned layouts in memory and local staging; always-canonical staging is not the fix.

For example, memory and staged runs use cached layout A. Every hot-file assembly
still takes a fresh sample. If it selects A again, matching shredded columns reach
the output without reconstruction and reshredding. If it selects B, incompatible
inputs are converted without losing residual content. Existing generations retain
their pinned layouts; future generations use the new cache hint.

The sampling unit stays the existing claim: if it rolls into multiple hot files,
those files share that claim's freshly selected layout. Every new claim resamples;
this revision does not introduce a separate sample for each rolled file.

Today claim sampling reconstructs whole batches before selecting sample rows,
merge reconstructs them again, and final encoding reshreds them even when the
selected mapping is unchanged. Gateway preparation and capture column construction
add full-payload materializations. These source-backed findings establish redundant
work, not the supplied profile's exact totals or an achievable allocation floor.

- **REQ-007 (integrated reads):** integrate the committed `variant-file-reads`
  work and retain layout-aware memory/staged reads, source-local planning,
  complete-value fallback, tenant proofs and generation/file leases. Close its
  existing correctness and performance obligations; merging commits alone is not
  proof. Do not import unrelated worktree deletions or review files.
- **REQ-008 (fresh learning):** every hot-file claim determines its output layout
  from a fresh deterministic, seeded, stratified sample under the existing policy.
  Typed leaves and residual fields both participate; evolving distributions,
  missing/null values, mixed types and nested/list values remain discoverable.
  Select sampled observations before complete-value reconstruction; do not
  reconstruct unsampled payloads merely to sample. Preserve the existing selected
  observations, seed, strata, list semantics and thresholds.
- **REQ-009 (preserved physical data):** when input Variant layouts match the
  freshly selected output layout, merge and final encoding preserve their shredded
  representation without complete-value reconstruction and reshredding. For mixed
  inputs, convert only incompatible Variant columns/inputs to the selected layout.
  Bounded canonical reconstruction is an allowed fallback for incompatible inputs,
  not matching ones. Preserve exact values, logical/physical footer identity,
  deterministic total order and byte-identical claim retries.
- **REQ-010 (request/capture copies):** remove redundant full-body preparation and
  copying in Gateway planning/dispatch and redundant intermediate payload
  materializations between redacted capture and Arrow column construction, using
  existing owners. Preserve model substitution, provider translation, validation,
  retry/fallback, secret scrubbing, binary references and error semantics. Reusing
  ownership must not retain an unnecessary second full-body representation.
  Only internal representation changes are authorized.
- **REQ-011 (bounded workspaces):** sampling, conversion, decoded merge and writer
  workspaces, including input/output overlap and concurrent claims, fit existing
  global/tenant/table reservations. Row counts alone are not a byte bound. Fit work
  to reserved bytes; an accepted large row remains processable without an uncounted
  oversize escape. Refusal precedes acceptance; acknowledged data drains or remains
  recoverable. Preserve existing admission policy and reservation categories.
- **REQ-012 (measured reduction):** demonstrate repeatable reductions in allocated
  bytes per reconciled call and payload-case peak RSS beyond baseline variation,
  attributable to removal of the specified passes and copies. Report live heap and
  post-drain memory separately. Retained work must settle across equivalent load/
  drain cycles. Lower throughput, larger queues or deferred work are not a fix.
  No absolute allocation floor, hard-RSS ceiling or new allocator is promised.
- **REQ-013 (direct verification copies):** remove duplicate original-input
  ownership between direct execution, the evaluation engine and hosted evidence
  reads. Reuse the same original declarations for staging without an equivalent
  clone at result assembly; keep synthetic attachments distinct. Borrow JSON for
  read-only assertion comparisons, retain successful step output once for
  dependency resolution and final results, and stream context hashing without a
  disposable full JSON buffer. Preserve public request/result and executor APIs,
  exact verdicts/captured values, validation, deadlines, concurrency, permissions,
  audit, pinned evidence and file limits. Direct input records retain
  `queue_execution=false`; no queued/group/scheduler redesign is authorized.
  Copies required by independent executor or cited-result ownership may remain
  with an explicit reason; sharing must not merely postpone a duplicate copy.

### Costs and authority changes

Fresh sampling, layout conversion, sorting, canonical WAL serialization and Parquet
encode/decode still cost CPU and memory. Sharing Arrow buffers does not make raw
received JSON a Variant array. Different generations/files may retain different
layouts; readers use native metadata and expose the same logical values.

This revision intentionally replaces the blanket reconstruction mechanism in the
Scribe Variant sampling/merge paragraph of `architecture/bifrost-design.md`
(currently lines 480–493). Update that owner in the same implementation: retain
fresh sampling and complete-value semantics, reconstruct only selected sample
observations and incompatible merge inputs, and preserve matching physical columns.
Keep learned-cache policy, canonical WAL, local stage format/metadata, recovery,
promotion and publication fencing. Forge continues combining source-footer counts
with its existing row-weighted policy; it does not freshly sample payloads.
No new persisted format, claim-membership key, layout registry, dependency family
or public setting is authorized.

## Invariants, scope and non-goals

- **INV-001:** detection failure, invalid settings and unrepresentable derived
  counts refuse startup before serving; no consumer starts with a fallback
  plan.
- **INV-002:** each process sizes itself from its own resources. A pod that
  sends its outbox to a peer Scribe uses its own configuration; the receiving
  peer uses its own.
- **INV-003:** usage counters, reservations and pressure monitoring stay live;
  only the startup configuration is immutable.
- **INV-004 (future concurrency changes):** a proposal to change outbox
  writers, lanes, delivery limits or batching must first show measured outbox
  saturation and a profile identifying its limiting stage. A CPU count alone
  is not evidence of an outbox bottleneck.
- **INV-005:** every hot-file assembly resamples; a matching layout permits
  physical reuse, never skipping learning or ignoring residual fields.
- **INV-006:** canonical WAL, ACK-before-handoff, staged recovery authority,
  schema identity, publication fencing and exact tenant attribution remain.
- **INV-007:** learned-leaf reads in memory/staging remain; no always-canonical
  replacement or second resident cache is introduced.
- **INV-008:** reductions cannot come from dropping evidence, extra redaction,
  changed responses, smaller admitted payload limits, weakened accounting or work
  left outstanding beyond measurement.

Production and production-equivalent test startup use the same resolution
path, with deterministic observations injectable for tests. Tenants keep
sharing the existing global governors with tenant attribution; every outbox
slice stays tenant-scoped. No permission, audit or durability boundary
changes. The verifier submission outbox keeps its existing four-lane geometry.

Excluded: CPU-scaled outbox geometry, cross-lane slice coalescing, linger or
batch-size tuning, queue-capacity, database-pool, WAL IO or shard scaling,
gateway admission or ownership-policy redesign, capture-content policy changes,
reservation fairness, memory partitions, new per-frame byte allowances,
speculative preallocation, new schedulers or pools,
live reconfiguration, new public settings, dependencies, SDK or wire changes.

Measured request/capture copy removal and byte-bounded work within existing Scribe
reservations are included. The separate draft `gateway-latency-capture` packet is
not adopted: capture admission, post-response ownership and scheduling policy
redesign remain outside this revision.

## Acceptance

- **AC-001 (startup resolution):** injected observations at C=1, 2, 4, 8, 16
  and 32 all resolve the fixed 4/4/8/32 outbox counts. Omitted Scribe workers
  follow the existing formulas from C; explicit settings keep their values; a
  cap lowers C for every consumer. Invalid settings, zero and overflow refuse
  startup through existing errors.
- **AC-002 (delivery limit and correctness):** gated-sink tests show the
  per-writer and process limits hold while acknowledgements are withheld, that
  cancelled blocking encoding stays counted until it finishes, and that a
  retry cannot start encoding past the limit. Existing same-lane order, retry
  identity, detail-before-summary, ACK-before-handoff, tenant separation,
  admission/loss and shutdown evidence remains green.
- **AC-003 (non-regression):** compare main and this change on the same host
  with every existing `bench:l1:gateway` step, three repetitions each as the
  driver already runs. For each step, compare the candidate's median sustained
  rate, server CPU, sampled peak RSS and server p99 against main's minimum and
  maximum repetition. A candidate median outside that range in the worse
  direction is re-run as a fresh, back-to-back main/candidate pair of that
  step; it is a regression only if it falls outside again. Errors, lost, shed
  and undrained captures stay zero. A larger-host scaling demonstration is not
  required.
- **AC-004 (one configuration):** combined and split-role targets, including a
  producer without Scribe, each started from injected observations and an
  effective CPU cap, resolve one configuration whose worker and outbox counts
  match what their consumers run and what the startup diagnostic reports.
  Consumer startup performs no additional host probe.
- **AC-005 (memory and scratch):** existing resource-owner tests continue to
  show unchanged headroom, caps, floors and shared accounting.

- **AC-006 (integrated reads):** queries over canonical/learned memory, staged and
  published data agree on complete values and selective results; covered paths avoid
  unrelated reconstruction. Complete the imported packet's remaining matched
  memory/staged performance proof without weakening its criteria.
- **AC-007 (learning/reuse):** cold input, unchanged learned layout, changed
  distribution and mixed layouts produce the freshly sampled layout and exact
  ordered values. Evidence shows no full unsampled reconstruction, and matching
  columns bypass merge reconstruction and final reshredding. Eligible residual-only
  fields become newly learned. Repeated claims write byte-identical objects.
- **AC-008 (request/capture):** owner tests demonstrate reduced materialization
  with unchanged model rewriting, native/translated responses, validation, selected
  redacted content, binary references and errors. Real server journeys read back
  exact attributed captures after drain, including cancellation, streaming, retries
  and split-role submission.
- **AC-009 (memory/durability):** large-payload, multi-run and concurrent claims
  under tight budgets account input/output overlap. Work progresses in reservations
  or applies existing pre-acceptance pressure; failure/cancellation releases once.
  Acknowledged rows survive pressure/restart and remain exact through staging,
  publication, promotion and compaction.
- **AC-010 (allocation evidence):** compare matched quiet-host runs with identical
  builds, actual resource/outbox geometry, payloads and repetitions. Use unprofiled
  L1 for non-regression and paired profiled runs for attribution. In the 64 KiB-per-
  body payload case at c64, median allocated bytes per reconciled call and peak RSS
  must each fall below the baseline's minimum repetition; repeat a fresh pair if
  unsettled. Profiles must also demonstrate removal of the specified passes/copies.
  Record both bodies (about 128 KiB captured per call), matching profile deltas to
  all warmup/measured/drain work in their interval. Report outstanding/completed
  work, throughput, CPU, p99, live heap, RSS time series and post-drain ownership
  across repeated equivalent cycles. Owned bytes settle within budgets rather
  than accumulate; RSS need not immediately return to its initial value. Separate
  total request allocation from incremental payload-capture cost using the existing
  matched metadata comparison; metadata capture is not capture disabled.
  Preserve AC-003's non-regression rule and zero errors/lost/shed/
  undrained captures. No 100 KiB or 3× allocation floor is asserted.
- **AC-011 (direct verification):** focused ownership/allocation observations and
  existing route/engine regressions show removal of the REQ-013 copies with exact
  original input/result readback, passed/failed/inconclusive behavior, audit
  counts, dependency outputs, capture digests and cancellation/concurrency
  behavior. Compare immutable pre-TASK-005 and candidate builds through every
  default `bench:l1:verification` step and its three repetitions on the same quiet
  host. Use paired profiles for clone/allocation attribution and unprofiled runs
  for throughput, CPU, overhead p99 and peak RSS. A worse-outside-baseline-range
  candidate median requires one fresh back-to-back pair and fails non-regression
  if worse again. Refusal, lost writes, incorrect verdicts and undrained work
  remain zero. Default assertion-only L1 does not cover action-output or hashed
  capture paths; focused owner observations prove those reductions without a new
  benchmark matrix. A valid record alone is not proof of improvement. Existing
  main/pre-TASK-002 Gateway/Scribe obligations are not reset or waived.

For AC-010, retain an immutable pre-TASK-002 resource-scaling baseline with fixed
revision-3 geometry, before read integration or copy removal. Compare the final
candidate against it; an integration-only run may separate read integration from
write-path savings. AC-003 separately compares the final candidate with main, and
the imported read packet retains its original performance comparison. Historical
TASK-001 L1 misses are not waived: the final candidate must satisfy these rules,
with no discretionary pass.

All existing correctness, reference and applicable real client/server journey
gates remain. A real SDK verification journey on combined and split targets
continues to read back exact, attributed evidence after drain.

## Decisions and approval

Revision 3 was approved on 2026-11-06. The maintainer approved revision 4 on
2026-11-07: retain shared resources
and fixed 4/4/8/32 geometry; integrate learned-layout reads; resample every hot-file
claim while preserving matching shredded columns; remove specified request/capture
copies; fit conversion work within existing byte reservations; and require matched
allocation/RSS evidence. This approval includes the explicit Scribe architecture correction above.

On 2026-11-09 the maintainer explicitly required TASK-005 to implement Gateway
and verification copy removal together. Revision 5 records that scope approval
as REQ-013/AC-011; TASK-005 owns both implementations and both L1 proofs, rather
than deferring direct verification to an audit or separate task. TASK-001–004
retain their original revision-pinned obligations and historical evidence.

There is no new persistent format, claim-membership, sampling-policy, Forge,
security or ACK decision delegated to execution. TASK-002 was the added task
proposal against revision 4; TASK-005 now carries the revision-5 copy-removal
scope. TASK-001's existing revision-2 prose
and evidence do not authorize restoring scaled geometry; the integrated candidate
must satisfy revision 3's fixed counts before allocation comparisons. Its revision-3
implementation intent is unchanged.

The TASK-001 dependency requires its revision-3 resource and cancellation behavior
before this work, not an assertion that historical performance evidence passed.
Final revision-5 verification must close outstanding L1 acceptance.

## Authority and baseline

Baseline: main `5234961d51798581a026834cfffd5efaadc226b4`. Owners:
`crates/wyrd/wyrd-server/src/config.rs`, `boot/`, `scribe_outbox.rs`,
`crates/shared/wyrd-runtime/src/outbox.rs` and
`crates/vala/vala-bifrost-redux/src/resources.rs`. Evidence: the Execution
Evidence section of `tasks/TASK-001-shared-resources-and-scaled-outbox.md`.
Added owners are Scribe sampling/merge/writer and admission, Gateway adapter
preparation, server capture projection, shared Variant encoding and the existing
capacity driver. TASK-002 records reuse and verification. The imported read source
is commit `5f92c71f33a98e7d6616aab9f0a201762da84ebf` on
`change/variant-file-reads-task-002`; its outstanding performance proof is included.
Follow [Bifrost memory](../../../architecture/bifrost-design.md#admission-and-memory),
[resource invariants](../../../architecture/bifrost-design.md#resource-and-failure-invariants)
and [overload](../../../architecture/operations/reliability-and-recovery.md#capacity-and-overload).
The [Wyrd doctrine](../../../architecture/wyrd-design.md#doctrine) owns the server
boundary and journey proof; [security posture](../../../architecture/wyrd-security-posture.md#security-principles)
owns unchanged tenant and audit authority.

## Revision history

- Revision 1, 2026-11-06: fresh-start draft. Its process-wide 2C delivery limit was
  rejected because it reduced available baseline concurrency.
- Revision 2, 2026-11-06: preserved eight deliveries per writer, scaled writers
  and lanes with CPU above a four-writer floor, and required one shared startup
  resource configuration. Approved 2026-11-06.
- Revision 3, 2026-11-06, approved: Gateway L1 showed revision 2's CPU-scaled
  lanes reduced throughput and raised CPU and memory through batching dilution,
  with no evidence that lane concurrency limited throughput. Restores fixed
  4/4/8/32 outbox geometry, keeps the shared configuration and correctness
  improvements, replaces the scaling criterion with L1 non-regression, and adds
  INV-004.
- Revision 4, 2026-11-07, approved: adds `variant-file-reads` integration, fresh
  per-claim sampling with physical-layout preservation, request/capture copy
  removal, byte-bounded workspaces and matched memory/allocation proof. Intentionally
  replaces unconditional sampling/merge reconstruction in the Bifrost owner, updated
  with the implementation. Approved by the maintainer on 2026-11-07.
- Revision 5, 2026-11-09, approved scope instruction: the maintainer explicitly
  required TASK-005 to implement both Gateway and direct verification copy
  removal. Adds REQ-013/AC-011 and Verification L1 proof while preserving the
  previously approved behavior, boundaries and baseline obligations.
