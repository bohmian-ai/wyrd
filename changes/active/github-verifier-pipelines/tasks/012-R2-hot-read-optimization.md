---
id: TASK-012-R2
status: proposed
approved_spec_revision: 17
depends_on: [TASK-012-R1]
obligations: [REQ-018, REQ-019, REQ-022, REQ-023, REQ-024]
---

# Benchmark UI SQL, then optimize shared hot reads

Add UI-shaped cases to the existing Bifrost L1 harness, record the original
baseline, then optimize shared hot execution against those unchanged queries.

**Design:** [hot-read design notes](../design/task-012-r2-hot-reads.md).
**Continuation:** [memtable timing supplement](012-R2-supplement-memtable-timing.md)
records the baseline pickup state and the ordered remaining work.
**Queries:** [catalog and schedules](../design/ui-sql/README.md),
[exact SQL and binds](../design/ui-sql/queries.json).

| Query family | Concrete SQL |
|---|---|
| Traces: overview, facets, roots and counts | [traces.sql](../design/ui-sql/traces.sql) |
| Verification: execution joins, list, facets and charts | [verification.sql](../design/ui-sql/verification.sql) |
| Logs: list, severity and facets | [logs.sql](../design/ui-sql/logs.sql) |
| Metrics: names, windows, histograms and keys | [metrics.sql](../design/ui-sql/metrics.sql) |
| Paging, nested filters, full details, labels and 24h | [variants.sql](../design/ui-sql/variants.sql) |
| Shared statistics | [statstrip.sql](../design/ui-sql/statstrip.sql) |

These are original builder outputs from pinned source revision
`4dd767d8f4eb5c20bdffea587e3ba2be1d38eb36`, with example parameters and recorded
source hashes. They have not been executed. No UI build, moving branch or
frontend checkout is needed. Query rewriting and frontend work are outside scope.

## Contract and owners

The maintainer selected this scope and baseline-first order on 2026-11-18.
TASK-012-R1 supplies the integrated memory/CPU owners and compatibility records.
Its maintainer-completed status is not reopened.

**Scope:** [revision 17](../spec-revisions/17-hot-read-optimization.md), approved
on 2026-11-18, extends REQ-024 beyond resource-manager integration. Inherited memory, CPU, durability
and no-regression constraints remain. This proposed task is not spec approval;
no new absolute UI latency target or hot/Parquet ratio is agreed.

Extend the existing owners:

- `capacity/query/{mod,isolated,run,telemetry}.rs`: declarations, real server,
  public client, native/OTLP fixtures, placement, snapshots, profiles and history.
- `scribe/memtable.rs`, `scribe/tail_rpc.rs`, `oracle/follower.rs` and
  `oracle/memtable_source.rs`: captured sources, execution, pruning and ownership.
- Existing query memory, allocation helpers, field-ID projection,
  `ReadRequirements` and DataFusion CPU-lane propagation.

## Implement in this order

1. Add first-class L1 `ui-sql`, its `bench:l1:bifrost:ui` selector and membership
   in the full Bifrost L1 aggregate. Reuse existing setup, controls, recording
   and validity rules. Move the catalog into the benchmark fixture owner; add
   typed Arrow answer checking without replacing existing integer cases.
2. Prepare the design's deterministic default/10× evidence and original query
   schedules. Measure identical rows/binds in canonical memtables, staged and
   published residence, plus the separate learned-layout arm. Reconcile actual
   placement and acknowledged rows outside timed windows. Resolve example
   IDs/metric names to real fixture values and freeze those binds before the
   baseline. Positive cases must have known matching input/output cardinalities,
   including populated scalar metrics, histograms and queued verification;
   empty-result cases are separate, explicitly identified negative cases.
3. **Save a full clean, unprofiled VALID baseline before production read edits.**
   Use three repetitions and enough samples per repetition (1,000 for p99;
   200 for p95), with declared adequate windows/budget. Preserve
   corpus/data/host/settings hashes.
   Dirty/subset/profile/smoke runs do not satisfy this checkpoint. The maintainer
   owns any clean source checkpoint; do not commit automatically. Then capture
   a separate diagnostic CPU profile of representative slow hot/Parquet cases
   on the same revision/data, with heap/allocation evidence where memory is a
   material cost. Attribute the gap before production edits; confirm which
   selected changes address the measured costs. Profiles do not replace the
   clean unprofiled baseline. If attribution contradicts the selected approach,
   report the evidence and options before redesigning it.
