---
id: TASK-008-CLOSEOUT-R3
kind: remediation
status: ready
spec: changes/active/verified-change-contract/spec.md
spec_revision: 57
original_task: changes/active/verified-change-contract/tasks/task-008-closeout.md
base: f6159606c5c959e8fcc3423574ab0e7e6c86ee13
reviewed_candidate: 8022436387f3a9a9499527ebf8b8b8140c6559cb
requirements: [REQ-171]
parent_task: TASK-008-CLOSEOUT
remediates: [FIND-TASK-008-CLOSEOUT-2, FIND-TASK-008-CLOSEOUT-14, FIND-TASK-008-CLOSEOUT-15]
route_to: wyrd-implement
---

# Close the capacity command process boundary and proof

## Outcome

Finish the existing `bench:capacity` lifetime correction so one absolute
deadline owns the complete command process tree and its durable elapsed
evidence, ordinary replica cleanup does not block a Tokio worker, and the
current revision-57 task record contains reproducible exact commands for every
named capacity test.

This is a bounded correction of candidate
`f6159606c5c959e8fcc3423574ab0e7e6c86ee13..8022436387f3a9a9499527ebf8b8b8140c6559cb`
under approved specification revision 57. It does not change the benchmark's
workload, SLOs, production server behavior, public APIs, or the caller-deferred
full default qualification run.

## Issue diagnosis

### FIND-TASK-008-CLOSEOUT-2 — the outer command still does not own its process tree or complete elapsed interval

REQ-171 and remediation R2 require the default command, including setup,
cleanup, diagnostics, report finalization, wrapper teardown, and exit, to live
inside one enforceable 30-minute boundary and to report elapsed time over that
same interval.

The candidate correctly fixes one start/deadline before RustFS setup, carries
it into the binary, bounds cooperative preparation and measurement, and owns
direct migration/setup children through `OperatorRun`. The remaining defect is
upstream in `mise.toml`: every phase uses GNU `timeout --foreground`, whose
native contract explicitly excludes children of the managed command. The
RustFS and build phases also have no forced-kill escalation. The timeouts that
do use `--kill-after` can force only their immediate wrapper or Cargo process,
not the nested shell, capacity binary, compiler, or other descendants. A
TERM-resistant descendant can therefore hold the command past the deadline or
survive after the task returns. The existing stalled-setup proof constructs
`Benchmark` directly and never exercises this outer topology.

The durable elapsed value is also incomplete. `Report::total_seconds` is
sampled before binary identity collection, report serialization and writes,
console rendering, capacity/Cargo exit, and Postgres teardown. It is labelled
as setup-to-exit duration even though the command tail has not happened. Slow
or failed finalization is therefore absent from the evidence R2 required.

### FIND-TASK-008-CLOSEOUT-14 — normal replica cleanup blocks the async runtime

`Benchmark::clean_up` is async because it first awaits client shutdown, but it
then invokes synchronous `LocalServer::stop` directly on the Tokio worker for
each replica. `stop` may spend the full 45-second grace in
`std::thread::sleep`, followed by blocking kill, wait, and filesystem log copy.
This is the normal cleanup path, not merely an exceptional destructor fallback.

The outer process-tree fix does not close this violation. Even with correct
shell supervision, ordinary in-binary cleanup can monopolize an async worker
and prevent timers, diagnostics, or sibling tasks on that worker from making
progress during a deadline-sensitive shutdown.

### FIND-TASK-008-CLOSEOUT-15 — current focused-test evidence is not reproducible

The current revision-57 implementation matrix in
`task-008-closeout.md:1498-1504` names six capacity tests while recording raw
`cargo nextest` commands or detached `-E` fragments. Repository authority
requires each named Rust test in a task or implementation report to carry and
run one complete repository-pinned `mise exec -- cargo nextest run --locked`
command with the package, target, applicable features, and exact selector.

The historical planning body's deleted `verification_capacity` anchors are
not part of this finding. The defect is limited to the candidate-added current
revision-57 PASS evidence, which a maintainer cannot copy to reproduce the
claimed focused result.

## Intended correction outcome

Every mandatory RustFS, build, wrapper, and run phase is supervised as a
complete descendant group under the existing absolute deadline. Expiry gives
orderly owners their existing cleanup opportunity, then forcibly terminates
and reaps any surviving descendants within the remaining bound. No later phase
starts after expiry, diagnostics remain attributable, and no command-owned
process remains after the task returns.

The command's durable elapsed evidence closes only after command-owned report
finalization and Postgres teardown, so it describes the same setup-to-exit
interval claimed by REQ-171. Inside the binary, normal replica stops preserve
their existing order and semantics while running off the Tokio worker. The
active task record gives each named focused test a complete exact command and
actual result.

## Decision-complete recommendation

Retain the single `WYRD_CAPACITY_STARTED`/`WYRD_CAPACITY_DEADLINE` values,
`Benchmark`, `Lifetime`, `LocalServer`, `OperatorRun`, the existing Postgres
wrapper, and the current failed-report path.

