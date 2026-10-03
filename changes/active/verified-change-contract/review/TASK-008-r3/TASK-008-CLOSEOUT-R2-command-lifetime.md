---
id: TASK-008-CLOSEOUT-R2
kind: remediation
status: ready
spec: changes/active/verified-change-contract/spec.md
spec_revision: 57
original_task: changes/active/verified-change-contract/tasks/task-008-closeout.md
base: f6159606c5c959e8fcc3423574ab0e7e6c86ee13
reviewed_candidate: 5c3bb79b3598abd88a3a234611fc400096adc975
requirements: [REQ-171]
parent_task: TASK-008-CLOSEOUT
remediates: [FIND-TASK-008-CLOSEOUT-2, FIND-TASK-008-CLOSEOUT-6]
route_to: wyrd-implement
---

# Enforce the complete capacity-command lifetime

## Outcome

Make the existing `mise run bench:capacity` command actually finish within
one absolute 30-minute setup-to-exit boundary, including command-owned setup,
benchmark preparation, migration and tenant setup children, measurement,
client and replica cleanup, diagnostic/report finalization, and process exit.
Document the cancellation and partial-progress contract on the operation that
enforces that boundary.

This is a bounded correction of candidate
`f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`
under approved specification revision 57. It does not change the benchmark's
workload, SLOs, production server behavior, or public APIs.

## Issue diagnosis

### FIND-TASK-008-CLOSEOUT-2 — the command-wide deadline is not enforceable

REQ-171 requires the default `mise run bench:capacity` command, including
setup, to complete within 30 minutes. The current timer is created in
`Benchmark::prepare`, after the mise task has already started RustFS, run its
setup container, entered the Postgres wrapper, and built release
`wyrd-server`. The binary's `main` then awaits all of `Benchmark::prepare`
outside `Lifetime::measure`.

The mandatory first-replica path calls `LocalServer::start`, whose migration
and four tenant setup commands use synchronous `Command::output()`. If one of
those children stalls, the async future does not yield, so Tokio cannot poll
the timer, drop the work, enter cleanup, reap that child, or write the failed
report. Replica stopping and report finalization are also performed after the
currently enforced measuring/client deadlines. `Report::total_seconds` begins
inside the binary, so it omits pre-binary command setup even when the outer
command eventually succeeds.

The paused-clock test passes a cooperative pending future directly to
`Lifetime`. It proves the timer arithmetic but not the reachable process path.
A slow wrapper/build or wedged migration/setup child can therefore keep the
operator command alive beyond the approved ceiling, leave owned processes
running until the child returns, omit the failed report, and publish an
incomplete elapsed duration.

### FIND-TASK-008-CLOSEOUT-6 — the cancellation owner is undocumented

`Lifetime::bounded` directly owns `tokio::time::timeout_at` and drops the
supplied workflow on expiry. Its rustdoc does not say that cancellation is
cooperative, that already-produced effects remain, which owners perform
cleanup, or that retry safety depends on the supplied workflow. Caller-specific
comments do not define the shared operation's contract. A maintainer can
therefore infer false rollback or atomicity at the exact boundary introduced
to enforce the benchmark lifetime.

## Intended correction outcome

One existing lifecycle boundary owns an absolute wall-clock deadline that
begins before any work performed by the default capacity command and governs
the complete process tree through final diagnostics, report, and exit. Every
operator child created for migration or tenant setup has an owned handle and
can be terminated and reaped when the remaining budget expires. Expiry returns
nonzero, starts no later benchmark work, retains useful stdout/stderr and
server logs, and uses the existing failed-report path whenever its report
owner has been initialized.

The final async/process owner documents cooperative cancellation, retained
partial effects, cleanup ownership, and workflow-specific retry meaning.

## Decision-complete recommendation

Retain `Benchmark` as the in-binary setup-to-report owner and `LocalServer` as
the server/process-harness owner. Establish the single absolute deadline at
the default command boundary before RustFS/Postgres/build/setup work begins,
then carry its remaining budget into the binary and through preparation,
measurement, client shutdown, replica stop, log/report finalization, and
exit. Use the repository's existing command/task environment to propagate the
deadline; do not introduce a public benchmark timeout option.

At `LocalServer`, replace the blocking migration/setup waits with owned child
lifecycles using standard process primitives already present in the harness.
The owner must retain each child handle, collect its stdout/stderr, observe the
same remaining absolute deadline, and terminate and reap the child on expiry.
For tenant setup, a serving replica may already exist; preserve its current
`Drop`/stop ownership so expiry also terminates and reaps the replica and
retains its log. Do not hide the issue behind another downstream duration
check: the invalid state is the unowned, uninterruptible child wait and must be
prevented there.

