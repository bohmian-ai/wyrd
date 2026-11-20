# TASK-004B-R1 re-review — FIX_REQUIRED

Five original findings have credible closure. FIND-TASK-004B-4 remains partial;
the remediation introduces confirmed FIND-TASK-004B-7. Approved Revision 1
loading and visual acceptance is not complete. Continue through the separate
[TASK-004B-R2](TASK-004B-R2-complete-support-lifecycle.md); do not restart closed
work or silently rewrite earlier review/implementation evidence.

## Subject, authority and source stability

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Original base: `2ee8bddcab3fff94a75ba35fbcdc4a91a9b9f49f`.
- Prior candidate: `7e2b3f6fab8c81fb85769eee37d0c7fd6b633146`.
- Current HEAD: `79a4ef66190e572c47aa93351e7362329f7bc73f`.
- Remediation committed range: `git diff 7e2b3f6fa 79a4ef661`; cumulative
  original range remains available. Ancestry is preserved.
- Explicitly requested review subject: current tree, including uncommitted
  `streamed.test.ts`, `ui-pages.integration.test.ts`, `ui-pages.test.ts` and R1
  evidence. Passing execution claims refer to that tree, not HEAD alone.
- Contract: approved SPEC-verification-workflow-ui revision 6, supporting
  SPEC-wyrd-ui-main-integration revision 28, original TASK-004B and R1 including
  the maintainer-approved Revision 1 browser acceptance.

Prior review and R1 are in `../TASK-004B-2026-11-18-01/`. Root/affected AGENTS,
spec-driven development, maintainer style, TESTING and the applicable architecture
owners continue to govern. Nothing here changes an approved product contract.

Before and after source review, HEAD and the inspected file hashes were
unchanged. The SHA256 of the sorted path-to-content-hash map for tracked
differences from the prior candidate plus prior review Markdown was
`042ed8d3cd52a02c01bd94aeac57d17cef78a1f1072cdbe0c646c3583e024ba8`.
This fingerprint identifies the inspected working tree; it is not a commit or
a substitute for a final immutable completion candidate.

The user initially required read-only assessment, so review performed no
implementation writes or runtime runs. The user's subsequent explicit request
to follow the standard authorizes these new review reports and remediation
task. Earlier reports, R1 and implementation files remain unchanged.

## Independent assignments

- Fresh `r1_ponytail_rev`: remediation closure for FIND-1 through FIND-6,
  affected callers, evidence, coverage, TDD and regression discovery. Its
  returned assessment is recorded in `task-review-remediation.md`.
- Fresh `r1_lifecycle_check`: separate focused validation of the newly
  discovered duplicate-validation seam and unresolved cancellation/loading
  paths. Its returned assessment is recorded in `lifecycle-validation.md`.

The remediation reviewer carries forward unchanged accepted boundaries; a new
full first-review fanout would repeat settled work. A focused lifecycle reviewer
was selected because split support requests materially alter cancellation and
context ownership and the fresh reviewer discovered a new affected-path issue.
Additional repo/domain/system reviewers were unnecessary: server production
behavior did not change in R1, analytical fixes are bounded, and their actual
proof/assertions were checked. The calling agent orchestrated and packaged the
returned reviews. No selected report is missing and no reviewer validated its
own newly discovered finding.

## Reconciled acceptance matrix

| Existing obligation / scenario | Implementation and assertion/execution evidence | Status |
|---|---|---|
| REQ-014 / original scenario 2 / FIND-1 | Per-stat matched-population projection; complete original/new series comparison and rendered sparse StatStrip trends in `ui-library.integration.test.ts:533–610`; recorded focused integration pass | PASS within supplied execution evidence |
| REQ-018 consolidation / scenario 5 / FIND-2 | List-only client load plus population-specific support; browser zero-support-reread assertions for list choices and one support read on population change | PASS for this finding |
| Scenario 5 and verification §4 / FIND-3 | Schedulers await invalidation and support; real browser clock holds support over two opportunities, failure/success settlement, pause/departure and five-list-read assertion | PASS |
| Scenario 5 and R1 Revision 1 / FIND-4 | Post-header SQL abort works; pre-header startup and Card names are uncancelled; proxy bypasses both missing stages | FAIL |
| REQ-014 / scenario 2 / FIND-5 | Populated real facet cutoff/null/native fixture; metered/unmetered gateway calls and distinct EUR/USD original/new results with independent facts | PASS within supplied execution evidence |
| REQ-018, AC-012 / scenario 0 / FIND-6 | Oracle both-form denial/tenant/deadline tests, ANALYZE cancellation/cleanup and resource refusal, MCP row/byte caps; recorded focused executions | PASS within supplied execution evidence |
| Scenario 5 one validation per refresh / new FIND-7 | List/support create distinct contexts; each validates Observe custom SQL. Prior within-context memo proof cannot carry across the changed seam | FAIL |
| R1 Revision 1 explicit loading/visual acceptance | Previous counts/charts appear under a new population without pending indication; initial Observe overview absent. Required rulings remain unapproved | FAIL / ruling unresolved |
| Retained Back/error/first-response and analytical composition | Existing accepted evidence plus recorded six browser journeys; no specific new defect established outside the rows above | PASS carried forward within stated limits |

Full finding records and source-validation decisions appear once in
[findings-validation.md](findings-validation.md). FIND-4 retains its existing
ID. FIND-7 is Observe-only REGRESSION; the proposed Verify variant was rejected.
Revision 1 loading acceptance is approved human scope and is not relabeled as a
new independently discovered finding.

## Verification, waits and process limits

The implementer records 61 focused unit passes; `ui:check` with zero type
errors and 93 passes; six focused browser journeys; two analytical integration
passes; and four focused Oracle selectors with format/lint success. Reviewers
checked relevant assertions and source, but did not rerun those commands. No
fresh runtime PASS is claimed. Root independently found no scratchpad path,
counts.txt or appendFileSync in UI sources and a clean `git diff --check`.
Sixteen screenshots exist in the reported temporary capture directory; this
review did not copy them, approve visible differences or establish permanent
artifact retention.

R1 adds no server production hot-path code or new PostgreSQL/Scribe/object-store/
Vault wait site. Host-private support endpoints call existing loaders, preserving
their existing authorized query operations; they do not add server coordination
IO. FIND-7 records duplicated existing validation, not a newly sanctioned wait.
The earlier hosted L1 benchmark override remains a disclosed performance limit;
no performance qualification is inferred or extended to future changes.

TDD: preserve the original task's recorded nonbehavioral RED/test-order
violations and the R1 malformed-JSON test-after-fix violation. A later mutation
check strengthens the assertion but does not establish historical test-first
order. No retroactive RED or churn is required.

This is the first unsuccessful remediation review, not two unsuccessful rounds.
The remaining scope consists of an incomplete original cancellation correction,
a fix-induced context regression, and already-approved visual acceptance. There
is no automatic expansion or need to reopen the closed analytical/EXPLAIN work.

Verdict: **FIX_REQUIRED**, not completion. No commit, merge, push or deployment
is authorized by this verdict.
