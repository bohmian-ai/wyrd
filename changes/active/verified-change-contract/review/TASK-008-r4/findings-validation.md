# TASK-008 round-four findings validation

## Immutable subject and validation scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..8022436387f3a9a9499527ebf8b8b8140c6559cb`
- Approved authority: `changes/active/verified-change-contract/spec.md`, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior reviews: `review/TASK-008-r1/`, `review/TASK-008-r2/`, and
  `review/TASK-008-r3/`
- Current remediation: `review/TASK-008-r3/TASK-008-CLOSEOUT-R2-command-lifetime.md`

I inspected the complete cumulative changed-path set and diff, the applicable
repository authority, the original task, prior stable ledgers and remediation,
and every round-four discovery and follow-up report. I then independently
traced the proposed failures through `bench:capacity`, the Postgres wrapper,
Cargo and benchmark descendants, `Lifetime`, `Benchmark`, `LocalServer`,
`OperatorRun`, `Report`, and the current task evidence. The candidate remained
at the named commit. This checkout has no `.codegraph/` directory, so source
and caller navigation used Git, `rg`, and direct source inspection.

Per the caller's explicit sequencing instruction,
`FIND-TASK-008-CLOSEOUT-13` is **DEFERRED** to integration after the other
workstreams merge. It is not a blocker or a retained candidate finding in this
ledger, and the deferral supplies no empirical AC-040/AC-041 qualification.

## Producer-to-consumer validation

`mise run bench:capacity` is the earliest owner. It fixes one Unix start and
deadline, runs two mandatory RustFS commands, enters
`with-test-postgres.sh`, builds the release server, and invokes `cargo run`.
The binary converts the same wall-clock values to one monotonic `Lifetime`,
bounds preparation and measurement, then shuts clients down, stops replicas,
constructs the report, writes it, prints it, and exits. The Postgres wrapper
tears its Compose project down only after that child exits or after its own
signal trap runs.

The command boundary is not yet enforceable over that topology. Every phase
uses GNU `timeout --foreground`. The installed native contract states that in
foreground mode children of the managed command are not timed out. The two
RustFS calls and both build branches also omit `--kill-after`, so a direct
TERM-resistant command can hold the task indefinitely. Calls with
`--kill-after` can hard-kill only the immediate wrapper or Cargo process; their
nested shell, capacity binary, and other descendants are not thereby owned.
The supplied native probe independently confirmed the first form: a
TERM-ignoring foreground command waited beyond its nominal timeout. The
in-binary stalled-setup test begins below this boundary and cannot prove it.

The same prior command-lifetime finding still owns the elapsed-report sink.
`Report::total_seconds` is sampled at `capacity/main.rs:402`, before binary
identity collection, serialization and file writes, console rendering,
capacity/Cargo exit, and Postgres teardown. R2 diagnosed incomplete elapsed
reporting and AC-R2-1 expressly requires the report's total duration to cover
the same setup-to-exit interval. Correct process-tree signaling alone would
not change this early sample, but it is not a new product invariant: both are
unclosed manifestations of prior `FIND-TASK-008-CLOSEOUT-2` and should receive
one coherent command-boundary correction.

Inside the binary, migration and setup children are now properly owned:
`OperatorRun::finish` yields while polling, and cancellation drops an owner
that kills and reaps the child and retains stderr. `Lifetime::bounded` now
documents cooperative cancellation, surviving effects, owner cleanup, and
workflow-specific retry safety. Those changes close prior
`FIND-TASK-008-CLOSEOUT-6` and the in-binary portion of prior FIND-2.

One separate async boundary remains invalid. `Benchmark::clean_up` directly
calls synchronous `LocalServer::stop` on the Tokio worker. `stop` can spend the
full 45-second `STOP_GRACE` in `std::thread::sleep`, then perform blocking
kill, wait, and log copy. This path is reached in every normal cleanup and is
independent of the shell ownership defect. The process-owner `Drop` paths are
not included in this finding: their cancellation fallback semantics need not
be redesigned to close the directly reachable normal-stop violation.