Once `Benchmark` exists, route preparation, setup, measurement, cleanup, or
deadline failure through its existing failed-report behavior so partial
records and diagnostics remain attributable. Work that fails or expires
before that owner exists must still stop the complete command nonzero within
the same deadline and leave no owned child behind. The reported elapsed value
must describe the same command-wide interval claimed by REQ-171.

On the operation that ultimately owns cooperative async cancellation—retain
`Lifetime::bounded` if it still owns that behavior, otherwise document its
direct replacement—state that expiry drops the supplied future at its next
cooperative yield, already-produced effects remain according to their owner,
owned process/client cleanup is performed by those owners, and retry safety is
specific to the interrupted workflow. Preserve the more specific cancellation
contracts on callers.

This correction uses existing benchmark, process, cleanup, reporting, and
standard-library mechanisms. It needs no new dependency, supervisor framework,
trait, generic layer, second timeout abstraction, or production runtime
change.

## Constraints and preserved behavior

- Preserve the approved four-tenant workload, operation rates, reference
  fixtures, warmup/ramp/sustained/scale-out sequence, knee rule, SLO thresholds,
  report schema, and three verdict steps.
- Preserve release `wyrd-server` replicas in peer mode over repository-managed
  Postgres and RustFS under the existing 8-CPU/16-GiB envelope.
- Preserve `Benchmark`, `Deployment`, `LocalServer`, client shutdown, replica
  stop/kill/reap, abnormal log retention, and failed-report ownership unless a
  minimal local adjustment is required to pass the one deadline through them.
- Preserve production request, provider, readiness, queue, Scribe, Oracle,
  Forge, audit, and verification timeouts and semantics.
- Preserve the existing focused fairness, exactly-once, judgment, queue, and
  capacity tests. Do not weaken, delete, ignore, or replace them.
- Keep `FIND-TASK-008-CLOSEOUT-13` deferred to the integration sequence named
  by the caller. This task makes the default run trustworthy; it does not run
  or claim that performance qualification.

## Non-goals

- No public API, CLI option, configuration setting, Card/schema, storage
  format, or production deployment decision.
- No new benchmark entry point, workload, SLO, retry/grace knob, or capacity
  claim.
- No production server timeout or request-cancellation change.
- No general-purpose subprocess supervisor or refactor of unrelated
  `wyrd-testing` fixtures.
- No execution of the deferred full default benchmark in this remediation.

## Acceptance criteria

### AC-R2-1 — one absolute setup-to-exit boundary

`mise run bench:capacity` starts one absolute deadline before its first
command-owned setup action. Every subsequent phase consumes the remaining
budget, and command exit, including bounded cleanup and diagnostic/report
finalization, occurs within that boundary. The report's total duration covers
the same interval.

Closes `FIND-TASK-008-CLOSEOUT-2`.

### AC-R2-2 — stalled operator children are owned and reaped

A migration or tenant-setup child that does not exit cannot block timer
progress. On expiry the child is terminated and reaped, no later setup or
measurement starts, any already-started replica and client owners clean up,
diagnostics are retained, and the command returns nonzero. When the benchmark
report owner exists, the report records the early stop and fails.

Closes `FIND-TASK-008-CLOSEOUT-2`.

### AC-R2-3 — cancellation and partial progress are explicit

Every materially changed async/process boundary in the final lifetime path
documents its errors and applicable cancellation, surviving effects, cleanup,
retry, and partial-progress behavior. The shared cancellation owner or its
direct replacement explicitly describes cooperative future drop and delegates
effect durability and retry safety to the supplied workflow's owner.

Closes `FIND-TASK-008-CLOSEOUT-6`.

### AC-R2-4 — adjacent benchmark behavior is unchanged

The capacity target, report/verdict tests, existing process cleanup tests, and
focused real-server correctness/durability proofs retain their behavior. No
production timeout, workload, SLO, report column, public surface, or deferred
performance claim changes.

Closes both findings without scope drift.

## Focused proof and broader verification

Add a process-level shortened-deadline proof using a controlled migration or
tenant-setup child that never exits. Exercise the tenant-setup form with a
serving replica already owned. Prove all of the following directly:

- the complete command returns nonzero within the shortened absolute bound;
- the stalled operator child is terminated and reaped;
- the owned replica is terminated and reaped and its diagnostic log is kept;
- later setup and measurement do not start;
- stdout/stderr or equivalent failure diagnostics are retained; and
- a failed report is written when the report owner has been initialized.

