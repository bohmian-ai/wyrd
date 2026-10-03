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
| 2 | Benchmark fixes (reviews R1–R5) | verified-change-contract rev 57 | `agent-a82d72f51901e1dc5` / `worktree-agent-a82d72f51901e1dc5` | `1d05642bf` | R5 verdict FIX_REQUIRED: 2 findings, both accepted as valid. FIND-16: replica shutdown must sit on an owner struct. FIND-17: the audit drain must count decisions still held in a replica's memory and committed rows that arrive after the stop. A fresh implementer is fixing them | R6 Codex review after the fixes |
| 3 | Forge concurrent planning (TASK-001, TASK-002) | forge-concurrent-planning rev 2 | `agent-ac58f45cb5b747685` / `worktree-agent-ac58f45cb5b747685` | `be922f434` | r2 verdict FIX_REQUIRED: 8 findings, all accepted as valid (failover deadline and kill proof, shutdown release error, worker expiry delay, full failed batch, AC-010 compaction proof, raw transaction, task revision, exact commands). The live_rewrite test change was judged sound. A fresh implementer is fixing them | r3 Codex review after the fixes |
| 4 | Audit outbox (3 tasks) | audit-outbox rev 1 | `agent-aad682fbca5074900` / `worktree-agent-aad682fbca5074900` | `f451d52be` | All 3 tasks implemented (T01 `0fa3d0d6b`, T02 `d2af088b9`, T03 `f451d52be`), all lanes green; r1 Codex review running | Validate r1 findings, then fix or merge |
| 5 | Verifier runtime under load (TASK-013, 014, 015) | verified-change-contract rev 59 (`5e5623a2e`) + rev 60 (`82f142580`) | `agent-a0ed64ce133bff9d3` / `worktree-agent-a0ed64ce133bff9d3` | `8b7218a92` | TASK-013 done (results through capture writer, no SYSTEM tokens; net −1,100 lines). TASK-015 paused because its outbox duplicated the audit outbox. Agent now on TASK-014 | TASK-015 after the audit outbox merges (see Shared outbox machinery). Then a Codex review |

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

### Shared outbox machinery (TASK-015, after the audit outbox merges)

The first TASK-015 attempt hand-wrote an Eval run-request outbox that copied
the audit outbox's queue, pending counter, stop signal, writer task, retry
backoff and per-tenant grouping. That copy is not allowed. TASK-015 instead:

- Moves the audit outbox's queue and writer machinery into one generic outbox
  type in a shared crate. It owns the queue, pending count, idle signal, stop,
  writer task, per-tenant grouping, retry with backoff, no count limit,
  never-drop retention, graceful-shutdown flush, and counting and logging of
  losses.
- Leaves each use with only what is specific to it: the item type and one
  "write this tenant's items" call. Audit's call writes to `vala.audit_staging`
  through Vala's Postgres. Eval's call is the multi-row
  `enqueue_observation_batch` insert through Wyrd's Postgres.
- Uses one shared type with **separate instances** for audit and Eval, not one
  queue. They write to different databases owned by different crates (no
  cross-crate SQL), and a slow database for one must not hold up the other.
  Each instance has its own queue and writer, so sharing the type costs no
  throughput.
- Each writer writes a tenant's queued items in one multi-row insert. If a
  single writer per instance is ever measured as the limit, write tenants
  concurrently inside the shared type. Never add a second outbox.
- Keeps the work-in-progress parts that are not duplicates: deriving run
  requests from Eval frames, the multi-row insert SQL, removing the 256 cap,
  the wiring, and the tests. Deletes the duplicate machinery.
- Proves both instances with their existing audit and Eval tests. Adds focused
  tests of the shared type for retry without drop, shutdown flush, and loss
  counting.

## Order of work

1. Now, in parallel: the audit outbox r1 review and fix loop; the benchmark
   R5 fixes then the R6 review (which checks only that the R5 findings are
   closed and nothing regressed); the Forge r2 fixes then the r3 review; and
   verifier runtime TASK-014.
2. Merge the audit outbox into `TASK-008` first. The benchmark and the
   verifier runtime both depend on it.
3. Merge Forge any time after PASS. It does not overlap with the others.
4. Merge the benchmark after the audit outbox. While merging, point its
   pending-audit metric at the new audit outbox and rerun its focused tests.
5. Verifier runtime, last:
   - merge the current `TASK-008` into its worktree;
   - run TASK-015 (the shared outbox machinery);
   - its own Codex review until PASS;
   - merge.
6. Closeout steps below, on the fully merged branch.

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
