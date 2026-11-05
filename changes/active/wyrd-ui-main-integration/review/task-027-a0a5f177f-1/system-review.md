# TASK-027 system-resilience review — cumulative R2

**Result: PASS.** No material proposed findings. Prior FIND-027-1 through FIND-027-4 are closed on the inspected source and available bounded proof.

## Immutable subject and scope

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; original base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; candidate/observed HEAD `a0a5f177f23960822222b851716460dec02804b3`.

Read the subject, current verification, original TASK-027, approved revision 12 REQ-030/036 and AC-015, both preceding verdicts/validated ledgers, and R1/R2 tasks including their execution evidence. Inspected the cumulative task-owned diff and resulting source, using the latest fix diff only to locate R2 changes. Applied repository agent rules, reference routing, spec-driven development, maintainer and testing guidance, Wyrd client/doctrine boundaries and applicable UI guidance. No CodeGraph directory exists. Unrelated merged Rust/deployment changes and separately reviewed TASK-003 foundation corrections are not attributed to layout.

This is an independent system review, with no current peer reports or intended verdict supplied/read. Only this report is written; source remains unchanged. UI-relative locations below start at `crates/wyrd/wyrd-server/wyrd-ui`.

## Deployed paths and ownership

| Path | Actual owner and consumers | Runtime consequence and recovery boundary |
|---|---|---|
| Direct Svelte renderer | `packages/ui/src/lib/WyrdView.svelte`; schema validates the document, start/settle owns independent panel reads, layout.ts owns pure geometry | All preview, capture, retained layout and measurements live in this browser component. An edit performs no server write. Component replacement drops local state. |
| Gallery | `src/routes/_gallery/+page.svelte:38-51,102` passes an editing handler and echoes the completed document | Host feedback resets retained state against the newly authored document; the layout-stripped content key keeps data loads stable. Each displayed renderer still owns its gesture/measurement lifecycle. |
| Example remote host | `examples/ui-app/src/routes/view/+page.svelte` uses the same renderer and remoteLoader; `packages/ui/src/lib/view.ts:50-65` forwards signal to fetch | Layout updates remain local. BFF/network failures affect panel content, not layout or sibling gestures. A layout change does not initiate a new fetch. |
| Generic DOM mount | `packages/ui/src/lib/view.ts:44-47` forwards options to the renderer and exposes destroy | Callback feedback is optional. The renderer retains all completed edits until a new document/component. Destroy invokes Svelte unmount and its read/gesture/measurement cleanup. |
| MCP App artifact | `packages/ui/src/app/main.ts:15-27`, mcpExecutor in `packages/ui/src/lib/mcp.ts` | A tool result destroys the preceding view and mounts a fresh one with a non-echoing layout callback. Edits last only for that render. MCP read cancellation uses the same per-panel AbortSignal through callServerTool; no durable layout/credential state is introduced. |

No new process, backend service, health check, retry loop, persistent store or deployment prerequisite is added by TASK-027. CSS Grid, existing Svelte measurements and the existing renderer are extended. Existing Chart dimension bindings remain the consumer for plot redraw; components do not acquire size parameters. A browser refresh, App remount or host replacement intentionally restores the supplied document rather than recovering unsaved edits.

## Failure and recovery assessment

| Credible interruption | Source path and affected capability | Recovery/proof assessment |
|---|---|---|
| Escape or pointercancel during move/resize | WyrdView `169-225`: cancel reaches release; stop removes target move/up/cancel and window Escape listeners and releases held capture; pointercancel clears preview | Earlier kept edits remain. Further events from that gesture cannot recreate the preview. A fresh gesture works. Existing exact pointer test covers focused/unfocused move and corner starts, later events and subsequent completion; supplied R1/R2 Chromium proof corroborates native capture/focus. |
| Blur/Enter while a pointer is held, including before first movement | `commit:160-167` releases before either early return or serialization; key and grip blur share this entry | One completed draft produces one callback, while completion without a draft produces none. Later move/up are inert. Both grip/corner producers share stop. Non-echo test includes earlier sibling retention and a fresh gesture; supplied trusted Chromium Tab/Enter sequences match the source. |
| View narrows below 720px or host removes callback during a gesture | `93-116`: editable drives pre-update cancellation before controls are removed; cancel clears only draft | No completion leaks from removal blur; kept edits survive. Returning to editable starts from completed state without an outline or stale producer. Exact component test covers narrow/restore and callback removal/restore with focused/unfocused starts; supplied Chromium proves real narrow/capture transitions. Static mount has no callback update API, so callback removal is appropriately exercised through Svelte rerender. |
| Host replaces document during a gesture | `102-111` keys cancellation/reset to supplied document identity, independently of parse-result recomputation | Producer stops and retained edits reset for the new document. Re-passing the same document while changing handler availability preserves completed edits. Layout-only new documents keep the same content key and loader source, so reads are not aborted/reloaded unnecessarily. |
| Renderer unmount or MCP receives a new result | `112` cleanup calls release; load effect cleanup at `83-89` aborts all controllers; MCP destroys old mount before replacement | Old capture/window listeners are removed. In-flight results cannot settle into the replacement: settle rejects aborted or no-longer-current controllers (`254-260`). Native dimension teardown removes observations. New render derives state from its own supplied document, with no saved-layout recovery obligation. |
| Remote/BFF/MCP read rejects, is delayed or completes after replacement | start/settle/abortAll at `228-260`; remoteLoader passes signal; MCP forwards it | Loading/error state stays within its panel; sibling panel loads/layout remain available. Controller membership and abort status fence stale settlement. Layout edits do not alter content/source and therefore do not amplify outage through extra requests. This preserves the existing read lifecycle rather than introducing service-level fail-closed behavior. |
| Intrinsic content grows after loading, changes with width, or uses an inherited-name panel ID | `125-130,306`: raw immutable measured record replaces an own computed ID entry; pure rowsFor/place uses current numeric occupancy | Border-box observation updates occupancy after actual layout settles. Fixed desktop tracks remain 40px; collision push-down is render-only. Auto content retains intrinsic height, explicit panels scroll internally. Special IDs have the same measurement path and own-entry reads. Tests drive 294→400px and assert changed downstream row; supplied R1 settling/width and R2 special-ID browser measurements cover physical layout beyond style strings. |

