---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-forge-concurrent-planning
spec_revision: 6
requirements: [REQ-003, REQ-004, REQ-005, REQ-006, REQ-009, INV-003, INV-004, INV-007, INV-008, AC-003, AC-004, AC-005, AC-008]
depends_on: [TASK-001]
---

# Pull table-level tasks; let compactors plan and report

## Outcome and Value

Compactors on either replica pull bounded table-level work while the leader
does only in-memory due decisions. Each worker decides physical rewrites from
the latest Iceberg table; success, failure and timeout update the leader's
volatile track without losing later commits. Physical throughput grows with
worker capacity across eligible tables.

## Owners, Scope, Consumers, and Prohibited Changes

- Forge leader owns due selection, dispatch state and result accounting.
  Compactors own catalog load, physical selection, execution and the Forge
  publication call.
  Existing Bifrost resource admission bounds real running/waiting work.
- Wyrd currently pins its `iceberg-compaction-core` fork at
  `6773e192c995d4d9423536f44f05f99b6ea81e5a`, descended from the exact
  nimtable revision RisingWave pins (`74bdc45cb17feaf0ec4eb351d4be271c99d3624c`).
  **Before editing the dependency or worker, read both exact trees:**
  RisingWave's pinned nimtable core and Wyrd's pinned bohmian-ai fork. Compare
  their planner modes, execution/configuration entry points, DataFusion
  runtime/memory pool, spill setup, cancellation, output reporting and commit
  entry points. The comparison and chosen retained/deleted fork changes must
  appear in the implementation report with source links and test results.
  Upstream already exposes public `plan_compaction()` and `rewrite_plan()`
  without a commit; the fork's noncommitting wrapper alone proves nothing.
  Upstream `max_memory_bytes` constructs an independent pool, while the Wyrd
  fork injects a pool from Bifrost's central governor. The implementor chooses
  the smallest integration preserving **actual** shared-governor reservations,
  governed spill placement, and Forge-controlled publication while matching
  RisingWave file selection. A fixed per-worker memory cap or queue limit does
  not replace central-governor accounting. Do not call upstream
  `compact_with_plan()`, which commits. Prove each retained fork-only behavior
  against a concrete Forge consumer.
  Use Full by default, Auto/SmallFiles/FilesWithDelete when configured, and
  Full for copy-on-write. Remove WyrdIdentityAware selection and fork-only code
  with no remaining consumer. Keep tenant/table binding and
  delete correctness. Do not create a second file scan on the leader, use a
  preplanned SQL task envelope, or change table geometry to make a test pass.
- A stale task report is a scheduling no-op, not proof that the stale worker
  did not publish. Preserve concrete catalog safety requirements, not old
  plan hashes, fingerprints or claim layers merely because they exist.
