# UI query and hot-read latency

Status: exploring — decision-complete recommendation for maintainer agreement;
not approved for implementation.

## Problem

Fresh development data makes Observe and Verification navigation noticeably slower
than the same deployment after its first approximately ten-minute seal. The
reported 3–8× hot/Parquet difference has not yet been reproduced with fixed rows,
absolute time bounds and comparable builds. Existing L1 hot fixtures prelearn a
Variant layout and omit the UI's page query bundles.

## Desired outcome

Users can navigate, filter and page through recent traces, logs, metrics and
verification executions without waiting for sealing to make them responsive.
The same analytical behavior remains available through ordinary Bifrost SQL.
Measurements distinguish storage execution, SQL duplication and UI critical-path
cost instead of hiding one with changes to another.

## Constraints and non-goals

- Maintainer-selected split: TASK-004B in `/home/thorrester/Documents/GitHub/wyrd`
  owns UI query auditing and consolidation; TASK-012-R2 in the task-004 worktree
  owns the L1 UI SQL scenario and shared hot-read optimization.
- TASK-012-R2 implements and records the new scenario's baseline before any
  hot-read optimization. Preserve the original query corpus for comparison.
- Preserve tenant authority, exact filters and aggregates, source capture and
  publication semantics, query memory bounds and terminal-safe delivery.
- No shortened seal interval, query-result cache, new durable dashboard store,
  UI-specific backend workflow, or weakened benchmark.
- This supplements the existing packet; it does not replace prior design,
  approved specifications, or historical TASK-012-R2 remediation records.

## Proposed implementation

Use two deliverables, with the measurement portion of TASK-012-R2 first:

1. TASK-012-R2: add the UI SQL L1 experiment, preserve current query bundles,
   capture a clean baseline and attribute the hot/Parquet difference.
2. TASK-004B: consolidate the audited UI reads and remove unrelated panels
   from the main list's render dependency. Develop after the baseline exists.
3. TASK-012-R2: change shared hot capture opening into a bounded lazy source,
   then qualify it with the unchanged baseline corpus and the consolidated
   corpus from TASK-004B. Keep UI-only and engine-only measurements separate.
4. Run the combined candidate and reconcile remaining latency by measured
   stage. Passing one improvement is not completion if the user journey still
   misses its agreed targets.

The numbering above is execution order within the user's two tasks, not four
new tasks. Scheduling/merges remain maintainer-owned. No production optimization
may precede the baseline. Existing unrelated dirty work must be recorded in the
comparison revision; clean performance evidence needs a caller-owned committed
checkpoint, not an agent commit or a benchmark-validity bypass.

### TASK-004B: completed UI query audit and selected consolidation

Audit source: `/home/thorrester/Documents/GitHub/wyrd`, branch
`task/wyrd-ui-main-integration/TASK-005`, inspected 2026-11-18. References below
are repository-relative so the design travels with either worktree. Counts are
Bifrost query requests, excluding authentication, optional SQL-predicate
validation and explicitly listed Card reads. Counts describe source, not
measured physical scans.

| Journey / current reads | Decision | Resulting query budget |
|---|---|---|
| Traces: five overview reads, five facets, root list, then visible-trace counts | Two overview aggregates, one multi-facet aggregate, list and bounded dependent counts | Five total; list can render before other sections finish |
| Spans mode: same five overview reads, five facets and a span list | Same overview/facet consolidation; no trace-count enrichment | Four total |
| Logs: four facets, severity timeline and list | One multi-facet aggregate; keep timeline and list | Three total |
| Verification: seven facets, two charts and list, plus one Verifier Card list | One combined facets/charts aggregate over the execution relation, plus list; keep the bounded Card list independent | Two Bifrost queries plus one Card list |
| Metrics rail, before selecting a metric: two point-table facets and one series-name read | Combine the facets; keep the series-name read | Two total |
| Selected metric: preceding rail plus scalar plot, histogram plot and eager group-key sample; optional dependent labels with `apart` | Keep the two distinct plot calculations, defer group-key discovery to opening the grouping control; keep bounded labels only when plotted separate series need them | Four initial reads, five with `apart`; one additional key-discovery read on demand; points inspector adds one only when open |
| StatStrip: two reads per requested stat | One bucket-plus-total aggregate per distinct source/population, with compatible stats as columns | No longer two requests per stat; tokens retain their separate unnested population and currency remains a dimension |
| Waterfall / span or log detail / execution detail | Keep exact-evidence reads on demand; do not preload details for table rows | No new eager reads or per-row requests |
| Query editor / authored SQL panels | Preserve supplied SQL and result bounds | No automatic SQL rewriting |
| Cards and Repository pages | Keep their typed control-plane reads; their loaders do not issue the repeated analytical dashboard bundles above | No new evidence cache or SQL projection |

