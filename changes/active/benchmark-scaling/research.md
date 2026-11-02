# Benchmark scaling research

Research for SPEC-benchmark-scaling revision 1, **draft awaiting human approval**.
Static inspection only, 2026-10-31. No builds, tests, benchmark execution or host
load. Executable/source inspection reference: `4379acc295df2e0a955d89556ba4296a4b6a6330`;
final measured reference: `92b62f9c0d3c8de03fa0ec4ed05c53d5cb435e89`.
Concurrent packet retirement is outside this change's write set.

## How to recover the evidence

The historical files below were read from `changes/active/benchmark-experiments/`.
They may disappear when that packet is retired. Recover their exact content
with `mise exec -- git show 4379acc295df2e0a955d89556ba4296a4b6a6330:<path>`.
Historical source is evidence, never architecture authority.

| Historical path within that packet | Relevant evidence |
|---|---|
| `tasks/TASK-008-hot-read-path-closeout.md` | Current staged restart plan; retained implementation and rollback evidence; V-SQL/V-READER/V-DIAG; final acceptance table. |
| `tasks/TASK-009-unified-queue-and-closeout.md` | Final-source gate and five L1 records at `92b62f9c0`; benchmark-records publication `274cbfb83`; verification driver-budget exception. This closes a different runtime change, not hot shredding. |
| `codex-reviews/item-10-prompt.md`, `item-10-recommendation.md` | Original 50.4 ms hot versus 17.7 ms published attribute-filter question; row lifecycle, sampling, alternatives and unmeasured recommendation. The recommendation preferred recent-data learning, but current Bifrost authority specifies successful-claim learning. It is not approval to change sampling. |
| `codex-reviews/item-9-prompt.md`, `item-9-diagnosis.md` | Historical write-scaling investigation: serialized WAL durability, fragmented shard groups and a then-four-worker WAL lane; staging/device contention hypothesis. Later worker policy changed: current `wal_io_workers` raises workers to at least shard count. Do not reuse the historical four-worker diagnosis as current proof. |
| `review/TASK-009-R1-f0ea3b402/findings-validation.md` | Missing final-source proof and public journey, and contradictory architecture ordering were distinct from correctness claims. |
| `review/TASK-009-R2-b6e69d66b/verdict.md` and `verification.md` | Bounded remediation PASS explicitly did not complete TASK-009's final gate/measurement/publication. Later task evidence supplies those. |
| `review/TASK-004-R1/task-review-invariants.md`, `review/TASK-004-R3-0bf0cb866/findings-validation.md` | Timing/cohort reconciliation and smoke judgment findings concern measurement integrity. They are not a TASK-008 physical-work verdict. |

The inspected review directory contains no dedicated TASK-008 review packet.
Its substantive failure/rollback explanation is retained in the task itself and
rollback commit. No independent PASS for hot shredding is inferred from reviews
of other tasks. Earlier reviews and recommendations describe their own source
revision and workload; they do not establish the cause of final L1 misses.

## Hot shredding: intent, implementation, rollback and remaining behavior

The existing lifecycle validates canonical Arrow Variant input, keeps canonical
IPC in the WAL, and may retain one learned physical representation in memory.
A successful claim assembly teaches an advisory layout for exact tenant/table/
version; a newly opened generation pins it, while an already-open or cold
generation retains its layout. Frozen rows stage as stored; claim merge reads
complete logical values and selects its own output layout. Forge combines
footer evidence under its existing rules, without discovering new keys from
rows. Clients and unions see canonical logical results.

Current Bifrost policy uses seeded principal-stratified claim sampling: subjects
below 30 rows share a stratum; 99% confidence and ±0.02 margin determine sample
size; a compatible key qualifies at 8% of sampled non-null roots. Forge's
retention threshold is 10%. These are existing writer controls, not new knobs.
The item-10 recommendation's alternative recent-data learner remains unapproved.

The requested commits were inspected with `git show` (including relevant diffs):

