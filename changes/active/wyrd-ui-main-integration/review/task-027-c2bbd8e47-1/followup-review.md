# TASK-027 focused active-pointer follow-up

Result: **RESOLVED**. Both proposed terminal paths are reachable and share one incomplete renderer-owned gesture lifetime. The direct Escape path is repaired; the common invariant still fails at completion and loss of editability. These are one bounded correction, not independent per-host defects. This is discovery evidence for fresh independent validation, not a final task verdict.

## Subject, scope and evidence

Base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; candidate/confirmed HEAD `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`. Read the task-review skill, subject and comparison, interaction proposals, system lifecycle claims, original TASK-027, REQ-036/AC-015, prior validated FIND-027-2 and R1. Inspected the cumulative renderer diff and complete current renderer, geometry and mount owner; traced gallery, example and MCP consumers and existing editing/cancellation/narrow tests. No `.codegraph` exists.

Applicable repository rules, spec-driven workflow, maintainer guidance and UI authority require source-backed reachability, existing ownership and preserved keyboard/responsive behavior. REQ-036 requires read-only behavior without a handler and below 720px, Escape cancellation and complete callback documents. Original scenario 3 requires one notification per completed change. R1 explicitly preserves Enter/blur, pointerup once, pointercancel, retained earlier edits and no reloads.