Completion/cancellation operate at the component/gesture boundary. There is no justification to take a server process or unrelated UI capability offline. Geometry responds to load outcomes as observed DOM size, so successful, failed or empty panels share the same occupancy mechanism.

## Prior finding closure against current source

| Stable finding | Current evidence | Disposition |
|---|---|---|
| FIND-027-1 — lost prior sibling edits | `commit:163-166` overlays every own entry in done onto parsed panels; layouts reads the same retained state (`118-120`). Non-echo a/b completion test checks the second whole document, untouched auto/colliding siblings, immutability and feedback without reload. | CLOSED. Callback and display share completed authored state; measurement/collision output remains excluded. |
| FIND-027-2 — incomplete gesture termination | cancel reaches release; commit now releases first; editable loss invokes cancel in a pre-effect; reset/unmount retain cleanup. All keyboard Enter/blur, pointerup/cancel, focus-independent Escape, breakpoint and handler-removal entries traced above. | CLOSED, including the R2 extended terminal paths. This preserves the original Escape repair rather than replacing it. |
| FIND-027-3 — intrinsic content stretches row pitch | layout.ts rowsFor/place reserves measured whole rows; renderer uses fixed 40px tracks, align-self:start for auto panels, and current reads the same placement. Native continuous height binding observes late content/width change. | CLOSED. R2 changes measurement storage, not geometry ownership; supplied physical R1 proof remains applicable and R2 changing-height proof supports continuity. |
| FIND-027-4 — prototype-name retained/measurement corruption | Object.hasOwn at retained selection, callback overlay and measurement fallback (`119,129,166`); computed-property raw replacement writer (`306`) establishes actual own keys including __proto__. | CLOSED. Tests cover constructor/toString/__proto__ initial placement, completion/valid whole output, cancellation and changed auto occupancy. The prior unsafe deep-proxy indexed writer is gone. |

## Observer support and proof integrity

Inspected actual installed Svelte `src/internal/client/dom/elements/bindings/size.js`, not just the test stub. bind_element_size observes the element's border box and reads element[type] in its callback, invokes an initial untracked measurement, and returns the observation unsubscribe on effect teardown. ResizeObserverSingleton unobserves the element when its last listener is removed. Both WyrdView offsetHeight/clientWidth and Chart clientWidth/clientHeight use this path; they do not consume entry.contentRect or box-size arrays.

R2 extends the existing jsdom ResizeObserver stub in `vitest-setup.ts` with targets and notify. A target-only entry is sufficient for these actual dimension consumers, because Svelte reads the element getter. The changed-height test supplies that getter and awaits tick after notify; it tests renderer reactivity, not physical browser geometry. Unmount removes targets through Svelte unobserve. The stub's live registry retains a small number of observer instances and disconnect clears targets; this does not establish a production leak or affect the reachable consumers. No additional observer mechanism is needed, and this stub is not proof of unsupported native ResizeObserver APIs.

The separate integration test's isolated jsdom/AppBridge observer stub remains a different environment. Its lack of notify is not silently treated as measurement proof. No test disabling, timeout increase or synthetic host load was introduced for layout remediation.

## Verification evidence and limits

Current orchestrator verification records successful exact layout contract/grid/edit/narrow/pointer selectors plus relevant TASK-003 regressions, UI check/build and documented workspace example check/build at this candidate. These runs are distinct from historical R1/R2 full-file/codegen and browser evidence. I performed source/diff/runtime-library inspection only, avoiding duplicate package builds or shared-workspace verification. HEAD was rechecked as the recorded candidate.

Supplied R1 physical geometry proof covers fixed tracks, short-header y4/top208, tall-chart collision push-down, loading settlement, width changes and mobile behavior. Supplied R2 trusted Chromium proof specifically covers non-echo completion terminals, narrow/capture transitions and arbitrary-ID height changes. The recorded human gallery acceptance remains supplied authority. These are bounded browser observations, not inferred Linux, cross-browser or live degraded-terminal parity.

No material proposed finding remains within TASK-027. No new recovery architecture, retry policy, persistence, host patch, dependency or broad server journey is necessary for these local renderer corrections.

Skipped fresh browser execution, repeated package checks and live infrastructure/platform qualification. Remaining proof limits are supplied macOS/Chromium scope and component-only handler-removal evidence; no source-backed runtime defect is inferred from those limits.