| Commit | What it actually introduced and what it does not prove |
|---|---|
| `34ba4f9b3` | Adapted a predicate to a generation's physical schema through DataFusion's expression adapter; filtered physical rows before reconstruction, converted changed bounds to inexact, retained the outer query filter. Failed adaptation/evaluation pruned nothing. This can reduce reconstructed rows but still reconstructs the retained root needed by the outer filter. |
| `22a0e3d76` | Learned-layout gauge and a two-day hot benchmark lifecycle: read canonical day 1, publish a claim, then read learned day 2. It verifies learning occurred, not that COUNT uses only leaves. |
| `12fc33f08` | Documented that lifecycle and raised hot's budget to 480 s. It is benchmark documentation/configuration, not another runtime optimization. Later preparation moved learning outside measured windows; do not restore this historical default. |
| `e8c0a0ff2` | Revision-22 spec and TASK-008: ordered live projection expressions, shared staged/published planner/cache, file facts, time-statistics fix and scan evidence. This commit specified outcomes rather than proving implementation. |
| `9424bd317` | Carried staged size/rows/time bounds from run footers; resolved table time axis; used `PlannedFiles` and node-local footer/range caches for staged and published reads. Retained local staged leases/store differences. Removed per-read stat/unknown-file facts. |
| `5459a1244` | Carried ordered projection expressions through private assignments/protobuf; evaluated physical memtable expressions; pushed filter/projection into staged scans; checked full authenticated schema separately from narrowed output and bound projection to decoded fragment authority before source access. |
| `8e9ee893e` | Installed union projection pushdown for every Oracle session, with local/remote scalar and layout telemetry journeys. A projection using all closure columns could otherwise remain above the union. It still did not test the filter-only COUNT shape end to end. |
| `07a157cb9` | Restored TASK-008 runtime to `471f69ff9`, keeping task/records. Reverted expression transport/pushdown, shared-reader changes, scan row/byte facts, fork repin and fixture/count repairs. This was not removal of the earlier learned-layout infrastructure. |

The full attempt included later work, not only the listed commits. Retained
evidence names fork `8d6d6bf58` fixing unknown timestamp containers that used
untyped nulls, memory scan accounting, and measurements at `9a6e15750`.
Rollback returned the fork pin to `19796a13e`; a pushed fork fix is not proof
that current executable source includes it.

### Why it was rolled back

The task records five performance-acceptance misses and a red `verify:bifrost`
(seven failures). Its earlier explanation that those failures also occurred at
the base did not close acceptance. Scalar leaves on hand-built fixtures were
not the benchmark's `COUNT(*) ... WHERE attributes ->> ...` query. The retained
leader filter could require reconstruction after physical filtering. Equal
bytes between shredded and long-tail tests were inconclusive because those
tests used different keys/selectivities. A VALID record only meant the run was
measurable and correct under its integrity checks.

At `9a6e15750`, compared with the recorded references:

| Observation | Interpretation |
|---|---|
| Shredded memory p99 36.635 → 38.011 ms; CPU/query 185.897 → 164.740 ms | Lower CPU did not satisfy the latency outcome. |
| Shredded staged p99 98.566 → 90.571 ms; CPU/query 258.772 → 254.310 ms | Improvement, without isolating projection versus shared-reader effects. |
| Memory point c1 770.1 queries/CPU-s versus recovery ≥861; c64 5.022 ms versus ≤4.31 ms | Historical recovery acceptance missed. Earlier fixtures differ, so not every historical comparison is causal. |
| Parquet point c64 10.789 → 11.297 ms; point c1 CPU +3.2%; memory small aggregate CPU +19.9% | Protected workloads regressed under the task's zero-tolerance comparison. Mechanism was not demonstrated. |

Historical records are `hot/20261029T223134Z-9a6e15750.json` and
`parquet/20261029T223613Z-9a6e15750.json` under Bifrost history. They remain
attempt evidence, not current-candidate proof.

### What remains

Prove real scalar and filter-only SQL, including empty projection/count row
cardinality, through canonical/learned memory, staged data and remote ownership.
Do not reconstruct discarded roots when covered leaves suffice. Preserve exact
mixed/residual/null behavior and fail before IO on invalid private authority.
Recover the shared-reader outcome through the existing file owner; retain
query-bound leases and validate cache identity/immutability. Fix demonstrated
timestamp pruning and truthful scan facts at their owners if still missing.
Measure same-key comparisons and ingest cost independently from reader changes.
No wholesale cherry-pick or reset is justified.

## Final L1 evidence and the outcomes it motivates