Audited owners are `packages/ui/src/lib/observe.ts` (`Binds`, `statQueries`,
`facetQuery`, `traceOverviewQueries`, `metricQueries`, `loadMetric`,
`loadExecutionCharts`, `loadRecordTable`, detail loaders), `schemas.ts::EXECUTIONS`,
`loaders.ts`, `node.ts`, `mcp.ts`, `execution.ts`, `cards.ts`, `repositories.ts`,
and the Observe/Verify host server loads. Default facet sets are in
`src/lib/observe-host.ts::FACETS` and `verify-host.ts`.

#### Aggregate SQL and exact populations

Use ordinary SQL `GROUPING SETS` and window ranking at the existing query
builders. The pinned DataFusion source has grouping-set and `grouping()`
coverage (`datafusion/sqllogictest/test_files/aggregate.slt:2613`,
`group_by.slt:5312`). This adds no dependency, SQL function, public endpoint or
query-result cache. Build and inspect the actual physical plan before claiming
one shared scan; a CTE or a UNION of the original queries does not establish it.

- **Trace overview:** one aggregate over matching spans, with grouping sets
  `(bucket)` and `()`, supplies span/error counts, distinct trace/error-trace
  totals and all-span duration percentile. A second aggregate supplies root
  duration percentiles per bucket and over the whole range. Keep two
  populations: a child-span filter still selects roots of matching traces,
  even when the root itself does not match. Preserve the existing root
  membership subquery, exact time bounds, error code and percentile function.
  Do not average bucket percentiles/distinct counts to invent whole-range
  answers. A grouping discriminator separates the total row from a real null.
- **Facets:** one grouping set per allowlisted facet, followed by SQL
  `row_number()` partitioned by facet, ordered by count descending and the
  original typed value ascending, returns eleven values per facet. Use
  `GROUPING(...)` to distinguish inactive grouping columns from genuine null
  values. Keep native typed columns in the bounded result and demultiplex
  them; do not stringify UUIDs/numbers before grouping or sorting. Traces
  retain distinct-trace counts; other signals retain row counts. Log severity
  retains its maximum severity-number annotation. Active filters continue
  applying to every facet, including its own column, as they do today.
- **Verification:** combine all seven facet grouping sets and both
  `(bucket, verdict)` / `(bucket, verifier)` sets in one aggregate over the
  existing execution relation. Apply the same range and filters once at the
  population boundary. The separate list reuses that authoritative SQL builder
  and retains server sort/cursor/limit. Thus the expensive execution relation
  is requested twice per refresh rather than ten times. Move the existing
  top-three-Verifiers plus `N more` numerical aggregation from host
  `ranked()` into SQL; the host may map UIDs to names and disambiguate versions,
  but must not aggregate analytical rows. Limit facets after grouping and chart
  series after calculating whole-range Verifier counts.
- **Verification relation:** preserve its checkpoint/result and status
  selection in this change. Required columns are projected and input predicates
  pushed as far down as their meaning permits. Do not put a page limit before
  a status/verdict filter. Do not add an upper time bound to events/results:
  an execution accepted in the window can finish later. Preserve the current
  one-hour lower-bound slack and direct versus queued behavior. Replacing the
  three event references with a different latest-event algorithm is not
  required to remove ten repeated relation evaluations; baseline plans/profile
  must expose any remaining event-join cost. Changing authoritative selection
  would require separately justified equivalence, not a query-count shortcut.
- **StatStrip:** grouping sets compute total and buckets together. Compatible
  stats share an aggregate; root, span, log, unnested usage and currency
  semantics remain distinct where required. No totals derived from percentile
  or distinct-count buckets in JavaScript.
- **Metrics:** keep cumulative reset, delta, gauge, exact integer/decimal,
  histogram and per-series math in SQL. Retain two plot queries because their
  algorithms/populations differ; combining them merely to reduce the request
  count adds unsupported complexity. Group-key sampling is supporting
  interaction and moves off initial render, using the existing authenticated
  metrics host route. Whole attribute roots remain restricted to that bounded
  sample, plotted-series labels, and explicitly requested detail. Preserve
  precision above 2^53, histogram bucket handling, reset boundaries and current
  result limits. Names come from the series index, not a new scan of points.

