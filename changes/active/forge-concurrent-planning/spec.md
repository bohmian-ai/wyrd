---
id: SPEC-forge-concurrent-planning
revision: 8
status: approved
---

# Forge Iceberg compaction and maintenance

## Objective and decision

Replace concurrent durable planning with the RisingWave Iceberg ownership model:
one elected Forge leader keeps per-table scheduling state in memory; compactors
on that pod or other replicas pull table-level tasks, load current Iceberg
metadata, decide physical work, rewrite, and publish. Compactor count scales
physical throughput across eligible tables. The leader does not read manifests,
select files, rewrite data, or list objects to decide whether a table is due.

Revision 6 supersedes revision 5 and the revision-2 concurrent-planning task
packet. The
implementation removes machinery earned only by leaderless durable planning.
Scribe hot visibility, Oracle pinned cuts, tenant isolation, exact Iceberg
publication, and safe deletion remain required outcomes. No new generic work
framework, second file selector, or durable scheduling state is authorized.

## Exact RisingWave reference behavior

The local RisingWave source is revision
e23ddf952c3e6ebc03cc254789e84d1179cfacae. This spec follows its
Iceberg path, not its separately durable Hummock compaction path.

| Mechanism | RisingWave source under src/ |
| --- | --- |
| Leader election and limited followers | meta/node/src/server.rs:179-291; meta/src/rpc/election/sql.rs:500-533 |
| Empty-on-start in-memory tracks and maintenance sets | meta/src/manager/iceberg_compaction/mod.rs:68-101 |
| Post-Iceberg-commit notification | connector/src/sink/iceberg/commit.rs:265-305,808; meta/src/manager/iceberg_compaction/stream.rs:30-41 |
| Due rule and Idle / PendingDispatch / InFlight | meta/src/manager/iceberg_compaction/schedule.rs:40-70,125-159,199-255 |
| Pull, timeout, oldest-due selection | storage/src/hummock/compactor/mod.rs:1606-1640; meta/src/manager/iceberg_compaction/schedule.rs:915-993 |
| Table-level dispatch; worker physical planning | meta/src/manager/iceberg_compaction/schedule.rs:428-490; storage/src/hummock/compactor/iceberg_compaction/iceberg_compactor_runner.rs:477-680 |
| Report consumes dispatch-time count only | meta/src/manager/iceberg_compaction/schedule.rs:317-353,1114-1176 |
| Hourly manifest rewrite, expiry, expired-file cleanup | meta/src/manager/iceberg_compaction/gc.rs:132-482 |

RisingWave defaults: enable_compaction false; type Full when enabled; interval
3600 seconds; snapshot-count threshold usize::MAX; small-file threshold 64 MiB;
output target 1024 MiB; delete-file count threshold 256; report deadline 30
minutes; maintenance tick 3600 seconds; snapshot expiry enabled with 24-hour
age fallback; manifest rewrite disabled. Sources:
connector/src/sink/iceberg/config.rs:230-240,348-423,783-818 and
common/src/config/meta.rs:725-735. An unset retain-last setting does not mean
retain zero snapshots. RisingWave's pinned compaction library has file-existence
checks and catalog retries, but its core/src/compaction/mod.rs:985 explicitly
leaves some concurrent data/delete validation unfinished.

## Required behavior

### REQ-001 — One active scheduling leader

Keep Forge's existing single-row leader election after dropping the
branch-only concurrent-planning commits. Exactly one replica dispatches
table-level work and runs maintenance timers. Standbys can host compactors and
take over leadership. Loss of leadership stops dispatch and leader timer work.
The leader election heartbeat/expiry is a separate 30-second failover
boundary, not the worker report deadline or a publication operation timeout;
graceful stop resigns immediately.
The leader's counters, due times, task states and maintenance sets are
process-local and disposable. No durable per-table demand, generation,
planning claim, schedule row or pending-task queue is authoritative for this
protocol.

### REQ-002 — Scribe publication is distinct from Iceberg promotion

Scribe's committed file_list makes immutable hot objects readable before
Iceberg promotion. Forge promotes those exact objects unchanged. Successful
Iceberg promotion, not Scribe's hot publication, notifies the scheduling
leader and increments the table's pending Iceberg-commit count with the
observed snapshot watermark. A lost in-memory hint cannot strand promotion:
leadership startup reconciles eligible unpromoted rows from existing durable
file_list evidence. Oracle cuts never double-read or lose rows at the hot to
Iceberg authority boundary.

