# Concurrency Domain Review

## Verdict

**FAIL** for immutable candidate `7ac45dec99535c881b7a936c66623044f15d8823`
over `c1508b375..7ac45dec9`.

The active-read/deletion protocol, pull ordering, stale-report handling, and
promotion settlement inspected here are internally coherent. The candidate is
not concurrency-safe as a packet, however: a coordinator can continue acting
as leader after its PostgreSQL term expires, and a narrow shutdown race turns
leader-dispatched work into independently claimable durable queue work. Both
are reachable production paths and contradict revision-11 leader/ownership
requirements.

## Boundary and authority coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Leader term, failover, volatile state | Spec REQ-001/006/008 and INV-001..004; TASK-001 scenarios 1-4; `forge/{scheduler,leadership,leader}.rs`; `vala-sql/src/queries/forge_leader.rs`; two-replica closeout journeys | **Fail**: findings CONC-001 and CONC-002 |
| Worker pull, capacity, oldest-due and stale reports | Spec REQ-003/004/006/009; TASK-002; `forge/{leader,worker}.rs`; `vala-sql/src/queries/forge_tasks.rs`; pinned RisingWave `schedule.rs` at `e23ddf952c3e6ebc03cc254789e84d1179cfacae`, especially timeout-before-selection and oldest-due ordering at lines 915-978 | **Fail** only at the dispatch/shutdown ownership seam (CONC-003); due ordering, pull cap, later-commit preservation and stale-task rejection match the required shape |
| Exactly-once promotion settlement | Spec REQ-002/008 and INV-005; TASK-001; `forge/{scheduler,scribe_promotion,worker}.rs`; promotion and production-route tests | **Pass** for the inspected transition: notification is emitted after the catalog commit, recovered publication does not emit a second notification, and durable promotion evidence is the recovery owner. Leader liveness around the sweep fails separately in CONC-001 |
| PostgreSQL-owned coordination time | `forge_leader.rs`, `forge_tasks.rs`, reader-authority migration, Forge cleanup queries; test-fix commits `05cceaf35`, `484c3b4f6`, and `3a51a24d5` | **Pass** for durable eligibility/expiry decisions: leader lease, task leases/retries, active-read abandonment, and corrected journey brackets use PostgreSQL time. Tokio/Forge clocks remain only for volatile cadence/schedule decisions and operation-local budgets |
| Active-read claim acquisition, descendants, deadline, release and drop | Spec revision 11 active-read text; TASK-005-R1; `20260910000025_oracle_reader_authority.sql`; `oracle/{planner,mod,query_stream}.rs`; `forge_operations.rs`; reader/expiry journeys | **Pass**: one tenant-scoped acquisition call records rows before object IO; PostgreSQL derives `abandon_after`; batch sources and distributed/analytical descendants settle before awaited release; `Drop` spawns the same release and safely falls back to deadline protection without a runtime |
| Destructive maintenance interleavings | Spec REQ-007 and revision-11 active-read rules; TASK-003/TASK-005-R1; `forge/{gc,expire,orphan_gc,table_authority}.rs`; `forge_operations.rs`; held-reader, expiration, orphan and closeout tests | Table authority plus refreshed active-read/root checks protect object correctness, but **leader ownership fails** because an already-started pass survives loss of the term (CONC-002) |
| Test-fix integrity | Diffs for `05cceaf35`, `484c3b4f6`, `3a51a24d5`; current assertions in `snapshot_expiration`, `production_routes`, `production_closeout`, `eval_verification`, `published`, and reader-expiry tests | **Pass**: clock changes align assertions with the clocks that stamp the state; held-reader changes strengthen the revision-11 rule; the orphan age floor is narrowed only for committed expired candidates while never-published attempt generations retain it. The remaining sibling-route ban in `production_closeout.rs:539-543` is scenario-local and did not mask either finding |

Repository authority read for this audit included `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`,
`architecture/references/README.md`, the analytical reliability and Iceberg
references routed by the navigation map, the revision-11 spec and revision
history, the packet task index, and concurrency-relevant task/evidence files.
Revision 11 was treated as authoritative where older task prose still mentions
retention or reader-protection designs.

