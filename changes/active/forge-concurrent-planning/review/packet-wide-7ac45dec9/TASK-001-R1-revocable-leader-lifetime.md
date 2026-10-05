---
id: TASK-001-R1
kind: remediation
status: review
spec: SPEC-forge-concurrent-planning
spec_revision: 11
parent_task: TASK-001
remediates: [FIND-TASK-001-1]
---

# Revocable Forge leader lifetime

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`
- Original task: `changes/active/forge-concurrent-planning/tasks/TASK-001-leader-and-promotion.md`
- Review verdict: `review/packet-wide-7ac45dec9/verdict.md`
- Reviewed candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Base: `c1508b375`

## Diagnosis

REQ-001 and INV-001 require loss of the 30-second PostgreSQL term to stop
dispatch and leader-timer work. The only heartbeat loop awaits hinted
promotion and a sequential promotion-debt sweep that can block on catalog,
object-store, and SQL IO. PostgreSQL can elect a successor while the old
process retains `held: Some`; peer handlers accept that cached term and an
already-running maintenance pass retains a cloned term without revocation.
The observable result is concurrent leader behavior and stale maintenance even
though downstream task/table fences reduce corruption risk.

## Intended correction outcome

PostgreSQL renewal progresses independently of promotion work. Expiry,
renewal failure, replacement, or shutdown revokes the exact in-memory term;
all dispatch, promotion, peer, and maintenance consumers stop before their next
durable effect.

## Decision-complete recommendation

Keep `ForgeLeadership` as the owner. Separate its renewal lifecycle from the
promotion sweep and make the existing `ForgeHeldTerm` revocable. Clearing or
replacing `held` must revoke that same term, and existing consumers must check
the revocation at their effect boundaries. Reuse the current term token,
schedule owner, and cancellation machinery. Do not add a second lease,
scheduler, durable schedule, or host-clock authority.

## Preserved behavior and non-goals

- Preserve single-row PostgreSQL election, immediate graceful resignation,
  volatile scheduling state, durable promotion recovery, and local/peer route
  equivalence.
- Preserve per-table/task fencing and per-table failure isolation.
- Do not redesign promotion, worker pull, or maintenance membership.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-1` | Promotion IO cannot prevent renewal; a replaced term is revoked; notify/pull/report and maintenance perform no later effect under it; the successor resumes recovery without duplicate settlement. |

## Focused proof and broader verification

Park a real hinted/debt promotion beyond the lease while a standby contends;
prove the successor obtains a larger token and the old replica refuses all
leader handlers. Pause maintenance, revoke the term, and prove no later rewrite,
expiration, or cleanup effect occurs. Run the exact focused tests, the Forge
journey, the owning Bifrost verification lane, format, lints, and diff check.


## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Promotion IO cannot prevent renewal | `forge/scheduler.rs` `Forge::renew` is a third `tokio::join!` branch that awaits only the election row. `forge/leadership.rs` `ForgeLeadership::heartbeat` serializes renewal and bounds it by the term's local deadline (`timeout_at`, measured from before the last successful acquire/renew, so it never falls after PostgreSQL's expiry) | `revoked_term_stops_promotion_dispatch_and_maintenance`: a hinted promotion is parked at its catalog commit, the row lapses, the standby acquires token 2 > 1, and the parked replica's renewal loop logs `term lost; revoking it fencing_token=1` while the promotion is still held | PASS |
| A replaced term is revoked | `ForgeHeldTerm.revocation` is a child of the coordinator shutdown. `set_held` cancels the term it replaces; `held()`/`term()` filter revoked terms; `resign` still resigns a revoked slot | same journey: `await_revoked(old_term)` and `held(old_leader).is_none()` | PASS |
| notify/pull/report perform no later effect under it | `term()` returns `FenceLost` for a revoked term; these handlers only touch the in-memory schedule | same journey: notify, pull and report under the old token all return `FenceLost` | PASS |
| Maintenance performs no later effect under it | `run_maintenance` and the debt sweep run under `term.revocation()`. `rewrite_manifests` checks it under the lease before its commit; `execute_accepted` refuses (`Shutdown`) before recording a new attempt once it is cancelled | same journey: a maintenance pass is paused after the catalog accepts the expiry, the term is replaced and revoked, then the pass is released. Snapshots and maintenance rows stay equal to the accepted state, the orphan remains, and the trace shows `snapshot expiration failed error=Forge scheduler was shut down` | PASS |
| The successor resumes recovery without duplicate settlement | Unchanged durable recovery from attempt rows | same journey: the successor counts the parked promotion once (`pending_commits == 1`); the third term rejoins, sweeps the orphan, and leaves 0 unsettled `forge_tasks` rows | PASS |

Preserved behavior and non-goals: single-row election, immediate resignation on stop, the volatile schedule, durable promotion recovery, and per-table fencing are unchanged. No second lease, scheduler, durable schedule or host-clock authority was added. Promotion, worker pull and maintenance membership were not redesigned.

Commands:

- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all -E "test(=production_closeout::revoked_term_stops_promotion_dispatch_and_maintenance)"'`: PASS
- `mise run test:bifrost:journey:forge`: 22/22 PASS
- `mise run test:bifrost`: 8/9 lanes PASS. In `integration:server`, `pg_verification_runtime::crash_after_detail_ack_reclaims_the_same_run_before_dispatch` failed once; see the diagnosis below. A rerun of `test:bifrost:integration:server:inner` passed 84/84, and the exact test also passed when rerun on its own.
- `mise run fmt`, `mise run lints`, `git diff --check`: clean