At the `bench:capacity` command boundary, replace immediate-process-only
foreground timeout behavior with native process-group supervision for every
RustFS, build, Postgres-wrapper, and benchmark-run phase. Each phase must use
the same remaining absolute budget and one finite TERM-to-KILL escalation that
reaches all descendants. Preserve interactive signal forwarding and let the
Postgres wrapper perform its existing teardown before forced escalation; do
not add a second deadline, public option, dependency, supervisor framework, or
retry/grace configuration surface.

The outer command owner is the only layer that observes completion of report
work, child exit, and wrapper teardown. Have that owner finalize the durable
total elapsed evidence after those events. Preserve the report schema and the
binary's partial/failure report; do not continue to label the binary's
pre-write sample as setup-to-exit duration. A small outer finalization of the
existing report artifact is preferable to moving command-process ownership
into production or duplicating the benchmark report model.

Keep `LocalServer::stop` as the synchronous process owner. In
`Benchmark::clean_up`, run each existing stop/reap/log-copy operation through
Tokio's installed blocking boundary and await it in the current newest-first
order. Preserve `STOP_GRACE`, forced kill/reap, log destinations, ordinal
ordering, result values, and error strings. Do not redesign `LocalServer` as an
async process framework or broaden the correction to fallback `Drop` paths.

Finally, change only the current revision-57 implementation evidence in the
task. Replace each raw or detached named-test entry with its complete exact
`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E
'test(=<exact-name>)'` command and record its actual result. Confirm exact
names from nextest. Leave the historical planning prose and tests themselves
unchanged.

## Constraints and preserved behavior

- Preserve the four-tenant workload, direct/queued five-kind mix, ingest and
  query rates, warmup/ramp/sustained/scale-out sequence, knee rule, SLOs,
  report cells, verdict steps, and profiling behavior.
- Preserve release `wyrd-server` replicas in peer mode over repository-managed
  Postgres and RustFS under the existing 8-CPU/16-GiB envelope.
- Preserve one absolute 30-minute command limit, the existing client-shutdown
  and replica-stop reservations, `Benchmark`, `Deployment`, `LocalServer`,
  `OperatorRun`, cleanup order, diagnostics, and failed-report ownership.
- Preserve production request, provider, readiness, queue, Scribe, Oracle,
  Forge, audit, and verification timeout and durability semantics.
- Preserve all focused fairness, exactly-once, judgment, queue, report, and
  capacity tests. Do not weaken, delete, ignore, or replace them.
- Keep `FIND-TASK-008-CLOSEOUT-13` deferred to the integration sequence named
  by the caller. This remediation makes the command trustworthy; it does not
  run or claim the full performance qualification.

## Non-goals

- No public API, CLI option, configuration setting, Card/schema, storage
  format, production deployment, or concurrency-policy decision.
- No new benchmark entry point, workload, SLO, report column, retry setting,
  grace setting, or capacity claim.
- No production server timeout or request-cancellation change.
- No general-purpose subprocess supervisor or unrelated release-harness
  refactor.
- No rewrite of historical task planning prose.
- No execution of the deferred unmodified `mise run bench:capacity` run.

## Acceptance criteria

### AC-R3-1 — the absolute deadline owns every descendant

Every mandatory phase of `mise run bench:capacity` supervises its complete
descendant group under the one exported absolute deadline. A descendant that
ignores the orderly termination signal is forcibly terminated and reaped
inside the remaining budget; no later phase starts, wrapper teardown and
diagnostics retain their current opportunity, the task exits nonzero, and no
command-owned descendant survives.

Closes the process-ownership portion of `FIND-TASK-008-CLOSEOUT-2`.

### AC-R3-2 — durable elapsed evidence covers the command tail

The retained report artifact records total elapsed time only after report
finalization, child exit, and Postgres wrapper teardown, covering the same
setup-to-exit interval governed by the absolute deadline. Failure or delay in
that tail is represented without presenting the earlier binary sample as the
complete duration.

Closes the elapsed-evidence portion of `FIND-TASK-008-CLOSEOUT-2`.

### AC-R3-3 — ordinary replica stop does not block Tokio

Normal cleanup runs each existing synchronous replica stop through an explicit
blocking boundary while preserving newest-first shutdown, per-replica grace,
kill/reap, log copy, diagnostics, result ordering, and errors. A concurrent
Tokio timer or heartbeat continues while a delayed stop is in progress.

Closes `FIND-TASK-008-CLOSEOUT-14`.

### AC-R3-4 — current focused evidence is exact and reproducible

Each of the six named capacity tests in the current revision-57 implementation
matrix has its own complete pinned `mise exec -- cargo nextest run --locked`
command with explicit package, binary target, and exact test expression, and
the recorded result matches execution.

Closes `FIND-TASK-008-CLOSEOUT-15`.

### AC-R3-5 — adjacent behavior is unchanged

