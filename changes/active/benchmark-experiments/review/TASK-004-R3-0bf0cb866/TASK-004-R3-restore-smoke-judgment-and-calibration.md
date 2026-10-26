---
id: TASK-004-R3
kind: remediation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 10
requirements: [REQ-007, REQ-008, REQ-013, REQ-015, INV-004, INV-005, AC-012]
depends_on: [TASK-004-R2]
parent_task: TASK-004
remediates: [FIND-TASK-004-8]
---

# Verify smoke plumbing, full-mode judging and bounded calibration

Route directly to `$wyrd-implement`. The maintainer withdrew F7 as written:
production errors, dropped/refused calls, capture shedding, wrong answers and
missed targets do not fail smoke when they are accurately measured and recorded.
Smoke verifies benchmark plumbing. Prove normal PASS/FAIL/INVALID judging in a
separate controlled test, and complete F8's bounded calibration. This task does
not optimize the engines or implement TASK-005 early.

## Acceptance context

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
- Original base: `0b500d677531ba501924b1c6d412fe14ff9aa5cd`.
- Prior candidate: `c9d72729b85187f2399725e0ed9496c4c4a39298`.
- Reviewed candidate / implementation base: `0bf0cb8662a221233a963ed147d57ef0cbb6720f`.
- Spec: `changes/active/benchmark-experiments/spec.md`, approved revision-8
  behavior plus the real-path smoke workflow and approved revision-10
  plumbing-only verdict/separate judging proof. The SHA below identifies the
  historical review authority, not the current revised spec.
  Reviewed working-tree SHA-256:
  `007d27e3a0e22a4aa912108da6a7478849a3f2f56c953fdea9e1491c65e96dce`.
  The maintainer explicitly approved the correction and directed this packet
  revision; no further approval is required.
- Original task: `changes/active/benchmark-experiments/tasks/TASK-004-server-benchmark-timing.md`.
- Prior remediation: `../TASK-004-R2/TASK-004-R2-finish-stop-counters-and-otlp-cohorts.md`.
- Review diagnosis, independently validated ledger and closure evidence:
  this directory's `verdict.md`, `findings-validation.md`,
  `remediation-review.md`, `maintainer-review.md`,
  `domain-review-security.md` and `verification.md`.

F1/F2/F3/F5 are closed, F6 remains closed, and F4's authenticated-activation
mechanic is superseded by the current arrival-time classification contract.
Preserve those accepted results and the PR215 integration. Do not restore
Pending/activate_request or reopen the original review. Preserve uncommitted
maintainer spec/TASK-005 revisions and existing benchmark artifacts.

## Maintainer direction — Plumbing-only smoke; separate judging proof

FIND-TASK-004-7 is withdrawn, not an implementation finding to close. The old
instruction to delete the blanket smoke criterion exemption is superseded.
The historical errors/shedding records demonstrate measured production behavior;
their counts do not establish broken smoke plumbing. Revision 10 owns this
correction. Do not alter historical reports to pretend they used that authority.

### Required behavior

Keep the actual normal benchmark workflow, fixture loading, publication waits,
measurement, stop/drain, outcome reconciliation, persistence and report readback.
A smoke PASS means those pieces worked. Production criteria remain explicitly
inapplicable, with all observed values and the smoke-only reason retained.
Record errors, refused/dropped calls, loss/shedding, wrong-result counts and
performance misses without using those outcomes alone to fail smoke or block
full-run preparation. Full runs continue to judge their existing criteria.

Smoke must still be INVALID for broken measurement or evidence: missing required
measurements/counters, reset counters, inconsistent outcome populations, zero
CPU, missing applicable scan facts, failed evidence/record writes or unreadable
records/reports. For example, 10 accurately accounted dropped calls are an
observation; 10 calls missing from outcome accounting are a plumbing failure.
No synthetic zero or favorable value may replace absent evidence. Preserve the
existing isolated prepass: plumbing PASS permits the full run, INVALID blocks it.

### Selected correction and proof

Keep the shared runner and current production-criterion inapplicability policy;
do not introduce a second smoke runner, judge or per-engine policy. Extend only
missing proof or incorrect measurement/persistence behavior. Already-correct
behavior is verification-only; no manufactured RED or runtime churn is required.

Use the existing deterministic normal/full-mode judgment and record/report tests
to feed a known failing result through the shared judge. Assert FAIL, its exact
observed value, exit 1, persisted/read-back verdict and regenerated report. Also
preserve controlled PASS and INVALID/exit 2 with invalidity precedence. Reuse
existing assertions where they already establish this complete chain; extend
only an actual gap. This is separate from live smoke and needs no real engine
performance regression or new test framework.

The smoke workflow proof must retain nonzero production error/drop readings and
a target miss while passing when required plumbing remains valid. Missing or
inconsistent evidence must still produce INVALID. Readback must preserve the
observations and explicit inapplicability. Preserve the invalid-prepass refusal
before full preparation. Do not weaken full-run criteria, alter concurrency or
change production capture/admission limits to get green smoke.

