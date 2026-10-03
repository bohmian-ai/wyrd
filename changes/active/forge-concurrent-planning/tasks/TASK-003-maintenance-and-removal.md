---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-forge-concurrent-planning
spec_revision: 6
requirements: [REQ-007, REQ-008, REQ-010, INV-005, INV-006, INV-008, AC-006, AC-007]
depends_on: [TASK-001, TASK-002]
---

# Leader timer, protected cleanup, and old-machinery removal

## Outcome and Value

The elected leader runs manifest rewrite, snapshot expiration and cleanup on
the RisingWave Iceberg timer model. A new leader starts with empty maintenance
membership. The concurrent planning subsystem and tests
that assert it are removed, while Scribe hot objects and Oracle-pinned cuts
remain protected.

## Owners, Scope, Consumers, and Prohibited Changes

- Forge owns timer execution, Iceberg maintenance and protected object
  cleanup. Oracle owns reader pins; Scribe file_list owns hot-object
  publication. Vala SQL owns only durable evidence still required for
  promotion, pinned reads or destructive-action recovery.
- Manifest rewrite, snapshot expiry, expired-file cleanup and orphan cleanup
  are distinct actions. A timer does not itself authorize deletion.
- Do not keep the old per-table schedule row, planning-demand claim or SQL
  task queue merely to invoke a timer. Do not delete a failed test or weaken
  its assertion without replacing the user-visible behavior it protected.
- Maintenance and failover proof must extend [Forge production closeout](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs:244) and its [WyrdTestCluster stop/restart and Forge clock](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:2126). Reuse [CommitUncertaintyCatalog](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/src/bifrost/forge_harness.rs:149) and existing Oracle reader/cleanup journeys for adverse commit and pin cases. No second catalog-fault wrapper, fake timer, replica fixture, object-deletion simulator or parallel cleanup proof harness. Extend the existing test owner only for a missing scenario.
- Keep focused engine-level expiry tests in the existing [Forge integration test modules](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/tests/integration/forge/expired_cleanup.rs:1) and [reader-expiry ordering module](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/tests/integration/forge/reader_expiry_ordering.rs:1); do not copy their setup into the cross-pod journey.
- The leader runs its own maintenance locally. Any worker result it consumes
  follows TASK-002's hard local/peer rule; do not add a maintenance-specific
  RPC, queue or transport switch.

## Approach

1. Put the maintenance timer on the active leader, matching RisingWave
   meta/src/manager/iceberg_compaction/gc.rs:132-207 at e23ddf95.
2. Evaluate manifest rewrite first; group eligible data manifests by
   partition spec and skip unsupported v3, matching gc.rs:39-118,328-482.
3. Then evaluate snapshot expiry with active-task watermark and existing
   Wyrd reader/operation protection; commit expiry before eligible file
   cleanup, matching gc.rs:209-325.
4. Reuse existing safe deletion checks for expired and never-published
   objects, with current tenant/table and hot/pin/unresolved roots.
5. Remove obsolete planning SQL/code/metrics/tests through a forward
   migration, then align architecture, deployment docs and verification.

## RisingWave mechanism and required comparison

Use local RisingWave revision e23ddf952c3e6ebc03cc254789e84d1179cfacae.
The implementation report must pair every source link with the Forge owner
and a passing test; a deliberate Wyrd difference must be named and justified.

| RisingWave Iceberg behavior and exact source | Forge proof |
| --- | --- |
| The configurable hourly leader loop copies the two maintenance sets, rewrites manifests first, then processes snapshot expiry; one table's failure does not abort others: [GC loop](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/gc.rs:132), [per-table work](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/gc.rs:164). | Timer-order and per-table failure-isolation integration test; standby runs no timer. |
| Manifest rewrite takes data manifests below target size, groups by partition spec, orders old first, and rewrites a batch with at least two manifests when target size or minimum merge count is met: [manifest plan](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/gc.rs:39), [commit](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/gc.rs:423). Format v3 is skipped: [format guard](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/gc.rs:345). | Fragmented same-spec manifests rewrite; one manifest, different specs, disabled setting and v3 do not rewrite. |
| Expiry computes now minus configured age (24-hour fallback), clamps to an in-flight task's watermark or skips without one, and skips when no snapshot is old enough: [age fallback](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/gc.rs:126), [watermark](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/gc.rs:219), [oldest check](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/gc.rs:264). | Boundary-time, in-flight watermark, missing-watermark and no-old-snapshot cases. Wyrd also protects Oracle pins and unresolved operations. |
| Expiry transaction commits before cleanup_expired_files; cleanup can fail after metadata commits: [expiry commit and cleanup](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/gc.rs:290). | Simulate cleanup failure after catalog success; retry deletes only files now proven unreachable and preserves current hot/reader/operation roots. |
| New Iceberg manager starts with empty maintenance sets after leader restart: [initialization](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/mod.rs:84). | Test no cold-table maintenance until its next Iceberg commit or manual request. Wyrd's separate never-published orphan sweep remains only for hot-object and unresolved-output safety. |