#### Page lifecycle, predicate validation and transport

Keep planning in portable `wyrd-ai-ui` loaders/builders so Node, MCP App and
authored views share the same query semantics. The host composes those loaders;
it does not invent a dashboard service or fetch all records to compute data.
Keep current component input contracts where possible and add only the private
composition/result-shaping needed by the built-in page.

Resolve absolute range once per refresh. For nonblank custom predicates, one
page-scoped validation against the signal's canonical table must complete
before any dependent query starts. Pass its normalized value through private
loader composition rather than revalidating separately for overview, facets,
list, names and plots. Public standalone loaders retain validation; invalid or
refused input issues no dependent reads. Do not cache a validation or result
across requests/principals or accept an unchecked caller-supplied normalized
predicate. Metrics series reads retain their existing column-availability
refusals; a point-only predicate must not silently become a different filter.

Start the main list/plot immediately after validation, then its independent
supporting aggregate(s). Expose them through existing SvelteKit promise/await
boundaries so the main result does not await facets, charts or Card-name lookup.
The trace-count enrichment is also secondary: render the root page and show
counts when its bounded follow-up completes. Use explicit loading/error states,
not zero-filled fake counts. All sections still load automatically except
group-key discovery and already-on-demand details/points. Whole-page completion
is measured as well as first useful content, so deferral alone cannot pass.

Keep one session client/executor shared by the refresh and its requests. The
Node executor already shares in-flight connections inside that executor; do not
add a credential-bearing global client cache. Every SQL request continues
through the ordinary authenticated Bifrost operation with unchanged audit and
authorization. Reduced requests also reduce repeated admission, permission,
planning and terminal/decoding overhead; do not bypass those boundaries.

Use the existing request AbortSignal and UI invalidation owners. A new range,
filter or navigation cancels obsolete work; late old results cannot replace new
state. Live refresh coalesces while a refresh is in flight rather than stacking
ten-second bundles. Drawer, cursor, sort and shown-column changes invalidate
only their consumers: paging does not reload range-wide aggregates; opening a
detail never reloads the dashboard. Range/filter changes refresh all affected
sections together under the new absolute bounds. Preserve URL/Back/reload
behavior and approved visual composition. Partial supporting-section failure
does not erase a successful list or mislabel an incomplete result as complete.

### TASK-012-R2, first: L1 `ui-sql` experiment and baseline

Add one first-class Bifrost L1 experiment to the existing capacity driver,
declaration, history and `mise run bench:l1:bifrost` aggregate. A dedicated
`mise run bench:l1:bifrost:ui` wrapper selects the same experiment for iteration.
Reuse `query::{isolated,run,telemetry}` and existing public-client query,
server-metric, profile and placement-control owners. Do not replace the current
hot/Parquet experiments or their criteria.

**SQL corpus.** Freeze the audited pre-change builders' SQL and typed binds in
a small versioned fixture under `benchmarks/fixtures/bifrost/ui-sql/`, together
with its UI source revision and logical input cases. Retain a separate
consolidated corpus from TASK-004B. The UI builder tests regenerate/compare its
exported statements to the corresponding fixture; this is a focused behavioral
test, not a second SQL generator or workload DSL. Copy the exact fixture between
worktrees as an explicit handoff. Rust executes SQL from that fixture through
`wyrd_client::Bifrost`, not approximations written independently. Baseline and
optimized corpus identities remain distinct in records and comparisons.

The corpus contains default and representative filtered/page/detail SQL for:

| Family | Required cases |
|---|---|
| Traces / Spans | Latest 51 candidates and bounded counts; overview; distinct facets; child-only match selecting a nonmatching root; next/back cursor; exact trace waterfall and a full Variant detail |
| Logs | Latest page, severity timeline/facets, exact detail with body/attributes |
| Verification | Actual execution-derived list, seven facets and two charts; direct completed/missing-result, queued pending/running/waiting/retrying/cancelled/errored/completed, old and selected checkpoint results, and status/verdict filters |
| Metrics | Series-name/group metadata, scalar window chart, histogram chart, separate-series labels, exact integer gauge/counter and reset cases, grouping by a literal Variant key; 24h range as well as default range |
| Page bundles | Original 12-read Traces and 10-read Verify schedules, including their real dependencies; corresponding consolidated schedules; Logs and Metrics complete journeys |

