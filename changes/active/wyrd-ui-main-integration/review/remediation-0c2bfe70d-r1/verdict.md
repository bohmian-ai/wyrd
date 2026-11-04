# Full remediation review verdict

**FIX_REQUIRED**

**FIND-TASK-004-1 through -6 are closed. FIND-TASK-004-7 and -8 remain open with smaller residual corrections. New affected-path regression FIND-TASK-004-9 is confirmed.** No production data-loss defect is established.

## Immutable subject and acceptance context

Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Original base: `9692bfd06c6bdf2585e88c791884a90669a5dc26`.
Prior candidate: `ed82f63bf35ca124c0a2980c160f0d64535eebd1`.
Current candidate: `0c2bfe70d5186dff4dc224c7e402b57c24f354d9`.

Reviewed the complete incremental `git diff ed82f63bf 0c2bfe70d`: seven commits, 27 changed files, +2,052 / −200, plus all eight original finding closures and affected callers/consumers/invariants/tests. The original cumulative range remains context. Prior candidate is the merge base; no rebase correspondence issue exists. `e214d8505..0c2bfe70d` adds only implementation evidence, so the reported implementation endpoint corresponds to current production source. Initial tree was clean; final HEAD still matched current candidate and no tracked source changes were made.

Approved authority is [spec.md](../../spec.md) revision 27 with existing context reconciliation and continuing caller deviation authorization. Original [TASK-004](../../tasks/TASK-004-observe-components.md), superseded [TASK-022](../../tasks/TASK-022-metrics-page.md), and [Bloom task](../../tasks/TASK-004-revision/TASK-04-bifrost-bloom-pruning.md) remain context with their TASK-02/TASK-03 owner transfers. Prior [verdict](../range-9692bfd06-ed82f63bf-20261104/verdict.md), [validated ledger](../range-9692bfd06-ed82f63bf-20261104/findings-validation.md), and [R1 remediation/evidence](../range-9692bfd06-ed82f63bf-20261104/TASK-004-R1-metrics-correctness-and-index-coverage.md) are retained. Approval of deviations avoids a renewed scope gate; it does not establish false numerical results or unobserved recovery events.

## Scope, independent topology and results

[scope.md](scope.md) records the shared navigation map, full changed/affected scope, reasons for specialists and carried-forward acceptance. Four fresh discovery agents independently expanded that map without one another's conclusions. A separate fresh focused reviewer resolved the F7 evidence dispute, then a fresh independent validator inspected the source and every proposal. The orchestrator routed inputs and wrote this verdict from that validated ledger; it did not validate its own findings.

| Role / report | Coverage and reason | Result |
|---|---|---|
| [Behavior review](task-review-behavior.md) | Entire incremental executable diff, all closures, affected UI/host/caller paths and maintainer/reuse obligations | FAIL; new B-R1-01 |
| [Invariant review](task-review-invariants.md) | Material SQL/numeric/identity/missing/qualifier changes and shared Chart siblings, F1–F5 | PASS; original causes close |
| [Persistent-data review](domain-review-persistent-data.md) | Context encoding/suppression/retry integrity, native/Gate producers, ordered admission and actual recovery lifecycle, F6/F7 | FAIL; F6 closes, F7 residual proof gap |
| [Standards review](standards-review.md) | All 27 changed surfaces, applicable authorities/rules, changed Rust helpers/tests and F8 | FAIL; original omissions fixed, one new const lacks mandatory docs |
| [Focused follow-up](followup-review.md) | Material conflict over F7: readable COMMIT versus completed sync/terminated old owners | RESOLVED; source shows F7 proof incomplete |
| [Independent validation](findings-validation.md) | Every closure/proposal, source/materiality/minimum correction and affected paths | Recommends FIX_REQUIRED; retained F7/F8/F9 |

Separate maintainer/system agents were not recreated automatically: behavior covered affected ownership/reuse, and data specialist covered changed lifecycle/recovery. All selected reports are complete. [claim-comparison.md](claim-comparison.md) records grouping and follow-up rationale. There was no new validator-originated finding needing another reviewer and no unresolved conflict after independent source validation.

## Prior closure and reconciled acceptance matrix

