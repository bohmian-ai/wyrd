# TASK-027 focused reset follow-up

**RESOLVED.** I027-R3-01 describes a reachable regression in the public direct-Svelte renderer. Retain it for independent validation, bounded to authored reactive document updates; no new host update API, persistence or merge protocol is required.

## Subject and inspection

Immutable candidate/observed HEAD: `a0a5f177f23960822222b851716460dec02804b3`; cumulative base: `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`. This follow-up addresses only the reset disagreement in claims-comparison.md. It does not repeat the full acceptance review or dictionary findings.

Read the assigned skill, subject/comparison, I027-R3-01, behavior/system reset claims, approved revision 12 REQ-030/036 and AC-015, original TASK-027, both prior validated ledgers and R1/R2 correction/evidence relevant to reset and retained/gesture ownership. Applied AGENTS.md, agent rules, spec-driven authority, maintainer guidance and repository Svelte patterns. No `.codegraph/` exists.

Inspected cumulative renderer/layout/view changes and complete current `packages/ui/src/lib/WyrdView.svelte`, `layout.ts`, `view.ts`; schema document/layout owners; relevant range/validation/load entry bodies; gallery and example page hosts; AppBridge bootstrap; public exports; and editing/pointer tests. Paths here are relative to `crates/wyrd/wyrd-server/wyrd-ui`, except the repository-root `examples/ui-app` page. Only this report is written.

## Authority resolves the required path

The requirement is supported positively, rather than by the absence of an immutable-only restriction:

- REQ-030 (`spec.md:488-515`) expressly requires custom-app portability, identifies user-authored Svelte applications as an in-scope host, and publishes `wyrd-ai-ui/view` with the `WyrdView` renderer. `packages/ui/src/lib/view.ts:13` actually exports that component. Static `mountView` is the additional non-Svelte projection, not the complete public renderer contract.
- REQ-036 (`spec.md:934-965`) puts layout in the authored document, requires valid layouts to render, says hosts decide what to keep, and says reloading a document shows its written layout. TASK-027 scenarios 2–3 require the supplied document to control placement and each completion to return its whole updated value. R1 preserves “new authored input” resetting local state; R2 preserves new-document cleanup and only adds availability cancellation that retains completed edits.
- This is a Svelte 5 component with a reactive `doc` prop, not a one-time constructor argument. Candidate `WyrdView.svelte:56,73-89` already observes validated document state and serialized range/panel inputs. A host-owned `$state` document updated through its own reference is normal Svelte input: installed Svelte 5.57.2 `types/index.d.ts:3429-3455` explicitly distinguishes `$state.raw` reassignment from deeply reactive `$state` proxies. The repository Svelte guide uses mutable local `$state` and derived props; its prohibition on mutating props applies to the child mutating another owner's state, not the host editing its own document.

Thus the scope conclusion is an inference from the promised direct-Svelte host, authored-document authority, existing reactive input behavior and preserved reset outcome together. REQ-036 does not literally prescribe a deep-watch algorithm, nor does “reloading” alone prove every nested mutation must count. The decisive regression is that an accepted host change already updates the renderer's parsed/load authority, yet retained editor state continues to override that newer authored authority. Requiring the host to replace its entire object to repair this split would introduce a host protocol where the existing direct component previously consumed those reactive changes.

The behavior/system reports correctly describe shipped hosts: gallery and example use `$state.raw` replacement, AppBridge remounts, and `mountView` exposes only destroy. Those source-backed facts explain why those callers avoid the defect. They do not narrow the explicitly published direct-Svelte component to those callers.

## Producer-to-sink evidence

At `WyrdView.svelte:102-111`, `given` derives only the top-level `doc` reference. Its pre-effect is the only post-initialization writer clearing `kept`, and calls `cancel()` to end draft/capture ownership. Nested reactive writes do not change `given`.