## Ordered Implementation Scenarios

### Scenario 1 — Manifest rewrite and expiry run in order

**Behavior.** Enabled table gets manifest rewrite before snapshot expiry on
the leader timer. A disabled option is skipped; v3 manifest rewrite is
refused; failure of one table does not stop later tables (REQ-007, AC-006).

**RED.** Add
forge::production_routes::leader_timer_rewrites_manifests_before_expiry
to the existing redux integration target for order, disablement and
per-table error isolation. The old planner emits durable periodic tasks
rather than this leader timer flow. Run exactly:
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E "test(=forge::production_routes::leader_timer_rewrites_manifests_before_expiry)"'

**GREEN.** Let one leader own the configurable hourly timer. Reuse Iceberg's
manifest transaction and existing table settings. Manifest rewrite remains
off by default; snapshot expiration becomes on by default with RisingWave's
24-hour age fallback and unset retain-last. Remove the old 32-commit
maintenance trigger. RisingWave's precise
reference is gc.rs:39-118,132-207,328-482.

**REFACTOR.** Remove persisted next_due_at scheduling and per-table
maintenance-demand materialization, not the underlying Iceberg operations.

### Scenario 2 — Expiration protects active work and Oracle readers

**Behavior.** Expiry uses configured age/retention, clamps cutoff to a
running task's observed snapshot, and skips when no safe watermark exists.
An Oracle cut, hot Scribe object, or unresolved operation prevents unsafe
deletion. Successful expiry precedes deletion of its uniquely expired files
(REQ-007, INV-005/006).

**RED.** Add
forge::snapshot_expiration::active_watermark_and_oracle_pin_block_expiry
to the existing redux integration target with an active compaction and
pinned Oracle cut. Include catalog-success/file-cleanup-failure followed by
safe retry using the existing expired-cleanup fault seam. A timer-only
rewrite that omits protection must fail.
Run exactly:
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E "test(=forge::snapshot_expiration::active_watermark_and_oracle_pin_block_expiry)"'

**GREEN.** Apply RisingWave's watermark rule from gc.rs:219-262 and its
expiry-then-cleanup sequence from gc.rs:264-316. Reuse Wyrd's
forge/reader_protection.rs:92-125, expiry_policy.rs:26-45 and
protection_roots.rs:19-65 for the actual Wyrd data sources.

**REFACTOR.** Eliminate stale candidate caches or duplicate retention
layers after proving that the final destructive check sees current roots.

### Scenario 3 — Empty maintenance restart and protected orphan cleanup

**Behavior.** A new leader has empty maintenance membership; a cold table
joins only after its next Iceberg commit or manual request. Never-published
orphan cleanup checks current Iceberg, Scribe and
Oracle roots before each deletion. Missing objects settle idempotently only
after safety validation (REQ-008, INV-006, AC-006).

**RED.** Add
production_closeout::empty_maintenance_restart_protects_orphans
to the existing `wyrd-testing` Forge journey for empty membership, rejoin
after a new commit, and unresolved-output protection. Run exactly:
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all -E "test(=production_closeout::empty_maintenance_restart_protects_orphans)"'

**GREEN.** Leave the new leader's maintenance sets empty and reuse the
current protected deletion boundary. RisingWave's Iceberg gc.rs has
post-expiry file cleanup but no equivalent never-published orphan sweep;
this Wyrd action is justified by hot objects and uncertain outputs.

**REFACTOR.** Delete only old durable maintenance schedule and task
preparation layers with no remaining cleanup evidence consumer.

## Acceptance Criteria

AC-006/007: ordering, retention, watermark, empty failover membership, protected
expired-file and orphan cleanup, and complete removal of concurrent
planning assertions have executable or static proof. Every old test is
classified as rewritten for a preserved behavior or removed because its
claim protocol no longer exists; deleting a failing test is prohibited.

## Expected Write Set and Consumer Closure

