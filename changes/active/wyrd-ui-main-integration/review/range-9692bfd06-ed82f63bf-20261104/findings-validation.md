# Independent findings validation

Recommendation: **FIX_REQUIRED**. Eight deduplicated findings remain. This is an implementation/proof recommendation, not a request for renewed approval of scope.

Subject: `/home/thorrester/Documents/GitHub/wyrd`, first full range `9692bfd06...ed82f63bf35ca124c0a2980c160f0d64535eebd1`. HEAD remains the candidate. No source was changed. No prior FIND IDs or closure evidence exist; IDs begin at `FIND-TASK-004-1`.

## Inputs, authority and bounded validation

Read scope.md, all six selected discovery reports, verification.md (including supplied performance limits), claim-comparison.md, approved specification revision 27, original TASK-004, historical/superseded TASK-022 and TASK-04 Bloom task. Read task-review/UI skills, contributor Rust/rustdoc and maintainer rules, architecture authority map, Bifrost's correlation, series, admission/schema contracts and change workflow. Current user authorization of potential spec deviations takes precedence; it does not turn an incorrect numeric result into correct behavior or establish an unperformed visual inspection.

Used CodeGraph before source navigation. Independently read the changed SQL/loader functions and their host, Chart/plot and canonical metric consumers; full series-id derivation, row selector, seen-cache/derived-frame/admission methods; native correlation and dimension stamping, public native SDK/Gate path and description journey; new persistence tests and missing Rust item documentation. Inspected the actual range and test synchronization change. Expanded the supplied map only around proposed causes and proof gaps; did not rerun comprehensive discovery or the external fork audit. Selected specialists cover the complete 61-file range and applicable language/boundary rules. No selected report is missing.

Independently executed a small assert-based Node probe against existing built modules, cross-checked with committed source: a null histogram sum emits mean 0; equal captions for distinct bounds sum means to 1.5; six groups produce 24 measures folded into `other`; apart/default histogram SQL is identical. No script or fixture was added. No Cargo execution or additional service workload was started. This confirms shaping/plan defects, not DataFusion or browser execution.

## Proposal disposition

| Discovery proposals | Disposition | Final ID / explanation |
|---|---|---|
| B-01, INV-04 | CONFIRMED, duplicate causes consolidated | FIND-TASK-004-1: lossy histogram identity selects generic additive rendering |
| B-02, INV-02 | CONFIRMED, duplicate causes consolidated | FIND-TASK-004-2: incomplete histogram selection/every-series workflow |
| B-03, INV-03 | CONFIRMED, duplicate causes consolidated | FIND-TASK-004-3: missing sums are not propagated through aggregation/shaping |
| B-04 | REVISED | FIND-TASK-004-4: preserve per-value censoring and estimate semantics; a generic title does not qualify each result. No new public ChartInput schema is required. |
| INV-01 | CONFIRMED | FIND-TASK-004-5: cumulative points collapse before reset-aware differencing |
| MAINT-001, DATA-01 | REVISED, one shared cause | FIND-TASK-004-6: index suppression assumes hashed attributes determine accepted query context. Native Run correlation and accepted dimensions are sibling producers of the same omission; no separate maintenance/refactor finding. |
| SYS-01 | REVISED | FIND-TASK-004-7: explicit AC-038 interruption/recovery proof absent. No demonstrated data loss or production durability defect is asserted. |
| REPO-1 | CONFIRMED | FIND-TASK-004-8: mandatory changed-item rustdoc omissions, batched with behavioral remediation |

No proposal is rejected. Repeated reports create no additional work. There is no new validator-originated blocker requiring focused source validation, and no unresolved proposal conflict warrants another discovery pass.

## Final decision-complete ledger

Paths below use `ui/` for `crates/wyrd/wyrd-server/wyrd-ui/` and `redux/` for `crates/vala/vala-bifrost-redux/`.

### FIND-TASK-004-1 — Histogram rendering adds unrelated latency values

**CONFIRMED; INCORRECT; sources B-01 / INV-04.** REQ-050/052 preserve incompatible bounds/meaning groups; truthful metric results cannot add percentiles. Location: `ui/packages/ui/src/lib/observe.ts:816–860`; host `src/routes/t/[tenantKey]/observe/+layout.svelte:343–355`; consumer `packages/ui/src/lib/components/Chart.svelte:103–107` and `plot.ts:43–78`.

