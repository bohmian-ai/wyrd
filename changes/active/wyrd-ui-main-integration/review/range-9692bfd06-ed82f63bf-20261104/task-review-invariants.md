# Invariant review

Result: **FAIL**. Four material proposals require independent validation.

Subject: `/home/thorrester/Documents/GitHub/wyrd`, `9692bfd06...ed82f63bf35ca124c0a2980c160f0d64535eebd1`. HEAD was the candidate at review completion. First range review; no carried-forward evidence. Source was not edited. Only this report is owned by this reviewer.

## Scope and authority

Reviewed SQL planning and producer-to-consumer invariants for list sorting/keyset paging, metric aggregation, series-index discovery/filtering, histogram windows, numeric representation, and chart identities. Read the shared scope, repository instructions, task-review and UI skills, workflow/maintainer authorities, architecture authority map, Bifrost signal/series/Variant sections, spec revision 27 (especially revisions 20, 23, 24, 26), TASK-004 and superseded TASK-022. TASK-022 is historical context; current spec controls histogram exceptions to its old ban. No extra approval requirement is proposed.

CodeGraph was queried first. Expanded navigation through `observe.ts`, `schemas.ts`, `loaders.ts`, `plot.ts`, `Chart.svelte`, Observe host/route/layout/metrics endpoint, metric point projection and schema, and focused library/page/browser tests. No nested AGENTS.md was found in the UI tree. Sensitive persistent admission and schema migration are outside this specialist's scope and assigned separately.

Existing owners reused by this implementation are the portable `Binds`/query executor, shared `Chart`, the `series` index for metadata, and canonical point projection. Corrections below belong in these owners; no new query service, API, aggregation service, or dependency is necessary.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Revision 23: server column ordering, nulls last, deterministic identity/time ties and keyset cursors | `schemas.ts:170–196`; `observe.ts::orderKeys/beyond/listQuery/loadRecordTable`; host `moved/readState/writeState` | `ui-pages.test.ts` keyset test exercises ascending/descending/null cursor SQL and page shaping; `ui-library.test.ts` sort/paging intents | PASS (source and existing focused assertions; no independent real SQL paging execution) |
| REQ-028/039: exact raw integer metric values and nested counts, absent distinct from zero | `EXACT`, `NESTED_EXACT`, canonical point nullable schema, metric SQL decimal intermediates/server summary text | `ui-pages.test.ts:206+`; metrics browser test unsafe integer and nested-count assertions | PASS for raw points and gauge/sum integer transport; histogram missing sum fails below |
| REQ-050/052/057: cumulative increases and resets are computed per series and window without losing recorded increments | `observe.ts:527–538,730–749` discards interior points before differencing | Existing SQL assertions only check endpoint algorithm, not an interior reset | FAIL — INV-01 |
| REQ-053/054 and AC-032: histogram top groups by count measure, totals stated, every-series view available | `observe.ts:724–752`; histogram host branch `+layout.svelte:341–356` | Existing histogram proof uses one series; source and local SQL equality check expose missing apart behavior | FAIL — INV-02 |
| REQ-022/028 and TASK-004 missing-value obligation: unavailable histogram mean remains unavailable | point projection `histogram_sum: point.sum`; `observe.ts:824,843` | Focused Node execution produces mean zero from a null sum | FAIL — INV-03 |
| REQ-050 merge rule and REQ-053 grouping: distinct histogram groups remain distinct and percentiles are not added together | `histogramWindows` captions and plain `Chart` path into `plot.ts::timeSeries` | Focused Node execution with six groups produces a summed `other` latency | FAIL — INV-04 |
| REQ-055/060: rail, keys and labels use scoped series metadata; search is bound/case-insensitive; point-only SQL remains rejected | `metricNamesQuery`, `groupKeysQuery`, `seriesLabelsQuery`, `Binds.hours`, metrics endpoint and predicate validation | `ui-pages.test.ts` query assertions; browser metrics rail search assertions; source tracing | PASS for inspected metadata paths |
| No replacement orchestration or scope expansion needed | host invokes portable shared loaders/executor; query values remain binds and column sources fixed/validated | Source inspection | PASS |

