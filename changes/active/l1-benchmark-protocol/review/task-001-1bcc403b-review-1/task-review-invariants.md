# Measurement invariant review

Subject: base `2418dc7848375c00ac6a991cafca88f722bcfdfe` through candidate
`1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`, against
`SPEC-l1-benchmark-protocol` revision 1 and the original TASK-001.
The checkout is `6e158cd39407a5c25e5edad163633765d99fdad9`; its only difference
from the candidate is an unrelated task status file. Reviewed source is unchanged.

## Boundary and authority

This specialist owns REQ-003/004 measurement accounting and their REQ-005/006
record and report seams. The system reviewer owns process lifecycle deadlines
and cleanup; the primary reviewer owns category selection, derived sweep
readings and targets, documentation, and the complete acceptance matrix.

Read the approved spec, original task and execution evidence, design notes,
root AGENTS.md, workflow, maintainer style, Rust and testing guidance, and
the Bifrost durability-and-visibility authority. CodeGraph exploration preceded
source tracing. No additional directory AGENTS.md governs the measured Rust
owners. Production durability remains the acknowledged append boundary;
the change does not add a wait for all publication or maintenance.

Navigation: workload adapters in `capacity/{query,verification,gateway}` feed
`capacity::Window` and `Population` readings into `experiment::run`, then
`judge`/`Repetition::aggregate`/`judge_criterion`; `summary::cells` consumes
the persisted aggregate. `Reading::pooled` restores recorder distributions
through `Distribution::restore` in `wyrd-benchmark/src/registry.rs`.

## Coverage and evidence

- Interval work and CPU remain separate from drained latency cohorts:
  `capacity.rs:180–211` freezes resource readings before draining;
  `until_stop` at 220–237 starts the stop scrape before joining the drain.
  The query adapter records successful rows and bytes minus late counts
  (`query/isolated.rs:432–450`), while `Population::collect` reconciles the
  complete admitted cohort. Verification and Gateway adapters likewise
  keep late-work counters separately. Source checks found no added mixing
  of interval and cohort populations. Assertions are
  `capacity::tests::the_measurement_freezes_at_the_stop_before_work_drains`
  (`capacity.rs:1386`) and
  `experiment::tests::histogram_cohorts_preserve_measurement_and_drain_windows`
  (`experiment.rs:6228`).
- `Lifecycle::window` (`experiment.rs:2190`) accounts admission and excess
  drain separately; `run` awaits each whole `measure` call before the next
  and bounds it by the shared remaining drain allowance (2602–2614).
  The paused-clock case test at 7337 checks three fixed windows, sequential
  draining, shared exhaustion, and setup overruns. Final stop/teardown
  accounting was referred to the system reviewer rather than accepted here.
- `Repetition::aggregate` (1752–1846) sums corresponding work, seconds and CPU,
  propagates missing inputs, and derives rates after those sums. It pools
  histogram counts rather than percentile values. `Reading::pooled`
  (1382–1414) rejects missing or incompatible window histograms, preserving
  bounds, counts and rank through recorder restore/add. The unequal-work
  test at 7190 asserts totals, a ratio different from mean-of-ratios, pooled
  sample count/rank/bounds, and missing CPU/work/histograms. Recorder test
  `registry::tests::kept_distributions_restore_and_pool` at 1239 asserts
  reconstructed buckets and quantile against a combined raw population.
- Full-mode measured-case targets use the unrounded aggregate while sample
  floors and `Over::Every` correctness still apply to each window
  (`judge_criterion`, 3238–3389). The case target test at 7473 checks a valid
  performance miss separately from invalid per-window sample and error
  evidence. `summary::cells` (387–450) takes matching work, CPU, duration,
  and latency from that aggregate. Historical runs keep their earlier
  interpretation because aggregate creation is conditional on phase
  protocol and comparisons reject case-protocol records; test at 7567.
- The smoke integrity exception below prevents accepting REQ-007 and the
  applicable REQ-003 integrity behavior. Existing full-mode negative tests
  and the positive connected smoke do not cover that exception.

The implementation record reports C1 (181 passed, 3 skipped), C2 (GREEN), and
C3 (one connected smoke passed in 152 s). Source assertions were inspected;
those results are retained as recorded evidence, not independently recreated.
A fresh focused nextest invocation was interrupted during dependency compilation
as recorded below. Full-size feasibility remains unverified as expressly allowed by AC-005
while performance execution is held. No full-size run was attempted.

