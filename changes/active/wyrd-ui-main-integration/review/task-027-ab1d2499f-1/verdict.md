# TASK-027 — FIX_REQUIRED

## Immutable subject

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`.
Approved specification `changes/active/wyrd-ui-main-integration/spec.md`, revision 12; original task `changes/active/wyrd-ui-main-integration/tasks/TASK-027-view-layout.md`.
The cumulative range and resulting source were reviewed; task implementation belongs to `d33de26e4`/`d90d98a12`, with unrelated merged upstream changes distinguished as recorded in [subject.md](subject.md). TASK-003 is a dependency reviewed separately. Candidate and tracked source stayed unchanged.

## Independent results and follow-up

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| repo-rev | [standards-review.md](standards-review.md) | PASS |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL |
| system-rev | [system-review.md](system-review.md) | FAIL |
| interaction domain-rev | [domain-review-interaction.md](domain-review-interaction.md) | FAIL |
| fresh focused followup-rev | [followup-review.md](followup-review.md) | RESOLVED |
| fresh ponytail-rev | [findings-validation.md](findings-validation.md) | Three retained findings |

All required reports are complete. [claims-comparison.md](claims-comparison.md) groups shared causes and records the mixed-layout evidence conflict. A fresh focused follow-up measured that path in the existing AppBridge browser harness, resolved the disagreement, and supplied its artifact identity and measurements. A separate fresh validator then traced every proposal, revising geometry closure to distinguish genuine collisions from wrong pitch. No orchestrator-originated finding is included. Rejected inline-loader speculation and nonblocking style notes do not enter remediation.

## Reconciled acceptance matrix

| Obligation | Implementation and proof | Result |
|---|---|---|
| Whole-unit bounded optional layout; reject width; no component size input | Strict LayoutSchema/PanelSchema and generated catalog; exact contract test and independent catalog check pass | PASS |
| Pure collision push-down, immutable authored documents | place copies input and advances later collisions; focused tests | PASS for fixed geometry; intrinsic occupancy fails below |
| Fixed 40px rows with mixed content-height panels | Auto panels modeled as h=1; CSS stretches intrinsic row while edit pitch stays fixed; fresh browser measurement | FAIL: FIND-027-3 |
| Components fill boxes, internal overflow and chart redraw | Existing fill/overflow/Chart measurement; supplied approved browser evidence | PASS within supplied scope |
| Handler opt-in, grid-snapped keyboard/pointer editing | Conditional affordances, shared edit function; focused tests and historical normal gestures | PASS for normal paths |
| Whole updated document once per completed change | commit serializes only current edited panel despite retained sibling edits | FAIL: FIND-027-1 |
| Escape cancels active pointer move/resize | key clears preview but captured move/up producer remains active | FAIL: FIND-027-2 |
| Layout edits and host echo never reload; source not mutated | Layout-stripped content key and stable source; exact editing and TASK-003 tests pass | PASS |
| Below 720px stack by y/x, retain h, disable editing | Measured width, ordering and height styles; exact narrow test and supplied mobile evidence | PASS |
| Gallery/example/AppBridge and per-render MCP state | Existing renderer/mount wiring and human approval 2026-10-20 | PASS within supplied scope; findings affect shared renderer |
| No persistence/add/remove/collapse/dependency/size-input expansion | Full owned diff and standards coverage | PASS |

## Validated ledger

Exact locations, complete producer/caller traces and selected minimal corrections are in [findings-validation.md](findings-validation.md).

| ID | Status | Class | Consequence |
|---|---|---|---|
| FIND-027-1 | CONFIRMED | INCORRECT | A later whole-document callback omits earlier completed sibling edits |
| FIND-027-2 | CONFIRMED | INCORRECT | Movement can revive and commit a pointer edit after Escape |
| FIND-027-3 | REVISED | INCORRECT | Intrinsic content changes fixed-grid pitch and later authored coordinates |

No prior finding/remediation ledger exists in this first review. All corrections remain within approved behavior and existing renderer/placement owners; no new product, API, persistence or architecture decision is required.

## Verification and disposition

[verification.md](verification.md) records fresh successful exact commands for all four TASK-027 scenarios, TASK-003 regressions, whole seven-test file, package artifact build and UI/package checks. Standards independently checked catalog drift. Historical broader builds/codegen and human/browser acceptance remain distinct from reruns. Existing tests miss cross-panel callback retention, Escape during a held pointer gesture and computed intrinsic grid geometry. Focused follow-up supplied fresh browser geometry evidence; no full browser/journey/release aggregate was rerun. These are bounded renderer findings, not Linux or production MCP-registration obligations.

Verdict: **FIX_REQUIRED**. Route [TASK-027-R1-layout-state-and-geometry.md](TASK-027-R1-layout-state-and-geometry.md) directly to `$wyrd-implement`. Review does not implement, merge, push or deploy. Re-review the complete cumulative original task after remediation, preserving FIND IDs and this evidence.
