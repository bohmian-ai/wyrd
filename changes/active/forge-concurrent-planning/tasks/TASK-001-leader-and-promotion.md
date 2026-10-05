---
id: TASK-001
kind: implementation
status: review
spec: SPEC-forge-concurrent-planning
spec_revision: 6
requirements: [REQ-001, REQ-002, REQ-003, REQ-008, INV-001, INV-002, INV-005, AC-001, AC-002, AC-003]
depends_on: []
---

# One leader, Iceberg commit tracking, and promotion recovery

## Outcome and Value

One elected leader tracks Iceberg commits in memory and can recover promotion
of Scribe hot objects after a lost hint or leader failure. Ordinary scheduling
uses no durable per-table demand or next-due record. This replaces the
revision-2 multi-planner contract, not the Scribe-to-Oracle read boundary.

## Owners, Scope, Consumers, and Prohibited Changes

- Forge coordinator in vala-bifrost-redux owns election lifecycle, per-table
  scheduling state and promotion notification. Vala SQL owns only source
  evidence required for unpromoted hot objects and election. Extend the
  existing singleton election row with the elected coordinator's private
  peer URI in the same fenced acquire/renew lifecycle; no second discovery
  or leader registry.
- Scribe file_list publication is hot-object authority; only successful Forge
  Iceberg promotion adds one compaction count. Oracle consumes both sources.
- Use tenant-qualified physical table identity. Standby coordinators do no
  leader scheduling. No second election, SQL scheduling counter, demand row,
  generation acknowledgement, per-table planning claim or compatibility path.
- Follow the branch rollback in tasks/README.md. Preserve the dirty diff for
  recovery, then discard its revision-2 pass-cutoff edits; do not port them.
- Replica proof must reuse [WyrdTestCluster](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:1209) with `BifrostClusterSpec::two_mixed()`, `stop_node`, and `restart_node`; extend the existing [Forge production closeout journey](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs:244). Reuse its shared Postgres/storage, stable node IDs, Forge clock and worker observer. No second cluster builder, process launcher, fake replica registry, clock or election harness. A local fixture may only compose these owners.
- Extend the existing [Scribe promotion journey](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/tests/bifrost/forge/scribe_promotion.rs:1) for the hot-versus-Iceberg commit boundary; do not create a separate file-list publication fixture.
- Route a promotion notification to the leader by the same locality rule as
  [Gateway capture](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-server/src/components/gateway/capture.rs:642): same replica calls the leader in-process; a different pod uses the
  internal peer connection. Both paths enter the same update method. No
  process-local channel is treated as delivery to a remote leader.

## Approach

1. Keep the singleton election recovered by dropping the concurrent-planning
   commits; compare its startup/shutdown paths with
   RisingWave meta/node/src/server.rs:179-291 and
   meta/src/manager/iceberg_compaction/mod.rs:68-101 at e23ddf95.
2. Restore a single active scheduling owner; start with empty volatile
   tenant/table tracks and maintenance sets on leadership acquisition.
3. Feed tracks only from confirmed Iceberg promotion, retaining Scribe
   file_list as the recovery source for pending promotion. Add the narrow
   Forge promotion-notify method to the existing private peer router; a
   same-replica promotion calls the same update owner directly. Resolve a
   remote leader from the fenced election row, never from an unfenced hint.
4. Apply RisingWave's Idle eligibility rule and commit count capture from
   schedule.rs:40-70,125-159,199-255; keep policy refresh outside the
   in-memory decision path.
5. Delete concurrent demand-claim scheduling consumers as they become
   unreachable; leave actual publication evidence for TASK-002 and TASK-003 closure.

## RisingWave mechanism and required comparison

Use the pinned local RisingWave revision e23ddf952c3e6ebc03cc254789e84d1179cfacae.
The implementation report must give a Wyrd source location and a passing
test for every row; a source citation by itself does not prove behavior.