Likely Forge gc/manifest/expiry/orphan modules, scheduler.rs,
planning_scheduler.rs, metrics.rs, vala-sql forge_tasks.rs and
forge_fair_claim.sql, a narrow migration for older planning-demand schema,
SQL row types, server wiring,
tests under vala-bifrost-redux/tests/integration/forge,
vala-sql/tests/pg_forge_tasks.rs and wyrd-testing Forge/Oracle journeys,
architecture/bifrost-design.md and Kubernetes/operator documentation.
Specific deletion candidates remaining after the branch rollback:
forge_planning_demands, forge_table_maintenance next-due state, older
planner claims and stale planning-debt metrics. Concurrent claim columns,
generation acknowledgement, failed-pass cutoffs, worker fairness cursor,
their tests and revision-2 docs disappear with the dropped commits.
Inspect all consumers of forge_tasks before deleting it: unresolved
publication, Oracle authority and cleanup may still need narrower evidence.

The implementor must account for these specific current owners before
deleting code or tests:

| Current Forge logic or proof | Required disposition |
| --- | --- |
| [planning_scheduler.rs](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/forge/planning_scheduler.rs:432) older planner loop, roster reseed and leader-side small-file candidate test | Delete after TASK-001/002 leader state and worker-current-head proofs pass. |
| [scheduler.rs](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:180) hint-to-demand and timer-paced planner orchestration; [planner.rs](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/forge/planner.rs:1) pre-dispatch plan/hash selection | Remove obsolete scheduling and second selection while retaining the narrowly needed promotion wake and the TASK-002 worker execution outcome. |
| [forge_tasks.rs](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-sql/src/queries/forge_tasks.rs:202) demand upsert/claim/renew/ack, durable next-due/roster queries, and [fair claim SQL](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-sql/src/queries/forge_fair_claim.sql:1) | Remove scheduling/queue consumers and SQL; inspect publication, Oracle and cleanup consumers of the same forge_tasks table before narrowing or dropping it. |
| [planning-demand migration](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-sql/migrations/20260910000011_forge_planning_demands.sql:3) and forge_table_maintenance state | Keep the older migration as history; add a narrow forward migration for obsolete live schema after consumer removal. The 20261003000100 concurrent migration disappears with its branch-only commit. |
| [planning metrics](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/forge/metrics.rs:34) and fleet debt SQL | Remove claim/pass/backlog metrics that no longer describe the leader, replacing only decision latency and worker progress measures used by TASK-002. |
| Forge integration and SQL tests at the old baseline | Remove older planning-demand assertions after their user outcomes are covered by TASK-001/002 tests; retain or rewrite healthy-table progress, failover and tenant isolation. Revision-2 concurrency tests disappear with their commits. |
| [expired_cleanup.rs](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/tests/integration/forge/expired_cleanup.rs:125), [reader expiry ordering](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/tests/integration/forge/reader_expiry_ordering.rs:141) | Replace task-claim fixture setup with leader-timer setup; keep assertions that Oracle readers and protected objects survive cleanup. |

## Verification and Evidence

Run the three focused exact commands above during RED/GREEN.
After TASK-001 through TASK-003 are integrated, run mise run verify:bifrost
once as the full Bifrost suite. Do not also run its component Bifrost lanes
as final verification. Run mise run docs:check only if docs/ changes,
because that site check is outside verify:bifrost. If a named test moves,
confirm its exact nextest selector and update its command before running.
Report an explicit removed-code and removed-test inventory; no silent
skip/ignore or gate weakening. REQ-010/AC-007 are static deletion and
documentation obligations, so no manufactured RED applies: inspect
git diff and run rg across production, migrations, tests and docs for
obsolete planner symbols. Retire older SQL with a forward migration; never edit
already-applied migrations. Rewrite tests for retained user behavior.
The completed task includes the comparison table above with exact Forge
source and test links, including expiry commit-before-cleanup failure proof.
Inventory reused cluster, clock, catalog-fault and Oracle pin/cleanup owners;
justify every new fixture against those owners. Duplicate maintenance test
infrastructure fails this task.

## Material Stop Conditions

Stop if a proposed deletion removes the only record needed to protect a
possibly published object, or if a destructive cleanup candidate cannot be
revalidated after leader failover. Narrow that evidence to the actual
consumer rather than reinstating the old planner.

## Authority Links

Approved ../spec.md revision 6; AGENTS.md §§2,11,12;
architecture/bifrost-design.md §§Maintenance, Convergence and cleanup;
RisingWave e23ddf95 gc.rs references in Approach and scenarios.
