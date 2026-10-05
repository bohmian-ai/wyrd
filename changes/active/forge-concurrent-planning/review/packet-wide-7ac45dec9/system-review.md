# System Resilience Review

## Immutable subject

- Base: `c1508b375`
- Candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Range: `c1508b375..7ac45dec9`
- Authority: approved spec revision 11 and packet tasks.
- Candidate remained at the named commit throughout review.

## Deployed-path coverage

| Path | Failure/recovery evidence | Assessment |
|---|---|---|
| Forge election and peer routing | `forge/leadership.rs:158-224,235-446`; `vala-sql/.../forge_leader.rs:56-173`; `wyrd-server/.../forge_peer.rs:85-155` | SQL fencing is correct at acquire/renew/current, graceful resign clears local state first, and remote routing reads only live SQL terms. The local held term is not self-expiring, however, and renewal shares a sequential supervisor with promotion recovery (SYS-001). |
| Promotion recovery | `forge/scheduler.rs:293-389`; `forge/scribe_promotion.rs:657-688,710-746,1131-1218`; `forge_tasks.rs:124-157` | Durable `file_list` debt recovers lost hints; promotion publication and SQL settlement are fenced/idempotent. Per-table failures remain retryable. The recovery sweep can block leader renewal (SYS-001). |
| Worker pull/result/timeout | `forge/leader.rs:546-648`; `forge/worker.rs:2522-2843`; `forge_tasks.rs:159-237,620-678,854-920` | Pull is capacity-owned, at most four; failed admission reports NotStarted; failures/timeouts preserve pending commits; stale reports are ignored. Durable task/lease fencing prevents an expired dispatch from silently reusing ownership. Lost reports recover via the leader deadline. |
| Maintenance and deletion | `forge/gc.rs:141-192`; `forge_operations.rs:850-959,1552-1615`; `forge_tasks.rs:1154-1230`; `forge/orphan_gc.rs:1123-1158,1430-1465`; `worker.rs:7752-7990` | Snapshot expiration and expired cleanup serialize active-read refusal through the table authority. Cleanup prepares/revalidates before external delete and retains durable replay evidence. Per-table errors do not stop later tables. A stale in-memory leader can continue the timer after SQL lease expiry (SYS-001), although downstream table/task fences reduce corruption risk. |
| Oracle active cut and terminal/drop release | `oracle/planner.rs:80-291`; `oracle/mod.rs:1119-1139,1866-1935`; `oracle/query_stream.rs:401-504,551-658`; SQL migration `20260910000025...sql:58-326`; `oracle_reader_authority.rs:203-337` | One tenant transaction commits claims before object IO. Normal terminal joins distributed/local descendants before awaited release. Drop release is non-blocking and intentionally falls back to deadline expiry on failure/no runtime. Reacquisition extends abandonment beyond the original absolute deadline (SYS-002). |
| Process blast radius | `wyrd-server/src/boot/mod.rs:908-1013,2137-2165`; `wyrd-server/src/app/server.rs:622-640,744-782` | Forge coordinator/worker are supervised server tasks; unexpected task exit triggers shared server shutdown rather than being hidden. Ordinary per-pass errors are logged and retried. A hung promotion sweep does not exit, so the server stays up but can advertise stale Forge readiness and leadership (SYS-001); unrelated HTTP/Oracle/Scribe serving remains process-live until another supervisor exits. |

## Review Findings

### Critical

- None.

### Important

