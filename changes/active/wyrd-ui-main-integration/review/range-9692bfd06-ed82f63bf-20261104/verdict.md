# Task range review verdict

**FIX_REQUIRED**

Eight independently validated findings remain. Review changes reports only; implementation was not modified, committed, pushed, merged or deployed.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `9692bfd06c6bdf2585e88c791884a90669a5dc26`.
- Candidate: `ed82f63bf35ca124c0a2980c160f0d64535eebd1`.
- First full review: `git diff 9692bfd06...ed82f63bf`; base is the merge base; 32 commits, 61 changed files, +5,060 / −1,580. No prior acceptance or closure evidence carried forward.
- Approved specification: [spec.md](../../spec.md), revision 27 including revisions 20–27. The caller expressly approves potential spec deviations. That authorization avoids a renewed scope-approval gate; it does not make false measurements correct or establish unperformed acceptance.
- Original assignments: [TASK-004](../../tasks/TASK-004-observe-components.md), historical [TASK-022](../../tasks/TASK-022-metrics-page.md), and [TASK-04 Bloom pruning](../../tasks/TASK-004-revision/TASK-04-bifrost-bloom-pruning.md). TASK-022 is superseded; TASK-004 transfers current closure through TASK-02/TASK-03. The reports follow those transfers for changed behavior without restarting unchanged pre-base work.
- Applicable root/nested rules, topic guides, workflow, UI brand and owning architecture are mapped in [standards-review.md](standards-review.md). Repository/architecture authority governs over stale task prescriptions.

The final identity check still matched the candidate. The only working-tree additions are this review directory. No source diff/archive is stored here; Git identifies the subject.

## Independent topology, scope and results

The orchestrator did not act as a reviewer or validate its own findings. All selected discovery reviewers were separate fresh agents, received the same [navigation map](scope.md), and independently expanded it. They received no other reviewer conclusions during discovery. Every selected report completed before fresh independent validation.

| Role and report | Reason selected / coverage | Discovery result |
|---|---|---|
| behavior-rev: [task-review-behavior.md](task-review-behavior.md) | Complete range, realistic caller/result paths, dev workload, Observe UI, task transfers and acceptance matrix | FAIL; B-01–B-04 |
| invariant-rev: [task-review-invariants.md](task-review-invariants.md) | Material SQL/numeric, reset/window, grouping/filtering, keyset and chart identity invariants | FAIL; INV-01–INV-04 |
| repo-rev: [standards-review.md](standards-review.md) | Broad Rust/TypeScript/Svelte/tooling/generated/authority coverage; explicit 61-file inventory and mandatory rules | FAIL; REPO-1 |
| maintainer-rev: [maintainer-review.md](maintainer-review.md) | Substantial metric loader and index/admission ownership/reuse changes | FAIL; MAINT-001 |
| system-rev: [system-review.md](system-review.md) | Changed ordered durability, failure/recovery/cache lifecycle and publication-test synchronization | FAIL; SYS-01 required-proof gap only, not demonstrated data loss |
| domain-rev-persistent-data: [domain-review-persistent-data.md](domain-review-persistent-data.md) | Series/index integrity, native correlation, schema/fingerprint/WAL compatibility and external pinned-fork boundary | FAIL; DATA-01 |
| fresh ponytail-rev: [findings-validation.md](findings-validation.md) | Independently traced every proposal, materiality and minimum safe corrections; reconciled acceptance/limits | Recommends FIX_REQUIRED; eight deduplicated findings |

Claim grouping and follow-up decision are recorded in [claim-comparison.md](claim-comparison.md). No focused follow-up was needed: matching proposals shared traced causes, authority resolved the index-context issue, and no material source conflict remained. No new validator-originated blocker arose. A unique finding was source-validated rather than decided by votes.

## Reconciled acceptance matrix

Detailed requirement/source/test mappings remain in the independent reports and validation; these are the reconciled outcomes for this range.

| Obligation | Implementation / evidence | Result |
|---|---|---|
| REQ-045–048 dev provider outcomes, latency/status/error/debug/log behavior | Exact pinned external example and MockServer source traced; fixture syntax verified | Source accepted; live workload not independently repeated |
| REQ-049 finite/repeated 500-request reconciliation | Exact pin and source inspected | Live acceptance unverified; no separate implementation defect inferred |
| REQ-050–054/057 truthful histogram grouping, selection, mean, estimates and reset increments | Producer→SQL→shaper→Chart paths plus independent built-module probes | FAIL: findings 1–5 |
| REQ-056 deterministic ids, native overwrite | Shared derivation and scoped source/test evidence | Source accepted; keep encoding unchanged |
| REQ-058/060 same-context index-backed reads | Native Run and accepted dimension producers traced; existing suppression loses second context | FAIL: finding 6 |
| REQ-059 revision-27 ordered admissions and bounded post-success cache | Ordinary series ingress completes before points dispatch; source/failure-test inspection | Ordering source accepted; context fix and AC-038 proof remain |
| AC-038 interruption/restart/read | New tests prove healthy suppression/refusal, not composed crash/recovery | FAIL required proof: finding 7; no demonstrated point data loss |
| REQ-061 / AC-039 latency | Caller reports 24h charts 315–715 ms at dev volume, 0.6–2.6 s at 10x, versus 500 ms target | Reported miss retained as approved-deviation limit; no speculative optimization prescribed |
| AC-040 series-scaled metadata reads | Caller reports unchanged series rows, list/pickers <200 ms, list elapsed time doubled at 10x | Row-count evidence positive; elapsed-time cause unsettled, not proof of point scanning |
| Revision-23 keyset/sort and raw integer accuracy | Invariant tracing and focused SQL/URL/input assertions; decimal/server-text paths inspected | Source/focused acceptance; real paging execution not repeated |
| Bloom/fingerprint/tagged WAL/permission/dependency boundary | Specialist inspected actual pinned fork, canonical removal/v3, fixture replay path, controls and shared reader; caller provides backend/fork executions | Scoped source acceptance with supplied execution evidence |
| Publication source-transition test | Actual settlement condition added before cache warm; exact rows/one repin/release assertions retained | Source accepted; historical diagnostician report unavailable, not independently reproduced |
| Mandatory contributor/generated rules | Full 61-file standards table; generator and catalog checks pass | FAIL: finding 8 only |
| Component/page desktop/mobile light/dark and interaction acceptance | No new human signoff; browser inventory/create unavailable | Unverified/unaccepted, with no fabricated visual defect or approval request |
| Existing owner/security/tenant/transport boundaries | Shared loader/executor, canonical stamping, Gate/Scribe and Chart reused | Source accepted; preserve adjacent behavior in correction |