The task's current revision-57 evidence also names six focused capacity tests
with either raw `cargo nextest` or detached `-E` fragments at
`task-008-closeout.md:1501-1504`. The candidate added this current evidence and
marks it PASS. Repository authority requires every named Rust test in a task
or implementation report to carry and run its complete repository-pinned
`mise exec -- cargo nextest run --locked` command. The older
`verification_capacity` anchors at lines 725-726 explicitly say they are to be
updated after the rename and sit in the task's historical planning body; they
are excluded from this finding.

## Proposal dispositions

| Discovery proposal | Disposition | Validation |
|---|---|---|
| `BHV-R4-001` | **REVISED** into prior `FIND-TASK-008-CLOSEOUT-2` | The two pre-Postgres TERM-only calls are reachable, but they are part of the wider outer process-tree ownership defect. |
| `INV-R4-001` | **REVISED** into prior `FIND-TASK-008-CLOSEOUT-2` | Confirmed: foreground timeout excludes Cargo/wrapper descendants, while the in-binary proof begins below that producer. |
| Process-lifetime portion of `STD-R4-001` | **REVISED** into prior `FIND-TASK-008-CLOSEOUT-2` | Same outer owner and consequence as the behavior, invariant, system, capacity, and lifecycle proposals. |
| Blocking-cleanup portion of `STD-R4-001` | **REVISED** as new `FIND-TASK-008-CLOSEOUT-14` | Narrowed to the normal `Benchmark::clean_up` -> `LocalServer::stop` path. Blocking fallback `Drop` paths are not included. |
| `STD-R4-002` | **REVISED** as new `FIND-TASK-008-CLOSEOUT-15` | Historical deleted-target anchors are excluded. The candidate-added current PASS evidence at lines 1501-1504 remains noncanonical and incomplete. |
| `SYS-R4-001` | **REVISED** into prior `FIND-TASK-008-CLOSEOUT-2` | Confirmed command descendant and wrapper-teardown reachability; speculative claims about every container effect are unnecessary to establish the defect. |
| `SYS-R4-002` | **REVISED** and consolidated into prior `FIND-TASK-008-CLOSEOUT-2` | The early `total_seconds` sink is independently reachable after process ownership is repaired, but incomplete elapsed reporting was already part of FIND-2's diagnosis and R2 correction. It does not earn a new stable ID. |
| `CAP-R4-001` | **REVISED** into prior `FIND-TASK-008-CLOSEOUT-2` | The missing forced escalation is confirmed. Its `set -e` subclaim is **REJECTED**: mise runs inline Unix tasks with `sh -o errexit -c`, so an early nonzero setup command already stops the outer task. |
| `PROC-R4-001` | **REVISED** into prior `FIND-TASK-008-CLOSEOUT-2` | Confirmed at the shell/process boundary and deduplicated with the other lifetime proposals. |
| Maintainer report's empty proposal set | **CONFIRMED as empty for its lens** | Its positive conclusions about `Benchmark`, `Lifetime`, `LocalServer`, `OperatorRun`, types, and documentation stand; they do not disprove the separately governed process and async-runtime failures. |
| Follow-up `total_seconds` proposal | **REVISED** into prior `FIND-TASK-008-CLOSEOUT-2` | Retained, but not as a separate finding because the prior stable finding and its remediation already include elapsed-report fidelity. |
| Follow-up async blocking-stop proposal | **CONFIRMED** as new `FIND-TASK-008-CLOSEOUT-14` | The direct normal cleanup call blocks a Tokio worker and remains after outer process-tree repair. |
| Follow-up exact-command proposal | **CONFIRMED in narrowed form** as new `FIND-TASK-008-CLOSEOUT-15` | Only the current revision-57 implementation record is material. |

