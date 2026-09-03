---
id: BIFROST-R4-T03-R01-TERMINAL-LIFECYCLE-EVIDENCE
title: Make Task 03 terminal, settlement, ownership, and shutdown evidence fail closed
kind: remediation
mode: REMEDIATE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 4
depends_on: [BIFROST-R4-T03-PHYSICAL-BASELINE]
requirements: [REQ-003, REQ-005, REQ-007, REQ-011]
invariants: [INV-003, INV-004, INV-005]
acceptance: [AC-002, AC-004, AC-005, AC-007]
parent_task: BIFROST-R4-T03-PHYSICAL-BASELINE
reviewed_base: ec5ec48a85878cc06428f437f7d8bbd50ecac4be
reviewed_candidate: 5b0f885c1423c7142aa01db7a51699cb2b373cee
planning_snapshot: b9d7d0cafafc421b4215917391d8827a672e6dc3
evidence_snapshot: 5b0f885c1423c7142aa01db7a51699cb2b373cee
remediates:
  - FIND-BIFROST-R4-T03-PHYSICAL-BASELINE-1
  - FIND-BIFROST-R4-T03-PHYSICAL-BASELINE-2
  - FIND-BIFROST-R4-T03-PHYSICAL-BASELINE-4
  - FIND-BIFROST-R4-T03-PHYSICAL-BASELINE-5
  - FIND-BIFROST-R4-T03-PHYSICAL-BASELINE-6
---

# Task 03 terminal and lifecycle evidence remediation

## Outcome and value

Task 03 fails closed when distributed metric rewriting, query terminal
validation, retained ownership, or ordered process shutdown is not clean. The
three-Oracle process journey proves each coordinator's exchange activity and
every Oracle's complete ownership return for that execution rather than relying
on cumulative counters or three selected gauges.

Required execution skill: `$wyrd-implement`.

## Review authority and retained implementation

The immutable reviewed candidate is
`5b0f885c1423c7142aa01db7a51699cb2b373cee` over base
`ec5ec48a85878cc06428f437f7d8bbd50ecac4be`; planning used snapshot
`b9d7d0cafafc421b4215917391d8827a672e6dc3`. Retain the accepted
`OracleSessionShape`, exact result/spill evidence, three-Oracle/one-Scribe
topology, graph-owned metric fold, listener isolation, and existing process
control protocol. This task closes only the five validated findings.

Primary owners:

- `crates/vala/vala-bifrost-redux/src/oracle/exec.rs`,
  `oracle/analytical.rs`, and `oracle/analytical_supervisor.rs`: metric rewrite
  and graph settlement.
- `crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs` and
  `process_cluster/child.rs`: terminal validation, bounded ownership
  inspection, and explicit shutdown reporting.
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/analytical.rs`
  and `listener.rs`: per-execution exchange, ownership, and shutdown evidence.

Affected consumers are inactive Analytical query controls, the physical
baseline journey, every explicit `BifrostProcessCluster::shutdown` caller, and
`Drop`. Do not change public query frames, terminal variants, routes, metrics,
resource limits, deadlines, deployment topology, retry policy, or generated
contracts. Add no dependency, feature, parallel lifecycle owner, or new
production telemetry registry.

## Ordered implementation scenarios

### Scenario 1 — Metric rewrite errors retain the graph as failed cleanup

**Behavior.** A completed distributed metric rewrite supplies the rewritten
plan used for physical evidence. A rewrite error is a graph-settlement failure:
it publishes `SettledFailure`, retains the graph in `Draining`, prevents clean
readiness, and cannot fall back to the unexecuted original plan. Maps REQ-007,
REQ-011, INV-003, INV-004, AC-004, AC-007 and remediates
FIND-BIFROST-R4-T03-PHYSICAL-BASELINE-1.

**RED.** Extend
`oracle::analytical_supervisor::tests::distributed_metrics_settle_within_the_graph_deadline`
without changing its existing timeout and successful-rewrite cases. Exercise
the immediate-error case in a fresh `AnalyticalSupervisor` with a fresh Oracle
resource fixture and graph whose fold returns `DataFusionError::Execution`.
Assert that this isolated supervisor publishes `SettledFailure`, retains
exactly one `Draining` graph with a recorded rewrite failure, exposes retained
state that the existing Analytical health/readiness checks reject, records no
new physical evidence, and reports that graph during shutdown. Do not reuse or
reset the supervisor containing the timeout graph. The case initially fails
because `.ok()?` erases the dependency error and `unwrap_or(plan)` converts it
into successful original-plan evidence. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical_supervisor::tests::distributed_metrics_settle_within_the_graph_deadline)'
```

