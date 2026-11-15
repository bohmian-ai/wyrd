---
id: TASK-003-R3
kind: remediation
status: ready
spec: SPEC-verifier-code-mode
spec_revision: 9
requirements: [REQ-009, REQ-011, REQ-012, REQ-013, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027, REQ-028, REQ-029, REQ-030, REQ-031, REQ-032, REQ-033, INV-001, INV-002, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, AC-007, AC-008, AC-009, AC-010, AC-015, AC-017, AC-018, AC-019, AC-020, AC-021, AC-022, AC-023, AC-024, AC-025, AC-026, AC-027, AC-028, AC-029, AC-030, AC-031, AC-032, AC-033, AC-034]
depends_on: [TASK-002B, TASK-003-R1]
parent_task: TASK-003
remediates: [TASK-003, TASK-003-R1]
---

# Dynamic process capacity and verification benchmark closeout

## Outcome and Value

A Bifrost-only process can use all available process capacity after necessary
server/Bifrost baseline protections. A process running verification, Gateway
and Bifrost can use the same capacity in whatever mix its current workload
needs. Idle capability shares cannot strand resources. Nothing is promised
twice, and query memory pressure still normally leads to disk spilling.

For example, idle Python workers and empty caches must not withhold a fixed
share from an active Bifrost query. When verification demand arrives, owners
return reclaimable capacity safely; resident Python state and outstanding
writes remain charged until their ownership actually ends. Waiting and
cooperative failures remain bounded and visible to callers.

This task owns **all** outstanding TASK-003/R1
closeout: six-population qualification, measured targets, profile analysis,
legacy reports, evidence reconciliation, sustained recovery and two replicas.
R1 dependency means its implemented cleanup, not its outstanding benchmark
PASS. Do not rebuild already-correct workload or profiling machinery.

**Planning disposition: ready.** The maintainer approved spec revision 9,
which adopts SR1–SR6 below. Required architecture reconciliation remains an
owner obligation before or with behavior. Existing implementation evidence
remains untouched.

## Spec revision required

These are material differences, not permission to reinterpret revision 8.
Quoted spec text is from preserved approved revision 8; architecture text
identifies the narrower owner that also needs reconciliation. The maintainer is being asked to approve these
boundary/proof changes in a specification revision, not a private algorithm.

1. **Dynamic use across every capability, not merely derived limits.**
   REQ-027 says “Replace the unevidenced 16-workspace, 2/8-worker, 16-waiter and
   2-GiB-pool defaults with derived capacity.” It also says “Remaining fixed
   capacity values require evidence, not just tests of their arithmetic.”
   Those statements permit evidenced fixed capacity and do not require
   use-driven borrowing across *all* server, Gateway and Bifrost work. Extend
   REQ-027/INV-010/AC-028 to require the maintainer's rule: only necessary
   server and Bifrost defaults are hard allocations; all other capacity is
   acquired/released dynamically, with no fixed capability shares. Separate
   security/content/per-operation ceilings from resource reservations. Measure
   and justify the baseline defaults; keep the supported 4-GiB deployment
   floor unless separately approved. Add Bifrost-only, mixed-capability and
   changing-demand proof. “All resources” means usable capacity after necessary
   safety/progress protections, not a promise of 100% physical RAM occupancy.
   The OS scheduler shares CPU dynamically across capabilities; only hosted
   Python runnable turns are bounded by effective CPUs. Oracle slots and
   Scribe/Forge CPU-derived concurrency retain their current owners; no
   cross-capability CPU ledger. Verification execute/enqueue bypass generic
   edge concurrency as `POST /v1/query` already does, because hosted admission
   bounds them by bytes. Other routes keep the 1,024 in-flight edge limit to
   protect their request handling from overload; buffered bodies still charge
   shared memory. `monty_min_workers` is a prewarming target, not a hard
   allocation: warm workers are charged and reapable when idle under pressure.
   At checkout, a live Python session reserves its enforceable peak envelope
   (allocator ceiling plus measured baseline, stack and IPC) until confirmed
   reset or reaping; suspension releases only its runnable turn. Waiting
   requests charge retained bytes to shared memory without a fixed waiting
   share/count; refusal of that charge returns immediate 503/`Retry-After: 1`
   or queued deferral without spending an attempt.

2. **Neutral cross-product accounting ownership.** INV-001 says “`wyrd-spec`
   owns wire/record/error contracts; Vala owns Agent verification orchestration;
   the hosted server owns tool IO and Monty resources; SDKs project the result.”
   The resource introduction says “shared accounting across existing owners.”
   Approve an explicit neutral accounting boundary usable by both Vala/Bifrost
   and the server, with server-owned composition and capability-owned policy.
   Proposed home: extend `wyrd-runtime` with only the common resource ownership
   mechanism for memory/scratch, adapting existing Bifrost accounting rather
   than duplicating it
   or moving its DataFusion/Oracle policy wholesale. This is a proposed new
   shared responsibility, not an approved owner inferred from the report.
   Vala must never import server concepts; `wyrd-spec` stays IO/async-free.
   Exact types, files and synchronization remain implementation-owned.
   No blanket “no new resource root” prohibition appears in the spec. Preserve no independent/double promises and no tenant
   quotas; do not use that wording to forbid an approved shared authority.

3. **Caches and Bifrost memory borrowing.** The compatibility section says
   “No Bifrost storage, query semantics or audit changes.” The owner obligations
   say “Preserve Oracle class semantics and existing supported memory floor
   unless separately approved.” In `architecture/bifrost-design.md`, Admission
   and memory expressly charges each cache's “whole budget” for “the storage
   owner's life”; query ceilings derive from the Bifrost budget. Extend the
   spec's permitted Bifrost resource changes and amend that owner to replace
   lifetime cache-budget withholding with use-driven ownership and safe
   reclamation, including references retained after eviction. Review derived
   query/role ceilings so they do not create fixed capability shares. Preserve
   class fairness, per-operation safety limits and typed query errors; changing
   those semantics requires separate explicit approval. Do not silently erase
   the storage prohibition or call a cache reservation physically allocated RAM.

4. **Aggregate spill/scratch ownership with spilling preserved.** REQ-027 says
   “plus Bifrost's memory/scratch consumers, under this plan without double
   promises.” This establishes the aggregate invariant, but does not replace
   the owner's `spill_share = scratch_limit / 2` and per-query spill lifecycle.
   Approve the cross-product scratch boundary and reconcile the Oracle/Forge
   spill owner text: use-driven charges for actual owned/writing spill bytes
   against one filesystem budget, shared with hosted and other disposable
   scratch. Per-query limits alone cannot establish the aggregate bound.
   Retain charges through writer settlement, deletion and cleanup failure;
   preserve DataFusion spilling as normal pressure handling and its safe typed
   failure when non-spillable memory/disk is exhausted. Do not reserve a fixed
   disk fraction per idle query or turn off spill to simplify accounting.
   Retain a per-query spill cap alongside the per-filesystem aggregate ledger
   (the `max-spill-per-node` / `query-max-spill-per-node` shape), so one query
   cannot consume the whole disk budget. A cap is not a reserved disk share.

5. **Process budgets across replicas and nested progress.** REQ-027 says
   “Explicit operator ceilings may only lower discovered resources.” REQ-029
   says “Keep sufficient unpromised capacity for admitted nested work to make
   progress.” REQ-033 says “Fairness and capacity are replica-local, with no
   promise of global fairness or linear scaling.” Clarify that host RAM/free
   filesystem observations do not assign disjoint budgets to co-located
   processes: deployment must provide non-overlapping allocations/ceilings for
   shared hosts/filesystems, with safe readiness failure if a supported safe
   assignment cannot be established. No new global scheduler is proposed.
   Clarify how protected progress coexists with dynamic borrowing: no permanent
   verification share, parent resident state stays charged, nested resources
   are acquired atomically after safe reclamation or fail cooperatively within
   the original deadline rather than waiting behind their own parent. Retain
   the ordinary nested-call success proof at tool cap 1. This clarifies a
   material guarantee; it does not claim fairness alone prevents deadlock.

6. **Inherited Python diagnostic exception.** REQ-024 requires “Python/V8
   allocation sites and allocation volume versus retained memory.” R1's
   Required diagnosis instead accepts Python managed retained/peak
   evidence and disclose unavailable allocation volume. Carry that exception
   forward as a requested REQ-024/INV-008/AC-025 revision: managed Python
   retention and peak suffice, native and V8 volume/retention remain required.
   This uses the existing workflow to diagnose surviving managed allocations
   and peak demand; total managed churn and short-lived allocation volume stay
   unknown. It waives no phase/CPU or native/V8 diagnostic domain.

Baseline protections, deployment-assignment
contract and protected-progress semantics must be unambiguous before ready;
measured sizing formulas/values and private implementation choices remain
implementation-owned within those protections and need no additional approval round.

## What Changes and What Must Stay True

- Resolve a safe process resource plan before consumers: portable CPU discovery
  via `std::thread::available_parallelism`, memory/filesystem discovery and
  lowering-only operator ceilings. Fail readiness for unavailable discovery,
  overflow, impossible baselines or unsafe composition; never guess capacity.
  Discovery estimates available process resources, not instantaneous spare CPU
  or an inferred container allocation. Shared-host budgets need explicit safe
  assignment as described above.
- Dynamically account memory and scratch per underlying filesystem across
  hosted execution, Gateway/recording, edge/body/upload
  buffering and Bifrost Scribe/Oracle/Forge/storage/transport. Ownership
  transfers do not charge one allocation twice; a new encoding/copy is a new
  allocation. Waiting metadata/inputs, worker baseline/heap/stack/IPC,
  idle/starting/resetting/reaping workers, caches and transient buffers count.
  Measure untracked overhead and reclamation/progress headroom. An allocator
  envelope is not an OS-enforced RSS limit. At checkout, reserve each live
  Python session's enforceable peak envelope: allocator ceiling plus measured
  baseline, stack and IPC, retaining it until reset completes or the worker is
  reaped. Transfer existing worker charges without double charging; after a
  healthy reset the idle worker's own memory stays charged. Track descriptor
  ownership/churn
  without nonportable descriptor-limit probing.
- Only necessary server/Bifrost defaults may be hard allocations. Other grants
  follow use; unused grants return and reclaimable caches/idle workers do not
  strand capacity. Already-owned live state is not forcibly revoked or
  immediately reusable. Reclaim only through the owning capability, spill
  queries normally, and settle cancellation/cleanup before reuse. Preserve
  Bifrost class protection and Scribe/WAL/flush progress under pressure. Keep
  both per-query spill caps and the shared per-filesystem aggregate ledger.
  `monty_min_workers` only targets prewarming: charge warm workers like any
  worker and allow idle reaping under pressure.
- The OS scheduler shares CPU across capabilities. Bound only hosted Python
  runnable turns by effective CPUs; keep Oracle slots and Scribe/Forge
  CPU-derived concurrency with their current owners, without a shared CPU
  ledger. Verification execute/enqueue bypass the generic edge limit, following
  `POST /v1/query` in `http/middleware/edge_capacity.rs`, because hosted
  admission bounds them by bytes. Other routes keep the 1,024 in-flight limit
  to protect request handling from overload; their buffered bodies charge the
  shared memory ledger.
- Direct/queued hosted admission is FIFO within tenant, round robin across
  eligible tenants and work-conserving per replica. A non-fitting tenant head
  cannot block eligible others or be bypassed within its FIFO. Acquire each
  required set atomically; do not wait with speculative partial grants or the
  runnable/tool permit a child needs. Resident parent state stays charged.
  Charge waiting requests' retained bytes to the shared memory ledger, with
  no separate fixed waiting share or waiter count. If that charge cannot be
  granted, immediate execution returns the existing 503 with `Retry-After: 1`;
  queued work defers without spending an attempt.
- Reuse Monty's subprocess finish/reset lifecycle. Each operation has fresh
  Python globals, mounts, callbacks, output, authority and allocator/session
  accounting. A normal program failure permits reuse only after a proven
  healthy reset; crash/cancel/failed or uncertain reset discards the worker.
  Suspension releases only the runnable turn, retaining the peak envelope;
  reacquisition is fair.
  Keep teardown ownership until actual reaping/settlement, not pool slot release.
- One absolute arrival deadline covers waiting, lookup, staging, checkout,
  execution and resumption. Shorter operation/cumulative Agent budgets still
  win and include host waits/retries. HTTP/MCP agree; edge timeout cannot change
  a capacity wait's error. Durable deadlines, accepted authority and attempt
  accounting remain; PostgreSQL derives coordination time from bound durations.

| Situation | Caller-visible result |
| --- | --- |
| Temporary occupancy; waiting bytes fit | Fair wait, then execution within original deadline. |
| Shared memory cannot grant retained waiting bytes | `WYRD_SERVER_503_SERVICE_UNAVAILABLE`, bounded structured resource/reason and HTTP `Retry-After: 1`; queued work defers without spending an attempt. |
| Arrival deadline expires, including queue wait | `VerificationExecutionTimedOut` 504; never capacity 503 or a failed-subject verdict. |
| Recording admission full | Existing recording-pressure 429; audit staging failure logs/counts and never fails permission-authorized work. |
| Cancel/disconnect/shutdown | Remove waiters, stop new dispatch, settle mediated work and reset/discard/reap; no late result or early/double release. |

Keep per-Agent non-code tool concurrency (default 8), shared request budgets,
content/security limits, parent-mediated IO/permissions, real citations and
complete code-mode records. SDKs project existing errors/retry hints without
silent replay. Operational telemetry exposes effective plan, reasons,
active/waiting counts/bytes, queue time, runnable/suspended workers and
spawn/reset/reap counts with bounded labels, never tenant/request metric labels.
Admission wait is a named timing phase outside the historical server residual.