## Proposed findings

### INV-01 — INCORRECT: differencing after window collapse loses cumulative observations on resets

- **Obligation:** REQ-050 says use increases between consecutive cumulative points and count post-reset points from zero; REQ-052/057 apply that rule to monotonic sums too.
- **Location:** `packages/ui/src/lib/observe.ts:534–538` and `:742–749`, especially endpoint aggregation before `lag`.
- **Producer → consumer:** OTLP sum/histogram points retain start time and value/count in canonical projection. Both queries reduce a series to first/last points per time bucket, then compare only the last point with the previous bucket's last point. The resulting window rate/count/bucket distribution feeds shared charts unchanged. The comments at 530 and 737 explicitly acknowledge the loss.
- **Reachable failing case:** previous bucket ends at count 100, then the next bucket records 110 before a restart and 5 after it (changed start). Required increase is 10 + 5 = 15. Both endpoint algorithms produce 5. With an unreported reset and last count later exceeding 100, even detection is lost. A deployment/restart inside a chart window is normal supported use, particularly wide ranges with wide windows.
- **Consequence/materiality:** request rates, histogram distributions and estimated percentiles omit actual observations. This is numerical correctness of the central requested workflow, not optional hardening.
- **Smallest correction:** retain each cumulative point until its per-series increase/reset has been computed, then aggregate those increments into existing windows/groups. Reuse the present SQL planner, exact decimal arithmetic for integer sums, histogram merge keys, and the existing delta/gauge paths. No storage or API change. Endpoint-only correction cannot recover discarded interior points.
- **Focused closure proof:** execute both generated SQL plans over an actual cumulative sequence with an interior start-time reset and assert the 15-count/increase result and corresponding histogram buckets; retain first-window baseline, delta sums, ordinary gauge and unsafe-integer regression checks.

### INV-02 — MISSING/INCORRECT: histogram selection ignores every-series mode and chooses alphabetic groups

- **Obligation:** REQ-053 selects the 20 largest groups and states the total; REQ-054/AC-032 require the corresponding histogram every-series workflow and top-series labels.
- **Location:** `observe.ts:725,727,751–752`; Observe `+layout.svelte:341–356` versus gauge/sum controls at 379–386.
- **Producer → consumer:** host `listInputs` creates `MetricInput.apart`; `loadMetric` passes it to `histogramQuery`; that query explicitly replaces it with false and aggregates away `series_id`. Rank is `dense_rank ORDER BY` meaning/group/bounds, with no count measure, no all-group total and no title disclosure. The histogram host offers group keys but no every-series control. Label lookup is gated on the gauge/sum chart returning rows, which histograms do not.
- **Reachable failing case:** group HTTP latency by 21 routes; the alphabetically last route has almost all requests. It is omitted and the title never tells the reader a group was dropped. A URL with `apart=1` still returns the same aggregated histogram as default. A focused local check confirmed default/apart generated SQL is identical.
- **Consequence/materiality:** common high-cardinality metrics hide the busiest route and cannot deliver the explicitly required per-series histogram investigation.
- **Smallest correction:** extend the existing histogram SQL/group/label path to honor apart, rank by aggregate count increase, retain totals and state the cap; expose the existing every-series host interaction for histogram output. Reuse `series_id` and `seriesLabelsQuery`, reading labels only for selected IDs, rather than inventing a second metadata owner. Preserve bounds/unit/temporality separation.
- **Focused closure proof:** generated SQL execution with 21 routes whose count ordering differs from lexical ordering; assert top selection and total, then apart mode over multiple same-bounds series and verify separate labeled output through the host.

### INV-03 — INCORRECT: an omitted histogram sum becomes a fabricated zero mean