**GREEN.** Make `oracle::exec::record_distributed_scan_metrics` return the
pinned dependency's `DataFusionResult<Arc<dyn ExecutionPlan>>` unchanged after
recording scan metrics; remove `.ok()?`. Make `AnalyticalGraphMetricFold` retain
that result-bearing future and make `settle` return the rewrite error instead
of storing the original plan as a fallback. Update the test-only
`from_future` seam to accept the same result shape. In
`AnalyticalGraphLifecycle::fold_physical_metrics`, distinguish timeout from an
immediate rewrite error, render either as the existing settlement-failure
detail, and let `AnalyticalGraphLifecycle::settle` use its existing
`retain_graph_cleanup`/`SettledFailure` route. Only a successful rewritten plan
may feed `output_sort_evidence` and production spill counters.

**REFACTOR.** The graph lifecycle remains the only deadline and settlement
owner. Do not introduce an error side channel, a second timer, fabricated zero
metrics, or original-plan fallback.

### Scenario 2 — The process query helper accepts only a validated success terminal

**Behavior.** Rows returned before a failed, degraded, malformed,
row-count-mismatched, or invalid-EOS terminal are discarded as an error. The
helper returns rows only after one well-formed `Success` terminal validates
against `PublishedOnly`, matches emitted rows, and closes the decoder with its
explicit EOS. Maps REQ-003, REQ-007, INV-003, AC-002, AC-004 and remediates
FIND-BIFROST-R4-T03-PHYSICAL-BASELINE-2.

**RED.** Add
`bifrost::process_cluster::child::tests::inactive_sql_terminal_rejects_failed_output_after_rows`.
Use the existing query IPC encoder/decoder and terminal fixtures to accept a
schema and one batch, then supply a structurally valid failed terminal whose
row count matches the preceding rows. Assert the process helper returns
`ProcessClusterError::Child` and never returns the row count. The Task 03 happy
path remains the success/EOS proof. Exact command:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=bifrost::process_cluster::child::tests::inactive_sql_terminal_rejects_failed_output_after_rows)'
```

**GREEN.** At the end of `drive_inactive_sql`, pass the observed terminal,
request visibility, emitted row count, and live `QueryIpcDecoder` through one
narrow private terminal helper. In order, call `QueryTerminalFrame::validate`,
`validate_emitted_rows`, require `QueryTerminalOutcome::Success`, call
`QueryIpcDecoder::accept_eos` with `arrow_ipc_eos`, and require the decoder to
report EOS accepted before returning rows. Map every refusal through the
existing `ProcessClusterError::Child` boundary. Keep the helper private to the
child process module; it exists only to make this terminal decision directly
testable.

**REFACTOR.** Reuse the contract and decoder methods verbatim. Do not reproduce
their validation matrix, accept `Degraded`, or add a second terminal model.

### Scenario 3 — Each execution proves exchange and complete ownership return

**Behavior.** Each coordinator execution independently increases exchange
batches and bytes, and every Oracle returns all Analytical, admission, root
query-resource, scratch, and live-gauge ownership to its exact pre-execution
baseline. Maps REQ-003, REQ-005, REQ-007, INV-004, INV-005, AC-002, AC-004,
AC-007 and remediates FIND-BIFROST-R4-T03-PHYSICAL-BASELINE-4 and
FIND-BIFROST-R4-T03-PHYSICAL-BASELINE-5.

**RED.** Extend
`peer_network::analytical::inactive_baseline_executes_join_group_spill_and_interchangeable_topology`.
Before each call to `execute_analytical_baseline`, capture that coordinator's
`EXCHANGE_COUNTERS` and one complete ownership snapshot from every Oracle.
After settlement, require both exchange totals to be strictly greater than
their corresponding pre-execution totals and every ownership snapshot to
equal its own baseline. This must fail if Oracle 1 merely inherits Oracle 0's
cumulative exchange totals or if a graph/attempt, cleanup failure, active or
queued query, peer reservation, query slot, query memory/scratch owner,
exchange/fragment owner, or process spill entry remains live. Exact command:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::analytical::inactive_baseline_executes_join_group_spill_and_interchangeable_topology)' --run-ignored=all"
```