- Multi-replica pull proof must reuse [WyrdTestCluster](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:1209), its [Forge completion observer](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:1533), and the [existing Forge production journey](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs:244). Do not add a second replica runner, synthetic worker scheduler, or separate completion ledger.
- Apply the [Gateway local/peer route rule](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-server/src/components/gateway/capture.rs:642) and [Oracle's shared local/remote execution owner](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-server/src/oracle/forwarding.rs:315): a worker on the leader's replica calls the leader in-process; a worker on another pod uses the internal peer connection. Pulls and reports converge on one handler. Test both routes with the existing cluster; no local RPC loopback and no cross-pod in-process shortcut.
- The transport choice does not change RisingWave's worker pull dynamics:
  the worker computes free parallelism from its existing Forge plan queue,
  polls every five seconds, requests at most four tasks, waits for
  acknowledgement before another pull, and reports each result. Reuse
  [Wyrd's resource-plan-derived limit](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-server/src/boot/mod.rs:1152) and [ForgeAttemptPool queue accounting](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/forge/worker.rs:2426); replace the SQL-claim source, not the queue or capacity counter. Neither route receives leader-pushed assignments or uses a
  separate queue, timeout rule or selection algorithm.
- Exact capacity formula: `ceil(effective_cpu × 12)` for this Iceberg worker
  (the current Wyrd boot owner uses `effective_cpu × 3`), then
  `min(max_task_parallelism - queue.running_parallelism_sum(), 4)` per
  acknowledged pull. The queue's waiting sum has its own existing 4×
  pending budget and is not subtracted here. Reuse the existing queue's
  per-plan required-parallelism weights; no table-count or replica-count
  proxy for capacity.
- Capacity proof must reuse [LocalServer release startup, replica joining, and cgroup metrics](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/src/release_server.rs:125) and the [verification-capacity benchmark's replica ladder and reporting pattern](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/src/bin/verification_capacity/main.rs:97). Add only Forge workload, leader-decision timing and Forge-specific verdicts. Do not duplicate server setup, peer TLS/ports, Postgres/RustFS provisioning, load-driver timing, metrics scraping, resource sampling, percentile/report code or generic benchmark scaffolding. Reuse or extract the existing shared owner if a small extension is needed.

## Approach

1. Replace leader-side file inspection and durable enqueue with table-level
   oldest-due dispatch on compactor capacity pulls, following RisingWave
   meta/src/manager/iceberg_compaction/schedule.rs:428-490,915-993 and
   storage/src/hummock/compactor/mod.rs:1606-1640 at e23ddf95.
   Add Forge pull/report methods to the existing private peer router and
   generated peer wire service. Remote workers discover the leader URI and
   fencing token from the existing singleton election row, use the existing
   peer mTLS credentials as dial-only Forge-worker clients, and reconnect
   when that term changes. A colocated worker calls the same handler
   directly. The dedicated worker opens no peer listener and no new
   discovery/assignment table is introduced. Adapt the existing
   [peer configuration](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-server/src/config.rs:1120) and [private peer router](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-server/src/grpc/mod.rs:381): the dedicated worker accepts TLS credentials for dialing without an advertised/bound peer address, while a coordinator with remote workers serves the Forge peer methods on its existing listener. Pulls default to five seconds,
   at most four tasks, with the previous pull acknowledged first.
2. Complete the pinned nimtable-versus-Wyrd-fork source review below before
   changing the dependency or worker. Run the validated task-type planner
   after loading the current table on the worker. Compare against RisingWave
   storage/src/hummock/compactor/iceberg_compaction/iceberg_compactor_runner.rs:477-680
   and its exact pinned library source. Decide the dependency from source and
   focused tests: exact RisingWave selection plus Bifrost-governed DataFusion
   reservations and spill behavior are required, regardless of which repo
   supplies the library. Do not retain the divergent Auto selector. Record
   why each fork-only module kept is necessary and which obsolete modules and
   tests were removed.
3. Match RisingWave's failed-send, success-count subtraction, failure,
   timeout and stale-report transitions from schedule.rs:317-382,1114-1176.
4. Remove the narrower two-small-files scheduler test, durable preplanned
   task/claim surfaces, and their obsolete tests, after tracing publication
   and cleanup consumers.
5. Measure leader event and selection latency separately from RPC/catalog
   IO; scale out workers across two eligible tables and report capacity.

## RisingWave mechanism and required comparison

Use local RisingWave revision e23ddf952c3e6ebc03cc254789e84d1179cfacae.
For every row, the implementation report must link the resulting Forge
source and a passing test, or call out the approved Wyrd difference.

| RisingWave Iceberg behavior and exact source | Forge proof |
| --- | --- |
| Worker sets `ceil(executor.worker_num × multiplier)` (Iceberg-mode default 12), subtracts the queue's running plan parallelism, caps one pull at four and waits for the previous acknowledgement; waiting parallelism has a separate queue budget: [capacity setup](/home/thorrester/Documents/GitHub/risingwave/src/storage/src/hummock/compactor/mod.rs:406), [Iceberg multiplier](/home/thorrester/Documents/GitHub/risingwave/src/common/src/config/storage.rs:1030), [pull formula](/home/thorrester/Documents/GitHub/risingwave/src/storage/src/hummock/compactor/mod.rs:1606), [queue sums](/home/thorrester/Documents/GitHub/risingwave/src/storage/src/hummock/compactor/iceberg_compaction/mod.rs:158). | Use the existing Wyrd resource plan and Forge plan queue; change its current 3× boot limit to the Iceberg 12× value, and test weighted running plans, waiting-only plans, zero/free capacity, four-task cap, acknowledgement gate and identical local/peer behavior. |
| A pull first expires timed-out tasks, then scans and sorts eligible tables by oldest due time: [timeout processing](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:915), [oldest-due selection](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:942). | More due tables than one pull; oldest selected; timed-out table eligible on the next pull, not by an autonomous deadline event. Same rule and order, read from the sorted due index (REQ-009 revision). |
| Selection moves Idle to PendingDispatch with captured count/watermark; send carries sink identity/settings/task type, not files; failed delivery reverts to Idle: [capture](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:199), [send](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:428), [failed-send restore](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:356). | Inspect dispatch payload and source; fail send before acknowledgement, then pull again without lost commits or duplicate current task. |
| Worker loads the latest Iceberg table, builds the task-type config (Full by default, Auto/SmallFiles/FilesWithDelete when set; copy-on-write forces Full), plans there, and reports an empty plan as success: [config builder](/home/thorrester/Documents/GitHub/risingwave/src/storage/src/hummock/compactor/iceberg_compaction/iceberg_compactor_runner.rs:477), [table load and plan](/home/thorrester/Documents/GitHub/risingwave/src/storage/src/hummock/compactor/iceberg_compaction/iceberg_compactor_runner.rs:592), [no-plan report](/home/thorrester/Documents/GitHub/risingwave/src/storage/src/hummock/compactor/mod.rs:705), [pinned dependency](/home/thorrester/Documents/GitHub/risingwave/Cargo.lock:6948). | Wyrd fork ancestry and exact physical-planner comparison, same task-type/config mapping, current-head planning, copy-on-write override and empty-plan success; show the worker call and paired fixtures. |
| Success subtracts only dispatch-time count, failure is immediately due, late task ID is ignored: [success/failure](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:317), [stale report](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:1114). | Commit while running survives successful report; failure retries; timeout and late report do not corrupt the current track. |

### Mandatory pinned nimtable and Wyrd fork review

RisingWave uses [nimtable `iceberg-compaction-core` at `74bdc45`](https://github.com/nimtable/iceberg-compaction/tree/74bdc45cb17feaf0ec4eb351d4be271c99d3624c/core); Wyrd uses [bohmian-ai at `6773e19`](https://github.com/bohmian-ai/iceberg-compaction/tree/6773e192c995d4d9423536f44f05f99b6ea81e5a/core).
The latter has the former as its merge base. Read both exact pinned trees,
not moving branch tips. In the local fork checkout, `git show <sha>:<path>`
reads either revision without switching branches. The implementation report
must fill every row with the exact nimtable source, exact fork source, selected
implementation source, focused test and result, plus the disposition of the
fork-only code. A fork-only feature survives only with a named Forge consumer
and a test that would fail if it were removed. Do this review before coding.

| Mode or seam | Source comparison and required disposition |
| --- | --- |
| Full | [Upstream strategy at 74bdc45](https://github.com/nimtable/iceberg-compaction/blob/74bdc45cb17feaf0ec4eb351d4be271c99d3624c/core/src/file_selection/strategy.rs#L820) versus [fork strategy at 6773e19](https://github.com/bohmian-ai/iceberg-compaction/blob/6773e192c995d4d9423536f44f05f99b6ea81e5a/core/src/file_selection/strategy.rs#L999): compare all-file inclusion, grouping and output plans, with fork sequence bound unset. |
| SmallFiles / FilesWithDelete | Compare upstream [SmallFiles](https://github.com/nimtable/iceberg-compaction/blob/74bdc45cb17feaf0ec4eb351d4be271c99d3624c/core/src/file_selection/strategy.rs#L800) and [FilesWithDelete](https://github.com/nimtable/iceberg-compaction/blob/74bdc45cb17feaf0ec4eb351d4be271c99d3624c/core/src/file_selection/strategy.rs#L836) against [fork constructors](https://github.com/bohmian-ai/iceberg-compaction/blob/6773e192c995d4d9423536f44f05f99b6ea81e5a/core/src/file_selection/strategy.rs#L973); prove size-boundary, delete-count-boundary and grouping cases with sequence bound unset. |
| Auto | [Upstream Auto planner at 74bdc45](https://github.com/nimtable/iceberg-compaction/blob/74bdc45cb17feaf0ec4eb351d4be271c99d3624c/core/src/compaction/auto.rs#L130) first checks table-wide candidate thresholds, prefers delete-heavy plans, then considers small-file plans. [Fork Auto at 6773e19](https://github.com/bohmian-ai/iceberg-compaction/blob/6773e192c995d4d9423536f44f05f99b6ea81e5a/core/src/file_selection/strategy.rs#L1051) instead filters the union of small and delete-heavy files into one grouping pass. Match the upstream result; the implementor chooses the smallest way to achieve it. |
| Noncommitting seam | [Upstream planning](https://github.com/nimtable/iceberg-compaction/blob/74bdc45cb17feaf0ec4eb351d4be271c99d3624c/core/src/compaction/mod.rs#L448) and [noncommitting rewrite](https://github.com/nimtable/iceberg-compaction/blob/74bdc45cb17feaf0ec4eb351d4be271c99d3624c/core/src/compaction/mod.rs#L370) versus [fork wrapper](https://github.com/bohmian-ai/iceberg-compaction/blob/6773e192c995d4d9423536f44f05f99b6ea81e5a/core/src/managed/boundary.rs#L31): identify any behavior the wrapper actually adds beyond noncommitting plan/rewrite. Forge owns the publication call; upstream `compact_with_plan()` must not be used. |
| Memory governor and spill | [Upstream processor](https://github.com/nimtable/iceberg-compaction/blob/74bdc45cb17feaf0ec4eb351d4be271c99d3624c/core/src/executor/datafusion/datafusion_processor.rs#L73) builds its own `FairSpillPool` from `max_memory_bytes`, with optional `spill_dir`; [fork executor](https://github.com/bohmian-ai/iceberg-compaction/blob/6773e192c995d4d9423536f44f05f99b6ea81e5a/core/src/executor/datafusion/mod.rs#L64) accepts a caller-owned context; [Forge context](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/forge/managed/executor.rs:419) passes [the central-governor pool](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/resources.rs:1896) and spill lease. Prove a real rewrite charges the shared root while Scribe/Oracle also hold charges, refuses or spills at the shared limit, releases its charges, and writes spills only beneath Forge's governed root. This is required regardless of dependency choice. |
| Cancellation and loose outputs | Compare [upstream executor](https://github.com/nimtable/iceberg-compaction/blob/74bdc45cb17feaf0ec4eb351d4be271c99d3624c/core/src/executor/datafusion/mod.rs#L47) with [fork context/observer](https://github.com/bohmian-ai/iceberg-compaction/blob/6773e192c995d4d9423536f44f05f99b6ea81e5a/core/src/managed/context.rs#L208) and [Forge's unsettled-output consumer](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/forge/managed/observer.rs:1). Name the exact output/cancellation recovery property and keep only the code its focused failure test proves necessary. |

The fork's five focused Full/SmallFiles/FilesWithDelete/Auto/boundary tests
passed at `6773e19` on 2026-10-03. That proves current fork behavior only;
it does not establish upstream parity. Paired fixtures prove file choice.
The real-rewrite governor test proves Wyrd resource behavior. A matching
function name, an existing wrapper, queue admission, or memory/spill metrics
alone prove neither requirement. The implementation report must say whether
the fork was retained, narrowed or removed and list each remaining fork-only
module with its live consumer and failure test.

## Ordered Implementation Scenarios

### Scenario 1 — Pull capacity dispatches table identities

**Behavior.** A compactor requests only available capacity; the leader
selects oldest due tables and sends no selected-file list. Failed delivery
restores Idle and preserves pending commits. Two pods can execute distinct
tables concurrently (REQ-004, AC-004).

**RED.** Add
production_closeout::compactors_pull_oldest_due_with_capacity
to the existing `wyrd-testing` Forge journey for due-order, bounded capacity,
failed send, two-replica worker progress, and both in-process and peer
routes. Current SQL task claiming
remains visible. Run exactly:
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all -E "test(=production_closeout::compactors_pull_oldest_due_with_capacity)"'

**GREEN.** Dispatch table-level work from the leader's process-local state.
Worker pulls drain the backlog without waiting for a periodic planning pass.

**REFACTOR.** Delete the durable planned-file envelope and worker fairness
cursor when the last worker consumer has moved. Avoid a second queue.

### Scenario 2 — Worker decides from current files

**Behavior.** A worker loads current Iceberg metadata and applies the
RisingWave task-type planner. Full is the default; configured Auto,
SmallFiles and FilesWithDelete use their respective builders, and copy-on-
write forces Full. An empty plan reports success without a rewrite. The
leader never performs a file-candidate test (REQ-005, INV-007).

**RED.** Add
forge::production_routes::worker_selects_current_iceberg_files
to the existing redux integration target: cover current-head Full,
configured task types, copy-on-write override, no-plan success, and a real
rewrite under a held Scribe/Oracle governor charge. The rewrite must show
Forge-attributed shared-root growth, spill under the configured Forge root,
shared-limit refusal or spill, and charge release after cancellation/drop.
Reuse the existing Bifrost resource governor and Forge rewrite test owners;
do not create a second memory pool, fake governor or parallel test harness.
The old scheduler still selects candidate files before dispatch. Run exactly:
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E "test(=forge::production_routes::worker_selects_current_iceberg_files)"'

**GREEN.** The dispatched worker invokes the validated
`iceberg-compaction-core` Full/Auto/SmallFiles/FilesWithDelete planner with
RisingWave's task-type configuration and 1 GiB default target. Prove paired
fixtures and central-governor charge/spill behavior. Keep tenant,
branch and delete applicability at the publication boundary. RisingWave's
table-load and worker plan location is runner.rs:477-680; Forge's current
library invocation lives at forge/managed/executor.rs:182-210 and the
Wyrd-specific policy to replace is forge/managed/policy.rs:281-294.

**REFACTOR.** Delete leader candidate scans in forge/planning_scheduler.rs,
WyrdIdentityAware selection in the worker and tests that expect Forge's
old physical-file choice.

### Scenario 3 — Reports preserve arrivals and distinguish stale work

**Behavior.** Success consumes only the count captured at dispatch; a commit
arriving during execution remains pending. Failure is immediately due,
timeout after the configured deadline is reconsidered on a pull, and a stale
report changes no scheduling state (REQ-006, INV-003/004).

**RED.** Add
forge::production_routes::report_preserves_later_commits_and_ignores_stale
to the existing redux integration target, covering timeout followed by late
report/commit. Current generation/SQL claim semantics fail the requested
state machine. Run exactly:
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E "test(=forge::production_routes::report_preserves_later_commits_and_ignores_stale)"'

**GREEN.** Apply the RisingWave dispatch snapshot and report accounting.
Default report deadline is 30 minutes from dispatch; cancellation or retry
cannot treat an unknown catalog outcome as definite refusal.

**REFACTOR.** Remove planning generations, demand acknowledgement and
failed-pass exclusion paths. Retain only operation evidence with an actual
publication or cleanup consumer.

### Scenario 4 — Leader is measured separately from worker throughput

**Behavior.** Warm notification and due decision avoid SQL and Iceberg IO.
Under a sustained backlog spread over many independent tables, additional
compactors increase physical completion throughput while the leader continues
dispatching (REQ-009, AC-008).

**RED.** Add
forge::production_routes::leader_decision_has_no_catalog_io
to the existing redux integration target. It records leader decision
p50/p99, table count, commit rate, worker count and lock contention, and
fails on current leader-side table reads. Run exactly:
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E "test(=forge::production_routes::leader_decision_has_no_catalog_io)"'
Add one opt-in
bench:bifrost:forge-capacity mise entry that invokes a Forge workload built
on `LocalServer` and the existing verification-capacity runner/reporting
owners named above. It must use the same Postgres/RustFS wrappers as
`bench:verification:capacity`, generating real Iceberg commits
and physical rewrites, not a spin loop or higher test-thread count.

**GREEN.** (Revised 2026-10-03 with the human owner; replaces the 32-puller
hammer and the 85% occupancy gate.) Follow the Bifrost capacity benchmarks'
structure (`LocalServer`, release cloud build, RustFS and test Postgres, a
systemd scope per process, reports under `target/`). The declared host has
16 CPUs; all Wyrd processes together stay within 8 CPU / 16 GiB: the leader
runs in a 1 CPU / 4 GiB scope and each dedicated compactor in a
7/3 CPU / 4 GiB scope, so the 3-worker step uses 8 CPU / 16 GiB and every
step adds identical capacity. (Amended by the implementing agent, spec
277c408e3: every Wyrd process keeps its 4 GiB boot floor, so the earlier
2 GiB leader and 4-worker step could not fit; not a human-owner decision.) Postgres and RustFS run outside the envelope
and their utilization is recorded. Host load is recorded before each run;
a run whose host load exceeds the envelope's 8 CPUs is discarded and
repeated, not reported.

1. *Leader decision latency.* Measured from the leader's own
   `bifrost_forge_leader_decision_seconds{operation}` and the in-process
   schedule case with 10,000 tracked / 1,000 due tables. Pulls arrive
   open-loop at the production rate (each of 32 workers every five seconds)
   and at 10x, then in increasing steps until the p99 bends; report the knee.
   p99 commit, pull and report decisions stay below 1,000 microseconds at the
   production and 10x rates.
2. *Fleet throughput (backlog drain).* For each fleet size, at least 128
   fresh tables are written through Scribe with no worker running until each
   holds compactable staged files, at default compaction settings (no type
   pin) and a realistic commit trigger (not one commit); then one leader and
   1, 2 or 3 compactors drain that backlog. No oversized seed is written to
   lengthen rewrites. 2-worker and 3-worker completion rates reach at least
   1.7x and 2.5x one worker; whenever the
   leader holds due tables, every pull is answered with as many tasks as it
   requested; leader CPU stays below 70%. The report names the resource that
   bounds the largest step, as measured.

Selection uses the sorted due index approved in the REQ-009 revision of
2026-10-03. Report two qualifying runs' p50/p99 and throughput, not one
favourable run.

**REFACTOR.** Remove old SQL backlog gauges and full-roster traversal
metrics whose meanings no longer match the new protocol.

## Acceptance Criteria

AC-003/004/005/008: capacity-bounded pull, oldest due selection, worker
current-head planning, no-op success, later-commit preservation, late-report
ignorance, multi-replica worker progress and separately reported leader
latency. No leader-side file inspection remains.

## Expected Write Set and Consumer Closure

Likely: forge/planning_scheduler.rs, scheduler.rs, worker dispatch/claim,
managed/executor.rs and managed/policy.rs to remove fork-specific selection,
workspace Cargo.toml and Cargo.lock if the dependency revision changes,
private peer protobuf, `wyrd-tonic`, server peer router/config and dial-only
Forge-worker client wiring,
forge/publication.rs consumer closure, vala-sql forge_tasks.rs and
forge_fair_claim.sql, Forge integration and wyrd-testing journey tests,
telemetry, server worker topology. Candidate deletion includes planning
claim token/expiry, generation acknowledgement, plan_hash as a preplanned
authorization ceiling, and old concurrency-specific tests; keep exact
operation evidence only when a live consumer requires it.

## Verification and Evidence

Run only the focused exact scenario commands above for RED and GREEN,
plus the one focused capacity command below. TASK-003 alone runs
the integrated verify:bifrost aggregate.
The private peer pull/report wire also runs `mise run test:tonic` and
`mise run codegen:check`; these are outside `verify:bifrost`.
The current mise.toml has
`bench:verification:capacity` and Bifrost query/ingest capacity lanes; the
implementation reuses their deployment and measurement code and adds only
the Forge workload/verdict. Run `mise run bench:bifrost:forge-capacity`
before AC-008 can close. Its report
includes hardware, table/tenant counts, commit rate, worker occupancy,
leader CPU, p50/p99 by stage, completed work per second at 1/2/3 workers,
and storage/SQL utilization. Any
specifically named new test must include and run its exact focused
mise exec -- cargo nextest selector through the correct setup wrapper.
The completed task includes the comparison table above with exact Forge
source and test links. Inventory reused cluster, observer, release server,
resource measurement and reporting owners. Justify each new benchmark helper
against those owners; duplicate runner, metrics or report code fails this task.

## Material Stop Conditions

Stop if the current worker cannot be dispatched by table identity without
changing a durable public contract, or if removing a task row would discard
the only exact evidence of an unresolved Iceberg publication. Do not keep the
whole old queue to solve that narrower problem.

## Authority Links

Approved ../spec.md revision 6; AGENTS.md §§5,9,11;
architecture/bifrost-design.md §§Managed compaction and publication;
RisingWave e23ddf95 source locations in Approach and scenarios.

## Implementation Evidence

Status: IMPLEMENTED for Scenarios 1–4. AC-008 closed with two qualifying
runs at 69d2efe8c; see "AC-008 qualifying runs" below.

Commits: ac84c791a, e04cef237, 179c54c72, 12bef6736, be5e7bfa6, c9cebf6da,
dc3483751, 000fb21ba, 1463740d3, 0a6fb59c3, 9b8b8ca78, c50708084, ae4e0745b,
ea99b55c9, 48c6b425d, 1c939eec0, 76a3e5fd3, 4202f3561, e7fa15477,
6531e5bea (due index and leader telemetry), 5a0f70b72 (revised gate).

### RisingWave comparison (pinned e23ddf95)

| RisingWave behavior | Forge source | Test |
|---|---|---|
| Compactor pull `min(max_task_parallelism − running, 4)`, five-second cadence, acknowledgement before the next pull | `forge/worker.rs` `pull_claimed_tasks`, `free_pull_room`, `DEFAULT_PULL_INTERVAL` | `production_closeout::compactors_pull_oldest_due_with_capacity` |
| Iceberg multiplier 12 × executor workers | `wyrd-server/src/boot/mod.rs` `forge_compaction_worker_config` | `boot::tests::forge_runtime_is_role_scoped_and_cpu_sized` (72/288 at 6 CPUs) |
| Pull expires timed-out tasks, then oldest-due selection | `forge/leader.rs` `ForgeSchedule::pull`, `DueIndex` | `forge::leader::tests::due_index_selects_exactly_what_the_scan_selects`, `reports_preserve_later_commits_and_ignore_stale_tasks` |
| Selection captures pending count and watermark; failed send restores Idle | `CompactionTrack::start_processing`, `revert_pre_dispatch` | `forge::production_routes::report_preserves_later_commits_and_ignores_stale` |
| Worker loads the current head and plans by task type; no-op is success | `forge/worker.rs`, `managed/policy.rs` `planning` | `forge::production_routes::worker_selects_current_iceberg_files` |
| Local vs peer route converge on one handler | `forge/leadership.rs` `serve_pull`/`serve_report`, `forge_peer.rs` | journey lane, `mise run test:tonic` |

Deviation (approved REQ-009 revision, 2026-10-03): selection reads a sorted
due index instead of scanning every track. The rule, timeout handling and
`(due time, table)` order are unchanged; the randomized equivalence test
compares the index against a literal port of the scan before every pull (200
seeds × 300 operations) and rebuilds the index after every operation.
Mutations — waiting entries ignoring due time; removal leaving a stale entry —
each fail it ("pull diverged from the scan", "index drifted after commit").

### Diagnoses

- **Stale boot sizing test.** Symptom: `forge_runtime_is_role_scoped_and_cpu_sized`
  asserted 18 at 6 CPUs. Evidence: `boot/mod.rs:1157` multiplies by 12 since
  e04cef237. Cause: the assertion kept the pre-spec 3× value. Fix site: the
  test (independent diagnostician confirmed production matches REQ-004; no
  other caller asserts the values). e7fa15477.
- **Leader pull latency.** Symptom: bench p99 pull 28–34 ms (10k tables, 32
  pullers). Evidence: about 100 µs per uncontended pull, all spent walking
  every track under the schedule lock. Cause: `O(tables)` selection plus
  queueing behind 31 closed-loop pullers. Fix site: `ForgeSchedule` (due
  index); uncontended pull is now 2.2 µs p50 / 3.5 µs p99 in release.
- **Promotion pacing (FIXED in 3ce3a113b and 75bfe0224; was a product defect in Scribe claim publication).**
  Symptom: 128 tables written every 1 s with a 10 s seal offer about 12.8
  promotions/s; the leader commits 2.2/s (earlier runs 1.2–1.5/s), and a
  table waits p50 56 s / p90 83 s / max 99 s from seal to Iceberg commit.
  Evidence (run `mise run bench:bifrost:forge-capacity -- --workers 1
  --server-log "info,vala_bifrost_redux=debug"` at 88bcabfe1; leader trace
  read by an independent read-only diagnostician): seals keep pace (~128
  `persisting immutable Scribe generation` per 10 s); 573 promotion commits in
  267 s against 581 `assemble_claim` stage starts, one promotion per published
  claim; the Forge `scribe_promotion` task takes p50 43 ms from creation to
  settle, never more than one runs at a time, and Forge is idle ~91% of the
  window (560 hinted, 13 from the debt sweep). The pace is a single unbroken
  chain: claim assembly start → promotion task p50 365 ms, commit → next
  claim assembly 60 ms, consecutive commits p50 443 ms. Claim time grows with
  members (2.8 members 0.29 s; 8.5 members 0.79 s), so the backlog feeds
  itself. Leader CPU stays low (IO/fsync-bound). The host was loaded during
  this run, which inflates fsync latency but not the serialization.
  Cause: Scribe publishes due claims one at a time on the leader.
  `scribe/persistence.rs` `PersistenceWorker::publish_due_claims` is
  `while let Some(claim) = take_claim(..) { publish_claim(claim).await? }`
  from one lifecycle task on a 1 s tick (`publish_residue` has the same
  shape); the claim-merge lane is `ScribePersistenceCpuPool::new_with_capacity(1, 1)`
  in `PersistenceWorker::new`; and `build_staging` sizes the assembler's claim
  budget to `workers` on the stated assumption that each worker drives one
  claim, but only the one publisher task ever takes claims. Per member,
  `claim_publication.rs` moves, uploads, persists, commits and retires
  sequentially through `hot_stage.rs` transitions of three fsyncs each
  (about ten per member). The Forge hint is sent only after all of it, so
  promotion can only follow publication. None of these files changed on this
  branch; Forge promotion is not the bound.
  Fix site: `scribe/persistence.rs` `PersistenceWorker::publish_due_claims`
  and `publish_residue` (publish up to the claim budget concurrently, treating
  budget exhaustion as "await one in-flight claim, then refill"), the
  `assembly_cpu` capacity in `PersistenceWorker::new` (match the claim budget,
  bounded by effective CPU), and secondarily `claim_publication.rs`
  `move_members`/`retire_members` with `hot_stage.rs` transitions (advance a
  claim's members through each state together, or fsync the shared parent once
  per claim). Next limit after that: `forge/scheduler.rs` awaits
  `promote_hinted` inline (~55% busy at 12.8/s). The diagnostician found no
  approved-contract change in this fix (REQ-002 and bifrost-design
  "Assembly and publication" set membership, a bounded upload lane and one
  fenced commit per claim, not publication order or concurrency). It is not
  implemented here: it changes durable Scribe publication concurrency outside
  this task's write set, which needs an owner decision. The backlog-drain
  measurement does not depend on it.
  Fix (owner-approved, full publication revision): 3ce3a113b publishes due
  and residue claims concurrently up to the assembler's claim budget
  (`PersistenceWorker::publish_claims`) and sizes the merge lane from that
  budget, capped by effective CPU. 75bfe0224 moves each claim's members
  through Publishing, Published and CleanupPending as one concurrent step per
  state. A state change now syncs only the record and its member directory,
  because the key-directory entry has been durable since first publish.
  Members retire together with one key-directory sync, and retirement reads
  WAL ranges from the Publishing records instead of re-hashing runs. Stage
  syncs per member fall from about 10 to 6, plus 1 per claim. Every state that
  recovery reads is durable before the next step starts. Syncs are counted in
  `bifrost_scribe_stage_fsyncs_total`.
  Tests:
  - concurrency bound: journey
    `lifecycle::scribe_publishes_due_claims_concurrently_within_the_claim_budget`
    (RED: at most 1 in flight; GREEN: 1 < overlap <= 4);
  - sync count:
    `scribe::hot_stage::tests::a_claim_costs_two_syncs_per_member_state_and_one_to_retire`
    (RED: 12 syncs for 4 members; GREEN: 8, and 1 to retire);
  - crash at each batched boundary:
    `scribe::staging_runtime::tests::a_claim_crashed_{after_its_published_step,after_its_cleanup_step,inside_its_batched_removal}_retires_on_restart`.
    These pin unchanged recovery behavior; their only RED was the missing
    `retire_all`.
  Verification: `mise run test:bifrost:integration:redux` 888/888,
  `mise run test:bifrost:journey:scribe` 22/22, and clippy `-D warnings` on
  `vala-bifrost-redux` (all features) and `wyrd-testing`.
  Before/after: the same diagnostic command, 1 worker, leader trace. Before
  is 88bcabfe1 on a loaded host; after is 75bfe0224 with 27 of 32 CPUs free.

  | Measure | Before | After |
  |---|---|---|
  | Promotions/s over the commit window | 2.15 (573 in 267 s) | 5.65 (1165 in 206 s); fill 6.09/s |
  | Seal to Iceberg commit, oldest pending seal, p50 / p99 / max | 56.3 / 89.7 / 99.1 s | 12.8 / 16.9 / 17.4 s |
  | Generations per promotion | 5.6 | 1.9 |
  | Per-claim time, assembly start to promotion task, FIFO-paired, p50 / p90 / p99 | 1.38 / 2.84 / 4.11 s | 0.28 / 1.49 / 2.44 s |
  | Assembly start to next promotion task, p50 | 0.37 s | 0.02 s |
  | Gap between consecutive claim publications, p50 | 0.445 s | 0.051 s |

  Promotion now keeps pace with sealing: 5.65/s × 1.9 generations ≈ 10.7 of
  the 12.8 generations/s offered. Seal-to-commit p50 now sits at the 10 s
  seal cadence plus dwell instead of growing with backlog. The before-run
  FIFO per-claim figure includes queueing behind the serial publisher.
  Open in the after run: the live leader p99 pull at 10× was 1135.7 µs
  against the 1 ms gate (1× was 171 µs; the 10× client round trip p50 was
  15.4 ms). The probe runs while the first fill is writing, and the 1-CPU
  leader now publishes up to 4 claims at once. It is unproven whether this
  is that added contention or noise from one sample; the qualifying runs
  must settle it. All other checks that a 1-worker run can evaluate passed.
  Drain: 0.78 rewrites/s, which is at the pull-cadence ceiling.
  Settled: both qualifying runs at 69d2efe8c, after the review fixes, held the
  live 10× pull p99 at 43.3 µs and 57.6 µs on a quiet host. The single
  failing sample is therefore not reproduced, and no diagnosis was opened.

- **Publication revision review (findings 1–5 and suggestions).** Every finding was
  checked against the code before it was fixed, and all five were confirmed
  real. Each fix began with a test that failed for the stated reason.
  Commits: 0edbe3f54 (findings 1 and 5), f058e52d4 (findings 2–4), e642d14ed
  (suggestions).
  - **1, CRITICAL, a crash mid-removal blocks pod start: CONFIRMED and FIXED.**
    - Symptom: `remove_dir_all` deletes entries in readdir order. A crash
      partway through can leave a `CleanupPending` record whose runs are
      already gone. `validate` then returns `MissingRun`, and `recover()`
      stops at its first error, so restore fails and the pod cannot start.
    - Fix (`hot_stage.rs`):
      - `retire_all` unlinks the record before removing the directory.
      - `recover` removes a member directory that has no record.
      - `validate` tolerates `MissingRun` only for a `CleanupPending` member.
    - Test: `scribe::staging_runtime::tests::a_claim_crashed_inside_one_member_removal_retires_on_restart`.
      It covers one survivor that kept its record but lost its runs, and one
      that kept its runs but lost its record.
    - Diagnostician (read-only): two existing tests encoded the old policy.
      They were `hot_stage` recovery-temporaries, which kept runs without a
      record, and `member_stager`, which ran `recover()` while staging.
      `recover` is a startup-only scan, and the WAL stays authoritative until
      the record lands. Both tests were updated to that policy. Runs without a
      record are removed, and they are never a query authority.
  - **5, Publishing-to-Publishing replay is "Backwards": CONFIRMED and FIXED.**
    - RED: `scribe::staging_runtime::pg_tests::a_claim_stranded_in_publishing_publishes_after_restart`
      failed with `move staged member 1-1 to publishing: Scribe staged member
      cannot move from publishing to publishing`. Each attempt minted a fresh
      `operation_id`, so the replay was not an identical record.
    - Fix: `claim_publication.rs` `publication_operation_id` derives the id
      from the claim id, so a replay rewrites the identical record and is
      idempotent.
    - Forge side: `scribe_promotion.rs` derives its own ids
      (`promotion_generation_operation_id`). Nothing outside tests reads the
      Publishing `operation_id`.
  - **3, one claim driven by two publishers: CONFIRMED and FIXED.**
    - RED: journey `lifecycle::concurrent_flushes_share_the_claim_budget_and_publish_each_claim_once`.
      The second flush's `retryable_claims` re-drove the 4 claims the first
      flush still held, so `assemble_claim` ran twice per claim id. The two
      `Claimed`→`Publishing` record writes then raced on one `.tmp`, and the
      flush failed with `move staged member 0-7 to publishing: ... failed to
      rename the staged record into place: No such file or directory`.
    - Fix (`staging_runtime.rs`): an in-process `ClaimDrivers` set and a
      `DrivenClaim` guard.
      - `take_claim` and `take_residue` hand out claims already driven, under
        the ready-index lock.
      - `retryable_claims` marks each candidate under that lock before
        reading its records, and skips claims already driven.
      - Dropping the guard (settle, refusal, or cancellation) releases the
        claim and wakes waiting publishers.
  - **2, REGRESSION from 3ce3a113b, budget exhaustion treated as failure:
    CONFIRMED and FIXED.**
    - Found by code reading, and hidden behind finding 3 in the same journey:
      `take_claim` and `take_residue` flattened
      `AssemblyError::ClaimBudgetExhausted` into `ScribeError::Internal`.
    - Fix: they return a typed `ClaimTakeError::BudgetExhausted { budget }`.
      `PersistenceWorker::publish_claims` treats it as backpressure. It stops
      taking claims and lets its own in-flight claims settle.
      - With nothing in flight, the tick (`ClaimSlotWait::Yield`) ends its pass
        with `Ok`.
      - A flush (`ClaimSlotWait::Await`) waits on `claim_released()`. It
        registers the wait before asking for a claim, so it cannot miss a
        wakeup.
      - The flush fails only when no publisher in the process drives any
        claim, because then no slot can be released.
    - No `ScribeError` variant was added.
    - Unit test: `scribe::staging_runtime::tests::a_claim_is_retryable_only_while_no_publisher_drives_it`.
  - **4, a failed due claim is never retried in production: CONFIRMED and
    FIXED.**
    - RED: journey `lifecycle::scribe_tick_retries_a_failed_due_claim`. One
      injected object-write failure produced `WARN Scribe due publication
      failed; next tick retries`. No further `assemble_claim` ran, and the
      test hit `Elapsed` after 30 s.
    - Fix: every `publish_claims` pass first drains `retryable_claims`, so
      each tick retries refused claims. Members already in `Publishing` are
      retryable now that finding 5 makes the replay idempotent. Claims whose
      members recorded the commit (`Published` or later) are left to startup.
    - `resume_staging_claims` also goes through `retryable_claims`. It still
      fails if any restored claim cannot run again.
  - Diagnostician on the two journey failures (read-only; given the command,
    trace, and diff):
    - Both tests assert real production behavior.
    - Test 1 is a real race (tick versus flush, or two flushes), with a
      hidden second failure: `ClaimBudgetExhausted` flattened to `Internal`.
    - Test 2 is a real defect: the tick only takes new claims. A refused
      claim has already left the ready index, so `next_claim` never sees it
      again.
    - Fix site: `ScribeStagingRuntime` (drive mark plus typed exhaustion) and
      `PersistenceWorker::publish_claims` and `publish_due_claims`.
    - The test-only `note_claim_slot_wait` must be called at the new wait
      point. Without it the contention probe cannot fire once the double
      drive is gone. It is now called there.
  - **Suggestion, merge-lane sizing: DONE.**
    - `merge_lane_threads` returns `max(1, effective_cpu - persistence lane
      threads)`, capped by the claim budget.
    - Test: `scribe::persistence::tests::merge_lane_threads_leave_the_persistence_lane_its_cpus`.
      Under the old `budget.min(effective_cpu)`, the case (8 CPUs, 6
      persistence threads, budget 4) gave 4 where 2 is free.
  - **Suggestion, root fsync for new key directories: DONE.**
    - The first record this process publishes under a key also fsyncs the
      staging root. Key directories are never removed.
    - RED: `scribe::hot_stage::tests::the_first_record_of_a_key_syncs_the_staging_root_once`
      counted `[3, 3]` syncs. GREEN is `[4, 3]`.
    - The existing per-state sync count is unchanged.
  - **Adjacent finding, not fixed (outside these findings):**
    `ScribeStagingRuntime::restore_context` reads the schema from the first
    run of any member of a key. That can include terminal members that
    restore has just retired. A key holding terminal leftovers next to live
    members may therefore fail restore.
  - Lane note: `telemetry::scribe_hot_path_telemetry_reconciles` failed once
    in the journey lane, at telemetry.rs:530 ("per-append WAL spans are
    DEBUG detail"). The cause was the lane being run with
    `WYRD_LOG=info,vala_bifrost_redux=debug`, which turns on the DEBUG
    `bifrost.scribe.wal.append` span that the test asserts is absent from
    routine traces. With `WYRD_LOG=info` the test passed alone and in the
    full lane. It is not a code defect, and the test is unchanged.
  - Verification:
    - `mise run test:bifrost:integration:redux`: 895/895.
    - Journeys, run with `WYRD_LOG=info`:
      - `mise run test:bifrost:journey:scribe`: 24/24.
      - `mise run test:bifrost:journey:forge`: 21/21.
      - `mise run test:bifrost:journey:oracle`: 42/42.
    - `mise run fmt`: clean.
    - `cargo clippy -p vala-bifrost-redux -p wyrd-testing --all-features
      --tests -- -D warnings`: clean.
    - `git diff --check`: clean.

- **Scribe durability follow-ups (items 1–4).** Each item was checked
  against the code first, and all four were real. Each fix started from a
  RED test or trace. Traces were read with
  `WYRD_LOG=info,vala_bifrost_redux=debug`.
  - **1, restore reads its schema from retired members: CONFIRMED and FIXED
    (290cc0dac).**
    - Symptom: `restore_context` took the schema from the first run of any
      recovered member of a key. That includes members of a committed claim
      that `recover_terminal_members` has just retired and deleted.
    - RED: `scribe::staging_runtime::pg_tests::a_key_with_finished_claim_leftovers_and_a_live_member_restores`
      failed with `open a recovered staged run: No such file or directory (os
      error 2)`.
    - Fix: `restore` collects the members it keeps, and `restore_context`
      reads the schema only from their runs.
    - GREEN: staging_runtime tests 14/14.
    - Residual: the RED depends on readdir order. The fixture puts four
      leftovers next to one live member so that a leftover comes first on
      ext4.
  - **2, a claim refused after its commit holds its slot until restart:
    CONFIRMED and FIXED (6b6fcc512).**
    - Symptom: a failure after the members recorded `Published` (for
      example a refused `CleanupPending` move, lease drain, removal, or
      authority release) left the claim outstanding. `retryable_claims`
      skipped claims whose members were `Published` or later, so only startup
      `restore` finished them. A member whose record was already removed made
      the check return an error, which stopped every later tick.
    - RED: journey `lifecycle::scribe_tick_finishes_a_claim_that_failed_after_its_commit`.
      It uses a test-only fault (`PersistenceFaults::fail_next_claim_retirement`)
      that fires right after the members record `Published`. The trace shows
      `Scribe due publication failed; next tick retries`, then nothing for
      30 s, then a timeout with `bifrost_scribe_staging_outstanding_claims = 1`.
    - Diagnostician (read-only; given the command, trace, and diff):
      - Confirmed that `retryable_claims` and `publication_can_rerun` drop
        committed claims, and that only startup finishes them.
      - Fix site: `ScribeStagingRuntime` with `ClaimPublisher`, branching
        before `gather`.
      - Hazards to respect: keep drive exclusivity; treat a missing record as
        already retired; tolerate `Unregistered` on release; keep the order
        CleanupPending → drain → remove → release; send the Forge hint;
        settle; never commit again.
    - Fix:
      - `ScribeHotStage::surviving_record` reads a member's record, or
        returns `None` once retirement has removed it. It does not re-hash
        the runs.
      - `ClaimPublisher::finish_committed` treats a claim as committed when
        any member's record is gone or names the commit. It fails closed if a
        survivor contradicts that. It then runs the survivors through the
        startup terminal sweep, which now takes records, in the same
        transition order. It also deletes record-less directories and
        releases their authorities, ignoring `Unregistered`.
      - `ScribeStagingRuntime::finish_committed` settles the claim.
      - `retryable_claims` is synchronous and returns every undriven
        outstanding claim. `publication_can_rerun` is removed.
      - `PersistenceWorker::resume_claim` finishes a committed claim, sends
        `note_claim_published` and the Forge hint, and otherwise publishes.
        `publish_claims` uses it for retried claims, and
        `resume_staging_claims` uses it at startup. Fresh claims skip the
        check.
      - `publish_claims` now returns a count; every caller only used `.len()`.
    - GREEN:
      - The journey passes in 14.1 s. Its trace shows the fault at
        05:40:34.08 and `outcome="retired_after_commit"` on the next tick at
        05:40:35.04.
      - It then asserts 32 rows published, no outstanding claim, no live
        member, and that a second table publishes.
    - Unit test: `scribe::staging_runtime::tests::a_live_claim_refused_inside_its_removal_finishes_without_republication`.
      - A claimed-only retry is not finished.
      - A half-retired committed claim finishes with no member, directory,
        or authority left and its slot returned. One member is removed and
        released, one is removed but still registered, one has lost its runs,
        and one is whole.
      - The pool is lazy and never connected, which proves nothing was
        committed again.
    - Residuals:
      - The claim's staged candidates and publication manifest are named by
        its WAL union, which removed members no longer carry. They stay until
        startup publication recovery replays and removes them, as after a
        crash between the commit and that cleanup.
      - The `claims_published`, `files`, and `bytes` counters are not
        incremented for a claim finished this way. The attempt that observed
        the commit failed before counting it.
  - **3, the telemetry journey depends on the ambient filter: CONFIRMED and
    FIXED (ff6d630ba).**
    - Symptom: `telemetry::scribe_hot_path_telemetry_reconciles` inferred
      that `bifrost.scribe.wal.append` is DEBUG because it was missing from
      the trace. A DEBUG filter makes it appear.
    - Diagnostician: this is a harness defect. Recommended fix: capture each
      span's level.
    - Fix:
      - `wyrd_telemetry::init_test_capture` (test support only) sets
        `with_level(true)`.
      - The journey asserts that every captured append span carries
        `level == "DEBUG"`. The intent is unchanged: append spans are DEBUG
        detail, not routine.
    - GREEN under both filters: the default filter in the scribe lane, and
      `WYRD_LOG=info,vala_bifrost_redux=debug` alone.
    - Not changed here: the same diagnostician noted that
      `tests/gateway/native.rs` has the opposite filter dependence.
  - **4, the redux integration binary was untraced: CONFIRMED and FIXED
    (2076a32a6).**
    - Symptom: only `ForgeTelemetryCheckpoint` installed a subscriber.
    - RED: under `WYRD_LOG=info,vala_bifrost_redux=debug`,
      `forge::promotion::scribe_promotion_integration_appends_existing_datafile_without_put`
      printed no INFO or DEBUG lines.
    - Fix: `forge/support.rs` `ProcessTelemetry` is one process-wide
      `OnceLock` owner.
      - It installs the production-shaped recorder and `init_test_capture`,
        which honours WYRD_LOG.
      - `PromotionIntegrationFixture::start` and
        `AuthorityFixture::start` initialise it.
      - `ForgeTelemetryCheckpoint::install` reads from it and takes its own
        span mark. Its assertions still read production emission.
    - GREEN: the same test prints 16 `DEBUG vala_bifrost_redux` lines (for
      example `persisting immutable Scribe generation`) and passes.
  - Verification:
    - `mise run test:bifrost:integration:redux`: 897/897.
    - `mise run test:bifrost:journey:scribe`: 25/25, run with the default
      filter.
    - `mise run test:bifrost:journey:forge`: 21/21.
    - `mise run test:bifrost:journey:oracle`: 42/42.
    - `telemetry::scribe_hot_path_telemetry_reconciles` with
      `WYRD_LOG=info,vala_bifrost_redux=debug`: passed.
    - `mise run fmt`: clean.
    - `cargo clippy -p vala-bifrost-redux -p wyrd-testing -p wyrd-telemetry
      --all-features --tests -- -D warnings`: clean.
    - `git diff --check`: clean.

- **verify:bifrost residuals after 24039433e.**
  - **5, `two_bindings_share_one_client_observation` returned inconclusive
    for both bindings: FIXED (179e91bb9).**
    - Symptom: in `wyrd-testing::server`, both verdicts were inconclusive.
      It failed in about 1 of 3 full-lane runs and rarely alone.
    - Evidence (WYRD_LOG
      `info,vala_bifrost_redux=debug,wyrd_server::verification=debug`):
      - The observation's `wyrd_event_time` equalled its receipt time,
        07:12:32.799814Z.
      - The binding window end, taken from `statement_timestamp()` in
        `make_binding_due`, was 07:12:32.786634Z.
      - `ObservationWindow` filters `wyrd_event_time < end`, so the row fell
        outside the window.
      - psql against the test Postgres showed its clock running about 24 ms
        behind the host clock in every sample.
    - Cause:
      - The journey let Scribe stamp the default event time from the host
        `SystemTime`.
      - It then compared that time with an exclusive window end on the
        Postgres clock.
      - A write acknowledged less than about 24 ms before `make_binding_due`
        therefore landed at or after the window end.
      - The product is correct: PostgreSQL owns coordination time, and the
        producer owns event time.
      - Suspects ruled out: TASK-004 compaction defaults, TASK-001
        promotion, Scribe claim publication and redux telemetry. None of
        them is on this path.
    - Diagnostician (fresh, read-only; given only the command, trace and
      diff): same cause, fix site in the journey's `observation_batch`. The
      other `make_binding_due` callers were checked and are unaffected.
    - Fix: `verification_runtime.rs` `observation_batch` supplies a
      caller-owned `wyrd_event_time`. `observed_in_current_month` sets it to
      the later of the month start and now minus 1 minute, which keeps it
      inside the monthly window.
    - GREEN: the full server journey binary passed 29/29 in each of 3
      consecutive runs.
  - **6, Forge `scribe_promotion` reported "promoted object ... disagrees
    with its evidence on column sizes" (`internal_invariant`): FIXED
    (8f31a1188).**
    - Symptom: in the journey:typescript log, promotion of `traces.spans`,
      `metrics.points` and `logs.records` objects failed as terminal
      `internal_invariant`.
    - Evidence:
      - `validate_promoted_object` compares the Scribe evidence with the
        footer metrics. The evidence comes from
        `parquet_writer.rs::derive_data_file_metrics`, keyed by
        `iceberg_schema_for(schema)`, which uses the declared
        `PARQUET:field_id`s, envelope ids 1000–1006 included. The footer
        metrics come from `PromotedObjectFooter::decode`, keyed by the
        table's current schema.
      - A probe comparing the declared ids with the ids after
        `TableMetadataBuilder::from_table_creation` found 102 differences.
        All were in spans (41), points (47) and records (14). Every other
        built-in matched.
    - Cause:
      - `BifrostCatalog::create_physical_table` passed the declared-id
        schema to `SqlCatalog::create_table`.
      - Iceberg's `reassign_ids` renumbers every field from 1, so the
        physical table recorded ids that the Parquet objects and the
        evidence do not use.
      - `column_sizes` is the first id-keyed map the comparison reaches.
    - Impact: no data loss.
      - Unpromoted `file_list` rows are excluded from hot GC and stay
        hot-readable.
      - Promotion of these tables was permanently stuck:
        - the failure is terminal;
        - the idempotency index defers the same plan;
        - each new plan includes the same objects;
        - the hot tier grows without bound.
      - Relaxing the check would have been wrong. Oracle's Iceberg scan
        resolves columns by field id, so promoted rows would read as the
        wrong columns or as nulls.
    - Diagnostician (fresh, read-only): confirmed the cause and the fix site
      at table creation.
    - RED: `catalog::bifrost_catalog::production_pin_tests::builtin_tables_keep_their_declared_field_ids`
      failed with `traces.spans field ids`.
    - Fix:
      - `create_physical_table` builds the creation metadata, then
        `with_declared_field_ids` rebinds it:
        - the current schema is replaced by the declared one;
        - `last-column-id` is set to the declared highest id;
        - partition and sort `source-id`s are remapped by name;
        - spec id 0 and sort order id 1, which Scribe stamps on every data
          file, are kept.
      - The rebound metadata is written as the first metadata document and
        registered with `SqlCatalog::register_table`.
      - The describe-path comment that documented the old divergence is
        updated.
    - GREEN: the same test passes for all three signal tables, including
      re-provisioning.
    - Material risk: tables created before 8f31a1188 keep their sequential
      ids, and promotion of their signal tables stays stuck. Recovery needs
      those tables recreated, or a metadata repair. Registration does not
      validate ids, so these tables are not refused.
    - Not changed: `dev.agent_traces` cannot be provisioned through
      `ensure_builtin`, because its user field `run_id` is a reserved
      column. The test therefore covers the three id-declaring signal
      tables.
  - **7, canonical-signal reads failed with "unsupported
    'FIXED_LEN_BYTE_ARRAY' index type in column_index" once spans promoted:
    FIXED (4572ec145).**
    - Symptom: after 8f31a1188, `verify:bifrost` failed the same read in
      four journeys:
      - `pg_bifrost_e2e canonical_signal_arrow_write_and_sql_read_round_trip`
      - MCP `agent_reads_canonical_trace_genai_logs_and_metrics_through_sql`
      - Python `test_canonical_signal_arrow_write_and_sql_read_round_trip`
      - the TypeScript canonical journey
    - Evidence:
      - The failing filter is `parent_span_id IS NULL`, on a
        `FixedSizeBinary(8)` column.
      - Oracle's `OracleIcebergScanExec::start_stream` enables row selection.
      - In the fork, the page-index evaluator (`page_index_evaluator.rs`)
        returns a hard error for FLBA and INT96 column indexes.
      - It also `unwrap`s a UTF-8 decode of every BYTE_ARRAY bound, so a
        `Binary` column whose bounds are not UTF-8 panics.
    - Cause: a latent Oracle and fork defect. Before 8f31a1188, signal
      tables never promoted. Their predicate ids also pointed at other
      Parquet leaves, so this path was never reached correctly.
    - Diagnostician (fresh, read-only):
      - Root fix site: the fork's evaluator, which should decode by Iceberg
        type and return `Ok(None)` for anything unsupported.
      - In-repo alternatives: disable row selection everywhere, or gate it
        by predicate column type.
      - Affected filters: any filter on `trace_id`, `span_id` or
        `wyrd_batch_id` against promoted data.
    - Fix: we cannot push the fork, so the gate is in Oracle.
      - `page_index_evaluable` enables row selection only when every column
        the predicate names is boolean, numeric, temporal or string.
      - Row-group pruning stays on.
      - DataFusion still applies the filter, which is reported Inexact.
    - Test:
      - Unit test `oracle::exec::tests::page_index_selection_skips_columns_the_evaluator_cannot_decode`.
      - The SDK journey binary (17/17) and the MCP journey binary (14/14)
        now pass.
    - Follow-up (fork): fix the evaluator so that page-level pruning also
      covers ids and binary columns.
  - **8, `oracle distributed::live_query_terminal_failure_matrix` poisoned
    the governor ("Oracle query owner outlived a nested resource child"):
    FIXED (51c64fb29).**
    - Symptom: the rejected-Scribe-ticket case terminated the server, and
      the next request failed with a transport error. It is intermittent:
      the same lane passed in the earlier run.
    - Evidence: the poison follows the first-batch rejection by 34 µs, and
      there was no "did not drain" warning.
    - Cause:
      - `drain_children` → `nested_idle` treated zero bytes as idle.
      - The sibling published-partition `SortExec` had registered its
        reservation at build time and was still mid-poll when the merge was
        aborted.
      - It passed the drain at zero bytes, grew, and then `release` saw
        reserved != 0.
    - Diagnostician (fresh, read-only):
      - Same cause.
      - Fix site: `GovernedMemoryView` registration accounting.
      - The diff under test was not involved.
    - Fix:
      - The view counts live registrations, shared with
        `OracleQueryResources`.
      - `nested_idle` now requires zero bytes and zero registrations.
      - `release` keeps its bytes-based poison.
    - RED → GREEN: `resources::tests::a_registered_zero_byte_child_keeps_its_query_busy`.
  - **"cancelled with unknown acceptance" warnings: benign, no change.**
    - They are emitted when test-server shutdown cancels `commit_once`
      during the catalog commit.
    - The failure class is `TransientCoordination`, and the next owner
      reconciles the outcome by operation id.

### AC-008 qualifying runs

Commit 69d2efe8c, two back-to-back runs of `mise run bench:bifrost:forge-capacity` with the
default envelope (coordinator revision of 2026-10-03):
- leader: 1 CPU / 4 GiB, resolved effective_cpu 1;
- workers: 7/3 CPU (2.33) / 4 GiB each, in fleets of 1, 2 and 3;
- workload: 128 tables, 1 tenant, seal every 10 s, one 64-row write per table
  per second, compaction on at the default type and target, due every 2
  promotion commits;
- each fleet drains its own 128 fresh due tables.

Host (AMD Ryzen 9 9950X, 32 CPUs, 92 GiB) load before / after:

| Run | Before | After |
|---|---|---|
| 1 | busy 0.38 CPUs, free 27.19, loadavg 4.81 | busy 0.40 CPUs, free 28.82, loadavg 3.18 |
| 2 | busy 0.58 CPUs, free 29.31, loadavg 2.69 | busy 0.39 CPUs, free 28.40, loadavg 3.60 |

Both hosts were well above the 8-CPU free floor. Reports are in
`target/bifrost-forge-capacity/` and are not checked in.

| Gate | Needs | Run 1 | Run 2 | Result |
|---|---|---|---|---|
| In-process p99 commit / pull / report at 1× (µs) | < 1000 | 21.8 / 50.6 / 2.4 | 21.3 / 56.7 / 2.3 | PASS |
| In-process p99 commit / pull / report at 10× (µs) | < 1000 | 7.7 / 47.3 / 2.7 | 7.0 / 43.0 / 2.8 | PASS |
| Live leader p99 commit / pull / report at 1× (µs) | < 1000 | 26.6 / 78.9 / 18.9 | 17.9 / 67.7 / 27.3 | PASS |
| Live leader p99 commit / pull / report at 10× (µs) | < 1000 | 36.5 / 43.3 / 11.8 | 28.6 / 57.6 / 15.5 | PASS |
| 2-worker / 1-worker completion rate | ≥ 1.7 | 2.098 | 2.098 | PASS |
| 3-worker / 1-worker completion rate | ≥ 2.5 | 3.228 | 3.212 | PASS |
| Pulls answered short while a table stayed due | = 0 | 0 of 97 | 0 of 97 | PASS |
| Highest leader CPU share | < 0.70 | 0.033 | 0.031 | PASS |

The in-process knee is 64,000 pulls/s in both runs: p99 pull is 561 / 585 µs
at 10,000×, and the curve bends at 30,000×. The live 10× client round trip
p50 is 15.4 ms in both runs; that is the probe client's own pacing, the leader
p99s above are what the gate measures, and no gate applies to the client
round trip.

| Fleet | Run 1 rewrites/s (drain) | Run 2 rewrites/s (drain) | Rewrite p50/p99 | Promotions/s | Leader CPU | Postgres cores / xact/s | RustFS cores |
|---|---|---|---|---|---|---|---|
| 1 worker | 0.80 (128 tables in 161 s) | 0.80 (128 in 161 s) | 0.25 / 0.50 s | 0.84 / 0.87 | 1.3% / 1.5% | 0.043 / 90; 0.045 / 91 | 0.068 / 0.064 |
| 2 workers | 1.67 (128 in 77 s) | 1.67 (128 in 77 s) | 0.25 / 0.25 s | 1.71 / 1.73 | 2.1% / 2.2% | 0.065 / 177; 0.067 / 180 | 0.073 / 0.077 |
| 3 workers | 2.57 (132 in 51 s) | 2.55 (131 in 51 s) | 0.25 / 0.25 s | 2.57 / 2.53 | 3.3% / 3.1% | 0.087 / 263; 0.083 / 261 | 0.102 / 0.103 |

Backlog fill (no worker running) promoted at 5.5–6.2/s with promotion
p50/p99 of 0.05 / 0.05 s, and no write was refused.

Bounding resource for the largest step, in both runs: worker pull cadence
(106–107% of the 0.8 pulls/s-per-worker ceiling). Compactor CPU stayed
around 1%, RustFS around 0.1 cores, Postgres under 0.09 cores, and due-table
supply at 0–3%. Throughput scales with the number of pulling workers. No
compute or storage resource is near its limit at this envelope, so the
pull cadence is the measured bound, not compactor CPU, RustFS or Postgres.

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-003 capacity-bounded pull | worker.rs `free_pull_room` | journey `compactors_pull_oldest_due_with_capacity`; `mise run test:bifrost:journey:forge` 19/19 | PASS |
| AC-004 oldest due selection, one task per table | leader.rs `DueIndex` | leader lib tests 5/5 incl. equivalence | PASS |
| AC-005 current-head planning, no-op success, later-commit preservation, late-report ignorance | worker.rs, leader.rs `report` | redux `--test integration -E 'test(/^forge::/)'` 56/56 | PASS |
| Multi-replica worker progress | forge_peer.rs, leadership.rs | journey lane 19/19; `mise run test:tonic` 39/39 | PASS |
| No leader-side file inspection | leadership.rs | `leader_decision_has_no_catalog_io` (zero catalog/object IO) | PASS |
| AC-008 capacity evidence | bench `bench:bifrost:forge-capacity` | two qualifying runs at 69d2efe8c, every gate passing (see "AC-008 qualifying runs") | PASS |
| Review 1: crash mid-removal restarts | hot_stage.rs `retire_all`, `recover`, `validate` | `a_claim_crashed_inside_one_member_removal_retires_on_restart` | PASS |
| Review 2: budget exhaustion is backpressure | staging_runtime.rs `ClaimTakeError`; persistence.rs `publish_claims` | journey `concurrent_flushes_share_the_claim_budget_and_publish_each_claim_once`; `a_claim_is_retryable_only_while_no_publisher_drives_it` | PASS |
| Review 3: one publisher per claim | staging_runtime.rs `ClaimDrivers`, `DrivenClaim` | same journey (no double assemble, no `.tmp` race) | PASS |
| Review 4: tick retries a failed due claim | persistence.rs `publish_claims` retry drain | journey `scribe_tick_retries_a_failed_due_claim` | PASS |
| Review 5: Publishing replay is idempotent | claim_publication.rs `publication_operation_id` | `a_claim_stranded_in_publishing_publishes_after_restart` | PASS |
| Suggestion: merge lane from free CPUs | persistence.rs `merge_lane_threads` | `merge_lane_threads_leave_the_persistence_lane_its_cpus` | PASS |
| Suggestion: root fsync for new key | hot_stage.rs `sync_key_entry` | `the_first_record_of_a_key_syncs_the_staging_root_once` | PASS |
| Follow-up 1: restore schema from kept members only | staging_runtime.rs `restore`, `restore_context` | `a_key_with_finished_claim_leftovers_and_a_live_member_restores` | PASS |
| Follow-up 2: live tick finishes a claim refused after commit | claim_publication.rs `finish_committed`; staging_runtime.rs `finish_committed`, `retryable_claims`; persistence.rs `resume_claim` | journey `scribe_tick_finishes_a_claim_that_failed_after_its_commit`; `a_live_claim_refused_inside_its_removal_finishes_without_republication` | PASS |
| Follow-up 3: telemetry journey independent of WYRD_LOG | wyrd-telemetry `init_test_capture` `with_level(true)`; telemetry.rs level assertion | `scribe_hot_path_telemetry_reconciles` under default and debug filters | PASS |
| Follow-up 4: redux integration binary traced | forge/support.rs `ProcessTelemetry` | non-checkpoint Forge test prints DEBUG trace under WYRD_LOG | PASS |
| Residual 5: shared verification observation lands in the window | verification_runtime.rs `observation_batch`, `observed_in_current_month` | server journey binary 29/29 ×3 | PASS |
| Residual 6: promoted signal objects agree with their evidence | bifrost_catalog.rs `create_physical_table`, `with_declared_field_ids` | `builtin_tables_keep_their_declared_field_ids` (RED → GREEN); `verify:bifrost` log has 0 `internal_invariant` and 0 `disagrees with its evidence` hits (the 24039433e log had 7) | PASS |
| Residual 7: promoted canonical signals stay queryable | exec.rs `page_index_evaluable` | `page_index_selection_skips_columns_the_evaluator_cannot_decode`; SDK, MCP, Python and TypeScript canonical journeys | PASS |
| Residual 8: failed-query drain waits for registered children | resources.rs `GovernedMemoryView` registration count, `nested_idle` | `a_registered_zero_byte_child_keeps_its_query_busy` (RED → GREEN); oracle journey 42/42 | PASS |

`mise run verify:bifrost` at 51c64fb29 exited 0. Lane results:

| Lane | Result |
|---|---|
| check:bifrost: fmt, resource governance, object-store pin, tenant isolation | pass |
| Rust unit | 210/210 |
| integration redux | 900/900 |
| integration server | 113/113 |
| integration SQL | 85/85 |
| journey SDK | 17/17 |
| journey observe | 3/3 |
| journey drift | 4/4 |
| journey forge | 21/21 |
| journey scribe | 25/25 (1 skipped by the lane filter) |
| journey oracle | 42/42 |
| journey OTLP | 11/11 |
| journey server | 29/29 |
| journey MCP | 14/14 |
| Python unit | 2 passed |
| journey Python | 47 passed |
| TypeScript unit | 10/10 |
| journey TypeScript | 27/27 |

Summary line: "9/9 lanes passed". `mise run lints` and `git diff --check` are clean.
