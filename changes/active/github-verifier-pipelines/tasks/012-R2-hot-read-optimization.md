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
   Evidence: **checker units GREEN; real-server evidence not run.**
   - Units (`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(/query::ui::/)'`, GREEN):
     `answer::tests::{typed_answers_compare_ids_timestamps_decimals_and_nulls,ties_compare_as_multisets_within_their_keys,variants_compare_by_value_not_layout}`,
     `tests::{frozen_catalog_holds_the_original_statements,tied_statements_declare_the_order_their_sql_leaves}`,
     `fixture::tests::every_fourth_root_invokes_an_agent` (RED `left: 0, right: 125`),
     `seed::tests::driven_statements_must_count_what_was_driven`, and
     `seed::tests::journey_statements_must_answer_what_was_exported` (RED
     `seed.rs:341 assertion failed: check("stat-spans-total", &answer("value", &["0"]), journey).is_err()`).
     They prove the checker and the known counts (`seed::known`, `Journey::known`), not that
     the server answers them.
   - Real-server runs cited here before were withdrawn on 2026-11-18: they were started
     from a retry loop around the `mise run bench:*` tasks and read with `jq`, contrary to
     `benchmarks/AGENTS.md`; their records were deleted, and real-server evidence awaits
     compliant runs.
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
   Evidence: **declaration GREEN; baseline checkpoint not reached.**
   - `tests::level_one_declares_complete_engine_coverage` (RED against the old 69-step
     matrix and on `ui-sql-10x is an extended L1 experiment`, then GREEN):
     `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_one_declares_complete_engine_coverage)'`.
     `workload::tests::short_percentiles_are_inconclusive` GREEN.
   - **TDD process violation:** the `diagnose` sample-floor helper was written before
     `short_percentiles_are_inconclusive`.
   - A full baseline and a profile were run, but they are withdrawn and do not satisfy this
     checkpoint. Real-server runs cited here before were withdrawn on 2026-11-18: they were started
     from a retry loop around the `mise run bench:*` tasks and read with `jq`, contrary to
     `benchmarks/AGENTS.md`; their records were deleted, and real-server evidence awaits
     compliant runs.
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
