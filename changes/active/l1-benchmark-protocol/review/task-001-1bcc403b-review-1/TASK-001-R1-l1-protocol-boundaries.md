---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-l1-benchmark-protocol
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, AC-001, AC-002, AC-003, AC-004]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-6, FIND-TASK-001-7]
---

# Restore L1 protocol boundaries and saved evidence

## Subject and outcome

Implement the settled behavior of [approved spec revision 2](../../spec.md),
using [original TASK-001](../../tasks/TASK-001-standardize-l1-benchmark-sweeps.md)
as historical implementation evidence.
Reviewed base: `2418dc7848375c00ac6a991cafca88f722bcfdfe`; candidate:
`1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`.
[Validated findings](findings-validation.md) define the diagnosed defects;
[verdict](verdict.md) records coverage and execution limits.

After correction, maintainers can discover cases without a build, run them
under the agreed lifecycle allowances, trust smoke integrity, and regenerate
full case evidence from saved records.
This remains one cohesive correction to the existing category sweep path.

The maintainer voided every previous performance target for these refactored
cases. FIND-TASK-001-5 is withdrawn by that approved scope change and creates
no remediation work; the six other finding IDs remain stable. Do not restore
old thresholds or introduce replacement targets. Correctness, custody,
sample-floor and lifecycle checks remain required. Ensure new L1 declarations,
tests and guidance do not treat legacy performance targets as current;
historical records retain their original interpretation.

## Diagnosed gaps and correction owners

- **FIND-TASK-001-1 / REQ-001:** category listing without a prepared release
  driver executes `cargo run` at `scripts/benchmarks/l1.sh:21–26`. Keep one
  authoritative inventory shared with selection; make the actual listing
  entry point build-free. Preserve exact routine/targeted membership,
  selector qualification and order. Do not add a second hand-maintained list.
- **FIND-TASK-001-2 / REQ-002/003:** `Sweep::attempt` applies a 510 s cutoff
  to a wrapper that includes allowed teardown, while `Lifecycle::finish`
  charges actual process stops against unused drain first. Align existing
  runner, workload, release-server and wrapper ownership with actual drain
  versus process/resource teardown. Preserve every approved phase allowance
  and production stop behavior. Both premature rejection and accepted slow
  teardown require closure.
- **FIND-TASK-001-3 / REQ-002/003:** Docker awaits in `compose_resources` and
  `confirm` are unbounded; late empty checks can set cleanup confirmed; stop
  paths renew allowances. At the existing sweep owner, share one remaining
  monotonic cleanup allowance across owned stop/removal/confirmation work.
  Bound external operations, retain timeout/unknown outcomes, and stop later
  launches when timely cleanup is unproven.
- **FIND-TASK-001-4 / REQ-003/005:** `keep_case` measures persistence duration,
  but `entry` discards it and `Lifecycle.persist_seconds` stays unavailable.
  Retain honest final-phase timing at existing record finalization and make
  the full reader recover it. Preserve late-save invalidation with a saved
  reference and unsaved outcomes without one. Do not fabricate a self-inclusive
  write duration.
- **FIND-TASK-001-6 / REQ-005/006, AC-004:** engine JSON drops
  `Repetition.timings` before the supervisor saves its case. Preserve full
  window timing/scan/counter/gauge snapshots within the new case protocol and
  hydrate the existing full renderer from durable evidence. Preserve original
  historical protocol interpretation; reuse the recorder and report owners.
- **FIND-TASK-001-7 / REQ-003/007:** case smoke uses legacy `Mode::Smoke`
  judgment that skips detected numeric correctness/custody failures. At the
  shared judgment owner, evaluate the existing per-window integrity criteria
  for case-protocol smoke while performance qualification stays inapplicable.
  Preserve unrelated L2 policy and historical readers.

## Observable closure scenarios

Follow bounded implementation execution and TESTING.md. Confirm actual test
names/selectors, record exact focused commands before RED, then record the
expected failure and GREEN result in these entries. No new test names are
prescribed. Use controlled clocks and owned local subprocesses for deadline
tests, and the real existing harness for connected proof.

