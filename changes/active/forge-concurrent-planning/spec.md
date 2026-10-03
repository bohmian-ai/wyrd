---
id: SPEC-forge-concurrent-planning
revision: 1
status: approved
---

# Forge concurrent planning

## Objective and user value

Forge maintenance (Scribe-hot promotion, compaction, snapshot expiry, orphan
cleanup) must keep up with a multi-tenant, multi-replica deployment under high
ingest load. Today one replica plans for the whole fleet behind a 15-minute
singleton lease: planning is paced by a 60 s timer at 256 tables per pass,
every table is re-seeded every cycle, a continuously written table can be
starved, the lease is never released, and a deploy or crash of the planning
replica stops all maintenance for up to 15 minutes. Adding replicas adds no
planning capacity.

After this change every coordinator replica plans concurrently, planning
capacity grows with replicas, backlog drains continuously instead of per tick,
no lease outlives the work it protects, and failover takes seconds.

## Evidence

Independent review of the current implementation (2026-10-03), verified
against source:

- One planner per fleet: `vala.forge_scheduler_state` singleton,
  `vala-sql` `forge_tasks.rs` `acquire_scheduler`/`renew_scheduler`; no
  release; `lease_ttl` 15 min (`forge/compact.rs`).
- Timer-paced: passes only on the `maintenance_interval` tick (60 s); a pass
  with remaining demand does not re-run; page `max_hints_per_wake` = 256;
  demands planned serially (`forge/scheduler.rs`, `forge/planning_scheduler.rs`).
- Unbounded roster re-seed of every registered table each cycle
  (`planning_scheduler.rs` `repair_roster`).
- Hot-table starvation: page order `last_requested_at`, reset by every hint; a
  generation change during planning rolls back the enqueued tasks and the
  demand is skipped for the cycle.
- Worker claims serialize fleet-wide on the single
  `vala.forge_worker_claim_state` row (`forge_fair_claim.sql`).
- Worker task leases also use the 15-minute TTL, so a crashed worker's task is
  stranded for up to 15 minutes.
- RisingWave reference (local copy): one meta-leader selector, but selection is
  pull-driven by compactor capacity (no timer cap), leader lease 30 s with
  resign on stop, task state dropped on report.

## Scope

- Forge planning authority, demand claiming, pacing, fairness, readiness,
  shutdown, and fleet-wide periodic work in `vala-bifrost-redux` `forge` and
  the `vala-sql` Forge queries and migrations they own.
- Forge worker claim fairness and worker task lease duration.
- Forge telemetry whose meaning changes (pass, debt, inventory gauges).
- `architecture/bifrost-design.md` §Maintenance: Forge and §Explicit non-goals
  wording.

## Non-goals

- A second compaction grouping, selection, or rewrite algorithm.
- Splitting one compaction plan across pods, or changing table file geometry.
- Changing the per-table execution invariants (one active attempt per table,
  base-snapshot validation, catalog compare-and-swap, cleanup handoff).
- Moving planning state into process memory or a leader-only cache.
- Compatibility with the singleton scheduler table or its fence; there is no
  migration path for in-flight singleton state beyond dropping it.

## Required behavior

### REQ-001 — Every coordinator replica plans

Every replica running the Forge coordinator role plans concurrently. There is
no fleet-wide planning lease, leader, or standby state. One planning algorithm
runs on every replica; multiple planning replicas are not "a second planner".

### REQ-002 — Per-table demand claims

Planning work is the durable per-table demand row. A planner claims a bounded
batch of eligible demand rows in one short committed statement
(`FOR UPDATE SKIP LOCKED`, no transaction held across catalog reads or
planning). A claim records the claiming owner, a unique claim token, and an
expiry derived in SQL from `statement_timestamp()` plus a caller-bound
duration. Concurrent planners never claim the same row while a claim is live.
An expired claim is claimable by any planner.

### REQ-003 — Release on completion, error, and shutdown

A claim lives only while its demand is being planned:

- Successful planning deletes the demand row, or — when a newer generation
  arrived during planning — clears the claim and keeps the row eligible, in the
  same transaction that enqueues the planned tasks.
- A planning error clears the claim; the demand stays eligible.
- Graceful shutdown clears every claim the replica holds before it exits.
- Only a crash leaves a claim to expire.

No Forge lease, claim, or fence row is held after the work it protects has
completed, failed, or been abandoned by graceful shutdown.

### REQ-004 — Claim-token fencing

Acknowledgement (task enqueue plus demand delete/release) succeeds only while
the caller's claim token still owns the row. A planner whose claim expired and
was taken over enqueues nothing. Duplicate planning after expiry is harmless:
execution invariants and idempotent task identity reject duplicates.

### REQ-005 — No starvation of written tables

Eligibility order is the age of the oldest unplanned request for the table
(`first_requested_at`), which hints never reset. A newer generation arriving
during planning never discards the tasks just planned; it only keeps the
demand eligible for another plan. A continuously written table is planned at
least once per claim cycle in which it is eligible.

### REQ-006 — Continuous, demand-driven pacing

A planner claims its next batch immediately while the previous batch was full.
Otherwise it waits for the earliest of: a local demand hint, the periodic tick,
or shutdown. The periodic tick exists only for time-based maintenance and
claim expiry, not to pace backlog. Each replica bounds its concurrent
per-table planning (catalog reads) by configuration.

### REQ-007 — Tenant-fair claiming without a global cursor