**Data.** Use deterministic canonical evidence with the distributions and
per-table batch sizes of the default 500-request `dev:full` journey, plus a
5,000-request-equivalent 10× fixture. Record actual table row/batch counts,
bytes, number of traces/executions/series and fixture digest rather than
equating request count with evidence count. Reuse native/OTLP producers and
the existing development workload's value shapes; fixture loading is outside
timed query windows and requires reconciled acknowledgements/outbox drain.
The benchmark is independent of GitHub, providers, Python service startup or a
browser. Include multiple services/Verifiers, skewed facets, nulls, repeated
trace IDs, timestamp ties, nested payloads, different checkpoint revisions and
late terminal results so an incorrect faster query cannot pass.

**Residence and layout.** Suppress autonomous movement using existing benchmark
settings, without changing production defaults. On a fresh process load the
fixture without teaching a layout; measure it in canonical memtables, explicitly
stage it, then publish it and measure the exact same logical rows and binds in
each state. Assert actual residence and zero missing/extra rows before every
window; `stage_scribe`/`publish_scribe` returning alone is insufficient.
Create a separate fresh-process arm that teaches layouts with a disjoint,
out-of-range sample before loading the identical measured fixture; measure
learned-hot residence and its transitions too. Never call a warmed query cache
a learned layout. Also exercise a mixed canonical/learned/live/published cut in
correctness coverage. Pin fixed absolute ranges; relative 'last hour' must not
change the rows between arms.

**Timing.** Keep first request separately; then existing warmup and three
unprofiled repetitions per declared step. Measure isolated queries at one client
for attribution and complete bundles at one page and eight concurrent pages
for contention. Same datasets/build/settings and page schedule before/after;
warm query/catalog caches equally per residence. Do not run the entire large
cross product for every leaf: individual representative leaf steps at the
default size, all page families at both sizes/residences, and the learned arm
for Variant/Verification representatives cover the distinctions without a
benchmark combinatorial framework. Give this experiment an explicit measured
runtime budget sized from the declared windows; do not silently shorten other
experiments to fit the old aggregate budget.

Record client full consumption/valid terminal, server full handoff, time to
first batch, query CPU, peak query memory and source counts/rows/batches/bytes.
For bundles record time to primary data, last section, per-query latency,
request count and errors; throughput is completed correct pages/second, not
the sum of individual query rates. Separate query profiles into existing
planning/admission, live capture metadata/open, expression adaptation,
row filter/projection, scan/execution, and response/decoding spans/counters.
Use closed benchmark-only labels and existing tracing rather than always-on
per-row timers. Preserve totals/inclusive nesting; do not add overlapping
stage times or infer execution time by subtracting p99s. Hot scan evidence
must no longer be silently unavailable for the new scenario.

Replace the integer-only answer restriction for these cases with a narrow
typed Arrow answer checker that preserves strings/IDs, nulls, exact timestamps,
decimals and Variant values. Keep the existing integer checker for existing
cases. Compare ordered pages exactly, normalize order only for unordered
relations, and check nested values semantically with their extension metadata.
Use simple fixture distributions for deterministic percentile expectations;
preserve the existing approximate function's semantics rather than requiring
bit-identical sketch results across physical partitioning. Any degraded/failed
terminal, missing row, wrong type, truncation, timeout or unexplained request
refusal invalidates the measurement.

**Baseline prerequisite.** Record the untouched engine plus frozen original SQL
in a full clean VALID run with all declared steps and repetitions before UI or
engine optimization. Preserve source/dependency/fixture hashes and host details.
Reproduce the developer observation once in debug with fixed bounds, then use
the profiling/release build for formal performance comparisons. Debug and
optimized results are separate cohorts. Profiles and selected/dirty runs are
diagnostic evidence, not final qualification. Do not invent a baseline p99
from the reviewer's supplied multiplier.

### TASK-012-R2, second: shared lazy hot-source execution

Extend existing `oracle::memtable_source::MemtableSource` and Scribe capture/read
owners rather than adding a new service, pool, source selector or materialized
dashboard layer.

