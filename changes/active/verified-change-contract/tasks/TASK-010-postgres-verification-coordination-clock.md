---
id: TASK-010
kind: remediation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 35
requirements: [REQ-078, REQ-079, REQ-081, REQ-098, REQ-112, REQ-115, REQ-146, REQ-152, INV-010, INV-015, AC-019, AC-020, AC-030, AC-033]
depends_on: [TASK-003, TASK-004]
parent_task: TASK-004
remediates: [FIND-CLOCK-001, FIND-CLOCK-002, FIND-CLOCK-003, FIND-CLOCK-004]
---

## Outcome and Value

Verification activity, schedules, run and dispatch availability, claims,
leases, retries, and worker deadlines use PostgreSQL as their single
coordination clock, so host/database skew cannot stall work or permit a second
pod to reclaim work while the first still believes its lease is live.

## Problem and Evidence

The implemented TASK-003 and TASK-004 paths use a process wall clock for
durable coordination that PostgreSQL later evaluates. A host clock ahead of
PostgreSQL can write work in the database's future and stall it; a host clock
behind PostgreSQL can write a lease that expires earlier than the worker
believes, allowing another pod to reclaim and execute the same work. Token
fencing prevents the stale holder from settling after a replacement claim, but
it cannot undo duplicate external execution.

- **FIND-CLOCK-001 — machine activity crosses clocks.**
  `crates/wyrd/wyrd-auth/src/issuance.rs` creates `issued_at` with
  `Utc::now()` and passes it to `record_machine_authentication`.
  `RECORD_AUTHENTICATION_SQL` in
  `crates/wyrd/wyrd-sql/src/queries/verification.rs` stores that bound value as
  `last_authenticated_at`; `BINDING_ACTIVITY_SQL` later compares the column to
  another Rust-derived bound cutoff. Activity can therefore expire early or
  late depending on which pod performs the write and read. Required outcome:
  the shared write stamps PostgreSQL statement time and the activity predicate
  derives its cutoff from PostgreSQL statement time; JWT `iat`/`exp` remain
  issuer-produced event facts.
- **FIND-CLOCK-002 — schedule eligibility crosses clocks.**
  `DUE_BINDING_SQL` and `DUE_TENANTS_SQL` in
  `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs` compare `next_run_at` to
  a bound process time supplied by the verification scheduler, while first
  arming and later cursor advancement calculate the next boundary from that
  process time. Different pods can see the same occurrence as due at different
  moments or skip the wrong outage window. Required outcome: due predicates
  use PostgreSQL statement time, and synchronous cron calculation uses a
  database-returned anchor before storing the deliberate future boundary.
- **FIND-CLOCK-003 — run and dispatch coordination crosses clocks.**
  `INSERT_RUN_SQL`, `EXHAUST_EXPIRED_SQL`, `CLAIM_RUN_SQL`,
  `RETRY_RUN_SQL`, `RELEASE_RUN_SQL`, `RUNNABLE_TENANTS_SQL`, and dispatch
  creation in `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs` write or
  compare Rust-supplied `now` values. `VerifierRunner` supplies one
  `VerificationClock` value for discovery and claims and computes lease and
  retry deadlines in Rust. This can stall pending/retrying work or let a
  second pod reclaim a still-executing attempt. Required outcome: immediate
  timestamps and predicates use `statement_timestamp()`, and lease/backoff
  deadlines are PostgreSQL statement time plus bound durations.
- **FIND-CLOCK-004 — the test clock masks the production split and invites
  reuse.** `crates/wyrd/wyrd-server/src/verification/clock.rs` intentionally
  drives scheduling, claims, lease expiry, retry backoff, settlement, and
  result event facts through one injectable wall clock. The runtime tests in
  `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs` advance that
  process clock to make PostgreSQL predicates pass, so they cannot catch
  host/database skew. TASK-005's baseline claims and TASK-007's Operator
  claims would otherwise reuse the same defective pattern. Required outcome:
  remove the coordination clock, retain `Instant` for process-local timing and
  producer time for result facts, and make time-specific tests arrange the
  PostgreSQL state whose predicate they exercise.