Planning claims and worker task claims are tenant-fair by per-tenant ranking
within the claim statement (each tenant's oldest work first, a per-tenant cap
per batch). Neither depends on a fleet-wide cursor row, so claims from
different replicas do not serialize on one row. `vala.forge_worker_claim_state`
is removed.

### REQ-008 — Due-based periodic maintenance

Time-based maintenance (periodic compaction review, snapshot expiry, orphan
cleanup) is scheduled per table from durable `next_due_at`-style state and
claimed through the same demand claims. No pass re-seeds every registered
table. Newly registered tables become due on registration discovery.
Fleet-wide singleton steps, if any remain, run as one conditional single-row
update (`... WHERE last_run_at < statement_timestamp() - interval RETURNING`)
that holds nothing after it commits.

### REQ-009 — Fleet debt and inventory from SQL

Compaction debt and inventory are persisted per table by the planner that
planned it and published as fleet-wide aggregates read from Postgres. Every
coordinator exports the same fleet value; dashboards aggregate with `max`, not
`sum`. No figure depends on one replica having traversed every table.

### REQ-010 — Short renewed worker leases

Worker task claims use a short TTL renewed by the existing heartbeat at no more
than one third of the TTL, so a crashed worker's task becomes claimable within
one TTL. Default planning-claim TTL and worker-lease TTL are 60 s, configurable
by deployment.

### REQ-011 — Readiness

Forge coordinator readiness reflects only whether the replica's planning and
worker loops are running and its last coordination-store interaction
succeeded. Remaining backlog, a full batch, or another replica's activity never
makes a replica unready.

### REQ-012 — Telemetry

Planning telemetry is per claim batch: claimed, planned, released, expired
takeovers, acknowledgement token rejections, batch duration, and the age of
the oldest eligible demand (fleet-wide, from SQL). Standby and pass-incomplete
signals are removed.

## Invariants

- INV-001: At most one live claim per demand row.
- INV-002: At most one active durable task attempt per tenant-qualified table
  (unchanged).
- INV-003: Planning state is durable in Postgres; no correctness depends on
  in-process state surviving.
- INV-004: All coordination deadlines derive from `statement_timestamp()`.
- INV-005: Tenant isolation: claims, demand, and tasks stay tenant-qualified;
  no claim statement returns another tenant's rows into a tenant-scoped path.

## Expensive-to-reverse decisions

- Removal of the singleton scheduler (`vala.forge_scheduler_state`) and
  `vala.forge_worker_claim_state`.
- New persisted columns on `vala.forge_planning_demands` (claim owner, token,
  expiry, `first_requested_at`) and durable per-table due/debt state.
- Multiple concurrent planners as the architecture; `bifrost-design.md` states
  explicitly that multiple planning replicas over one algorithm are permitted
  and that no fleet-wide Forge planning lease exists.
- Default 60 s planning-claim and worker-lease TTLs.

## Acceptance criteria

- AC-001 (REQ-001, REQ-002, INV-001): Two coordinator replicas planning one
  backlog claim disjoint demand rows and together enqueue exactly the tasks one
  planner would; no duplicate active attempts. Integration test on real
  Postgres.
- AC-002 (REQ-003): After planning completes, fails, or the replica shuts down
  gracefully, the replica holds no demand claim, task claim, or Forge lease
  row. Integration test.
- AC-003 (REQ-003, REQ-004, REQ-010): A replica killed mid-plan and mid-task
  leaves claims that another replica takes over within one TTL; the dead
  replica's late acknowledgement enqueues nothing. Integration test.
- AC-004 (REQ-005): A table receiving a hint on every planning attempt still
  gets its tasks enqueued every cycle it is eligible. Integration test.
- AC-005 (REQ-006): A backlog larger than one batch drains back-to-back without
  waiting for the periodic tick. Integration test with a long tick.
- AC-006 (REQ-007): With one tenant holding a large backlog and another a
  single demand, the small tenant's demand is claimed in the first batch on
  either replica; worker claims show the same. Integration test.
- AC-007 (REQ-008): A cycle with N registered, not-due tables issues no
  per-table re-seed writes. Integration test.
- AC-008 (REQ-011): A replica with backlog remaining stays ready; a replica
  whose coordination store fails becomes unready. Integration test.
- AC-009 (scale): In `mise run bench:capacity` (verified-change-contract
  REQ-171), the Forge demand backlog meets that run's saturation SLO (drained
  within 60 s after load stops) in every judged step, and the two-replica
  scale-out step passes. No separate Forge benchmark exists.
- AC-010: User journey: two-replica `wyrd-server` deployment with Scribe
  ingest across several tenants; compaction tasks complete for every written
  table, and stopping either replica gracefully does not stall maintenance.
- AC-011: `bifrost-design.md`, Kubernetes docs, and generated docs describe
  concurrent planning; no singleton, standby, or planner-lease wording remains.

## Open material decisions

None for revision 1.

## Authority links

- `AGENTS.md` §2, §15 (PostgreSQL owns coordination timestamps)
- `architecture/bifrost-design.md` §Maintenance: Forge, §Explicit non-goals
- `architecture/wyrd-design.md`

## Revision history

- Revision 1 (2026-10-03, approved): Replace the singleton Forge planner with
  concurrent per-table demand claims after the capacity benchmark and an
  independent review showed timer-paced, single-replica planning, leases held
  past their work, and 15-minute failover.
