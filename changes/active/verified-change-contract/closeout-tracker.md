# TASK-008 closeout tracker

The goal is to close out TASK-008 and the gateway capture refactor. Every row
below must reach **Done** before the goal is complete. Update this file
whenever a status changes.

Rules for every worktree:

- Implement with `wyrd-implement`.
- Review with `wyrd-task-review`. The reviewer is always Codex `gpt-5.6-sol`
  at medium effort, and each worktree gets its own separate review.
- Accept only valid findings. Valid findings go to a fresh `wyrd-implement`
  agent, and the worktree is reviewed again until the verdict is PASS.
- Merge into `wyrd/verified-change-contract/TASK-008` only after PASS.
- Delete the worktree and its `target/` directory after merging. Keep
  `~/Documents/GitHub/wyrd/target`.

## Workstreams

| # | Work | Spec | Worktree / branch | Latest commit | Status | Next step |
|---|---|---|---|---|---|---|
| 1 | Gateway capture refactor | verified-change-contract rev 54 | merged | `de3dfaca6` | **Done** | — |
| 2 | Benchmark fixes (reviews R1–R4) | verified-change-contract rev 57 | `agent-a82d72f51901e1dc5` / `worktree-agent-a82d72f51901e1dc5` | `0973a03e5` | R4 findings fixed in `0a8ad01b7` (process-group deadline, blocking stop off async workers, exact test commands); R5 Codex review running | Validate R5 findings, then fix or merge |
| 3 | Forge concurrent planning (TASK-001, TASK-002) | forge-concurrent-planning rev 2 | `agent-ac58f45cb5b747685` / `worktree-agent-ac58f45cb5b747685` | `f67b59b2c` | r1 findings fixed (all lanes green); r2 Codex review running against base `ce5c09ef3` | Validate r2 findings, then fix or merge |
| 4 | Audit outbox (3 tasks) | audit-outbox rev 1 | `agent-aad682fbca5074900` / `worktree-agent-aad682fbca5074900` | `0fa3d0d6b` (in progress) | Implementing | Codex review when the agent finishes |
| 5 | Verifier runtime under load (tasks TASK-013+) | verified-change-contract rev 59 (`5e5623a2e`) + rev 60 (`82f142580`) | new agent worktree (rev59-impl) | — | Planning and implementing | Codex review when the agent finishes |

### 2. Benchmark fixes

- Implemented: one 30-minute run deadline; the judge provider wait is
  reported; one `Benchmark` owner type; cancellation docs; a step-kind enum;
  Scribe staged members counted in the backlog; a backlog drained at 60.0 s
  passes; one report table; matched CPU and memory windows; the duplicate-run
  check removed; the `bench:bifrost:query-capacity` benchmark deleted.
- Moved into normal test lanes: two-replica exactly-once claims, flooding
  tenant fairness, the failed LLM-judge verdict, and 100-feature drift landing
  exactly once with flat client bytes.
- Review finding 13 (the full default benchmark run) is done at integration,
  after workstreams 3, 4 and 5 merge.

### 3. Forge concurrent planning

- Every replica plans, using per-table demand claims that are released when
  work completes, fails, or the server shuts down.
- Batches run back to back, and periodic maintenance runs when each table is
  due.
- Worker claims are fair across tenants. All claims last 60 s.
- Gate repairs in `7b56776ff`: the unwrap audit skips out-of-line test
  modules; an SDK test's casts were fixed; one Python file was formatted.
- Open notes from the implementer:
  - A failed demand moves to the back of its tenant's queue.
  - The table publication lease is still 15 minutes, which the spec left
    unchanged.

### 4. Audit outbox

- Every audit decision goes through one batched, non-blocking outbox, like
  Oracle's.
- The audit-unavailable error codes are removed.
- Publication progress no longer lives on the chain-head row.

### 5. Verifier runtime under load (revision 59)

- REQ-077 (rev 60): Eval runs come from a batched run-request outbox like audit's. The ack covers receipt only, flushes are batched per tenant, a failed flush retries and never drops, graceful shutdown flushes, and losing unflushed requests on a hard kill is accepted.
- REQ-086 and REQ-087: results go through the server-internal capture writer.
  No tokens and no Gate are involved, and Gate refuses public writes to the
  result tables. Reads use a tokenless SYSTEM authority.
- REQ-181: there is no 64-tenant limit and no execution count cap. A run
  refused by a full shared resource returns to the queue without consuming an
  attempt.
- REQ-182: Verifier Cards are cached per process, LRU-evicted at 64 MiB.
- REQ-183: each run's result is stored once in Postgres and written from
  those stored bytes every time, so a run is never executed twice.
- REQ-184: leases are renewed with one statement per tenant.
- REQ-185: a run holds a connection only to claim, store its result, settle,
  and renew its lease.
- Drift runs its SQL and scores what comes back. It has no completeness check.
- Proof: AC-044, plus the revised AC-014, AC-023, AC-030 and AC-043.

## Integration and closeout (after workstreams 2–5 pass review)

| Step | Status |
|---|---|
| Merge the approved branches into `wyrd/verified-change-contract/TASK-008` | Pending |
| `mise run bench:capacity` (default run) with results recorded (review finding 13) | Pending |
| `mise run bench:capacity -- --profile` | Pending |
| Scenario 7: optimizations with before/after numbers, or a recorded justification for none | Pending |
| Replace every `GATE_PENDING` in the CLOSE-01 matrix | Pending |
| `mise run gate`, run once | Pending |
| `git diff --check` | Pending |
| Evidence tables in `tasks/task-008-closeout.md` | Pending |
| Commits ending with the session trailer | Pending |
| Delete the agent worktrees and their `target/` directories; keep `wyrd/target` | Pending |