| RisingWave Iceberg behavior and source | Forge proof |
| --- | --- |
| Only the elected meta node constructs the Iceberg manager; followers do not schedule: [leader lifecycle](/home/thorrester/Documents/GitHub/risingwave/src/meta/node/src/server.rs:179). SQL election row heartbeat/expiry selects a replacement: [election](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/rpc/election/sql.rs:500). | Two-replica real-server journey: one active scheduler, standby compactor still works, takeover after leader stop/loss. Show the Forge election source and actual failover time. |
| Manager starts with empty track and maintenance maps: [manager initialization](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/mod.rs:84). | Restart assertion: ordinary counters, in-flight schedule and maintenance membership are gone. Only Scribe hot-promotion recovery is separate. |
| Sink committer sends a notification only after Iceberg commit: [post-commit send](/home/thorrester/Documents/GitHub/risingwave/src/connector/src/sink/iceberg/commit.rs:808); [leader receiver](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/stream.rs:30). | Scribe hot publication alone leaves the Iceberg count unchanged; one successful promotion emits one notification. Test lost hint/restart recovery against file_list. Do not add scheduler dedup state that RisingWave lacks. |
| A sink update refreshes settings when due, at a 60-second default interval, and updates expiry/manifest sets even if compaction is disabled: [sink update](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:657), [set membership](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:711), [refresh default](/home/thorrester/Documents/GitHub/risingwave/src/common/src/config/meta.rs:733). | Change policy while compaction is disabled: scheduling stops but enabled maintenance remains. No global timer refresh or catalog query in the warm commit decision. |
| Idle becomes due on commit threshold or elapsed interval with positive pending count: [due rule](/home/thorrester/Documents/GitHub/risingwave/src/meta/src/manager/iceberg_compaction/schedule.rs:136). Default interval 3600 s and count threshold usize::MAX: [sink defaults](/home/thorrester/Documents/GitHub/risingwave/src/connector/src/sink/iceberg/config.rs:783). | Exact boundary tests: zero commits past interval stays idle; one commit before interval waits; interval boundary dispatches; configured count threshold dispatches early; manual request forces dispatch. |

Compaction is disabled by default, matching RisingWave. Scribe
hot-publication recovery is the one Wyrd-specific recovery obligation.

## Ordered Implementation Scenarios

### Scenario 1 — Single leader and volatile state

**Behavior.** Two replicas elect one scheduling authority; the standby can
take over. The successor has no copied pending count or in-flight schedule.
Only the leader runs leader timer and dispatch work (REQ-001, INV-001/002).

**RED.** Add production_closeout::one_leader_failover_volatile_state
to the existing `wyrd-testing` Forge journey. It demonstrates the old scheduler
does not recover an empty volatile schedule on leader takeover. Run exactly:
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all -E "test(=production_closeout::one_leader_failover_volatile_state)"'

**GREEN.** Reuse the existing single-row election and add process-local
tracks. The 30-second leader expiry is separate from worker
report and publication deadlines; resign on graceful stop. Stop dispatch
immediately on lost leadership. Keep both replicas capable of
hosting worker execution. Publish/clear the leader peer URI with its owner
and fencing token so remote workers can dial only the current term; an
expired row is never a valid route.

**REFACTOR.** Remove the concurrent-planning owner/token/renewal loop and
standby-incompatible readiness/telemetry assertions once no consumer needs
them. Compare RisingWave server.rs:179-291.

### Scenario 2 — Hot publication does not count as Iceberg compaction demand

**Behavior.** A hot file_list commit remains Oracle-readable; only successful
Iceberg fast append increments one pending commit and records its snapshot.
No promotion writes a replacement data object (REQ-002, INV-005).

**RED.** Add
forge::production_routes::promotion_notification_only_after_iceberg_commit
to the existing redux integration target. The existing hint path reports
demand before exact promotion and does not prove the new counter boundary.
Run exactly:
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E "test(=forge::production_routes::promotion_notification_only_after_iceberg_commit)"'

**GREEN.** Notify the leader at the successful Iceberg commit boundary:
in-process on the same replica, private peer method across pods. Both call
one update owner and use exact hot file evidence and authority settlement.
RisingWave reference: connector/src/sink/iceberg/commit.rs:265-305,808 and
meta/src/manager/iceberg_compaction/stream.rs:30-41.