`crates/vala/vala-sql/src/queries/oracle_reader_authority.rs` is the existing
repository precedent: it writes relative leases with `statement_timestamp()`,
evaluates expiry in SQL, and returns database time beside a deadline when a
caller needs a remaining interval.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-sql` owns every durable timestamp assignment and predicate for
verification coordination. `wyrd-auth` calls the shared activity write without
supplying its JWT clock. `wyrd-server` owns process-local polling, timeout,
latency, and producer event facts while consuming PostgreSQL decisions.
TASK-005, TASK-006, TASK-007, and TASK-008 consume the corrected paths.

Use the existing Oracle SQL-clock pattern. Do not add a clock abstraction,
clock synchronization, skew tolerance, safety margin, permanent grep check,
sleep-based proof, or a second queue path. Do not change wire contracts,
schedule semantics, inactivity duration, attempt budgets, retry durations,
lease durations, result event-time semantics, or JWT `iat`/`exp` ownership.
Tests may deliberately place a row in the past or future only when the test is
about time.

## Approach

1. Move the shared machine-activity write and activity predicate to PostgreSQL
   time, returning the database anchor needed for an initial cron boundary.
2. Move immediate run/dispatch timestamps, due discovery, lease construction,
   expiry, retries, releases, and settlements into their owning SQL statements;
   bind durations or explicit future schedule instants, not Rust wall-clock
   coordination instants.
3. Keep cron calculation synchronous using database-returned anchors, Rust
   `Instant` for process-local timing, and producer wall time only for event
   facts.
4. Remove the verification coordination clock and update all callers and test
   support to consume database verdicts or returned intervals.
5. Leave one behavioral Postgres regression test on each corrected shared
   write path and rerun the existing restart, stale-fence, scheduling, and
   authentication journeys.

## Ordered Implementation Scenarios

### Scenario 1 — Authentication and activity share PostgreSQL time

**Behavior.** A qualifying machine exchange records activity at PostgreSQL's
statement time. Activity reads compare that stored value with a PostgreSQL
statement-time cutoff, and the same exchange returns the database anchor from
which Rust calculates an unarmed schedule's deliberate next future boundary.
Renewal never moves activity backward or resets an armed cursor; excluded grant
types remain no-ops.

**RED.** Add
`database_clock_owns_machine_activity_and_schedule_arming` to the existing
Postgres binding suite and run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_verification_bindings -E 'test(=database_clock_owns_machine_activity_and_schedule_arming)'"
```

The case must fail while the shared write accepts a Rust wall-clock instant or
the activity predicate accepts a Rust-derived cutoff.

**GREEN.** Make the shared SQL path own the stamp and cutoff, return only the
database anchor needed for cron calculation, and update auth and status callers
without changing which exchanges qualify.

**REFACTOR.** Remove obsolete wall-clock parameters and duplicate cutoff
arithmetic while retaining the existing synchronous cron parser.

### Scenario 2 — Queue availability, claims, leases, and retries share PostgreSQL time

**Behavior.** Immediate run and dispatch work becomes available at PostgreSQL
statement time. Cross-tenant discovery and tenant claims evaluate availability
and expiry with PostgreSQL statement time; lease and retry deadlines are SQL
time plus bound durations; settlement and release timestamps come from their
own statements. Rust receives stored deadlines, verdicts, or remaining
intervals only when needed and never compares them with its wall clock.