## FIND-TASK-004-8 — Calibrate the measurement path, including observation cost

### Diagnosis

AC-012 and R2's Required proof call for controlled known-delay, known-CPU and
instrumentation-overhead checks through the same measurement path, against
independent references and justified tolerances. The sole new calibration at
`crates/wyrd/wyrd-testing/src/capacity.rs:1085–1193` directly records timings,
reads an in-process snapshot, manually constructs Population/Repetition, and
separately checks this process's CPU reader around a bounded computation.

Those are useful local tests. They do not exercise Window/resource accounting
through the normal runner. Their `overhead_p50_ms` is service time remaining
after a simulated wait, not the extra cost of recording measurements. There
is no equivalent workload without collection or comparison of observation
cost. The test can pass while the actual resource-window wiring is wrong;
it cannot establish the calibration its name/evidence claims.

### Selected correction

Complete one bounded controlled case using existing release-server/engine
Workload and measurement owners: actual Window/resource accounting, retained
timing readout and Population/Repetition conversion through shared
`experiment::run` and its record. Compare measured duration and CPU with
independent references for a known delay and bounded in-process CPU work.
Declare reference values and justified nonzero tolerances before execution.

Compare the same controlled workload with benchmark collection disabled and
enabled, using the existing LocalServer startup configuration for isolated
processes. Record independently observed wall/CPU costs, their difference and
the predeclared justified allowance. Distinguish observer cost from the
operation's residual time. A disabled run need not produce timing histograms;
its independent measurements are the control for the otherwise equivalent
workload. The enabled run must traverse the normal measurement/readout path.

Reuse existing launch, Workload, Window, resource and record owners. Extend
their proof rather than adding a side runner, collector, dependency, registry
reset or production enable/disable API. The existing Scripted runner fixture
fabricates Repetition readings and alone cannot prove actual measurement.
Retain useful local arithmetic/reader tests. This is one calibration case,
not TASK-005's comprehensive per-case metric and profile proof.

### Closure proof

The controlled case retains time, CPU and timing evidence comparable with its
independent references. The enabled/disabled comparison reports observation
cost and rejects a cost outside its declared allowance. Demonstrate that the
assertions reject materially wrong duration/CPU readings and an excessive
observation cost at the existing assertion seam; no mutation-testing framework
or artificial host contention is required. Record the exact command, reference
values, tolerance rationale and observed results. Do not claim literal zero
instrumentation overhead.

## Scope, acceptance and verification

| Obligation | Required observation |
|---|---|
| Maintainer direction; F7 withdrawn | Smoke records production outcomes without judging their targets; broken plumbing is INVALID. Separate controlled normal-mode proof retains PASS/FAIL/INVALID through judgment, persistence, report and exit codes. |
| F8 | One controlled actual measurement-to-record case agrees with independent time/CPU references; equivalent enabled/disabled collection quantifies observation cost within a predeclared justified allowance. |

Preserve real fixture loading/publication, every selected step and phase,
stop/drain separation, exact cohorts, retained distributions, normal production
authentication/tenant/durability behavior, optional tracing/profiling, smoke mode
and baseline exclusion, clean-source rules, and full-run windows/repetitions.
The practical asynchronous stop scrape is accepted; no atomic distributed
snapshot is requested. No broad source audit, unrelated schema work, engine
optimization, full-size qualification or language journey suites in this task.

TASK-005 owns the writes-hot/writes-parquet split and complete four-experiment
Bifrost matrix. R2's wording demanding four experiments before that split is
corrected here: this remediation exercises every currently declared default
plan. TASK-005 must exercise all four after implementing them; no final matrix
obligation is dropped and no missing case may be hidden by smoke selection.

Use the exact existing selectors while extending their proofs:

```sh
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::smoke_runs_the_selected_workflow_and_stays_out_of_history)'
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::criteria_distinguish_regression_noise_and_invalidity)'
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::reports_regenerate_from_persisted_records)'
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=capacity::tests::known_delay_overhead_and_cpu_calibrate_the_measurement_path)'
```

Confirm actual target/selectors for any added real-server proof, put it in the
owning Postgres/server-backed test tier, and use the repository Postgres wrapper
and existing release-build prerequisites. Record its exact command and expected
RED cause before claiming GREEN. Do not present the old direct-reader selector
alone as real-path proof. Run only the narrow measurement/experiment and engine
projection lanes affected by the proof, plus scoped fmt/clippy and diff checks.

Run the existing `mise run bench:smoke` on clean final source, without
`--allow-dirty`, after integrating corrections. Retain every default plan's
record/timing evidence, regenerate its report with `capacity report <record>`,
and verify the readings and criterion applicability survive readback. Record
actual exit codes: smoke is PASS/0 for functioning plumbing or INVALID/2 for
broken plumbing. Production outcomes remain visible but do not fail smoke;
full runs still judge them normally. Diagnose invalid/missing evidence at its
existing owner. A smoke run does not replace the independent calibration or
TASK-005 qualification.