4. Open retained descriptors/statistics without evaluating all rows. Adapt once
   per generation; filter/project/reconstruct bounded batches on stream demand
   through the existing source, partitions, CPU lanes and memory owners. Keep
   typed-path/zero-copy behavior. Add conservative UTF8/fixed-binary identity
   statistics and account/measure their append and memory costs.
5. Repeat the original experiment unchanged and qualify full existing L1.
   Report residual stage costs; a faster source-open timer alone is insufficient.

No new pools, scheduler, index service, cache, durable projection, source mode,
layout-learning wait, shorter seal interval, schema/migration or dependency.
Preserve exact filters, static/all-match pruning, unknown-bound fallback,
filter/fetch/projection rewrites and volatile/dynamic behavior. Sorted top-N may
still consume all qualifying input. Keep signed capture identities, pinned
cuts, publication deduplication, field-ID schema evolution and real resource
custody through cancellation/late children. Late failures use the existing
failed terminal, never a fresh-state reopen or successful partial result.

## Ordered acceptance scenarios and evidence

Add actual assertions, exact focused commands, observed RED/GREEN and records
in these entries. Do not create a second evidence ledger. Existing correct
behavior keeps its existing proof; no manufactured RED is required for it.

1. **Typed original queries.** Ingest a reconciled canonical fixture; the real
   Bifrost client runs catalog trace/Verification lists with parameters and
   consumes their terminal. Assert exact ordered IDs/timestamps/nulls and types;
   extend to decimals/Variant and remaining families. Wrong answers, truncation,
   degradation or incomplete terminals invalidate the measurement. Use fixture
   distributions with deterministic approximate-aggregate expectations.
   Assert known matching-input and output cardinalities for positive cases,
   not merely a non-empty aggregate row: a COUNT over no matches is insufficient.
   Require populated scalar/histogram series and queued execution cases with
   their expected values. Preserve legitimate zero values; exercise intended
   empty results separately. Wrong example bindings must fail correctness.
   **RED:** missing scenario/integer-only checker cannot deliver these results.
   **Tier:** checker unit plus real-server integration/journey.
   **Authority:** REQ-018/024; Bifrost Query/Variant SQL/Public surface.
   Evidence: **checker units GREEN; real-server smokes VALID with 0 wrong answers.**
   - Units (`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(/query::ui::/)'`, GREEN):
     `answer::tests::{typed_answers_compare_ids_timestamps_decimals_and_nulls,ties_compare_as_multisets_within_their_keys,variants_compare_by_value_not_layout}`,
     `tests::{frozen_catalog_holds_the_original_statements,tied_statements_declare_the_order_their_sql_leaves}`,
     `fixture::tests::every_fourth_root_invokes_an_agent` (RED `left: 0, right: 125`),
     `seed::tests::driven_statements_must_count_what_was_driven`, and
     `seed::tests::journey_statements_must_answer_what_was_exported` (RED
     `seed.rs:341 assertion failed: check("stat-spans-total", &answer("value", &["0"]), journey).is_err()`).
     They prove the checker and the known counts (`seed::known`, `Journey::known`), not that
     the server answers them.
   - Smokes, each run once at `6fc40b230`: `mise run bench:l1:bifrost:ui --smoke`
     (`target/benchmarks/smoke/bifrost/ui-sql/20261119T005053Z-6fc40b230.json`),
     `mise run bench:l1:bifrost:ui:10x --smoke` (`…/ui-sql-10x/20261119T010050Z-6fc40b230.json`),
     `mise run bench:l1:bifrost:ui:learned --smoke` (`…/ui-sql-learned/20261119T010156Z-6fc40b230.json`).
     `capacity report` of each: `verdict: VALID (exit 0)`, and for all six steps
     `exact-answers: wrong_results ≤ 0 queries every rep | 0 queries spread 0`. Earlier
     runs were withdrawn on 2026-11-18 (started from a retry loop and read with `jq`,
     contrary to `benchmarks/AGENTS.md`) and their records deleted.