Keep the cooperative lifetime unit proof as supporting coverage and run every
new specifically named Rust test with the exact repository-pinned
`mise exec -- cargo nextest run --locked -p wyrd-testing ... -E
'test(=<exact-name>)'` command appropriate to its target. Confirm the exact
test name with `cargo nextest list`; include the repository-managed environment
wrapper if the proof uses Postgres or a real server.

Then run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity
mise run fmt
mise run lints
git diff --check
```

Run the narrowest repository-managed process or journey lane that owns any
new real-server proof. Do not substitute the deferred unmodified
`mise run bench:capacity` execution for the stalled-child proof, and do not
claim empirical AC-040/AC-041 qualification in this remediation.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-R2-1 one absolute setup-to-exit boundary | `mise.toml` `bench:capacity` fixes `WYRD_CAPACITY_STARTED`/`WYRD_CAPACITY_DEADLINE` (start + 30 min) before the first `docker compose` and gives every step (`rustfs`, `rustfs-setup`, Postgres wrapper, release build, `cargo run`) only the remaining budget through `timeout --foreground`; the binary's `Lifetime::from_command` (`capacity/main.rs`) reads the same deadline, bounds `Benchmark::prepare`, measuring, client shutdown, and reserves replica stops plus a 60 s `EXIT_RESERVE` for the report, wrapper teardown, and exit; `total_seconds`/`setup_seconds` count from the command start | Shim dry run of the task script (stubbed `docker`/`cargo`): every step receives the exported deadline and the arguments still reach the binary; `budget` refuses an exhausted budget with exit 124; `tests::an_unfinished_benchmark_fails_and_cleans_up_by_its_deadline` | PASS |
| AC-R2-2 stalled operator children are owned and reaped | `OperatorRun` (`release_server.rs`) replaces blocking `Command::output` for `migrate` and each `setup`: stdout to an unlinked file, stderr to a kept file, `finish` polls `try_wait` and yields, `Drop` kills, reaps, and prints stderr; the already-serving replica keeps its `LocalServer` `Drop` (kill, reap, keep root and `server.log`) | `tests::a_stalled_tenant_setup_stops_the_run_by_its_deadline` (stand-in server in a real systemd user scope, `setup` never exits, 12 s deadline): run returns a failed verdict before the deadline, `setup` and replica pids are reaped, only `t0` setup started, no step record, `server.log` and `setup-t0.stderr` kept, `report.json` fails with `measuring passed its share`. Red check: with a blocking child wait substituted, the same test hung until nextest's SIGTERM at 81.6 s | PASS |
| AC-R2-3 cancellation and partial progress explicit | `Lifetime::bounded` documents cooperative drop at the next yield, non-interruption of non-yielding work, no rollback of produced effects, owner `Drop` cleanup, workflow-specific retry; `LocalServer::start` and `OperatorRun::finish` document their cancellation; caller contracts on `measure`/`shut_down` retained | `mise run lints` (clippy `-D warnings`, all features/targets) | PASS |
| AC-R2-4 adjacent behavior unchanged | No workload, SLO, report column, production timeout, CLI option, or public surface changed; report field docs only clarify the start point | `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`: 14 passed, 1 skipped (the ignored process proof); `release_server::tests`: 2 passed | PASS |

Commands:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=only \
  -E 'test(=tests::a_stalled_tenant_setup_stops_the_run_by_its_deadline)'   # 1 passed, 10.0 s
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity \
  -E 'test(=tests::an_unfinished_benchmark_fails_and_cleans_up_by_its_deadline)'   # 1 passed
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity   # 14 passed, 1 skipped
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/release_server::/)'   # 2 passed
mise run fmt; mise run lints; git diff --check   # clean
```

Limits: the stalled-setup proof is `#[ignore]`d because it needs a
delegating systemd user manager, `python3`, and port 8080; it uses no
Postgres or real server, so no wrapper applies. It drives `Benchmark` from
`prepare` through the written report; `main`'s mapping of a failed verdict to
a nonzero exit is unchanged. The shell budget was proven by a stubbed dry run,
not a full command. `FIND-TASK-008-CLOSEOUT-13` (the unmodified default run)
stays deferred to integration; no AC-040/AC-041 capacity claim is made.
Non-goals held: no CLI option, cap, synthetic load, dependency, or production
change; only `mise.toml` and `crates/wyrd/wyrd-testing` changed.