Producer SQL separates meaning, chosen keys and bounds. `histogramWindows` retains that tuple privately but emits only a caption and numeric measure. Same chosen-key caption under different bounds becomes the same public series string. The host supplies no `identity`, so Chart chooses generic `timeSeries`, which adds coincident points and folds lines after twenty into summed `other`. Two same-service/different-bound groups with means .5 and 1 plot 1.5. Six ordinary groups yield 24 mean/percentile lines, exceeding the generic ceiling even though SQL legitimately selects twenty groups. Rate/distribution sibling outputs use the same lossy group captions; they need the same preserved group identity and distinguishable presentation. This is routine rollout/grouping use and materially false measurement.

**Smallest correction:** preserve typed meaning/bounds/chosen-key (or apart-series) identity plus measure identity through existing histogram shaping and shared Chart props. Use existing identity-preserving `recordedSeries` behavior, leaving the authored generic summation/folding path unchanged. Keep group selection upstream; shared Chart may select visible lines without adding hidden means or percentiles, with honest visibility/selection disclosure. Distinguish ambiguous distribution/rate captions too. No replacement chart, transport, storage or public schema is needed. Merely changing captions cannot fix overflow addition; globally disabling authored-chart folding changes unrelated behavior.

**Focused closure:** pass two equal-caption/different-bound groups, null versus literal absent-marker groups, and six histogram groups through the real shaping/plot selection path. Assert separate original numeric values, distinguishable identities and no summed `other` latency. Verify the host forwards identity and selected-group disclosure.

### FIND-TASK-004-2 — Histogram selection omits required busiest and every-series results

**CONFIRMED; MISSING/INCORRECT; sources B-02 / INV-02.** REQ-053/054/057 and AC-032. Locations: `observe.ts:649–695,724–753`; Observe layout `:341–394`.

Host state can produce `apart`, but `histogramQuery` explicitly overrides it to false, groups away series_id, ranks lexically by meaning/key/bounds and returns no all-group total. `loadMetric` fetches selected series labels only from the gauge/sum result, which is empty for histograms. The histogram host offers only keys. Thus the busiest lexically-last route among 21 routes disappears, its exclusion is undisclosed, and the required per-series histogram investigation cannot be reached even with `apart=1`. SQL plan equality is independently reproduced. Index-backed key/label helpers already exist; this is incomplete extension of their workflow, not missing infrastructure.

**Smallest correction:** extend existing histogram query/loader to preserve series_id in apart mode, rank selected groups/series by count increase over the range, retain all-match totals, and read labels only for selected IDs using `seriesLabelsQuery`. Expose the existing every-series interaction in the histogram heading and disclose totals/cap. Default compatible-group aggregation, bounds/unit/temporality separation and query limits remain. Do not expose irrelevant gauge aggregation choices or add another label owner.

**Focused closure:** execute generated SQL with 21 routes whose largest count is lexically last; assert chosen top twenty and total 21. Exercise apart mode with multiple compatible series through loader and host, distinct index-backed labels, and only selected-ID metadata reads. Cover the rendered control, not URL parsing alone.

### FIND-TASK-004-3 — Missing histogram sums fabricate means

**CONFIRMED; INCORRECT; sources B-03 / INV-03.** REQ-028/039/050, AC-018 and original TASK-004's missing-value rule. Locations: `observe.ts:746–750,824–845`; supported producer `redux/src/tables/metrics/projection.rs:637–638` (`histogram_sum: point.sum`) and nullable point schema.

OTLP accepts a count/bucket histogram with no sum. SQL sum remains null when every sum is absent, but shaping substitutes zero and emits zero/count. Mixed known/missing contributors are worse: SQL sums known values while count includes all contributors. Both are valid ordinary histogram shapes, so the mean is invented, even though rate and bucket percentiles are usable. An actual recorded zero sum must remain distinguishable from absence.

**Smallest correction:** preserve availability at the existing SQL aggregation and tally owner: emit a mean only when all sums required for that window's merged observations are known. Propagate missing cumulative baseline sums as unavailable too. Keep valid counts, distribution and percentile estimates. Do not derive sums from bounds, reject otherwise valid ingestion, or build a generic missing-data framework. A shaper null check alone cannot repair mixed SQL aggregates.

**Focused closure:** generated SQL plus shared shaping for absent, mixed known/absent, cumulative unavailable-baseline and recorded-zero sums. Assert unavailable mean for indeterminate windows, true zero for recorded zero, and retained rate/distribution/estimates.

### FIND-TASK-004-4 — Censored percentile values lose their qualifier