1. **Open captures as descriptors.** Keep the frozen source selection and
   publication revision handshake. Return retained source/run/batch descriptors,
   recorded statistics, schemas and accounting owners. Resolve partition
   ranges, field-ID projection and batch-statistics exclusions using metadata.
   Do not evaluate every batch or allocate a vector of computed results during
   `open_capture`, `live_leaf` or physical-source construction.
2. **Prepare expressions once per captured generation.** Reuse
   `ReadRequirements::adapt`, canonical fallback and logical-column helpers.
   Keep zero-copy Arrow projections for unchanged columns; typed-leaf reads
   continue avoiding root reconstruction. Cache only within the query/source,
   not across principals or mutable generations. Merge/reuse recorded bounds
   without repeatedly visiting retained payload buffers for accounting or
   rebuilding per-batch statistics on every plan statistics request.
3. **Evaluate when pulled.** Deal retained batch descriptors to the existing
   byte-sized/capped execution partitions. The source's stream evaluates one
   needed batch at a time, filters before expensive output reconstruction when
   semantics permit, drops empty batches, and yields bounded results. Normal
   DataFusion polling/spawned partitions inherit the existing process CPU
   lanes and query context. No thread per batch, new Rayon/Tokio pool, unbounded
   `spawn_blocking` fanout, or permit held while awaiting downstream demand.
   If a retained batch exceeds the execution batch size, process bounded row
   slices so cancellation/fairness do not wait for a full large append batch.
4. **Keep pushdown exact.** Preserve static batch pruning and all-match proofs;
   unknown bounds always evaluate, never exclude. Dynamic/volatile predicates
   remain at their supported DataFusion owner; do not evaluate volatile
   expressions early or twice. Preserve projection/fetch rewrites without
   dropping filters or falsely declaring order. A sorted top-N may still need
   every qualifying row; lazy streaming is not a promise that LIMIT skips a
   required sort. Keep conservative statistics after filtering/casts/null-fill.
5. **Complete scalar pruning.** Extend the existing per-append statistics owner
   with exact min/max for the UI's UTF8 and fixed-size-binary identity columns,
   including all-null/empty handling and byte-lexical ordering matching Arrow.
   Do not traverse Variant roots or create a secondary index/Bloom structure.
   Unknown/unsupported/schema-adapted columns remain conservative. Reuse bounds
   for sizing and read pruning. Measure append CPU and retained statistics
   overhead: writes must not regress to improve reads. If this change fails
   that requirement, report it and the evidence before selecting a redesign.
6. **Hold real owners.** Captured Arrow allocations retain their Scribe charge
   until the last reader releases them. Query-created masks, filtered/computed
   outputs and reconstruction buffers use the admitted query memory pool and
   existing ownership/accounting helpers. Shared buffers are not debited twice;
   output charges remain valid while downstream operators retain the batch.
   Do not pretend memory was freed because a waiter dropped. Cancellation,
   failure and deadline drop future work and release all completed ownership;
   late worker memory remains charged until actually disposed.

Apply the same source behavior to in-process and authenticated remote follower
execution. Preserve schemas/field IDs, earlier-generation null fill, current
table version projection, signatures, attempt/capture identity and exactly-once
source union. Never reopen fresh Scribe state for a stream, skip a lost accepted
capture, or return successful partial rows. Failure can move from open time to
stream time; the existing terminal remains authoritative and all public SDKs
must reject the failed stream correctly. Staged and published Parquet continue
using the existing shared Parquet scan.

Do not move layout learning onto the caller's ingest path or rewrite Variant
encoding as part of this repair. Canonical hot generations must meet the UI
latency objectives without a learned-layout prerequisite. Profiles of canonical
and learned reads, full roots and typed paths distinguish remaining Variant
cost; the completed task reports those costs rather than crediting a media
change for a representation change.

### Acceptance and evidence

The following numerical UI objectives are proposed for maintainer agreement;
they are not existing approved SLOs or claims about expected speedup. On the
same supported quiet benchmark host and optimized build, at one page in flight:

| Fixture | Primary list/plot, end-to-end p99 | All automatic page sections, end-to-end p99 |
|---|---|---|
| Default 500-request equivalent | At most 200 ms | At most 500 ms |
| 10× equivalent | At most 400 ms | At most 1,000 ms |

