# TASK-004 R2 Persistence, Concurrency, and Shutdown Domain Review

## Reviewed Boundary

- Immutable base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Cumulative candidate: `2af4cc3ff95a609d1df682be4f633345f96934e1`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-004-r1/`
- Domain scope: Postgres transaction ownership, scheduled and manual durable
  work, claim/lease fencing, retry and attempt accounting, ACK-gated
  settlement and dispatch, crash reclaim, concurrency limits, cancellation,
  drain, and shutdown recovery.

The cumulative base-to-candidate range and the remediation delta were
reviewed. The reachable flow was traced from due-binding discovery and manual
enqueue through tenant transaction commit, permit acquisition, run claim,
execution, result ACK, fenced settlement or release, lease expiry/reclaim,
dispatch creation, capability restart, and runtime shutdown. `.codegraph/` is
absent, so caller and source coverage used repository search and direct source
inspection.

## Authority and Source Coverage

| Authority or source | Domain coverage |
|---|---|
| `AGENTS.md` sections 2-6, 9, and 11-12 | Server ownership of durable behavior, caller-owned `TenantConn` transaction lifecycle, bounded async work, and required real-boundary verification. |
| `architecture/agent-rules.md` | `WyrdPostgres`/`TenantConn`/`OperatorPool` ownership, forced-RLS transaction composition, internal mechanics without authorization audit, and exact test invocation rules. |
| `architecture/references/languages/spec-driven-development.md` | Approved-spec authority, cumulative remediation review, and tests as evidence rather than replacement authority. |
| `architecture/references/languages/testing-workflows.md` | Real-Postgres concurrency proof and capability-scoped evidence. |
| `architecture/references/domain/analytical-operations-reliability.md` | Durable lease/fence authority, bounded ownership, replayable recovery, and shutdown preserving durable evidence. |
| Approved spec revision 33: `REQ-078`-`REQ-082`, `REQ-085`-`REQ-087`, `REQ-096`-`REQ-097`, `REQ-115`, `REQ-119`, `REQ-121`-`REQ-122`, `REQ-135`-`REQ-137`, `REQ-145`-`REQ-146`; `INV-004`, `INV-007`, `INV-010`, `INV-011`; `AC-015`, `AC-020`, `AC-023`, `AC-028`, `AC-030` | Durable control/analytical split, exact frozen identity, schedule/cursor atomicity, lease fencing, ACK-gated completion, bounded supervision, immediate claim closure, drain, and restart recovery. |
| Original TASK-004 Scenarios 2-7; R1 verdict, `FIND-TASK-004-1`, `FIND-TASK-004-5`, and remediation acceptance criteria | Manual/scheduled enqueue, claims, retries, shutdown, raw-pool ownership, and the required lock-controlled closure proof. |
| `crates/wyrd/wyrd-sql/migrations/20260601000029_verifier_runs.sql`, `20260601000031_verifier_run_idempotency.sql`; `queries/verifier_runs.rs`; `tenant_conn.rs` | Durable constraints, RLS, unique occurrence/idempotency keys, claims, fencing, settlement, release/refund, and commit/drop behavior. |
| `crates/wyrd/wyrd-server/src/verification/{mod,scheduler,runner,permits,publisher,health}.rs` | Runtime composition, transaction opening, permit-before-claim ordering, cancellation boundaries, publication and settlement, drain, and supervision. |
| `crates/wyrd/wyrd-testing/src/verification.rs`; `pg_verifier_runs.rs`; `pg_verification_runtime.rs`; role-separated verification journey | Owner-backed fixture transactions, SQL transition proof, blocked-claim tests, commit-race release, detail-ACK crash reclaim, shutdown drain/release, and remote result identity. |
| TASK-004 and TASK-004-R1 implementation evidence | Recorded focused commands and full affected lanes; explicit implementation limit that an in-flight scheduler commit may land after `stop`. |

## Boundary Assessment

| Boundary | Result | Evidence |
|---|---|---|
| Connection and transaction ownership (`FIND-TASK-004-1`) | PASS | Scheduler, runner, publisher, and `VerificationFixture` retain `WyrdPostgres` and open tenant transactions through `tenant_conn`; only cross-tenant due/runnable/depth reads retain `OperatorPool`. No runtime or fixture owner carries raw `PgPool`, `TenantConn::acquire`, or the removed `RunnerPools`. |
| Durable schema, occurrence identity, and idempotency | PASS | Tenant-qualified keys, forced RLS, unique scheduled occurrences and requester/idempotency pairs, frozen run inputs, and one-transaction schedule insert/cursor movement remain intact. |
| Permit ordering and bounded execution | PASS | Tenant and global permits are acquired before durable runner claims and held through settlement; existing multi-tenant saturation evidence exercises tenant progress and configured ceilings. |
| Runner claim, fencing, attempts, and late-commit release | PASS | The runner races cancellation while opening/updating an uncommitted claim, awaits commit to a known outcome, then applies the existing token-fenced release/refund transition rather than executing when cancellation won. The deferred-trigger/advisory-lock test directly proves the commit-wins branch. |
| ACK-gated completion, dispatch, and crash reclaim | PASS | Completion and failed-verdict dispatch share one tenant transaction only after required result ACKs. The added detail-ACK/summary-unknown crash test proves the same run remains incomplete at attempt 1, creates no dispatch, is reclaimed at attempt 2, and dispatches once only after later complete publication. |
| Drain and recoverability | PASS | Pre-cancellation executions drain within the grace; over-grace work takes the fenced release/refund path. Capability loss leaves durable leases for expiry/reclaim, and no process-local run registry is introduced. |
| Scheduler shutdown admission (`FIND-TASK-004-5`) | **FAIL** | Cancellation drops the pass future even while `TenantConn::commit` is in flight. Once PostgreSQL has accepted `COMMIT`, dropping the future can only queue a later rollback outside the completed transaction. The candidate expressly records that such a commit may still create a pending run and advance its cursor after `stop`. See `PERSIST-R2-001`. |

## Prior-Finding Closure

| Prior finding | Result | Domain evidence |
|---|---|---|
| `FIND-TASK-004-1` | CLOSED | Raw application-pool ownership was removed from every runtime and fixture owner in scope; the existing Postgres owner now opens tenant work. |
| `FIND-TASK-004-5` | **OPEN** | Runner cancellation and commit-wins release are closed, and scheduler cancellation before commit is covered. The scheduler commit-in-flight case remains explicitly admitted by the candidate and is not covered by its scheduler lock test. |

## Material Proposed Finding

### PERSIST-R2-001 — INCORRECT: scheduler cancellation can still commit newly durable work

- **Prior stable finding:** `FIND-TASK-004-5` remains open for the scheduler
  commit-in-flight branch.
- **Violated obligation:** `REQ-146` requires shutdown to stop new claims
  immediately; `AC-030` requires proof that claims stop immediately; the
  original TASK-004 Scenario 7 requires only already admitted work to drain;
  and the R1 remediation acceptance criterion requires lock-controlled proof
  that scheduler cancellation admits no new durable work.
- **Exact locations:**
  - `crates/wyrd/wyrd-server/src/verification/scheduler.rs:105-117,161-166`
  - `crates/wyrd/wyrd-sql/src/tenant_conn.rs:67-73`
  - `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1181-1250`
  - `changes/active/verified-change-contract/review/TASK-004-r1/TASK-004-R1-close-validated-runtime-gaps.md:226,248`
- **Evidence and reachability:** `VerificationScheduler::run` races the entire
  `report_depth`/`pass` future against `stop`. `schedule_tenant` commits the
  occurrence with `conn.commit().await`; cancellation therefore drops that
  commit future. `TenantConn::commit` delegates to SQLx's consuming
  transaction commit. SQLx marks the transaction closed only after awaiting
  PostgreSQL's `COMMIT`; dropping it after the command was sent queues a
  rollback, but PostgreSQL can process `COMMIT` first, leaving the subsequent
  rollback outside any transaction. The implementation report independently
  confirms the reachable consequence: "A scheduler commit already in flight
  when `stop` fires may still land atomically as a pending run." The added
  scheduler test blocks the run write before commit and proves only the
  uncommitted rollback branch; unlike the runner test, it never holds the
  scheduler at deferred `COMMIT` and therefore cannot falsify the conceded
  branch.
- **Observable consequence:** after shutdown admission is closed, the
  scheduler may still create a new pending run and advance the binding cursor.
  That work was not durable before cancellation, contradicts immediate claim
  closure, and changes the next scheduled occurrence visible after restart.
  The fact that the stopping process does not execute the run does not remove
  the post-cancellation durable admission.
- **Required testable correction:** make scheduler occurrence commit and
  shutdown closure have one explicit, known ordering at the existing scheduler
  owner, without weakening schedule insert/cursor atomicity. If shutdown wins,
  the occurrence transaction must be known rolled back and no later tenant may
  begin; if commit wins, it must be classified as admitted before closure from
  a completed commit outcome rather than by cancelling an unknown commit.
  Reuse the existing transaction and cancellation mechanisms; do not add a
  shutdown table, second scheduling protocol, process-local run registry, or
  cross-table recovery path. Add a deferred-commit, lock-controlled Postgres
  test analogous to the runner commit-race test that cancels while scheduler
  `COMMIT` is blocked and proves the selected ordering, exact run count, and
  cursor state after release and restart.

## Verification Evidence and Limits

- `git diff --check 9431906eeb1c7b67a0efcec09487fd1d848f70a8..2af4cc3ff95a609d1df682be4f633345f96934e1`
  passed during this review.
- The cumulative migrations, queue transitions, scheduler, runner, publisher,
  runtime supervisor, fixture, and relevant Postgres/runtime/journey tests were
  inspected directly.
- The remediation report records all focused persistence/concurrency tests and
  `test:sql`, `test:wyrd`, `test:bifrost:integration:server`,
  `test:bifrost:journey:server`, `check:tenant-isolation`,
  `check:from-pools-allowlist`, `fmt`, and `lints` as passing at the immutable
  candidate.
- No Cargo-backed lane was rerun in this Wave-1 review. Existing green lanes do
  not cover the conceded scheduler commit-in-flight interleaving.
- Drift and Eval production engine bodies and Operator delivery remain assigned
  to later tasks; completed-result and dispatch-control paths are exercised by
  the approved test-support engine script and are not treated as missing
  TASK-004 behavior.
- Candidate identity still resolved to
  `2af4cc3ff95a609d1df682be4f633345f96934e1` immediately before this report was
  written.

## Overall Result

**FAIL**

Connection ownership, runner claim fencing and late-commit refund, durable
retry/attempt accounting, ACK-gated settlement, crash reclaim, and bounded
drain satisfy this domain. The scheduler still has an explicitly reachable
post-cancellation durable commit, so `FIND-TASK-004-5` is not fully closed.
