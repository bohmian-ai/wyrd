# Data-domain review — FAIL

Subject: cumulative `22335c2c40a5ad24c27bfa0d4686f4da49f922c5...3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4`; latest-fix base `7cda4e93159ab328de2a74068d5e1978a2590818`. HEAD independently confirmed exact candidate. Source remained immutable. This report is the only file written by this reviewer. No current-round sibling reports were read.

## Material findings

### DATA-1 — new pre-admission exception does not enforce its terminal no-input/no-snapshot shape

Classification: VIOLATION; Important; proposed new finding, distinct from the prior pending-consumption root cause.

Obligation: approved revision 18 REQ-004/spec lines 355–364 and architecture/wyrd-design.md:1308–1316 permit omitting authority and pinned inputs only for the truthful pre-admission terminal shape, which has no source inputs/snapshot. R1 explicitly requires narrowing the SQL constraints while retaining runnable invariants.

Locations: `crates/wyrd/wyrd-sql/migrations/20261018000000_verification_group_pending.sql:46–75`, especially `:62–73`.

Evidence: `verifier_executions_refused_shape` permits authority NULL for an errored, attempt-zero group without restricting its inputs. The separate input constraint's ordinary group arm still accepts object inputs/routes even with NULL authority. The group constraint equates false with “not a 32-byte digest” and “not an object”; these predicates do not require NULL. A fresh migrated schema's copied CHECK/NOT NULL constraints accepted each of these states:

1. A valid pre-admission errored group row updated to `inputs='{}'::jsonb, route_overrides='{}'::jsonb`, retaining NULL authority and NULL snapshot.
2. That row updated to `inputs=NULL, route_overrides=NULL, group_sources='[]'::jsonb, group_snapshot_sha256=decode('00','hex')`, retaining NULL authority and terminal status.

Both statements reported `UPDATE 1`. Thus an array-shaped snapshot and a one-byte digest are admitted precisely by the newly introduced exception. A `group_sources` array cannot decode as the map used by `StatusRow`/`GroupSnapshot`, so this persisted state is incompatible with ordinary status readback, in addition to misrepresenting a pre-admission failure.

Reachability/impact boundary: `VerifierExecutionQueue::refuse_group` is the only production refusal writer (`verifier_executions.rs:1832`, invoked by scheduler.rs:480); its current INSERT emits correct NULLs. No public request exploit or currently emitted malformed row is claimed. This finding concerns the explicitly required durable constraint boundary weakened by the new exception. The test's synthetic row bypasses foreign keys only by using a temporary CHECK clone; the relevant checked values and constraints are the actual migrated definitions, not a hand-retyped predicate.

Smallest correction: make the NULL-authority exception require all input/window/record/route/snapshot columns NULL, and express ordinary JSON object requirements as total booleans (`IS NOT DISTINCT FROM 'object'`, or explicit non-NULL checks). Ensure refused groups cannot fall through the ordinary input arms. Do not add another writer, lifecycle state or public contract. Extend existing P2 with rejected updates for object inputs on refused rows and non-NULL malformed snapshot columns; retain valid refusal insert/readback and runnable NULL-authority rejection.

Related three-valued-logic observation, not a separate new finding: the same current input CHECK accepts runnable pending group rows with NULL inputs/routes, because `jsonb_typeof(NULL)='object'` is UNKNOWN and PostgreSQL CHECK accepts UNKNOWN. A migrated CHECK clone demonstrated this with a non-NULL object authority and valid 32-byte/object snapshot. The original `20261016000000` input constraint already had this expression, and all enumerated production runnable writers provide their proper input shapes. This is not presented as a newly reachable public regression. Totalizing the materially rewritten input predicate addresses this without unrelated refactoring.

### DATA-2 / FIND-TASK-003a-2 residual — day durations are calendar days in PostgreSQL, not fixed elapsed days

Classification: INCORRECT; Important; preserve stable FIND-TASK-003a-2 for incomplete accepted-duration exactness.

