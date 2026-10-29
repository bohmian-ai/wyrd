# TASK-027 cumulative R2 re-review — FIX_REQUIRED

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Original base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; immutable candidate `a0a5f177f23960822222b851716460dec02804b3`.
Approved specification `changes/active/wyrd-ui-main-integration/spec.md` revision 12; original task `tasks/TASK-027-view-layout.md` in that packet.
Prior verdicts/ledgers/R1/R2 evidence: `../task-027-ab1d2499f-1/` and `../task-027-c2bbd8e47-1/`.
The complete cumulative subject and attribution are recorded in [subject.md](subject.md). Unrelated upstream and separately owned TASK-003 changes are not layout findings. HEAD and tracked implementation source remained unchanged.

## Independent results and reconciliation

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | PASS |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| repo-rev | [standards-review.md](standards-review.md) | PASS |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | PASS |
| system-rev | [system-review.md](system-review.md) | PASS |
| interaction domain-rev | [domain-review-interaction.md](domain-review-interaction.md) | PASS |
| fresh focused followup-rev | [followup-review.md](followup-review.md) | RESOLVED |
| fresh ponytail-rev | [findings-validation.md](findings-validation.md) | One retained new finding |

All required reports are complete. [claims-comparison.md](claims-comparison.md) records the material disagreement about reset scope: ordinary shipped replacement hosts pass, while the public direct-Svelte host also consumes deep reactive authored changes. A fresh follow-up established positive scope from REQ-030/036 and the actual reactive parser/load graph, not merely absence of an immutable-only restriction. A separate fresh validator checked this evidence independently and reproduced the split reset/parsed authority with valid component input. Generic PASS results are reconciled to their inspected replacement/handler paths, not counted as votes. No orchestrator-originated finding is included.

## Reconciled acceptance and prior closure

| Obligation / prior finding | Current source and proof | Result |
|---|---|---|
| Bounded optional whole-unit layout, width rejected, view-only catalog publication | Existing strict schema/generator; exact contract selector and prior codegen proof | PASS |
| Full retained callback, FIND-027-1 | All own completed edits overlaid; non-echo two-panel output, untouched siblings and replay tests | CLOSED for original serializer cause |
| Gesture lifetime, FIND-027-2 including revised terminals | Release before commit early return; existing Escape/cancel/reset/unmount; availability pre-effect; held blur/Enter/breakpoint/handler tests and supplied Chromium evidence | CLOSED for diagnosed terminal-entry causes |
| Fixed pitch and measured auto occupancy, FIND-027-3 | One rowsFor/place authority, fixed 40px tracks/native continuous dimensions; pure/DOM and supplied physical geometry evidence | CLOSED |
| Accepted arbitrary-ID state, FIND-027-4 | Own-entry reads and raw immutable dimension writes; constructor/toString/__proto__ output and changed-height tests | CLOSED including writer/reactivity |
| Authored document controls display, callback and local reset across supported hosts | given tracks top-level reference only while parsed/content accept nested Svelte changes; stale kept/draft masks newer authored values | FAIL: FIND-027-5 |
| Immutable input, render-only collisions, no layout-only reload | Existing copied output, geometry and separate layout-stripped load key | PASS for inspected ordinary transitions; reset defect above remains |
| Components fill/scroll/redraw; mobile steady-state ordering and disabled editing | Existing fill/Chart dimensions/narrow styles and recorded tests/browser proof | PASS within supplied scope |
| Optional accessible editing, whole-document callback once, renderer-local MCP state and shared hosts | Complete source traces and prior/R2 focus/capture/callback proof | PASS for repaired paths; new reactive reset path above fails |
| Non-goals: persistence/saved views/add-remove/collapse/size props/dependencies/backend changes | Attributed cumulative diff and all standards/owner coverage | PASS |

Full original matrices remain in both implementation reports. FIND-027-1..4 stay closed with current-source evidence. New FIND-027-5 is a missing dependency in the R2 reset observer, not another lost-sibling serializer, absent cleanup implementation, grid-pitch or arbitrary-key defect. Correct existing owners receive stale state because the reset fails to observe the authored transition.

## Active validated ledger

**FIND-027-5 — CONFIRMED / REGRESSION.** `packages/ui/src/lib/WyrdView.svelte:102-111` resets retained edits/capture on replacement identity only. A valid direct-Svelte host nested update changes parsed authored layout while the same proxy leaves reset dormant; old kept layout wins and is later serialized over the newer host value, and held gestures can survive accepted range/input updates. Positive public-host authority, full writer/caller trace, independent runtime probes and selected minimal correction are in [findings-validation.md](findings-validation.md): observe both replacement reference and stable full parsed authored value at the existing reset effect, preserving unchanged-document handler updates and the separate layout-stripped load key.

## Verification and disposition

[verification.md](verification.md) records nine exact focused selectors, UI/example check/build and native Node smoke green. They omit same-reference reactive authored writes. Original/R1/R2 full-file, physical browser and human acceptance are supplied historical evidence; current source still closes their diagnosed paths. Independent follow-up/validator compiled actual renderer and used installed Svelte runtime plus actual schema to reproduce authored y7/effective y1/stale callback and uncancelled draft; they do not claim fresh mounted-browser proof. No generated inputs changed in R2, so codegen was not repeated. Linux/live infrastructure scope remains unverified rather than inferred failure.

Verdict: **FIX_REQUIRED**. Route [TASK-027-R3-reactive-document-reset.md](TASK-027-R3-reactive-document-reset.md) directly to `$wyrd-implement`. No new product/API/durable decision is needed. Review does not implement, merge, push or deploy. Subsequent review includes original task, all prior remediation rounds/ledgers and complete cumulative candidate.
