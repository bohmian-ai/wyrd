# Process lifecycle and concurrency domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review inputs: `review/TASK-008-r1/` through `review/TASK-008-r4/`
- Reviewed remediation: `review/TASK-008-r4/TASK-008-CLOSEOUT-R3-process-boundary-and-proof.md`

`HEAD` resolved to the candidate before and after this review. CodeGraph is not
indexed in this checkout, so navigation used Git, `rg`, and direct source
inspection. Prior findings and remediation evidence were treated as hypotheses
and checked against the cumulative candidate. The integrator's explicit
rejection of extending the report total across post-report teardown is accepted
caller authority: report-before-teardown is intentional and is not a finding.

## Reviewed boundary

This pass traced the benchmark's process and async-runtime ownership end to end:

1. `mise run bench:capacity` fixes one wall-clock start and deadline before
   RustFS setup. Its `phase` function places each RustFS, Postgres-wrapper,
   build, and benchmark-run command in a GNU `timeout` process group, gives it
   only the remaining absolute budget, forwards `INT`/`TERM`, escalates from
   `TERM` to `KILL`, waits for the timeout owner, and sweeps remaining members
   of that process group.
2. The Postgres wrapper owns its isolated Compose project, runs the nested Bash
   command synchronously, and tears the project and volumes down from its
   `EXIT`/`INT`/`TERM` traps. The nested build and run phases reuse the same
   exported `phase` definition and absolute deadline.
3. The capacity binary converts the exported wall-clock boundary to one Tokio
   `Lifetime`. Preparation and measurement are cooperatively bounded;
   `OperatorRun` owns each migration/setup child across cancellation; clients
   shut down before replicas; and `LocalServer` remains the synchronous
   signal/wait/log-copy owner.
4. Normal replica cleanup now moves each `LocalServer::stop` to
   `tokio::task::spawn_blocking`, awaits replicas newest first, and reverses the
   collected results back to ordinal order. Abnormal ownership fallback still
   kills and reaps through `Drop`.
5. The report is intentionally written after in-binary client and replica
   cleanup but before Postgres-wrapper teardown. Per caller authority, its
   `total_seconds` is an in-binary report value rather than a post-teardown
   rewrite.

The cumulative candidate does not change production server scheduling,
Postgres claims or fences, data durability, or runtime concurrency policy.
Completed database and object-store effects remain explicit partial progress;
the isolated Postgres lifecycle is removed by the wrapper.

## Authority and source coverage