**REVISED; INCORRECT; source B-04.** REQ-050 and INV-009. Locations: `observe.ts:845–850,866–883`; Chart tooltip `Chart.svelte:405–414` and numeric summary `:119–125`.

`bucketPercentile` produces `{value,atLeast}` correctly. The shaper discards the per-value flag, retaining one global boolean. Tooltip and summary format the surviving lower bound as an ordinary number; the global title cannot identify which percentile/window is censored in a mixed sequence. The title additionally calls sum/count mean estimated from buckets. A tail beyond the last finite bound therefore appears as a definite p95/p99 latency. This violates explicitly required output semantics even without a visual-layout failure.

**Smallest correction:** retain per-value estimated/lower-bound presentation in existing histogram props/results and shared Chart presentation, through tooltip, accessible/summary text and agent-readable loader output. Show `≥` for censored estimates, label percentiles estimated wherever surfaced, and label sum/count mean accurately. Ensure summary values derived from censored inputs cannot be reported as exact; either conservatively qualify their bound or leave an indeterminate summary unavailable. Reuse the existing shared presentation seam and numeric geometry; do not modify durable telemetry or invent a public ChartInput field. This is separate from identity: identity fixes alone cannot recover a discarded censor flag.

**Focused closure:** one bounded and one top-bucket window together, with different affected percentiles. Assert qualified values in loader output, tooltip and summary, and an exact mean without an estimate label. No additional percentile estimator is needed.

### FIND-TASK-004-5 — Window endpoint collapse loses cumulative reset increments

**CONFIRMED; INCORRECT; source INV-01.** REQ-050/052/057. Locations: `observe.ts:524–538` and `:730–749`.

Canonical points preserve per-point start and value/count. Both queries first reduce each series/window to endpoints, then compare the final endpoint with the prior window. Previous count 100 followed by 110 and a reset to 5 inside the next window must contribute 10+5=15. The implemented start-change branch returns only 5; the intermediate 110 is already gone. The source comments acknowledge the omission. Restarts within ordinary chart windows are supported, so request rates, bucket distributions and percentile inputs omit real observations. This affects sums and histograms through the same algorithmic cause.

**Smallest correction:** compute reset-aware per-series increments from consecutive points before reducing into existing time windows and meaning/key groups. Keep current range-start baseline policy, delta paths, gauges, exact decimal integer arithmetic and grouping boundaries. No storage, precomputed service, wider scan outside the approved range, or new dependency is required. Endpoint guards cannot reconstruct discarded points.

**Focused closure:** execute both generated SQL plans over an actual cumulative sequence with an interior start-time reset; assert increase/count 15 and corresponding histogram bucket increments. Retain decreasing-value reset, first-window baseline, ordinary gauge/delta and unsafe-integer assertions in the affected narrow tests.

### FIND-TASK-004-6 — Series-index deduplication suppresses distinct accepted contexts

**REVISED; INCORRECT/REGRESSION; sources MAINT-001 / DATA-01.** REQ-058/060 and established native correlation/accepted dimension contracts. Locations: `redux/src/tables/metrics/series.rs:492–576`; `src/scribe/series_index.rs:43,130–143,178–197`; consumers `ui/packages/ui/src/lib/observe.ts:552–584,900–907`.

Independent producer trace: `series_ids` hashes metric meaning/bounds/resource/scope/attributes; it excludes accepted workload/environment and independent native run_id. Public `Bifrost::write_batch` sends native batches through Gate's `dispatch_native_frame`; the writable-description journey and `caller_run_id_column` explicitly permit independent run_id. `stamp_correlation_columns` preserves that native value. Gate `frame_dimensions` resolves accepted workload/environment separately, and `stamp_dimensions` overwrites Service labels/environment after derivation. OTLP correlation from retained point attributes is a sibling case where the original assumption holds; native correlation and accepted dimensions prove it is not universal.

The in-slice selector asks only once for `(hour,id)`; cross-slice `admitted` and process `SeriesSeen` suppress the same pair. Two points with the same raw identity/hour under Runs A/B or accepted environments dev/prod therefore persist correct different point context but retain only the first context in the index. The second scoped rail/key/label query sees no matching metric despite durable matching points. This is index coverage loss, **not point data loss**, and is normal supported SDK/context use. Restart/overflow may change the symptom, not repair the invariant.