Add
`bifrost::process_cluster::tests::oracle_ownership_snapshot_round_trips` to pin
the bounded test-control serialization shape. Exact command:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=bifrost::process_cluster::tests::oracle_ownership_snapshot_round_trips)'
```

**GREEN.** Add one test-only `OracleOwnershipSnapshot` request/response to the
existing process control protocol and one `ProcessNode` accessor. The child
assembles it only by projecting existing production owners:

- `AnalyticalExecutionHandle::live()` for leader/follower attempts, graphs,
  and cleanup failures;
- `Oracle::runtime_inspection()` for active/queued queries, reserved memory and
  spill, and pending/running peer slots;
- `OracleResources::snapshot()` for Oracle query count, class count, slot
  units, query memory, query scratch, and active flag;
- the existing process-owned scratch inspection; and
- the existing live attempt/exchange/fragment production gauge totals.

The snapshot carries fixed scalar fields only; it owns no state and grants no
new capability. Graph and attempt counts are the production ownership boundary
for their graph-local worker, task cache, driver tasks, and connections: no
test-only cache or task registry is added. Capture and compare the complete
snapshot inside each iteration of `coordinate_baseline`. Move exchange counter
sampling into the same before/after boundary and require `after > before` for
both families.

**REFACTOR.** Keep inspection read-only, test-only, and bounded. Production
owners remain authoritative; the process protocol may project them but may not
duplicate lifecycle accounting or infer cleanup from elapsed time.

### Scenario 4 — Explicit shutdown reports failure after reaping every child

**Behavior.** Explicit cluster shutdown returns an error when any child rejects
the request, misses graceful exit and is killed, cannot be reaped, or has a
reader/reaper join failure, while still attempting and joining every child.
`Drop` remains best-effort because it cannot return an error. Maps REQ-007,
AC-005 and remediates FIND-BIFROST-R4-T03-PHYSICAL-BASELINE-6.

**RED.** Add the ignored process-harness test
`peer_network::listener::explicit_shutdown_reports_failure_after_reaping_every_child`.
Start exactly `[ProcessNodeTarget::Oracle, ProcessNodeTarget::Scribe]`, kill the
Oracle through the existing `ProcessNode::kill` seam, then call explicit
cluster shutdown. Assert it returns the Oracle's rejected-request error, the
Scribe reports normal shutdown, and both children are reaped with no node left
in the cluster. Exact command:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::listener::explicit_shutdown_reports_failure_after_reaping_every_child)' --run-ignored=all"
```

