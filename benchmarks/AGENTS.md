# benchmarks

Benchmarks are opt-in experiments run by the `capacity` harness in
`crates/wyrd/wyrd-testing`. [README.md](README.md) owns the commands, flags,
records, and readers. These rules keep every run and every reading the same,
whoever runs it.

## Running

- ALWAYS run through a `mise run bench:*` task, narrowed with its documented
  flags: `--steps`, `--profile [cpu|heap|both]`, `--pace`, `--smoke`, and
  `--allow-dirty`.
- NEVER start a run by invoking `capacity` or `wyrd-server` directly, copy
  binaries into scratch directories, or wrap a run in your own script or
  loop. The `capacity report`, `capacity compare` and `capacity profile`
  readers are the only direct invocations. Each
  repetition count, warmup, and step order comes from the plan.
- To compare builds, run the same task with the same flags in a worktree
  checked out at each commit. To compare code variants, measure each variant
  as its own dirty tree (`--allow-dirty`), one at a time.
- Run on a quiet host: no other build, test, or benchmark on the machine,
  whoever owns it. Never generate synthetic host load.
- Check existing records first; do not rerun to answer a question a record
  already answers.

## Reading results

- ALWAYS report a run by pasting its `capacity report <record.json>` output,
  then add interpretation below it. Compare two runs with
  `capacity compare <before.json> <after.json>` and paste its output as the
  Comparison; `mise run bench:results` charts a metric across history.
- `capacity report` shows every series the server records for each step. A
  value the run keeps but the report omits is a reader defect: extend the
  reader.
- Explain a result from the report's per-statement diagnostics and server
  series first. Profile only when they cannot name the cost.
- Memory growth: start with the record's opening and peak resident memory,
  then `--pace`, which adds resident and governed memory by owner after each
  repetition drains. Use heap captures only when those readings cannot
  name the owner.
- Profiled runs: run and read them exactly as
  [Profiling](README.md#profiling) shows: CPU captures with
  `capacity profile`, heap captures with `pprof` and its three
  listed commands. No other reader.
- NEVER parse records, timing snapshots, server logs, or captures with your
  own scripts, decoders, or hand-built tables.

## Reporting to the maintainer

Every benchmark report, from any implementor, has these sections in this
order and nothing else:

1. **Question.** One sentence: what the run was meant to answer.
2. **Run.** The exact `mise run bench:*` command, the commit and dirty
   state, the record path, and the verdict (`VALID` or `INVALID`, with its
   reasons).
3. **Report.** The `capacity report <record.json>` output, pasted
   unedited. For a profiled run, also the `capacity profile` output for the
   frames the answer relies on.
4. **Comparison.** The named earlier record it is compared against (same
   task, same flags, same host), and for each metric the answer relies on:
   before, after, change, and whether the change is outside the
   repetitions' spread: the `capacity compare <before.json> <after.json>`
   output, pasted unedited. Compare only VALID records.
5. **Answer.** The conclusion in plain words, tied to the numbers above.
6. **Unproven.** What the run does not show, and what would show it.

Rules:

- Never put a number in the report that is not in a pasted report or record.
- A difference inside the repetitions' spread is "no measured change".
- A profiled, `--steps`, `--pace`, or `--smoke` run is diagnostic: say so in
  the answer, and never present it as a qualification.
- When a run is INVALID, report that first and do not interpret its numbers.

## Adding a reading

Readings exist to debug and explain a result first. Before adding one, check
whether an existing reading already answers the question.

- Server-side values go through the existing `metrics` facade, which the
  `bench` server routes to the `wyrd-benchmark` recorder. Explicit request
  timings use `wyrd_benchmark::record_timing`. Never add another recorder,
  exporter, sidecar log, or ad hoc timer.
- A benchmark-only metric or timing compiles only under the `bench`
  feature. A shipped build keeps only operational metrics, and a production
  metric follows its owner's telemetry rules (for Bifrost, the Telemetry
  section of `architecture/bifrost-design.md`): closed, bounded labels; no
  tenant, id, path, or SQL labels.
- In the harness, declare an unjudged reading as a diagnostic observation
  (`Metric::diagnostic` in `wyrd-testing`) with its boundary and
  population. Judge a value only through a declared criterion.
- Each new reading ships with a test in its owner and its entry in
  [README.md](README.md), and appears in `capacity report`. If the record
  holds the value but no reader shows it, extend the reader. Never write a
  one-off script.

## Records

Only a clean, unprofiled, full run enters `benchmarks/history/`, which is the
`benchmark-records` branch. Every other run stays under `target/benchmarks/`.
Never commit records on a code branch.