## Findings

### CONC-001 — Important — INCORRECT — promotion work can outlive the PostgreSQL leader term and leave a locally authoritative stale leader

**Source:**

- `crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:206-224`
- `crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:337-350`
- `crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:366-391`
- `crates/vala/vala-bifrost-redux/src/forge/leadership.rs:145-155`
- `crates/vala/vala-bifrost-redux/src/forge/leadership.rs:295-301`
- `crates/vala/vala-bifrost-redux/src/forge/leadership.rs:379-386`
- `crates/vala/vala-sql/src/queries/forge_leader.rs:105-127`

The only renewal loop awaits both `promote_hinted(...)` and the complete,
sequential `sweep_promotion_debt(...)`. Those operations perform catalog,
object-store, planning, publication and SQL IO and have no bound below the
30-second leader TTL. While either await is in progress, no heartbeat runs. If
it exceeds the TTL, PostgreSQL permits a standby to acquire a new token, but
the old process retains `held: Some(...)` because local state is cleared only
by its next renewal attempt.

This is not merely stale bookkeeping. The in-process `notify`, `pull`, and
`report` routes trust `held().is_some()`, and `term()` checks only the cached
token. During that interval the expired process still accepts promotion
notices and serves dispatches while the successor legitimately leads with a
new empty schedule. Revision-11 REQ-001 requires exactly one dispatcher and
requires loss of leadership to stop dispatch immediately; TASK-001 explicitly
separates the 30-second election expiry from publication duration.

The current `one_leader_failover_volatile_state` journey stops the leader
gracefully (`production_closeout.rs:2421-2429`) and therefore cannot expose
lease expiry during slow or stalled leader work.

**Required correction:** keep renewal independent of potentially unbounded
promotion work and revoke local authority as soon as the PostgreSQL term can
no longer be proven live. Add a deterministic two-replica test that holds a
hint/sweep past the term, proves the successor acquires a larger token, and
proves the former leader refuses local and peer pull/notice/report operations
and stops the held work.

### CONC-002 — Important — INCORRECT — an in-progress maintenance pass continues after heartbeat loss drops the term

**Source:**

- `crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:168-171`
- `crates/vala/vala-bifrost-redux/src/forge/leadership.rs:168-186`
- `crates/vala/vala-bifrost-redux/src/forge/gc.rs:152-191`

Supervision and maintenance run concurrently. A failed/replaced renewal clears
`ForgeLeadership.held`, but `run_maintenance` captures an `Arc<ForgeHeldTerm>`
once, borrows its schedule, and then runs every rewrite, expiration and orphan
cleanup with only process-shutdown checks. Clearing the shared `held` slot does
not invalidate that captured `Arc`, and no table iteration or destructive
boundary revalidates the term/token.

Consequently a former leader can continue the old term's timer pass after a
successor starts its own leader timer. Per-table leases and maintenance
authority still protect same-table storage transitions, but they do not
satisfy REQ-001's single timer owner or its explicit “loss of leadership stops
... leader timer work” rule; the old pass can continue across other tables
using membership that belongs to a dead volatile term.

Existing standby/empty-membership tests invoke a new pass after failover. They
do not revoke leadership while a pass is already paused inside a table
operation.

**Required correction:** bind a maintenance pass to a revocable term guard and
check/cancel it before every table and before each durable destructive effect.
Add a deterministic failover test paused inside a maintenance pass, replace
the term, resume the former leader, and prove it performs no further rewrite,
expiration, cleanup, or orphan work while the successor alone owns the timer.

### CONC-003 — Important — INCORRECT — shutdown after dispatch admission releases leader-owned work into the ordinary durable queue

**Source:**

- `crates/vala/vala-bifrost-redux/src/forge/worker.rs:2540-2605`
- `crates/vala/vala-bifrost-redux/src/forge/worker.rs:2703-2759`
- `crates/vala/vala-bifrost-redux/src/forge/worker.rs:3035-3061`
- `crates/vala/vala-bifrost-redux/src/forge/worker.rs:3587-3606`
- `crates/vala/vala-bifrost-redux/src/forge/worker.rs:3894-3907`
- `crates/vala/vala-bifrost-redux/src/forge/worker.rs:8555-8603`
- `crates/vala/vala-sql/src/queries/forge_tasks.rs:159-236`
- `crates/vala/vala-sql/src/queries/forge_tasks.rs:593-644`

