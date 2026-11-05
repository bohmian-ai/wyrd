# Behavior review — complete immutable range

Result: **FAIL**.

Subject: `/home/thorrester/Documents/GitHub/wyrd`, base `9692bfd06`, candidate `ed82f63bf35ca124c0a2980c160f0d64535eebd1`. First range review, `git diff 9692bfd06...ed82f63bf`; no prior acceptance carried forward. HEAD remained the candidate; source was not edited. Only this report was written.

## Authority and coverage

Read the shared scope map, task-review and UI skills, root rules, applicable TS/docs rules, specification revision 27, TASK-004, TASK-022, TASK-04 Bloom task, the transferred TASK-02 live-workflow contract and TASK-03 delivery/evidence context, architecture authority map, the Bifrost metric/schema/Oracle sections, security boundary sections, maintainer style and change workflow. Used CodeGraph before locating code.

TASK-022 is superseded, not an independent executable task. TASK-004 transfers the live closure and historical TASK-022 outcomes to TASK-004-REV-02; revision 19/TASK-03 then inherits its outstanding delivery/visual obligations. Revisions 20–27 amend that inherited behavior. This review follows those transfers for changed behavior and does not reopen unchanged pre-base authentication, components, or full-library delivery.

Independently inspected changed UI loaders/schemas/actions and their callers, Observe route/layout/search endpoint, Chart/RecordTable/RecordDetail/Drawer consumers, unit/browser tests; the pinned dev launcher, MockServer fixtures and clean external example checkout at exact `ee83b2559fb66a89b673786d3bcbe7e65cd26c17` (app, desk, batch); point projection/validator and series-id/index owners, Gate reservation, Scribe ingress/series-before-points dispatch, schema/fingerprint and Oracle pin/test changes. Read consumer-to-producer paths for each finding below. Persistent-data, replay/concurrency, SQL arithmetic/keyset, repository gates, and external fork internals receive additional independent specialist coverage; this report does not claim their unexecuted journeys as independently passing.

Existing owners remain appropriate: metric reads extend `observe.ts` through the existing executor; histogram charts extend existing `Chart`; derived ids extend the canonical point validator/projection; series writes extend Scribe ingress and ordinary table admission. No replacement transport, UI durable owner, custom distributed codec, or speculative dependency was found. The defects below are in behavior of those extensions, not an argument for new machinery.

## Acceptance matrix

`PASS` denotes inspected source/proof for the bounded row, not proof of an unexecuted broader journey. Where proof is missing, `FAIL (unverified)` records the limit separately from source defects.