**RED.** Add `database_clock_owns_verifier_queue_deadlines` to the existing
Postgres run suite and run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs -E 'test(=database_clock_owns_verifier_queue_deadlines)'"
```

The case must fail while enqueue, due discovery, claim, lease, retry, release,
settlement, or dispatch creation binds a Rust wall-clock coordination instant.

**GREEN.** Apply the repository's existing SQL-owned lease pattern to the
shared queue transitions and bind only durations, identities, errors, results,
and deliberate future schedule instants.

**REFACTOR.** Collapse redundant timestamp parameters and return values; keep
one queue owner and the existing token fence.

### Scenario 3 — Runtime tests exercise the database clock

**Behavior.** Verification runtime coordination has no injectable wall clock.
Scheduler polling and execution timeouts remain process-local, result
start/end/event timestamps remain producer facts, and lease-reclaim/retry tests
drive the database state they are specifically testing. Restart and stale-token
fencing remain unchanged.

**RED.** Remove the manual coordination-clock dependency, then run the existing
focused reclaim cases:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_verification_runtime -E 'test(=expired_lease_is_reclaimed_and_the_stale_holder_is_fenced)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_verification_runtime -E 'test(=crashed_runner_restarts_and_reclaims_without_duplicates)'"
```

They must initially fail because they advance process time instead of arranging
the PostgreSQL expiry decision under test.

**GREEN.** Drive expired/due database state only in tests whose subject is
time, retain deterministic synchronization for worker progress, and keep
event-fact assertions independent of exact wall-clock values.

**REFACTOR.** Delete the unused verification clock and its builder/test-support
surface; keep `Instant` where elapsed process time is the actual subject.

## Acceptance Criteria

- Every verification timestamp has one classified owner: PostgreSQL
  coordination, Rust in-process timing, or producer event fact.
- Machine activity and its inactivity predicate use PostgreSQL statement time;
  schedule arming uses the returned database anchor and later due predicates
  use PostgreSQL statement time.
- Run and dispatch enqueue, discovery, claim, expiry, retry, settlement,
  release, and deadline decisions contain no Rust wall-clock coordination
  instant.
- Relative deadlines are derived in SQL from durations, and expiry/eligibility
  predicates use `statement_timestamp()`, not transaction `now()`.
- The verification coordination clock abstraction is removed; JWT and result
  event facts retain producer ownership, and process timeouts retain `Instant`.
- One behavioral Postgres regression covers each corrected shared write path;
  no test uses skew tolerance or backdating to make a non-time predicate pass.
- Existing activity exclusions, schedule no-backfill, lease fencing, retry
  budgets, restart recovery, result publication, tenant isolation, and public
  status behavior remain green.

## Expected Write Set and Consumer Closure

Likely owners and consumers include verification and verifier-run queries in
`wyrd-sql`, token issuance in `wyrd-auth`, the verification scheduler/runner
and runtime composition in `wyrd-server`, existing Postgres integration tests,
and `wyrd-testing` helpers that currently accept coordination timestamps. No
public schema, SDK, Bifrost table, or migration shape is expected to change.

## Verification and Evidence

Run every RED command above by exact name, then:

```bash
mise run test:sql
mise run test:principals:unit
mise run test:principals:integration
mise run test:cards:integration
mise run test:wyrd
mise run check:tenant-isolation
mise run fmt
mise run lints
git diff --check
```

Evidence must classify every touched timestamp by writer and evaluator and
confirm that both belong to the same decision domain.

## Material Stop Conditions

Stop for specification authority if correctness requires changing public wire
types, inactivity/schedule/retry/lease semantics, attempt or timeout bounds,
event-time ownership, or the accepted at-least-once delivery ceiling. Stop for
architecture authority if a required coordination path cannot use PostgreSQL
without introducing another durable clock or queue owner.

## Authority Links

- `changes/active/verified-change-contract/spec.md` revision 35
- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
- `architecture/references/domain/analytical-operations-reliability.md`

## Implementation Evidence

### Timestamp ownership

| Domain | Writer | Evaluator | Locations |
|---|---|---|---|
| PostgreSQL coordination | `statement_timestamp()` in SQL | `statement_timestamp()` in SQL | `wyrd-sql/src/queries/verification.rs` (`RECORD_AUTHENTICATION_SQL`, `BINDING_ACTIVITY_SQL`), `wyrd-sql/src/queries/verifier_runs.rs` (insert, due discovery, claim, lease, retry, release, terminate, settle, dispatch insert, runnable/due tenant lists) |
| Rust in-process timing | `std::time::Instant`, tokio timers | same process | `wyrd-server/src/verification/runner.rs` elapsed-time telemetry; `RuntimeLimits` poll/backoff/grace/timeouts |
| Producer event fact | `Utc::now()` in the producing process | downstream readers | JWT `iat`/`exp` in `wyrd-auth/src/issuance.rs`; result `started_at`/`event_time` in `wyrd-server/src/verification/runner.rs` |