`admit_dispatch` checks shutdown before loading the table, then awaits the load
and `insert_claimed`, records the dispatch in `self.dispatched`, and calls
`start_claim`. If shutdown arrives during those awaits, `start_claim` takes its
early shutdown branch and calls the generic `release_claim_at_shutdown`.
That helper writes the row as immediately `retryable`; it neither uses the
already-implemented `close_dispatched` transition nor removes/reports the
current dispatch. `pull_claimed_tasks` reports `NotStarted` only for the
remaining iterator entries when `admit_dispatch` returns `None`.

The SQL owner documents the violated invariant directly at
`forge_tasks.rs:620-628`: a leader-dispatched row must never become retryable,
because `claim_fair` can then give it to a second owner whose outcome the
leader does not receive. This race does exactly that. The leader keeps the
table `InFlight` until report timeout while another worker can execute the
durable row through the ordinary claim path; the later leader retry can then
create another execution episode. This violates the leader-owned pull/result
protocol and the at-most-one-current-task requirement.

Existing cancellation tests exercise ordinary claimed attempts and in-episode
dispatched failures, which do route through `close_dispatched`. No test covers
shutdown after `insert_claimed` commits but before `start_claim` opens the
episode.

**Required correction:** make this admission-cancellation edge preserve
dispatch ownership: close the accepted row through the dispatch-specific
terminal transition, remove its dispatch bookkeeping, and report the current
dispatch as not started (or failed under the packet's chosen report contract)
before stopping. Add a deterministic pause after dispatched insertion and a
shutdown test proving the row is never fair-claimable and the leader becomes
eligible through exactly one report path.

## Passed concurrency assessment

- The leader schedule expires timed-out entries only during a pull and selects
  oldest-due entries first, matching the pinned RisingWave shape. Matching
  success subtracts the captured count, later commits survive, and stale task
  IDs do not mutate replacement work.
- The active-read acquisition and destructive-preparation lock the same
  tenant-qualified maintenance-authority row. Every inspected expiration,
  expired-object cleanup, and orphan deletion path refreshes active reads and
  catalog/protection roots at its destructive boundary.
- `oracle_active_table_reads.abandon_after` is calculated by PostgreSQL from
  the query's remaining deadline, and cleanup removes rows only at
  `abandon_after <= statement_timestamp()`. No reader-fence liveness or fixed
  six-hour lifetime remains in this decision.
- `query_stream` drops batch sources, joins distributed/analytical descendants,
  and only then awaits claim release. An abandoned owner uses the same release
  asynchronously; failure or absence of a runtime remains fail-safe because
  the row blocks cleanup until its deadline.
- Promotion recovery uses durable operation evidence. A normal committed
  promotion notifies after the catalog commit; recovered committed evidence is
  settled without sending a duplicate notification.
- The reviewed green-gate fixes change the clock or expected revision-11
  behavior rather than weakening safety assertions. In particular, the held
  reader is now exercised across three destructive passes, and expired
  candidates no longer inherit an orphan age delay while never-published
  attempt generations still do.

## Verification notes and limits

- Re-ran:
  `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/forge::leader::tests/)'`
  — 5 passed, 823 skipped.
- Reviewed the requester's recorded green results for `mise run
  verify:bifrost`, `mise run test:principals:integration`, formatting, lints,
  and `git diff --check`; this domain audit did not rerun the full Postgres,
  RustFS, multi-replica, or release benchmark lanes.
- The pinned RisingWave repository/commit was present and read by immutable
  `git show`; no moving upstream source was used.
- CONC-001 and CONC-002 need deterministic lease-expiry/failover fault seams;
  current tests expose only graceful resignation or passes begun after the
  role transition. CONC-003 needs a pause at the post-`insert_claimed`,
  pre-episode boundary. Running existing green lanes cannot falsify these
  interleavings because none schedules those boundaries.
- No production source was modified. This report is the only file written by
  this reviewer.