Append evidence and the next immutable candidate. Commit/push only if asked.
Next review covers F8 closure, the maintainer-directed separate judging proof,
plumbing-only smoke evidence and affected consumers/tests/docs. F7 must not be
reopened under its withdrawn interpretation. Carry forward accepted original
findings instead of rediscovery.

## Implementation evidence

Commits on `change/benchmark-experiments` after `0bf0cb866`: `dc45a9d5e`
(judging proof, smoke readback, docs) and `0e29eba74` (F8 calibration).
Next immutable candidate: `0e29eba74` plus this evidence commit.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Smoke records production outcomes without judging them | `experiment.rs` smoke test: 10 errors and a p99 miss PASS; readback keeps both observed values beside inapplicable criteria with the smoke reason; empty population and zero CPU stay INVALID | `experiment::tests::smoke_runs_the_selected_workflow_and_stays_out_of_history` | PASS |
| Separate full-mode judging proof | `reports_regenerate_from_persisted_records`: read-back FAIL/exit 1 with observed 0.7, PASS/exit 0, INVALID/exit 2, and busy-host INVALID/exit 2 over a kept miss; report regenerates from the record alone | that selector and `criteria_distinguish_regression_noise_and_invalidity` | PASS |
| Invalid prepass still blocks preparation | unchanged `prepass.failed` refusal | smoke selector above | PASS |
| F8 known delay, known CPU, timing readout through `experiment::run` and the record | `verification/run.rs` `calibration` module: real L1 `Bench`, `Window`, `ResourceWindow`, `Population`, published and re-read record, kept timing files | `verification::run::tests::calibration::the_measurement_path_agrees_with_independent_references` | PASS |
| F8 observation cost against a collection-off control | `LocalServer::restart` with `WYRD_METRICS_BENCHMARK_SNAPSHOT=0` on the same fixture; timing readout absent; same driver and window | same test | PASS |
| Assertions reject wrong readings | same test: window read in ms, CPU 0, server time over the client round trip, cost 2x allowance each rejected | same test | PASS |
| Clean final-source smoke | `mise run bench:smoke` at `0e29eba74`, clean worktree, no `--allow-dirty` | exit 0 in 1,056 s; see below | PASS |

F8 command (after `mise run bench:build`, quiet host):
`RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=verification::run::tests::calibration::the_measurement_path_agrees_with_independent_references)'`
It is `#[ignore]` (maintainer choice) because it needs the release server,
Postgres and a quiet host; family lanes skip it.

References, tolerances (declared in code before the run) and results:

| Check | Reference and tolerance | Observed |
|---|---|---|
| Window seconds | declared 5 s, +0.05 s (ms timer plus stop scrape) | 5.0007 s, 5.0009 s |
| Known 1 s driver spin | spun minus plain driver CPU in [0.8 s, 1 s + 0.02 s ticks + 25% of plain driver CPU] = [0.80, 1.2625] | 1.0000 s |
| Server timing | server p50 bucket within (0, client round-trip p50]; count = verifications + late | (0.139647, 0.139775] ms under 0.227 ms; reconciled |
| Observation cost, wall per verification | within ±max(10% of control, 0.01 ms) | 0.2412 collected vs 0.2350 control: +0.0062 ms (allowance ±0.0235) |
| Observation cost, server CPU per verification | same | 0.3274 vs 0.3190: +0.0084 ms (allowance ±0.0319) |
| Operation residual (not observer cost) | reported separately | mean 0.146 ms |

The cost is a measured difference between two sequential windows, not a
claim of zero instrumentation overhead.

Clean smoke at `0e29eba74`: all eight default plans PASS/0 (L1 Bifrost
writes, hot, parquet; Verification; Gateway; L2 Bifrost, Verification,
Gateway). Every record has `mode: smoke`, 0 judged criteria, every
inapplicable criterion keeps its observed value (232 in all), timing evidence
is kept per plan, `capacity report <record>` regenerates each with exit 0,
and `benchmarks/` is unchanged. Production outcomes stay visible: Verification
c64 recorded 13,787 server errors and the smoke still PASSed.

Other checks: `cargo fmt --check -p wyrd-testing`, `cargo clippy --locked -p
wyrd-testing --all-targets -- -D warnings`, 66 `experiment::`, `capacity::`,
`release_server::` and capacity-binary tests, `git diff --check`: all pass.

Non-goals stayed out: no engine optimization, no TASK-005 matrix, no new
dependency, no production enable/disable API, no change to full-run criteria,
concurrency or capture/admission limits. Changed outside tests:
`release_server.rs` (`restart`, caller environment overrides the timing
default), `capacity.rs` (`release_binary` finds the server from `deps/`),
`benchmarks/README.md`, `mise.toml` `bench:smoke` description, `Judgment`
rustdoc. Limits: the calibration is one case on one host (Linux x86_64);
four-experiment Bifrost smoke remains TASK-005's.