All UI source paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`, except the example under repository-root `examples/ui-app`. This pass inspected the existing `escape.mjs` probe but did not execute a new browser session or add a harness. The domain report's actual Chromium sequences corroborate the independently traced state transitions; they remain that reviewer's runtime evidence.

## Complete relevant ownership and entry points

`packages/ui/src/lib/WyrdView.svelte` owns all relevant state and event lifetimes:

- `width`, `narrow`, `editable` at 91–94 derive availability solely from view width and `onlayout`.
- `kept`, `draft`, `release` at 95–100 respectively retain completions, hold the current preview, and stop the current pointer producer.
- The parsed-document pre-effect at 102–109 calls `cancel` then resets `kept`. Unmount cleanup at 110 calls `release`. Neither observes `editable`.
- `layouts` at 112–114 overlays `draft` ahead of `kept` and authored state. Placement, reading order and box styling at 117–142 therefore consume a stranded draft even when editing controls disappear. `layout.ts::place/edit` are pure geometry and have no lifecycle authority.
- `commit` at 150–155 is called by Enter (172–174), grip blur (305), and pointerup through `end` (205–207). It updates retained state and serializes all retained edits, but never calls `release`.
- `cancel` at 159–161 releases the producer then clears only `draft`. Handle Escape (175–176), window Escape (196), new drag (188), and new authored document (106) use it.
- `drag` at 185–214 creates captured `move`, `end`, `escape`, and `stop` closures for either grip or corner. `move` unconditionally recreates `draft` from the pointerdown layout. `stop` clears `release`, removes all four listeners and releases capture; `end` stops before completing/cancelling. There is no `lostpointercapture` listener or editability cancellation path.
- Conditional blocks at 297–307 and 320–322 remove the handles when `editable` becomes false; the surrounding panel remains mounted. Svelte teardown of the removed handle does not call the renderer's `release` closure or clear `draft`.

`view.ts::mountView` at 44–47 forwards props and supplies destroy only; recording hosts need not echo output. MCP `packages/ui/src/app/main.ts:22–27` destroys/remounts on each new tool result and intentionally supplies a no-op layout callback. Gallery `src/routes/_gallery/+page.svelte:37–51,102` and repository-root `examples/ui-app/src/routes/view/+page.svelte:7–11` echo callback documents. These are projections of the same renderer. No sibling gesture store, completion serializer or host cleanup already closes the gap. Existing loader identity/content boundaries at renderer 61–89 remain separate and must remain unchanged.

## Resolution of the two proposals

### INTERACTION-027-1: completion while a pointer remains held

A focused grip receives pointerdown; `preventDefault` preserves existing focus. Pointermove produces a draft. Tab causes native blur and `commit`, or Enter calls the same function while focus remains. Commit retains/publishes the preview and clears `draft`, but captured `move` and `end` remain registered. Further movement recreates a preview from the original pointerdown layout; pointerup publishes a second completion. This is directly reachable without any host mutation. The domain Chromium Tab sequence reports y=1 at blur and y=2 at later release, matching these exact source writes.

Enter has the same cause and completion path even though the supplied physical probe used Tab. Before the first move, Enter/blur hits `commit`'s early return with `draft` absent while `release` already exists. A correction placed only after that guard leaves this terminal variant alive. Stopping a gesture must therefore be independent of whether it already produced a draft.

The echoing hosts can hide the second completion: callback updates `doc`, whose parsed-document pre-effect cancels the remaining producer. That is host-dependent incidental cleanup, and the first early completion still occurred. MountView and MCP demonstrate why feedback cannot be required. Ordinary keyboard blur completion is existing intentional behavior; suppressing all blur during a pointer gesture would choose different interaction semantics unnecessarily. Keep completion and end its producer.

### INTERACTION-027-2: editing becomes unavailable

During a held move or resize, width crosses below 720px, or a direct Svelte host removes `onlayout`. `editable=false` removes the captured control while leaving the panel, `draft`, `release`, and window Escape listener alive. No code calls `cancel` for that transition. Releasing the pointer after the target disappears need not dispatch the target's `pointerup` handler; even native capture loss has no registered cancellation handler. The stranded draft remains first priority in `layouts`, affects narrow reading order/height, and returns with the editing outline when width/handler is restored. The reported real gallery width sequence confirms that concrete outcome.

A focused-control removal can also involve blur. Cleanup must run before that removal can turn the now-unavailable preview into an accidental completion. The selected outcome is cancellation at loss of editability: discard only the current preview, stop listeners/capture, keep completed state, and emit nothing. Observing the renderer's existing `editable` boundary before control removal covers both responsive and handler transitions; host-specific resize hooks do not.

## Relationship to prior FIND-027-2 and system claims

Prior FIND-027-2 diagnosed draft cancellation without ending its pointer producer. R1 added the correct reusable `release` owner and connected Escape, replacement and unmount, so the original Escape reproduction is genuinely closed. These newly inspected paths expose that same producer/preview invariant at other terminal entries: after completion or loss of editability, no producer from the ended interaction may create another draft or callback.

Recommend consolidating INTERACTION-027-1 and INTERACTION-027-2 under the existing FIND-027-2 lifecycle boundary, with a revised affected-path list, rather than assigning separate fixes to blur, Enter, narrow mode and handler removal. Final ID/status belongs to independent validation. Do not claim the direct Escape fix itself is absent or broken.

System-review's Escape, pointercancel, new-document and unmount traces remain supported. Its steady-state narrow/desktop trace and normal completion trace omit held-pointer terminal transitions; those PASS observations do not establish cleanup for these sequences. There is no server/process failure implication and no new persistence or availability requirement.

## Smallest decision-complete correction and closure proof

Keep the existing renderer, `release/stop`, `commit` and `cancel`. Completion must stop any active pointer producer before checking for a draft or publishing; that preserves ordinary keyboard Enter/blur and makes pointerup cleanup harmlessly already completed. Loss of editability must invoke cancellation before controls disappear, preserving `kept` and cancelling only the current preview. This needs no host changes, replacement gesture system, new lifecycle state model, dependency, or public contract. A per-move editable guard alone leaves dangling listeners and stale draft; a host callback guard is too late. A general lost-capture subsystem is unnecessary for the diagnosed availability transition when the owner already knows editing ended.

Use existing focused component event facilities plus bounded existing-browser confirmation:

1. Non-echo host, with an earlier retained sibling edit: hold a grip drag, preview, Tab/blur; separately preview then Enter. Each publishes the preview once. Further move/up produces no new preview or callback. Include completion before first pointermove: no callback then, and later events remain inert. Exercise shared corner behavior where a focused grip provides the completion entry.
2. During held grip move and corner resize, cross below 720px, release after controls disappear, then widen. No completion; prior effective/retained layout returns without editing outline. Repeat availability loss by removing/reinstating `onlayout` through the existing Svelte test host. Include focused and unfocused starts so removal blur cannot publish an unavailable draft.
3. Keep existing keyboard-only Enter and blur completion, keyboard Escape, pointer Escape regardless of focus, pointercancel, fresh pointer completion once, immutable whole-document retained output, no extra loads, fixed-h narrow stacking and reading order. A fresh gesture after each interruption still works.

Extend existing TASK-027 focused tests; do not build a new harness. Run exact affected selectors through `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '<exact existing or added test name>'`, then the existing file and UI check/build lanes for the renderer write set. Actual browser capture/focus and breakpoint transition proof supplements DOM events. Broad journeys, codegen and infrastructure gates are unnecessary for this bounded lifecycle-only correction unless its write set unexpectedly expands.

Skipped dictionary validation and full task re-review as assigned; no source, fixture or harness changed. Risk: this follow-up adds independent source proof, not fresh physical browser execution; independent validation must assess the recorded browser evidence and final fix proof.