**REFACTOR.** Delete the Scribe-to-Forge scheduling hint as a compaction
counter source; preserve any wake needed for pending hot promotion.

### Scenario 3 — Hot promotion survives leader restart

**Behavior.** A killed leader's lost hint does not strand an unpromoted hot
object. The successor starts with empty ordinary compaction counters and
maintenance membership. A promotion
on the leader replica uses the in-process update; a promotion on another pod
uses the internal peer notification (REQ-002/008).

**RED.** Add
production_closeout::restart_recovers_hot_promotion_with_empty_schedule
to the existing `wyrd-testing` Forge journey. It must fail if promotion is
visible only through the dead process's memory or the successor inherits
old maintenance membership. Run exactly:
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all -E "test(=production_closeout::restart_recovers_hot_promotion_with_empty_schedule)"'

**GREEN.** On leadership acquisition, read existing file_list promotion debt
only. Leave the schedule and maintenance sets empty, as in RisingWave. No
catalog roster scan or persisted rewrite queue.

**REFACTOR.** Remove recurring full-catalog planning reseed and persisted
next-due state; retain only the hot-promotion recovery read.

## Acceptance Criteria

AC-001/002/003: exact hot vs Iceberg boundary, single leader failover,
zero-commit inactivity, configured interval and disabled count trigger,
manual force, and empty-on-restart membership are visible through the cited tests.
Later commits must remain for TASK-002's report handling.

## Expected Write Set and Consumer Closure

Likely: forge/scheduler.rs, planning_scheduler.rs, scribe_promotion.rs,
Scribe hint sender, coordinator/server startup and readiness, catalog roster
consumer, singleton election SQL/row type, private peer promotion-notify wire
and router, vala-sql demand queries, Forge promotion and two-replica journeys.
The revision-2 task files have been removed from this packet and do not
guide new behavior.
Inspect each existing test before deletion: replace old concurrency assertions
with the same user-visible journey.

## Verification and Evidence

Run only the focused exact scenario commands above for RED and GREEN.
The new private peer notification wire also runs `mise run test:tonic` and
`mise run codegen:check`; neither is part of `verify:bifrost`.
Confirm each selector selects one test; record the test result and Forge
source link. TASK-003 alone runs the integrated verify:bifrost aggregate.
The task report must include the table above with source locations, focused
test names/commands and observed results, plus the two deliberate differences.
List the existing election, cluster, clock and promotion owners reused;
identify every new fixture/helper and why an existing owner could not serve it.
Duplicate replica or election machinery fails this task.

## Material Stop Conditions

Stop if no existing election boundary can prevent an old leader dispatch
after takeover, or if promotion recovery cannot distinguish committed hot
files from exact already-promoted files without a new persistent contract.

## Authority Links

Approved ../spec.md revision 6; AGENTS.md §§2,9,11;
architecture/bifrost-design.md §§Scribe hot promotion, Maintenance;
RisingWave e23ddf95 sources listed in Approach and scenarios.

## Implementation Evidence

Status: IMPLEMENTED. The Scenario 3 REFACTOR (removing catalog-wide
planning, demand rows and `schedule_once`) landed with TASK-003 in 37a526d61.
Commits 431eccbfc, 89f2441cc, fa560aa14, 85f7f2b24, a367d9f0c, fce4cfd7c,
and the boot-election fix c6ceefb0a.

### RisingWave comparison (pinned e23ddf95)