The point-only predicate refusal and unproven NOTIFY timing claim are disclosed in validation, not converted into independently demonstrated defects. Caller-authorized deviations do not require another spec gate. Missing independent reruns are not automatically implementation failures. Required crash proof and mandatory rustdoc are explicit obligations, distinguished from that general limitation.

## Validated ledger and correction scope

Only independently CONFIRMED/REVISED findings are retained. Full evidence, exact paths, producer/sibling traces, materiality, correction burden and focused proof are in [findings-validation.md](findings-validation.md) and copied into the remediation task.

| ID | Status / sources | Classification and material consequence | Selected correction boundary |
|---|---|---|---|
| FIND-TASK-004-1 | CONFIRMED; B-01, INV-04 | INCORRECT: histogram captions collide and generic Chart folding adds unrelated means/percentiles | Preserve typed histogram group/measure identity and use existing identity-preserving Chart path; retain generic authored-chart semantics |
| FIND-TASK-004-2 | CONFIRMED; B-02, INV-02 | MISSING/INCORRECT: busiest groups can be omitted; totals/every-series histogram workflow missing | Existing histogram SQL/loader/host rank by count increase, honor apart, disclose total and use selected-ID index labels |
| FIND-TASK-004-3 | CONFIRMED; B-03, INV-03 | INCORRECT: absent/mixed histogram sums fabricate means | Track required-sum availability through SQL and shaping; preserve useful count/bucket estimates |
| FIND-TASK-004-4 | REVISED; B-04 | INCORRECT: top-bucket lower bounds lose per-value qualification; exact mean called estimated | Retain qualifier/estimate meaning through shared histogram loader, tooltip and summary presentation |
| FIND-TASK-004-5 | CONFIRMED; INV-01 | INCORRECT: endpoint collapse loses increments around an intra-window reset | Compute consecutive reset-aware increments before window aggregation for sums/histograms |
| FIND-TASK-004-6 | REVISED; MAINT-001, DATA-01 | INCORRECT/REGRESSION: index dedup discards supported Run/workload/environment contexts; scoped metadata disappears while points remain | One context-aware row/key discriminator across in-slice/cross-slice/process suppression and derived identity; preserve stored series_id and existing context stamping |
| FIND-TASK-004-7 | REVISED; SYS-01 | MISSING required AC-038 proof, without source-proven data loss | Focused actual retained-volume interruption/restart/read through existing recovery, including a nonvacuous recovered-point case |
| FIND-TASK-004-8 | CONFIRMED; REPO-1 | VIOLATION: changed Rust helper/test panic docs and nested helper docs omit mandatory rustdoc | Accurate item/panic docs batched with behavioral remediation, no style-only round |

## Verification, limits and next handoff

Independent observed passes: six focused UI page tests; shared package/host `pnpm check` with zero errors/warnings; six theme generator tests; theme and catalog parity checks; shell syntax; MockServer JSON parsing; range whitespace check. Source-matched built-module probes independently reproduced histogram shaping/selection defects. Exact command/results and limits are in [verification.md](verification.md) and specialist reports.

Caller-reported, not newly executed here: metrics journey, 516 library tests, five Oracle journey passes, touched-crate fmt/clippy and pinned fork regression. No fresh Cargo/Postgres/fork suite, live workload/latency experiment or broad gate was run. Browser tooling could not open a browser, so no visual signoff is claimed. No synthetic host load was generated. These limits do not weaken the source-backed findings or imply all unexecuted behavior is defective.

Remediation: [TASK-004-R1-metrics-correctness-and-index-coverage.md](TASK-004-R1-metrics-correctness-and-index-coverage.md), routed directly to `$wyrd-implement`. It contains diagnoses, selected minimum corrections, preserved behavior/non-goals and focused proofs for all eight IDs. It does not request unrelated broad suites or a new product/API/dependency/recovery architecture.

This is the first review, so there are no prior findings to close and no two-round convergence assessment. The next review retains this candidate, original task, verdict, validated ledger and remediation as cumulative acceptance context; it checks closure, the new incremental diff and affected paths rather than restarting all discovery. Unchanged accepted Bloom/WAL/dependency/dev/sort evidence carries forward unless changed code/dependencies or new evidence invalidate it.
