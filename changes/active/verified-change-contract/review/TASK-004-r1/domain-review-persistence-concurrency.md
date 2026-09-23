# TASK-004 Persistence and Concurrency Domain Review

## Reviewed Boundary

- Immutable base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Cumulative candidate: `49ad24707de47378b9df51764034c49a129c6b8b`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Domain scope: Postgres run/dispatch control state, caller-owned transactions,
  scheduling cursors, idempotent enqueue, claims, lease fencing, retry and
  terminal transitions, permit ordering and fairness, runtime supervision,
  health, crash/restart recovery, and cooperative shutdown

The complete base-to-candidate diff was reviewed. The reachable flow was traced
from manual and scheduled enqueue through `wyrd.verifier_runs`, cross-tenant
discovery, tenant-scoped claim and commit, engine execution, remote result ACK,
fenced settlement, failed-verdict dispatch insertion, lease expiry/reclaim,
capability restart, and shutdown release. `.codegraph/` is absent, so symbol and
caller coverage used repository search and direct source inspection.

## Authority and Source Coverage

| Authority or source | Coverage |
|---|---|
| `AGENTS.md` sections 2-6, 9, and 11-12 | Server ownership of durable behavior, typed identities, bounded async work, caller-owned `TenantConn` transactions, test tiers, and completion gates. |
| `architecture/agent-rules.md` | Forced-RLS tenant transactions, connection boundaries, audit exclusion for worker mechanics, and exact verification expectations. |
| `changes/active/verified-change-contract/spec.md` REQ-078-082, REQ-085-087, REQ-096-097, REQ-115, REQ-119, REQ-121-122, REQ-135-137, REQ-145-146; INV-004/007/010/011; AC-015/020/023/028/030 | Durable control/analytical split, exact frozen run identity, schedule and cursor atomicity, leases/retries, result ACK settlement, concurrency ceilings, supervision, health, and immediate claim stop plus bounded drain. |
| TASK-004 Scenarios 2-7 and acceptance criteria | Manual enqueue, one-window scheduling, durable claims/retries, ACK-gated completion, status, limits, restart, and shutdown behavior. |
| `verification-control-flow.html` | Maintainer-locked scheduler, runner, retry, handoff, bounded capacity, and 30-second shutdown flow. |
| `architecture/references/languages/testing-workflows.md` | Required real-Postgres seam proof and user-journey priority. |
| `architecture/references/domain/analytical-operations-reliability.md` | Durable lease/fence authority, bounded ownership, restart recovery, and shutdown preservation of replayable work. |
| SQL migrations 29 and 31; `queries/verifier_runs.rs` | Table constraints, forced RLS, unique occurrence/idempotency keys, transactional cursor movement, claims, fencing, retry/exhaustion, release, status, and cross-tenant discovery. |
| `verification/{scheduler,runner,mod,permits,health,publisher}.rs` | Runtime loops, permit-before-claim ordering, execution/publication bounds, supervision/restart, liveness, and drain/release behavior. |
| `pg_verifier_runs.rs` and `pg_verification_runtime.rs` | Duplicate occurrence, stale lease, retry exhaustion, dispatch settlement, release/refund, permit ceilings, restart, result publication, runner drain, and scheduler restart coverage. |

## Boundary Assessment

| Seam | Result | Evidence |
|---|---|---|
| Durable schema and tenant isolation | PASS | Migration 29 gives runs and dispatches tenant-qualified keys, input/status shape constraints, enabled and forced RLS, and tenant runtime access through `TenantConn`; cross-tenant discovery is read-only through `OperatorPool`. Migration 31 scopes idempotency keys by tenant and requester. |
| Manual enqueue and idempotency | PASS | `VerificationControl::enqueue` appends the allow/deny audit and enqueues in one tenant transaction. `enqueue_manual` serializes equal requester/key pairs with a transaction advisory lock and compares the stored request digest before reuse. |
| Scheduler occurrence atomicity | PASS, except shutdown finding below | `schedule_next_due` locks one due binding, resolves activity/readiness, inserts the unique fixed-window run, and advances to the next future cursor in the caller-owned transaction. Concurrent schedulers and restart are covered against Postgres. |
| Claims, retries, and fencing | PASS | Claim increments the durable attempt under `FOR UPDATE SKIP LOCKED`, installs a UUIDv7 lease token, and commits before execution. Completion, retry, terminal settlement, and shutdown release all predicate on the token and running state. Expired final attempts settle errored; stale holders affect zero rows. |
| Result/dispatch settlement | PASS | The runner completes only after every required publication ACK. Completion and failed-verdict dispatch fanout share one tenant transaction, while unique run/operator identity absorbs settlement replay. Partial analytical rows cannot create dispatches. |
| Capacity and tenant progress | PASS | The runner acquires the tenant permit before the global permit and both before the durable claim. Defaults are 16 global/4 tenant, permits remain held through settlement, and the Postgres runtime test exercises cross-tenant progress under saturation. |
| Crash/restart supervision and health | PASS | Capability exits are joined, marked down, counted, restarted after bounded backoff, and marked up on respawn. Runner task loss leaves only durable leases, which expire into the token-fenced reclaim path. |
| Shutdown claim admission | **FAIL** | Scheduler and runner cancellation is checked outside, but not across, their awaited durable claim transactions. See `PERSIST-001`. |