Apply these to canonical-hot, learned-hot, staged and published residences.
Report hot/Parquet ratios per identical SQL case and complete page; a target
ratio is at most 1.5× for full-page p99 using `max(Parquet p99, 20 ms)` as the
comparison floor for tiny measurements. This new target is not permission to
regress any existing case. Eight-page concurrency must improve correct page
throughput and tail latency versus the matched original baseline, with no
additional refusals or memory regression. First-request costs remain separately
visible and the debug dev journey must lose its repeatable first-seal latency
cliff; debug timings are not compared to optimized absolute SLOs.

Compare four candidates with identical corpus/fixtures and host: original
engine/original SQL; original engine/consolidated SQL; optimized engine/original
SQL; optimized engine/consolidated SQL. All original-corpus cases remain in the
comparison. Final qualification includes the full clean Bifrost L1 aggregate
and unchanged Gateway and Verification L1 on the final integrated revision,
against newest compatible history. Preserve gateway-payload-c64 throughput and
r3000 goals (Python p99 2.5 ms, Rust/TypeScript p99 2 ms); no claim that this UI
repair itself guarantees those targets. No invented regression allowance,
host-load manipulation, omitted steps or smoke/dirty qualification.

Focused behavioral proof:

| Situation | Required observation |
|---|---|
| Existing versus consolidated SQL on deterministic evidence | Same list/cursor rows, distinct/null facet counts and independent top-eleven ordering, exact totals, percentile meaning, metric resets/integers/histograms and queued selected result/status; no per-row reads |
| Child span matches, root does not; several matching children | Root appears once, correct trace facets/counts and duration; root/span populations remain distinct |
| Fresh, learned, mixed and evolving schemas across stage/publication | Same logical rows exactly once, correct field-ID/null projection and Variant extensions; covered typed paths do not reconstruct siblings |
| Capture constructed but never pulled; LIMIT consumer stops; cancel/deadline during scan | No row-expression work before demand, no remaining batch evaluation after drop, bounded ownership released and no new-state reopening; sorts are allowed to consume required input |
| Selective ID/time filter with complete, missing or all-null bounds | Proven batches prune; unknown bounds fall back exactly; comparison/null semantics unchanged |
| Low memory or late filter/conversion failure; follower lost after accepted capture | Stable terminal failure, never successful partial/truncated data; charges and staged leases survive real ownership and release once |
| Page/filter/range/back/drawer/live navigation under slow responses | Main data not blocked by supporting reads, old work cancelled, no refresh pile-up, paging avoids aggregate reload, no late stale results, errors remain local and explicit |
| Node, MCP App and production BFF with denied/foreign tenant | Same ordinary SQL behavior, refusals preserved, no leaked rows or validation bypass |
| Slow UI with ingests active | Query stage attribution distinguishes CPU-lane waits, planning, scans, serialization and BFF/SDK cost; primary and complete page times both recorded |

Use the existing `ui:check`, UI real-server Node/MCP/production-BFF integration
lanes, Bifrost unit/integration/Oracle journeys, exact focused nextest commands,
format/lints and `codegen:check` when schemas change. Live UI observation after
the deterministic L1 comparison exercises the real `dev:full` debug stack from
fresh data through sealing and during another real workload batch; it does not
replace L1 with a generic benchmark or turn this into L2. Collect both primary
render and fully settled page timing, SDK decode/BFF/SSR time and server stages.
No new schema, endpoint or data producer is needed just to expose the scenario.

## Evidence and rationale

Verified source findings (2026-11-18):

- Traces overview and dispatch: `packages/ui/src/lib/observe.ts:983,1023`;
  facets `:265`; list/count dependency `:1159`; host
  `src/routes/t/[tenantKey]/observe/+layout.server.ts:47`.
- Verification relation `packages/ui/src/lib/schemas.ts:242`; charts
  `observe.ts:1106`; all ten reads are started by
  `src/routes/t/[tenantKey]/verify/+page.server.ts:75`. The relation has three
  logical event references; optimizer elimination and actual physical scan
  counts have not been measured. Host `ranked()` currently aggregates chart
  rows; moving it into SQL follows the UI's server-owned aggregation rule.
- Metrics plot/labels/keys `observe.ts:523,566,586,660`; StatStrip `:209,229`;
  standalone SQL panel `loaders.ts:225`; detail evidence `execution.ts:189`.
- Eager evaluation: `scribe::LiveCapture::read` (`memtable.rs:2000`) and
  `ReadableSource::project` (`:2272`) called by `tail_rpc.rs:940` and
  `oracle/follower.rs:379,509`. It constructs all result batches before the
  downstream source executes. `oracle/memtable_source.rs` already provides
  static pruning, matched-batch proofs, partition sizing and ownership seams.
