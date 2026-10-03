# TASK-008 round-five behavior review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Approved authority: `changes/active/verified-change-contract/spec.md`, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review hypotheses: `review/TASK-008-r1/` through `review/TASK-008-r4/`
- Reviewed remediation: `review/TASK-008-r4/TASK-008-CLOSEOUT-R3-process-boundary-and-proof.md` and its implementation evidence

The candidate remained at the named commit throughout this review. The checkout
has no `.codegraph/` index, so navigation used Git, `rg`, and direct source and
caller inspection. I reviewed the complete cumulative diff, then traced the
latest remediation from the mise task through its process groups and from
`Benchmark::clean_up` through `stop_replicas` and `LocalServer::stop`.

The caller explicitly defers `FIND-TASK-008-CLOSEOUT-13`, the full unmodified
default benchmark, until the other workstreams have merged. It is not a blocker
for this candidate and supplies no empirical AC-040/AC-041 qualification.

The integrator also explicitly rejected the prior request to include work after
the report is written in `Report::total_seconds`. The report is intentionally
written before Postgres-wrapper teardown. I therefore did not reassert that
rejected sub-part of prior `FIND-TASK-008-CLOSEOUT-2`.

## Realistic caller-to-result trace

`mise run bench:capacity` fixes one start and absolute deadline before any
RustFS action. Its `phase` function starts each mandatory RustFS, Postgres
wrapper, release build, and capacity-run command under GNU `timeout` without
`--foreground`, so the timeout PID is the process-group leader. Each phase
computes its share from the same deadline, gives the group `TERM`, escalates to
`KILL`, waits for the timeout owner, and sweeps any remaining member of that
group before a later phase may begin. The nested build and run phases reserve
more time than the outer Postgres wrapper, leaving the wrapper its teardown
window. The two task-entry tests execute the actual TOML script with a shortened
deadline and TERM-ignoring descendants; they prove that a pre-wrapper hang stops
later setup and that a nested run hang still permits wrapper teardown before
the command fails.

Inside the binary, `Lifetime` continues to bound preparation and measurement.
`Benchmark::run` performs client cleanup and passes every owned replica to
`stop_replicas`. That function preserves newest-first stop order but moves each
synchronous `LocalServer::stop` invocation to `tokio::task::spawn_blocking`,
awaits it, and restores ordinal result order. Thus the existing grace, clean
exit, forced kill, reap, log copy, and diagnostic result stay with
`LocalServer`, while Tokio timers and sibling async work can progress.

