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
   Evidence: **PARTIAL.**
   - Checker units: `query::ui::answer::tests::{typed_answers_compare_ids_timestamps_decimals_and_nulls,
     ties_compare_as_multisets_within_their_keys,variants_compare_by_value_not_layout}` and
     `query::ui::tests::{frozen_catalog_holds_the_original_statements,tied_statements_declare_the_order_their_sql_leaves}`
     (`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(/query::ui::/)'`; GREEN).
     Their RED runs predate this record and are not reconstructed.
   - Real server: `Workload::prepare` in `capacity/query/ui/workload.rs` answers all 69
     statements in catalog order and fails on any failed terminal; every timed answer is
     compared with its typed reference (`exact-answers` criterion).
   - Known counts, `capacity/query/ui/seed.rs::{known,check}`, unit
     `query::ui::seed::tests::driven_statements_must_count_what_was_driven`. Prepare seeds
     100 metadata-captured gateway calls to the local mock provider (canned usage of 11 + 4
     tokens; a fake local price so `cost` is filled) and 60 direct plus 40 submitted
     assertion verifications, every odd one failing. It binds those statements to the seeding
     window and requires calls 100, tokens 1,500, cost 0.0054 USD, 51 listed executions,
     60 completed / 40 pending, 50 passed / 50 failed and 50 failed-filter rows.
     `query::ui::fixture::tests::every_fourth_root_invokes_an_agent`: RED `left: 0, right: 125`,
     then GREEN; `stat-invocations-*` must count `Journey::invocations` (125; 1,250 at 10×).
   - RED (`mise run bench:l1:bifrost:ui -- --smoke --allow-dirty` before seeding): prepare
     failed naming every verify-* and stat-{calls,tokens,cost}-* statement (for example
     `verify-list answers 0 rows`, `stat-calls-total answers [["0"]]`).
     GREEN: `target/benchmarks/smoke/bifrost/ui-sql/20261118T202920Z-e24285ae5.json`, VALID.
   - Negative case: `verify-filter-status` (bind `waiting`) must answer no row. The server
     writes only `checkpoint` execution events, so the catalog's `event = 'status'` subquery
     never matches and every submitted execution reads `pending`. This is a finding about the
     frozen SQL, recorded rather than rewritten (query rewriting is out of scope).
   - Journey statements, `Journey::known` in `capacity/query/ui/fixture.rs`, unit
     `query::ui::seed::tests::journey_statements_must_answer_what_was_exported`: RED
     `seed.rs:341 assertion failed: check("stat-spans-total", &answer("value", &["0"]), journey).is_err()`,
     then GREEN. Every trace, log and metric statement over the hour must answer what the
     model exported: span, error, trace and log counts and minute bins; facet values and counts;
     list, cursor-page and child-match order by trace, span and log identity; metric names; the
     newest points' integer values; the 1 h, 24 h, per-series and nested charts' counter rate,
     delta and gauge totals; and the histograms' counted requests and summed milliseconds.
     The approximate percentiles keep only their recorded answer. Group keys assert the exact set of
     series attributes (every route, `/batch` and `/gauge`), not a row count: the series table kept 100+ rows in the default arm and
     80 in the learned arm for the same 8 series.
     Smokes with these checks (VALID): `ui-sql/20261118T215246Z-eeb58ed9b.json`,
     `ui-sql-10x/20261118T215346Z-eeb58ed9b.json`,
     `ui-sql-learned/20261118T215447Z-eeb58ed9b.json`.
   - **Open:** none for the baseline.