| Requirement / constraint | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-045: deterministic ordinary provider failures, fixed success delays, unchanged judge/tool handling | `expectations.json` order-specific lookup responses, higher-priority failures, unchanged tool/final-answer matching; pinned `Desk` keeps normal gateway/model addresses and default retry behavior | Inspected fixture and exact-pin app source; no live provider replay here | PASS (source) |
| REQ-046–048: real statuses, counters/histograms, correlated error/debug/warn logs | Exact-pin `src/app.py` sets explicit OK, catches API failures into 502/504, emits providers' metrics; `src/desk.py` records exceptions/logs within ask span; `logTimelineQuery` maps 1–8 to debug | Source trace plus existing UI log/browser assertions | PASS (source) |
| REQ-049: 500/concurrency-8 deterministic finite reconciliation and repeat batches | Exact-pin `scripts/batch.py` plan/dispatch/baseline/readback/missing; `example-app.sh` pins the upstream commit and production wheel instead of patching source | Static exact case allocation and per-call identity/counter-baseline inspection; default-500/repeated live batch not executed | FAIL (unverified live acceptance) |
| REQ-050/051: shared conventional histogram rate/mean/estimated percentile/distribution plots | `histogramQuery`, `histogramWindows`, `histogramProps`; Observe histogram branch uses shared Chart | Existing histogram unit tests pass; source and executable checks expose B-01/B-02/B-03/B-04 | FAIL |
| REQ-050/INV-009: unbounded percentile is a labeled lower bound; exact mean is not called estimated | `bucketPercentile` returns `atLeast`, but shaping discards its per-value flag; histogram heading labels mean and percentiles together as estimated | Existing unit expects a bare numeric p99 for the unbounded bucket | FAIL (B-04) |
| REQ-052/053/054/057: server windows, chosen keys, largest 20, every-series view, truthful identity | Numeric path uses per-series windows/rank, `named` and existing Chart identity/exact summary; histogram path differs | Existing gauge/sum SQL/unit/browser proof; histogram caller-to-render reproduction below | FAIL (B-01/B-02) |
| REQ-055/060: searched/scoped names and label/key reads use series index with explicit limits | `metricNamesQuery`, `groupKeysQuery`, `seriesLabelsQuery`; authenticated search endpoint validates inherited filters and uses request session | Focused ui-pages tests pass; inspected no points reference in index-query SQL | PASS |
| REQ-056/058: shared deterministic derived id, raw writer overwritten, hourly registered index with attribution/Blooms | `metrics/series.rs::with_series_ids/series_rows`, point validator/projection, table registry/layout; Gate refuses client index writes | Existing golden/canonicalization/admission tests inspected; execution delegated to specialists/root | PASS (source; specialist proof required) |
| REQ-059/revision 27: index durable before points; bounded seen-set only after success | `ingress.rs` calls `admit_series_first` before dispatch; ordinary admission completes before `remember`; cache is per tenant/hour and bounded | Persistence-path tests inspected; crash/recovery execution not repeated here | PASS (source; specialist proof required) |
| REQ-061/AC-039/040: measured 1h/24h server times and rows, 10x point scaling | Index-query SQL avoids point scans for named operations | No measured 1h/24h/10x evidence independently obtained in this review | FAIL (unverified performance acceptance) |
| Revision 23: URL sort/keyset state, reset on changed investigation, typed action validation, SQL execution remains bounded | `schemas.ts`, list/order/cursor loader, `observe-host.ts::moved/step`; RecordTable header/buttons | Existing focused UI tests pass; SQL invariant specialist covers ties/nulls/precision | PASS (source; specialist proof required) |
| REQ-039/AC-018: missing values remain unavailable, incompatible metric meanings never silently combine | Numeric path preserves exact summary text; raw point list casts integer/nested counts; histogram presentation violates missing/identity rules | Executable B-01/B-03 reproductions | FAIL |
| Component layout changes remain portable and usable | Shared Chart dimensions/tooltip/range, Drawer overflow, detail raw/multiline rendering, log resize; host composition remains layout-only | Root reports check 0 errors/warnings; no interactive browser available for visual inspection | FAIL (unverified visual/accessibility acceptance) |
| Bloom task: generic predicate-only scalar correction, one fork pin, untagged canonical schema, retained historical WAL identity | Workspace pins and Oracle codec agree; schemas remove class label; Oracle production regression, historical WAL regression and distributed hot/promoted/rewritten tests added | Source/test inspection; root reports focused Oracle/library/fork checks, not independently rerun here | PASS (source; persistent-data specialist proof required) |
| INV-001/002/004 and no new UI durable/security authority | Existing native SDK/executor/server owners retained; search uses browser session, filters validated, SQL values bound; client series writes reserved | Source review; no broad gate claim | PASS |
| AC-030/036 and transferred individual desktop/mobile light/dark acceptance | Browser tests contain histogram smoke and canvas checks; historical evidence predates this full candidate's histogram changes | Root attempted browser inventory/create: unavailable. No new human acceptance inferred | FAIL (unverified acceptance) |

## Findings

### B-01 — INCORRECT: histogram group identity is lost before rendering, and the generic chart invents aggregates

Obligation: REQ-050 merging, REQ-052 meaning separation, REQ-039 truthful metrics, and TASK-02's explicit prohibition on treating generic duplicate-point summation/series folding as valid metric aggregation.

Locations: `packages/ui/src/lib/observe.ts:824–850` (`histogramWindows` group/label/output); consumer `components/Chart.svelte:103–105`; existing `plot.ts:43–75` (`timeSeries`).

Producer SQL groups by meaning, chosen keys and bounds. Shaping preserves that identity only in its private Map; the public series caption uses only chosen-key text. Two groups with the same `service_name=checkout` but bounds `[1]` and `[2]` both become `mean · checkout` / `p95 estimated · checkout`. The conventional histogram branch passes no typed Chart identity, so `timeSeries` sums equal-caption values at an instant. Independently executed existing built modules: means 0.5 and 1 became a plotted mean of 1.5; p95 .95 and 1.9 became 2.85. Different bounds are explicitly supported and arise naturally during instrumentation rollout. This reports false latency, not just ambiguous wording.

The same consumer sums overflow lines into `other`: six valid groups produce 24 mean/percentile lines, which the generic 20-series policy folds into 19 lines plus an `other` latency. Independently reproduced `measures:24`, `drawn:20`, `other` present. Adding unrelated means/percentiles is meaningless.

Smallest correction boundary: keep typed histogram meaning/bounds/group identity through shared shaping into Chart; reuse the existing recorded-identity Chart path or another existing presentation seam that never applies generic sum/fold semantics. Give distinct captions where required and preserve compatible-group aggregation on the server. Ensure the existing ceiling has an honest presentation rather than silently adding unrelated latency values; no new transport or durable owner is required. Closure proof must drive shaped histogram data through actual Chart/plot behavior with two same-key/different-bound groups and at least six groups, verifying exact separate values and no summed `other` percentile.

### B-02 — MISSING/INCORRECT: histogram grouping omits largest-group selection, total disclosure and every-series mode

Obligation: REQ-053/054/057 and explicit AC-032 (group a histogram, then choose every series apart).

Locations: `observe.ts:724–766` (`histogramQuery`); `observe.ts:649–695` (`loadMetric`); `observe/+layout.svelte:343–363` histogram branch versus calculation controls at 378–394.