### REQ-003 — Per-table scheduling state and due rule

State is keyed by tenant and physical table identity and moves among Idle,
PendingDispatch and InFlight. Each successful Iceberg promotion adds one
pending commit and updates the latest snapshot. An Idle table is due when
pending commits reach the threshold, or its next compaction time has passed
and it has at least one pending commit. Defaults match RisingWave: 3600-second
interval and disabled count trigger (usize::MAX). No timer-only rewrite occurs
for zero commits. Manual compaction may force dispatch. Table policy comes
from authoritative catalog settings and refreshes outside the lock-protected
in-memory decision path. Settings can disable compaction without disabling
snapshot expiration or manifest maintenance. Match RisingWave's
disabled-by-default compaction setting; a table must explicitly enable it.

### REQ-004 — Worker capacity pull

An idle compactor on any replica pulls up to its available capacity. The
leader selects the oldest due tables and captures each pending commit count
and snapshot watermark in its in-memory dispatch state. The worker receives
tenant-qualified table identity, target branch, settings, task type and task
identity, with no planned files or watermark. One table has at most one
current dispatched task. A failed send before acceptance restores Idle without
consuming commits. Pulls drain eligible backlog independently of the timer;
local resource admission bounds worker running and waiting work.
Routing is fixed by pod locality, as for Oracle and Gateway: when a worker
and leader share a replica, call the leader handler in-process; when they
run on different pods, use the authenticated internal peer connection.
Both paths enter the same handler and state machine. Never loop a local
call through the network or use an in-process shortcut across pods; expose
no public client API. Match RisingWave's default five-second pull interval,
maximum four table tasks per pull and wait-for-acknowledgement behavior.
The worker owns capacity calculation and pull cadence on both paths; neither
path pushes tasks from the leader or changes due selection, timeout, dispatch
capture, acknowledgement, or report semantics.
As in RisingWave, `max_task_parallelism = ceil(executor_worker_count ×
compactor_max_task_multiplier)`; the Iceberg-mode default multiplier is 12.
For each acknowledged pull,
`pull_task_count = min(max_task_parallelism - running_parallelism_sum, 4)`.
`running_parallelism_sum` is the local queue's sum of plan-required
parallelism, not the number of tables or pods. Waiting parallelism is a
separate bounded queue and is not subtracted in this pull formula. Wyrd uses
its existing effective-CPU resource plan as the executor worker count and its
existing Forge plan queue for both sums; no second capacity ledger is added.
The elected coordinator publishes its private peer URI with its owner and
fencing token in the existing singleton election row. A remote Forge worker
reads that row to find the current leader, dials it with the existing peer
mTLS credentials, and reconnects after election/fence change. Extend the
existing private peer router with only Forge promotion-notify, pull and
report methods. Dedicated
Forge workers use those credentials as dial-only clients and open no peer
listener. The election URI is routing information, never a durable task or
schedule.

### REQ-005 — Worker physical decision

The worker loads the latest Iceberg table and plans with the behavior of the
exact nimtable `iceberg-compaction-core` revision RisingWave pins,
`74bdc45cb17feaf0ec4eb351d4be271c99d3624c`. Wyrd currently pins its
fork at `6773e192c995d4d9423536f44f05f99b6ea81e5a`, which descends from
that revision but changes file selection. The implementor decides whether to
retain and trim the fork or use the upstream dependency after validating both
physical parity and Bifrost's central-governor requirement. Every Forge
DataFusion rewrite must charge its memory reservations to the one Bifrost
governor shared with Scribe and Oracle, and spills must use the governed Forge
scratch root. An independent `max_memory_bytes` pool is insufficient for that
requirement. Upstream already exposes separate public `plan_compaction()` and
`rewrite_plan()` methods that do not commit; a noncommitting wrapper alone
does not justify a fork. Forge retains control of its publication call. Do not
call upstream `compact_with_plan()` because that method commits. For physical
selection, match RisingWave's Full default and configured Auto,
SmallFiles and FilesWithDelete modes, forcing Full for copy-on-write. Build
their settings as in RisingWave's `iceberg_compactor_runner.rs:477-590` and
prove selection parity against the pinned upstream source and paired fixtures.
The fork's current Auto selects a union of small and delete-heavy files,
whereas RisingWave's Auto checks table-wide thresholds and prioritizes the
delete-heavy plan. Remove Forge's WyrdIdentityAware selection and leader-side
candidate-file scanning. Delete fork-only machinery without a proven remaining
consumer; do not add a second planner. The leader sends no file
list. Keep tenant, branch, delete and Oracle/hot-object safety checks at the
publication boundary. An empty plan reports success without a rewrite.