Diagnosis of the one `test:bifrost` failure:

- **Symptom:** in `pg_verification_runtime::crash_after_detail_ack_reclaims_the_same_run_before_dispatch`, the in-process Forge worker got `pool timed out while waiting for an open connection` at 02:22:35. That stopped the supervised worker, and the test's Scribe flush then failed with `ingress dispatcher is closed`.
- **Evidence:** the whole process starved for Postgres at the same moment. The card reconciler timed out on the same pool, the readiness Postgres probe timed out at 1.5s, and the next acquire took 7.58s. The only leadership SQL in the window is the initial acquire at 02:22:29.35. The new renewal loop first ticks at +10s (02:22:39), after the failure.
- **Cause:** pool or Postgres starvation inside the verification-runtime harness, not leadership SQL from this change. It did not reproduce in two reruns. I could not identify the holder of the connections from the info-level trace.
- **Fix site:** outside this task's write set (the verification runtime harness and pool sizing). Reported to the caller as a risk.

### Follow-up: single renewal path and fence attribution

| Requirement | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One renewal path | `scheduler.rs` `Forge::renew` is the only caller of `ForgeLeadership::heartbeat`. The supervisor heartbeat only sweeps under the held term, and test-triggered passes renew in the renewal loop before the supervisor sweeps and records them | `revoked_term_stops_promotion_dispatch_and_maintenance` and `mise run test:bifrost:journey:forge` (22/22) | PASS |
| Debt sweep starts at acquisition | In production, the renewal loop's first tick fires at boot. Each acquisition sends `ForgeSweep::Acquired` to the supervisor over an unbounded channel, so renewal never waits and the sweep starts as soon as the supervisor is free | design and code inspection; the journey lane exercises the triggered path | PASS |
| A revoked term surfaces as `FenceLost` | `ForgeHeldTerm::attribute` maps `Shutdown`/`ShutdownRetained` to `FenceLost { forge:leader }` when the term was revoked and coordinator shutdown was not. It is applied to the maintenance and debt-sweep failure logs | unit test `forge::leadership::tests::revoked_stop_is_attributed_to_the_leader_fence`; the journey trace shows `Forge snapshot expiration failed error=Forge lost lease fence 'forge:leader'` | PASS |

Commands: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::leadership::tests::revoked_stop_is_attributed_to_the_leader_fence)'` PASS; the focused journey command above PASS; `mise run test:bifrost:journey:forge` 22/22 PASS; `mise run fmt`, `mise run lints` and `git diff --check` clean.

Pool-timeout follow-up (read-only diagnostician): a Postgres-side stall of about 7s starved the app, operator and Vala pools at once, including a bare readiness `SELECT 1`, so the cause is not lock contention and not this change. It is fatal only because `ForgeWorker::run_event_loop` (`worker.rs` ~2444/2451/2467/2554) propagates read-only SQL errors as `Err`, and `wyrd_server::app::supervise` then terminates the process. That fix site is outside this task's write set and overlaps TASK-002-R1's `worker.rs` edits, so it is escalated to the caller.

### Follow-up: Forge worker survives an unreachable database (caller-directed)

- **Symptom:** `pg_verification_runtime::crash_after_detail_ack_reclaims_the_same_run_before_dispatch` failed once in `test:bifrost` with `Forge worker stopped after it could not settle its work error=... pool timed out while waiting for an open connection`. `supervise` then terminated the in-process server, so the test's flush hit `ingress dispatcher is closed`.
- **Evidence:** about 7s of Postgres starvation hit the operator, app and Vala pools at once, including a readiness `SELECT 1`. `ForgeWorker::run_event_loop` propagated the read-only reclaim, recovery-claim, unattended-authority and fair-claim errors with `?`, and `run()` returned them to the fatal supervisor.
- **Cause:** a statement the database never answered was treated like an unsettleable durable failure.
- **Fix site:** `forge/worker.rs` `ForgeWorker::answered`. Those four loop reads route through it. `ForgeError::is_database_unavailable` (`PoolTimedOut` or I/O on a `Sql` error, a `TransientCoordination` case) retracts readiness and backs off through the existing `wait_for_progress`. Every other error, and every settlement, reconciliation and lease-release failure, still stops the worker. `reconcile_one_prepared` was split into `claim_prepared` and `reconcile_claimed_prepared`, so only its claim is retried. Startup drain is unchanged.

| Requirement | Verification | Result |
|---|---|---|
| A transient pool failure on a loop read backs off and the loop continues | new integration test `forge::production_routes::worker_backs_off_while_the_operator_database_is_unreachable`. Against the previous `worker.rs` it fails (`finished=true`, `could not settle its work ... pool timed out`); with the fix it passes | PASS |
| Only an unanswered statement is classified as unavailable | unit test `forge::error::tests::database_unavailable_is_only_an_unanswered_statement` | PASS |
| No regression | `mise run test:bifrost:integration:server` 84/84; `mise run test:bifrost:integration:redux` 893/893; `mise run fmt`, `mise run lints` and `git diff --check` clean | PASS |