## Material Proposed Findings

### PERSIST-001 — INCORRECT: shutdown can commit new scheduler and runner claims after cancellation

- **Violated obligation:** Spec REQ-146 and AC-030 require shutdown to stop new
  claims immediately, then drain only work already in flight. TASK-004 Scenario
  7 repeats that contract.
- **Locations:**
  - `crates/wyrd/wyrd-server/src/verification/scheduler.rs:89-104,117-149`
  - `crates/wyrd/wyrd-server/src/verification/runner.rs:214-251,260-270`
- **Evidence:** `VerificationScheduler::run` calls `report_depth()` and the
  complete `pass()` before its first cancellation select. Cancellation during a
  pass is not observed by `pass` or `schedule_tenant`, so additional due-binding
  transactions can continue through `conn.commit()` after shutdown. The runner
  checks `stop` at line 230, but then awaits `self.claim(...)`; cancellation
  while `TenantConn::acquire`, the claim update, or its commit is pending is not
  rechecked. The successfully claimed run is subsequently spawned at lines
  245-251. `process` does not treat `stop` as an execution cancellation; it
  executes that newly claimed run during the drain unless the later `abandon`
  deadline fires.
- **Observable consequence:** a shutdown request can advance a schedule cursor
  and enqueue a new run, or lease and begin executing queued work, after claim
  admission was required to close. A blocked/slow database transaction widens
  this race. The newly started work may consume the 30-second drain, publish a
  result, or be released only at its end, so shutdown behavior depends on work
  admitted after cancellation rather than only pre-existing in-flight work.
- **Required testable correction:** make cancellation authoritative at both
  durable claim commit boundaries. A cancelled scheduler must roll back or stop
  before committing another binding claim/cursor advance, and a cancelled
  runner must roll back or release/refund any claim that cannot be proven
  committed before admission closed; it must not spawn its execution as new
  drain work. Preserve the existing one-transaction schedule atomicity,
  permit-before-claim ordering, lease token fencing, and durable identity.
  Add real-Postgres tests that hold the relevant row/transaction lock, start a
  scheduler tick or runner claim, cancel shutdown while the operation is
  blocked, release the lock, and assert no post-cancellation cursor/run creation
  and no post-cancellation lease/execution respectively.

## Verification Evidence and Limits

- `git diff --check 9431906eeb1c7b67a0efcec09487fd1d848f70a8..49ad24707de47378b9df51764034c49a129c6b8b`
  passed.
- The complete migration, SQL queue implementation, runtime scheduler/runner,
  permits, health, publisher, manual control transaction, and their Postgres
  tests were inspected directly.
- Existing tests substantiate duplicate cron exclusion, activity/readiness
  skipping, lease expiry and stale-token fencing, bounded retry exhaustion,
  dispatch insertion, permit limits, runner crash/restart, and release/refund.
- Existing shutdown tests cancel after one runner execution is already claimed;
  they do not block a scheduler or runner claim across cancellation and therefore
  do not exercise `PERSIST-001`. The scheduler tests stop only after the due
  occurrence is already committed.
- No Cargo-backed lane was rerun in this Wave-1 review. The task artifact records
  the broader named lanes as passing, but those results do not cover the missing
  cancellation/claim interleaving above.
- The candidate still resolved to
  `49ad24707de47378b9df51764034c49a129c6b8b` immediately before this report was
  written.

## Overall Result

**FAIL**

The durable schema, transaction composition, occurrence deduplication, lease
fencing, retries, ACK-gated settlement, capacity bounds, and crash recovery are
otherwise coherent, but the reachable post-cancellation claim race violates the
explicit shutdown contract and requires bounded correction plus direct
Postgres concurrency proof.