- Shared CPU lanes already exist (`oracle/mod.rs::laned_execution`,
  `oracle/telemetry.rs::QuerySpanJoinSetTracer`); the repair must reuse them,
  not assume every async poll is currently outside all CPU admission.
- Cold layout `scribe/layout_cache.rs:1`; learning after claim encoding
  `staging_runtime.rs:887`. Reads do not shred per query; typed-path adaptation
  and union projection pushdown already exist in `memtable.rs` / `oracle/live.rs`.
- Primitive-only hot bounds: `scribe/memtable.rs::batch_statistics` near
  `:2800`; conservatively missing string/binary bounds limit UI ID pruning.
- L1 prelearning `capacity/query/isolated.rs:134`; integer-only answer helper
  `capacity/query/run.rs:671`. Historical clean shredded filter p99 about
  25 ms hot versus 18 ms Parquet; native point favored hot. Those fixtures
  query different cohorts and do not establish page-workload parity.
- `dev:full` builds debug (`mise.toml:1317`), fresh data, default 500 requests
  at concurrency eight; default seal max age is 600 seconds
  (`scribe/mod.rs:494,526`). Matching the observed transition is supportive,
  not a controlled causal measurement.

No matching 3–8× reviewer artifact or running development stack was found in
the preceding research. No live queries, builds, benchmarks or consequential
experiments were run for this design. Query counts and code paths are verified;
relative latency contributions, grouping-set plans and improvements remain
implementation evidence. This proposal addresses each discovered amplification
source and measures residuals; it does not assert every reported millisecond
has already been explained.

## Decisions and alternatives

User decisions: two task owners/worktrees; audit now; new L1 scenario and baseline
before shared hot-read changes; address both UI and common-engine latency.

Recommended decisions: grouping-set consolidation with exact populations,
independent section rendering and scoped cancellation; descriptors plus lazy
batch execution in the existing source; scalar ID statistics; permanent
original/consolidated SQL comparison; canonical/learned residence matrix.

Rejected: one giant UNION of unchanged queries (does not prove scan sharing),
UI-side totals/percentiles/status reconstruction, a dashboard result cache,
durable execution-head table, materialized per-page data, eager ingest layout
learning, blind parallel batch work, and shorter sealing as the latency fix.
These either hide the shared cost, change meaning, add custody/invalidation,
or repeat the earlier unbounded-concurrency optimization failure.

## Standards conflicts

`architecture/bifrost-design.md:734–790` already requires immutable captured,
demand-driven bounded live streams, source consistency, ownership and terminal
safety. `:1081–1160` owns one ordinary query service and Variant behavior. The
selected repair conforms; no source mode/freshness selector is added.

Approved specification revision 16 REQ-024 currently limits Bifrost changes to
shared pressure/CPU integration. TASK-012-R2's additional query optimization
needs an explicit approved specification amendment extending that scope while
retaining memory/durability/no-regression obligations. Design is not that
approval. Record the UI scope addition to this packet as well: older notes say
'No UI work here'; the maintainer has now selected cross-worktree TASK-004B.
Do not rewrite historical approvals or architecture to bless a conflicting
implementation. Historical review-produced TASK-012-R2 files remain untouched;
the new task uses its descriptive hot-read name and explicit scope in the index.

## Open questions

No unresolved architectural choice prevents specification of this proposal.
Maintainer agreement remains needed for the proposal, especially the explicitly
new numerical UI objectives. Profile attribution and exact physical plans are
baseline/implementation proof, not a promise to redesign silently if the
selected approach fails. Private function/stream-state names, partition
descriptor layout and test placement are implementation-owned.

## Handoff

The design remains exploring until the maintainer agrees it. On agreement,
`wyrd-spec` records the bounded amendment and approved numerical objectives;
`wyrd-plan` carries it into exactly TASK-004B and TASK-012-R2 with the dependency
and baseline checkpoint above. Neither workflow is invoked automatically.

Keep this supplement and its link in `design/notes.md` present in both worktrees
so neither task depends on reading another checkout. The source audit is of the
UI checkout; common-engine/benchmark references were also checked in task-004,
which has ongoing allocation edits. Preserve those owners' changes. No code,
task contract, authority document, branch, staging or commit was changed here.