- **Obligation:** REQ-022/028, retained by revision 24, and TASK-004 explicitly require missing values to remain distinct from zero.
- **Location:** `observe.ts:824` and `:843`, with SQL aggregation at `:749–750`.
- **Producer → consumer:** `PointRow::histogram` (`tables/metrics/projection.rs:637–638`) preserves `point.sum: Option<f64>` as nullable `histogram_sum`; the point schema declares it nullable. Delta histogram SQL returns null sum when no sum exists; `histogramWindows` converts null to 0 and emits mean 0/count. Mixed known/unknown sums are also silently summed over only known rows while count includes all observations.
- **Reachable failing case:** a valid delta explicit histogram with count 1, buckets [1,0], bounds [1], and omitted sum. The local shaper check returns `[instant,"mean",0]`. Buckets still support percentiles and rate; no mean is known.
- **Consequence/materiality:** a displayed zero latency/measurement is invented evidence and a materially false result for a supported nullable metric shape.
- **Smallest correction:** preserve sum availability through SQL aggregation and shaping, emitting a mean only when all contributing observation sums needed by the window are known. Keep count rate, bucket distribution and percentiles available. This is a small correction at the existing aggregation/shaping owner, not a new missing-data framework.
- **Focused closure proof:** one null-sum delta point and a mixed known/missing-sum group both omit the mean while keeping rate/bucket estimates; a recorded sum 0 still emits a zero mean.

### INV-04 — INCORRECT: histogram captions become identities and generic chart folding adds percentiles

- **Obligation:** REQ-050 merges only compatible groups; REQ-053 keeps distinct typed grouping values separate; interpolated percentiles must describe recorded bucket distributions.
- **Location:** `observe.ts:834–848`; histogram `Chart` calls in `+layout.svelte:344,355`; `Chart.svelte:105`; existing `plot.ts:60–71` consumer.
- **Producer → consumer:** the shaper correctly holds typed group tuples internally, but serializes each group into a display-caption `series`. The histogram host passes no typed identity, so `Chart` chooses generic `timeSeries`. That function merges equal captions and folds more than 20 measure lines by adding remaining values into `other`. Each histogram group emits four lines, so six ordinary groups exceed that limit even though SQL permits twenty groups.
- **Reachable failing cases:** six groups, each with one count, bound [1], sum .5, produce 24 latency measures. A focused execution yields 20 chart series including `other` with latency 2.5, the sum of several unrelated mean/percentile lines. Separately, grouped absent values and literal `∅`, or equal group captions under distinct unit/temporality/bounds meanings, can have equal captions and are added before the cap.
- **Consequence/materiality:** the plot invents a latency/percentile and merges incompatible groups on routine supported grouping paths. This is not a request to improve the generic authored-chart fallback; the new histogram consumer selects the wrong mode.
- **Smallest correction:** preserve a typed histogram group/measure identity through loader output and use shared Chart's existing identity-preserving rendering path (or an equally local non-folding path). Keep captions presentation-only; never add means or percentiles across groups. Respect the approved cap by selecting groups upstream and preserve their measures without numeric folding. Do not globally remove generic authored-chart behavior.
- **Focused closure proof:** render six groups and assert every visible latency value comes from one group's mean/estimated quantile and no summed `other` exists; assert absent/literal-empty-marker and same-label/different-meaning groups remain separate. Existing `recordedSeries` and typed-identity tests are the nearby reusable pattern.

## Verification and limits

Read the actual committed range and full changed loader/query functions, plus their host and plot consumers. Independently ran a short `mise exec -- node --input-type=module` probe importing existing `packages/ui/dist/observe.js` and `dist/plot.js`; the relevant implementations were cross-checked against source. Outputs were: missing sum mean 0; six groups generated 24 measures folded to 20 series with `other=2.5`; apart/default histogram SQL identical. These pure checks support the source proof; they are not real SQL/browser coverage.

No costly server, database, browser, or full library suites were started, to avoid concurrent resource contention; orchestrator owns those checks. Existing test assertions were inspected, not credited as independently rerun passes. No implementation or tests were edited. Numeric ranking/cursor reasoning was reviewed statically; full DataFusion execution remains a validation limit. No additional issue is proposed solely for an unexecuted test or a speculative numeric extreme.
