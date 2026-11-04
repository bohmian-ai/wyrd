# Discovery claim comparison

Immutable subject and reviewer selection: scope.md. This is routing/grouping, not source validation or a verdict.

- B-01 and INV-04 propose one histogram identity/presentation cause: caption merging and generic >20 folding add unrelated latency values.
- B-02 and INV-02 propose one histogram selection/workflow cause: apart ignored, lexical ranking, missing totals and selected-series label flow.
- B-03 and INV-03 propose one missing histogram-sum cause: absent/mixed sums invent a mean.
- INV-01 separately proposes cumulative endpoint collapse loses interior reset increments for sums and histograms.
- B-04 separately proposes loss of per-value lower-bound qualification and mislabeling exact mean as estimated.
- MAINT-001 and DATA-01 propose one index coverage cause at different supported producers: series/hour keys suppress distinct accepted dimensions or native Run correlation. The correction must cover within-slice, across-slice and process suppression plus derived batch identity without changing stored series_id encoding.
- SYS-01 proposes an explicit missing AC-038 recovery proof; it does not allege demonstrated data loss. Validation must assess proof materiality separately from production correctness.
- Standards report, once complete, provides mandatory-gate findings separately.

No focused follow-up selected before validation: these proposals provide source/caller paths, duplicates have compatible diagnoses, and no material reviewer conflict needs another discovery vote. Fresh independent validation will accept, revise or reject each claim and correction. A new validator-originated blocker requires separate focused source validation before retention.

All reviewers distinguish caller-reported backend/fork passes from independent UI checks. Behavior matrix unverified live/performance/visual rows must be reconciled explicitly: absence of an independently repeated run does not alone prove missing implementation. Caller authorizes potential spec deviations; do not invent a renewed approval gate, silently waive defects, or claim unavailable human acceptance.

All six selected discovery reports completed. REPO-1 is one grouped mandatory rustdoc correction covering new series test/helper panic docs and nested assert_untagged docs, to batch with behavior remediation. Independent validation received every completed report and the supplemental caller verification limits. Source candidate remains unchanged.