No new Card/API mode, worker transport, client scheduler, authorization root,
global tenant quota, PostgreSQL semaphore/coordination table/hot-path query,
database pool expansion, workload reduction, profiling framework or public SDK
benchmark API. No explicit cgroups, `/proc`, `prlimit`, namespaces, credential
switching, sandbox-exec, seccomp/Landlock or `cfg(target_os)` behavior. Remove
existing explicit cgroup discovery/pressure probes, consumers and misleading
telemetry without weakening portable ingress protection. Benchmark collectors
remain compiled out of production; ordinary operational telemetry remains.

## Approach

1. Reconcile approved revision 9's owner sections before/with governed
   behavior.
2. Extend common accounting/composition and existing capability adapters;
   establish measured baselines and dynamic grants, cache reclamation and
   aggregate spill ownership without parallel resource promises.
3. Extend hosted admission, worker reset/turn lifecycle, edge/recording charges
   and arrival-deadline/error propagation at their current owners.
4. Prove borrowing, isolation, cleanup, nested progress and public overload with
   focused TDD/journeys; reconcile bounded operational and worker measurements.
5. Calibrate and freeze acceptance inputs, then qualify the unchanged benchmark
   and run sustained/two-replica recovery experiments.
6. Inspect actual diagnostic artifacts, regenerate reports, reconcile every
   inherited ledger row, restore/check production packages and submit for
   independent task review. Do not equate planning or a VALID smoke with PASS.

## Ordered Implementation Scenarios

New executable behavior uses TDD, one scenario at a time. Run its focused
command, confirm failure for the expected missing behavior, implement the
minimum fix, rerun relevant earlier scenarios, then refactor. Names marked
planned in Verification must be confirmed after addition and before RED.

### Scenario 1 — Safe plans and borrowing across capability mixes

**Behavior.** Small/large injected resource snapshots and lowering ceilings
produce safe plans; zero/overflow/unavailable discovery and impossible baseline
plans fail readiness. Bifrost-only work can consume usable capacity without
idle hosted/Gateway shares. Mixed work shifts memory/scratch use across
capabilities; aggregate grants never exceed assigned roots. Python runnable
turns stay within effective CPUs while Bifrost concurrency remains owner-bound;
there is no cross-capability CPU charge. Same-filesystem
directories share one budget; separate filesystems do not falsely share space.

**RED.** Add [V1](#v1-common-accounting-and-capability-pressure) plan/accounting
and [V3](#v3-public-journeys) mixed-capability journey assertions. Current
independent boot composition and standing cache charges fail dynamic borrowing.

**GREEN.** Compose common ownership before consumers; route all listed
consumers through it, preserving necessary baseline/progress protections and
lowering-only ceilings. Prove actual capability use, not just arithmetic.

**REFACTOR.** Remove duplicate discovery/ledgers and unevidenced fixed shares;
leave capability policy with its owner. Do not add a general scheduler.

### Scenario 2 — Reclamation and spill never return still-owned bytes

**Behavior.** Cache growth competes with active work; eviction returns only
truly unreferenced allocations. A caller retaining cached bytes still owns a
charge. Multiple spilling queries, Forge work and hosted scratch sharing a
filesystem cannot collectively exceed its budget; a per-query spill cap also
prevents one query consuming the whole budget, without precharging idle space.
Memory pressure spills where
supported; non-spillable or full-disk cases fail cooperatively. Cancellation,
slow writes and failed deletion retain ownership and prevent unsafe reissue;
subsequent healthy work can progress without a global process crash.

**RED.** Add V1 cache/spill pressure cases and V3 real query pressure assertions.
Current lifetime cache precharge and independent per-query disk managers cannot
prove actual-use reclamation or both per-query and aggregate disk bounds.

**GREEN.** Extend cache/reference and query/Forge scratch lifecycles to charge
and release actual ownership through the common boundary. Preserve existing
spill operators/runtime, per-query caps, class semantics, cleanup and typed
resource failures.

**REFACTOR.** Remove superseded standing charges and independent promises; keep
one memory/scratch ownership trail. Do not replace the storage/query engine.

### Scenario 3 — Fair waits and nested calls make bounded progress

**Behavior.** A refilling tenant cannot bypass another eligible tenant; tenant
FIFO holds. Waiting requests charge retained bytes to shared memory without
fixed waiting shares/counts; failed charges cause immediate 503 with
`Retry-After: 1` or queued deferral without an attempt. A lone tenant borrows
idle capacity; a non-fitting head permits
other tenants to run. Cancelled waiters leave no bytes. Failed atomic admission
leaves no partial grant. A mediated query completes at runnable/tool cap 1;
under exhausted child resources it fails within the original deadline without
queuing behind its own retained parent. Suspended state remains charged.

**RED.** Add V1 fair contention and V2 suspended/nested worker assertions.
Immediate refusal and independent semaphores fail fair wait/progress assertions.

**GREEN.** Share hosted direct/queued admission and fair runnable reacquisition;
settle rejected/cancelled waits. Preserve Bifrost's distinct query-class arbiter
and the approved nested-progress contract from the spec revision.

**REFACTOR.** Remove duplicate admission/count waiters and partial-acquisition
paths, retaining byte bounds and existing accepted-authority boundaries.

### Scenario 4 — Reused workers isolate sessions through every exit

**Behavior.** The same healthy subprocess serves tenant A then B; B sees none
of A's state/mounts/callbacks/output/authority/accounting. Checkout reserves the allocator ceiling plus measured baseline, stack and IPC
until confirmed reset or reaping. A suspended session lets another run by
releasing only its runnable turn. Resume stays CPU-bounded. Prewarmed workers
remain charged; idle pressure may reap them below `monty_min_workers`.
Normal error plus healthy reset can reuse; crash, spawn failure, cancel,
failed reset and shutdown cannot expose a dirty worker or release live process
ownership early. Later authorized work succeeds.

**RED.** Add [V2](#v2-real-worker-lifecycle) real-worker identity, suspension and
failure-settlement cases. Spawn-per-checkout does not satisfy process reuse;
pool count release alone does not prove reaping.

**GREEN.** Use existing Monty finish/reset and parent lifecycle owners; remove
forced single-use recycling, separating runnable permits from resident grants.

**REFACTOR.** Consolidate teardown while keeping distinct healthy/reset/discard
outcomes. No custom reset protocol, new interpreter or OS isolation.

### Scenario 5 — Public waits, errors and recovery retain their meaning

**Behavior.** Real SDK/HTTP/MCP callers observe wait then success, refusal of
retained waiting bytes by shared memory as 503/`Retry-After: 1`, arrival expiry
504 and independent recording 429. Verification execute/enqueue bypass generic
edge concurrency like `POST /v1/query`; other routes keep the 1,024 limit and
charge buffered bodies to shared memory. Edge timeout cannot bypass the
verification deadline/error contracts. Cancellation stops
dispatch/publication and releases only settled ownership. Queued capacity
deferral preserves attempts, earlier durable deadline and accepted authority,
then recovers; no SDK replays a direct call. Caught host failure cannot become
a completed subject judgment. Permission audit remains non-blocking.

**RED.** Add V3 focused journeys through actual workers/server/Postgres. Current
deadline starts at engine execution, and generic resource 503 lacks retry
projection; assert those failures without duplicating every fault per SDK.

**GREEN.** Extend existing service/runner/edge/error owners and SDK projections,
retaining stable catalog codes and current recording/authorization boundaries.

**REFACTOR.** Consolidate error/deadline mapping; no prose parsing, client retry
wrapper, durable schema or new hot-path database statement.

### Scenario 6 — Measurements cover admission and interpreter children

**Behavior.** Wait/run/suspend/reset/reap transitions balance bounded-label
counts/bytes. Benchmark CPU/memory includes worker starts/exits within the
window, separately from parent/client/provider costs. Missing child counters
remain unavailable, never zero; historical parent-only records retain scope.
Uniformly extended windows, if needed, retain sample/integrity rules.

**RED.** Add [V4](#v4-benchmark-and-report-regressions) exiting-child/missing-counter
and persisted-report assertions. PID-only collection cannot establish complete
worker cost. Use V1/V2 transition assertions for operational telemetry.

**GREEN.** Extend existing process-window/recorder owners, preserving honest
phase populations, overlap, total-time boundaries and non-bench exclusion.

**REFACTOR.** Keep one recorder/report/accounting owner; remove stale total-CPU
claims and prohibited cgroup fields instead of preserving aliases.

## Acceptance Criteria

SR1–SR6 map the extensions
approved in revision 9 to Spec revision required above. Each row is carried, not silently deferred.

| Approved obligations | Concrete proof and owner |
| --- | --- |
| REQ-027; INV-001/010; AC-028 | Startup/common accounting, Scenarios 1–2/V1/V3: valid discovery/ceilings, impossible-plan refusal, exactly-once aggregate memory/scratch ownership including request bodies, retained inputs, recording and Bifrost. Dynamic Bifrost-only/mixed borrowing, Python-only runnable bounds, unchanged Bifrost CPU owners, neutral owner, retained cache charges, per-query plus aggregate spill bounds and assigned replica budgets require SR1–SR5 approval. |
| REQ-029; REQ-011/013; AC-007/008/030 | Hosted admission, Scenario 3/V1/V2: tenant FIFO/rotation, idle borrowing, shared-memory waiting charges without fixed waiting shares/counts, cancellation, atomic resource sets, nested cap-1 progress and shared write/scratch/no-overwrite behavior. No partial grant or deadlock; SR5 settles exhausted-child semantics. |
| REQ-028/012; INV-002/005/010; AC-009/015/029 | Worker/workspace owners, Scenario 4/V2/V3: actual process reuse, fresh sessions, checkout peak-envelope retention through confirmed reset/reap, suspension releasing only runnable turns, charged/reapable prewarming, all failure exits/reaping and subsequent success; preserve authored-code isolation and escape/security regression proof. |
| REQ-009/030; INV-004/006; AC-010/017/018/031 | Service/runner/edge/SDKs, Scenario 5/V3: shared-memory waiting-charge refusal as 503/Retry-After: 1, wait/504/429, execute/enqueue edge bypass and retained other-route 1,024 protection/body charges, no false judgment/late result/replay, accepted authority, non-blocking audit and queued attempt recovery. |
| REQ-031; INV-007/009; AC-032 | Telemetry/process-window owners, Scenario 6/V4 plus V1/V2: balanced transitions, named admission phase, parent/child/churn coverage, bounded labels, unavailable readings and historical scopes preserved. |
| REQ-019–023/032; AC-019–024/033 | Existing benchmark owners, V4/V5: all six public SDK populations, one Assert/two independent Agents/four governed exchanges/two real code operations per acknowledged verification; exact citations/data, compact/details, total call-return/result-ready boundaries. Clean qualified release evidence with zero c64 errors, every repetition's sample floor, unchanged overhead criterion and frozen measured target. |
| REQ-033; AC-034 | Harness owners, V6: sustained >=200 tenants, saturation/drain/recovery, bounded fair local progress and stable parent/worker memory/disk/descriptors; repeat on two real replicas with disjoint budgets and no global fairness/scaling claim. |
| REQ-024/026; INV-008/009; AC-025/027 | Shared recorder/profile/build/package owners, V7: actual phase/CPU/native/Python/V8 artifact findings, exact process/window/feature identities and production exclusion; SR6 resolves Python managed volume. Capture count or VALID smoke alone is insufficient. |
| REQ-025; AC-022/026; all remaining TASK-003/R1 rows | V4/V5/V7: L2 unchanged, original code-mode records/citations/permissions, compact decisions and explicit authorized details, persisted schema-3 reports and actual legacy-method regeneration. Reconcile every R1 Obligations and Implementation Evidence row, including R1-DUP and R1-DOC; independent task-review PASS required for parent closeout. |

## Implementation Reference

Inspected current source on 2026-11-14; revalidate before implementation. The
diagnostician report at
[changes/active/verifier-code-mode/design/capacity-diagnosis.md](../design/capacity-diagnosis.md)
is evidence, not authority. Repository claims relied on here were checked at
the owners below. Its external production analogies and proposed broker design
are not acceptance authority; no external research claim is needed by this task.

- **Common composition:** `crates/vala/vala-bifrost-redux/src/resources.rs`
  `ResourcePlan`, `SystemResourceSnapshot`, `BifrostRuntimeResources`,
  `BifrostResourceGovernor`, `GovernedMemoryRoot::try_grow` already provide
  discovery, joint Bifrost charges, cooperative refusal and rollback. They
  depend on DataFusion/Oracle concepts, so moving the whole module to shared
  runtime is inappropriate. `crates/shared/wyrd-runtime/src/lib.rs` has runtime
  and outbox shells but no cross-product allocator; both server and redux
  manifests already depend on it. After SR2, extend only neutral ownership
  there and reuse Bifrost adapters. This fills a real boundary gap rather than
  adding a second governor. Shared runtime must remain SQL/PyO3-free.
- **Boot/callers:** server `boot/mod.rs` builds Bifrost resources near 392,
  storage caches near 449, independently configured outbox near 923 and
  `HostedBlocks::start` near 1337. Extend this composition before consumers.
  `detect_snapshot`, `container_memory_limit`, `container_memory_used` and
  `cgroup_pressure` explicitly probe cgroups; `scribe/ingress.rs` uses
  `cgroup_tripwire_engaged`, `scribe/memory.rs` exposes/consumes the readings.
  Remove these paths together and retain portable ingress pressure proof.
- **Dynamic Bifrost:** `storage/mod.rs::with_oracle_caches` reserves entire
  footer/range budgets; `storage/policy.rs::resolve_cache_bytes` derives those
  ceilings. `storage/range_cache.rs::{get,put}` returns cloned `Bytes` and
  copies inserted ranges: eviction alone cannot prove allocation lifetime.
  Extend existing footer/range cache ownership and callers, not another cache.
  `resources.rs::try_acquire_oracle` charges slots, constructs per-query runtime
  and uses `oracle_query_spill_limit` (half scratch); `oracle/spill.rs`
  `build_query_runtime` creates separate bounded DiskManagers. Its existing
  inline tests cover isolated spill/cleanup, not aggregate shared-filesystem
  ownership. Extend these and Forge's existing spill owner/callers after
  tracing them during implementation; do not claim Forge aggregate enforcement
  was established by the diagnostician or this plan.
- **Hosted:** `verification/hosted/mod.rs::HostedLimits::{default,live_workers}`,
  `workspace.rs::Workspaces::open` and `sandbox.rs::Sandbox::{start,exec,shutdown}`
  own current fixed defaults/immediate semaphores. `start` sets
  `max_checkouts_per_worker = Some(1)`; `exec` drops checkout. Installed
  `monty-pool-1.1.0/src/checkout.rs::Checkout::finish/finish_session` uses Reset
  for subprocess reuse. Its Drop and `pool.rs::release_worker` free pool count
  while reaping may continue: extend parent ownership proof, do not equate a
  free pool slot with freed RAM. `hosted/tools.rs` mediates Bifrost query
  cancellation and retains blocking-work writer/permits through settlement.
  Keep `worker.rs` allocator/IPC isolation, extend actual worker lifecycle tests
  in server `tests/integration/sandbox_worker.rs` and workspace inline/PG tests.
- **Edge/recording/Gateway:** server `config.rs` defaults to 1,024 in-flight
  requests, 1-MiB generic bodies and 25-MiB audio uploads (content limits are
  ceilings, not eager reservations). `http/middleware/edge_capacity.rs` bypasses
  public queries; `body_limit.rs` currently acquires Scribe transport ownership
  only on Scribe routes and collects other generic bodies separately. Include
  route-specific uploads and query paths in the accounting inventory rather
  than assuming one middleware covers all bodies. `scribe_outbox.rs` aliases
  `wyrd_runtime::outbox::Outbox<ScribeSink>`; Gateway
  `components/gateway/invocation.rs::capture_slot` uses its reservation path.
  Extend those ownership transfers, including encode/decode copies, preserving
  recording backpressure and non-blocking audit. No second recorder/outbox.
- **Deadline/errors:** `components/verification/service.rs::execute/run_direct`
  accepts `RequestArrival` but passes `Instant::now() + timeout` to `engine.run`.
  `verification/runner.rs` maps Busy to Deferred. `http/error.rs::retryable_now`
  omits generic ServiceUnavailable. Extend arrival/deadline/error projection
  through edge timeout, HTTP/MCP and normal SDKs; no accepted-authority refresh
  or new direct-verification SQL. Stable wire errors stay in `wyrd-spec`.
- **Measurement/workload:** `wyrd-testing/src/capacity.rs::ResourceWindow`,
  `release_server/os.rs::{process_cpu_usec,PeakSampler}` currently measure one
  PID; extend their window/lifecycle accounting for child churn. Shared fixture,
  sample validation and declarations live in
  `src/bin/capacity/verification/{workload,run,mod}.rs`; `experiment.rs` and
  `profile.rs` own capture/report lifecycle. Existing declaration, workload,
  phase and persisted-report selectors appear in V4. Reuse SDK boundary drivers
  in Python/TS test paths and `wyrd-benchmark`'s bench-only shared recorder.
  `wyrd-testing/src/bifrost/cluster.rs::BifrostClusterSpec::two_mixed` provides
  real cluster infrastructure, not proof that hosted multi-tenant saturation
  is already implemented. Extend the existing harness for the missing workload.

Likely closure includes runtime exports, Bifrost resource/cache/spill adapters,
server config/boot/hosted/edge/outbox/Gateway/telemetry, SDK error projections,
benchmark drivers/reports and authored settings/architecture docs. Read local
SDK/docs/SQL AGENTS before editing those trees. No SQL/schema change is intended;
regenerate contracts only from owners if affected, author Python stubs if needed.
The complete workflow and consumers, not this path inventory, determine scope.

## Verification and Evidence

**Future implementation commands only; none run by this planning task.** Cargo
work runs sequentially. Packages/targets/runners were checked in manifests,
module roots, TESTING.md and `mise tasks ls --hidden`/`mise.toml`. Planned names
are not existing tests: confirm actual selectors after addition before RED;
record an equivalent focused command if approved private placement differs.
Do not accept zero selected tests. No full language journey campaign or `gate`;
integrated change review owns those once.

### V1 Common accounting and capability pressure

All selectors here are **planned**. The runtime selector is conditional on SR2
approval. Use injected snapshots and deterministic ownership/coordination, not
synthetic host load. Exercise Scenarios 1–3, including retained cache references,
multiple spillers hitting per-query and aggregate filesystem caps, rollback,
cleanup failures and bounded transition telemetry. Assert shared-memory waiting
charges/refusal with no fixed waiting share/count, Python-only runnable bounds
and preserved Bifrost CPU concurrency owners.

```bash
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=resources::tests::process_grants_follow_use_without_double_promises)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=resources::tests::cache_and_spill_ownership_share_process_capacity)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tests::admission_rotates_tenants_and_accounts_waiting_bytes)'
```

### V2 Real-worker lifecycle

Planned identity/isolation, suspend/nested-progress and failure-settlement cases
extend the existing integration module; its Cargo-provided subprocess executable
is the worker, not an inferred target path. Assert checkout peak-envelope
retention through suspension/reset/reap and charged prewarming that may shrink
below `monty_min_workers` under idle pressure. Also run the existing owning lane
for registered Artifact/workspace/isolation regressions.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=sandbox_worker::sandbox_reuses_processes_with_fresh_tenant_sessions)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=sandbox_worker::suspended_sessions_release_runnable_capacity)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=sandbox_worker::failed_workers_settle_before_capacity_returns)'
mise run test:verification:sandbox
```

### V3 Public journeys

All capacity selectors below are **planned** in existing modules/targets. Real
SDK → WyrdTestServer + repository Postgres → SDK and actual Monty are required.
The mixed-capability server journey drives Bifrost alone, then overlapping
Gateway/verification/Bifrost under constrained grants with real query spilling.
The Rust SDK journey covers the complete wait/error/cancel/queue/authority
matrix; other surfaces prove exposed wait/error/cancellation/result boundaries.
Assert HTTP retry headers at the existing transport seam if hidden by an SDK.
Hold the generic edge full: execute/enqueue and public queries still reach
owner admission, while other routes retain the 1,024 in-flight protection and
shared buffered-body charges. Refuse retained waiting-byte grants to prove
immediate 503/`Retry-After: 1` and queued deferral without spending an attempt.

```bash
mise run build:server
mise run py:setup
mise run ts:build
mise run ts:build:testing
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::capability_mixes_borrow_and_return_process_resources)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verifier_building_blocks::verification_waits_and_recovers_under_capacity_pressure)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run --no-sync python -m pytest -q tests/integration/test_verifier_building_blocks.py::test_verification_waits_and_recovers_under_capacity_pressure'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t "^verification waits and recovers under capacity pressure$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::verification_waits_and_recovers_under_capacity_pressure)"'
```

### V4 Benchmark and report regressions

First selector is **planned**; remaining selectors exist. Preserve one fixture,
correct counts, whole-call timing, phase completeness, L2 declarations and
persisted diagnostics after raw artifacts disappear. Missing phase evidence
must not become an inapplicable/zero reading.

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::run::tests::worker_process_resources_preserve_measurement_scope)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::tests::level_one_declares_direct_sweep_and_strict_objective) | test(=verification::tests::level_two_declares_phases_and_records_rate_overrides) | test(=verification::run::tests::canonical_workload_requires_all_three_checks) | test(=verification::run::tests::canonical_client_latency_includes_all_work) | test(=verification::run::tests::language_drivers_preserve_public_boundaries_and_counts) | test(=verification::run::tests::language_phase_breakdown_requires_complete_evidence)'
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::reports_keep_repetition_diagnostics_after_artifacts_are_removed) | test(=experiment::tests::reports_regenerate_from_persisted_records)'
```