| Obligation | Reconciled implementation and verification evidence | Result |
|---|---|---|
| F1: histogram group/measure identity; no adding/folding unrelated latency | Typed keyed output reaches existing recorded-series plotting; six-group and distinct-bound/null-marker fixtures; independent unit pass | CLOSED |
| F2: busiest 20, totals, apart mode and selected-ID labels | Histogram SQL ranks by count, computes total, keeps series in apart; existing label owner used; real SQL and browser cases executed | CLOSED for original missing query/control cause |
| F3: missing means versus real zero | SQL availability tallies and shaper preserve null, including mixed contributors/cumulative baseline; real SQL missing/mixed/zero fixtures pass | CLOSED |
| F4: estimates/lower bounds retain qualifiers | Per-point text reaches tooltip and conservative summaries; exact mean correctly labeled; component/loader fixtures pass | CLOSED |
| F5: reset increments before windows, preserved integer/gauge/delta semantics | Point-level lag precedes window aggregation in both plans; real query asserts 15/60, decrease 14/60, buckets 8/7, delta 7/60 and exact gauge | CLOSED |
| F6: complete context-aware index with stable stored series id | Same widened key reaches row/cross-slice/process suppression and derived batch identity; native/context case and release-scoped browser rail pass | CLOSED |
| F7: actual post-sync unanswered interruption and stopped old owner before replay | Existing tests recover nonvacuous data, but crash trigger sees readable COMMIT before separately awaited sync; old ownership/IO completion unobserved | OPEN: missing required boundary proof, not demonstrated data loss |
| F8: all changed Rust items documented | Original helper/test/nested assertion omissions fixed; new `CONTEXT` const has no rustdoc despite explicit every-item rule | OPEN: bounded mandatory documentation correction |
| Histogram visibility across grouping/apart navigation | New keyed Chart instances retain local old selection keys without existing host per-view bindings | FAIL: new F9, independent of corrected math/ranking |
| Changed authority/index granularity, tenant/validation/dependency/generated rules | Full standards map; context reconciliation matches existing stamped/native contracts; no new generated input or durable owner | Accepted except F8 |
| Unchanged dev/sort/raw transport/fingerprint/tagged WAL/Bloom/fork/permission boundaries | Prior source-backed evidence carried; production owners/dependencies untouched; any shared Chart effects explicitly reviewed | Accepted by carry-forward |
| Existing performance/AC040 and human visual limits | Prior supplied latency miss and unsettled elapsed-time cause retained; automated browser pass does not establish human signoff | Disclosed unchanged limitations, no new approval/optimization gate |

Only directly affected paths were reopened: typed histogram selection changes can affect retained view state, and new crash tests require checking their actual event ordering. No unrelated complete rediscovery occurred. F1–F6 closure is source-backed and remains available for the next round.

## Validated retained ledger

Full exact evidence, caller/writer tracing, materiality and correction burden are in [findings-validation.md](findings-validation.md); the following ledger summarizes only independently retained findings.

| Stable ID | Status / source IDs | Diagnosis and consequence | Minimum selected correction |
|---|---|---|---|
| FIND-TASK-004-7 | REVISED; prior SYS-01, DATA-R1-01, focused follow-up | `scribe_persistence_path.rs:1539–1568` detects readable COMMIT before separate awaited SyncWal; dropping ingress/Scribe does not establish old owner/IO termination. Passing reconstruction counts therefore do not prove the claimed durable interruption. | Observe existing post-sync retained group with response unanswered; use existing abort/retained-task/WAL drain lifecycle and establish completion before replacement; retain current ordinary replay/retry/8-point assertions. No production durability redesign or second restart. |
| FIND-TASK-004-8 | CONFIRMED; REPO-R1-1 continuing prior REPO-1 | New test-local `const CONTEXT` at `scribe_persistence_path.rs:1301` lacks rustdoc under the explicit every-item gate. | One accurate fixture-purpose rustdoc, batched with F7/F9; inspect all actually added Rust items together, no runtime test or style-only round. |
| FIND-TASK-004-9 | CONFIRMED; B-R1-01 | Histogram latency/rate Charts at Observe layout `:344–367` retain local selected keys after mode/group changes; old/new typed identity tuples are disjoint and populated plots draw no lines. | Reuse existing host per-view selection map and Chart shown/callback bindings, independently for latency/rate, preserving same-view and sibling/generic behavior. |

The behavior report's provisional F7 closure is rejected as complete closure after independent ordering/lifecycle tracing. Its F8 claim closes the original listed omissions only. Whitespace in the newly committed prior system report is rejected as a blocker; the impossible fixed-size digest panic concern is not retained. Neither generates remediation work.

## Verification and limits

Independent root executions all passed:

- 22 UI unit tests across the affected page/library files.
- Three exact Rust tests: both new retained-WAL recovery cases and accepted-context case.
- Actual metric-math integration against fresh repository Postgres: 1 selected test passed.
- Filtered authenticated Metrics browser journey against fresh Postgres/rebuilt BFF: 1 selected journey passed.

[verification.md](verification.md) records exact commands, selections, results and commit correspondence. Other implementation check/build/fmt/clippy/17-case results remain supplied evidence, not newly rerun wholesale here. These green runs establish their actual assertions; they cannot replace missing timing/interaction assertions. No generated input changed; no new full codegen requirement is imposed.

The immutable range whitespace check reports a new blank EOF line in the preserved prior system report; plain clean-working-tree `git diff --check` does not test that range. This report-only issue is nonblocking and prior reports remain unchanged. Broad suites, manual visual inspection, new latency measurements and synthetic load were not run. Existing performance misses and absent human visual acceptance remain explicit. The one-restart fixture limit does not become an unrequested second-restart obligation.

## Remediation and next review

Execute [TASK-004-R2-recovery-boundary-and-histogram-selection.md](TASK-004-R2-recovery-boundary-and-histogram-selection.md) through `$wyrd-implement`. It contains all three diagnoses, source-backed selected corrections, constraints/non-goals, acceptance mapping and focused proof; no rediscovery or new architecture decision is needed. No implementation, commit, push, merge or deployment was performed by this review.

This is the first unsuccessful remediation review; the two-unsuccessful-remediation convergence threshold is not reached. Causes are bounded: F7/F8 incomplete closure, and F9 a fix-induced host-state regression. The next review checks `0c2bfe70d..<new-candidate>` and F7/F8/F9 affected paths, carrying F1–F6 and unchanged accepted evidence forward unless concrete changed dependencies/new evidence invalidate it. It must not restart numeric/index discovery merely because another round began.
