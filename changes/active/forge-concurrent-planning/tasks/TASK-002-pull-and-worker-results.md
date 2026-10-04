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
runs in a 1 CPU / 2 GiB scope and each dedicated compactor in a
1.5 CPU / 3 GiB scope, so the 4-worker step uses 7 CPU / 14 GiB and every
step adds identical capacity. Postgres and RustFS run outside the envelope
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
2. *Fleet throughput.* At least 128 independent tables written continuously
   through Scribe at a production-like rate, with compaction enabled at the
   default `full` type, the default 1 GiB file target and a realistic commit
   trigger (not one commit), with one leader and 1, 2 and 4 compactors. No
   oversized seed is written to lengthen rewrites. 2-worker and 4-worker
   completion rates reach at least 1.7x and 3.0x one worker; whenever the
   leader holds due tables, every pull is answered with as many tasks as it
   requested; leader CPU stays below 70%. The report names the resource that
   bounds the largest step (compactor CPU, RustFS or Postgres).

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

## Implementation Evidence

Status: IMPLEMENTED for Scenarios 1–3. Scenario 4 (AC-008) is OPEN until the
revised capacity bench (GREEN above, revised 2026-10-03) reports two
qualifying runs; results are appended below when they exist.

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
- **Promotion pacing (OPEN, product defect in Scribe claim publication).**
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

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-003 capacity-bounded pull | worker.rs `free_pull_room` | journey `compactors_pull_oldest_due_with_capacity`; `mise run test:bifrost:journey:forge` 19/19 | PASS |
| AC-004 oldest due selection, one task per table | leader.rs `DueIndex` | leader lib tests 5/5 incl. equivalence | PASS |
| AC-005 current-head planning, no-op success, later-commit preservation, late-report ignorance | worker.rs, leader.rs `report` | redux `--test integration -E 'test(/^forge::/)'` 56/56 | PASS |
| Multi-replica worker progress | forge_peer.rs, leadership.rs | journey lane 19/19; `mise run test:tonic` 39/39 | PASS |
| No leader-side file inspection | leadership.rs | `leader_decision_has_no_catalog_io` (zero catalog/object IO) | PASS |
| AC-008 capacity evidence | bench `bench:bifrost:forge-capacity` | pending revised two-run report | OPEN |