### V5 Calibration, six-population qualification and legacy reports (R1 V3)

This is measurement/regression proof, not manufactured TDD. Measure server plus
all worker CPU seconds per completed canonical verification per SDK, with
separate client/controller/provider scopes, churn coverage, queue wait and
resource headroom. Record the separate calibration's exact invocation before
execution; extend the existing harness if needed, no assumed calibration flag.
Freeze inputs and `effective_cores / CPU_seconds_per_verification` before final
qualification. Own observed utilization cannot set its target. Derive mean
latency expectations from throughput and 64 closed-loop callers and tails from
measured service/queue distributions; no new arbitrary client p99 SLA, guessed
2,000/3,200 rate, 50/100-ms target or efficiency fraction.

At c64 require CPU approaching effective cores, **or** no capacity waiting
because 64 callers bound load, **or** a named saturated non-CPU resource backed
by measurements. Idle CPU plus unexplained capacity waiting fails. Missing
worker CPU prevents a measured target claim. Retain the derivation and misses.

```bash
mise run bench:l1:verification
mise run bench:results
mise exec -- target/profiling/release/capacity report benchmarks/history/verification/direct-verify/20261114T130251Z-4bc727c82.json
```

Final qualification is full, clean-source, quiet-host and unprofiled release
bench mode across Rust/Python/TS c1/c64 with common settings, repetitions and
integrity rules. Record loaded native/runtime/build identities and measured
instrumentation mode; bench artifacts are local, not production packages.
Require zero c64 errors in every SDK/repetition, >=1,000 p99 samples in every
population/repetition including TS c1 and unchanged server-overhead p99 <0.5 ms.
If necessary extend windows uniformly, recording method/runtime-budget changes;
never omit repetitions, fabricate samples or rerun for luck. Separate
VALID/INVALID integrity from met/missed/unsettled criteria. Compare with the
newest compatible same-host experiment record, preserving failed records.

Publish throughput, errors/late outcomes, exact exchange/code reconciliation,
call-return/result-ready and server p50/p95/p99, phase populations and measured
resource scopes. Details reads stay outside verify timing; no phase/percentile
subtraction from totals. Render the new actual record and an actual
pre-extension legacy-method record with `capacity report` using their real
paths; retain both invocations/output. The failed 20261114 record above is not
the older legacy proof and must remain INVALID. No full L2 rerun is required.
Source cleanliness needing Git action is caller-owned; no commit/push/reset is
authorized. A dirty/smoke/profile/subset run cannot become qualification.

### V6 Sustained saturation/recovery and two replicas

Extend existing experiment/harness only for missing hosted workloads. Record
exact invocations before running; no existing flag is assumed. These are opt-in
experiments, not new default CI/performance gates. Use authenticated tenants,
the same canonical verification/governed mock provider, direct plus queued work,
>=200 active tenants, a busy tenant continuously refilling, mixed resource
demand and suspended code. Exercise hundreds of edge arrivals without generic
edge shedding preempting hosted admission. Include Bifrost-only and changing
mixed-capability pressure without changing the canonical L1 fixture.

Record offered/accepted/completed/refused/cancelled counts, per-tenant eligible
progress, bounded queue/grant/worker time series, memory/disk/descriptors for
parent and children, baseline, sustained saturation, load removal and drain.
Choose actual durations covering repeated turnover/reset/reuse and enough
post-warmup observation to distinguish plateau from accumulating ownership;
state the rationale. Scratch/descriptors and live reservations/queues return to
valid idle baseline; memory plateaus within plan with runtime retention
measured. An increasing trend is not excused by staying below a limit.

Repeat on two real server processes sharing repository Postgres/storage. Record
load routing, per-replica plans and baseline protections, non-overlapping host
and filesystem budgets when co-located, and results on each. Saturate one while
the other has headroom; prove independent fair progress and recovery without
database coordination. Do not let both claim the same host RAM/free disk or
claim global fairness, quotas or linear speedup. Expected overload 503/429 must
reconcile separately from L1's zero-error rule. Unmeasured children, starvation,
leaks or an undrained backlog fail this proof.

### V7 Diagnostic analysis, inherited ledger and production isolation

These are artifact/static/regression proofs; do not repeat completed RED work
or introduce code churn when existing behavior is correct. Planning confirmed
the failed clean record and R1 smoke/log exist; it did not certify their profile
coverage. Preserve the failed clean record's historical refusals, overhead and
sample misses. Inspect the saved R1 record
`target/benchmarks/smoke/verification/direct-verify/20261114T130923Z-e3759ee3e.json`,
adjacent `20261114T130923Z/profiles/` artifacts and `target/r1-logs/v4b.log`.
Its VALID verdict/66-capture inventory is not artifact-level AC-025 acceptance.
Preserve the Node V8 log-location correction documented in R1 history.

Write findings from actual SDK conversion/transport/server hot paths and waits:
phase latency and populations, CPU, native allocation volume/retention, V8
volume/retention, and Python managed allocation sites/retention/peak. SR6
proposes removing only managed Python volume; until revision 9 is explicitly
approved, revision 8 still requires it. Attribute exact tool/version/build, process and invocation/window;
distinguish Rust SDK from controller/mock-provider costs, inclusive sandbox
waits from Python CPU and shared runtime/GC from exclusive work. RSS alone,
synthetic probes, opaque native calls, capture filenames or server-only evidence
cannot establish complete client diagnosis. No second collector or profiling
validation framework. Old captures may explain old runtime, not certify changed
reset/suspend behavior. If captures are unusable or changed seams need fresh
evidence, use the existing targeted diagnostic and record actual artifacts:

```bash
mise run bench:l1:verification -- --smoke --profile --allow-dirty
```

Reconcile **every** row in R1's Obligations and Implementation Evidence, and
TASK-003's inherited proof, with concrete paths/results PASS/FAIL/UNVERIFIED.
Mark unchanged carried proof separately from changed seams rerun. Include live
phase populations/both Agents, full V3 qualification, actual legacy re-render,
V4 findings, V5 package exclusion, R1-DUP removal/reuse and R1-DOC README/help.
No pending/FAIL/UNVERIFIED required row may be hidden by a parent/task PASS.

R1 [V1–V2 and V5](TASK-003-R1-simplify-benchmark-and-closeout.md#verification-and-evidence)
remain the authoritative exact command inventory for inherited disabled/enabled
envelope, SDK boundary, isolated non-bench library/feature-graph and production
wheel/npm checks; execute affected proofs when owner paths/features change and
record commands/results by that reference without copying divergent commands.
V4 above lists overlapping workload/report commands once here. Inspect actual
non-bench server binary allocator/control exclusions as well as library/graphs;
test/dev feature unification is not production proof. Restore normal SDKs after
bench runs with V3's setup/build commands, then run R1's isolated archive import/
contents checks. No local bench wheel/addon may leak into published packages.
Bench-enabled counterparts must still expose shared diagnostic observations.

Build the normal server binary separately from bench/test feature unions for
the binary allocator/control inspection; record its exact artifact identity:

```bash
mise exec -- cargo build --locked -p wyrd-server --bin wyrd-server --release
```

Broader changed-owner checks (conditional language/docs checks only if touched):

```bash
mise run fmt:check
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run codegen:check
mise run py:format:check
mise run py:lints
mise run py:typecheck
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
mise run docs:check
mise exec -- git diff --check
```

Run additional touched Bifrost/runtime owners' narrow module regressions,
including cooperative pressure, ingress, scratch cleanup and protected progress;
record exact selected commands, retaining security/recording/isolation tests.
Do not widen into a full journey campaign. Review every added/moved hot-path
wait on PostgreSQL, Scribe, object storage or Vault against sanctioned waits;
do not add warm-path catalog/verification queries or block async hot paths.

## Completion Evidence

Deliver approved revised authority/task binding; source-backed final ownership
map and justified baseline measurements; scenario RED/GREEN/refactor records;
focused and broader commands/results; bounded telemetry/accounting and process
resource observations; frozen calibration/target derivation and clean full
qualification record/comparison; actual new/legacy report renders; sustained
and two-replica records with durations/progress/drain; artifact-level diagnostic
findings and production-isolation evidence; complete inherited ledger.

Independent task-review PASS is required before claiming TASK-003/R1 closeout.
Preserve failures and gaps honestly. Change completion remains integrated
change review's job; this task grants no commit, push, release or final PASS.

## Material Stop Conditions

- Unsafe baselines, unassigned overlapping replica budgets, untracked mandatory
  allocations, retained-cache reference lifetimes, cleanup/reaping uncertainty
  or nested progress that cannot be proven require the precise missing
  authority/proof, not guessed shares, early release or platform exceptions.
- A new dependency/feature, public API, durable schema, authorization change,
  Bifrost class/queue/error policy or database coordination beyond approved
  scope returns for narrow authority. Private helpers/placement are not blockers.
- Missing worker CPU/mandatory profile domain means incomplete evidence, never
  zero cost or a met target. Preserve otherwise valid totals separately.
- Qualification/overhead/sample/integrity misses, leaks or starvation remain
  failures to correct within scope. If redesign is necessary, report failure,
  cause and options before another approach; do not chain redesigns or weaken
  tests/gates. No constant raising or workload reduction to qualify.
- Clean-source/quiet-host prerequisite failure leaves qualification pending;
  finish unaffected work. No Git action, synthetic host load, repeated host
  waiting/rerun loop or promotion of diagnostic results is authorized.

## Authority Links

- [Approved spec revision 9](../spec.md#resource-backed-capacity);
  [design evidence](../design/notes.md).
- [TASK-003](TASK-003-canonical-sync-verification-benchmark.md) and
  [R1 evidence/commands](TASK-003-R1-simplify-benchmark-and-closeout.md).
- [Root rules](../../../../AGENTS.md), [workflow](../../../spec-driven-development.md),
  [execution](../../../implementation-execution.md), [testing](../../../../TESTING.md),
  [benchmark method](../../../../benchmarks/README.md).
- [Verifier owner](../../../../architecture/wyrd-design.md#agents-tools-and-workspaces),
  [Bifrost resource owner](../../../../architecture/bifrost-design.md#admission-and-memory),
  including Scribe Shards and hierarchical ownership / Resource and failure
  invariants and Oracle/Forge spill sections, to be reconciled under approval.

## Obligations

| ID | Promise (contract source) | Planned proof |
| --- | --- | --- |
| R3-PLAN | One process plan before consumers: `available_parallelism` CPU, portable memory/filesystem discovery, lowering-only ceilings; readiness fails on zero/overflow/unavailable/impossible baselines; no cgroup probes or consumers (REQ-027, AC-028, SR1, SR5) | V1 runtime/redux plan tests; boot refusal test; source inspection of removed cgroup paths |
| R3-MEM | One process memory ledger shared by hosted (workspaces, retained inputs, waiting bytes, workers), edge/body/upload buffers, Gateway/recording outbox and Bifrost Scribe/Oracle/Forge/transport; transfers keep one charge; Bifrost-only borrows all usable memory; mixed load shifts; no fixed capability shares beyond measured baselines (REQ-027, INV-001/010, AC-028, SR1–SR2) | V1 `process_grants_follow_use_without_double_promises`, redux governor composition test, V3 mixed-capability journey |
| R3-CACHE | Footer/range caches charge actual retained allocations; eviction returns bytes only after the last reference releases (REQ-027, SR3) | V1 `cache_and_spill_ownership_share_process_capacity` |
| R3-SPILL | Oracle/Forge spill and hosted scratch charge one aggregate ledger per filesystem, with per-query spill caps retained; charges kept through writer settlement/deletion/cleanup failure; normal spilling and typed exhaustion preserved (REQ-027, SR4) | V1 redux spill tests; V3 real query spilling under constrained grants |
| R3-REPLICA | Co-located processes require non-overlapping assigned budgets; readiness fails when an unsafe assignment is detected (REQ-027, REQ-033, SR5) | V1 plan test; V6 two-replica record |
| R3-CPU | Only hosted Python runnable turns are bounded by effective CPUs; Oracle slots and Scribe/Forge CPU concurrency unchanged; no cross-capability CPU ledger (REQ-027, SR1) | V1/V2 runnable-bound assertions; source inspection |
| R3-ADMIT | Hosted direct/queued admission: tenant FIFO, round robin, work conserving; waiting retained bytes charge shared memory with no waiter count/share; refused charge → 503 `Retry-After: 1` or queued deferral without spending an attempt; atomic sets, no partial grant; cancelled waiters leave no bytes; nested calls succeed at cap 1, exhausted child fails within the original deadline (REQ-029, REQ-011/013, AC-007/008/030, SR5) | V1 `admission_rotates_tenants_and_accounts_waiting_bytes`; V2 nested/suspend cases |
| R3-WORKER | Healthy workers reused with fresh sessions; checkout charges allocator ceiling + measured baseline/stack/IPC until confirmed reset or reap; suspension releases only the runnable turn; prewarmed workers charged and reapable below `monty_min_workers`; crash/cancel/failed reset discard without early release; later work succeeds (REQ-028/012, INV-002/005, AC-009/015/029) | V2 three real-worker tests + `test:verification:sandbox` |
| R3-DEADLINE | One absolute arrival deadline across wait/lookup/staging/checkout/execution/resume; expiry → 504 `VerificationExecutionTimedOut`; edge timeout cannot change it; HTTP/MCP/SDK agree; queued deferral keeps attempts, durable deadline, accepted authority; no replay; recording 429 and non-blocking audit unchanged (REQ-009/030, INV-004/006, AC-010/017/018/031) | V3 Rust/Python/TS/MCP journeys |
| R3-EDGE | Verification execute/enqueue bypass generic edge concurrency like `POST /v1/query`; other routes keep 1,024 in-flight and charge buffered bodies to shared memory (REQ-027/030, AC-031) | V3 server journey + edge middleware unit test |
| R3-TEL | Effective plan, bounded-label reasons, active/waiting counts/bytes, queue wait (named timing phase), runnable/suspended workers, spawn/reset/reap counts; no tenant/request labels (REQ-031, INV-007/009, AC-032) | V1/V2 transition assertions; V4 phase test |
| R3-BENCH | Benchmark CPU/memory includes worker children started/exited in the window, separately from parent/client/provider; missing counters unavailable, not zero; historical parent-only scope preserved (REQ-031, AC-032) | V4 `worker_process_resources_preserve_measurement_scope` + existing V4 selectors |
| R3-QUAL | Frozen calibration and c64 target derivation; clean six-population qualification with zero c64 errors, ≥1,000 p99 samples each, server overhead p99 < 0.5 ms; comparison and new + legacy `capacity report` renders (REQ-019–023/032, AC-019–024/033) | V5 records |
| R3-SUSTAIN | Sustained ≥200-tenant saturation/drain/recovery and two-replica experiment with disjoint budgets, stable parent+worker memory/disk/descriptors (REQ-033, AC-034) | V6 records |
| R3-DIAG | Artifact-level phase/CPU/native/V8/Python managed retention+peak findings with identities; production exclusion of bench instrumentation (REQ-024/026, INV-008/009, AC-025/027, SR6) | V7 findings + package/binary checks |
| R3-LEDGER | Every R1 Obligations/Implementation Evidence row and TASK-003 inherited proof reconciled PASS/FAIL/UNVERIFIED (REQ-025, AC-022/026) | V7 reconciliation table |
| R3-DOCS | Owner sections reconciled: wyrd-design Verifier → Agents, tools, and workspaces; bifrost-design Admission and memory, Scribe shards/resource invariants, Oracle/Forge spill (Architecture owner obligations) | Content review of diff |

## Implementation Notes

- Process exception (R3-MEM/R3-SPILL): `wyrd_runtime::resources` and the Oracle charged spill factory were written with their tests in one step; RED was only the missing-type compile failure. The behavioral assertions are `resources::tests::process_grants_follow_use_without_double_promises` and `oracle::spill::tests::spillers_share_the_process_scratch_ledger`.
- Deviation (R3-PLAN): plan discovery stays at the Bifrost owner (`detect_snapshot`), which builds the neutral `ProcessResources`; the server takes it through `BifrostRuntimeResources::process()`.
- Diagnosis, `resources::tests::query_spill_directory_ends_with_its_envelope_not_its_runtime`:
  - **Symptom:** no live spill directory after the first spill (`[]`).
  - **Evidence:** `OracleExecution::remove_spill_directories` and the test read `runtime.disk_manager.temp_dir_paths()`. With `DiskManagerMode::Custom`, that manager only delegates to the factory and owns no directories.
  - **Cause:** the directories moved to the per-query manager inside `ChargedSpillFactory`.
  - **Fix site:** `build_query_runtime` returns the query disk manager, and `OracleExecution` keeps it as `spill_disk`. Its only other caller, the follower test fixture, has spill disabled.
- R3-CACHE RED: `resources::tests::cache_and_spill_ownership_share_process_capacity` failed before the change with `empty caches withhold no process memory` (left 214748364, right 0), the standing whole-budget charge. Footer cache sweep cost measured in a release build with 100,000 cached footers: 723 ns per put, 238 ns per get. The sweep is amortized and lookups are lock-free.
- Diagnosis, `footer_cache_charges_held_parquet_footers_until_released`: **Symptom:** 1,496 bytes still charged after the last holder dropped. **Evidence:** `mini_moka` `invalidate` queues the value in its pending write buffer. **Cause:** the sweep ran before pending evictions were applied, so the cache still held the handle. **Fix site:** `FooterCache::sweep` calls `ConcurrentCacheExt::sync` before scanning.
- Design finding (R3-WORKER), `sandbox_worker::sandbox_reuses_processes_with_fresh_tenant_sessions`: after a session allocated a 200 MiB string, the reset idle worker's RSS stayed at 288,071,680 bytes for 5 s, against a 33,554,432-byte resident charge. mimalloc keeps freed pages, so a reset returns no memory. The resolution, reported to the user before redesigning, is that a worker keeps its whole session envelope charged until `reap` finds its process gone. The next session's lease replaces that charge instead of adding to it. The test now asserts the idle charge is exactly one envelope, that it never grows across sessions, and that RSS ≤ charge. It also caught a test defect: Monty has no `globals()`, so the probe now uses try/except `NameError`.
- Diagnosis, `sandbox_worker::sandbox_waits_for_process_memory`:
  - **Symptom:** of two concurrent operations in memory for one envelope + 4 KiB, the second is Busy "process memory did not free before the deadline".
  - **Evidence:** `Sandbox::exec` reaps idle workers only once, before `admission.acquire`. The finished worker then sits idle holding its envelope, and nothing reaps it while the waiter waits. No trace events fire on this path.
  - **Cause:** idle workers were never reaped while an admission waiter was blocked.
  - **Fix site:** `sandbox::reap`, the existing sweeper, also closes idle workers whenever `HostedAdmission::waiting` is non-zero. The test runs the refusal cases first, before any worker exists.
- Diagnosis, `pg_hosted_artifacts::capacity_refusal_is_busy_at_every_hosted_consumer`:
  - **Symptom:** "capacity returns once released" was Busy at the deadline, with no worker charged.
  - **Evidence:** `HostedAdmission::acquire` refused only `bytes > limit`, but the grant is `held + bytes`. Memory was exactly one envelope.
  - **Cause:** a request that can never fit waited out its deadline.
  - **Fix site:** `HostedAdmission::acquire` refuses `bytes + held > limit`. `Sandbox::start` requires memory strictly above one envelope. Callers checked: `Sandbox::exec` is the only caller of `acquire`.
  - **RED:** `admission::tests::a_lone_request_borrows_the_idle_ledger` timed out at 1 s with the old check.
  - **Test changes:** the test now occupies memory with `occupy_for_test`. Its two-program sibling case had relied on the removed one-worker cap; it now leaves 4 KiB free beside one idle worker, so the 8 KiB sibling source is refused at `hold` while the catching program runs.
- Diagnosis, `pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees`:
  - **Symptom:** the adder was never deferred.
  - **Evidence:** repeated `Scribe refused a frame ... reason="saturated"`.
  - **Cause:** `occupy_for_test(0)` starved the shared ledger Scribe uses, so the runner never reached Python.
  - **Fix site:** the test leaves 64 MiB, which it asserts is less than one envelope. The operation waits out its 2 s code timeout and defers.
- Fresh read-only diagnostician (given the command, the failures and the diff): it confirmed all four causes and fix sites. It found that `checkout.finish()` returns the worker to the pool before the lease records its envelope, so a concurrent checkout could double-charge it; the lease now drops before `finish`. It also found that reuse needs two envelopes free, which is now documented with a `ponytail:` comment. Its other residuals: reap thrash, which is already noted; zombie/PID reuse, judged low; and the spawn charge dropping during the checkout-timeout kill, which is bounded by the drop-kill.
- Process exception (R3-WORKER): the retention redesign changed production before the updated reuse assertion failed against it. The RED was the original failure above.
- `WORKER_BASELINE_BYTES` (16 MiB): an idle `wyrd-server subprocess` worker (debug build) measured 1,884 KiB RSS with `ps -o rss=`.
- Design correction (R3-MEM, workspace scratch): reviewing with the user showed that charging each workspace's granted `/work` room reserved up to `work_write_max` for workspaces that never write. `Workspace::admit_write`, called under the writer lock before every `/work` write (Python `OsCall` and `fs.extract`), now sets the charge to inputs + held + that write. Past the `/work` limit it refuses as `WriteRefusal::Limit`; a full process ledger refuses as `WriteRefusal::Scratch`. `work_room` is a pure limit again and serves as the mount's `write_bytes_limit`. Proof: `workspace::tests::admitted_writes_charge_process_scratch`. Process exception: the test was written against the new API, so its RED was the missing-method compile failure.
- Design correction (R3-MEM/R3-ADMIT, evidence and scratch refusals): every evidence read passes through `TenantEvidence::stored`, which read the whole object before any charge. Workspace staging, Artifact code sources, citations, `extract` and native media were all uncharged until then. `stored` now charges the declared size to process memory before the fetch, returns `Loaded { bytes, charge }`, and refuses Busy when the charge does not fit. `fs.extract` charges its document and serialized output to memory through `Workspace::charge_memory`. A process-scratch refusal of a `/work` write (`WriteRefusal::Scratch`) now stops the operation Busy, from Python and from `fs.extract`, instead of raising `OSError` or `work_budget`, so process load never reaches the program or the model. RED: `sandbox_worker::python_writes_beyond_process_scratch_stop_busy` returned `'caught'` (Python caught the refusal as `OSError`) before the change. Process exception: `pg_hosted_artifacts::evidence_reads_charge_process_memory_before_reading` and `workspace::tests::extraction_charges_process_memory_and_scratch` were written after their production change. Known limit, marked `ponytail:`: native media's charge ends when the `MediaRef` is built, while the provider request still holds the encoded bytes.
- R3-DEADLINE (server part): `VerificationService::execute` derives one deadline, `verify_timeout` after the request's `RequestArrival` (now, for MCP). It bounds target lookup (`timeout_at`) and the engine. A capacity Busy that ends at or after that deadline maps to 504 `VerificationExecutionTimedOut`, not 503. RED: `pg_verification_routes::direct_execution_waits_end_at_the_arrival_deadline` returned 503 `process memory did not free before the deadline`; GREEN after. `http/error.rs::retryable_now` now covers generic `ServiceUnavailable` (`Retry-After: 1`). RED: `http::error::error_mapper_tests::ingest_busy_http_retry_metadata` failed without the arm. Still open: the SDK, MCP and TypeScript projections, and the V3 journeys.
- R3-EDGE (edge part): `edge_timeout::owns_admission` routes `POST /v1/query` and `/v1/verification/{execute,enqueue}` around `EdgeCapacity`; evidence uploads keep the limit. RED: `edge_capacity::tests::owner_admitted_routes_bypass_the_full_concurrency_limit` shed execute before the change. Still open: buffered body charges in `body_limit.rs`.
- Design correction (R3-MEM, workspace staging, approved by the maintainer): a workspace no longer reads evidence whole into process memory before writing it. `Workspaces::open` creates the workspace, charges scratch for each input's declared size, and `TenantEvidence::stage` streams the object (one GET through `get_object_stream`) into an unmounted `staging/` file, charging memory per chunk, hashing and counting as it passes, then verifying length, SHA-256, and, for uploaded evidence, the format sniff from the file. Once every input is staged, a blocking pass renames each into `/inputs`; a File archive unpacks from its staged file, each entry streamed to disk with scratch charged per piece, so the compressed and unpacked bytes share scratch until the staged archive is removed and its charge returned. `zip_guard` and `sniffs_as` read any `Read + Seek` source; text sniffing validates UTF-8 in 64 KiB pieces. Hashing stays: File completion and Artifact activation do not re-hash stored bodies and stored objects can be replaced. Rejected: links or direct mounts (expose mutable storage), a shared digest cache (no measured repeat), symlinks (break the mount boundary). Native media, the `extract` action, citations, and Artifact code stay buffered with charge-before-read. Known limit, marked `ponytail:`: a backend chunk is allocated before it is charged. RED: `pg_hosted_artifacts::workspaces_stream_evidence_to_disk` failed with `Busy { reason: "process memory cannot hold the evidence read now" }` before the change; it now stages a File four times process memory and refuses a tampered body with no file or charge left. Unit proof: `workspace::tests::archives_unpack_beside_their_staged_file`, `evidence::tests::text_sniffing_reads_utf8_in_pieces` (process exception: written after the change).
- Diagnosis (`workspace::pg_tests::workspace_inputs_beyond_process_memory_are_busy`, "admission never waits"). Symptom: the open's first poll was pending. Evidence: `Workspaces::open` awaited `spawn_blocking(Workspace::create)` before the memory charge. Cause: the new blocking hop put an await ahead of the non-waiting refusal. Fix site: `Workspaces::open` creates the directory inline (a few `mkdir`s), so the refusal precedes any await; no other caller creates workspaces.
- Wyrd side of the Forge spill factory: the fork change is commit `21ceaf7` on branch `wyrd/caller-spill-factory` (`SpillLease::with_temp_file_factory`); the pin bump and Forge's capped factory remain open.
- R3-TEL/R3-BENCH profiling (maintainer requirement: hot-path code must be profileable and attributable to a phase): workspace staging runs inside the inclusive `engine` phase metric, so it carries `debug` spans in the existing in-process span pattern: `verification.workspace.open` (`inputs`), a per-file `verification.workspace.stage` (`bytes`, `chunks`), and `verification.workspace.install` (`inputs`, `bytes`) for the blocking placement pass. The design's span catalog in `architecture/wyrd-design.md` lists them. No new phase metric was added: the benchmark suite's phase list is a wire contract. Proof: `workspace::pg_tests::workspace_open_is_profiled_as_its_own_spans` (process exception: written after the instrumentation).
- R3-SPILL (Forge spill charged to the shared process scratch ledger): the fork pin moves to `21ceaf7` on `wyrd-production`, which adds `SpillLease::with_temp_file_factory`. Forge's `governed_context_for` installs `oracle::spill::charged_spill_factory` over its leased spill root with DataFusion's default per-attempt cap, so Oracle and Forge spill draw on one `ByteLedger` (`ForgeResources::scratch`). Diagnosis of the follow-on failure:
  - **Symptom:** `forge::managed::executor::tests::rewrite_pool_charges_root_and_releases_on_cancel` failed with "spills land beneath the leased forge-spill root, saw []".
  - **Evidence:** `DiskManagerBuilder::with_temp_file_factory` puts the outer manager in Custom mode; `temp_dir_paths()` reports only Directories-mode paths, which now belong to the factory's inner manager.
  - **Cause:** the assertion observed a DataFusion reporting detail, not placement. The file-path assertion and the empty-root check still prove placement and cleanup.
  - **Fix site:** the test only (fresh diagnostician confirmed; no other caller reads the outer manager's `temp_dir_paths`; `resources.rs` Oracle test uses the inner manager). The `temp_dir_paths` assertion was replaced with an assertion that a 4096-byte spill write charges `forge.scratch()` and that dropping the context returns it to 0.
  - Proof: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(forge::managed::executor::tests::) | test(oracle::spill::tests::)'` 5/5 pass; fmt and lints clean.
- R3-EDGE (buffered body part): `http/middleware/body_limit.rs` charges every buffered body before it is buffered through one `BodyCharge`: Scribe OTLP routes keep Bifrost transport admission (which draws on the process memory ledger through the shared Bifrost cap), every other buffered route charges the process memory ledger directly (`BifrostTransportAdmission::process_memory`, the same `ProcessResources` root hosted verification charges). A declared length is charged up front, undeclared or longer bodies frame by frame; a refusal is the existing retryable `503 WYRD_SERVER_503_SERVICE_UNAVAILABLE` with `Retry-After: 1`, and the charge returns when the handler does. A single-frame body is passed on without a copy. Owner-bounded routes (verification execute/enqueue/evidence, audio uploads, local uploads) still pass unbuffered.
  - RED: `http::middleware::body_limit::tests::generic_bodies_charge_process_memory` observed `[0, 0]` charged bytes in the handler, expected `[40, 40]`, before the charge existed (parameter plumbed, not used).
  - Journey: `pg_router_smoke::generic_body_beyond_process_memory_returns_503` occupies the live server's process memory, sees a sized and a streamed generic body refused 503 with `Retry-After: 1` before auth, then the same request reach the auth layer once memory frees (process exception: written after the implementation; its first run asserted `401` for the fixture's malformed bearer token, which the auth layer answers `400 WYRD_AUTH_400_BAD_TOKEN_FORMAT`; the expectation in the new test was corrected).
  - Proof: `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(http::)'` 36/36; `... --test integration -E 'test(pg_router_smoke::)'` 35/35; fmt and lints clean.
- R3-DEADLINE (Rust SDK journeys, process exception: written after the server behaviour, so no RED beyond the server tier's): `sdks/wyrd-sdk-rust/tests/integration/verification_capacity.rs` over `fixtures/cards/verification_capacity/` drives the public `observe().verify` and submit paths against `WyrdTestServer` with a 2 s verify timeout (`with_verify_timeout_for_test`). `verify_waits_for_hosted_memory_then_passes` waits under a held envelope and passes once it frees; `submit_defers_under_hosted_memory_then_passes` defers a queued execution and passes on release; `verify_refusals_follow_the_arrival_deadline` proves the immediate 503 `WYRD_SERVER_503_SERVICE_UNAVAILABLE` with the bounded `reason`, the 504 `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` within [timeout, 2 x timeout) with no waiter left, a cancelled waiter leaving the queue, and a passing verify after release. The harness gained `occupy_hosted_memory`/`hosted_waiters` (Rust, Python).
  - Diagnosis 1 (refusal returned 504, not 503). **Symptom:** the full-memory verify timed out. **Evidence:** the trace showed the request waiting in `HostedAdmission::acquire` until the deadline; `occupy_for_test` charged `available()` once. **Cause:** releases by Scribe and the request body after the snapshot freed memory, so `hold` was granted and the envelope waited; the server was correct. **Fix site (fresh diagnostician):** test support `Sandbox::occupy_for_test`, now an `OccupiedForTest` hold that absorbs releases until dropped; callers `pg_verification_routes`, `sandbox_worker`, `pg_hosted_artifacts` re-run green.
  - Diagnosis 2 (`details.retry_after_seconds` was Null). **Symptom:** verification_capacity.rs refusal assertion. **Evidence:** `components/verification/service.rs` builds `ServiceUnavailable` with `details: {"reason"}`; `http/error.rs::wyrd_error_response_from_parts` renders the hint as the `Retry-After: 1` header via `retryable_now`, pinned by `ingest_busy_http_retry_metadata`. **Cause:** the assertion asked for a detail the contract (spec AC-031, line 219 here) never promises; only the 429 staging refusal carries it. **Fix site (fresh diagnostician):** the journey assertion, now code plus `details.reason`; the Rust client does not project response headers, so the header stays proven at the HTTP tier.
  - Proof: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner >/dev/null && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(verification_capacity::)'"` 3/3; `... -p wyrd-server --test integration --run-ignored=all -E 'test(=pg_verification_routes::direct_execution_waits_end_at_the_arrival_deadline) | test(sandbox_worker::) | test(pg_hosted_artifacts::)'` 30/30; fmt and lints clean. Still open after the Python journey below for R3-DEADLINE: TypeScript and MCP journeys.
- R3-DEADLINE (Python SDK journeys, same process exception): `sdks/wyrd-sdk-python/tests/integration/test_verification_capacity.py` drives `observe.verify`/`observe.submit` against `WyrdTestServer(verification_runtime=True, verify_timeout_seconds=2)` with the public `occupy_hosted_memory` (a `HeldMemory` context manager) and `hosted_waiters` controls, over the shared `fixtures/cards/verification_capacity/`. It proves the wait then pass, the queued deferral then pass, the immediate 503 with its bounded `reason`, and the 504 within [timeout, 2 x timeout) with no waiter left, then a pass after release. Python verify is a blocking call, so caller cancellation stays proven in the Rust SDK journey.
  - Proof: `mise run py:setup`, then `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner >/dev/null && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verification_capacity.py"` 3/3; `py:format`, `py:lints` (stub `__all__` parity) and `py:typecheck` clean.
- R3-DEADLINE (TypeScript SDK journeys, same process exception): `sdks/wyrd-sdk-ts/wyrd/tests/integration/verification-capacity.test.ts` drives `observe.verify`/`observe.submit` against the napi harness started with `verifyTimeoutMs: 2000`, with `occupyHostedMemory` (a `NativeHeldMemory` with `release`) and `hostedWaiters` (`native-testing/src/lib.rs`; `control` is now generic over its value), over the shared fixtures. It proves the same wait, deferral, 503 `reason`, 504 window and post-release pass as Python. TS `verify` takes no `AbortSignal`, so caller cancellation stays proven in the Rust SDK journey.
  - Proof: `mise run ts:build && mise run ts:build:testing`, then `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner >/dev/null && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verification-capacity.test.ts"` 3/3; `ts:format`, `ts:lints`, `ts:typecheck`, `ts:napi:check`, fmt and lints clean.
- R3-DEADLINE (MCP journey, same process exception): `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs::pg_tests::execution_waits_and_refusals_follow_hosted_capacity` drives `verification.execute`/`verification.enqueue` through a real `rmcp` client against a 2 s verify timeout. It covers the hosted-memory 503 with its bounded `reason`, the 504 within [timeout, 2 x timeout) with no waiter left, then a direct execution that waits and a queued one that defers under one hold, both passing once it frees. MCP returns these 5xx refusals as JSON-RPC errors carrying the problem document (`mcp::wyrd_error_to_mcp`); the journey reads that document.
  - Diagnosis 1 (edge refusal before the tool). **Symptom:** a 0-byte hold returned a transport 503 "request body capacity is occupied". **Evidence:** `/mcp` is a buffered route; `body_limit.rs` charges the 262,603-byte body to the shared ledger before the handler. **Cause:** test setup; the edge charge refuses first, as R3-EDGE requires. **Fix site (fresh diagnostician):** the journey leaves 768 KiB for a 512 KiB answer under the 1 MiB `/mcp` body limit: the body fits, body plus retained bytes do not. The SDK journeys are unaffected: `/v1/verification/execute` passes unbuffered, as their asserted hosted `reason` shows.
  - Diagnosis 2 (the 504 step passed). **Symptom:** after a first passing execution, the held execution ran and passed. **Evidence:** `Sandbox::exec` closes idle workers when `available() < envelope`, then `acquire` takes the freed envelope before the hold's absorbing task wakes. **Cause:** intended reaping under pressure; the test ordered a worker into existence before the refusals. **Fix site (fresh diagnostician):** the journey runs the refusal and timeout before any worker exists, then the wait and deferral together. The `occupy_for_test` doc now states that an operation may take memory it frees for itself. The Python and TypeScript journeys start a server per test and time out before any worker exists, so they are unaffected.
  - Proof: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner >/dev/null && cargo nextest run --locked -p wyrd-mcp --test mcp --run-ignored=all -E 'test(=verification::pg_tests::execution_waits_and_refusals_follow_hosted_capacity)'"` passed 3 of 3 runs; fmt and lints clean. R3-DEADLINE journeys now cover Rust, Python, TypeScript and MCP.
- R3-TEL (operational telemetry, all bounded labels, none by tenant or request). Owners extended, no new owner:
  - `app/metrics.rs` names the families: `wyrd_process_resource_plan{resource}` (set once at boot from the shared `bifrost_resources.plan()` path that production and the test server both run), `wyrd_verification_hosted_operations{state=waiting|active}` and `wyrd_verification_hosted_bytes{state}`, `wyrd_verification_hosted_refusals_total{reason=memory|expired}` (`AdmissionRefusal::reason`), `wyrd_verification_hosted_wait_seconds{outcome=granted|expired}` (queued waits only; added to the verification duration buckets), `wyrd_verification_python_workers{state=runnable|suspended}`, and `wyrd_verification_python_worker_events_total{event=spawn|reset|reap}`.
  - Admission (`hosted/admission.rs`): `Waiter::joined`/`left` are the only places the waiting bytes change, and they move the gauges with them, so push, closed-head pops, grants and cancellation removal stay balanced. Every refusal is counted once where it is made.
  - Sandbox (`hosted/sandbox.rs`): a `Raised` RAII guard undoes each gauge transition exactly once on success, failure or cancellation. These are the active grant (count plus envelope bytes), the checked-out session (`suspended`), and each turn (`runnable` +1, `suspended` -1 while it holds the CPU permit). Spawn is counted when a new PID is first charged (prewarm or first lease), reset after a confirmed `checkout.finish()`, and reap when `Workers::sweep` drops a gone PID's charge.
  - Named timing phase (mechanics decision): the hosted wait (`hold` → `pool.close` reap under pressure → `acquire`) is timed as `Envelope::Capacity`, a `verification.direct.capacity` benchmark part and the `server.capacity` diagnostic (`run::ENVELOPES`, `benchmarks/README.md`). It nests inside the sandbox envelope and the Verifier's blocks, so it is never in, nor charged to, the overhead residual (total less block union), and the outside part is unchanged. An envelope rather than a `Phase`: `Phase` entries are top-level execution stages, and this wait happens inside a block. Queued executions read it through the production wait histogram. `Sandbox::exec` takes the execution's `&Envelopes` (the tool path passes the tools' record; `Sandbox::run` for code checks and tests pass the inert default).
  - RED: `admission::tests::admission_transitions_balance_their_telemetry` failed with `wyrd_verification_hosted_operations{state="waiting"} 2 missing`. The three V2 tests failed with `(0.0, 0.0, 0.0)` against the expected spawn/reset/reap counts and an active count of 0 against 1. `canonical_phases_explain_total_latency` failed to compile on the missing `Envelope::Capacity`. The boot plan test failed with `wyrd_process_resource_plan{resource="effective_cpu"} missing` while the gauges sat on the production-only dependency path. They moved to the shared boot path.
  - Diagnosis (`suspended_sessions_release_runnable_capacity` failed 4 of 8 runs). **Symptom:** mid-test gauges read `runnable` 1, `suspended` 0. **Evidence:** an unbiased `select!` raced the program against the other operation; when the other got the single CPU permit first, the program took the permit as it was released and was still mid-turn when `select!` returned and stopped polling it. **Cause:** test ordering. The test never established that the program had parked on the writer, and the earlier `memory.used() >= envelope` check could not tell. **Fix site (fresh diagnostician):** the test only, since production accounting is correct. It drives the program alone, biased, until the gauges show it parked (active 1, envelope bytes, runnable 0, suspended 1, a state only a parked program reaches with nothing else running), polling like the file's `settle`, then runs the other operation and re-checks. 10 of 10 runs passed.
  - Diagnosis (`level_one_declares_direct_sweep_and_strict_objective`, a V4 selector). **Symptom:** `Python waiters` missing from the recorded limits. **Evidence:** commit `4ef1271b5` rewrote the limits text when it removed the waiter count, but left the assertion. **Cause:** a stale assertion. The text matches R3-ADMIT (no waiter count). **Fix site (fresh diagnostician):** the assertion list now requires `wait fairly per tenant for shared process memory` and rejects any `waiters` count. Historical records are untouched.
  - Proof: `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::admission::tests::admission_transitions_balance_their_telemetry)'`; `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=sandbox_worker::sandbox_reuses_processes_with_fresh_tenant_sessions) | test(=sandbox_worker::suspended_sessions_release_runnable_capacity) | test(=sandbox_worker::failed_workers_settle_before_capacity_returns)'`; `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner >/dev/null && cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all -E 'test(=pg_router_smoke::boot_publishes_the_process_resource_plan)'"`; `mise exec -- cargo nextest run --locked -p wyrd-server --features bench --lib -E 'test(=verification::telemetry::tests::canonical_phases_explain_total_latency)'`; `mise run test:verification:sandbox` (29 passed); the V4 existing selectors (6 passed). fmt and lints clean. Process exception: the wait histogram, refusal counter and plan gauges were written in the same step as their RED assertions.
- R3-BENCH (worker children in benchmark CPU/memory). Owners extended, no new owner: `capacity::ResourceWindow` (now single-server, since every caller passed one) reads the server's and its live worker children's CPU at open and freeze and samples both memories. `Resources`/`Usage` carry `worker_cpu_seconds`/`worker_peak_memory` beside the unchanged parent-only `cpu_seconds`, so `server_cpu_seconds`, `cores` and every per-CPU derivation keep their historical scope.
  - Why not PID-only: a worker that exits in the window is gone from the OS by the close. The `bench` snapshot route therefore sets `wyrd_process_reaped_children_cpu_seconds` from `getrusage(RUSAGE_CHILDREN)` (`app/metrics.rs::record_reaped_children_cpu`). It uses `nix`, which was already in the lockfile, as an optional dependency the existing `bench` feature enables, so there is no new feature or `unsafe`, and production builds never link it. `capacity::worker_cpu_seconds` adds the reaped growth (snapshot before the open, snapshot started just after the freeze, read by `reaped_worker_cpu` through `Metrics::value`, which is presence-aware) to the live-children growth (`os::children_cpu_usec` via `sysinfo`, threads excluded). A worker that exits, starts or spans the window therefore counts only its in-window CPU. A missing counter or a total going backwards is `None`.
  - Memory: `PeakSampler::children` samples the live children's summed RSS at the existing 10 ms interval and lists all processes every 100 ms to find new children. If no child is ever resident the reading is unsampled, never zero.
  - Records: `run.rs::set_workers` writes declared raw readings `worker_cpu_seconds` and `worker_peak_memory_bytes` (`work_metrics`, so both direct and competing steps declare them) as missing-with-reason when unavailable. Records written earlier lack them and keep parent-only server CPU. `benchmarks/README.md` documents the scope. Known ceiling (`ponytail:`): a worker exiting in the milliseconds between a live read and its scrape is counted at both ends or at neither.
  - Churn (AC-032): `set_worker_churn` records `worker_spawns`, `worker_resets` and `worker_reaps` as the growth of `wyrd_verification_python_worker_events_total{event}` between the window's own open and stop scrapes (direct steps), or between the opening scrape and the one after the freeze (competing steps). An event absent at both ends reads zero; a total that went backwards is missing. Asserted in the same selector, written after the first GREEN (process exception).
  - RED: `verification::run::tests::worker_process_resources_preserve_measurement_scope` failed to compile on the missing `capacity::worker_cpu_seconds` and `set_workers`.
  - Proof: that test (an exited, a started and a spanning worker sum to their in-window CPU; each missing end and a backwards counter are `None`; unavailable readings are missing with a reason; the parent CPU is unchanged in new and historical records); `release_server::os::tests::reads_live_children_but_not_threads` (real processes: no child reads 0 CPU and unsampled memory although the process has threads, a missing parent reads `None`, a live child is seen with memory > 0); `app::metrics::tests::benchmark_mode_installs_only_benchmark_recorder` (`--features bench`; every snapshot carries the gauge; process exception: assertion written after the gauge). Also the existing V4 selectors (6 passed) and the V4 persisted-report lib selectors (2 passed). `cargo nextest run --locked -p wyrd-testing --bin capacity` 75 passed. fmt, lints, `check:deps` and `docs:check` clean.
- R3-DEADLINE test controls (resolves a red guard found while proving R3-BENCH).
  - Diagnosis. **Symptom:** `wyrd-testing --lib` `server::tests::public_controls_are_the_three_sanctioned_operations` failed with `Python WyrdTestServer exposes test hooks outside the three sanctioned controls: ["hosted_waiters", "occupy_hosted_memory"]`. **Evidence:** the R3-DEADLINE Python and TypeScript journey commits added those members to the public `wyrd.testing` stub and the `@wyrd/testing` declaration, and the journeys polled `hosted_waiters` in a loop. **Cause:** a contract conflict. TESTING.md allows only narrowly scoped controls and no polling loops in test bodies, and the guard (REQ-195) pins three controls. **Fix site:** this is a public test-surface decision, so it went to the maintainer.
  - Decision (maintainer, 2026-11-14): sanction both as narrowly scoped hosted-memory controls, with the queue read made a blocking wait. `WyrdTestServer::hosted_waiters` became `wait_for_hosted_waiters(count, timeout)`, which returns `HostedWaitersNotReached` with the last count at the deadline. It polls internally (admission publishes no queue-change signal, marked `ponytail:`), as `wait_for_baseline` does. Python (`wait_for_hosted_waiters(count, timeout)`, GIL released) and TypeScript (`waitForHostedWaiters(count, timeoutMs)`; the native `verify` starts before its promise is awaited, so blocking is safe) project it. The four journeys call it instead of looping. The guard (`public_controls_are_the_sanctioned_operations`) and TESTING.md list five controls, and `native.d.cts` was regenerated by `ts:build:testing`. Open for the maintainer: REQ-195 in the active `verified-change-contract` spec still says "exactly three" controls; it is that change's approved text and was not edited here.
  - Proof: the guard selector; `-p wyrd-ai ... -E 'test(verification_capacity::)'` 3/3; `-p wyrd-mcp --test mcp -P journey ... verification::pg_tests::execution_waits_and_refusals_follow_hosted_capacity` 1/1; after `py:setup`, `tests/integration/test_verification_capacity.py` 3/3; `verification-capacity.test.ts` 3/3. fmt, lints, `py:format`, `py:lints`, `py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck` and `ts:napi:check` clean. (`server::teardown_tests::shutdown_aborts_and_joins_a_serve_task_that_outlives_its_drain` needs `WYRD_TEST_DATABASE_ADMIN_URL` and is not this diff.)
- Decision (maintainer, 2026-11-14, V7/R3-DIAG): diagnose only the current runtime. The saved R1 smoke record, its profiles and `target/r1-logs/v4b.log` are not analyzed. R3-DIAG findings come from a fresh `mise run bench:l1:verification -- --smoke --profile --allow-dirty` capture of the committed code, taken after V5 qualification so profiling never overlaps a qualifying run. The failed 20261114 clean record stays INVALID and untouched.
- R3-DOCS: `architecture/wyrd-design.md` Agents, tools, and workspaces now owns the hosted capacity mechanics: worker reuse and discard, the peak-envelope grant, fair per-tenant admission, retained envelope charges, prewarming, suspension, workspace scratch, Busy 503 / arrival-deadline 504 / queued deferral, the edge bypass and the telemetry families. `architecture/bifrost-design.md` Admission and memory, Scribe shards, distributed execution, Forge, and Resource and failure invariants now describe the process plan (`available_parallelism`, `sysinfo` memory, data-root free space less 256 MiB, lowering-only ceilings, readiness failures, the plan gauge), the shared process memory and scratch ledgers, use-charged caches with references held after eviction, and the per-query `spill_cap` charged to scratch. Stale cgroup, whole-budget cache and `spill_share` text was removed. Proof: content review of the diff against source (envelope = 2 x allocator ceiling + resident baseline, `sandbox.rs::session_bytes`; 60 s `verify_timeout`, `hosted/mod.rs`), `docs:check` and `git diff --check` clean.
- R3-PLAN floor (found in R3-DOCS review): live detection checked the 4-GiB floor on host memory before the process ceiling applied, so a below-floor ceiling on a large host booted. RED: `resources::tests::pod_memory_below_the_floor_is_refused` failed at its new `detect` assertion with a `MIN_POD_MEMORY_BYTES - 1` ceiling. Fix: `detect_with_transport_message_limit` checks the floor on the detected memory lowered by the ceiling. The injected harness path (`from_snapshot`, 3 GiB) is not a boot path and is unchanged. Proof: that selector and `process_memory_ceiling_only_lowers_discovery` pass; `-p vala-bifrost-redux --lib` 918 passed, the 19 failures all refuse to start without `WYRD_TEST_DATABASE_ADMIN_URL`; lints clean. bifrost-design Admission and memory states the floor on the budget.
- R3-DOCS review of the other flagged gaps: the Forge spill cap is DataFusion's per-attempt default, charged to the shared scratch ledger (recorded under R3-SPILL above), so the aggregate bound holds. R3-REPLICA overlap is not detectable without cross-process coordination, which the spec excludes ("no global scheduler or database coordination"); the deployment assigns disjoint ceilings, readiness refuses impossible plans, and V6's two-replica record is the proof.
- V5/R3-QUAL (clean qualification, `mise run bench:l1:verification` at bb5200e65 + floor fix, quiet host, 866 s): record `benchmarks/history/verification/direct-verify/20261114T220658Z-5c37628d6.json`, VALID. All six populations had zero errors, zero wrong verdicts, zero lost writes and zero unreconciled work, with 3,748-31,068 overhead samples per repetition (>= 1,000 each). **Objective missed:** server overhead p99 at c64 was Rust 1.208 ms, Python 0.558 ms and TypeScript 1.388 ms (median), against < 0.5 ms; c1 was 0.187/0.202/0.206 ms. Comparison with the newest same-host record (`20261114T130251Z-4bc727c82`, INVALID, kept): c64 throughput rose from 9.9 to 178.7 verifications/s/vCPU (worker reuse: 57,634 resets, 9 spawns, 0 reaps per repetition), CPU per verification fell from 45.9 ms to 3.3 ms, and Verifier p99 fell from 44.2 ms to 18.9 ms. Meanwhile the overhead tail grew from 0.603 ms to 1.311 ms (rep 1): `outside` p99 0.339 -> 0.700 ms, `stage` p99 0.244 -> 0.516 ms, `setup` p99 0.027 -> 0.129 ms, and executions over 1 ms went from 3 to 479. R3-QUAL is FAIL until the c64 overhead tail is diagnosed and fixed; diagnosis proceeds from the V7 profiled capture.
- V7/R3-DIAG capture: `mise run bench:l1:verification -- --smoke --profile --allow-dirty` at the floor-fix commit, record `target/benchmarks/smoke/verification/direct-verify/20261114T221710Z-5c37628d6.json` (VALID smoke, 121 s), with profiles under `20261114T221710Z/profiles/`: per step, samply CPU for the server, driver, and Python and TypeScript drivers, plus heap pprof for the server and drivers, Python tracemalloc, and V8 heapsnapshot/heapprofile.
- R3-QUAL diagnosis (c64 overhead tail). **Symptom:** see V5 above; `outside`, `stage` and `setup` p99 doubled at c64 while CPU per verification fell 14x. **Evidence:** the c64 server CPU profile (`direct-verify-c64/cpu-server-128657.json.gz`, symbolized against `nm -C` of the release binary). 97.6% of samples are on `tokio-rt-worker` threads, and the top self frames are libc `unlinkat` 5.7%, `mkdir` 5.4%, `syscall` 4.7%, `writev` 2.3% and `getdents64` 1.0%. Inclusive: `Arc<Workspace>::drop_slow` -> `TempDir::drop` -> `remove_dir_all` 7.9% under `Agent` drop inside `AgentExecutor::execute`, and `HostedTools::open` -> `Workspaces::open` -> `DirBuilder::mkdir` 5.4%. `Workspace::create` (`hosted/workspace.rs`) makes the tempdir and three subdirectories inline on the async task, and the last `Workspace` drop removes the tree inline, so each Agent check does about 4 mkdirs plus a recursive removal on a runtime worker. That is about 11k workspaces/s at c64. **Cause and fix site:** pending the fresh read-only diagnostician's report (timing/concurrency cause).
  - Diagnostician (fresh, read-only, given the failing command, the record, the profile and the code areas): the first-ranked cause is workspace filesystem syscalls on tokio runtime workers. Removal is 7.9% of server samples: the last `Workspace` drop at the end of `AgentExecutor::invoke` (`vala-verify/src/blocks/agent.rs:477-492`, through `Invocation`, `hosted/tools.rs:202-208`) runs `TempDir`'s `remove_dir_all` inline. Creation is 5.6%: `Workspace::create` runs inline in `Workspaces::open`. About 47k workspaces per window, each one poll of roughly 80-85 us CPU that never yields; a worker stuck in that syscall cannot run the tasks it just woke (its LIFO slot is not stealable), so other executions' `outside` phase waits. The remaining contributors are spread evenly: the tracing/OTel span layer (about 12%), bench-only heap sampling and recorder (5.9%, global locks; bench builds only) and `stage_direct` payload encoding (3.4%). The runtime workers were about 50% busy, which weakens a pure-queueing explanation, and samply shows only on-CPU time, so syscall sleep in ext4 metadata is invisible. Fix site: `Workspace`, the one owner every create and drop path goes through (`HostedTools::open`, `Invocation`, `Sandbox::exec`'s borrow, the cancelled open, `materialize`).
  - Fix: `Workspace` keeps its root as a path, and `create` and a new `Drop` run the directory creation and removal through `off_worker`, which is `tokio::task::block_in_place` on a multi-threaded runtime worker and inline elsewhere. Removal stays synchronous, so the charge still returns after the bytes are gone, the `workspace_cleans_up_on_drop` and tools `the last owner removes the workspace` tests hold, and no directory is orphaned at shutdown, which a detached `spawn_blocking` would risk. No RED test applies: the behavior is unchanged and the defect is the V5 measurement. The guard `workspace::tests::workspace_removal_is_synchronous_off_the_runtime_worker` (multi-thread runtime, drop on a worker and on a blocking thread) passes; `-p wyrd-server --lib -E 'test(/verification::hosted::/)'` under repository Postgres passes 36/36; fmt and lints are clean. Proof of the fix is a re-run of V5.
  - Re-run V5 after the fix: record `benchmarks/history/verification/direct-verify/20261114T225226Z-85247a826.json`, VALID, zero errors. **The fix regressed the tail:** c64 overhead p99 was Rust 1.69-1.74 ms, Python 0.94-0.97 ms and TypeScript 1.66-1.80 ms (from 1.21/0.56/1.39), with 735-868 executions over 1 ms per repetition, `outside` p99 about 1.0 ms and `stage` p99 about 0.75 ms. Throughput fell from 178.7 to 155-162 verifications/s/vCPU, and c1 was unchanged. At about 11k workspaces/s, `block_in_place`'s per-call core handoff costs more than the stall it removes. The diagnosis stands; the remedy failed. Stopped for a maintainer decision before another approach; R3-QUAL stays FAIL.
  - Decision (maintainer, 2026-11-14): reuse workspace directories. `block_in_place` is reverted. `Workspaces` keeps a free list of emptied roots (each with empty `inputs/`, `work/`, `staging/`). `open` takes one instead of creating a tree, and only a fresh pool slot pays `tempdir` plus three `mkdir`. Dropping an opened workspace spawns one blocking-pool task that empties the three directories without following links, then returns the scratch charge, then returns the root; a root that cannot be emptied is removed instead. A refused or cancelled open is never recycled and removes its partial directory and returns its charge at once, as before. `materialize` workspaces (not opened through `Workspaces`) are removed inline. An unrun task at runtime shutdown returns its charge with the dropped closure, and boot clears `verification-scratch`. Proof: `workspace::pg_tests::workspace_cleans_up_on_drop` now asserts that the charge returns after emptying (awaiting `ByteLedger::changed`, no sleep), that the inputs and a code-made nested `/work` directory are empty, and that the next open reuses the emptied root. `cancelled_open_returns_its_charges` and `workspace_inputs_beyond_process_scratch_are_busy` still assert no directory and no charge remain. `-p wyrd-server --lib -E 'test(/verification::hosted::/)'` under repository Postgres passes 36/36; fmt and lints are clean. The V5 re-run measures it.
  - V5 re-run with reuse: record `benchmarks/history/verification/direct-verify/20261114T231733Z-85a723cfd.json`, VALID, zero errors. c64 overhead p99 was Rust 1.36-1.38 ms, Python 0.49-0.70 ms (median 0.507) and TypeScript 1.30-1.33 ms. Throughput rose against the first V5 (c64 Rust 185-197 vs 178.7, c1 29.5 vs 26.7 verifications/s/vCPU), so reuse is kept. Objective still missed: R3-QUAL FAIL. Heap sampling is off in qualification (`heap::start` runs only on the profiling POST), so its lock is not a cause. **Cause evidence:** the tail tracks host CPU saturation, not client language. In c64 repetition 1, total host cores were Rust about 26.3 of 32 (server 18.6, workers 3.9, client and driver 3.8), TypeScript about 25.6, and Python about 21.7 (server 16.2, workers 2.3, driver 3.2). Python's tail is 0.5 ms at higher throughput (6,766/s) than Rust (6,169/s). `stage` and `setup` have no awaits yet carry 0.67 ms and 0.13 ms p99 tails, which is OS run-queue delay. Stopped for a maintainer decision.

  - Decision (maintainer, 2026-11-14): cut server CPU per verification. One pass, then report.
  - Server CPU pass 1, tracing (`dc72bcd2f`). The hot-path spans that are not tenant-catalog spans were already `debug` (`verification.load`, `verification.engine`, `verification.block`, `gateway.route`, `gateway.authorize`, `gateway.admit`, `gateway.capture`). The remaining `info` spans (`request`, `verification.execute`, `gateway.call`, `gateway.execute`, `invoke_agent`, `chat`) are catalog spans that tenant export needs, so none were removed. The OpenTelemetry layer attached each span's OTel context on every poll (`ContextGuard` and `GUARD_STACK` push and pop, about 0.6% self plus the registry `extensions_mut` lookup). No code reads the attached context: `context::current_context`, the sampler and `wyrd-client` resolve it from the current `tracing` span. `subscriber()` now sets `with_context_activation(false)`, so `on_enter` and `on_exit` return at once and a new span's parent comes from `ctx.lookup_current()`. Proof that the behavior is unchanged: `-p wyrd-telemetry --all-features` 30/30 (including `capture_pipeline_preserves_span_semantics` and `production_filters_preserve_tenant_traces`); under repository Postgres, `-p wyrd-server --lib -E 'test(/trace|span|otel|telemetry/)'` 26/26; the `-p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(/^(otel_export|action_timeline|verify_in_real_time)::/)'` journeys 29/29. Clippy is clean. No RED applies, because the defect is the V5 measurement. Allocator churn was not changed in this pass.
  - V5 re-run after pass 1: record `benchmarks/history/verification/direct-verify/20261114T234742Z-814794222.json`, VALID, zero errors, wrong verdicts, lost writes and unreconciled work. c64 overhead p99 (median) was Rust 1.368 ms, Python 0.465 ms and TypeScript 1.326 ms, against 1.37/0.507/1.33 for the reuse record. Throughput at c64 was 196.1/202.5/186.1 verifications/s/vCPU (c1 28.9/26.8/25.1), and Rust c64 still had 640 executions over 1 ms. The change sits within run-to-run noise: per-poll context attachment was not a material part of the tail. Objective still missed; R3-QUAL FAIL. Stopped for a maintainer decision.
  - Diagnostician (fresh, read-only Codex, given the four V5 records, the 20261114T221710Z profiles and the diff since 5c37628d6, without the host-saturation hypothesis). It found two failure sites, each sufficient alone. (1) The `outside` phase p99 is 0.69-0.86 ms for Rust and TypeScript in every repetition of `20261114T234742Z-814794222` (Python 0.20-0.36 ms). The span runs from HTTP arrival (`http/middleware/metrics.rs:39`) through body handling, authentication (about 6.5% inclusive in the baseline profile) and the unphased work before `run_direct` returns, so the cause is unresolved. (2) `stage` p99 is 0.63-0.68 ms (Python 0.21-0.36 ms). It is synchronous projection in `stage_direct` (`service.rs:938`, `results.rs:290,454`; `ResultPayloadBuilder::build` is 2.1% inclusive). Load from the experiment itself is a likely amplifier: about 26.5 combined cores of 32 logical CPUs on 16 physical cores. Full saturation is unproven, because the records carry no run-queue or lock-wait attribution. Reachability: possible but not established, since Python already meets the objective in median (repetitions 0.465, 0.748 and 0.443 ms). Next steps it recommends: split the `outside` attribution (arrival to control entry, control entry to finish, body decode, result construction and teardown) and take a current capture with scheduling and lock attribution before any production fix. The candidate production fix is to hand an owned direct-execution record to the existing Scribe outbox and project it in the consumer, keeping reservation, accounting and shutdown behavior. Moving the load generator off-host would be a diagnostic, not qualification of the objective. No further workspace or tracing change is justified without a current profile.
  - Decision (maintainer, 2026-11-14): follow the diagnostician and attribute `outside` before any further fix. Bench-only: `ExecutionTelemetry::emit` (`verification/telemetry.rs`) derives three diagnostic parts from the timestamps it already holds, with no new clock reads on the request path. They are `verification.direct.outside.before` (arrival to the first phase: middleware, authentication, decode and routing), `.between` (gaps between phases) and `.after` (the last phase to finish: result construction and teardown), and they sum to the unchanged `outside` part. The harness declares them as `phase_outside_{before,between,after}_{p99,max}_ms` (`PHASE_TIMINGS`) and as reachable boundaries (`main.rs` `DIRECT`); `benchmarks/README.md` describes them. Production builds and the overhead calculation are unchanged. RED: `verification::telemetry::tests::outside_time_splits_around_the_phases` failed with `no verification.direct.outside.before`. Proof: `-p wyrd-server --features bench --lib -E 'test(/verification::telemetry::tests::/) | test(=verification::hosted::blocks::tests::histogram_overhead_excludes_real_overlapping_blocks)'` 8/8; `-p wyrd-testing --bin capacity` 75/75. fmt, lints, `docs:check` and `git diff --check` are clean.
  - V5 run with the split: record `benchmarks/history/verification/direct-verify/20261115T001303Z-9a4587b9a.json`, VALID, zero errors. c64 overhead p99 (median) was Rust 1.331 ms, Python 0.477 ms and TypeScript 1.372 ms, so the objective is still missed for Rust and TypeScript. Per-repetition c64 p99, in ms: `outside.before` was Rust 0.65-0.71, TypeScript 0.76-0.79 and Python 0.19-0.30; `outside.between` was at most 0.014 everywhere; `outside.after` was Rust and TypeScript 0.09-0.12 and Python 0.03-0.05; `stage` was Rust and TypeScript 0.63-0.70 and Python 0.22-0.34. The tail is therefore the front door (arrival to `admit`: middleware, authentication, decode and routing) plus `stage`, and both are about 3x higher for the Rust and TypeScript clients against the same Verifier, with the same 1,040-byte responses. Repetition 1 also shows more work per verification: server CPU was Rust 18.3 cores at 6,302/s and TypeScript 18.4 at 6,075/s, against Python 16.0 at 6,789/s (about 2.9 ms and 3.0 ms against 2.35 ms of server CPU per verification). Worker CPU was Rust 19.6 s and TypeScript 22.0 s against Python 11.3 s. The cause of that per-client difference is open.
  - Diagnostician (fresh, read-only Codex, on the client-population asymmetry in `20261115T001303Z-9a4587b9a`, given the record, the table and the code areas, without the GIL hypothesis). All three populations reach the server through `wyrd-client` `Observe::verify`, with the same cached client, pool, request body, retries and headers. There is no duplicate client transport. Benchmark-only machinery adds `sdk.verify` binding timers in Python and TypeScript and benchmark case headers in Rust. What differs is the server load: about 55.7 / 19.4 / 52.4 active server executions (throughput x mean duration) for Rust / Python / TypeScript. Python's binding `sdk.verify.core` averages 9.21 ms against 3.30 ms of shared `client.verify`, a 5.9 ms per-call gap inside `trace::block_on` (trace lookup, detach, runtime bridging and GIL reacquisition; not yet attributed), so its 64 threads keep only about 22 calls in the shared client. TypeScript's binding adds none (9.65 against 9.64 ms). At c1 the model and sandbox times agree across populations (513/521/543 us and 236/244/253 us), so the 3x slowdown comes with concurrency. Amplifiers: the Rust clients share the controller's 8-worker runtime with the local provider, and the provider's `Conversations::reply` builds the first-turn response under the ledger mutex (`workload.rs:301`). The fixed population order and retained server state are confounders. Validity gap: the driver-saturation check reads controller CPU, not a foreign child's CPU or dispatch limit (`experiment.rs:2494`). Conclusion: the c64 steps are SDK closed-loop journeys at unequal server load, not comparable server-overhead measurements. The smallest correction it proposes is to report achieved server concurrency and add a matched-arrival-rate diagnostic. Fix sites to consider: attribute the Python `trace::block_on` gap; move the Rust driver into a child process; narrow the provider ledger lock; record runtime worker counts.
  - Decision (maintainer, 2026-11-14): follow the diagnostician's corrections 1-3. Implemented at c4c5765ff. (1) Every direct step records `server_concurrency`, the time-averaged count of executions in progress (summed `verification.direct` time over the window, `run.rs::server_concurrency`). The closed-loop c64 rows are now described as comparable only within a language (`benchmarks/README.md`). (2) Paced matched-rate diagnostics `direct-verify{,-python,-typescript}-r3000` (`PACED_CLIENTS` 64, `PACED_RATE` 3,000/s). Client `i`'s `k`-th call waits for slot `i + k x clients`, due `slot / rate` after the window opens (`run.rs::Pace`, the Python and TypeScript driver loops). Arrivals are judged like L2's: missed or a lateness p99 over 5 ms is INVALID. The steps carry correctness criteria only. (3) The driver CPU budget of a foreign step holds the controller plus the child process. Process exception: `paced_slots_spread_the_rate_across_clients` and the declaration-table changes were written with the code, not RED first. Proof: `-p wyrd-testing --bin capacity` 76/76; lints, py/ts lints, `docs:check` clean.
  - Smoke `20261115T015510Z-c4c5765ff`: INVALID, `arrivals.late direct-verify-python-r3000` (lateness p99 13.6 ms; Rust 1.3, TypeScript 0.7). At the matched rate, overhead p99 was Rust 0.24, Python 0.27 and TypeScript 0.35 ms at concurrency 3.9 / 3.8 / 4.5, against 1.19 / 0.57 / 1.32 ms at 55 / 21 / 51 in the closed-loop c64 rows.
  - Diagnostician (fresh, read-only Codex, `target/codex-diag3.md`): proposed GIL handoff contention and a scoped `sys.setswitchinterval(0.0001)` for paced Python windows. Applied as 4fd5a1e76; it did not hold. Smoke `20261115T020519Z-4fd5a1e76` was INVALID, with Python lateness p99 12.9 ms plus `arrivals.missed` (1 Rust, 2 Python).
  - Diagnostician (fresh, read-only Codex, `target/codex-diag4.md`, given both records and both diffs). Lateness: a standalone 64-thread experiment pacing 3,000/s with and without a GIL-releasing native call stayed at most 0.4 ms late at both switch intervals, so the GIL mechanism does not reproduce 13 ms. Measurement defect: the Python driver opened its window clock before the recorder snapshot (which holds the GIL), command handling and worker submission, so setup delay was charged as arrival lateness; TypeScript likewise opened before its snapshot. Missed: every driver dropped a slot due before `stop` whose wake-up landed after it (`run.rs` drive, `benchmark_driver.py`, `verification-driver.mjs`); standard open-loop practice sends every scheduled slot and judges its lateness. Recommendation: revert 4fd5a1e76; open the window after setup behind a barrier; measure lateness immediately before `verify`; send every due slot; record per-slot lateness.
  - Decision (maintainer, 2026-11-14): follow it. 87406f57a reverts 4fd5a1e76. 23a733622: paced windows send every slot due before `stop` (the actual-time cutoff applies to closed-loop windows only); the Python driver takes its snapshot, then holds its callers at a `threading.Barrier` whose action opens the window; TypeScript takes its snapshot before opening; lateness is read immediately before `verify`. Each paced call also reports `due_ns`, and `run.rs::set_late` records `arrivals_late` (calls more than 5 ms late) and `arrivals_late_{first,last}_ms` (their due times), diagnostic only. RED: none recorded for the cutoff and ordering (process exception; proven by the smoke below). `late_arrivals_report_count_and_due_span` was written with `set_late` (process exception). Proof: `-p wyrd-testing --bin capacity` 77/77; fmt, lints, py and ts lints clean.
  - Smoke `20261115T035011Z-23a733622`: VALID. Paced arrivals were 9,000 / 9,000 sent with 0 missed in every language, lateness p99 Rust 1.05, Python 2.20 and TypeScript 0.65 ms. Python's 70 late calls were all due 1,029.7-1,054.3 ms into the window: one pause of about 25 ms, not a recurring one; its cause is open. Overhead p99 at the matched rate: Rust 0.249, Python 0.269, TypeScript 0.339 ms, at concurrency 4.0 / 3.9 / 4.5.
  - V5 at 370662d5f (`benchmarks/history/verification/direct-verify/20261115T035230Z-370662d5f.json`): INVALID, `arrivals.late direct-verify-python-r3000` in all three repetitions (16.5 / 22.0 / 19.8 ms), with 246 / 309 / 259 late calls due from about 1 s to 4.5 s into each window. Matched-rate overhead p99 (median): Rust 0.256, Python 0.301, TypeScript 0.302 ms. c64 overhead p99: Rust 1.27, TypeScript 1.29 ms (objective missed), Python 0.52 ms at 19.5 executions in progress against 55.7 and 52.1.
  - Diagnostician (fresh, read-only Codex, `target/codex-diag5.md`): ranked the driver's cyclic garbage collection over the retained per-call `Call` and outcome containers first, and asked for a measurement before any fix: buffered GC events plus caller wake timestamps.
  - Measurement, 05dcf50c9: the Python driver reports its interpreter's collections (`gc.callbacks`) and each paced call's `woke_ns`; `set_late` records `driver_gc_collections`, `driver_gc_max_ms`, `arrivals_late_in_gc` and `arrivals_wake_p99_us`, all declared metrics, diagnostic only. `late_arrivals_report_count_and_due_span` was extended with the code (process exception). Run `target/benchmarks/verification/direct-verify/20261115T044031Z-05dcf50c9.json` (`--steps direct-verify-python-r3000`): 321 collections per window, longest 53-67 ms; 697 of 699 late calls waited through a collection; wake p99 equal to send lateness p99. Cause: each collection scans the driver's retained window state and holds the GIL, so sleeping callers cannot wake.
  - Fix, 04ce94623: the driver encodes each call to JSON as it finishes (`Call.encode`) and splices the strings into its `done` answer, so a window retains no scanned containers. Run `target/benchmarks/verification/direct-verify/20261115T044326Z-04ce94623.json`: VALID; 0 late calls, one collection per window of at most 0.084 ms, lateness p99 130-138 µs, client p99 1.98-2.77 ms (was 6.1 ms).
  - Bifrost restart test: `bifrost::cluster::tests::cluster_restart_rederives_same_plan_from_retained_snapshot` failed every time (`governed_memory_used_bytes` 0, expected the two cache ceilings). Diagnostician (fresh, read-only Codex, `target/codex-diag6.md`): stale expectation since 3de3bec7a moved the footer and range caches to entry-level charging; `architecture/bifrost-design.md` says "an empty cache withholds nothing". Fix 1f4cd1e21 expects 0 and corrects the stale precharge comment in `tests/bifrost/oracle/capacity.rs`. Proof: `-p wyrd-testing --lib` under `with-test-postgres.sh` 70/70.
  - Report: d1c3b9420 adds a declared `compare` control the shared renderer reads (`Record::render_comparison`), and Verification declares it by language over rate, `server_concurrency`, overhead p99, server and client p50/p99, with a note per workload. RED: none (written with the code; process exception). Proof: `experiment::tests::report_compares_declared_steps_side_by_side`.
