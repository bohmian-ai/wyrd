---
id: TASK-003-R2
kind: remediation
status: superseded
superseded_by: TASK-003-R3
spec: SPEC-verifier-code-mode
spec_revision: 8
requirements: [REQ-009, REQ-011, REQ-012, REQ-013, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027, REQ-028, REQ-029, REQ-030, REQ-031, REQ-032, REQ-033, INV-001, INV-002, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, AC-007, AC-008, AC-009, AC-010, AC-015, AC-017, AC-018, AC-019, AC-020, AC-021, AC-022, AC-023, AC-024, AC-025, AC-026, AC-027, AC-028, AC-029, AC-030, AC-031, AC-032, AC-033, AC-034]
depends_on: [TASK-002B, TASK-003-R1]
parent_task: TASK-003
remediates: [TASK-003, TASK-003-R1]
---

# Resource-backed hosted execution and verification benchmark closeout

## Outcome and Value

A tenant's verification waits for fairly scheduled process resources instead
of immediately failing when the seventeenth Agent opens a workspace. Healthy
Python workers serve fresh sessions repeatedly; suspended programs leave CPU
available to other tenants while their memory stays accounted for. Hosted
execution, recording and Bifrost share one process resource plan, so one
capability cannot silently promise memory or scratch already promised elsewhere.

On the benchmark host (32 CPU threads, 91 GiB RAM), c64 can need 128 Agent
workspaces. The clean run currently rejects most arrivals at a fixed count of
16. This task fixes capacity and lifecycle at their owners, then measures the
unchanged three-SDK workload and finishes TASK-003's outstanding evidence.
The result must explain sustainable throughput, queueing and resource use,
including interpreter children, rather than merely raising limits until a run
passes.