`histogramQuery` forces `{ ...input, apart:false }`, aggregates away `series_id`, and ranks by the lexical meaning/key/bounds tuple, not count increase. It returns no total number of groups. The histogram branch offers key entry only, with no every-series control. Even an explicit shared URL `apart=1` is ignored for histograms. `loadMetric` labels series only from the gauge/sum chart; that chart is empty for a histogram, so the index-label path does not run.

A metric with 21 route groups can silently omit its busiest route because it sorts after the first 20. A reader cannot obtain the separately required series investigation for the histogram, and no total tells them groups were excluded. These are ordinary production metric use and an explicit acceptance case.

Smallest correction: extend the existing histogram SQL/loader and the existing calculation/identity controls to honor `apart`, count and disclose all matching groups/series, select the 20 by the declared count-increase measure, and read only the selected ids' labels through the existing series-index owner. Keep default compatible-bounds aggregation. No second aggregation service or ChartInput wire extension is needed. Prove 21 groups with the largest lexically last, and a histogram every-series browser/control path with distinct index-backed labels and total disclosure.

### B-03 — INCORRECT: an absent histogram sum is rendered as a zero mean

Obligation: REQ-028/039/050 and AC-018: missing values are not zero and unavailable aggregates stay unavailable.

Location: `observe.ts:826` tally construction and `:845` mean push. Producer: nullable `histogram_sum` in `tables/metrics/points.rs:127`, projected by ordinary OTLP ingestion; histogram SQL `sum(dv)` preserves SQL NULL.

A valid histogram with count/buckets and no recorded sum reaches the loader with null `sum`. `numeric(row[sumi] ?? 0)` changes it to zero, then publishes `0/count` as an observed mean. Independently executed `histogramWindows` on count 10, buckets `[10,0]`, sum null: output included `['...','mean',0]` while usable percentiles remained present. Optional absent sums are a supported metric representation; zero latency is materially false.

Smallest correction: preserve sum availability through existing SQL merging and local tally construction, omitting/unavailable mean when the recorded data cannot determine it, while retaining valid rate, percentiles and distribution. Cover both an absent sum and a compatible merged group where one contributing sum is absent; do not weaken ingestion or invent sums from bounds. A small shared-loader fixture plus existing SQL journey is enough.

### B-04 — INCORRECT: lower-bound percentiles lose the `≥` qualifier at every numeric consumer

Obligation: REQ-050 and INV-009 expressly require a rank in the unbounded bucket to show `≥` the last finite bound, with estimates labeled wherever displayed; exact values must not be labeled estimates.

Locations: `observe.ts:847–850`, `:880–883`; consumers Chart tooltip at `Chart.svelte:405–414` and summary derived at `:119–125`.

`bucketPercentile` correctly returns `{value,atLeast:true}`, but `histogramWindows` saves only `value` and one global boolean. `histogramProps` appends a general title saying a percentile in the top bucket shows the lower bound. The tooltip and Latest/Max/Mean still print that bound as a plain numeric percentile, and neither the instant nor the affected percentile is identified as censored. Existing unit explicitly expects bare numeric p99=1 for the unbounded case. Mixing bounded and unbounded observations makes the title unable to qualify individual numbers. Also the title says `mean and percentiles estimated from buckets`, even though mean is exactly sum/count.

A slow tail entirely beyond the last bound therefore appears as a specific p95/p99 value in the user's inspection/summary. This violates the explicitly chosen estimate semantics.

Smallest correction: retain the per-value lower-bound qualifier in the shared histogram presentation and preserve it in the existing tooltip/summary/accessible output; distinguish exact mean from estimated percentiles. Reuse existing Chart presentation seams, with no new durable contract. Prove one bounded and one unbounded window together, including tooltip/summary text and machine-readable loader output: the censored value must remain identifiable as `≥ bound`, and exact mean must not be labeled estimated.

## Verification and limits

Independently ran `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts`: package/AppBridge build succeeded; 1 file, 6 tests passed. These tests mostly pin SQL text and small shaping examples; they do not exercise the failing histogram consumers above. Ran read-only Node checks against the built `loaders.js` and `plot.js` for B-01/B-03 and `histogramQuery(apart:true)` for B-02. Ran `git diff --check 9692bfd06...ed82f63bf` successfully. Rechecked HEAD and worktree: only review artifacts are untracked.

Did not run default-500 dev workload, Postgres/browser journeys, crash tests, Rust builds or the external fork suite; costly execution was coordinated with the orchestrator/specialists. Root separately reports `pnpm check` 0 errors/warnings. Root's browser inventory/create attempt found the browser unavailable, so light/dark/mobile visual acceptance cannot be newly established here. Reported 516 library/five Oracle journey passes remain caller evidence, not independent execution in this report. No new source defect is inferred solely from unavailable visual or performance proof.

Four source-proven material behavior defects remain; **FAIL** is independent of those verification limits.