All links below refer to retained schema-3 records on the separate
`benchmark-records` history worktree; code branch source is `92b62f9c0`.
Host: AMD Ryzen 9 9950X, 32 logical CPUs, approximately 91 GiB RAM, Linux
x86_64; release, frame pointers, local `file://` store, three repetitions.
Figures are medians across repetitions; percentile readings carry histogram
bucket bounds and sample counts. These host results are not production SLOs.

| Record / case | Result | Existing target |
|---|---|---|
| [Gateway overhead](../../../benchmarks/history/gateway/overhead/20261031T131441Z-92b62f9c0.json), buffered 1 KiB metadata c64 | overhead p99 0.491263 ms | `<0.5 ms`, met |
| Same, streamed 1 KiB metadata c64 | overhead p99 0.239103 ms | `<0.5 ms`, met; retain every declared handoff check |
| Same, buffered 64 KiB metadata c64 | overhead p99 1.876991 ms; 20.501 cores | `<0.5 ms`, missed |
| Same, buffered 64 KiB payload c64 | overhead p50 2.541567 ms, p99 156.762111 ms; 11.460940 cores | `<0.5 ms`, missed |
| [Bifrost parquet](../../../benchmarks/history/bifrost/parquet/20261031T130839Z-92b62f9c0.json), point c64 | server p99 11.698175 ms | `<5 ms`, missed |
| [Writes-parquet](../../../benchmarks/history/bifrost/writes-parquet/20261031T125731Z-92b62f9c0.json), spans peak | 15.852214 wire MiB/CPU-s | `≥30`, missed |
| [Bifrost hot](../../../benchmarks/history/bifrost/hot/20261031T130456Z-92b62f9c0.json) | Retained current memory/staged comparison baseline; some memory scan facts absent | Absence cannot be relabelled zero or compared as measured work |

All four listed Bifrost/Gateway records are VALID. The fifth closeout record,
Verification, was INVALID on driver saturation (6.54/6.47 versus 6.40 cores)
and accepted by the maintainer as recorded in TASK-009. That exception does
not change this proposal's validity or performance rules.

Hot Variant work directly targets attribute read CPU, latency and unnecessary
reconstruction. Event point lookups do not automatically benefit from it.
Shredding adds write conversion and can worsen span efficiency; the write miss
is therefore both a protected cost and potentially separate work. Historical
WAL evidence motivates checking waits but does not prove the final span miss's
cause. Neither it nor the gateway's spare CPU establishes hardware saturation.

## Gateway path and constant inventory

CodeGraph was used before source tracing, with queries for gateway capture,
connection pools/concurrency/buffers, `OutboxWriter`, `ScribeSink`,
`GatewayEngine`, `adapter/http.rs`, `adapter/stream.rs`, `ReadableSource::project`
and `ScribeTailResolver`. Some broad queries returned unrelated symbols;
targeted source inspection then resolved the concrete path below.

Public ingress authenticates (including cached API-key exchange), reads a
tenant snapshot, authorizes and reserves capture, applies relevant ledger
charges, then invokes the shared GatewayEngine. HttpProviderDispatch uses the
screened Skald transport; buffered or streaming handoff retains accounting and
capture ownership. Capture and audit enter the one Scribe outbox; its lanes
project batches and submit locally or over authenticated peers until ACK.
Capture reservations are held until completion releases capacity. Four writers,
four lanes, frame fanout and batch size therefore affect upstream admission
even though the Scribe write happens after the provider response.

This is a source-backed inventory of the inspected serving/capture path,
including fixed defaults and derived ratios. Protocol marker strings, test
fixtures and unrelated verification workers are excluded. Installed-library
internals and every media codec's private limits were not exhaustively audited;
task planning must revalidate the actual baseline and dependency versions.