**Smallest correction:** at the existing series row/key owner, distinguish the already-stamped semantic context used for scoped metadata reads, in addition to series/hour. Use the same discrimination for in-slice, cross-slice and process suppression and deterministic derived-batch identity. Preserve stable series_id bytes, ordinary native correlation and accepted dimension stamping. Cover effective workload/subject/Run and promoted Service/environment context, including other copied semantic query dimensions that can vary independently of hashed attributes; ephemeral per-admission timestamps/request stamps must not turn this into per-point indexing. Readers continue counting distinct series_id; repeats remain valid. Reconcile the spec/Bifrost sentence falsely claiming all copied context is fixed by attributes with this accepted-context coverage. Current caller authorization permits that reconciliation without another approval stop.

This costs a bounded extension to an existing index key and proof. Disabling all dedup defeats series-scaled cost; adding context to series_id changes approved point identity; scanning points as a downstream fallback defeats index reads; removing native correlation regresses clients. None is the selected correction. No new table columns, index service, registry resolver or cross-table transaction is needed.

**Focused closure:** existing native/persistence fixtures exercise identical raw series/hour in distinct Runs within one batch and across batches, and accepted workload/environment variants across batches. Assert unchanged series_id, correct points and index visibility under each corresponding scope; exercise within-slice selection and unchanged-context repeat suppression. Derived identity must distinguish different selected contexts on retry, while failed-series refusal, bounded cache and ordering remain intact. Extend the existing filtered Metrics journey to prove a nonempty rail for both contexts; do not repeat an unrelated full journey suite in task remediation.

### FIND-TASK-004-7 — Required composed interruption/recovery proof is absent

**REVISED; MISSING required proof; source SYS-01.** Revision 27 explicitly preserves AC-038. Location: `redux/src/scribe/tests/scribe_persistence_path.rs:1229–1320` and missing changed-path interruption coverage.

Production `ingress.rs:803–814` awaits `admit_series_first` before sending points and awaiting their durable completion. The helper remembers keys only after ordinary series admission success. Restart begins with an empty cache and ordinary WAL recovery. Source supports the ordered guarantee. Existing new tests verify normal repeated writes and decoded-size refusal/retry in the same live Scribe; neither interrupts/reconstructs/replays the composed path. Historical tagged-WAL recovery tests cover a different invariant. No AC-038 execution evidence is supplied. Therefore required proof is missing; it would be incorrect to claim observed lost data.

**Smallest correction:** add and execute focused interruption/restart/read coverage using existing retained-volume recovery facilities. Exercise interruption after durable series and before points, plus points durably admitted before response/ack so recovered-point assertions are nonvacuous. Restart through ordinary recovery against retained volume; every recovered point must have its tenant/hour/series/context index coverage. Reuse existing pauses/fault facilities where possible. No new recovery mechanism, production transaction, process-wide shutdown or broad fault matrix is justified.

**Focused closure:** record the actual interruption boundaries, recovered rows and ordinary retry result; require at least one recovered point in the before-response case. Existing live refusal tests remain. This is the narrow explicit acceptance test, not a demand to prove all conceivable crashes.

### FIND-TASK-004-8 — New Rust items lack mandatory documentation

**CONFIRMED; VIOLATION; source REPO-1.** Root Code rustdoc rule and `agent_docs/rust.md:234–240` explicitly include private/test helpers/tests and possible panics. Locations: `redux/src/tables/metrics/series.rs:638–784` and `src/tables/mod.rs:2505`.

The helpers `point`, `projected`, `id`, `otlp_id`, `replaced` and five identity tests have prose but no `# Panics`, despite direct expect/assert or the documented fixture chain. Nested `assert_untagged` has no rustdoc at all and asserts recursively. Enclosing test docs do not document a separate helper. This is an explicit mandatory merge gate, not a claim of production panics or a preference about documentation length.

**Smallest correction:** add accurate intent/invariant and panic-condition documentation for these changed items, checking the complete new module and nested helper together. Preserve behavior. Batch with the above corrections; no style-only round, helper redesign or new test is required.

**Focused closure:** inspect meaningful item documentation and panic conditions; run the owning format/lint checks already required for the Rust write set.

## Reconciled acceptance and verification recommendations

The discovery matrices are useful coverage records, but their `FAIL (unverified)` rows must not be reported as independently proven implementation failures. Reconcile as follows:

| Obligation / boundary | Reconciled evidence | Recommendation |
|---|---|---|
| REQ-045–048 mock failure/status/log workflows | Behavior report traces exact pinned example and fixture; focused UI tests inspected/executed | Source accepted; no claim of a freshly executed provider/live workflow |
| REQ-049 finite 500-request/repeated reconciliation | Exact pin/source inspected; current default-500 repeat execution unavailable | Unverified live acceptance; no separate defect established solely by absence |
| REQ-050–054/057 truthful histogram/sum/reset/grouping | Actual producer→SQL→shaper→Chart source and independent pure probe | FAIL: findings 1–5 |
| REQ-056 stable derived IDs/native override | Shared derivation and golden/native tests; specialist source coverage | Source accepted; keep stable encoding during finding 6 remediation |
| REQ-058/060 scoped series metadata | Index reads use series, but accepted-context keys suppress supported rows | FAIL: finding 6; do not claim index coverage universally correct |
| REQ-059 ordered admission/cache lifecycle | Ordinary series admission awaited before point dispatch; bounded post-success cache | Source accepted, with scoped-context correction and AC-038 proof outstanding |
| AC-038 crash/restart | Healthy/refusal tests are insufficient for explicit interruption criterion | FAIL required proof: finding 7; no demonstrated data loss |
| REQ-061/AC-039 latency | Caller reports 24h chart reads 315–715ms at dev volume and .6–2.6s at 10x, versus 500ms target; not independently remeasured | Reported target miss, not merely missing evidence. Do not call target met or prescribe a speculative optimization. Current blanket authorization permits deviation; retain explicit observed/reported limit in task/change acceptance. No new approval gate. |
| AC-040 series scaling | Caller reports unchanged series row count, rail/pickers under 200ms, list elapsed time doubled at 10x for undiagnosed reason | Row-count property has supplied positive evidence; elapsed-time scaling remains unsettled. Do not turn doubled time alone into proven point scanning: SQL uses index and unchanged rows support distinction. |
| Revision 23 ordering/keyset / raw integer accuracy | Invariant report covers typed cursors, ties/nulls, decimal intermediate/exact text; focused tests pass | Source/focused acceptance, with no claim of independently repeated real SQL/browser coverage |
| Bloom/schema/WAL/fork boundary | Persistent specialist inspected pinned fork, scalar predicate-only path, shared reader, schema/fingerprint and tagged WAL consumer; caller supplies library/journey/fork passes | Carry scoped acceptance and supplied execution evidence; no new durability/codec/migration machinery justified |
| Oracle publication test synchronization | Changed test observes settled durable file state before warm probe; exact identities, one repin, release assertions retained | No weakened assertion finding. Historical diagnostician artifact/trace unavailable; preserve this evidence limit rather than asserting independent reproduction. |
| Mandatory repository rules / generated parity | Standards report covers all 61 files; generator checks pass; new Rust docs inspected | FAIL only for finding 8; broad Rust/codegen reruns not independently claimed |
| Desktop/mobile light/dark, keyboard/touch/overflow, AC-030/036 visual checkpoints | Browser tooling unavailable and no new human signoff supplied | Unverified/unaccepted visual checkpoint. Do not label visual defects without evidence or silently claim signoff. Caller authorization of deviations introduces no new approval request. |
| No replacement durable/security/transport owner | Existing loader/executor, Gate/Scribe, canonical schemas and shared Chart retained; specialists cover relevant boundaries | Source accepted; preserve adjacent behavior in all corrections |

Human-supplied latency/filter/NOTIFY concerns remain distinguishable from independently validated findings. Point-only predicates are explicitly refused by the index path's current documented contract; with caller-approved deviations, this does not create a new blocker here. An unproven NOTIFY timing assumption alone is not a demonstrated production failure. The scope above does not authorize inventing new benchmarks, synthetic host load or a cache redesign.

## Verdict recommendation and closure boundary

Recommend **FIX_REQUIRED**, with findings `FIND-TASK-004-1` through `FIND-TASK-004-8` in one bounded remediation packet. The numeric/index defects and explicit proof/rustdoc gates independently prevent PASS. Source is available, immutable and fully assigned; BLOCKED is not warranted. Current authorization covers potential scope deviations, so SPEC_REVISION_REQUIRED is not warranted for the selected corrections.

Task remediation should run focused tests for these causes plus the narrow UI/Rust lanes for its actual write set. Do not require another entire broad suite, fork retest or full user-journey collection for unchanged boundaries. Change review retains the remaining integrated/live/visual acceptance accounting. No prior remediation convergence assessment applies to this first review.

Skipped fresh Cargo/Postgres/fork/browser runs and live latency measurement; visual acceptance remains unperformed, and supplied performance misses remain disclosed.