1. **Build-free category discovery** — FIND-TASK-001-1; REQ-001/AC-001.
   With the prepared driver absent, invoke each actual category listing entry
   point and assert the authoritative selectors/statuses. Assert no Cargo
   build, benchmark preparation or case launch occurs. Exercise listing via
   the shell/mise seam, not solely direct Rust dispatch. Keep selection parity
   assertions in the existing CLI test owner. RED is an attempted build or
   mismatched inventory.

   **Evidence (PASS; TDD PASS).** Commit `be099fd8e`, plus `bd079f568` for literal quoting of an unknown category. The single inventory is `crates/wyrd/wyrd-testing/src/bin/capacity/cases.tsv`. The driver compiles it in (`cases.rs` `REGISTRY`/`CASES`), and `scripts/benchmarks/l1.sh --list-cases` reads it with awk and builds nothing. Test `tests::category_listing_scripts_build_nothing` (`bin/capacity/main.rs:1015`):
   - It copies `l1.sh` and `cases.tsv` into a checkout with no driver and puts `cargo`/`mise` stubs on `PATH`.
   - For each category it runs `bash scripts/benchmarks/l1.sh <category> --list-cases` and asserts stdout equals `Category::list()`. That fixes the selectors, routine/targeted status, `family/id` qualification and order.
   - It asserts that an unknown category or an extra flag exits 2, and that no stub was ever called.
   - Selection parity stays in the existing CLI tests.

   Command: `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::category_listing_scripts_build_nothing)'`.
   - RED: the listing called the cargo stub and the success assertion failed.
   - GREEN: 1 passed.
   - An unquoted error message would have run a category value like `'$(echo pwned)'` as a command. Single quotes fixed it, and the fix was checked by hand.
2. **Actual phase boundaries** — FIND-TASK-001-2; REQ-002/003/AC-003.
   Through existing runner/supervisor lifecycle owners, advance controlled
   time so setup/warmup/measurement/drain consume 490 s, process teardown 10 s
   and PostgreSQL cleanup 20 s. The allowed 520 s lifecycle must complete
   without the old 510 s interruption. Separately make an SDK/process stop
   exceed teardown's 60 s despite unused drain; assert invalidity, an accurate
   teardown overrun and bounded cleanup. Slow real drain must still consume
   drain allowance. RED is premature cutoff or a slow stop accepted as drain.

   **Evidence (PASS; TDD VIOLATED for the supervisor cutoff sub-case).** Commits `84baff618` and `b893418bf`.
   - **Runner.** `Lifecycle::finish` (`experiment.rs:2215`) charges all stop time to teardown. The runner's finish grace is `Lifecycle::teardown_left` (`experiment.rs:2190`), and an overrun records `phase.deadline` naming teardown. Test `experiment::tests::case_stops_spend_teardown_not_unused_drain` (`experiment.rs:7509`) checks three cases:
     - 1 s drains with a 10 s stop: drain 4 s, teardown 10 s, VALID.
     - A 70 s stop: drain stays 4 s, teardown is cut at 60 s, INVALID with `phase.deadline` naming teardown.
     - 10 s drains: 40 s charged to drain and none to teardown, VALID. This slow-drain check was added after the fix, so it only verifies behavior that was already correct.

     Command: `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::case_stops_spend_teardown_not_unused_drain)'`. RED: drain was 14 instead of 4, so the stop had been charged to drain. GREEN: 1 passed. The `experiment::tests::` group also passes: 26 passed.
   - **Supervisor.** `Sweep::attempt` (`sweep.rs`):
     - It cuts off a case that has kept no record at 510 s, which is 590 minus 20 s persist minus 60 s teardown.
     - A case that wrote its record before then is in teardown: `teardown_left` (`sweep.rs:465`) gives its PostgreSQL wrapper the rest of the 60 s teardown allowance after its own process teardown.
     - The stop and the cleanup check are both capped at the 570 s limit, which leaves persistence its 20 s.
     - The wrapper's time is charged to teardown.

     Tests:
     - `sweep::tests::an_admitted_wrapper_teardown_is_charged_to_teardown` (`sweep.rs:1047`, real time): 490 s, then 10 s of process teardown, then 20 s of wrapper. Teardown is 30–32 s, VALID, cleanup confirmed.
     - `sweep::tests::a_recorded_case_keeps_its_teardown_allowance_past_the_cutoff` (`sweep.rs:1237`, paused clock): a recorded case whose wrapper hangs runs past 510 s and is stopped at 559–571 s. INVALID, with the reason `teardown did not finish within its 60s allowance`.

     Command: `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=<name>)'` for each test.
     - GREEN: both pass.
     - RED was never observed for either test. The first passed on its first run, and the second was written after its code.
     - The process violation is recorded here. It is not repaired by inventing a failing run after the fact.
   - **Diagnosis (held commit).** A Wyrd Factory review held the commit:
     - Symptom: a stop at 570 s plus a fresh 60 s teardown could run past the 590 s total.
     - Cause: the cutoff reserved only the persist time.
     - Fix site: `Sweep::attempt`. The cutoff now reserves teardown time, and the stop and cleanup check are capped at the 570 s limit. No other caller computes this limit.