2. **Same data, different residence/layout.** Execute fixed SQL/absolute binds
   before/after explicit stage/publication, then on separately prelearned data.
   Assert actual residence, identical logical rows and no duplicate identities;
   cover mixed sources, publication overlap, schema null-fill and extensions.
   No timed interval includes ingest/placement drift. **RED:** current hot setup
   prelearns and lacks this paired experiment. **Tier:** placement integration
   and public query journey. **Authority:** REQ-018/024; immutable source and
   nested-field query rules. Evidence: **PARTIAL.**
   - `Ui::place` moves rows only through the bench Scribe stage/publish control, outside
     every window. It requires staging to move at least the journey's rows and publication to
     leave each table's exact row count in Parquet footers. Every catalog statement, standalone
     ones included, must answer in staged and published exactly as in memtables before the
     step's first window; smoke records above and below are VALID in all three residences.
   - Learned arm: `ui-sql-learned` publishes a 10,000-span `hot` sample (2026-01-01, its own
     trace ids) and requires a learned `bifrost.traces` Variant layout through the shared
     `learn_span_layout` (the `hot` prelearn now uses it too) before the default journey
     loads. Smoke: `target/benchmarks/smoke/bifrost/ui-sql-learned/20261118T204803Z-7b988f68b.json`, VALID.
   - **Open:** no assertion yet of no duplicate identities, mixed sources, publication
     overlap, schema null-fill or extensions.
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
   - Clean, unprofiled L1 baseline at `56b8d7443` (clean tree, no `--allow-dirty`), all
     VALID with zero wrong results, about 160 s of the 300 s budget each:
     `benchmarks/history/bifrost/ui-sql/20261118T215633Z-56b8d7443.json`,
     `benchmarks/history/bifrost/ui-sql-10x/20261118T220239Z-56b8d7443.json`,
     `benchmarks/history/bifrost/ui-sql-learned/20261118T220520Z-56b8d7443.json`.
     Default, requests/s (c1 / c8): memtable 74.9 / 301.8, staged 79.8 / 339.8, published
     88.5 / 417.9; metrics page p50 at c8: 56 / 37 / 34 ms. At 10×: memtable 48.0 / 195.1,
     published 71.3 / 307.7. Learned layout is within noise of canonical. Hot memtable reads
     are the slowest residence, and the gap widens with volume.
   - Per-page p95 is inconclusive at c1 (about 95 to 112 page samples per 5 s repetition,
     under the 200 floor) and per-page p99 everywhere; c8 page p95 is conclusive (386 to 533).
     The step-level `client_p99_ms` is reported from 380 to 2,104 samples per repetition;
     that existing step reading applies no sample floor.
   - Plan (review revision): default L1 `ui-sql` is six steps, `{memtable,staged,published}-ui-c{1,8}`,
     at the standard 5 s warmup, 3 × 5 s and 300 s budget. Each client cycles traces,
     verification, logs and metrics pages in a fixed order from its own offset, keeping each
     page's dependencies and concurrency. Each page and page statement reports client latency
     and sample count as `diagnostic.driver.{page,statement}.*`; a percentile under its floor
     (200 for p95, 1,000 for p99) is recorded as inconclusive
     (`query::ui::workload::tests::short_percentiles_are_inconclusive`). Standalone
     statements are correctness-only.
   - The 10× and learned-layout comparisons are separate opt-in experiments,
     `ui-sql-10x` (`bench:l1:bifrost:ui:10x`) and `ui-sql-learned`
     (`bench:l1:bifrost:ui:learned`), outside `bench:l1:bifrost` and `bench:l1`.
   - `tests::level_one_declares_complete_engine_coverage`: RED against the old 69-step
     matrix, then GREEN; RED again on `ui-sql-10x is an extended L1 experiment`, then GREEN.
     Command: `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_one_declares_complete_engine_coverage)'`.
   - Smokes (VALID, `--allow-dirty`, diagnostic only): `ui-sql/20261118T205029Z-7b988f68b.json`,
     `ui-sql-10x/20261118T204416Z-7b988f68b.json`, `hot/20261118T204917Z-7b988f68b.json`.
     A smoke took about 57 s, 24 s of it windows, so setup, seeding and placement took about
     33 s; full L1 should need about 155 s of 300 s. That is an estimate, not a measurement.
   - **TDD process violation:** the `diagnose` sample-floor helper in
     `capacity/query/ui/workload.rs` was written before `short_percentiles_are_inconclusive`.
   - Commits: `be1dda235`, `2cb79aeec`, `e24285ae5`, `7b988f68b`, `dfea1d71c`.
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