Retain the normal ordered-shutdown proof in Scenario 3's listener journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete)' --run-ignored=all"
```

**GREEN.** Replace the reaper's unbounded `()` completion channel with
`std::sync::mpsc::sync_channel(1)` carrying `Result<(), String>`. The reaper
must record the first `try_wait`, kill, or wait error, attempt all remaining
cleanup that is still possible, and send that first process-control error only
after cleanup. `ProcessNode::kill` and `ProcessNode::shutdown` must consume and
propagate that result. `ProcessNode::shutdown` likewise preserves the first
error in operation order—shutdown-request failure, graceful timeout or channel
failure, forced-kill/reaper failure, then reader/reaper join failure—while
always closing stdin, forcing termination when required, and joining its
threads. A forced kill after the graceful timeout is itself a shutdown error
even when kill, reap, and joins succeed.

Make `BifrostProcessCluster::shutdown` return
`Result<(), ProcessClusterError>`, attempt every node in cluster order after a
failure, clear the node list, and return the first child error with later child
labels/details appended. Update every explicit shutdown caller to propagate or
assert the result. Define failure handling at the three callers that currently
clean up implicitly:

- `start`: after a node launch error, shut down the nodes already started. If
  cleanup succeeds, return the launch error unchanged. If cleanup fails, return
  one `ProcessClusterError::Child` containing both the launch and cleanup
  details.
- `restart`: remove and shut down the previous node first. Launch no
  replacement unless that shutdown succeeds; on shutdown failure, return it
  with the slot left empty. A later replacement-launch failure keeps the
  existing empty-slot behavior.
- `probe_startup_failure`: when the invalid node unexpectedly starts, preserve
  that primary probe failure, shut the node down, and append any cleanup
  failure to the returned `ProcessClusterError::Child`. An expected launch
  failure remains the successful probe result.

`Drop` is the only caller allowed to discard a shutdown result with `let _ =`.

**REFACTOR.** Keep one shutdown path and the existing reaper. Do not add a
shutdown trait, background supervisor, retry loop, or configurable test
timeout.

## Cross-scenario decisions and authority

Metric rewrite, terminal validation, ownership inspection, and shutdown all
close the same Task 03 proof: a process journey cannot report a successful
physical baseline unless its result terminal and metric fold are valid, its
current execution produced exchange, all ownership returned, and explicit
teardown succeeded. Failures reuse existing production state machines and
contract validators; test control remains a read-only projection.

Authority:

- `AGENTS.md`, especially sections 5, 6, 11, 12, 14, and 15.
- `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx`.
- `architecture/bifrost-design.md`, especially distributed execution, terminal
  framing, cleanup, readiness, and ordered shutdown.
- `architecture/wyrd-security-posture.md` for fail-closed peer and terminal
  behavior.
- `architecture/operations/reliability-and-recovery.md` for partial-result
  prevention, retained cleanup, readiness, and shutdown evidence.
- `architecture/references/domain/datafusion.md`,
  `architecture/references/domain/olap-serving.md`, and
  `architecture/references/domain/analytical-operations-reliability.md`.
- `architecture/references/languages/spec-driven-development.md` and
  `architecture/references/languages/testing-workflows.md`.

## Broader verification

Run every named test at its scenario's exact command, then:

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run check:bifrost-oracle-deploy
mise run check:bifrost-resource-governance
mise run check:object-store-pin
git diff --check
```

No contract, codegen, Python, TypeScript, client-tier, or PyO3 surface changes,
so their checks are not required.

## Completion evidence

- The metric-settlement test passes timeout, successful rewrite, and immediate
  rewrite-error cases; the error case retains `Draining`, reports
  `SettledFailure`, blocks readiness, and publishes no success evidence.
- The terminal-helper test rejects matching rows followed by a valid failed
  terminal; the process journey accepts only a validated success terminal and
  explicit EOS.
- Each coordinator execution independently increments exchange batches and
  bytes, and every Oracle's complete ownership snapshot returns exactly to its
  own pre-execution baseline.
- The ownership control shape round-trips without unbounded or identity-bearing
  fields.
- Explicit shutdown failure is observable after every child is reaped; the
  normal listener journey proves ordered shutdown still succeeds.