### REQ-006 — Result, timeout and failover

Dispatch captures pending count and snapshot watermark. A matching success
subtracts only that count, preserving later commits, and starts the next
interval. Failure returns the table to Idle, due immediately. On a subsequent
pull, an InFlight task past its configurable report deadline becomes eligible
again; default deadline is 30 minutes from dispatch. A report for an obsolete
task identity cannot change the new leader's scheduling state. A timed-out
worker can still be executing; task-ID report rejection alone is not commit
fencing. Publication must preserve correct table, branch, inputs, delete
semantics, tenant and operation identity. A new leader starts with empty
scheduling counters, not replayed planning claims.

### REQ-007 — Timer maintenance and cleanup

The leader's default hourly timer first considers enabled manifest rewriting,
then enabled snapshot expiration. Manifest rewriting groups eligible small
data manifests by partition spec; unsupported Iceberg v3 tables are skipped.
Manifest rewrite remains disabled by default. Snapshot expiration becomes
enabled by default, matching RisingWave, with the 24-hour age fallback and
unset retain-last default. Iceberg's current snapshot remains protected by
its own expiry semantics. Remove the current
32-commit per-table maintenance trigger: timer work is independent of
ordinary compaction commit counts.
Expiration uses configured age and retained refs; an active compaction's
observed snapshot clamps its cutoff, and no safe watermark means skip. It
commits expiration before cleanup of files used only by expired snapshots.
A per-table error is reported and does not stop subsequent tables.

Wyrd also protects Oracle pinned cuts, unresolved publications, and Scribe
hot objects without exact promotion evidence. Never-published orphan cleanup
is separate because RisingWave's Iceberg GC loop does not implement it.
Destructive cleanup freshly validates catalog reachability, tenant/table
identity and these protection roots. Honor an explicit per-table retain-last
setting; unset does not mean retain zero snapshots.

### REQ-008 — Empty leader restart and hot promotion recovery

On leadership acquisition, start with empty compaction tracks and
maintenance sets, as RisingWave does. Reconcile unpromoted Scribe hot
file_list rows so lost in-memory notification does not strand readable hot
data; this recovery is limited to the existing hot-publication obligation.
Do not scan the registered-table catalog to seed cold maintenance, replay
compaction counters, or persist a schedule. A table joins the maintenance
sets on its next Iceberg commit or manual request.

### REQ-009 — Leader throughput

Warm commit notification and due selection use only in-memory scheduling
state; no SQL claim, manifest scan, catalog read or object IO occurs while
making that decision. At least two eligible tables can execute concurrently
on distinct replica compactors while the leader keeps dispatching. One
table's task can run bounded internal parallel plans; adding workers does
not split one table into arbitrary leader-issued tasks. Measure p50/p99
leader event update and pull selection separately from RPC, catalog lookup
and worker time under the existing capacity workload, recording table count,
commit-event rate, worker count and contention. Microsecond leader decisions
are a measured target, not an asserted RisingWave guarantee: RisingWave scans
all tracked tables and sorts eligible ones under a write lock
(schedule.rs:942-963). Keep RisingWave's due rule, timeout handling and
oldest-due order exactly, but select from a sorted due index maintained as
tracks change, so a pull costs O(limit · log tables) rather than O(tables)
under the lock. A randomized test proves the index selects exactly what the
RisingWave scan selects. The leader records each commit, pull and report
decision in `bifrost_forge_leader_decision_seconds{operation}`.
(Revision 2026-10-03, approved by the human owner after the capacity bench
measured the full scan at about 100 µs per pull and a 28–34 ms p99 under 32
pullers.)

### REQ-010 — Delete superseded machinery