Coordination writers and evaluators are both PostgreSQL, so no decision
compares instants from two clocks.

### Acceptance matrix

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Every verification timestamp has one classified owner | ownership table above | `mise run test:wyrd` | PASS |
| Machine activity, its inactivity predicate, schedule arming, and due predicates use PostgreSQL statement time | `wyrd-sql/src/queries/verification.rs:RECORD_AUTHENTICATION_SQL`, `BINDING_ACTIVITY_SQL`, `record_machine_authentication`; `InactivityTimeout::seconds` | `pg_verification_bindings::database_clock_owns_machine_activity_and_schedule_arming` | PASS |
| Enqueue, discovery, claim, expiry, retry, settlement, release, and deadline decisions carry no Rust wall-clock instant | `wyrd-sql/src/queries/verifier_runs.rs` — every public method lost its `now` parameter | `pg_verifier_runs::database_clock_owns_verifier_queue_deadlines` | PASS |
| Relative deadlines derived in SQL; predicates use `statement_timestamp()` | `CLAIM_RUN_SQL` / `RETRY_RUN_SQL` bind `$n::bigint * INTERVAL '1 millisecond'`; `BINDING_ACTIVITY_SQL` binds `$2::bigint * INTERVAL '1 second'` | `pg_verifier_runs::database_clock_owns_verifier_queue_deadlines` (stored backoff read as `next_attempt_at - updated_at`) | PASS |
| Clock abstraction removed; JWT and result event facts keep producer ownership; process timeouts keep `Instant` | `wyrd-server/src/verification/clock.rs` deleted; builder `clock` method and field removed from `verification/mod.rs`, `scheduler.rs`, `runner.rs` | `mise run lints`, `mise run test:wyrd` | PASS |
| One behavioral Postgres regression per corrected shared write path; no skew tolerance | `pg_verification_bindings.rs`, `pg_verifier_runs.rs`, `pg_verification_runtime.rs` arrange rows via `expire_deadlines` / `make_binding_due` / interval-bound backdating | `mise run test:sql`, `test:bifrost:integration:server` targets | PASS |
| Existing exclusions, no-backfill, fencing, budgets, restart recovery, publication, isolation, and status behavior stay green | unchanged semantics | `mise run test:sql`, `test:principals:unit`, `test:principals:integration`, `test:cards:integration`, `test:wyrd`, `pg_verification_runtime` (19/19) | PASS |

### Commands

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_REG_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_verification_bindings -E 'test(=database_clock_owns_machine_activity_and_schedule_arming)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_REG_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs -E 'test(=database_clock_owns_verifier_queue_deadlines)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime --test-threads=1"
mise run test:sql
mise run test:principals:unit
mise run test:principals:integration
mise run test:cards:integration
mise run test:wyrd
mise run check:tenant-isolation
mise run codegen:check
mise run fmt
mise run lints
git diff --check
```

All passed.

### Material notes

- `mise run test:cards:integration` also exercises the Rust SDK manual-run
  journey, which failed on a defect outside this task: `WyrdError::from_code`
  skipped every variant whose `details` field is spelled `Value` rather than
  `serde_json::Value`, so the client collapsed
  `WYRD_VERIFICATION_400_INVALID_WINDOW` onto a 502 `UpstreamFailure`. Fixed at
  the root in `wyrd-error-derive` (`types_equal` compares a path type by its
  final segment) with a regression case in that crate's suite.
- `RETRY_RUN_SQL` now returns `next_attempt_at`, so `RetryOutcome::Scheduled`
  reports the database's stored deadline and a fenced token still yields
  `StaleLease` from the empty result.
- `GREATEST(last_authenticated_at, statement_timestamp())` is retained so a
  database clock stepping backward cannot move recorded activity backward.
