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
| A pull first expires timed-out tasks, then scans and sorts eligible tables by oldest due time: [timeout processing](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:915), [oldest-due selection](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:942). | More due tables than one pull; oldest selected; timed-out table eligible on the next pull, not by an autonomous deadline event. Use the same scan/sort shape. |
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

**GREEN.** Measure separate event, selection, RPC/catalog and worker stages.
Run a fixed 10,000 tracked-table / 1,000 due-table scheduling case with 32
concurrent pullers: p99 warm commit-state update and p99 in-memory pull
selection must each stay below 1,000 microseconds on the declared host.
Then run real worker-bound rewrites over at least 128 independent eligible
tables with one leader and 1, 2 and 4 identical compactor replicas. Under
steady backlog, 2-worker and 4-worker completion rates must reach at least
1.7x and 3.0x the one-worker rate, respectively; leader CPU must stay below
70%, and workers must be at least 85% occupied. Record object-store and
Postgres utilization so another saturated dependency cannot be mistaken for
a leader bottleneck. If the RisingWave scan misses the in-memory latency
target, report a failed capacity gate; do not substitute another scheduling
algorithm in this task. Report repeated p50/p99 and throughput
measurements, not one favorable run.

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
leader CPU, p50/p99 by stage, completed work per second at 1/2/4 workers,
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