2. **Same data, different residence/layout.** Execute fixed SQL/absolute binds
   before/after explicit stage/publication, then on separately prelearned data.
   Assert actual residence, identical logical rows and no duplicate identities;
   cover mixed sources, publication overlap, schema null-fill and extensions.
   No timed interval includes ingest/placement drift. **RED:** current hot setup
   prelearns and lacks this paired experiment. **Tier:** placement integration
   and public query journey. **Authority:** REQ-018/024; immutable source and
   nested-field query rules. Evidence: **not run.** Placement and the learned arm are
   implemented (`Ui::place`, `ui-sql-learned`, `learn_span_layout`) but unproven.
   Real-server runs cited here before were withdrawn on 2026-11-18: they were started
     from a retry loop around the `mise run bench:*` tasks and read with `jq`, contrary to
     `benchmarks/AGENTS.md`; their records were deleted, and real-server evidence awaits
     compliant runs.
3. **Qualified original baseline.** Run the original page schedules at one/eight
   pages and default/10× volume, with representative standalone/layout cases.
   Record primary/full-result times, per-query latency, correct pages/s, first
   request, first batch, CPU/memory and scan facts. Warm equally per residence.
   Missing samples/timing/source facts, wrong residence/results or failed
   terminals make the measurement INVALID. Save the clean full baseline before
   scenario 4's production edits. A separate diagnostic profile on the baseline
   revision must explain the material hot/Parquet cost contributors before
   optimization: distinguish CPU, waits, layout, scan work and allocation costs
   using the existing stage observations. Add heap/allocation evidence when
   needed. Record the attribution and its link to the selected source changes;
   a profile is diagnostic evidence, never qualification. Contradictory evidence
   requires reporting options before redesign. **RED:** declaration/runner/
   evidence absent.
   **Tier:** declaration/record tests and opt-in real-server L1.
   **Authority:** REQ-018/024; benchmark Measurement/Verdicts/Records.
   Evidence: **declaration GREEN; clean full baseline VALID.**
   - `tests::level_one_declares_complete_engine_coverage` (RED against the old 69-step
     matrix and on `ui-sql-10x is an extended L1 experiment`, then GREEN):
     `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_one_declares_complete_engine_coverage)'`.
     `workload::tests::short_percentiles_are_inconclusive` GREEN.
   - **TDD process violation:** the `diagnose` sample-floor helper was written before
     `short_percentiles_are_inconclusive`.
   - `mise run bench:l1:bifrost:ui`, run once at `6fc40b230` on a clean tree: `benchmarks/history/bifrost/ui-sql/20261119T010259Z-6fc40b230.json`,
     `verdict: VALID (exit 0)`, 0 wrong answers in every step. From `capacity report`:
     ```
     | memtable-ui-c1 | clients=1 residence="memtable" | 77.16 requests/s; 20.579 queries/s/vCPU; 156.948 queries/cpu-s; 6.372 ms/query | (3.15, 3.152] / (22.348, 22.364] / (24.658, 24.674] (client 9.767 / 26.784 / 28.704) | 0 total (median 0) |
     | memtable-ui-c8 | clients=8 residence="memtable" | 336.279 requests/s; 89.652 queries/s/vCPU; 112.681 queries/cpu-s; 8.875 ms/query | (4.506, 4.51] / (41.714, 41.746] / (55.443, 55.476] (client 16.324 / 61.49 / 80.901) | 0 total (median 0) |
     | staged-ui-c1 | clients=1 residence="staged" | 83.098 requests/s; 22.126 queries/s/vCPU; 157.061 queries/cpu-s; 6.367 ms/query | (3.34, 3.342] / (15.729, 15.737] / (18.448, 18.465] (client 10.64 / 20.68 / 23.058) | 0 total (median 0) |
     | staged-ui-c8 | clients=8 residence="staged" | 350.88 requests/s; 93.568 queries/s/vCPU; 111.683 queries/cpu-s; 8.954 ms/query | (5.853, 5.857] / (29.18, 29.196] / (35.258, 35.291] (client 21.964 / 39.454 / 44.016) | 0 total (median 0) |
     | published-ui-c1 | clients=1 residence="published" | 91.861 requests/s; 24.453 queries/s/vCPU; 189.615 queries/cpu-s; 5.274 ms/query | (2.847, 2.849] / (15.688, 15.696] / (18.498, 18.514] (client 9.145 / 20.033 / 21.324) | 0 total (median 0) |
     | published-ui-c8 | clients=8 residence="published" | 423.238 requests/s; 112.906 queries/s/vCPU; 138.973 queries/cpu-s; 7.196 ms/query | (4.567, 4.571] / (26.411, 26.427] / (32.342, 32.358] (client 17.244 / 36.134 / 39.779) | 0 total (median 0) |
     ```
   - `mise run bench:l1:bifrost:ui:10x`, run once at `6fc40b230` on a clean tree: `benchmarks/history/bifrost/ui-sql-10x/20261119T010539Z-6fc40b230.json`,
     `verdict: VALID (exit 0)`, 0 wrong answers in every step. From `capacity report`:
     ```
     | memtable-ui-c1 | clients=1 residence="memtable" | 48.416 requests/s; 12.935 queries/s/vCPU; 104.352 queries/cpu-s; 9.583 ms/query | (6.734, 6.738] / (31.949, 31.965] / (36.012, 36.045] (client 15.996 / 38.877 / 40.527) | 0 total (median 0) |
     | memtable-ui-c8 | clients=8 residence="memtable" | 198.195 requests/s; 53.045 queries/s/vCPU; 72.932 queries/cpu-s; 13.711 ms/query | (11.739, 11.747] / (57.672, 57.704] / (91.423, 91.488] (client 27.117 / 99.115 / 120.894) | 0 total (median 0) |
     | staged-ui-c1 | clients=1 residence="staged" | 65.842 requests/s; 17.564 queries/s/vCPU; 133.553 queries/cpu-s; 7.488 ms/query | (4.764, 4.768] / (23.74, 23.757] / (27.181, 27.197] (client 13.266 / 29.556 / 31.147) | 0 total (median 0) |
     | staged-ui-c8 | clients=8 residence="staged" | 256.076 requests/s; 68.373 queries/s/vCPU; 85.407 queries/cpu-s; 11.709 ms/query | (9.535, 9.544] / (42.762, 42.795] / (55.149, 55.181] (client 28.905 / 60.145 / 65.45) | 0 total (median 0) |
     | published-ui-c1 | clients=1 residence="published" | 72.218 requests/s; 19.257 queries/s/vCPU; 158.866 queries/cpu-s; 6.295 ms/query | (3.942, 3.944] / (23.446, 23.462] / (26.968, 26.984] (client 11.273 / 28.921 / 30.725) | 0 total (median 0) |
     | published-ui-c8 | clients=8 residence="published" | 311.867 requests/s; 83.295 queries/s/vCPU; 104.416 queries/cpu-s; 9.577 ms/query | (6.844, 6.849] / (42.303, 42.336] / (51.872, 51.905] (client 20.672 / 55.492 / 59.801) | 0 total (median 0) |
     ```
   - `mise run bench:l1:bifrost:ui:learned`, run once at `6fc40b230` on a clean tree: `benchmarks/history/bifrost/ui-sql-learned/20261119T010926Z-6fc40b230.json`,
     `verdict: VALID (exit 0)`, 0 wrong answers in every step. From `capacity report`:
     ```
     | memtable-ui-c1 | clients=1 residence="memtable" | 75.999 requests/s; 20.24 queries/s/vCPU; 150.971 queries/cpu-s; 6.624 ms/query | (3.467, 3.469] / (22.774, 22.79] / (24.838, 24.855] (client 9.656 / 26.974 / 29.87) | 0 total (median 0) |
     | memtable-ui-c8 | clients=8 residence="memtable" | 320.676 requests/s; 85.545 queries/s/vCPU; 107.028 queries/cpu-s; 9.343 ms/query | (5.239, 5.243] / (43.319, 43.352] / (57.508, 57.541] (client 17.058 / 63.132 / 87.731) | 0 total (median 0) |
     | staged-ui-c1 | clients=1 residence="staged" | 82.313 requests/s; 21.917 queries/s/vCPU; 150.722 queries/cpu-s; 6.635 ms/query | (3.752, 3.754] / (15.884, 15.892] / (18.645, 18.661] (client 10.729 / 21.07 / 22.561) | 0 total (median 0) |
     | staged-ui-c8 | clients=8 residence="staged" | 342.713 requests/s; 91.439 queries/s/vCPU; 108.289 queries/cpu-s; 9.235 ms/query | (6.828, 6.832] / (29.737, 29.753] / (35.586, 35.619] (client 22.649 / 40.445 / 45.241) | 0 total (median 0) |
     | published-ui-c1 | clients=1 residence="published" | 91.639 requests/s; 24.41 queries/s/vCPU; 189.282 queries/cpu-s; 5.283 ms/query | (2.865, 2.867] / (15.72, 15.729] / (18.53, 18.547] (client 9.223 / 20.519 / 21.625) | 0 total (median 0) |
     | published-ui-c8 | clients=8 residence="published" | 420.278 requests/s; 112.012 queries/s/vCPU; 136.823 queries/cpu-s; 7.309 ms/query | (4.612, 4.616] / (27.181, 27.197] / (33.194, 33.21] (client 17.159 / 36.641 / 40.279) | 0 total (median 0) |
     ```
   - Each report states `scanned_bytes_per_query` and `scanned_rows_per_query` as "an answered
     query has no executed-plan scanned bytes/rows": the scan facts are absent.
   - Earlier baseline records were withdrawn on 2026-11-18 (retry loop, `jq`) and deleted.