For a valid two-panel document, complete A locally at y1 without echoing the callback, then have the host write `doc.panels[0].layout.y = 7`. The schema derivation accepts y7. `layouts:118-120` still selects own `kept.a` y1 first; `placements`, `boxStyle` and `current` consequently use y1. Completing B runs `commit:160-166`, spreading stale kept A onto current parsed panels and reporting A y1 rather than authored y7. No callback consumer creates the corruption.

Likewise a host reactive range/input update changes `content`, and the existing load generation can restart, while the skipped reset leaves draft and `release` alive. `drag:196-225` retains its pointerdown `from`; its move closure can still write that old preview and pointerup commits it. `cancel:169-172` would end this producer, but neither the independent availability effect nor the loader effect owns document reset. Both grip and corner share this lifecycle. This claim does not require a server outage, unsupported prop mutation by the child or a new public update method.

## Independent read-only checks

Ran through the repository toolchain in the UI workspace:

`mise exec -- node --input-type=module` compiled the actual candidate `WyrdView.svelte` with installed `svelte/compiler`, zero warnings. Emission confirms `parsed = $.derived(() => ViewDocumentSchema.safeParse($$props.doc))`, `given = $.derived(() => $$props.doc)`, and the reset pre-effect reads only `given` before untracked cancellation/clear.

`mise exec -- node --conditions=browser --input-type=module` used installed Svelte proxy/state/derived/effect/flush machinery and candidate `packages/ui/dist/schemas.js`, executing the relevant selection/reset/serialization expressions. Assertions passed with:

```json
{"state":{"authored":7,"effective":1,"resetCount":1,"draft":{"id":"b","layout":{"x":8,"y":2,"w":4,"h":6}},"releases":0},"callbackA":{"x":0,"y":1,"w":8,"h":6}}
```

The assertions establish the unchanged-reference reset mismatch, stale whole-document callback, and retained draft after changing reactive range from 6h to 12h. This is an installed-runtime state probe plus complete source trace, not a mounted DOM or trusted-pointer browser reproduction. Initial probe setup used a non-exported `source` helper; corrected to installed `state`. An initial compiler invocation from repository root could not resolve Svelte; rerunning in the UI workspace succeeded. No application test failed, source was generated, or gate was weakened.

## Selected minimum correction and closure boundary

Keep reset in the existing renderer. Retain reference observation so an actually new document object still resets even when equal in content. Add a stable full validated authored-document value key to that same reset effect, using the existing `JSON.stringify` derived-value pattern already used for `content`; include layouts as well as range, panels and their inputs. Use the current schema result, not another validator/parser or raw-object traversal protocol. A validation failure can have the existing invalid sentinel rather than a serialized error object. Any accepted authored value change then reaches the existing untracked `cancel(); kept = {}` before rendering.

Stable value equality is necessary: simply resetting on each parsed-result allocation reopens R2's same-document handler-toggle failure. Reference equality alone is insufficient as demonstrated. The load key remains separately layout-stripped; neither resets nor the new key should become a reason to reload layout-only changes. Retain native measurements, own-key overrides, input immutability, retained edits on unchanged-document handler toggles, and release-before-commit/availability cleanup. No host edits, new store, dependency, harness or public API are needed. This is one reset-owner correction, not separate display/callback/gesture guards.

Focused proof belongs in existing editing and pointer component tests with an actual native reactive proxy: complete A, mutate authored A to y7 through the same proxy, assert authored placement and B's subsequent whole-document callback preserve y7; update range or accepted input while holding a draft, then verify old move/up are inert and emit nothing. Cover both shared pointer producers. Retain unchanged-document handler removal/restoration, unchanged-object rerender, genuine new-reference reset, input immutability, and unchanged load count on layout-only changes. Run the exact existing TASK-027 Layout editing and Pointer edit cancellation selectors through mise, followed by UI check/build. No new broad journey or platform proof is justified for this bounded correction.

Skipped duplicate package suites and fresh DOM/browser capture execution. Risk: runtime evidence is a state-propagation probe; the selected correction still needs focused component proof at implementation time.
