# Bifrost write throughput and bounded retry history: execution

Contract: [approved spec revision 1](../spec.md). Selected solution:
[design notes](../design/notes.md), [CPU design](../design/throughput.md), and
[cleanup health](../design/cleanup-health-proposal.md).

| Order | Task | Dependency output |
|---|---|---|
| 1 | [Fence lifecycle](TASK-001-fence-lifecycle.md) | Domain history independent of receipts; accepted-only recovery; safe cleanup and admission health. |
| 2 | [CPU throughput](TASK-002-cpu-throughput.md) | Consumes task 1's accepted-write/retry/recovery behavior and last accepted benchmark checkpoint. |

There are exactly two tasks and three logical scenarios in each. Execute each
scenario test-first, add proof to that entry, then pass its benchmark checkpoint
before continuing. Equivalent tenancy, concurrency and failure variants remain
inside their scenario. No implementation, review, commit or benchmark has been
performed by this plan. Git and execution scheduling remain caller-owned.

## Shared correctness execution

Follow [implementation execution](../../../implementation-execution.md) and
[testing](../../../../TESTING.md). Revalidate current owner code before editing;
paths guide implementation rather than restrict necessary adjacent changes.

New tests have no invented names here. Add/select the test in the stated owner,
confirm its actual nextest name, and record its exact focused command **before
RED**. For a new name, use the real package/target and `-E 'test(=<actual name>)'`;
the brackets are explanatory, not a runnable selector. A package lane alone
does not establish that an individual test ran. Library tests use `--lib`;
integration and journey tests use their manifest's existing `--test` target.

Postgres-backed commands use `mise exec -- scripts/postgres/with-test-postgres.sh
-- bash -lc '...'`, with the same migration setup as the owning mise task.
Use `-P journey --run-ignored=all` for journey tests. The redux integration
target requires its existing `test-support` feature; do not use the unrelated
all-features union. Server integration uses `--features test-support`.
Pure unit tests need no Postgres or server. Keep Cargo-backed commands serial.

Record assertion locations, focused commands, actual RED cause, GREEN result,
benchmark record/comparisons and limitations directly under each scenario.
Already-correct behavior uses verification evidence; do not manufacture RED or
production churn. Preserve every expected result while appending proof.

## Benchmark checkpoint after every scenario

Read [benchmark rules](../../../../benchmarks/AGENTS.md) and its
[command/record reference](../../../../benchmarks/README.md). Establish the matching
original baseline before task 1's first scenario, retaining the exact source state,
host, controls, settings and records. Existing records may supply it only if they
match those controls; otherwise run the following same-method baseline before edits.
Keep matching full-write/read references for the final qualification as well.

Run these three commands **serially**, after focused correctness is GREEN:

```bash
mise run bench:l1:bifrost:writes -- --steps otlp-logs-c64 --allow-dirty
mise run bench:l1:bifrost:hot -- --steps point-memtable-c64 --allow-dirty
mise run bench:l1:bifrost:parquet -- --steps point-c64 --allow-dirty
```

The normal experiment owns repetitions and warmup. Use the same flags for the
baseline and every checkpoint, preserving fixtures, resource settings, compression
and whole-server CPU accounting. No profile, smoke, custom timing reader, added
synthetic load or concurrent build/test/benchmark. These dirty subset runs are
progress evidence, **not qualification**; they belong under `target/benchmarks/`.

Use the supported readers for every record and both comparisons:

```bash
mise exec -- target/profiling/release/capacity report <record.json>
mise exec -- target/profiling/release/capacity compare <original-baseline.json> <record.json>
mise exec -- target/profiling/release/capacity compare <previous-accepted-checkpoint.json> <record.json>
```

Replace placeholders with recorded paths; compare corresponding experiments only.
After scenario 1, compare to the original baseline; thereafter also compare to the
previous accepted checkpoint, including across the task boundary. Paste unedited
reader output and the benchmark rules' Question/Run/Report/Comparison/Answer/Unproven
report into each scenario's evidence or link its saved report there. A valid exit
does not by itself establish a performance target. Inside-spread changes mean
no measured change. Invalid/inconclusive results or material regression block
progression until resolved; do not mask cumulative drift or silently lower 80.

The 80 input MiB/server CPU-second threshold is final CPU-task acceptance, not an
intermediate threshold. At completion, the CPU task runs the full writes, hot,
Parquet and mixed competing-load experiments once, using matching baselines and
the improved full-write calibration. These are additional final qualification,
not additional scenarios. Clean qualification requires a caller-provided clean
candidate; never commit or push merely to make a benchmark clean. Dirty results
cannot substitute for that evidence.

Integrated change review owns complete shipped-surface journey lanes and required
aggregate verification. Tasks run the affected focused tests and owner checks
listed in them, with no redundant broad-gate checklist.