**Authority:** approved spec revision 8 ([Resource-backed capacity](../spec.md#resource-backed-capacity))
authorizes this runtime correction. `depends_on: TASK-003-R1` means its implemented cleanup,
not its still-open benchmark PASS: R2 owns those remaining obligations.

## What Changes and What Must Stay True

### One resource plan and one set of promises

Resolve the plan at startup before consumers. Discover CPU using
`std::thread::available_parallelism`, memory and filesystem availability through
portable interfaces, and apply explicit operator ceilings only downward.
Discovery is an estimate of process resources, not instantaneous spare CPU or
an inferred container allocation. Shared-host deployments use lower ceilings.
Unavailable discovery or an impossible positive plan must fail readiness with
an actionable error; do not fall back to guessed counts.

Replace the unevidenced 16-workspace, 2/8-worker, 16-waiter and 2-GiB-pool
defaults with derived capacity. Account retained request/input bytes, workspace
materialization and scratch, interpreter baseline/heap/stack/IPC, idle and
transitional workers, recording buffers, and Bifrost reservations exactly once.
Measure baseline/headroom; do not present the current 384-MiB envelope as an
OS-enforced RSS bound. The HTTP edge's fixed 1,024 in-flight bound and its
buffered bodies are in scope: waiting raises in-flight requests, so edge
admission charges the same plan and must not shed with a generic 503 ahead of
fair hosted admission (prove it in V6 at hundreds of tenants). Fixed capacity that remains must have evidence and a
stated protection; arithmetic-only tests are insufficient justification.

Extend the existing resource composition and reservation owners. Preserve
Bifrost's role/class and supported-memory-floor contracts, cooperative memory
failure and protected progress. No new independent per-tenant resource root.
Remove both Bifrost cgroup discovery and live pressure probes, including their
consumers and misleading telemetry. Use portable accounting/pressure evidence
without weakening ingress protection. Account scratch sharing a filesystem
once even when consumers use different directories. Track descriptor ownership
and worker churn; do not introduce nonportable descriptor-limit probing.

### Reuse processes; isolate sessions

Use Monty's existing subprocess finish/reset lifecycle for healthy operations.
A fresh session must clear prior Python state, mounts, callbacks, output,
authority and session/allocator accounting, including across tenants. Crashed,
cancelled and failed-reset workers are discarded. An ordinary program failure
may return a worker only after a proven successful reset; uncertain health
requires discard. Charge resetting/reaping workers until their ownership ends.

Effective CPUs bound runnable interpreter turns. A program suspended on a
mediated tool/model wait releases its runnable permit and keeps its resident
memory reservation. Reacquire runnable capacity fairly before resumption.
Resident session count follows accounted memory, not CPU count. Preserve
parent-mediated IO/permissions, enforced interpreter limits and fresh-session
isolation identically on all supported platforms.

### Fair admission, deadlines and overload

Direct and queued hosted work share FIFO within tenant, round robin across
tenants, and work-conserving admission on each replica. Eligible tenants make
progress while a busy tenant refills; idle capacity is borrowable. Bound waiting
by all retained bytes, not another guessed count. Acquire a required resource
set atomically. Do not wait with a partial acquisition or a permit nested work
needs. Resources already in use by a suspended session remain charged; that
ownership must not become an admission cycle when children need resources.

One absolute deadline begins at arrival and includes queueing, lookup, staging,
checkout, execution and resumption. Preserve shorter operation/cumulative Agent
budgets, including host waits and retries. HTTP and MCP use the same deadline
semantics; the generic edge timeout must not turn an admitted verification's
capacity wait into a different timeout. Queued execution retains its existing
durable scheduling/accepted-authority contract and PostgreSQL-owned coordination
timestamps: no fresh timeout at hosted dequeue and no spent attempt on capacity
deferral. Existing earlier durable deadlines continue to win.

| Condition | Required visible result |
| --- | --- |
| Temporary occupancy, waiting bytes available | Wait fairly, then run within the original deadline. |
| Accounted waiting capacity full | Existing `WYRD_SERVER_503_SERVICE_UNAVAILABLE`, safe structured resource/reason details and HTTP `Retry-After: 1`; queued work defers without spending an attempt. |
| Absolute deadline expires, including in queue | Existing verification timeout 504 (`VerificationExecutionTimedOut`), never a capacity 503 or a failed-subject judgment. |
| Recording admission full | Existing recording-pressure 429; permission audit stays non-blocking. |
| Cancellation/disconnect/shutdown | Remove waiters, stop dispatch, settle mediated work and reset/discard/reap as applicable before releasing owned reservations; no late result. |

SDKs expose existing errors and retry hints without silent verification replay.
Use bounded structured reasons rather than parsing prose or allocating tenant
metric labels. Record admission wait as a named server timing phase so the
existing server-overhead residual keeps its meaning: waits are neither hidden
in nor charged to it. Export effective plan, active/waiting counts and bytes, queue
wait, runnable/suspended workers and spawn/reset/reap counts using existing
operational telemetry. Replica-local admission makes no global fairness promise
and adds no PostgreSQL semaphore, coordination table or hot-path statement.

### Preserve the workload and complete the evidence

Keep one Assert, two independently eligible Agents, four governed model
exchanges and two real code operations per acknowledged verification. Preserve
all six Rust/Python/TypeScript c1/c64 populations, original citations, compact
decisions/explicit details, accepted authority, recording, per-Agent tool cap,
content/security limits, L2 and honest total-time boundaries. Benchmark child
CPU/memory must join parent measurements; parent-only historical readings keep
their original meaning. Keep profiling separate and benchmark machinery out of
production artifacts.

This is one outcome spanning the necessary owners: resource-backed execution
that can qualify the existing workload. No new Card/API mode, public benchmark
API, second recorder, worker transport, general profiling framework, database
pool expansion, global tenant quotas, client retry, workload reduction, OS
sandbox, `/proc`, cgroup, `prlimit` or `cfg(target_os)` implementation.

## Approach

1. After revision approval, reconcile the architecture owner sections listed
   below before or with their behavior. Revalidate R1's implemented baseline
   and preserve the failed run and diagnostic artifacts.
2. Extend startup composition into one resource plan, remove explicit cgroup
   probing and compose hosted, recording and Bifrost reservations without
   independent memory/scratch promises.
3. Extend existing hosted admission to tenant-fair, byte-bounded atomic waits
   shared by direct/queued work; carry arrival deadlines and exact overload
   semantics through existing service, edge and error owners.
4. Reuse healthy Monty subprocesses with fresh sessions, separate runnable CPU
   from resident memory and prove cancellation/reset/reaping ownership.
5. Add operational observations and complete parent/worker benchmark accounting;
   prove changed behavior with focused TDD and public journeys.
6. Calibrate CPU cost and derive targets, qualify the six populations, run
   sustained tenant/recovery and two-replica experiments, analyze V4 artifacts
   and reconcile R1's evidence ledger for independent review.

## Ordered Implementation Scenarios

### Scenario 1 — Consumers cannot promise the same resources twice

**Behavior.** Startup on differently sized resource snapshots derives capacity
from the same CPU/RAM/filesystem plan. Lower ceilings reduce it; zero, malformed,
overflowing, unavailable or impossible inputs refuse readiness. Hosted workers,
workspace staging, waiting payloads, recording and Bifrost cannot jointly reserve
more memory or scratch than available. Failure releases only owned resources;
an allocation failure cannot leave the ledger corrupted. Existing Bifrost
pressure protection works without cgroup data.

**RED.** Add planned resource-plan/accounting proofs in [V1](#v1-resource-plan-and-admission).
Use small and large snapshots, lowering ceilings and joint consumer pressure;
show the current independent defaults and overlapping budgets fail the required
aggregate assertions. Include overflow and discovery failure, not host-dependent
timing or synthetic host load.

**GREEN.** Compose existing resource roots at startup; derive capacity and route
all consumers through their charged shares. Remove cgroup probes/consumers and
make unsafe plans fail startup. Run V1's resource cases.

**REFACTOR.** Remove duplicate detection, obsolete fixed-capacity arithmetic and
parallel ledgers. Keep capability-specific accounting at its current owner and
cross-product composition server-owned; do not move server IO into `wyrd-spec`.

### Scenario 2 — Contending tenants wait fairly without deadlock

**Behavior.** Mix direct and queued work from multiple tenants under constrained
resources. Same-tenant requests start FIFO; rotation gives each eligible tenant
its turn and a lone tenant can use idle resources. A cancelled head leaves no
waiting bytes. A request that cannot acquire its whole resource set holds none
of that acquisition. Code awaiting nested work completes even with one runnable
permit and non-code tool cap 1. A tenant whose head does not fit cannot block
other eligible tenants or be bypassed within its own FIFO.

**RED.** Add the planned admission proof in V1 and extend the real-worker proof
in [V2](#v2-real-worker-lifecycle). Deterministically hold/release resources to
observe start order, byte totals and nested progress. Current immediate refusal
and independent semaphores must fail those observations.

**GREEN.** Route direct/queued resource acquisition through the common local
fair admission owner, with atomic required sets, cancellable byte-charged waits
and progress-safe nesting. Rerun Scenario 1 and V1/V2 contention cases.

**REFACTOR.** Remove duplicate per-path admission and obsolete count waiters.
Do not duplicate Bifrost's query-class arbiter or create a PostgreSQL scheduler.

### Scenario 3 — Reused processes keep no prior tenant or Python state

**Behavior.** A healthy worker serves tenant A then B with a reset between them;
B cannot observe A's globals, mounts, pending callbacks, output, credentials or
accounting. Prove actual process reuse, not isolation achieved by respawning.
Suspending one session releases runnable capacity so another runs, while
resident bytes stay charged; resumption cannot exceed the CPU bound. Crash,
cancel, failed reset and spawn failure leave no reusable dirty worker and no
released reservation for a still-live worker. Subsequent authorized work succeeds.

**RED.** Add planned lifecycle cases in V2, extending existing state-isolation
and shutdown coverage. Observe real worker identity and ownership transitions
using existing test/telemetry facilities. Current one-checkout recycling fails
reuse; held runnable capacity during a mediated wait fails other-tenant progress.

**GREEN.** Use the installed Monty finish/reset API and existing pool; remove
forced single-checkout recycling. Enforce runnable turns independently of
resident-session admission and settle all failed/abandoned lifecycle paths.
Rerun earlier scenarios and real-worker regressions.

**REFACTOR.** Remove obsolete spawn-per-operation assumptions and duplicate
cleanup. No custom Reset protocol, interpreter implementation or OS isolation.

### Scenario 4 — Public overload, timeout and recovery are unambiguous

**Behavior.** Through each public SDK and MCP/HTTP, hold resources, submit a
verification, release capacity and observe success. Fill the byte-bounded
waiting space and observe the exact 503/retry contract; expire a wait and
observe verification 504 measured from arrival. Independently exhaust recording
and retain 429. Cancel waiting/running work and then succeed without leaked
state, further dispatch or a late result. Queued refusal defers with unchanged
attempt count and accepted authority, then recovers; no client silently replays
the immediate verification. Host failures cannot be caught into a verdict.

**RED.** Add planned focused journeys in [V3](#v3-public-journeys). They drive
real SDK → WyrdTestServer + repository Postgres → SDK and actual workers. Current
code refuses before waiting, restarts the engine deadline after lookup, and
lacks resource-503 retry headers, so these observations must fail. Keep exhaustive
ordering/fault mechanics in V1/V2 rather than repeating them per language.

**GREEN.** Carry arrival deadline and structured resource refusal through the
existing service/runner/edge/error owners and ordinary SDK projections; retain
recording and accepted-authority boundaries. Run V3 and preceding scenarios.

**REFACTOR.** Consolidate duplicate error/deadline mapping. Preserve stable
catalog variants and typed problems; no string parsing, client retry wrapper or
new durable state.

### Scenario 5 — Resource evidence covers worker processes and admission

**Behavior.** Work that waits, runs, suspends, resumes, resets and reaps produces
reconcilable counts/bytes/reasons and queue wait without tenant/request labels.
The benchmark distinguishes server from interpreter-child CPU/memory, includes
workers spawned or reaped during a window, and never reads a missing child
counter as zero or double-counts it in aggregate CPU. Old parent-only records
still render honestly. TypeScript c1 must obtain the unchanged sample floor
without fabricating samples or selectively omitting a repetition.

**RED.** Add planned reporting/accounting cases in [V4](#v4-benchmark-evidence-and-regressions)
covering a child that exits during the window and a missing counter. Current
server-PID-only collection must fail completeness. Extend the existing capacity
declaration/report proof for any common window/sample change; retain historical
record interpretation and exact workload counts.

**GREEN.** Extend existing process/resource and recorder owners to collect
worker lifecycle costs and bound-label operational transitions. If necessary,
extend common windows uniformly and record their method identity; do not relax
the floor. Rerun V1–V4 affected proof and production-isolation checks.

**REFACTOR.** Keep one recorder, renderer and process-accounting owner. Remove
stale parent-only aggregate claims and cgroup fields instead of aliases; no new
collector or synthetic profile validator.

## Acceptance Criteria

| Obligations | Owner and concrete proof / failure meaning |
| --- | --- |
| REQ-027; INV-001/010; AC-028 | Startup/resource owners, V1: larger/smaller snapshots and lower ceilings derive capacity; unsafe discovery/overflow fails readiness. Mixed hosted/recording/Bifrost reservations, including scratch on one filesystem, never exceed the common roots. Independent roots promising the same bytes fail. |
| REQ-029; REQ-011/013; AC-007/008/030 | Hosted admission, V1/V2: FIFO/tenant rotation, idle borrowing, cancelled waiter removal, exact byte accounting, atomic acquisition and nested progress at cap 1. Bypassing another eligible tenant, partial hold or deadlock fails. |
| REQ-028; REQ-012; INV-002/005/010; AC-009/015/029 | Sandbox/worker owners, V2 plus V3 two-tenant journey: same healthy process, fresh session, no residual globals/mounts/callback/output/authority/accounting; runnable bound and retained suspended bytes; failed reset/crash/cancellation reaps before release and later work succeeds. Respawn-only isolation is incomplete. |
| REQ-009/030; INV-004/006; AC-010/017/018/031 | Service/runner/HTTP/MCP/SDKs, V3: wait/succeed, full-byte-queue 503 with resource/reason and Retry-After, arrival-expiry 504, independent recording 429, cancellation and queued recovery without spent attempt or authority refresh. No false judgment, late publication, swallowed host failure or silent replay. |
| REQ-031; INV-007/009; AC-032 | Operational metrics and benchmark process owner, V4: bounded label sets and balanced lifecycle counts/bytes; parent plus child totals and churn, unavailable counter explicit, historical parent-only scope preserved. Normal packages still exclude benchmark machinery. |
| REQ-019–023/032; AC-019–024/033 | Capacity benchmark, V5: clean six-population record, every c64 repetition zero errors, correct checks/exchanges/code counts and details, >=1,000 p99 samples in every repetition including TS c1; unchanged c64 server-overhead p99 <0.5 ms, plus recorded measurement-derived throughput target met. Integrity VALID alone does not mean performance criteria met. |
| REQ-033; AC-034 | Existing experiment/harness owners, V6: hundreds of tenants under sustained saturation recover with bounded waiting/resources, fair eligible progress and no accumulating parent/child memory, disk or descriptors; two replicas show those properties independently without database coordination. |
| REQ-024/026; INV-008/009; AC-025/027 | R1 V4/V5 carried into V7: artifact-level phase/CPU/native and managed allocation analysis, exact feature/tool/process/window identities and production exclusion. A VALID smoke or list of 66 captures alone cannot close diagnosis. |
| REQ-025; AC-022/026; remaining R1 obligations | V4/V5/V7: retain compact/details, original records/citations, persisted and legacy report semantics, L2 declarations and prior security/cleanup evidence; reconcile every R1 Obligations/Implementation Evidence row. Changed seams need new proof, unchanged passed work is not rebuilt. |

## Implementation Reference

Inspected baseline: R1 evidence and current source at `e3759ee3e`; revalidate
before implementation. Both diagnoses are evidence, not authority: codex-b's
fresh-process recommendation and both guessed numeric targets are superseded
by the maintainer decision. Paths below guide ownership, not private code shape.

- **Resource composition:** `vala-bifrost-redux/src/resources.rs` already owns
  `ResourcePlan`, `SystemResourceSnapshot`, `BifrostRuntimeResources`, governor
  reservations and `detect_snapshot`; server `boot/mod.rs` composes them before
  storage/roles. `host_memory` uses installed `sysinfo`; scratch uses `statvfs`.
  Current detection plus `container_memory_limit`, `container_memory_used` and
  `cgroup_pressure` explicitly probe cgroups. Scribe `ingress.rs` and
  `scribe/memory.rs` consume the tripwire/fields. Extend this composition with
  cross-product allocation, eliminate prohibited probing and update consumers;
  a separate discovery service or independent hosted budget would duplicate it.
- **Hosted owners/callers:** `HostedLimits::{default,live_workers}` in
  `verification/hosted/mod.rs` derives five workers from the 2-GiB/384-MiB
  arithmetic. `Workspaces::{new,open}` in `workspace.rs` uses a fixed semaphore
  and `try_acquire_owned`; `tools.rs` opens it for each Agent. `Sandbox::{start,
  exec,shutdown}` in `sandbox.rs` has another immediate semaphore, sets
  `max_checkouts_per_worker = Some(1)` and drops every checkout. Extend these
  owners into joint fair admission and session reuse, not a parallel executor.
  `worker.rs` owns the interpreter allocator and thread/IPC boundary; preserve
  those limits while measuring their full resident cost.
- **Reuse API:** installed `monty-pool-1.1.0/src/checkout.rs::Checkout::finish`
  calls `finish_session`, sends existing Reset for subprocess workers and
  returns healthy workers to the pool; error discards them. Use it rather than
  inventing a protocol or patching Monty. Existing
  `sandbox_worker.rs` state-isolation, runaway and shutdown tests use the
  real Cargo-provided server executable; `hosted/workspace/{tests,pg_tests}.rs`
  owns scratch/refusal tests. Extend their assertions for charged waits/reuse.
- **Arrival through execution:** `components/verification/service.rs::execute`
  accepts `RequestArrival`, reserves two outbox slots in `run_direct`, then
  currently starts a fresh deadline at `engine.run`. `verification/runner.rs`
  maps `EngineError::Busy` to Deferred without spending an attempt. Extend these
  existing paths and `http/middleware/{edge_capacity,edge_timeout,body_limit}.rs`
  only as needed for charged arrival waits. Do not duplicate accepted authority,
  add a direct-verification SQL statement or expand database pools from RAM.
- **Recording and errors:** `scribe_outbox.rs::ScribeOutbox` aliases runtime
  `Outbox<ScribeSink>` with byte-weighted reservations and fixed defaults;
  compose its bytes/progress allowance under the process plan, preserving
  batching, recording 429 and non-blocking audit. `http/error.rs::retryable_now`
  currently omits generic resource 503. Extend its resource-refusal projection
  using existing `WyrdError::ServiceUnavailable` details, not all unrelated 503s.
  `wyrd-spec/src/error.rs` owns contracts; SDKs project, never schedule/retry.
- **Measurement:** `wyrd-testing/src/capacity.rs::ResourceWindow` opens/freezes
  `LocalServer` counters; `release_server/os.rs::process_cpu_usec` and
  `PeakSampler` read one PID. Extend those owners for worker descendants and
  churn with portable interfaces. `capacity/verification/{workload,run,mod}.rs`
  already owns the shared fixture, populations, exact counts and criterion;
  Python/TS drivers remain under SDK test/profile paths. Existing `experiment`
  and `profile` owners retain reporting/capture lifecycle. Reuse
  `WyrdTestServer`, release-server composition and existing cluster support
  (`wyrd-testing/src/bifrost/cluster.rs`, `BifrostTopology::TwoPod`) where suitable;
  do not assume the Bifrost-only topology already provisions hosted verification.
  Extend the current harness for the precise missing hosted multi-tenant/
  two-replica experiment rather than a separate benchmark framework.
- **Public/test/config closure:** server config/boot, existing runtime outbox
  reservation consumers, Vala block cancellation propagation, operational
  telemetry, Rust `verifier_building_blocks` journey, Python/TS corresponding
  stories and MCP `verification::pg_tests` own the changed seams. Reuse shared
  checked-in Card fixtures and repository services. Update authored docs/settings
  and generated contracts only from their owners if projections change. Read
  nested SDK/docs/SQL `AGENTS.md` before touching those trees; no SQL change is
  intended. Installed portable dependencies are the first choice; a new
  dependency/feature still needs authority if actually necessary.

Architecture obligations, before or with implementation:

1. `architecture/wyrd-design.md`, **Verifier → Agents, tools, and workspaces**
   (fresh-session paragraph around line 1996): retain isolation, specify reuse,
   runnable/resident resources, tenant fairness, arrival deadlines, refusal and
   settled cleanup; establish cross-product resource ownership here.
2. `architecture/bifrost-design.md`, **Query: Oracle → Admission and memory**
   (resource-envelope paragraphs around lines 1183–1213): reconcile independent
   memory/headroom examples, immutable Bifrost-only discovery and overrides with
   one process plan and downward ceilings; remove cgroup assumptions. Preserve
   query classes, supported memory floor, existing queue/deadline/error semantics.
3. The same Bifrost owner's **Ingest: Scribe → Shards and hierarchical ownership**
   and **Resource and failure invariants**: compose complete-vector admission,
   recording and scratch roots without double promises, preserving once-only
   release and bounded progress. Evidence or derivation must justify remaining
   fixed capacity. These are approved-intent owner corrections, not edits to
   make architecture follow an accidental implementation.

## Verification and Evidence

**Planning only:** none of these commands runs while writing this task. At
implementation, run Cargo-backed work sequentially. New names below are
**planned**, not existing selectors; packages/targets/setup were inspected.
Confirm each actual selector after adding it and before RED. If private owner
placement changes, record an equivalent exact command without weakening proof.
Each proof must assert behavior, not merely list calls or metrics.

### V1 Resource plan and admission

Planned focused resource proofs: discovery/ceilings and non-overlapping
consumer roots at the existing Bifrost resource owner; fair hosted contention
at the existing server owner. Pure injected resource snapshots need no server.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=resources::tests::process_plan_prevents_overlapping_reservations)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tests::admission_rotates_tenants_and_accounts_waiting_bytes)'
```

Cover increasing/decreasing host resources, operator ceilings, discovery failure,
overflow, exact release and joint pressure. Retain the owning Bifrost ingress
and memory-pressure regressions after removing their cgroup consumers; record
actual changed selectors in evidence, without inventing replacement host data.

### V2 Real-worker lifecycle

The existing integration target builds its own actual `wyrd-server subprocess`
binary; use that executable, not an inferred target path. Existing sandbox lane
supplies the Postgres wrapper for its companion artifact tests. Planned cases:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=sandbox_worker::sandbox_reuses_processes_with_fresh_tenant_sessions)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=sandbox_worker::suspended_sessions_release_runnable_capacity)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=sandbox_worker::failed_workers_settle_before_capacity_returns)'
mise run test:verification:sandbox
```

Separate focused assertions identify reset isolation, mediated-wait progress and
failure cleanup. Use deterministic coordination and existing worker/pool test
facilities, no sleeps to guess ordering or new production fault API. Extend
the existing same-session state/mount/shutdown coverage instead of replacing
required assertions. Run focused commands for any additionally named tests.

### V3 Public journeys

Reuse the existing Rust, Python, TS building-block stories and MCP verification
module; add one focused capacity/recovery journey per surface, with the same
scenario meaning. Rust's public client drives HTTP; additionally assert HTTP
headers at its existing transport/error test seam if the SDK hides them. Real
server + repository Postgres + real Monty is mandatory. Planned selectors:

```bash
mise run build:server
mise run py:setup
mise run ts:build
mise run ts:build:testing
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verifier_building_blocks::verification_waits_and_recovers_under_capacity_pressure)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run --no-sync python -m pytest -q tests/integration/test_verifier_building_blocks.py::test_verification_waits_and_recovers_under_capacity_pressure'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t "^verification waits and recovers under capacity pressure$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::verification_waits_and_recovers_under_capacity_pressure)"'
```

Keep distinct assertions for success after wait, byte-queue refusal metadata,
arrival expiry, recording pressure, cancellation, cross-tenant reset isolation
and queued attempt/authority preservation. The shared Rust journey owns the
full recovery/attempt matrix; other surfaces prove their actual exposed wait,
error, cancellation and result boundaries. No full SDK/MCP suite here.

### V4 Benchmark evidence and regressions

Planned parent/child/report proof plus inspected existing declaration and
persisted-report tests:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::run::tests::worker_process_resources_preserve_measurement_scope)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::tests::level_one_declares_direct_sweep_and_strict_objective) | test(=verification::tests::level_two_declares_phases_and_records_rate_overrides) | test(=verification::run::tests::canonical_workload_requires_all_three_checks) | test(=verification::run::tests::canonical_client_latency_includes_all_work) | test(=verification::run::tests::language_drivers_preserve_public_boundaries_and_counts) | test(=verification::run::tests::language_phase_breakdown_requires_complete_evidence)'
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::reports_keep_repetition_diagnostics_after_artifacts_are_removed) | test(=experiment::tests::reports_regenerate_from_persisted_records)'
```

Operational telemetry proof belongs to V1/V2's transition assertions: check
bounded reason labels and balanced gauges/counters, including cancelled waiting
bytes and retained resetting/reaping memory. Benchmark-only collection remains
behind bench. Historical parent-only CPU is labelled as such, never rewritten
to include invented worker cost.

### V5 Calibration and clean six-population qualification (R1 V3)

Before final qualification, measure canonical CPU cost using server **and all
interpreter children**, per SDK, excluding separately labelled client/provider
fixture CPU. Record CPU seconds per completed verification, effective cores,
sampling/churn coverage, queue wait and measured resource headroom. Use a
separate calibration window to derive the c64 CPU ceiling:
`effective_cores / CPU_seconds_per_verification`. The system's own measured
utilization is never an input: that would let an artificial limit set its own
target. Qualification meets the throughput criterion when, at c64, either
parent-plus-worker CPU approaches effective cores, or there is no capacity
waiting (load bounded by the 64 closed-loop callers), or the record names the
saturated non-CPU resource (IO, recording, descriptors, memory) with its
measurement. Idle CPU alongside capacity waiting with no named saturated
resource fails. Freeze calibration inputs and the ceiling before qualification;
do not fit them to the qualification result. Derive c64 mean latency expectations from measured throughput and 64
closed-loop callers, with measured queue/service distributions for tails.
Do not invent a new total-client p99 SLA, arbitrary efficiency fraction or adopt
either diagnosis's guessed rate. A missing CPU domain blocks this derivation.

```bash
mise run bench:l1:verification
mise run bench:results
mise exec -- target/profiling/release/capacity report benchmarks/history/verification/direct-verify/20261114T130251Z-4bc727c82.json
```

Use the existing bench owners to run and record the separate calibration;
record its exact invocation before execution. The first command above is the
final full qualification, not calibration. Keep all normal repetitions and the
canonical workload/limits other than approved derived process capacity. If
common windows must grow to get 1,000 samples, declare this uniformly, preserve
precision/integrity and identify the new method and runtime budget honestly.
Never repeat until luck passes, drop a repetition or fabricate samples.

Record every population's successful/error counts, exact four-exchange/two-code
reconciliation, throughput, call-return/result-ready and server p50/p95/p99,
overhead criterion and phase populations. Require zero c64 errors in every SDK,
all sample floors and overhead p99 <0.5 ms; report derived throughput target
met/missed/unsettled separately from VALID/INVALID. Retain misses and diagnose
at the owner; no weakened criterion. A clean final candidate and quiet host
are prerequisites. No commit/push/reset is authorized by this task; if source
cleanliness needs a caller-owned Git action, keep qualification pending.

Re-render the new record using its actual path and an actual pre-extension
legacy record using its path; record both commands and outputs in evidence.
The failed 20261114 record is retained and re-rendered as INVALID, not promoted
or treated as the required older legacy-method proof. No full L2 rerun.

### V6 Sustained saturation/recovery and two replicas

Extend the existing experiment/harness only for the missing workload, with
recorded exact invocations before execution. These are opt-in experiments,
not new default CI/performance gates. No existing CLI flag for this workload
is assumed. Use real authenticated tenants, the same canonical verification
and governed mock provider; combine direct and queued work. Include at least
200 active tenants (a workload size, not a production capacity constant), a
busy tenant that continually refills, and mixed resource demand with suspended
code so other tenants must progress.

Record baseline, sustained saturation, load removal and drain/recovery phases,
offered/accepted/completed/refused/cancelled work, fair per-tenant progress and
queue/resource time series. Size the saturation interval to cover repeated
queue turnover and worker reset/reuse; extend observation until post-warmup
memory/disk/descriptor trends can distinguish a plateau from accumulating
ownership. State actual durations and that rationale; a short smoke is not
sustained evidence. Compare final live reservations/workers/queues to idle
ownership; scratch and descriptors return to their valid baseline and memory
plateaus within the plan, allowing explicitly measured runtime retention.
No rising trend may be dismissed merely because the limit was not yet reached.

Repeat with two actual server processes and shared repository Postgres/storage,
recording load distribution and effective plan per replica. On one host, apply
lower ceilings so their combined budgets fit the host; do not let each claim
91 GiB. Saturate one replica while the other has headroom and show local fair
progress, bounded queues and recovery on both without cluster coordination.
Do not claim global FIFO, global tenant quotas or linear speedup. Expected
overload 503/429 during saturation must reconcile and remain distinct from the
zero-error c64 qualification contract. Evidence with unmeasured worker children,
leaked scratch/descriptors, starvation or an undrained backlog cannot pass.

### V7 R1 diagnostic analysis, ledger and changed-owner checks

R1's cleanup is implemented. Reuse its Obligations and Implementation Evidence;
do not repeat all RED or reimplement profiling. Preserve these facts:

- The clean L1 record above is INVALID: c64 refusals reconcile exactly to
  workspace opens (Rust approximately 356k, Python 59k, TS 156k per repetition),
  Rust c64 overhead p99 is 0.664 ms, and TS c1 has 980–997 samples. None is
  current acceptance.
- V4 profiled smoke was VALID with 66 captures, but its AC-025 artifact
  analysis is unwritten. Its record is
  `target/benchmarks/smoke/verification/direct-verify/20261114T130923Z-e3759ee3e.json`,
  artifacts are under the adjacent `20261114T130923Z/profiles/`, and
  `target/r1-logs/v4b.log` records its invocation and verdict. Inspect them;
  validity and capture count alone do not prove attribution. Commit
  `e3759ee3e` redirects Node's V8 log into the run directory; preserve this
  capture-location correction.

Write artifact-level findings for real SDK conversion/transport and server
hot paths, CPU, native volume/retention and V8 volume/retention, Python managed
retained/peak memory, with process/window/phase and exact tool/build identity.
Distinguish Rust SDK from mixed controller/mock-provider costs, inclusive waits
from pure Python CPU and runtime/GC shared costs from exclusive work. Python
managed allocation volume remains a disclosed limit, not a demand for a new
tool. Reuse usable existing captures; if the changed worker model requires new
evidence or an old mandatory domain is unusable, use a targeted diagnostic:

```bash
mise run bench:l1:verification -- --smoke --profile --allow-dirty
```

R1's previous captures can explain its historical runtime, not certify R2's
new reset/suspension lifecycle. Update the R1 evidence ledger during later
implementation with every inherited row's concrete proof path and PASS/FAIL/
UNVERIFIED, separating carried-forward unchanged evidence from rerun changed
seams. Include V3/live phase populations, legacy re-render, V4 findings and
V5 production-isolation evidence. Leave no “pending” row hidden by a task PASS.

Run R1's V1/V5 focused compile-time/package proofs when their owning paths or
feature graphs change, recording their existing exact commands once by
reference to that task. In particular use its isolated production wheel/addon
checks after restoring normal SDK builds; bench artifacts must not leak into
packages. Required changed-owner checks:

```bash
mise run fmt:check
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run codegen:check
mise exec -- git diff --check
```

For changed Python/TS code run the existing format/lint/type checks and
`ts:napi:check` if native declarations change; public docs changes run
`docs:check`. Record exact commands as applicable, not an unrelated broad suite.
Additional touched runtime/Bifrost owners need their narrow changed-selector
regressions; retain existing security, recording and cooperative-pressure proof.
No `gate` or full language journey campaign in this task; integrated change
review owns those. Independent task review must PASS before TASK-003/R1 closeout
is claimed; change completion remains the change-review workflow's job.

## Material Stop Conditions

- A safe cross-product plan, cross-tenant reset isolation or suspended/nested
  progress cannot be proven within these owners: report the exact ownership or
  contract gap before substituting another design. Difficulty alone is not a
  blocker; private placement and equivalent proof remain implementation-owned.
- A solution needs a new dependency/feature, public API, durable schema,
  authorization change, database coordination, Oracle policy change or platform
  exception beyond the proposal: return for the narrow missing authority.
- CPU/child accounting or a mandatory diagnostic domain is unavailable: retain
  evidence as incomplete, never infer zeros or declare a measured target met.
- Qualification misses the measured target, overhead, sample or integrity rule,
  or sustained/two-replica evidence leaks or starves: preserve the failure and
  correct within scope. If a material redesign is needed, report what failed,
  why and the options before starting it; do not chain redesigns or relax gates.
- Clean source/quiet-host state blocks qualification: finish unaffected work
  and record the precise pending prerequisite. No Git action, synthetic host
  load, repeated host-wait loop or dirty-run promotion is authorized.

## Authority Links

- [Approved revision 8](../spec.md), [revision 7](../spec-revisions/revision-7.md)
- [Parent and historical evidence](TASK-003-canonical-sync-verification-benchmark.md)
- [R1 obligations, implementation evidence and exact inherited checks](TASK-003-R1-simplify-benchmark-and-closeout.md)
- [Repository rules](../../../../AGENTS.md), [workflow](../../../spec-driven-development.md), [execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md), [benchmark method](../../../../benchmarks/README.md)
- [Wyrd owner](../../../../architecture/wyrd-design.md#agents-tools-and-workspaces), [Bifrost resource owner](../../../../architecture/bifrost-design.md#admission-and-memory)