## Ponytail correction analysis

For `FIND-TASK-008-CLOSEOUT-2`, deletion is unavailable because REQ-171 and
R2 AC-R2-1/2 explicitly require one complete setup-to-exit deadline and
matching elapsed evidence. Existing `Lifetime`, `Benchmark`, `LocalServer`,
`OperatorRun`, report failure, and wrapper teardown owners should remain; a
new supervisor or public timeout surface would duplicate them. Native GNU
timeout/process-group behavior is already installed and is the minimum outer
mechanism. The command boundary must apply one finite TERM-to-KILL path to
each phase's complete descendant group, while leaving the existing in-binary
owners responsible for orderly diagnostics and cleanup. The same command
owner is the only place that can know the end of the required interval, so it
must finalize durable elapsed evidence after command-owned report and wrapper
teardown rather than continuing to label the binary's earlier sample as the
setup-to-exit total. This is one correction to the prior invariant, not a
second timeout abstraction or report schema redesign.

For `FIND-TASK-008-CLOSEOUT-14`, deleting normal replica stop is impossible:
the benchmark must retain orderly shutdown evidence and logs. `LocalServer`
already owns the synchronous stop operation, and Tokio already supplies the
installed blocking boundary. The minimum correction keeps stop order,
`STOP_GRACE`, kill/reap behavior, logs, and result shape, but runs each owned
synchronous stop through `tokio::task::spawn_blocking` (or the repository's
equivalent existing blocking boundary) instead of on the async worker. No
async `LocalServer` redesign, trait, channel, or general process supervisor is
justified.

For `FIND-TASK-008-CLOSEOUT-15`, no code or test change is needed. The exact
tests and one `capacity` target already exist. The minimum correction is to
replace each raw or detached current evidence entry with its own complete
`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E
'test(=<exact-name>)'` command and retain the actual result. The historical
planning body need not be rewritten to close the current evidence defect.

## Final deduplicated finding ledger

### `FIND-TASK-008-CLOSEOUT-2` — REVISED — INCORRECT

- **Discovery source IDs:** `BHV-R4-001`, `INV-R4-001`, process-lifetime
  portion of `STD-R4-001`, `SYS-R4-001`, `SYS-R4-002`, `CAP-R4-001`, and
  `PROC-R4-001`; follow-up uncertainty 1.
- **Violated obligation:** Revision-57 REQ-171 and R2 AC-R2-1/2 require the
  complete default command to finish inside one enforceable absolute
  setup-to-exit boundary, leave no command-owned child after expiry, stop
  later work, preserve attributable cleanup/diagnostics, and report total
  duration over that same interval.
- **Exact location:** `mise.toml:511-548`;
  `scripts/postgres/with-test-postgres.sh:19-40,119-123`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:392-424,702-728`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/report.rs:371-383`.
- **Evidence:** GNU `timeout --foreground` does not time out children. The
  RustFS/build phases lack kill escalation, and wrapper/Cargo kill escalation
  reaches only the immediate process, so nested command work may survive or
  hold the command past its deadline. The independent native probe confirmed
  that TERM-only foreground timeout waited past its nominal limit for
  TERM-ignoring work. Separately, `total_seconds` is sampled before report
  serialization/write, process exit, and Postgres teardown, so the durable
  report cannot describe the required setup-to-exit interval. The current
  stalled-setup test starts inside `Benchmark` and proves neither outer
  behavior.
- **Observable consequence:** A wedged or TERM-resistant setup/build/run
  process can outlive the advertised 30-minute command, retain nested work,
  bypass orderly report/wrapper cleanup, or allow the command to return while
  a descendant remains. Even after signaling is corrected, slow final report
  or wrapper teardown is absent from the value presented as total command
  duration.