3. **Bounded cleanup failure** — FIND-TASK-001-3; REQ-002/003/AC-002.
   Drive owned external cleanup operations that never reply, reply empty
   after allowance exhaustion, and require termination without an engine
   record. Assert one shared allowance, invalid/unconfirmed cleanup evidence,
   a persisted sweep and NOT RUN next-case rows with no next launch. Use the
   sweep supervisor's actual cleanup path; injecting a precomputed false
   flag alone is insufficient. RED is hanging, renewed grace or late success
   permitting continuation.

   **Evidence (PASS; TDD PASS).** Commit `b893418bf`.
   - Each Docker call in `compose_resources` (`sweep.rs:531`) and `confirm` (`sweep.rs:590`) runs under `timeout_at` with `kill_on_drop`. A timeout is kept as the reason `docker ... did not answer within its allowance`.
   - Stop, removal and the cleanup check share one deadline, which is set once when the stop begins.
   - An empty Docker answer after that deadline counts as "cleanup not confirmed".
   - A sweep whose cleanup is unconfirmed records every later case NOT RUN.

   Tests drive the supervisor's real cleanup path, with a fake wrapper and fake `docker` scripts:
   - `sweep::tests::a_hung_case_is_stopped_at_its_cutoff_within_one_cleanup_allowance` (`sweep.rs:1184`, paused clock): the case never writes a record and ignores TERM, and Docker never answers. Results:
     - total time is 510–571 s and `cleanup.seconds ≤ 60.5`;
     - the reasons name the 510 s deadline, `kept no engine record` and `cleanup was not confirmed`;
     - the rows are [INVALID, NOT RUN] and the sweep record is saved.
   - `sweep::tests::late_or_missing_cleanup_answers_stop_the_sweep` (`sweep.rs:1284`, real time): Docker answers late (2 s) and never (600 s) while only 0.5 s of teardown is left. One case is launched, the rows are [INVALID, NOT RUN], and cleanup is unconfirmed.
   - The existing `sweep::tests::unconfirmed_cleanup_stops_later_launches` still passes.

   Command: `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=<name>)'`.
   - RED, hung case: the supervisor never returned, and the test's outer `timeout` fired (`Elapsed`).
   - RED, late answer: 2 cases were launched instead of 1.
   - GREEN: `sweep::tests::` 8 passed.

   **Diagnosis (hang after the first fix).**
   - Symptom: the paused-clock hung-case test hung under the new code.
   - Evidence: `confirm`'s loop ran again and again without the clock advancing.
   - Cause: at `now == deadline` the loop neither returned nor slept, so it started Docker again on every pass.
   - Fix site: the deadline check in `confirm`. It now returns at `now >= deadline` with an unconfirmed reason, and sleeps only `POLL.min(deadline - now)`.
   - A fresh read-only diagnostician, given only the command, trace and diff, confirmed the same cause and fix site and found no other callers.
4. **Complete saved case evidence** — FIND-TASK-001-4/-6; REQ-003/005/006,
   AC-004. Send recorded timing, scan, counter and gauge snapshots through
   child serialization and supervisor persistence. Remove temporary artifacts;
   read with `capacity report` and assert original window labels/values plus
   honest available persistence timing. Preserve identical regenerated sweep
   output and full case detail. Exercise unsaved and late-saved outcomes with
   their required invalid status/references. RED is lost series, unavailable
   final-phase duration or incorrect persistence outcome. Extend the existing
   saved-record tests and focused connected harness rather than add a second
   reporting system.

   **Evidence (PASS; TDD PASS).** Commit `6414b1434`.
   - **Snapshots.** `Sweep::attempt` reads the child record with the existing `Record::read_with_timings`. `CaseRecord::timings_of` moves each repetition's `TimingEvidence` into the new `CaseRecord.timings`, keyed by step and in repetition order. When a case record is read, `CaseRecord::hydrate` (`summary.rs:153`) moves them back, so the existing `Record::render` prints every series. Schema-3 and L2 records are unchanged.
   - **Persistence time.** `keep_case` times the write. `summary::entry` stores that time as `SweepEntry.persist_seconds` in the sweep record, which is written after the case record, so the case record never claims to time its own write. `CaseRecord::persisted` (`summary.rs:171`) fills `Lifecycle.persist_seconds` from the sweep entry that names the file, for both `read_case` and `capacity report <case.json>`.

   Test `sweep::tests::saved_cases_keep_window_snapshots_and_persistence_time` (`sweep.rs:1089`):
   - Setup: real `wyrd_benchmark` snapshots of one timing, one scan, a labelled counter and a gauge, kept as a child process would keep them. A fake wrapper hands over the engine record, and the real `attempt` runs.
   - After the artifacts are removed, the sweep table is unchanged.
   - The case report lists `repetition 1 server measured window`, `timing bifrost.ingest success`, `scan success`, `l1_saved_counter_total{lane=kept}` and `l1_saved_gauge{lane=kept}`.
   - The report shows `| persist and report | <measured> | 20 |`.
   - The hydrated `TimingEvidence` equals what was kept.

   `sweep::tests::persistence_failures_invalidate_the_row` (`sweep.rs:1352`) adds two cases:
   - a record that could not be kept is INVALID with no reference and `persist_seconds` `None`;
   - a late save is INVALID, keeps its reference and records `persist_seconds` `Some(25.5)`.

   The existing `tests::saved_sweeps_regenerate_their_summary_and_case_reports` (`main.rs:661`) still passes.

   Command: `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=sweep::tests::saved_cases_keep_window_snapshots_and_persistence_time)'`.
   - RED: the run panicked with `measured persistence`, because the entry's persistence time was unavailable. The lost series were never seen failing on their own, because that first assertion came before them.
   - GREEN: 1 passed; the full capacity run gives 110 passed.
   - The connected proof is covered by scenario 6's lane.
