# Remediation review subject and coverage

Repository: /home/thorrester/Documents/GitHub/wyrd.
Original base: 9692bfd06c6bdf2585e88c791884a90669a5dc26.
Prior candidate: ed82f63bf35ca124c0a2980c160f0d64535eebd1.
Current candidate: 0c2bfe70d5186dff4dc224c7e402b57c24f354d9.
Review complete incremental diff `git diff ed82f63bf 0c2bfe70d`, all eight prior finding closures and affected callers, consumers, invariants and tests. Cumulative original range is available as context. Initial HEAD is current candidate and tree is clean. Do not edit source or prior reports. Only assigned reports may be written here. Candidate movement blocks review.

## Authority and prior acceptance

Read AGENTS.md, applicable nested AGENTS (none below the currently changed UI/redux paths per prior inventory; confirm), changes/spec-driven-development.md, agent_docs/maintainer-style.md, applicable Rust/errors/agent-surfaces topic guides, TESTING.md and owning architecture sections. CodeGraph exists: consult it before code search/read. UI work also uses .agents/skills/wyrd-ui/SKILL.md and brand authority.
Approved spec: changes/active/wyrd-ui-main-integration/spec.md revision 27 with caller-authorized reconciliation of context-aware index granularity. User's explicit approval of potential spec deviations persists; no renewed approval gate inferred, no waiver of incorrect values or mandatory gates.
Original tasks: changes/active/wyrd-ui-main-integration/tasks/TASK-004-observe-components.md, TASK-022-metrics-page.md (superseded), TASK-004-revision/TASK-04-bifrost-bloom-pruning.md. Current closure transfers through TASK-02/TASK-03 as recorded previously.
Prior review: changes/active/wyrd-ui-main-integration/review/range-9692bfd06-ed82f63bf-20261104/ (read verdict.md, findings-validation.md, original reports and TASK-004-R1-metrics-correctness-and-index-coverage.md including appended evidence).
Retain FIND-TASK-004-1 through -8. Do not assign new IDs for the same cause; next unused ID is -9.

Carry prior source-backed acceptance for unchanged dev workload, sort/paging, raw integer transport, canonical fingerprint/WAL fixture, Bloom/fork/dependency/permission boundaries. Reopen only when fix/dependency/authority changes can affect these, or a specific prior proof gap/new material evidence warrants it; record reason. User requests FULL re-review, so cover every changed item, every prior closure and affected paths rigorously; this is not automatic rediscovery of every unrelated pre-base feature.

## Starting navigation map (expand independently)
UI root crates/wyrd/wyrd-server/wyrd-ui:
- packages/ui/src/lib/observe.ts metricQueries/histogramQuery, histogramWindows/Props, KeyedChartProps, loadMetric/named; changes reset before windowing, histogram group ranking/apart/totals, missing-sum availability, typed group/measure identity, per-value text/summary qualifiers. Consumers Chart.svelte, plot.ts recordedSeries/identityCaptions and Observe +layout.svelte. Tests ui-pages.test.ts, ui-library.test.ts, ui-library.integration.test.ts and ui-pages.integration.test.ts.
- Shared Chart/plot changes affect numeric charts and authored charts as sibling consumers; inspect preservation of existing generic summation/folding and exact value presentation.
- ui-signals.integration.ts exportOtlp optional headers affects sibling test fixture calls. Metrics browser journey uses existing observed-service fixtures and release-scoped rail reads.
Rust root crates/vala/vala-bifrost-redux:
- tables/metrics/series.rs CONTEXT_COLUMNS/SeriesKey/series_rows; scribe/series_index.rs seen/in-batch suppression and v2 derived frame identity. Trace prepared point native correlation and Gate-stamped dimension producers through row production/cache/ordinary ordered admission into filtered metadata consumers. Preserve stored series_id encoding.
- scribe/tests/scribe_persistence_path.rs: accepted-context coverage and new crash_once_durable/restarted_scribe tests, retained WAL, faults and recovery; inspect actual nonvacuous crash boundaries/ack state/lifecycle and existing helpers, not just names. tables/metrics/series.rs and tables/mod.rs new/changed rustdoc and tests.
- spec.md and architecture/bifrost-design.md reconciliation of accepted semantic context; compare actual copied/queryable columns and avoid ephemeral-admission-driven indexing.
- Prior review packet added to Git in this incremental range; treat it as preserved acceptance context, not newly executed source.

## Available verification evidence

Read exact appended commands/results, not just completion claims. Reported: UI unit tests (22), Rust focused tests (17), two retained-WAL crash tests and accepted-context case, pnpm check/build, fmt/clippy, filtered Metrics browser journey (one pass), diff --check. Metric-math integration passed earlier in remediation; caller says UI code unchanged since. No codegen check (no generated inputs changed), no unrelated Postgres pg_tests. Recovery fixture proves one restart; it lacks long-term storage and retires replayed WAL rather than persisting a second restart. Assess adequacy for explicit AC038 without inventing an unbounded crash matrix. Performance miss and absent human visual acceptance remain prior disclosed limitations.
Coordinate Cargo execution sequentially with root; no concurrent Cargo builds and no synthetic host load. Distinguish supplied results from independently run checks.

## Reviewer selection

behavior-rev: complete incremental diff/closure coverage, all affected behavior/callers, including UI and standards ownership/reuse not delegated elsewhere.
invariant-rev: materially rewritten SQL, numeric/identity/missing/qualifier invariants F1–F5, plus sibling generic/metric Chart callers.
domain-rev-persistent-data: material context-key/deterministic retry/index integrity and actual retained-volume recovery F6/F7, producer-to-consumer plus failures.
repo-rev: mandatory changed-item rustdoc F8, all incremental surfaces/authority/generated rules and changed helpers/tests.
No separate maintainer/system agents: behavior reviewer covers maintainer/reuse and domain reviewer covers changed lifecycle/recovery; no unrelated ownership or runtime production topology replacement.
Fresh independent ponytail validation after discovery; selected reports must all complete. Followup only for a real source conflict/unresolved path or independent validation of a new validator finding.