Remove concurrent planner claims, owner tokens, renewal/expiry, generation
acknowledgement, failed-pass exclusions, persisted per-table next-due state,
pre-dispatch planned-file envelopes, global worker fairness cursor,
timer-paced roster repair, and obsolete metrics/docs/tests. Retain an old
task, publication or reconciliation component only where an actual
REQ-002/005/007 safety outcome still consumes it. No compatibility aliases.
Drop the branch-only concurrent-planning migration with its commits. Retire
older planning-demand schema only after its consumers are removed and
unsettled publication and cleanup evidence has a safe owner.

### REQ-011 — Compaction is on for every table

(Added 2026-10-03, approved by the human owner.) Bifrost owns its Iceberg
tables, so automatic compaction is enabled for every table by default,
including built-in tables. `wyrd.forge.enable-compaction` remains readable;
an absent property means enabled. This intentionally departs from
RisingWave's sink default (`false`). The staged-file target (512 MiB) and
the compaction file target (1 GiB) do not change.

### REQ-012 — Tables choose their compaction type

(Added 2026-10-03, approved by the human owner as a public contract change.)
Table registration accepts an optional `compaction_type` (`auto`, `full`,
`small-files`, `files-with-delete`), following the existing
`compaction_target_file_size_bytes` contract end to end: wyrd-spec request
and description, Rust, Python and TypeScript SDKs, server validation, and the
catalog writing `wyrd.forge.compaction.type` in the create transaction.
Omitted stores no property, so the table uses the REQ-013 default. A re-register may omit it or repeat the stored value; a
different value is a stable conflict error, as for the file target. The
table description reports the stored type. Copy-on-write tables still
compact `full`.

### REQ-013 — Default compaction merges staged files once and never revisits finished files

(Added 2026-10-03, approved by the human owner.) Scribe stages toward 512 MiB
and Forge compacts toward 1 GiB; neither changes. A table that names no
compaction type compacts `small-files`. The small-file threshold is no longer
a fixed 64 MiB: it is 75% of the table's resolved file target (768 MiB at the
1 GiB default, following a table's own `write.target-file-size-bytes`).
Files below it are merge candidates; files at or above it are finished and
are never selected again. Forge's `small-files` plan sets the core's existing group filter
(`min_group_file_count = 2`), so a lone staged file waits for a partner;
`full`, `auto` and `files-with-delete` keep upstream grouping. `full`, `auto` and
`files-with-delete` remain available per table through REQ-012.

### REQ-014 — One tenant-scoped Oracle catalog selection

(Added 2026-10-04, approved by the human owner.) Iceberg catalog identity is
tenant relative. A catalog lookup by an Oracle request can expose only tables
registered to its authenticated tenant, enforced by tenant-scoped database
authority rather than an Oracle-side name check alone. Forge retains its
internal platform credential for Iceberg publication, compaction, and
maintenance. Oracle uses tenant-scoped database authority for its catalog and
Scribe cut reads; it does not use a global catalog credential on the request
path.

One Oracle query acquires its complete set of referenced-table catalog pointers
and unresolved Scribe hot-file candidates through one SQL `SELECT`, including
the first read of a table and a cut that needs new reader protection. That
database operation establishes durable, bounded protection before returning
the cut. Protection covers both the Iceberg objects and Scribe hot objects
that the result may read, even across publication, compaction, expiration, and
cleanup. Forge defers reclamation covered by an active reader. A bounded
table-wide read pin is acceptable for this purpose; release, cancellation,
node failure, and stale-owner fencing cannot leave permanent pins. The
returned catalog pointers and hot candidates represent one consistent
database view, and Oracle still validates and reconciles them against the
immutable Iceberg metadata and manifests before scanning. Promotion or catalog
movement after selection cannot duplicate, omit, or prematurely delete rows.

The one-`SELECT` requirement concerns SQL statements, not object-store reads
or a claim of one database network round trip. Transaction setup and commit
remain visible in the step count. The implementation must reduce the serial
database steps of cut acquisition compared with the current two catalog
pointer reads plus hot-file read, without using elapsed time as the proof.
Latency targets and before/after milliseconds belong to the separate
optimization-and-benchmarks change.

### REQ-015 — Iceberg assigns every table's physical field IDs