Locations: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:202` (`statement_timestamp() + (b.group_end ->> 'after')::interval`); contrasting contract `crates/wyrd-spec/src/card/trigger.rs:82–119` scales `d` to 86,400 seconds.

Obligation: revision 18 REQ-004/spec:305–312 and architecture/wyrd-design.md:1283–1286 require exact accepted durations, including equivalent fixed-unit representations. The shared parser now enforces the year bound correctly, but SQL reinterprets authored day units using the session timezone.

Evidence: production pool construction preserves DSN/default PostgreSQL session settings (`wyrd-sql/src/pool.rs:194–219`); TenantConn binds only tenant identity (`tenant_conn.rs:23,104–107`). Neither establishes UTC. In the authorized managed-PG session, the exact SQL operation on `t='2026-03-07 12:00:00-05'::timestamptz` produced:

| Session TimeZone | elapsed seconds for `t + '1d'::interval` | elapsed seconds for `t + '24h'::interval` |
|---|---:|---:|
| UTC | 86400 | 86400 |
| America/New_York | 82800 | 86400 |

This is a valid self-hosted PostgreSQL configuration and an accepted Trigger duration. Across spring DST, `after: 1d` schedules an hour earlier than the equivalent `after: 24h`; across fall DST the corresponding calendar-day behavior delays it. The counterexample needs no wall-clock wait. The supplied P2 checks one year against one hour under the current database date/default timezone, so it does not exclude this error. In the executed fixed-date sample `365d` and `8760h` both produced 31,536,000 seconds; this report does not claim otherwise for that sample.

Smallest correction: in this existing SQL owner, add an elapsed-seconds interval derived from the accepted duration instead of a calendar-day interval. Preserve the authored wire/storage text; no option, global timezone policy or duration abstraction is needed. For example, converting the parsed interval's epoch to seconds and multiplying `INTERVAL '1 second'` avoids day-bearing arithmetic. Extend P2 with fixed-date `1d` versus `24h` arithmetic under a DST-observing session timezone, or equivalent direct proof of the exact production expression; retain current year-bound and due/debounce assertions.

## Boundary and source coverage

| Boundary/obligation | Source and caller evidence | Assessment |
|---|---|---|
| ACK-only pending producer, no new request SQL | ScribeWrite::group_activity reads named owner/subject/Verifier/group fields from inputs or summary result; ScribeSink::write submits all frames, then schedule_groups, then reserved submission staging (`scribe_outbox.rs:275–295,766–802,1026–1061`). schedule_groups excludes terminally lost items and commits its one tenant transaction. | PASS. Failed SQL replays ordinary outbox slices before consuming submission reservations. Accepted ACK/visibility/crash-loss limitations are not findings. |
| Exact match and tenant isolation | RECORD_GROUP_ACTIVITY_SQL matches frozen owner, subject and source Verifier; aligned arrays and SELECT DISTINCT avoid a duplicate target in one upsert. New table has tenant/binding/group PK, binding tenant FK/cascade, forced RLS and operator policy. Tenant queue operations use TenantConn; only due-tenant discovery/depth uses OperatorPool. | PASS, except exact elapsed-day arithmetic above. No hand tenant filter or raw-pool domain API introduced. |
| Owner identity and inactivity | BINDING_ACTIVITY_SQL retains actual stored `p.id` regardless of account status; active gate checks principal, owner Card and DB-derived inactivity. claim_due_group propagates identity separately from active. | PASS. Missing attribution remains pending; inactive/deprecated owner can produce truthful attributed terminal error. Shared cron caller still sees same admission gate. |
| Refusal classification and transaction | scheduler hand_off distinguishes rejected authority/read denial from unavailable infrastructure/read/enqueue errors. hand_off_groups claims row and runs refuse_group/enqueue, then consume_group, and commits once; early None/error/cancellation drops transaction. | PASS for production writes and atomicity. New schema exception fails its narrower shape invariant as DATA-1. |
| Racing arrival | Claim uses FOR UPDATE SKIP LOCKED on pending row; consume runs before commit with lock retained. Concurrent INSERT ON CONFLICT waits, then creates a fresh pending row after deletion or updates retained row after rollback. No later delete occurs after release/evaluation. | PASS. Actual ACK row-lock observation is now in C13: source turn/flush races held claim, pg_stat_activity observes waiting pending INSERT, and fresh pending is read after handoff. |
| Runnable writers and claims | Enumerated INSERT_SUBMISSION_SQL/accept_submission, INSERT_SCHEDULED_SQL/enqueue_scheduled/apply_due, INSERT_GROUP_SQL/enqueue_group, INSERT_GROUP_ERROR_SQL/refuse_group. Only scheduler owns production consume/refuse; GroupPoller owns production enqueue_group; ScribeSink owns production record_group_activity. claim selects only pending/retrying/expired-running, so truthful errored refusal never runs. | PASS for emitted states. Ordinary manual/schedule inputs and actual accepted authority remain provided. CHECK limitation is explicitly qualified above. |
| Snapshot digest and retries | PinnedGroup::new hashes JCS binding/group, ordered selected execution/result IDs and pinned declarations; unique index covers tenant/binding/group/digest. INSERT conflict resolves that exact digest. Claim reconstructs stored GroupSnapshot/inputs. | PASS. No outstanding gate, coverage inventory or adoption machinery. |
| Settlement and ordinary siblings | complete takes existing binding lock before COMPLETE_SQL; DB statement settlement timestamp, predecessor filter on same binding/group + non-NULL verdict; replay flag prevents recomparison. Dispatch uniqueness preserved. Manual/schedule failure handling remains normal; terminal refused group has no verdict and cannot become predecessor. | PASS. Distinct snapshots overlapping and settlement-order reactions are approved. |
| Public readback | binding_status uses existing newest execution ID; execution_status decodes nullable group sources without needing accepted authority; VerificationService get_binding/get_execution requires normal cards:read before tenant SQL. No result pointer means direct errored-state return, no analytical result fetch. | PASS for proper refusal rows. New malformed snapshot state would fail decoding, as noted. |
| Bounds/capacity/cancellation | group handoff acquires shared capacity before TenantConn/claim, max 32 per tenant, bounded separate cron/group loops; group poller uses query deadline and ordinary authorization. | PASS within accepted backlog/fairness limits. |

## Prior stable findings and cleanup closure within this domain

- FIND-TASK-003a-1: producer/consumer correction is implemented. Retryable absence and query failure leave pending; owner/activity/auth refusal writes one errored group execution and consumes atomically; record failure rolls back. C13 now injects actual refusal INSERT failure and proves pending/no partial error, then success/no repeat. New data-shape issue is tracked separately as DATA-1 rather than reviving the old consume-on-failure diagnosis.
- FIND-TASK-003a-2: write-time maximum and SQL clamp removal are implemented, but exact accepted durations remain incomplete as DATA-2 above. The year P2 assertion is useful and does not prove DST-sensitive exactness.
- FIND-TASK-003a-4: SQL/concurrency part closed from actual C13 bodies: fault persistence observes no-read capacity/idle cases; a real ACK blocks behind held pending claim; actual scheduler run admits cron after stall, then cancellation preserves group. A comment at C13:847 acknowledges the daily-cursor proof can fail during the first 30 seconds of a UTC day; root was notified for cross-review consideration, not counted again here.
- HUMAN-CLEANUP 1: unused DueGroup fields/projections removed; pending due column/predicates/order/indexes, exact source tuple matching, enqueue resolution/readiness and DueBinding cron state retained.
- Other prior FIND-3/5/6 and HUMAN-CLEANUP 2/3 were not independently closed by this data-domain role; their dedicated runtime/standards reviewers own complete proof. No negative conclusion is inferred from that scoped delegation.

## Verification

Read AGENTS/user-supplied rules, agent-rules, task-review domain role, spec-driven-development, maintainer-style, applicable doctrine/design/Bifrost/security authority, approved revision 18, original task including original-base locked SQL requirements, prior verdict/findings/R1 and appended verification evidence. CodeGraph was used before source navigation. Source inventory/caller searches expanded the provided map, including manual/cron/group/refusal writers, shared binding activity, outbox producer and status consumers.

Root authorized one isolated sequential managed-PG session after its own P1/P2/P7 reruns completed. Exact command prefix:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1' <<'SQL'
```