The complete capacity target, in-binary stalled-setup proof, release-server
tests, benchmark workload/report/verdict behavior, process cleanup semantics,
and focused real-server correctness/durability proofs retain their behavior.
No production timeout, public surface, workload, SLO, or deferred performance
claim changes.

## Focused proof and broader verification

Add or extend one task-entry process proof with PATH-controlled stand-ins and
a shortened absolute deadline. Exercise at least a pre-wrapper phase and a
nested run phase whose descendant ignores TERM. Prove bounded nonzero task
completion, descendant termination/reaping, no later phase, preserved wrapper
teardown/diagnostics, and no surviving command-owned process. Delay report
finalization or wrapper teardown in this proof and assert the durable total
elapsed evidence includes that tail. A successful shim dry run or direct call
to `Benchmark` is not equivalent proof.

Add a focused normal-cleanup proof using the existing local-server stand-in:
delay one replica stop, run a Tokio timer/heartbeat concurrently, and prove the
timer progresses while stop retains the same reaping, log, ordering, and result
semantics.

Run and record complete exact commands for these six tests:

- `load::tests::mix_offers_the_required_rates`
- `load::tests::queries_read_the_last_five_minutes`
- `report::tests::every_slo_failure_fails_the_step`
- `report::tests::verdict_needs_every_verdict_step`
- `evidence::tests::quantile_reads_bucket_deltas`
- `evidence::tests::raw_percentiles_use_nearest_rank`

Every new specifically named Rust proof must likewise use its exact
repository-pinned `mise exec -- cargo nextest run --locked -p wyrd-testing ...
-E 'test(=<exact-name>)'` command with the correct target and any required
environment wrapper. Confirm names with `mise exec -- cargo nextest list`.

Then run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/release_server::/)'
mise run fmt
mise run lints
git diff --check
```

Run the narrowest repository-managed process lane required by the new
task-entry proof. Do not substitute the deferred unmodified capacity benchmark
for these focused failure-path proofs, and do not claim empirical AC-040 or
AC-041 qualification in this remediation.

## Implementation evidence

Integrator validation: FIND-2 accepted in part (process-group ownership);
FIND-14 and FIND-15 accepted; FIND-13 stays deferred to integration.

**Rejected part of FIND-TASK-008-CLOSEOUT-2 (AC-R3-2).** The integrator
rejected extending the report's elapsed total over work after the report is
written (report finalization, wrapper teardown). The report is written before
teardown by design and no second report or outer rewrite is produced, so
`Report::total_seconds` and the report schema are unchanged.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-R3-1 every phase owns and terminates its descendant group by the deadline | `mise.toml` `bench:capacity`: one `phase RESERVE KILL CMD` runs each RustFS, wrapper, build, and run step under non-foreground `timeout --kill-after` (own process group, TERM then KILL to the whole group), forwards INT/TERM to that group, and sweeps survivors with `pkill -KILL -g` when the step ends; the wrapped bash reuses the exported definition | `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=only -E 'test(=tests::a_hung_setup_step_is_killed_with_its_descendants_by_the_deadline)'` (1 passed, 7.0 s); `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=only -E 'test(=tests::a_hung_benchmark_run_is_killed_with_its_descendants_by_the_deadline)'` (1 passed, 30.1 s). Both run the task text from `mise.toml` with only the limit shortened and TERM-ignoring stand-ins. Against the previous task text the run proof failed (descendant survived) and the setup proof hung past its deadline | PASS |
| AC-R3-2 elapsed covers the command tail | Rejected by the integrator (above) | n/a | REJECTED |
| AC-R3-3 replica stop does not block Tokio | `capacity/main.rs::stop_replicas` runs each `LocalServer::stop` through `tokio::task::spawn_blocking`, newest first, results in ordinal order; `Benchmark::clean_up` calls it | `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=only -E 'test(=tests::a_slow_replica_stop_leaves_the_runtime_free)'` (1 passed); with the stop inlined on the worker the same test failed with `the heartbeat ticked 0 times during the stop` | PASS |
| AC-R3-4 exact focused commands | `tasks/task-008-closeout.md` revision-57 matrix: six complete `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=...)'` commands | each command run individually: 1 passed each | PASS |
| AC-R3-5 adjacent behavior unchanged | No workload, SLO, report, production, or public-surface change | `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity` (14 passed, 4 skipped); `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity --run-ignored=only -E 'test(=tests::a_stalled_tenant_setup_stops_the_run_by_its_deadline)'` (1 passed); `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/release_server::/)'` (2 passed); `mise run fmt`, `mise run lints`, `git diff --check` pass | PASS |

The stalling stand-in server now exits two seconds after `SIGTERM` (Python
signal handler) so the stop proof has a measurable clean stop; `SIGKILL`
still ends it as one process, so the stalled-setup proof is unchanged.
Non-goals held: no CLI option, configuration, cap, synthetic load, or full
benchmark run; FIND-13 remains deferred.