(Added 2026-10-04, explicitly approved by the human owner.) Built-in signal
tables and custom tables do not declare their own numeric field IDs. The
registered Iceberg table assigns the IDs, including nested and Bifrost-managed
fields. Scribe writes those IDs into Parquet, Oracle interprets them from the
registered table, and Forge keeps its exact file-to-table ID validation. The
canonical signal fingerprint covers the ordered names, physical types,
nullability, nesting, and semantic sensitivity metadata without treating
numeric field IDs as canonical identity. Fresh signal-table promotion works
without editing Iceberg's generated metadata document to restore declared
IDs. Nothing has shipped, so no legacy-table migration is required.

### REQ-016 — Prove typed pruning on both Oracle tiers

(Added 2026-10-04, directed by the human owner after review.) Oracle's hot
and Iceberg readers use the Bloom filters declared in each table's physical
layout, including binary `trace_id` values, while preserving exact results
and conservative behavior for unsupported or missing index evidence. The
closed scan-predicate contract carries binary literals losslessly to hot and
distributed readers. The Iceberg reader also handles binary page bounds
without disabling page selection for other predicates in the query. Tests
isolate Bloom exclusion from min/max exclusion rather than inferring Bloom
use from a file's Bloom metadata. `wyrd_request_id` equality lookups return
exact rows on hot, promoted, and rewritten cuts and demonstrate row-group
min/max exclusion where row-group ranges are disjoint; this requirement does
not add a Bloom filter for that column or promise pruning when ranges overlap.

## Invariants

- INV-001: One active scheduling leader; compactors on any replica may work.
- INV-002: No durable scheduling counter, demand claim, next-due timestamp,
  or planned-task queue is needed for correct scheduling.
- INV-003: At most one current task per tenant-qualified physical table;
  expired workers may still be physically running.
- INV-004: A new commit during a task survives that task's success.
- INV-005: Hot Scribe objects and Oracle pinned cuts stay queryable until
  their exact authoritative replacement or retention release.
- INV-006: Age, path shape, stale candidate evidence or an ambiguous catalog
  response alone never authorize deletion.
- INV-007: Physical file selection has one worker-side compaction-library owner.
- INV-008: Tenant isolation and bounded worker resource admission remain.
- INV-009: A tenant-scoped Oracle cut never exposes another tenant's catalog
  pointer, and no object named by an active cut is reclaimed before its
  protection is released.
- INV-010: Parquet field IDs match the registered Iceberg table's assigned
  IDs; built-in declarations and fingerprints do not own numeric IDs.

## Acceptance criteria

- AC-001: Two replicas elect one scheduler; compactors on either pod pull,
  and a standby takes over after leader loss.
- AC-002: Lost hints or leader restart do not strand hot promotion; only
  successful Iceberg promotion counts toward compaction.
- AC-003: Count/interval, zero-commit, disablement, manual, concurrent-arrival
  and no-file cases match REQ-003/005/006.
- AC-004: Two eligible tables run on different workers; leader dispatches
  table-level tasks with no file scan or SQL demand claim.
- AC-005: Failed send, failure, timeout, stale report and failover preserve
  correct scheduling and publication outcomes.
- AC-006: Manifest rewrite precedes expiry; post-expiry cleanup respects
  reader pins, active watermarks, hot objects and unresolved outputs;
  maintenance membership after failover follows RisingWave's empty restart.
- AC-007: Superseded concurrent planning code, SQL, metrics, docs and tests
  are removed or rewritten without disabling a failing gate.
- AC-008: Capacity evidence reports leader decision p50/p99 and worker
  scale-out, without claiming unmeasured microsecond latency. Revised
  2026-10-03 with the human owner: Wyrd processes run within 8 CPU / 16 GiB
  in fixed per-process scopes; leader latency is measured open-loop at
  production and 10x pull rates from `bifrost_forge_leader_decision_seconds`
  plus a rate sweep to its knee; fleet throughput uses a production-like
  Scribe workload at default compaction settings and gates on 1.7x/3.0x
  scaling, full pull answers under backlog and leader CPU below 70%, naming
  the bounding resource. The 85% occupancy gate is removed. Amended
  2026-10-03: every Wyrd process keeps its 4 GiB boot floor
  (`MIN_POD_MEMORY_BYTES`), so the 16 GiB envelope holds one leader
  (1 CPU / 4 GiB) and at most three workers (7/3 CPU / 4 GiB each); the
  scaling gates are 2 workers >= 1.7x and 3 workers >= 2.5x of 1 worker.
  Worker throughput is measured as backlog drain: tables are written through
  Scribe with no worker running until each holds compactable staged files,
  then each fleet size drains an equal fresh backlog, so the measurement is
  bounded by compaction and not by the arrival rate of due tables.
