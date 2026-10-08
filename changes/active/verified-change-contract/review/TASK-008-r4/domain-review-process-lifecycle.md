# Process lifecycle, durability, and concurrency domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review inputs: `review/TASK-008-r1/`, `review/TASK-008-r2/`, and `review/TASK-008-r3/`
- Remediation input: `review/TASK-008-r3/TASK-008-CLOSEOUT-R2-command-lifetime.md` and its appended implementation evidence

`HEAD` resolved to the candidate before this review. CodeGraph is not indexed
in this checkout, so source and caller tracing used Git and repository-native
search directly. Prior conclusions and implementation evidence were treated as
claims to recheck against the cumulative candidate.

## Reviewed boundary

This pass traced the complete operator-command lifetime and its partial effects:

1. `mise run bench:capacity` fixes `WYRD_CAPACITY_STARTED` and
   `WYRD_CAPACITY_DEADLINE`, then starts RustFS, the RustFS setup container, the
   repository Postgres wrapper, the release build, and the capacity binary.
2. The Postgres wrapper owns a Compose project and credentials, traps
   `EXIT`/`INT`/`TERM`, runs its child synchronously, and tears the project down
   in the trap.
3. The capacity binary converts the wall-clock deadline to a Tokio instant,
   reserves client, replica, report, wrapper, and exit time, and bounds
   preparation and measurement.
4. `LocalServer::start` runs migration, starts a release server in a systemd
   user scope, waits for readiness, and runs four tenant setup commands.
   `OperatorRun` now owns each migration/setup child, polls it cooperatively,
   stores stdout outside the filesystem, retains stderr, and kills/reaps it on
   cancellation.
5. `Benchmark::clean_up` bounds client shutdown, stops replicas newest first,
   copies logs, builds the report, and returns a failing process status when
   the verdict or lifecycle fails.

The candidate does not modify production queue, database transaction,
claim/fence, or durable publication behavior. Database rows and object-store
effects completed before cancellation remain, as now documented; the
repository Postgres wrapper is intended to remove its isolated database and
volumes on exit.

## Authority and source coverage

