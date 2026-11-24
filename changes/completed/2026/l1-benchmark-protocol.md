# Standardized L1 benchmark sweeps

- Change: `l1-benchmark-protocol`
- Specification: `SPEC-l1-benchmark-protocol`, approved revision 2
- Completed: 2026-11-22
- Reviewed target: `3b0294c45`, holding the TASK-001-R2 snapshot whose independent task review passed. The maintainer approved completion in place of a final change review.
- Delivery reference: not supplied

L1 is the benchmark run most often. Each case now runs in its own fresh environment with its own lifecycle and record, so its result no longer depends on which other cases ran before it. One command per category runs a sweep, and every sweep ends with one Markdown table that can be regenerated from the saved records.

## Shipped behavior

- `mise run benchmark:l1:verification`, `benchmark:l1:gateway` and `benchmark:l1:bifrost` all run `scripts/benchmarks/l1.sh <category>`.
  - By default they run the routine cases in declared order: 5 Verification, 5 Gateway and 18 Bifrost (8 `writes-parquet` writes, then 3 `hot`, 4 `parquet` and 3 `ui-sql` reads).
  - `-- --list-cases` lists every selector with routine or targeted status and builds nothing.
  - `-- --cases a,b` runs exactly those cases in that order, and an unknown or ambiguous selector fails before any case starts.
  - `-- --profile cpu|heap|both` marks every reading diagnostic.
  - One registry, `cases.tsv`, feeds both the listing script and the driver.
- Each case is its own `capacity case` process. It runs under the PostgreSQL wrapper with its own Compose project and temporary storage root, and runs one engine step on the case clock: a 30 s warmup and three 30 s measured windows.
- **Phase allowances:** setup 240 s, warmup 30 s, measurement 90 s, drain and capture 150 s shared, teardown 60 s, persistence 20 s; 590 s in all.
  - The case announces its teardown before it stops any process. From that moment one teardown deadline, at most 570 s into the case, governs process stop, wrapper disposal, any forced stop and cleanup confirmation.
  - A case that has not begun teardown by 510 s is stopped.
  - A missed allowance, failed integrity check, or missing lifecycle or measurement evidence makes the case INVALID.
- Cleanup counts as confirmed only when every owned process, container, volume and storage root is gone before the teardown deadline. Cleanup that finishes later is recorded but not confirmed. An invalid case with confirmed cleanup is followed by the next case. Unconfirmed cleanup or an interrupt records every later case NOT RUN with its reason.
- Each attempted case keeps a versioned JSON case record with identity, phase timings, window and aggregate evidence, full timing snapshots, drain and cleanup outcomes, and a verdict. The sweep keeps `sweep.json`: the invocation, mode, ordered selection, outcomes, and either record references or reasons. Both live under `target/benchmarks/l1/<category>/<sweep>/`; smoke sweeps go under `target/benchmarks/smoke/l1/`.
  - Persistence that fails or exceeds 20 s makes the row INVALID. The measured persistence time is kept in the sweep.
- The sweep ends with one Markdown table: one row per case with status, throughput, server CPU per unit, a labeled headline latency, Target `none` and the record reference. Bifrost also has an input column.
  - `capacity report <sweep.json>` reproduces the table after temporary artifacts are deleted.
  - `capacity report <case.json>` renders the full case report.
- `mise run test:benchmark:l1` runs two-case smoke sweeps through the real supervisor, wrapper, Docker and PostgreSQL. It judges workflow and evidence integrity only.

## Invariants and constraints

- A case's declaration and preparation are identical whether it runs alone, in a subset or in a reordered sweep. No server, database, storage, client, SDK or provider state outlives the case.
- Rates and efficiency are ratios of the three windows' summed numerators and denominators. Latency percentiles pool the windows' histograms. Unavailable and inapplicable readings are never shown as zero.
- No previous performance target applies to L1 cases, and none replaces them. Correctness, custody, sample-floor and lifecycle checks still judge every window, including in smoke runs.
- Allowances are maxima, never sleeps. No stage renews the teardown deadline, and late physical cleanup never authorizes another launch.
- Case-protocol records never enter L2 comparison or calibration paths. Historical records keep their original interpretation.

## Decisions and deviations

- Each case gets its own full environment. The alternatives (resetting counters, fixing individual publication barriers, or starting only a fresh server) were rejected because they leave PostgreSQL, clients or background work shared.
- The short workflow check moved from short L1 timing into the harness test lane.
- Revision 2: the maintainer voided all previous performance targets for the refactored cases.
- The teardown lifecycle was corrected in review remediation:
  - R1 bounded cleanup inside the envelope and charged process stops to teardown.
  - R2 replaced the record-based phase guess with the runner's teardown announcement and one shared deadline.
- **Deferred:** comparison methodology, performance-change qualification and L2 calibration policy. L1 case records have no comparison reader yet.
- **Records policy, decided at completion:** the maintainer directed that L1 sweep records are recorded on the `benchmark-records` branch, rejecting the earlier "L2-only history" rule. [Benchmark rules](../../../benchmarks/AGENTS.md) and the README History section still need that update.
- TDD history was disclosed as violated for some R1 sub-cases; the maintainer accepted this as non-blocking.

## Acceptance closure

- **AC-001:** registry, listing, selection, ordering and profiled labeling are covered by inline capacity tests, including the build-free listing check.
- **AC-002:** fresh-resource, invalid-but-confirmed continuation, unconfirmed-cleanup stop and interrupt behavior are covered by inline sweep tests on a held paused clock, and by the connected `test:benchmark:l1` lane (passed in 221 s).
- **AC-003:** late work versus window totals, aggregate rates, pooled latency, integrity invalidation and no-target output are covered by runner and capacity unit tests.
- **AC-004:** a saved sweep and case reports regenerate after artifact removal, and INVALID, NOT RUN, profiled and legacy statuses are preserved, by saved-sweep tests.
- **AC-005:** full-size feasibility on benchmark hosts is **unverified**. No full-size sweep has run, and the first official baseline run is the pending check.

At the reviewed snapshot, 112 capacity tests and 27 runner tests passed, and format, lints and `git diff --check` were clean.

## Current owners

- [Benchmark guide](../../../benchmarks/README.md) and [benchmark rules](../../../benchmarks/AGENTS.md); category tasks in [mise.toml](../../../mise.toml) and [the L1 script](../../../scripts/benchmarks/l1.sh).
- [Case registry](../../../crates/wyrd/wyrd-testing/src/bin/capacity/cases.tsv) and [selection](../../../crates/wyrd/wyrd-testing/src/bin/capacity/cases.rs).
- [Sweep supervisor and case process](../../../crates/wyrd/wyrd-testing/src/bin/capacity/sweep.rs) and [records and summary](../../../crates/wyrd/wyrd-testing/src/bin/capacity/summary.rs).
- [Runner lifecycle and allowances](../../../crates/wyrd/wyrd-testing/src/experiment.rs).
- Durable acknowledgement for writes: [Bifrost durability and visibility](../../../architecture/bifrost-design.md#durability-and-visibility).
- Connected harness: [pg_tests](../../../crates/wyrd/wyrd-testing/src/bin/capacity/tests/pg_tests.rs).
