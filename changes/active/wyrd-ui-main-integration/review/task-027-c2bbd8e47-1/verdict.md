# TASK-027 cumulative re-review — FIX_REQUIRED

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Original base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; immutable candidate `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`.
Approved spec `changes/active/wyrd-ui-main-integration/spec.md` revision 12; original task `tasks/TASK-027-view-layout.md` in that packet.
Prior verdict/validated ledger/R1 evidence: `../task-027-ab1d2499f-1/`.
[subject.md](subject.md) records full cumulative scope and attribution. Source and HEAD remained unchanged; unrelated upstream and separately owned TASK-003 changes are not TASK-027 findings.

## Independent evidence

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| repo-rev | [standards-review.md](standards-review.md) | PASS |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL |
| system-rev | [system-review.md](system-review.md) | PASS |
| interaction domain-rev | [domain-review-interaction.md](domain-review-interaction.md) | FAIL |
| fresh focused followup-rev | [followup-review.md](followup-review.md) | RESOLVED |
| fresh ponytail-rev | [findings-validation.md](findings-validation.md) | Two active findings |

All required reports are complete. [claims-comparison.md](claims-comparison.md) groups shared causes and records why fresh follow-up was required: newly browser-observed completion/editability transitions exposed incomplete gesture lifetime beyond the prior Escape-only correction. Follow-up and independent validation consolidated these terminal paths under existing FIND-027-2. The arbitrary-ID dictionary issue remains one new FIND-027-4, including its measurement writer. Only independently validated proposals are included.

## Prior closure and reconciled acceptance

| Obligation | Implementation/proof | Result |
|---|---|---|
| Bounded strict layout, width rejected, view-only schema, size-agnostic catalog components | Existing schema/catalog; exact contract test and codegen pass | PASS |
| Fixed 12-column/40px geometry and continuously measured intrinsic occupancy | rowsFor/place, offsetHeight observation, fixed CSS tracks; ordinary-ID tests and historical measured browser evidence | PASS for original FIND-027-3; special-ID measurements fail below |
| Whole document includes all retained sibling edits | done overlays every actual completion; non-echo and feedback tests | FIND-027-1 CLOSED for original cause |
| Escape ends active pointer producer and capture | Shared cancel/release cleanup; focused/unfocused grip/corner tests and fresh domain browser check | Original FIND-027-2 Escape path repaired |
| All terminal interaction entries prevent later producer writes | commit does not release; editable loss removes controls without cancelling; fresh domain browser probes and independent source validation | FAIL: revised FIND-027-2 |
| Valid arbitrary panel IDs preserve authored layout and callback validity | Inherited ordinary-object reads and Svelte proxy measurement writes mishandle constructor/toString/__proto__ | FAIL: FIND-027-4 |
| Pure collision displacement stays render-only; immutable input and no layout-only reload | Shared placement and content/source keys; exact editing/grid tests | PASS within ordinary-ID scope; retained-state defect above remains |
| Components fill boxes, overflow contained, charts redraw | Existing fill/Chart measurements and original/R1 browser evidence | PASS within recorded scope |
| Narrow steady-state stacking by y/x, explicit h retained, controls absent | Width/order/styles and exact narrow test | PASS in steady state; active transition fails under FIND-027-2 |
| Gallery/example/MCP/mountView use one renderer; MCP edits remain render-local | Host source traces, checks/builds, original human approval 2026-10-20 | PASS within recorded scope; shared renderer failures above apply |
| No persistence/new dependency/component size input/add-remove/collapse/framework | Complete attributed diff and standards coverage | PASS |

Full obligation matrices are in both implementation reports and reconciled against [findings-validation.md](findings-validation.md). FIND-027-1 and FIND-027-3 remain closed. FIND-027-2 is revised at its common producer/preview-lifetime boundary: do not misreport the direct Escape repair as broken, or assign separate findings per blur/Enter/breakpoint/host. FIND-027-4 is a distinct dictionary source and does not reopen the original sibling-serialization or grid-pitch diagnoses.

## Active validated ledger

| Stable ID | Status | Class | Correction boundary |
|---|---|---|---|
| FIND-027-2 | REVISED | INCORRECT | Complete renderer gesture lifetime at completion and loss of editability |
| FIND-027-4 | REVISED | INCORRECT | Own-entry retained/measurement reads plus safe reactive arbitrary-ID measurement writes |

The validator accounts for all discovery IDs, validates all writers/callers and selects minimal complete corrections. Reads-only dictionary guards are insufficient: the installed Svelte deep proxy also drops inherited-key measurement writes. No public/schema/security/persistence decision is required.

## Verification and disposition

[verification.md](verification.md) records fresh green nine exact selectors, UI/example workspace checks/builds, codegen and diff check; the ten-test file was independently rerun in this task pair. Domain review freshly reproduced held-pointer blur duplicate completion and breakpoint-stranded preview in existing Chromium/gallery/mountView facilities; no new harness/source was written. Independent validator reproduced schema/geometry/proxy corruption and confirmed feasibility of the existing readonly dimension binding mechanism. Historical geometry/human evidence remains distinct from fresh execution; Linux/live infrastructure and broad aggregates were not rerun or inferred to pass.

Verdict: **FIX_REQUIRED**. Route [TASK-027-R2-gesture-terminals-and-panel-ids.md](TASK-027-R2-gesture-terminals-and-panel-ids.md) directly to `$wyrd-implement`. Review does not implement, merge, push or deploy. Next review must include original task, R1/R2 and both ledgers over the complete cumulative candidate.
