# TASK-027-R4 — Preserve narrow rendering across document replacement

| Field | Value |
|---|---|
| Kind / status | Remediation / ready |
| Original task | `changes/active/wyrd-ui-main-integration/tasks/TASK-027-view-layout.md` |
| Approved specification | `changes/active/wyrd-ui-main-integration/spec.md`, revision 13 |
| Obligations | REQ-030, REQ-036, AC-015 |
| Remediates | FIND-027-6 |
| Cumulative base | `5e57416d562386066c1aadda9dd01783b9cd4eb5` |
| Reviewed candidate | `7579a2f603c023daae0d15b9d5cc3013eec6f00d` |
| Execution | Route directly to `$wyrd-implement`; add `wyrd-ui` for renderer work |

A custom Svelte host must be able to pass another valid document while its view is narrow. Today, replacing a two-panel a/b document with c/d at 500px throws before the new panels render. Correct the existing narrow reading-order join so it excludes obsolete loaded identities while preserving every current panel state and the existing loading lifecycle.

## Diagnosis and evidence

FIND-027-6 is independently confirmed in [findings-validation.md](findings-validation.md), corroborated by the focused [followup-review.md](followup-review.md). UI-relative locations below start at `crates/wyrd/wyrd-server/wyrd-ui/`.

`packages/ui/src/lib/WyrdView.svelte:137-139` sorts effect-owned loaded `panels` against the current parsed document's `placements` Map. `parsed` at line 56 and placements at 129-134 change immediately with the authored document. Only `start` at 236-255 replaces the loaded collection, through the post-render effect at 83-89. During document replacement, the comparator therefore receives previous-generation rows whose IDs are absent from current placements. Its line-138 non-null assertions disappear at runtime; reading `.y` throws.

Fresh independent mounted actual-source probes validate both documents, mount a/b at 500px, replace with c/d, and observe `TypeError: Cannot read properties of undefined (reading 'y')`. The same transition succeeds wide. Neither editing nor a remote dependency is needed. Existing initial narrow tests and green 40/40 integrated tests omit changing panel membership while narrow.

REQ-030 explicitly publishes the renderer for custom Svelte apps, and REQ-036/AC-015 require narrow rendering. Reader add/remove controls remain a non-goal; passing the next complete document is already supported. The failure affects the mounted view and does not imply a server outage. FIND-027-1..5 remain closed: this is a distinct cross-generation presentation join, not a defective authored-reset or gesture cleanup.

## Selected correction and ownership

Use the existing narrow ordering/projection owner and the existing current placements Map. Admit loaded rows to narrow ordering only when their identities have current authored placements. Obsolete rows must not enter the comparator; new rows continue to arrive through the existing `start` effect. Retain all current-ID rows, including loading, denied/failed and panel-validation-error states, regardless of props availability. Preserve stable `(y,x)` sorting and document-order ties.

This join is the first place two independently valid, differently scheduled authorities are combined, so it owns the membership invariant. Map membership already supplies the answer. Keep the wide branch and the existing `start`/`abortAll`/`settle` ownership intact. Do not move IO into a pre-effect, change authored reset, fabricate coordinates for obsolete IDs, retain old geometry, impose cross-document ID rules, force host remounts, add a new generation store, or repair each host separately. Follow-up and validator memory-only variants independently exercised this minimal boundary and preserved the adjacent states; those variants did not change the candidate and do not replace a persisted regression test.

Expected write scope is the shared renderer and its existing component regression tests. Reuse native Svelte proxy/measurement and deferred-loader facilities already present. No helper shape or new fixture module is required by this task.

## Preserved behavior and non-goals

Preserve fixed 12-column/40px geometry, measured intrinsic occupancy, arbitrary accepted IDs, render-only collision displacement, input immutability, all retained sibling callback edits, authored-value/reference reset, gesture terminals and per-render host lifetimes. Same-ID content/range updates still reload normally; layout-only updates/echo still avoid reads and brushed-range resets. Existing controllers still abort old loads and reject stale settlement. Loading and error panels remain visible for current IDs.

No schema/catalog, SDK, server, security, persistence, saved-view, component-size-input, host API, dependency or browser-driver change belongs here. Preserve unrelated TASK-003 and flat-identity corrections. No specification revision is required for the selected correction.

## Acceptance and focused proof

Use existing component tests with real installed Svelte scheduling and simulated narrow width. Follow scenario-by-scenario RED/GREEN, proving the pre-fix exception before changing the shared source.

| Finding / scenario | Required observable proof |
|---|---|
| FIND-027-6: valid replacement at 500px, at least two outgoing rows | Replace a/b with distinct valid c/d; no render exception; only current IDs in resolved `(y,x)` order, full width, explicit h retained, no edit controls |
| Same cause: membership reduction | Replace with a valid smaller panel set; removed IDs disappear without a comparator failure |
| Loading and result lifecycle | Keep an outgoing read pending across replacement; its signal is aborted and late completion cannot resurrect old content; current loading and failed-result panels remain visible |
| Shared projection regression controls | Invalid-to-valid recovery and wide replacement still render; same-ID changed input still refreshes reads; layout-only changed document preserves read count |
| Prior closure | Existing layout editing and pointer cancellation assertions remain green, including reactive authored reset and handler toggles |

Extend the existing narrow scenario rather than introducing a new harness or dependency. Source tracing proves current panel-validation-error IDs come from the same parsed list as placements; preserve that construction and do not filter on successful props. Run the exact affected selectors and narrow checks:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Narrow views$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Pointer edit cancellation$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
```

Record RED/GREEN, exact assertion outcomes and limits, and run `git diff --check`. No geometry change is selected, so no new physical-layout proof is needed. No schema regeneration, broad aggregate or full server journey suite is required for this bounded renderer/test remediation. Later task review reassesses the complete original cumulative range and all prior findings.