| Owner and source | Current value | Meaning and relevance |
|---|---|---|
| `wyrd-server/src/config.rs` `default_pool_max`, `default_pool_acquire_ms`; `wyrd-sql/src/pool.rs` | app max 32, min 2, acquire 5 s; platform admin max 2/min 0/acquire 5 s | Configurable role budgets, not raw hardware capacity. App idle/lifetime 5/30 min, statement cache 256; platform 1/15 min, cache 64. Validate aggregate replica/surge budget before enlarging. Migration max 2 is startup-only. |
| Same server config; `src/http/router.rs::apply_protected_edge` | concurrency 1,024; body 1 MiB; request 30 s; audio upload 25 MiB | Configurable shared edge settings. Concurrency can couple unrelated routes. Body/audio sizes are safety/protocol policy, not worker sizing. |
| `components/gateway/api_keys.rs` | 4,096 entries; reuse at most 60 s and token expiry | Memory/cache churn versus revocation/freshness policy. One warm tenant/key per client in L1 does not exercise tenant-cardinality scaling. |
| `components/gateway/snapshots.rs` | 4,096 tenants | LRU is count-bounded, not byte-accounted; large tenant configurations affect memory. Invalidation generation and cache-off fallback cannot be weakened to improve hit rate. |
| `wyrd-sql/src/listener.rs` | heartbeat 30 s, probe 5 s, reconnect 1 s | Existing silent-listener failure bound and cancellation policy. Not an automatic CPU multiplier. Separate listener connections consume deployment budget. |
| `skald-providers/src/endpoint.rs` via `EndpointPolicy::transport` | DNS 5 s, connect 10 s, exchange backstop 10 min; idle-per-host `usize::MAX` | Actual gateway egress overrides generic transport defaults. Unlimited idle count is not a fixed active-call cap; library idle expiry still applies. Need aggregate connection accounting, not a false claim that gateway is limited to eight connections. |
| `skald-providers/src/transport.rs` | generic timeout 30 s/connect 5 s/idle 8; max response 16 MiB | Gateway overrides the first three through EndpointPolicy. Response bound still applies unless the concrete path supplies its own limit; private response/read buffers need accounting. |
| `wyrd-gateway/src/adapter/stream.rs` | FRAME_BUFFER 16 per relay; terminal grace 5 s | Native and translated media/chat relays use bounded channels. Frame count alone does not describe bytes; per-event limits and slow-reader lifetime matter. A oneshot handoff is one completion, not a configurable queue. |
| `wyrd-gateway/src/health.rs` | 5 failures; cooldown 30 s | Circuit-breaker policy. Changing it affects retries/availability, not just capacity. |
| `components/gateway/invocation.rs` | reclaim grace 1 min; capture retry-after 1 s | Lease/retry policy; capture waits against the call deadline. Not arbitrary worker sizing. |
| `components/gateway/multipart.rs`; `wyrd-gateway/src/adapter/mod.rs` | boundary 70 bytes, headers 16 KiB, text part 1 MiB; embedding inputs 2,048 | Parsing/protocol bounds. Preserve validation; not explanations for the buffered 64 KiB chat tail without path evidence. |
| `wyrd-server/src/scribe_outbox.rs` | SCRIBE_WRITERS 4; SCRIBE_LANES 4 per tenant | Global in-flight slices and tenant lane parallelism. Direct candidates for a resource-coherent policy; increasing both can amplify Scribe pressure. |
| Same | linger 50 ms; batch 4,096 writes / 32 MiB | Batching trade-off: fixed linger adds visibility delay; batch lifetime holds reservations. Existing comments describe avoiding very large backlog writes. Neither comment proves final p99 attribution. |
| Same | outbox defaults 1,048,576 items / 1 GiB | Configurable via Scribe settings. Count, bytes, queued, reserved, retrying and in-flight ownership must reconcile. Defaults do not describe total process RSS. |
| Same | 8 frames in flight per slice; frame budget = request ceiling / 4; peer submit 30 s | Fanout and encoded/decoded memory depend on all active slices and Scribe request budget. At a 16 MiB ceiling, nominal frames are 4 MiB and one 32 MiB write is eight frames. Payload overhead and actual Arrow encoding still require validation. |
| Same | TENANT_SPAN_PERMITS 2,048 | Bounds server-span materialization for one tenant. Shared evidence pressure must be separated from capture throughput. |
| `wyrd-runtime/src/outbox.rs` | reserved share 3/4; retry 50 ms → 5 s; shed log 1 s | Reserved/unreserved item and byte shares, retry pacing and logging. Preserve audit availability and identical admitted-slice retry; do not interpret the whole budget as capture-only memory. |
| `wyrd-server/src/boot/mod.rs`; Bifrost `resources.rs` | existing ResourcePlan effective CPU/memory; resource-derived Oracle worker runtime; Scribe coordination/ingress/persistence/WAL settings | Existing resource ownership to evaluate before adding a parallel planner. Bifrost's governed root is not a free gateway allocation. Current WAL workers are at least shard count, superseding historical four-worker assumptions. |