| Boundary | Authority and inspected source | Assessment |
|---|---|---|
| One complete bounded command | Revision-57 `REQ-171` (`spec.md:1706-1713`); R3 `AC-R3-1`; `mise.toml:507-557` | **PASS.** Every mandatory phase derives its timeout from the one exported deadline, runs in a non-foreground timeout group, has finite escalation, returns failure on expiry, and prevents the shell from advancing under `errexit`. |
| Nested process groups and signals | R3 process-ownership outcome; `mise.toml:526-556`; native GNU `timeout` behavior; focused cancellation probe | **PASS.** Each waiting shell installs a trap that signals its timeout owner; timeout forwards to its group, and nested shells repeat the handoff to their inner timeout groups. An early TERM probe with a TERM-ignoring inner command exited after kill escalation with no remaining `timeout` or `sleep`. |
| Wrapper cleanup and durable isolation | R3 `AC-R3-1`; `scripts/postgres/with-test-postgres.sh:11-40,52-123`; `mise.toml:545-556` | **PASS.** The wrapper receives orderly termination before forced escalation and its teardown trap owns Compose project/volume removal. The nested run deadline expires early enough to leave the outer wrapper its separate teardown interval. |
| In-binary deadline and partial progress | Revision-57 `REQ-171`; `capacity/main.rs:81-275,277-425` | **PASS.** One `Lifetime` owns preparation, measurement, client shutdown reserves, replica reserves, and failure reporting. Cancellation and surviving durable effects are documented at the owner. |
| Migration/setup child ownership | R3 preserved behavior; `release_server.rs:130-207,636-732`; `capacity/main.rs:478-510` | **PASS.** `OperatorRun` polls owned children without blocking Tokio, keeps diagnostic stderr, stores credential-bearing stdout in an unnamed file, and kills/waits on cancellation. |
| Normal replica cleanup concurrency | `AGENTS.md` section 6; R3 `AC-R3-3`; `capacity/main.rs:581-677`; `release_server.rs:365-405` | **PASS.** Synchronous TERM/grace/kill/wait/log-copy remains on `LocalServer`, but executes on Tokio's blocking pool. Await order, result order, stop grace, and errors are preserved. |
| Abnormal server cleanup | R3 adjacent behavior; `release_server.rs:112-128,365-405,487-502` | **PASS.** An unconsumed `LocalServer` still kills and waits for its child and preserves the abnormal log directory. |
| Later-phase prevention and diagnostics | R3 `AC-R3-1`/`AC-R3-5`; `capacity/main.rs:803-935,994-1153` | **PASS.** The in-binary stalled-setup proof checks child reaping, no later setup or measurement, retained server/setup logs, and a failed report. Task-text proofs check a pre-wrapper failure starts no later phase and a nested run failure still observes wrapper teardown. |
| Report/teardown sequencing | Caller instruction; R3 implementation-evidence rejection of `AC-R3-2`; `capacity/main.rs:379-425`; `report.rs:371-444`; Postgres wrapper exit trap | **PASS under caller authority.** The report closes before wrapper teardown by design. No post-report total or outer report rewrite is required. |
| Deferred default qualification | Caller instruction; prior `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED, non-blocking.** The unmodified full default benchmark remains sequenced after integration and supplies no evidence for this candidate review. |

## Material proposed findings

None.

The latest remediation closes the prior process-tree and Tokio-worker findings
within the accepted boundary. I found no reachable lifecycle, signal,
cancellation, cleanup-order, or process-survival defect that violates the
approved task after expanding the trace through the outer wrapper and nested
timeout groups.

## Prior-finding closure

| Finding | Domain result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-2` process-tree portion | **CLOSED.** Non-foreground timeout groups, finite escalation, a post-wait group sweep, and task-entry proofs replace the descendant-excluding foreground timeouts. The rejected post-report-total sub-part is not reviewed as an open obligation. |
| `FIND-TASK-008-CLOSEOUT-14` | **CLOSED.** Normal `LocalServer::stop` work executes through `spawn_blocking`; the heartbeat proof establishes that the Tokio worker remains available during a delayed stop. |
| `FIND-TASK-008-CLOSEOUT-15` | Outside this domain's process boundary; the current task record contains complete pinned commands for the six named capacity tests. |
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED** by explicit caller sequencing and non-blocking for this candidate. |

## Verification and proof limits

- Recorded remediation evidence reports:
  - `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`
    — 14 passed, 4 skipped;
  - both task-entry process proofs passed when selected with
    `--run-ignored=only` (about 7 seconds and 30 seconds respectively);
  - the slow-replica-stop heartbeat proof passed;
  - the existing stalled-tenant-setup process proof passed;
  - the focused `release_server` tests, format, lints, and diff check passed.
- Source inspection confirms the ignored process tests extract the actual
  `bench:capacity` task text and change only the single 30-minute limit. Their
  TERM-ignoring descendants inherit `SIG_IGN`, so the passing tests exercise
  forced group escalation rather than a cooperative child.
- A read-only local process probe additionally exercised early TERM through
  nested exported `phase` calls with a TERM-ignoring inner process. It exited
  after the configured escalation and left no live timeout or child process.
- The task-entry tests accept an already-terminated zombie as stopped while its
  new parent completes reaping. This does not leave executing benchmark work;
  direct owners (`timeout`, `OperatorRun`, and `LocalServer`) each wait for the
  children they can own. No material lifecycle gap follows from that transient
  platform reaping boundary.
- The full unmodified default run was not executed. By caller instruction,
  `FIND-TASK-008-CLOSEOUT-13` remains deferred to integration and is neither a
  blocker nor empirical capacity evidence here.

## Overall result

**PASS**

The candidate now bounds each command phase and its descendants under the one
absolute deadline, preserves wrapper cleanup, prevents later phases after
failure, owns direct subprocesses across cancellation, and keeps synchronous
replica stop work off Tokio workers. The intentional report-before-teardown
sequence and deferred default benchmark are non-blocking under caller authority.