| Boundary | Authority and inspected source | Assessment |
|---|---|---|
| Complete 30-minute command | Revision-57 `REQ-171` (`spec.md:1706-1713`); remediation `AC-R2-1`; `mise.toml:507-549`; native GNU `timeout --help`/`timeout(1)` | **FAIL.** Foreground timeout explicitly excludes descendants; see `PROC-R4-001`. |
| Wrapper and durable teardown | Remediation `AC-R2-1`/`AC-R2-2`; `scripts/postgres/with-test-postgres.sh:11-40,52-123`; `mise.toml:530-548` | **FAIL at the shell boundary.** TERM/KILL targets the wrapper only, so its running child can survive and teardown can be interrupted. |
| In-binary absolute deadline | Remediation `AC-R2-1`/`AC-R2-3`; `capacity/main.rs:81-275,313-425,702-729` | **PASS locally.** The command start/deadline feed one `Lifetime`; preparation, measuring, client shutdown, replica reserves, elapsed reporting, and explicit cooperative-cancellation semantics share it. |
| Migration and setup children | Remediation `AC-R2-2`/`AC-R2-3`; `release_server.rs:130-207,636-732`; `capacity/main.rs:478-510` | **PASS for direct operator children.** `OperatorRun` owns, polls, kills, reaps, and preserves diagnostics without storing credential-bearing stdout on a named path. |
| Serving process/systemd scope | Remediation `AC-R2-2`; `release_server.rs:210-278,365-405,487-502`; process proof `capacity/main.rs:778-898` | **PASS for in-binary cancellation.** The focused proof exercises an already-serving replica plus a stalled setup and verifies setup/replica reaping, retained logs, no later phase, and a failed report. |
| Cleanup and report failure | Remediation `AC-R2-1`/`AC-R2-2`; `capacity/main.rs:379-425,581-613`; `report.rs`; process proof `capacity/main.rs:806-898` | **PASS when the binary remains the owner.** Finished effects remain explicit, clients are bounded, replicas stop or drop-kill, and partial measurement becomes a failed report. Shell-level forced termination can bypass this owner through `PROC-R4-001`. |
| Credentials and diagnostic retention | `AGENTS.md` secret-handling rules; `release_server.rs:102-110,130-207,636-732`; `capacity/main.rs:615-635`; `mise.toml:509` | **PASS within task scope.** Operator command strings exclude the environment, setup stdout is unnamed and deleted, API keys use `SecretString`, and checked-in AWS values are local emulator credentials. Retained stderr and server logs do not intentionally contain setup credentials. |
| Deferred qualification | Caller instruction; prior `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED, non-blocking.** No empirical capacity or 30-minute default-run qualification is inferred here. |

## Material proposed finding

### `PROC-R4-001` — INCORRECT — the shell deadline does not own or reap its process tree

- **Violated obligation:** Revision-57 REQ-171 and remediation AC-R2-1 require
  the complete default command, including setup and exit, to finish under one
  absolute 30-minute boundary. The remediation further requires work before
  the in-binary report owner exists to leave no owned child behind.
- **Exact location:** `mise.toml:511-548`, especially every
  `timeout --foreground` at lines 527, 529, 531, 542, 545, and 548;
  `scripts/postgres/with-test-postgres.sh:19-40,119-123`.
- **Evidence:** The task comment says `--foreground` keeps every step in the
  task process group. The installed native GNU Coreutils contract says the
  opposite for deadline ownership: in foreground mode, "children of COMMAND
  will not be timed out." The outer timeout therefore signals only the
  Postgres wrapper. That wrapper synchronously waits for its `bash -lc` child
  and does not explicitly forward TERM to it. Five seconds later the outer
  timeout may KILL only the wrapper while the inner shell, `cargo build`,
  `cargo run`, capacity binary, or systemd-scoped server continues. The two
  RustFS Docker commands and both build branches also omit `--kill-after`, so
  even their direct command has no enforced escalation if it does not exit on
  TERM. Docker-started setup containers can likewise outlive the killed client.
  The recorded shim dry run proves budget propagation and argument forwarding,
  not signal delivery, descendant termination, reaping, or teardown.
- **Reachable consequence:** A wedged build/run or a command that ignores or
  defers TERM can keep work alive after the advertised deadline. Killing the
  Postgres wrapper during its signal trap can strand its Compose project and
  volumes, including partially migrated or tenant-provisioned state. A
  surviving capacity binary can also leave its systemd-scoped replica and
  credential-bearing isolated database running after the operator command has
  returned or its direct wrapper has died. This violates both bounded lifetime
  and cleanup/durability attribution; it is not the deferred empirical
  throughput qualification.
- **Testable correction:** Make the existing mise command own one signalable
  process group (or explicitly tracked equivalent) for each shell phase so the
  absolute deadline reaches the wrapper and every descendant, while preserving
  the wrapper's opportunity to tear Postgres down. Add bounded escalation and
  reaping for the RustFS setup and release-build phases as well as the run
  phase. Preserve the one deadline/environment, current benchmark workload,
  in-binary `Lifetime`, `OperatorRun`, report behavior, and all production
  semantics. Prove the shell boundary with a shortened-deadline process test
  whose wrapper and nested child resist or defer TERM: the command must return
  nonzero within the bound, no descendant or systemd scope may survive, and
  wrapper-owned teardown/diagnostics must be observed. A stub that exits when
  called is insufficient.

## Positive controls

- `Lifetime::bounded` now documents cooperative cancellation, surviving
  effects, cleanup ownership, and workflow-specific retry safety, closing the
  documentation part of prior `FIND-TASK-008-CLOSEOUT-6`.
- Migration and tenant setup no longer use uninterruptible
  `Command::output()`. `OperatorRun` polls an owned child and its `Drop` kills
  and reaps it.
- Setup credentials stay in unnamed stdout storage and `SecretString`; command
  diagnostics omit environment values.
- The process-level ignored test is materially stronger than the former
  paused-clock unit test: it exercises an owned replica, a stalled setup,
  reaping, retained diagnostics, prevention of later work, and failed-report
  generation.
- `LocalServer` preserves abnormal server logs while clean shutdown copies
  normal logs into the report directory.

## Verification and proof limits

- Per orchestrator direction, this domain pass ran no Cargo or mise command
  while other review agents shared the checkout. It relied on the remediation's
  recorded evidence: capacity target 14 passed/1 skipped, the focused stalled
  setup proof passed when explicitly selected, release-server tests passed,
  and format/lints/diff-check were recorded clean.
- The stalled-setup proof begins inside `Benchmark`; it does not execute the
  mise task, RustFS commands, Postgres wrapper, nested shell, Cargo process, or
  outer timeout. No recorded evidence injects a stubborn nested shell child at
  that boundary.
- Native `timeout` documentation was inspected locally. It explicitly states
  that `--foreground` does not time out children and that `--kill-after` is
  needed for escalation.
- `FIND-TASK-008-CLOSEOUT-13`, the full unmodified default benchmark, remains
  deferred to integration by explicit caller instruction and is not a blocker
  or a substitute for the focused process-tree proof above.

## Overall result

**FAIL**

The remediation closes the in-binary cooperative-cancellation, direct
operator-child, log-retention, and partial-report paths, but the actual default
command's outer shell boundary does not own its descendants. `PROC-R4-001` is
a reachable, bounded lifecycle defect within the approved task.
