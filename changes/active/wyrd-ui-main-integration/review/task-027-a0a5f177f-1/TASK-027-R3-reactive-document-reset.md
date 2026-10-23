---
id: TASK-027-R3
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 12
parent_task: TASK-027
remediates: [FIND-027-5]
---

# Reset local edits when reactive authored document state changes

Execute with `$wyrd-implement` under approved revision 12.
Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Specification `changes/active/wyrd-ui-main-integration/spec.md` revision 12 approved.
Original task `changes/active/wyrd-ui-main-integration/tasks/TASK-027-view-layout.md`.
Original base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; reviewed cumulative candidate `a0a5f177f23960822222b851716460dec02804b3`.
Prior ledgers/verdicts/R1/R2 evidence: `../task-027-ab1d2499f-1/` and `../task-027-c2bbd8e47-1/`.
Current independent diagnosis/verdict: `findings-validation.md`, `followup-review.md` and `verdict.md` beside this task.
Obligations REQ-030 public custom-Svelte renderer, REQ-036 authored-document authority/whole callback, AC-015 and preserved new-authored-input reset.

UI-relative paths below begin at `crates/wyrd/wyrd-server/wyrd-ui`.

## FIND-027-5 diagnosis and required scope

R2 keys local editing reset to `given = $derived(doc)` at `packages/ui/src/lib/WyrdView.svelte:102-111`. This correctly preserves completed edits on unchanged-document handler toggles, but observes only replacement identity. Existing parsed validation and serialized loading content at lines 56/73-89 also consume nested reactive state. The reset owner therefore stops observing accepted authored changes which those sibling authorities already accept.

A supported direct-Svelte host holds an ordinary reactive document with two valid Chart panels and bounded layouts, passes it to WyrdView and records callbacks without echoing. Complete A locally at y1; the host then changes its own `doc.panels[0].layout.y = 7` through the same proxy. Parsed now accepts y7, but given is still the same object and reset does not clear kept. Display remains at y1; completing B serializes stale kept A y1 over the newer authored A y7. A held move/corner similarly survives reactive range or accepted input updates because the same skipped reset owns cancellation; later events can publish the old pointerdown-origin draft.

Independent installed-runtime/schema/compiler probes confirm this state split using valid ordinary component inputs. Existing tests cover replacement references and unchanged handler updates, not nested host-authored writes. Gallery/example raw replacement and static mountView avoid it; they do not define the entire API: REQ-030 expressly publishes WyrdView for user-authored Svelte apps, and this component already processes their nested changes. The correction is not based solely on absence of an immutable-only restriction and does not require adding a new update API.

## Selected smallest complete correction

Extend the existing renderer's document-reset pre-effect to observe both actual replacement reference and a stable full validated authored-document value signature. Reuse parsed and the existing JSON.stringify derived-value comparison pattern, including layout, title, range, panels and inputs; unsuccessful parse can use the existing undefined invalid sentinel. Accepted changed content reaches the existing untracked cancellation/kept clear before rendering. Keep replacement observation so an equal-content new object still resets, and stable value equality so unchanged-document reparse or handler-only rerender does not reset.

Keep the loading key independently layout-stripped. Layout-only authored updates reset editing/retained state without new reads or brushed-range reset; accepted input/range changes retain their existing data-generation behavior. Retain shared commit/cancel/release, availability cleanup, native measurements, own-key records and input immutability. No new parser/validator, general deep-diff/watcher framework, store, host merge, update protocol, dependency or public API is earned.

Resetting on each parsed-result allocation would reopen R2's real handler-toggle bug. Identity-only is the current defect; layout-only observation misses accepted input/range transitions; the loading content key omits layout by design. A callback filter cannot restore correct display or stop a held producer. Correct this one reset owner rather than add downstream guards.

The accepted component input field is unknown at the outer view schema boundary. Do not infer that view parsing promises support for arbitrary cyclic/non-JSON JavaScript values, or expand this change into generic serialization/invalid-input policy. Use valid ordinary component input for the required regression; preserve existing validation behavior and do not introduce new schema restrictions.

## Preserved behavior and write scope

Own only the shared renderer reset derivation/effect and directly affected existing component regression assertions. Preserve all closed FIND-027-1..4, R2 unchanged-document retained edits, own-ID reactive measurement, fixed grid/intrinsic occupancy, render-only collisions, no input mutation, no layout-only loads, existing gesture completion/cancel semantics and all host lifetimes. Preserve separate TASK-003 corrections. No persistence/saved views, component size input, add/remove/collapse, dependency, backend/SDK/security/public-contract change or unrelated cleanup.

This is a distinct R2 observation-boundary regression, not a reopened serializer/gesture cleanup implementation or geometry/key issue; keep old FIND IDs closed and use FIND-027-5 for this diagnosis. Escalate only a genuine material behavior/API/durable decision; none is required for the selected correction.

## Acceptance and focused proof

Use existing component facilities with an actual native Svelte reactive document proxy and valid component inputs; fixture/helper organization remains implementer-owned. Apply scenario-by-scenario Red/Green:

- Complete A without echo, change authored A through the same proxy to y7, flush, and assert authored placement. Complete B and assert its whole callback preserves A y7. A layout-only mutation causes no additional reads, and editing does not mutate host input.
- During held grip move and corner resize, change accepted range or panel input through the same proxy. Reset removes the preview and old move/up are inert with no callback; a fresh gesture still works. Reuse the shared reset/drag owner rather than add event-specific guards.
- Retain handler removal/restoration and unchanged-object rerenders preserving completed edits when authored values have not changed. Genuine new-reference reset, including equal-content replacement, still works. Existing terminal, own-ID and geometry cases remain green.

These assertions directly prove the reactive dependency. No new native-browser infrastructure or live-server journey is required; existing browser evidence still covers repaired native focus/capture terminal behavior. Run exact affected selectors and narrow checks:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Pointer edit cancellation$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- git diff --check
```

Record exact RED/GREEN and final results/limits, including an anchored selector if any new named test is chosen. No schema/catalog change is needed, so do not prescribe codegen, release/Linux builds, repository aggregates or whole user-journey suites for this renderer/test-only correction. Later review reassesses the full cumulative original task and all prior rounds with stable IDs.
