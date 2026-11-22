# benchmarks

Benchmarks are opt-in experiments run by the `capacity` harness in
`crates/wyrd/wyrd-testing`. [README.md](README.md) owns the commands, flags,
records, and readers. These rules keep every run and every reading the same,
whoever runs it.

## Running

- ALWAYS run L1 through its category task, `mise run
  benchmark:l1:<verification|gateway|bifrost>`, narrowed only with its
  documented flags: `--list-cases`, `--cases <id>[,<id>...]`,
  `--profile [cpu|heap|both]`, `--allow-dirty`, and Gateway's `--pace`.
  Run L2 through `mise run bench:l2:<engine>` with its documented flags
  (`--steps`, `--profile`, `--smoke`, `--allow-dirty`).
- Use the L1 vocabulary: a **case** is one declared workload, fixture and
  load; a **sweep** is one category invocation over selected cases; a
  **phase** is part of a case's lifecycle; a **window** is one measured
  admission interval. Find a selector with `--list-cases` before naming it.
- NEVER start a run by invoking `capacity` or `wyrd-server` directly, copy
  binaries into scratch directories, or wrap a run in your own script or
  loop. The `capacity report`, `capacity compare` and `capacity profile`
  readers are the only direct invocations. Each case's warmup, windows
  and phase allowances, and the order of a sweep, come from the plan.
- To compare builds, run the same task with the same flags in a worktree
  checked out at each commit. To compare code variants, measure each variant
  as its own dirty tree (`--allow-dirty`), one at a time.
- Run on a quiet host: no other build, test, or benchmark on the machine,
  whoever owns it. Never generate synthetic host load.
- Check existing records first; do not rerun to answer a question a record
  already answers.

## Reading results

- ALWAYS report an L1 sweep by pasting the Markdown table it ends with
  verbatim (or `capacity report <sweep.json>`, which renders the same
  table), then add interpretation in prose below it. Read a case's full
  evidence with `capacity report <case-record.json>`.
- Report an L2 run by pasting its `capacity report <record.json>` output.
  Compare two L2 runs with `capacity compare <before.json> <after.json>`
  and paste its output as the Comparison; `mise run bench:results` charts a
  metric across history. L1 case records have no comparison reader yet.
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
2. **Run.** The exact `mise run benchmark:l1:*` or `mise run bench:l2:*`
   command, the commit and dirty state, the sweep or record path, and the
   verdict (each case's status for a sweep; `VALID` or `INVALID` with its
   reasons for an L2 record).
3. **Report.** For an L1 sweep, its summary table, pasted verbatim; for an
   L2 run, the `capacity report <record.json>` output, pasted unedited. For
   a profiled run, also the `capacity profile` output for the frames the
   answer relies on.
4. **Comparison.** For an L2 run, the named earlier record it is compared
   against (same task, same flags, same host), and for each metric the
   answer relies on: before, after, change, and whether the change is
   outside the repetitions' spread: the `capacity compare <before.json>
   <after.json>` output, pasted unedited. Compare only VALID records. For an
   L1 sweep, write "not available": the L1 comparison methodology is not
   defined yet.
5. **Answer.** The conclusion in plain words, tied to the numbers above.
6. **Unproven.** What the run does not show, and what would show it.

Rules:

- Never put a number in the report that is not in a pasted report or record.
- A difference inside the repetitions' spread is "no measured change".
- A profiled or `--pace` L1 sweep, and a profiled, `--steps`, `--pace` or
  `--smoke` L2 run, is diagnostic: say so in the answer, and never present
  it as a qualification. An L1 `--cases` selection is not diagnostic: it
  changes coverage, which the table's Coverage line states.
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

Only a clean, unprofiled, full L2 run enters `benchmarks/history/`, which is
the `benchmark-records` branch. Every L1 sweep keeps its sweep and case
records under `target/benchmarks/l1/<category>/<sweep>/`, and every other run
stays under `target/benchmarks/`. Never commit records on a code branch.