| RisingWave behavior | Forge source | Focused test / command result |
| --- | --- | --- |
| Only the elected node schedules; SQL row heartbeat/expiry elects a replacement (server.rs:179, election/sql.rs:500) | [ForgeLeadership::heartbeat](crates/vala/vala-bifrost-redux/src/forge/leadership.rs:159), [resign](crates/vala/vala-bifrost-redux/src/forge/leadership.rs:206), 30 s term / 10 s heartbeat ([leadership.rs:31](crates/vala/vala-bifrost-redux/src/forge/leadership.rs:31)); SQL [forge_leader.rs:71](crates/vala/vala-sql/src/queries/forge_leader.rs:71) | `production_closeout::one_leader_failover_volatile_state`: PASS; takeover after leader stop observed in about 79 ms |
| Manager starts with empty track and maintenance maps (iceberg_compaction/mod.rs:84) | [ForgeSchedule::new](crates/vala/vala-bifrost-redux/src/forge/leader.rs:270) built per acquired term | `production_closeout::restart_recovers_hot_promotion_with_empty_schedule`: PASS; `one_leader_failover_volatile_state`: PASS |
| Notification only after Iceberg commit (commit.rs:808, stream.rs:30) | [notify_promotion_commit](crates/vala/vala-bifrost-redux/src/forge/scribe_promotion.rs:708) → in-process [ForgeLeadership::notify](crates/vala/vala-bifrost-redux/src/forge/leadership.rs:255) or peer [notify_promotion](crates/wyrd/wyrd-server/src/grpc/forge_peer.rs:74); single update owner [ForgeSchedule::notify_commit](crates/vala/vala-bifrost-redux/src/forge/leader.rs:287) | `forge::production_routes::promotion_notification_only_after_iceberg_commit`: PASS (parked commit → no track; released → pending_commits 1) |
| Sink update refreshes settings and maintenance membership even when compaction is disabled (schedule.rs:657,711) | [apply_membership](crates/vala/vala-bifrost-redux/src/forge/leader.rs:337); settings from the commit's returned metadata ([settings.rs](crates/vala/vala-bifrost-redux/src/forge/settings.rs:112)) | `forge::leader::tests::disabled_compaction_keeps_maintenance_membership`: PASS; `forge::settings::tests::settings_default_to_risingwave_and_parse_overrides`: PASS |
| Idle due on count threshold or interval with pending > 0; defaults 3600 s / usize::MAX (schedule.rs:136, config.rs:783) | [should_trigger](crates/vala/vala-bifrost-redux/src/forge/leader.rs:135), [force](crates/vala/vala-bifrost-redux/src/forge/leader.rs:156), [ForgeTableSettings::default](crates/vala/vala-bifrost-redux/src/forge/settings.rs:93) | `forge::leader::tests::due_rule_boundaries_match_risingwave`: PASS (zero commits stay idle, pre-interval waits, boundary dispatches, count early, manual force) |
| Reports keep later commits; stale task ids are ignored | [report](crates/vala/vala-bifrost-redux/src/forge/leader.rs:428) | `forge::leader::tests::reports_preserve_later_commits_and_ignore_stale_tasks`: PASS (later-report handling continues in TASK-002) |
| Pull picks oldest due tables up to the limit | [pull](crates/vala/vala-bifrost-redux/src/forge/leader.rs:366) | `forge::leader::tests::pull_selects_oldest_due_tables_up_to_limit`: PASS |

Approved Wyrd difference used here: Scribe hot-publication recovery. A new
leader reads only `file_list` promotion debt
([sweep_promotion_debt](crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:344)) on acquisition and on each
planning pass. No ordinary counter is recovered.

Interim difference, marked with a `ponytail:` comment in
[lead](crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:297): the old `schedule_once` still plans rewrite and
maintenance until TASK-002/003 replace it. A pass that finds promotion owed
skips it, which keeps the old order where promotion outranks rewrite.

### Diagnoses recorded during GREEN

- **Duplicate promotion after a retryable failure**
  - *Symptom:* `scribe_promotion_ambiguity_reconciles_without_recommit` and
    `..._lease_loss_and_takeover_settle_once` failed with "Cannot add files
    already referenced".
  - *Evidence:* WYRD_LOG trace showed a retryable promotion task followed by
    a new claimed promotion on the same table.
  - *Cause:* `ForgeTasks::insert_claimed` conflicted only on the active index
    (claimed/running/prepared).
  - *Fix site:* `insert_claimed` (crates/vala/vala-sql/src/queries/forge_tasks.rs)
    now refuses when a ready or retryable task of the same table and strategy
    is queued. The pg test `insert_claimed_records_one_active_attempt_per_table`
    was extended first (RED, then GREEN).
