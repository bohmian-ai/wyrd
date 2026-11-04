---
id: TASK-004-R1
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 27
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-2, FIND-TASK-004-3, FIND-TASK-004-4, FIND-TASK-004-5, FIND-TASK-004-6, FIND-TASK-004-7, FIND-TASK-004-8]
---

# Correct metric results and scoped index coverage

Execute through `$wyrd-implement`. This is one consolidated correction packet, not a new feature/design plan. The caller authorizes potential spec deviations; no renewed approval gate is required for the selected corrections below.

## Subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Original base: `9692bfd06c6bdf2585e88c791884a90669a5dc26`.
Reviewed candidate/prior candidate for the next review: `ed82f63bf35ca124c0a2980c160f0d64535eebd1`.
Approved spec: `changes/active/wyrd-ui-main-integration/spec.md`, revision 27, including revisions 20–27 and the caller's blanket deviation authorization.
Original tasks: `changes/active/wyrd-ui-main-integration/tasks/TASK-004-observe-components.md`, `TASK-022-metrics-page.md` and `TASK-004-revision/TASK-04-bifrost-bloom-pruning.md` under that same tasks directory. TASK-022 is historical/superseded; TASK-004 transfers current live closure through TASK-02/TASK-03. Preserve those owner transfers.
Acceptance context: this directory's `verdict.md`, `findings-validation.md`, six discovery reports, `scope.md`, and `verification.md`. Repository/owning architecture rules remain authoritative. The new index granularity claim must be reconciled with the pre-existing accepted-context/native-correlation contract as described below.

## Outcome, ownership and sequence

A Metrics investigation must show the recorded values truthfully: reset increments survive windowing; unrelated histogram groups/percentiles never add together; absent means stay unavailable; censored estimates retain their qualifiers; busiest/every-series selection works. Index-backed discovery must retain every admitted semantic filter context while preserving stable series IDs and series-before-points durability.

Reuse `packages/ui/src/lib/observe.ts`, existing shared Chart presentation and host controls, canonical point/context stamping, the series row/key owner and ordinary Scribe admission/recovery. Close the SQL/shaper/Chart/host seam together, then the shared series suppression/derived-identity seam and its focused recovery proof. Batch mandatory Rust documentation with these changes. Reversible private method/layout/fixture choices remain implementation-owned.

For each executable scenario below, perform RED on the smallest direct regression, GREEN with the bounded correction, then refactor only if the passing behavior reveals a simpler existing mechanism. Document the selected command, expected failing observation and passing result. Documentation-only changes use source inspection rather than manufactured TDD.

## Diagnoses and decision-complete corrections

The following ledger is copied from independent validation so the implementer need not rediscover defects or select a new correction boundary.


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


## Preserved behavior and non-goals

Keep the exact versioned series-id encoding, native and OTLP correlation semantics, authoritative dimension stamping, tenant isolation, reserved index writes, bounded cache behavior, ordered durable admissions, existing query ceilings, exact integer text/DECIMAL behavior, and compatible histogram merge rules. Preserve generic authored-chart sum/fold semantics for its existing callers. Do not modify the DataFusion pin, scalar predicate/pruning fix, WAL/fence formats, canonical schema fingerprint, permission classifications, unrelated dev workload, sorting or paging.

No new service, transport, table columns, public ChartInput schema, dependency, visibility option, point-scanning metadata fallback, per-point index, speculative cache redesign, cross-table transaction or broad fault matrix. Reconcile the inaccurate index granularity sentence in the existing spec/Bifrost owner; do not add a parallel authority. Do not claim latency or visual acceptance is established by these corrections.

## Acceptance and focused proof

Each finding's focused closure above is mandatory and maps directly to its ID. In sequence: (1) distinct histogram identities and six-group no-fold rendering; (2) real SQL top-20/total and histogram apart control/labels; (3) absent/mixed/cumulative/zero sums; (4) mixed bounded/censored tooltip, loader and summaries; (5) interior cumulative resets for both SQL paths with exact integers retained; (6) native Run and accepted scope coverage within/across batches with unchanged-context suppression; (7) actual retained-volume interruption/restart/read with a nonvacuous recovered point; (8) complete accurate docs on listed changed items. Do not substitute SQL-string assertions for the numeric query execution proofs or a healthy write for the crash proof.

Use the existing UI unit files, filtered Metrics browser journey, and owning Rust inline/persistence tests; extend existing fixtures instead of adding a harness. New test titles/selectors remain implementation-owned: confirm exact names from source/nextest listing and record an exact command for every new named regression. No no-selected/skipped result is proof.

Relevant existing focused commands, run sequentially from repository root unless noted:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts src/lib/ui-library.test.ts
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::scribe_persistence_path::a_series_has_one_row_per_hour_and_repeats_are_suppressed) | test(=scribe::scribe_persistence_path::a_failed_series_admission_fails_the_points_admission)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^tables::metrics::series::tests::/) | test(/^scribe::series_index::tests::/) | test(/^scribe::scribe_persistence_path::/)'
```

The module-filter command covers the changed embedded persistence boundary, not a full backend collection. Select the new retained-volume recovery cases individually once implemented; if they belong in the existing Postgres crash/restart owner, use its repository Postgres wrapper and exact selectors rather than broadening all backend tests.

For the **single existing affected Metrics journey**, reuse setup from `changes/active/wyrd-ui-main-integration/references/verification.md`: build current TS bindings/harness/server/UI only as needed, using the owning mise tasks (`ts:build`, `ts:build:testing`, `build:server`), and run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated metrics investigation$"'
```

This filtered affected-path proof is required for the changed UI/scoped-discovery seams; it is not a demand to rerun every user-journey suite. Use real generated-query execution inside the existing test boundary for the reset/selection cases. Do not introduce a new selector/configuration surface.

Finish with the narrow write-set checks:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- cargo fmt --all -- --check
mise exec -- cargo clippy --locked -p vala-bifrost-redux --all-features --all-targets -- -D warnings
```

Run `mise run codegen:check` only if catalog/generated contract inputs change; fix generators and regenerate rather than editing projections. Extend format/lint scope only if the actual correction touches another owner. No `gate`, full fork suite, unrelated journey collection or synthetic host-load benchmark is required in this remediation. Final integrated change review owns its broader gates once.

Record focused proof/results and update the current task evidence to cover this remediation without rewriting prior reports or claiming stale evidence proves revision 27. The caller-supplied 24h latency miss (315–715 ms dev, 0.6–2.6 s at 10x) and absent visual acceptance remain explicit limits; this packet does not prescribe an unvalidated optimization. AC-040's row-count property and elapsed-time uncertainty remain distinct.

## Next review boundary

Retain the original task, this verdict/ledger, this remediation task and prior candidate. Next review checks finding closure, `ed82f63bf35ca124c0a2980c160f0d64535eebd1..<new-candidate>` and affected consumers/invariants, with the original cumulative range available. Carry accepted unchanged Bloom/WAL/dependency/dev/sort behavior forward unless changed dependencies or new evidence invalidate it. No convergence assessment is needed for this first review. Implementation does not imply permission to commit, push, merge or deploy.