- All broader verification commands pass on the remediation candidate.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if closure requires accepting degraded/partial
results, changing a public terminal or error contract, weakening cleanup or
readiness, extending deadlines, changing retry behavior, or changing topology.
Return `PLAN_BLOCKED` if the pinned dependency cannot preserve metric rewrite
errors or the existing Oracle/Analytical owners cannot expose their current
test-tier inspection without a parallel production owner. Repository inspection
found both capabilities, so no block is known.

## Execution evidence

Executed under `$wyrd-implement` on base
`5b0f885c1423c7142aa01db7a51699cb2b373cee`. Every scenario command below was
run verbatim as written in its scenario unless a correction is named.

### Scenario 1 — metric rewrite errors retain the graph

- **RED.** With the seam still shaped `Option<Arc<dyn ExecutionPlan>>`, the
  rewrite-error case was driven with the `None` that `.ok()?` produces from a
  refused rewrite. Observed failure:
  `assertion left == right failed: a rewrite error cannot publish a success
  terminal / left: SettledSuccess(Some(AnalyticalAttemptRelease { ... outcome:
  Success ... })) / right: SettledFailure`.
- **GREEN.** `oracle::exec::record_distributed_scan_metrics` now returns
  `DataFusionResult<Arc<dyn ExecutionPlan>>`; `AnalyticalGraphMetricFold`
  carries that result and `settle` propagates the error;
  `fold_physical_metrics` renders it as the new `METRIC_FOLD_REFUSED` detail
  and takes the existing `retain_graph_cleanup`/`SettledFailure` route.
  Command passed.
- **REFACTOR.** `AnalyticalGraphMetricFold::plan` was removed rather than
  retained unread: it existed only as the `unwrap_or` fallback, and the fold is
  constructed only for a distributed plan. `from_future` lost the same
  parameter. The rewrite-error case was extracted to
  `rewrite_error_retains_its_own_graph` to keep the test function inside the
  workspace `too_many_lines` bound; it remains one test.

### Scenario 2 — validated success terminal

- **RED.** `accept_query_terminal` was first introduced holding today's
  behavior verbatim (`let _ = terminal; Ok(emitted_rows)`), which is what
  `drive_inactive_sql` did inline. Observed failure:
  `rows preceding a failed terminal are not a result: Ok(3)`.
- **GREEN.** The helper now calls `QueryTerminalFrame::validate`,
  `validate_emitted_rows`, requires `QueryTerminalOutcome::Success`, calls
  `QueryIpcDecoder::accept_eos` with `arrow_ipc_eos`, and requires
  `eos_accepted()`, mapping every refusal through
  `ProcessClusterError::Child`. `drive_inactive_sql` returns through it.
  Command passed.

### Scenario 3 — per-execution exchange and complete ownership

- **GREEN.** `ControlRequest::OracleOwnership` /
  `ControlResponse::OracleOwnership` and `ProcessNode::ownership_snapshot`
  carry a 22-field `OracleOwnershipSnapshot` projected from
  `AnalyticalExecutionHandle::live()`, `Oracle::runtime_inspection()`,
  `OracleResources::snapshot()`, the process scratch inspection, and the three
  live production gauges. `coordinate_baseline` captures and compares one
  complete snapshot per Oracle inside each iteration and samples
  `EXCHANGE_COUNTERS` in the same before/after boundary, requiring
  `after > before` for both families. The `LIVE_GAUGES` constant was deleted:
  the snapshot subsumes it, and leaving it would be a second, weaker copy of
  the same claim.
- Both commands passed:
  `peer_network::analytical::inactive_baseline_executes_join_group_spill_and_interchangeable_topology`
  and `bifrost::process_cluster::tests::oracle_ownership_snapshot_round_trips`.
- **Limitation, recorded rather than papered over.** The round-trip test had a
  real RED (an off-by-one field-count pin). The journey's strengthened
  assertions were green on first run and no demonstrated RED exists for them:
  producing one would require a coordinator that executes the baseline while
  exchanging nothing, or an Oracle that genuinely leaks an owner, neither of
  which is reachable without changing production behavior. The mutation
  argument stands in its place — restoring the cumulative `> 0.0` check makes
  coordinator 1's exchange claim satisfiable by coordinator 0's totals, and
  narrowing the comparison back to scratch plus three gauges stops observing
  attempts, graphs, cleanup failures, admitted and queued queries, peer
  reservations, slot units, query memory, and query scratch.