The SQL transaction used `CREATE TEMP TABLE review_execution_shapes (LIKE wyrd.verifier_executions INCLUDING DEFAULTS INCLUDING CONSTRAINTS)`, inserted one otherwise valid attempt-zero errored group with UUIDv7-shaped identities, binding/owner/Trigger attribution, structured error, settlement time and NULL authority/inputs/snapshot, then performed the exact updates shown above. It selected the resulting proof columns after each update. The fixed-date portion was:

```sql
SET LOCAL TIME ZONE 'UTC';
SELECT current_setting('TimeZone'),
       extract(epoch FROM (t + '1d'::interval)-t),
       extract(epoch FROM (t + '24h'::interval)-t)
FROM (VALUES ('2026-03-07 12:00:00-05'::timestamptz)) v(t);
SET LOCAL TIME ZONE 'America/New_York';
-- Repeat the identical SELECT, then 365d versus 8760h for the same t.
ROLLBACK;
```

Session exit 0. Setup selected and passed exactly one Wyrd migration/idempotence test (103 skipped) and exactly one Vala migration/idempotence test (58 skipped). Three malformed-state updates each affected one row. Fixed-date UTC/New_York results are recorded above. Transaction rolled back and managed environment shut down. No source/test files, live repository data, production databases, stress loads or capacity tests were changed/run. No providers, full builds, broad gates or benchmarks were run by this reviewer. Existing candidate P1/P2/P3/static evidence was inspected as evidence, not represented as this reviewer's reruns. The temporary clone proves current CHECK/NOT NULL behavior; it intentionally does not prove FK/RLS execution or public reachability of malformed writes.

Overall: **FAIL** for the two bounded data-domain obligations above. Accepted crash-loss, visibility, retention, durable-backlog, settlement-order and fairness limits remain accepted.