- **Rewrite planned over a queued promotion**
  - *Symptom:* after that fix, the successor ran `small_files` instead of the
    retry.
  - *Cause:* `promote_table` reported "executed here", so `schedule_once` ran.
  - *Fix site:* `promote_table` and `sweep_promotion_debt` now report "owed".
    A failed promotion also counts as owed.
- **Missing successful pass span**
  - *Symptom:* `live_rewrite.rs:929` found no successful pass span.
  - *Cause:* the pass span recorded `leader` instead of the established
    `succeeded`.
  - *Fix site:* `run_pass`.
- **Rewrite operations counted before the measurement window**
  - *Symptom:* `production_closeout.rs:1392` counted 3 operations against 1
    snapshot.
  - *Cause (independent read-only diagnostician):* the leader promotes inside
    its boot pass, as REQ-008 requires. The test read the promoted cut after a
    driven pass that had already rewritten it.
  - *Fix site:* the test reads the cut after `await_boot_pass`. The same
    reorder was applied to the `ReaderCleanupJourney` setup.
- **Readiness reported on incomplete or failed planning**
  - *Symptom:* three `pg_router_smoke` readiness tests reported ready after
    an incomplete or failed planning pass.
  - *Cause:* readiness dropped the old `standby || !incomplete` rule.
  - *Fix site:* `lead` returns completeness and `run_pass` publishes it.
- **Object-store readiness test timed out**
  - *Symptom:* `coordinator_object_store_failure_clears_readiness` timed out.
  - *Cause (independent read-only diagnostician):* the observer hold that used
    to park the worker now parks the inline promotion inside the pass.
  - *Fix site:* the test drops the hold. A completed pass with a snapshot
    already proves the attempt settled.

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 single leader, failover, volatile state | leadership.rs, scheduler.rs `lead` | `mise run test:bifrost:journey:forge` 17/17 | PASS |
| AC-002 hot vs Iceberg boundary, restart recovery | scribe_promotion.rs, scheduler.rs sweep, forge_peer.rs | redux `--test integration -E 'test(/^forge::/)'` 53/53; journey 17/17 | PASS |
| AC-003 zero-commit inactivity, interval, disabled count, manual force, empty membership | leader.rs, settings.rs | `cargo nextest run -p vala-bifrost-redux --lib -E 'test(/forge::(leader\|settings\|leadership)/)'` 5/5 | PASS |
| Single attempt per table, including queued retries | forge_tasks.rs `insert_claimed` | vala-sql `pg_forge_tasks` 27/27 | PASS |
| Coordinator readiness and pass contract | scheduler.rs `run_pass` | wyrd-server `pg_router_smoke` 36/36 | PASS |
| Private peer wire | wyrd-tonic proto, forge_peer.rs | `mise run test:tonic` 39/39; `mise run codegen:check` pass; `mise run check:proto-drift` pass | PASS |
| Format, lints, whitespace | — | `mise run fmt`, `mise run lints`, `git diff --check` clean | PASS |

Reused owners:
- `WyrdTestCluster` (`two_mixed`, `stop_node`, `restart_node`)
- `ForgeWorkerCompletionObserver`
- the existing singleton election row (`forge_scheduler_state`)
- `ForgeWorker` (as the coordinator's executor)
- `PromotionCatalogSeam` and `SupervisedPromotion`
- the Forge clock

New helpers, and why no existing owner could serve them:
- `ForgeLeadership`: holds term state.
- `ForgeLeaderPeer`: the peer notify route.
- `LeaderJourney`: composes the existing cluster owners only.
- `promote_owed`, `promotion_task_for_test`, `promote_for_test`: tests drive
  the coordinator's promotion route, which has no worker queue.
- `request_pass`/`await_pass` and `coordinator_stop`: a pass now blocks on
  inline promotion, so fixtures must observe the attempt before awaiting the
  pass.
- `release_parked_commit`: the notification test releases a parked commit.

Non-goals stayed excluded: no worker pull, no physical planning change and no
maintenance timer (TASK-002/003). `verify:bifrost` is left to TASK-003 as the
packet requires.