### Scenario 4 — explicit shutdown reports failure after reaping

- **RED.** `explicit_shutdown_reports_failure_after_reaping_every_child` did
  not compile against `BifrostProcessCluster::shutdown`, which returned `()`:
  `expected (), found Result<_, _>` — there was no result for a caller to
  observe.
- **GREEN.** The reaper's completion channel became
  `sync_channel(1)` carrying `Result<(), String>`; it records the first
  `try_wait`, kill, or wait error and sends it only after every remaining
  cleanup step. `ProcessNode::kill` and `ProcessNode::shutdown` consume and
  propagate it, `shutdown` preserving the first error in operation order and a
  forced kill after the graceful timeout counting as a failure on its own.
  `BifrostProcessCluster::shutdown` returns `Result<(), ProcessClusterError>`,
  attempts every node in cluster order, clears the list, and joins each
  failing child's label and detail. `start`, `restart`, and
  `probe_startup_failure` handle cleanup exactly as the task defines; `Drop` is
  the only caller discarding the result. Both commands passed.
- **In-scope consumer correction.** `prove_terminal_ordering(PeerLoss)` kills a
  pod on purpose and then shuts the cluster down, so it is now a caller of a
  reporting shutdown. Rather than discard the result, it asserts the report
  names exactly the pod it killed and no other, which is the same evidence in
  that journey's own terms.

### Broader verification

`mise run fmt`, `mise run lints`, `mise run test:bifrost` (973 passed),
`mise run test:bifrost:journey:oracle` (16 passed),
`mise run check:bifrost-oracle-deploy`,
`mise run check:bifrost-resource-governance`,
`mise run check:object-store-pin`, and `git diff --check` all pass.

Six `wyrd-testing` lib tests (`bifrost::scribe_workload::tests::…` and five
`bifrost::forge_harness::worker_lifecycle_tests::…`) fail identically on the
reviewed base `5b0f885c` and on this candidate under an ad-hoc
`cargo nextest run -p wyrd-testing --lib`; they pass through their owning
`mise` lanes. Pre-existing and unrelated to this task.

## Review remediation — FIND-BIFROST-R4-T03-PHYSICAL-BASELINE-6

A child that accepted `Shutdown` but whose `WyrdTestServer::shutdown` failed
exits `ExitCode::FAILURE` within the deadline. `reap` matched `Ok(Some(_))` and
discarded that status, so `ProcessNode::shutdown`, the cluster, and the journey
all reported a clean ordered shutdown.

- **RED.** `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E
  'test(=bifrost::process_cluster::tests::a_natural_non_zero_child_exit_is_a_shutdown_failure)'`
  failed on `a failed exit is reported`: the status-mapping owner returned
  `None` for a non-success exit, which is what the reaper did.
- **GREEN.** `natural_exit_failure` reports a non-success status from the
  reaper's natural-exit branch only. The deliberate `ReaperCommand::Kill` branch
  is untouched, so `ProcessNode::kill` still treats its own kill status as the
  expected outcome rather than a shutdown failure. The focused test passes.
- **Closure verification.** `mise run test:bifrost:journey:oracle` passes 16/16,
  including `explicit_shutdown_reports_failure_after_reaping_every_child` and
  `peer_listener_is_isolated_mtls_and_role_complete`. One earlier run of that
  lane failed its six `pg_*` journeys on Postgres startup contention; the rerun
  passed every one, and no process-cluster journey was involved.
- **Broader verification.** `mise run fmt`, `mise run lints`,
  `mise run test:bifrost` (973 passed), the three boundary checks, and
  `git diff --check` all pass.