- **Decision-complete minimum correction:** Retain the single exported
  deadline and existing in-binary lifecycle owners. At `bench:capacity`, use
  native process-group ownership with a finite TERM-to-KILL escalation for
  every RustFS, build, wrapper, and run phase so each phase's descendants—not
  only its immediate command—are stopped and reaped within the remaining
  absolute budget. Preserve the Postgres wrapper's opportunity to perform its
  teardown and the binary's `Lifetime`/`OperatorRun`/failed-report behavior.
  Make the same outer command owner finalize the durable total elapsed value
  only after all command-owned report and wrapper teardown work is complete;
  do not claim the existing pre-write sample is setup-to-exit duration. Keep
  the report schema, workload, SLOs, public surfaces, production behavior, and
  one deadline; add no dependency, supervisor framework, retry/grace knob, or
  public timeout option.
- **Focused closure proof:** Drive the actual task script with PATH-controlled
  stand-ins and a shortened absolute deadline in at least the pre-wrapper and
  nested run positions. A descendant that ignores TERM must be killed, gone
  after the task exits nonzero within the bound, and must prevent later
  phases; wrapper teardown and diagnostics must still be observed. Delay
  report finalization or wrapper teardown in the same command-boundary proof
  and show that the durable total elapsed evidence includes that tail. Retain
  the in-binary stalled-tenant-setup proof and the complete `capacity` target.
  A successful-command shim or the deferred full benchmark is not a substitute.

### `FIND-TASK-008-CLOSEOUT-14` — CONFIRMED — VIOLATION

- **Discovery source IDs:** blocking-cleanup portion of `STD-R4-001`;
  follow-up uncertainty 3.