- AC-009: A production-shaped Oracle journey proves one tenant-scoped SQL
  `SELECT` for all tables in a query, including cold and protected-cut cases,
  with fewer serial database steps than the current path. Separate evidence
  proves cross-tenant denial, exact hot/Iceberg results through publication
  and cleanup races, and safe release on cancellation or failure. Statement
  and step counts are asserted; no elapsed-time threshold is asserted here.
- AC-010: Fresh spans, points, records, and custom-table files promote and
  rewrite with field IDs matching their Iceberg table, with no declared-ID
  metadata rewrite. Forge still rejects a mismatched file.
- AC-011: Hot, promoted, and rewritten journeys prove actual Bloom exclusion
  for declared keys, including binary `trace_id`, and exact
  `wyrd_request_id` results with row-group min/max exclusion when the fixture
  has disjoint ranges. Binary page pruning and mixed predicates remain exact.

## Deletion and consumer map

Candidates, subject only to actual safety consumers:

- crates/vala/vala-bifrost-redux/src/forge/planning_scheduler.rs: roster
  reseed, durable planning, leader candidate scan, two-small-file test.
- crates/vala/vala-bifrost-redux/src/forge/scheduler.rs: concurrent demand
  and timer-paced planner orchestration.
- crates/vala/vala-sql/src/queries/forge_tasks.rs and forge_fair_claim.sql:
  planning claims, durable queue and fairness cursor portions. Actual
  unresolved publication/cleanup evidence remains only where needed.
- crates/vala/vala-sql/migrations/20261003000100_forge_concurrent_planning.sql:
  remove by dropping its branch-only commit. The older
  20260910000011_forge_planning_demands.sql remains migration history;
  retire its obsolete live schema with a narrow forward migration.
- crates/vala/vala-bifrost-redux/src/forge/metrics.rs, Forge integration
  tests, vala-sql Forge tests, Kubernetes/operator docs and
  architecture/bifrost-design.md: replace older planning-demand assertions
  with this contract. The revision-2 concurrent-planner tests disappear with
  their commits. Preserve any older user outcome that remains relevant.

## Open material decisions

None. Wyrd-specific requirements are hot Scribe publication recovery,
Oracle/hot-object protection at deletion, and charging all Forge DataFusion
rewrite reservations to Bifrost's central governor with governed spill
placement. The implementor owns the smallest library integration that meets
those requirements. Scheduler, physical file selection, worker pull, result
handling and maintenance behavior follow the pinned RisingWave Iceberg sources.

## Revision history and authority

- Revision 8 (2026-10-04, explicitly approved by the human owner for removing
  declared IDs and directed through review comments for pruning coverage):
  make Iceberg the field-ID owner and prove binary Bloom and request-ID
  filtering across hot and Iceberg cuts.
- Revision 7 (2026-10-04, explicitly approved by the human owner): make
  Iceberg catalog identity tenant relative for Oracle, retain Forge's platform
  authority, and require one protected SQL selection per Oracle query without
  moving latency measurement out of the benchmark change.
- Revision 6 (2026-10-03, directed by the user in this conversation): require
  source comparison of nimtable and Wyrd's fork; preserve central-governor
  charging and spill placement without mandating a dependency choice.
- Revision 4 (2026-10-03, directed by the user in this conversation):
  identical RisingWave worker pull dynamics on local and peer routes; use
  its Full-default library planner and empty-on-restart maintenance state.
- Revision 3 (2026-10-03, approved by the user in this conversation):
  replace concurrent durable planning with RisingWave-style Iceberg
  scheduling and worker ownership.
- Revision 2 (2026-10-03, superseded): concurrent durable claims.
- Revision 1 (2026-10-03, superseded): response to singleton bottleneck.

Authority: AGENTS.md; architecture/agent-rules.md;
architecture/wyrd-design.md; architecture/wyrd-doctrine.mdx;
architecture/bifrost-design.md (concurrent-planning section to revise
under this approved change); architecture/references/domain/iceberg.md.