The final gateway record also records DB environment maxima of 8 for
`WYRD_DB_MAX_CONNECTIONS` and `WYRD_DB_MAX_CONNECTIONS_VALA`. Those recorded
inputs are not proof every effective pool used eight: configuration projection
and role pool selection must be confirmed before attributing waits. Likewise a
pool's *idle* limit is not its active request concurrency. The default 1,024
edge cap is above 64 clients; its existence alone does not explain this run.

### Hypotheses and proof that could distinguish them

Reservation waiting behind slow capture drain is plausible: Scribe ACK releases
reserved bytes; the fixed writer/lane/frame/batch geometry may limit that drain.
But locks, payload projection, allocator pressure, database contention, device
durability and client backpressure can also produce a tail with low aggregate
CPU. Preserve the same captured bytes, answer population and timing boundary
while separating enqueue/start/ACK waits, bytes held, CPU and dependency service
time. Attribute each request's overhead directly; never subtract two aggregate
percentiles or sum overlapping stage histograms as exclusive time.

Scale one identified allocation at a time, holding downstream budgets and
workload fixed. Then validate the combined policy on constrained and larger
allocations, multiple tenants and slow readers. More writers that merely move
the queue to WAL are not proof of successful resource scaling. Larger
outboxes that improve a five-second window by accumulating evidence are not
sustainable capacity; show bounded backlog, drain and recovery.

## Existing gateway-capacity packet: reuse and conflicts

Read `spec.md`, revisions 4, 5 and 7 and TASK-003. The directory contains no
`spec-revision-6.md`, although revision 7 refers to revisions 4–6. Do not invent
missing revision authority. Revision 7's explicit source-order rule and current
architecture remain readable; any reliance on an omitted amendment needs
reconciliation before approval.

The main packet defines shared real-server overhead and paced multi-tenant
capacity measurements; revision 4 admits snapshot caching, SQL admission,
common production/test startup and row attribution, while retaining settlement
semantics. TASK-003 bounds silent LISTEN failures (30 s heartbeat + 5 s probe,
1 s reconnect) and preserves invalidation fences/uncached fallback. Revision 7
puts source order in the Verifier and rejects cross-batch arrival-order promises.
None justifies a second harness, gateway owner, notification service or Scribe
writer path. General performance optimization was explicitly excluded there.

Revision 5 allowed shed-only capture saturation and prohibited reservation
backpressure. Current `wyrd-design.md` Principal model and invocation/outbox
source require reservation before dispatch; current L1 requires one capture
per arrival without drop/loss/shedding. Operations Capacity and overload still
describes captures as non-blocking shed writes. The narrow gateway-specific
contract governs this draft; the inconsistency must be resolved in authority
before an implementation-ready handoff. This follow-up excludes restoration of
revision-5 shedding, its old matrices/targets and startup/worker/source-order
reimplementation. It does not assert that the entire old packet is superseded.

## Alternatives and remaining limits

Keeping the existing runtime and documenting misses costs least but leaves
the required real-SQL behavior and resource ceilings unresolved. Replaying
TASK-008 wholesale repeats unisolated changes and weak count-query evidence.
A second canonical/shredded copy makes read fallback simple but consumes more
resident memory and conversion work. New fixed key lists or recent-data
sampling change policy and do not fix downstream reconstruction by themselves.
An elastic queue without a budget merely postpones overload. Multiplying all
constants by cores violates memory, connection and freshness constraints.

The proposed approach retains existing learned layouts, proves physical work
through actual SQL, shares existing reading owners and makes gateway geometry
coherent with explicit allocations. It offers no unmeasured speedup or selected
private algorithm. Exact allocations, tolerances, fairness and resource
configuration ownership remain material decisions in the specification.

Local raw profiles referenced by old notes were not recovered or independently
reanalyzed. Retained records support measured outcomes, not a final causal
diagnosis. History links require the benchmark-records worktree; immutable Git
references preserve retiring packet evidence without copying it into this one.