- **SYS-001 — INCORRECT — A slow promotion-debt sweep can let the SQL leader lease expire while the old replica continues dispatch and maintenance.**
  - **Violated obligation:** REQ-001/INV-001 require one active scheduling leader and loss of leadership to stop dispatch and leader timer work; the 30-second election lease is independent of publication work.
  - **Location:** `crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:337-350,366-389`; `crates/vala/vala-bifrost-redux/src/forge/leadership.rs:158-205,235-344`; `crates/vala/vala-bifrost-redux/src/forge/gc.rs:152-190`; `crates/wyrd/wyrd-server/src/grpc/forge_peer.rs:85-155`.
  - **Evidence:** `lead()` renews once and then awaits `sweep_promotion_debt`; that sweep iterates every owed table sequentially and awaits full promotion/catalog/object-store work. During the await, no heartbeat runs. The SQL lease remains 30 seconds (`leadership.rs:38-42`, `forge_leader.rs:105-127`), but `held` remains `Some`. `term()` checks only the in-memory token, so notify/pull/report handlers continue accepting the expired term; `run_maintenance` snapshots the same held term once and can execute the complete timer pass. A standby can therefore acquire a successor term after PostgreSQL expiry while the old replica still acts as leader. Existing `one_leader_failover_volatile_state` proves graceful stop/resign, not lease expiry during blocked recovery; `promotion_notification_only_after_iceberg_commit` deliberately parks this sweep but releases it before the lease boundary.
  - **Observable consequence:** under slow/unavailable catalog or object storage, two replicas can concurrently believe they are scheduling leaders. Downstream per-table/task fences reduce data-corruption risk, but duplicate dispatch, stale maintenance work, lost reports against different volatile schedules, and false coordinator readiness remain reachable.
  - **Required correction:** keep lease renewal independent of promotion recovery and make a failed or bounded-time renewal clear/cancel the local held term before peer handlers or maintenance may continue using it. Promotion sweeping must observe term loss/cancellation. Preserve the in-memory hot path and PostgreSQL as the authoritative coordination clock.
  - **Focused closure proof:** park one real promotion sweep for longer than the leader TTL while a standby contends; prove the standby acquires a larger token, the expired replica refuses notify/pull/report and stops timer work before the parked IO resumes, then prove recovery continues on the successor without duplicate settlement.

- **SYS-002 — INCORRECT — The one permitted metadata reacquisition refreshes `abandon_after` with the original duration, extending protection beyond the query’s absolute deadline.**
  - **Violated obligation:** revision-11 REQ-014/INV-005/AC-009 require each row to expire at the query’s own deadline, with Oracle binding the remaining duration at acquisition and PostgreSQL deriving the timestamp.
  - **Location:** `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:1887-1920`; `crates/vala/vala-bifrost-redux/src/oracle/planner.rs:173-219`; `crates/vala/vala-sql/src/queries/oracle_reader_authority.rs:262-318`; migration `crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql:293-305`.
  - **Evidence:** `prepare_query_attempt` computes one `duration` from the absolute deadline and stores it in the Copy `ActiveReadOwner`. `acquire_and_materialize` reuses that same owner after a metadata `NotFound`. SQL’s conflict update resets `acquired_at` and `abandon_after` to the retry’s `statement_timestamp() + p_deadline_ms`. Time spent in the first acquisition/materialization is therefore added again on retry. The journey at `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:4458-4529` proves the retry count but injects immediate faults and never asserts the retained row’s absolute expiry after a delayed retry.
  - **Observable consequence:** if Oracle crashes after a delayed reacquisition, Forge destructive cleanup remains blocked beyond the query deadline. Repeated retries are bounded to one, so the overrun is bounded by pre-retry elapsed time, but revision 11’s exact deadline recovery is not met.
  - **Required correction:** derive the remaining duration from the existing absolute deadline immediately before every SQL acquisition, including the single retry, and fail with query timeout when no positive millisecond remains. Do not introduce a cap or a second deadline source.
  - **Focused closure proof:** delay the first materialization before forced `NotFound`, reacquire, crash/drop without release, and assert from PostgreSQL time that Forge continues refusing before the original absolute deadline and discards immediately after it—not one retry interval later.

### Suggestions

- None.

## Open Questions

- None.

## Recovery and proof assessment

- Intended crash fallback for dropped Oracle claims is safe and explicitly approved: failed/background release retains protection until `abandon_after`; no availability finding is raised for that tradeoff.
- Worker report loss, stale reports, and expired durable attempts have source-backed recovery paths. The remaining risk is the leader-term lifetime around blocked promotion recovery, not worker publication identity.
- Existing green aggregate claims are useful but do not cover either failure timing above.

## Verification Notes

- Reviewed the complete range’s relevant runtime source, SQL migration/query owners, server composition/supervision, and production-shaped Forge/Oracle journeys.
- Did not rerun the broad Postgres/object-store suite; the supplied `verify:bifrost` and principals results were considered but not accepted as proof of uncovered failure timing.
- `git diff --check c1508b375..7ac45dec9` currently reports blank-line-at-EOF errors in `revision/TASK-005-R1-implementation-reference.md:464` and `tasks/TASK-003-maintenance-and-removal.md:307`; this is a verification-claim discrepancy for the standards review, not a resilience finding.

## Overall result

**FAIL** — SYS-001 and SYS-002 are bounded, reachable resilience defects in required leader and deadline-recovery behavior.