4. **Work follows demand.** Open a captured multi-batch canonical/learned source
   without pulling: row expressions/reconstruction have not run. Pull through
   its actual DataFusion source and observe bounded work/results. Where the
   plan can stop early, drop avoids later evaluation. Covered paths avoid root
   reconstruction; large appends remain bounded execution units. **RED:** eager
   capture opening computes results first. **Tier:** source integration and
   connected query journey. **Authority:** REQ-019/022–024; demand-driven live
   streams/nested reads. Evidence: not run.
5. **Pruning remains exact.** Run catalog ID/time/child-match filters over batches
   with complete, missing, all-null and schema-adapted bounds. Prove exclusions
   safe, unknowns conservative, native ordering/null semantics and pushdown
   interactions unchanged. Measure new statistics' retained bytes/append work.
   **RED:** string/binary bounds absent; retain primitive fallback regressions.
   **Tier:** statistics/source integration plus public results from 1–2.
   **Authority:** REQ-019/023/024; field-ID/pruning/resource rules.
   Evidence: not run.
6. **Lifecycle and terminal safety.** Through local and remote follower paths,
   drop/cancel/expire, exhaust query memory, fail a later expression or lose an
   accepted capture. No future batch evaluation or fresh-state reopen occurs.
   Failures cannot become successful partial answers. Capture/output/workspace
   charges and staged leases follow actual owners, including late children,
   and release once. **RED:** eager work cannot be cancelled after it already
   ran before demand; preserve unchanged security/terminal regression proofs.
   **Tier:** owner integration and real Oracle/Scribe journeys; retain affected
   Rust/Python/TypeScript/HTTP/MCP terminal coverage. **Authority:**
   REQ-019/022–024; captured streams/resource/failure rules. Evidence: not run.