5. **No inherited performance targets** — revised REQ-004/006, AC-003/004;
   maintainer-approved scope change, not an independently discovered defect.
   Through the existing new-L1 declaration, judgment and saved-report path,
   exercise measurements that would miss historical thresholds and verify
   those thresholds produce no current target judgment in ordinary or profiled
   runs. Keep aggregate readings and missing-input reporting intact without
   launching omitted cases. Preserve correctness, custody, sample-floor and
   lifecycle failure judgments, and historical record interpretation. Reuse
   existing CLI/report and runner test owners; RED is an inherited target
   still being applied. Already-correct paths need verification only.

   **Evidence (PASS; TDD PASS).** Commits `c27089a96` and `75759c6cf`.
   - In `judge_criterion` (`experiment.rs:3247`), a case-protocol (`aggregate`) `Over::Median` target is inapplicable, with the reason `previous performance targets do not apply to L1 cases`. Sample floors and `Over::Every` correctness, custody and lifecycle checks still judge every window.
   - The summary's Target cell (`summary.rs` `target`) shows only applicable targets, so L1 rows read `none`.
   - The diagnostic label no longer says "targets unchanged".
   - Guidance updated: `mise.toml` `benchmark:l1:*` descriptions and `benchmarks/README.md` (Aggregates and validity, the Target column, `--profile`, the exit section).
   - Historical schema-3 and L2 judgment is unchanged; the existing legacy and calibration tests pass.

   Tests:
   - `experiment::tests::case_targets_are_void_while_integrity_still_invalidates` (`experiment.rs:7607`): a window that would miss the old p99 leaves it inapplicable, with the observed value equal to the aggregate, and the case VALID. A window under its sample floor or with errors is still INVALID.
   - `tests::profiled_sweeps_apply_no_previous_targets_and_keep_capture_evidence` (`main.rs:877`): an ordinary and a profiled sweep at 900 µs, which would miss the old 500 µs target. Both are inapplicable and identical, the row ends `| none | <record> |`, the mode reads `diagnostic (profile cpu; readings are diagnostic)`, and the capture notes are kept.

   Commands: `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::case_targets_are_void_while_integrity_still_invalidates)'` and `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::profiled_sweeps_apply_no_previous_targets_and_keep_capture_evidence)'`.
   - RED, runner: `overhead-slo` was judged `Some(Missed)`.
   - RED, report: the panic at the mode-label assertion (`targets unchanged`).
   - GREEN: both pass.