The remainder of the cumulative candidate retains the single `capacity`
driver, public-client workload, report and evidence owners, and the focused
server/SDK correctness tests required outside the benchmark. No latest-round
change enters a production server or public-contract owner.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Review the exact approved revision 57 and current task | `spec.md:1-4`; task frontmatter and revision-57 implementation matrix | Immutable Git/source inspection | PASS |
| CLOSE-01: inherited TASK-008 proof owners remain present | Cumulative diff retains the registered SDK/server/runtime/storage/auth/audit/Operator/generated-contract tests mapped in the task; the latest delta changes only benchmark lifecycle and evidence artifacts | Recorded focused and gate evidence inspected; no inherited production owner changed in the latest delta | PASS, subject to verification limits |
| REQ-171: exactly one server capacity entry point and binary | `mise.toml` `[tasks."bench:capacity"]`; one `capacity` target; obsolete server-capacity drivers deleted | Cumulative changed-path and repository search inspection | PASS |
| REQ-171: four-tenant public-client mix and warmup/ramp/knee/sustained/scale-out sequence | `capacity/load.rs`, `fixture.rs`, `main.rs::Benchmark::measure`, `step.rs::Deployment::run` | Capacity target: 14 passed; focused workload tests included | PASS structurally |
| REQ-171: traffic, errors, latency, overhead, ingest drain, all required backlogs, CPU/memory, and verdict report | `capacity/{evidence,step,report}.rs`; one shared step/operation table and verdict-step selection | Capacity target: report/evidence/drain/resource tests passed | PASS structurally |
| AC-040: five reference workloads, non-judge paired overhead and sample floor, judge wait reported separately | `fixture.rs`, `evidence.rs::Scrapes::overhead`, `judge.rs`, `report.rs::overhead` | Focused capacity tests passed; full empirical qualification is caller-deferred | PASS structurally; empirical result deferred |
| AC-041/AC-042: default-queue workload construction plus real-server exactly-once, flat-byte, burst, admission, sealing, and retry proof | `load.rs`; queue/client/SDK journey owners and `observe_run.rs::sustained_hundred_feature_drift_lands_exactly_once_with_flat_client_bytes` remain in the cumulative candidate | Recorded real-server and SDK evidence inspected; full empirical qualification is caller-deferred | PASS structurally; empirical result deferred |
| Revision-57 test-owned correctness: cross-replica exclusive claims, tenant fairness, direct failed judge/SPC judgments, queued Custom failure | Focused server and Rust SDK tests remain outside the benchmark, as REQ-171 requires | Exact commands and results recorded in the task | PASS |
| Prior findings 1 and 3 through 12 remain closed | Single entry point, separate judge wait, current task metadata, cohesive lifecycle owner, typed step identity, complete backlog, exact drain edge, common report schema/resource interval, and test-owned correctness remain present | Cumulative source and prior proof inspection | PASS |
| Prior FIND-2 accepted portion / AC-R3-1: the one absolute deadline owns mandatory command descendant groups, stops later phases, and permits wrapper teardown | `mise.toml:511-556` `phase`; shared exported deadline; nested reserve ordering and group sweep | `a_hung_setup_step_is_killed_with_its_descendants_by_the_deadline` and `a_hung_benchmark_run_is_killed_with_its_descendants_by_the_deadline` passed in this review | PASS |
| Rejected FIND-2 sub-part / AC-R3-2 | The report remains intentionally written before wrapper teardown; no second report or outer rewrite | Explicit integrator decision supplied by the caller and recorded in remediation evidence | EXCLUDED by authority; not reasserted |
| Prior FIND-14 / AC-R3-3: ordinary replica stop does not block a Tokio worker while preserving stop semantics and ordering | `capacity/main.rs:649-677` moves each existing synchronous stop to `spawn_blocking`, newest first, then reverses results to ordinal order | Source trace; recorded focused test passed in a suitable systemd-user-manager environment. This review's rerun could not start its stand-in scope because the current sandbox has no user D-Bus | PASS structurally; environment-limited rerun |
| Prior FIND-15 / AC-R3-4: every current named capacity unit test has a complete pinned command | Task implementation matrix lines 1501-1504 contain six complete `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=...)'` commands | Commands recorded as individually passing; whole capacity target passed in this review | PASS |
| AC-R3-5 and task non-goals: workload, SLOs, report schema, production behavior, and public surfaces are unchanged | Latest implementation delta is confined to `mise.toml`, benchmark lifecycle/tests, and task/review evidence | Latest diff and cumulative consumer inspection | PASS |
| FIND-TASK-008-CLOSEOUT-13: unmodified default capacity execution | Caller sequences it after integration | Not run by design | DEFERRED, non-blocking |

## Proposed findings

None.

The latest remediation closes the accepted process-group and async-cleanup
hypotheses at their existing owners. I found no reachable missing, incorrect,
drifting, violating, or regressing behavior that remains within this
candidate's authorized review boundary.

## Prior-finding assessment

- Closed: prior findings 1 through 12, 14, and 15, with only the
  integrator-accepted process-ownership portion retained for FIND-2.
- Excluded by explicit integrator decision: FIND-2's proposed post-report
  teardown inclusion in the report total.
- Deferred by explicit caller sequencing and not blocking this candidate:
  `FIND-TASK-008-CLOSEOUT-13`.

## Verification notes and limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`:
  14 passed, 0 failed, 4 ignored environment/process tests skipped.
- Explicit ignored task-entry proofs in one run:
  - `a_hung_setup_step_is_killed_with_its_descendants_by_the_deadline`: passed.
  - `a_hung_benchmark_run_is_killed_with_its_descendants_by_the_deadline`: passed.
  - `a_slow_replica_stop_leaves_the_runtime_free`: could not reach the tested
    stop path because this sandbox's systemd user scope failed with
    `Failed to connect to bus: Operation not permitted`. The test is explicitly
    ignored unless a delegating systemd user manager is available; its recorded
    suitable-environment PASS and the direct `spawn_blocking` source path remain
    the available proof.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E
  'test(/release_server::/)'`: 2 passed.
- `git diff --check
  f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`:
  passed.
- The full default `mise run bench:capacity` remains explicitly deferred; this
  review makes no empirical capacity or AC-040/AC-041 performance claim.

## Overall result

**PASS**

The cumulative candidate satisfies the original task at the behavior level
within the caller's explicit deferral and integrator decision. The validated
latest-round lifecycle fixes preserve the approved workload and adjacent
behavior, and this review proposes no material finding.