## Proposed blocking finding INV-1 — smoke skips correctness and custody criteria

Classification: `INCORRECT`.

Authority: REQ-007 requires harness smoke checks of workflow **and evidence
integrity**; REQ-003 requires preserving correctness and custody checks and
making failed integrity checks invalid. AC-002 requires the connected smoke
to exercise the normal sweep evidence path.

Location: `crates/wyrd/wyrd-testing/src/experiment.rs:3316–3322`, reached from
`capacity/sweep.rs:594–604`.

Starting conditions and trigger: the L1 harness starts a normal short Bifrost
read case and one or more successful server query responses contain incorrect
rows, alongside enough correct responses for nonempty successful latency and
CPU evidence. `query/run.rs:137–138` includes wrong-result responses in the
server-success cohort, so the normal timing reconciliation can remain exact.
The client detects the incorrect rows in `query/run.rs:800–803`; the adapter
retains nonzero `errors` and `wrong_results` readings
(`query/isolated.rs:613–631`). The declaration supplies `no-errors` and
`exact-answers` as `Criterion::every` (`query/mod.rs:1069–1075,1107`).

Expected: the case is INVALID for its failed correctness criterion, and the
saved sweep preserves that invalid result, while performance targets remain
inapplicable in a smoke.

Actual: `sweep::narrow` sets `Mode::Smoke`. After checking presence and sample
availability, `judge_criterion` returns inapplicable for **every** smoke
criterion before evaluating `Over::Every`. A present nonzero error/correctness
reading therefore produces no invalid criterion reason. If all other evidence
is healthy the case and sweep can be VALID despite detecting wrong answers.
The same branch bypasses Gateway capture-loss and Verification verdict/evidence
criteria. These readings are not independently converted to workload invalid
reasons merely because their numeric criterion is violated.

Proof: the pre-existing
`experiment::tests::smoke_runs_the_selected_workflow_and_stays_out_of_history`
explicitly asserts errors `10` and `no-errors` inapplicable at
`experiment.rs:6130–6148`. The newly added L1 case-target test exercises its
error-negative case in Full mode only; the connected smoke at
`capacity/tests/pg_tests.rs:720` tests healthy workloads and lifecycle failures,
not failed numeric correctness criteria. Reusing the legacy smoke semantics
does not satisfy the new smoke contract.

Consequence: the required harness lane can pass a server change that returns
incorrect answers or loses evidence, defeating the smoke's stated integrity
purpose and giving a false VALID record.

Smallest correction boundary: the shared experiment judgment owner should
evaluate case-protocol smoke correctness/custody criteria on each window,
while leaving performance targets and performance sample qualification
inapplicable. Preserve historical record interpretation and avoid requiring
unrelated L2 smoke policy changes. Add a focused case-protocol smoke negative
test for present nonzero correctness evidence, plus a connected smoke
assertion demonstrating its invalid result through the existing sweep path.
Reuse current criteria and reporting; no new recorder or validity subsystem is
needed.

## Verification and process limits

Behavioral conclusion for this boundary: `FIX_REQUIRED` due to INV-1, pending
independent validation. No additional aggregate or measured-case target defect
was retained. Derived sweep target retention is independently owned by the
primary reviewer.

TDD: `VIOLATED` for scenarios 1–4 as explicitly recorded by implementation.
Later focused execution cannot establish historical RED; no production churn
or manufactured historical failure is requested.

Fresh runtime verification attempted:

```sh
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::case_aggregates_sum_windows_and_pool_histograms) | test(=experiment::tests::case_clock_charges_shared_allowances_and_invalidates_overruns) | test(=experiment::tests::case_targets_judge_aggregates_apart_from_validity) | test(=experiment::tests::smoke_runs_the_selected_workflow_and_stays_out_of_history) | test(=capacity::tests::the_measurement_freezes_at_the_stop_before_work_drains)'
```

The command waited on the package cache, rebuilt dependencies, and was cleanly
interrupted with SIGINT (exit 130) while compiling `wyrd-server`, at the
orchestrator's instruction to bound review execution. Tests had not started;
this is neither a passing result nor a test failure. No recorder focused test
or full-size benchmark was attempted. The immutable source checks and existing
implementation execution claims above are kept distinct from fresh execution.
Cargo slot released after interruption. Tracked source remained clean and HEAD
remained `6e158cd39407a5c25e5edad163633765d99fdad9` at completion. Review reports
are the only writes; no implementation, commit, merge, or performance execution.