- **Violated obligation:** `AGENTS.md` section 6 and
  `architecture/references/languages/rust-core.md:464-477` prohibit blocking
  a Tokio worker and require an explicit blocking strategy for synchronous
  work inside an async runtime.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:581-613` and
  `crates/wyrd/wyrd-testing/src/release_server.rs:365-405`.
- **Evidence:** Async `Benchmark::clean_up` directly invokes
  `LocalServer::stop`. That synchronous method can poll with
  `std::thread::sleep` for the full 45-second grace, then performs blocking
  kill, wait, and filesystem copy. It is the ordinary cleanup path for every
  replica, not an exceptional destructor fallback.
- **Observable consequence:** Normal cleanup monopolizes a Tokio worker for
  up to one stop grace per replica. Runtime timers, diagnostics, and other
  tasks sharing that worker cannot progress while the benchmark is supposedly
  executing an async, deadline-sensitive cleanup workflow.
- **Decision-complete minimum correction:** Keep `Benchmark` as cleanup owner
  and `LocalServer` as process owner. Move each existing synchronous
  `LocalServer::stop` operation through Tokio's installed blocking boundary,
  await its result, and preserve newest-first order, `STOP_GRACE`, kill/reap,
  log copy, shutdown evidence, and error strings. Do not convert the process
  harness to an async framework or broaden this correction to fallback
  destructors.
- **Focused closure proof:** Use the existing local-server stand-in facilities
  to exercise a delayed normal stop while a Tokio timer/heartbeat on the async
  runtime continues to advance, then assert the same stop result, reaping, and
  log evidence. Run the exact focused proof, the existing stalled-setup proof,
  and the complete `capacity` target.

### `FIND-TASK-008-CLOSEOUT-15` — CONFIRMED — VIOLATION

- **Discovery source IDs:** `STD-R4-002`; follow-up uncertainty 2.
- **Violated obligation:** `AGENTS.md` section 11,
  `architecture/agent-rules.md`, and the spec-driven testing authority require
  each named Rust test in a task artifact or implementation report to carry
  and run its complete repository-pinned `mise exec -- cargo nextest run`
  command with explicit package, target, applicable features, and exact test
  expression.
- **Exact location:**
  `changes/active/verified-change-contract/tasks/task-008-closeout.md:1498-1504`.
- **Evidence:** The current revision-57 PASS matrix records raw `cargo
  nextest` for `mix_offers_the_required_rates` and
  `queries_read_the_last_five_minutes`, then records only detached `-E`
  fragments for four report/evidence tests. Those entries are not standalone
  repository-pinned commands and can be copied without selecting the intended
  target or test.
- **Observable consequence:** The active task packet cannot reproduce the
  focused proof it claims. A maintainer cannot tell from each evidence cell
  which repository toolchain, package, target, or exact selector produced the
  PASS result.
- **Decision-complete minimum correction:** Change only the current
  revision-57 evidence rows. Give each of the six named tests its complete
  `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E
  'test(=<exact-name>)'` command and record its result. Confirm names from the
  pinned nextest listing. Do not edit tests, production code, the historical
  planning body, or replace focused evidence with only the whole-target run.
- **Focused closure proof:** Run and record the six complete exact commands
  for `load::tests::{mix_offers_the_required_rates,queries_read_the_last_five_minutes}`,
  `report::tests::{every_slo_failure_fails_the_step,verdict_needs_every_verdict_step}`,
  and `evidence::tests::{quantile_reads_bucket_deltas,raw_percentiles_use_nearest_rank}`;
  then retain the already-green complete `capacity` target result.

## Prior-finding closure

| Prior finding | Validation result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-1` | CLOSED: only `bench:capacity` remains as a server-capacity entry point. |
| `FIND-TASK-008-CLOSEOUT-2` | OPEN, REVISED: command start propagation and direct operator-child ownership are corrected, but outer foreground timeouts exclude descendants and the report still closes its elapsed interval before the required command tail. |
| `FIND-TASK-008-CLOSEOUT-3` | CLOSED: judge-provider wait remains separate from engine overhead. |
| `FIND-TASK-008-CLOSEOUT-4` | CLOSED: the task and approved specification both identify revision 57. |
| `FIND-TASK-008-CLOSEOUT-5` | CLOSED: `Benchmark` and `Deployment` retain the cohesive lifecycle and reconnection owners. |
| `FIND-TASK-008-CLOSEOUT-6` | CLOSED: `Lifetime::bounded` now documents cooperative cancellation, surviving effects, cleanup ownership, and workflow-specific retry safety. |
| `FIND-TASK-008-CLOSEOUT-7` through `FIND-TASK-008-CLOSEOUT-12` | CLOSED: typed step identity, complete Scribe backlog, exact drain edge, one report table, common resource interval, and test-owned duplicate-claim correctness remain intact. |
| `FIND-TASK-008-CLOSEOUT-13` | DEFERRED by explicit caller sequencing to post-merge integration; non-blocking for this candidate and not empirical acceptance evidence. |

## Verification assessment

- The independently available focused command
  `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`
  passed 14 tests with one environment-gated test skipped.
- The native GNU timeout probe confirmed that a TERM-only foreground timeout
  waited beyond its nominal limit for TERM-ignoring work.
- Recorded R2 evidence also includes the explicitly selected stalled-setup
  process proof, `release_server` unit tests, format, lints, and diff check.
- No available proof exercises expiry through the actual outer
  mise/wrapper/Cargo descendant topology, a delayed setup-to-exit report tail,
  or event-loop progress during normal synchronous replica stop.
- The full unmodified default benchmark remains expressly deferred under
  `FIND-TASK-008-CLOSEOUT-13` and is neither a candidate blocker nor proof for
  the focused defects above.

## Validation result

The final validated ledger contains three bounded implementation findings:
continued `FIND-TASK-008-CLOSEOUT-2`, new
`FIND-TASK-008-CLOSEOUT-14`, and new
`FIND-TASK-008-CLOSEOUT-15`. Their corrections remain within the approved
private benchmark, process-harness, and active task-evidence boundaries. None
requires a new public API, product behavior, production concurrency contract,
security decision, persistent-data decision, or specification revision.

**Validated result: FIX_REQUIRED.**