7. **End-to-end improvement without tradeoffs.** Compare the unchanged original
   corpus and method with scenario 3, all three repetitions and residences.
   Demonstrate improvement in affected complete hot paths, reporting server/
   client latency, full bundles, throughput, CPU, memory, refusals and scans.
   Attribute planning/admission, opening, adaptation, filtering/projection/
   reconstruction, execution and decode; never add nested times or subtract
   percentiles. For every remaining material contributor to the shared-read
   slowdown, record its stage/owner, supporting measurements, before/after cost
   and whether it was addressed or remains unresolved. Do not claim completion
   from a small improvement while material lag remains unexplained. Explicitly
   unresolved material work keeps the task incomplete unless the maintainer
   accepts the evidenced limit; it does not authorize silent scope expansion.
   A VALID measurement alone does not establish task completion.
   Qualify full clean Bifrost,
   Gateway and Verification L1 against newest compatible history: no correctness,
   memory, refusal or write-performance regression. Keep gateway-payload/r3000
   goals visible without claiming this task alone achieves them. A failed
   approach needs evidence/options before redesign, not completion.
   **Tier:** opt-in unprofiled qualification; profiles/subsets are diagnostic.
   **Authority:** REQ-018/024; existing benchmark comparison rules.
   Evidence: not run.

## Verification

Use existing owner modules, redux's integration target and `wyrd-testing`'s
Oracle/Scribe targets; no binary per scenario. Record actual new test names and
exact focused nextest commands before RED. Existing owner commands:

```sh
mise run test:bifrost:integration:redux
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:scribe
mise run fmt
mise run lints
mise run bench:l1:bifrost
mise run bench:l1:gateway
mise run bench:l1:verification
```

Add the UI experiment selector before using it for the baseline. Run additional
existing terminal-surface lanes only where affected proof is otherwise missing.
No default L2, broad gate, dev stack or frontend build. Do not mutate another
checkout, commit, or weaken validity criteria to qualify.

Planning check: independent comprehension and the focused recheck established
scope, baseline ordering and per-repetition sampling. SQL extraction/document
links are authoring checks, not
implementation or performance evidence; scenario evidence remains unrun.