6. **Integrity-preserving short sweep** — FIND-TASK-001-7; REQ-003/007,
   AC-002. In the shared judge's focused tests, give case-protocol smoke
   present nonzero correctness/custody evidence alongside otherwise healthy
   measurements; assert INVALID with the existing integrity reason and
   inapplicable performance qualification. In the existing connected smoke
   harness, demonstrate that detected bad evidence produces a saved INVALID
   sweep outcome, with normal cleanup/continuation preserved. Keep the existing
   two sequential real-client/server cases and fresh-resource assertions.
   RED is a detected integrity failure reported VALID.

   **Evidence (PASS; focused TDD PASS, connected RED not observed).** Commits `c27089a96` and `f436ada92`.
   - **Judge.** A `Mode::Smoke` record with an aggregate (a case-protocol smoke) now judges its per-window integrity criteria. Performance qualification stays inapplicable. A legacy smoke without an aggregate is still not judged (`judge_criterion`).
   - **Focused test.** `experiment::tests::case_smokes_judge_integrity_but_not_performance` (`experiment.rs:7666`): a case smoke with `errors = 10` and otherwise healthy readings is INVALID through `no-errors`, and `overhead-slo` is inapplicable.
     - Command: `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::case_smokes_judge_integrity_but_not_performance)'`.
     - RED: the smoke came out VALID. GREEN: 1 passed.
   - **Connected test.** `tests::pg_tests::l1_sweeps_isolate_and_record_cases` (`bin/capacity/tests/pg_tests.rs:723`) keeps its two sequential Bifrost cases, its fresh-resource assertions and its failure sweep. It adds a Gateway smoke of `buffered-1kib-capture-metadata.c1,streamed-1kib-capture-metadata.c1` with `--pace 1000000`; the documented pace flag counts every missed arrival as an error. It asserts:
     - both rows are saved INVALID;
     - each kept engine record has a `no-errors` criterion reason;
     - each case's cleanup is confirmed, and the second case still ran;
     - the regenerated table shows both rows as INVALID.
   - **Command.** `mise run test:benchmark:l1`, which runs `cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only -E 'test(=tests::pg_tests::l1_sweeps_isolate_and_record_cases)'` with Docker 29.5.2. GREEN: 1 passed in 284 s.
   - **Connected RED was not observed.** The judge fix was committed before this assertion existed.

   **Diagnosis (first connected run).**
   - Symptom: `pg_tests.rs:919` panicked. The case was INVALID with the reason `repetition 1 read 1996471, missing ≤ 0`, but the assertion looked for the text `no-errors`.
   - Evidence: `judge_criterion` (`experiment.rs:3361-3368`) never puts the criterion id in the message. `Reason::of` keeps it in `criterion`, and `Sweep::finished` copies only `reason.message` into the case record.
   - Cause: the new assertion read the wrong field. The behavior under test was correct.
   - Fix site: that assertion only. It now checks `engine.reasons[].criterion == "no-errors"`, and no runtime code changed.
   - A fresh read-only diagnostician, given the command, trace and diff, reported the same cause and recommended this fix.

For focused Rust assertions confirm the actual names, then use
`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=<confirmed-test-name>)'`
for CLI/sweep/report tests and
`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=<confirmed-test-name>)'`
for runner/judgment tests, with default features and repository setup where
required. Exercise the connected path with `mise run test:benchmark:l1`
and record its actual focused selector. Run Cargo-backed checks sequentially.
Find owning tasks with `mise tasks ls --hidden`; run `mise run fmt:check`,
`mise run lints`, the affected fast owner checks and `git diff --check` after
focused closure. Update existing benchmark guidance only where the corrected
entry point or evidence contract requires it.

**Closure checks**, run one at a time after the final commit:
- `mise run fmt:check`: exit 0.
- `mise run lints`: exit 0. The first run flagged `clippy::single_match_else` in the stop path, fixed in the commit "Satisfy clippy in the L1 case stop path".
- `git diff --check 19fd9cfcf..HEAD`: clean.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`: 110 passed, 3 skipped (Postgres lanes).
- `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^experiment::tests::/)'`: 26 passed.
- `mise run test:benchmark:l1`: 1 passed.

**Diff audit.** Changed files:
- `cases.tsv`, `cases.rs`, `main.rs`, `summary.rs`, `sweep.rs` and `tests/pg_tests.rs` under `crates/wyrd/wyrd-testing/src/bin/capacity/`;
- `crates/wyrd/wyrd-testing/src/experiment.rs`;
- `scripts/benchmarks/l1.sh`, `mise.toml` and `benchmarks/README.md`;
- this task file.

No replacement targets, production waits, new recorders, duplicate registry, L2 policy or historical-record change was introduced.

## Preserved behavior and exclusions

Keep existing case declarations, routine order, fresh resources, workload
clients/providers, durable-ack boundary, correctness and evidence-integrity checks,
historical interpretation and comparison/calibration guards. Do not add
production waits, new recorders, a duplicate registry or general lifecycle
framework. Baseline/comparison methodology and L2 policy remain outside scope.
Full-size feasibility stays unverified while approved AC-005 execution is held;
short harness proof does not establish performance feasibility.
Previous performance targets are void for new L1 cases; replacement targets
remain outside this task. This does not rewrite historical records or L2 policy.

Original TDD compliance remains VIOLATED for scenarios 1–4. Record new closure
work test-first; do not undo correct behavior or manufacture historical RED.
No commit or push is authorized by this task-review artifact.
