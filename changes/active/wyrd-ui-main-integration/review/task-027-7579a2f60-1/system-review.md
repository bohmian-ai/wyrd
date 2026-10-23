# TASK-027 cumulative system resilience review

Overall result: **FAIL**. One proposed layout regression requires independent validation: narrow document replacement can throw before the renderer's panel generation catches up with the new authored document. FIND-027-1..5 otherwise close for their diagnosed causes on the inspected current source.

## Subject and authority

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
- Base: `5e57416d562386066c1aadda9dd01783b9cd4eb5`.
- Candidate and final inspected HEAD: `7579a2f603c023daae0d15b9d5cc3013eec6f00d`.
- Complete cumulative diff inspected with `git diff 5e57416d5...7579a2f60`, distinguishing attributed layout changes and adjacent TASK-003 seams as recorded in `subject.md`. PR #205 upstream changes are excluded.
- Approved specification revision 13: REQ-030's public renderer/host, per-panel loading and document replacement; REQ-036's narrow ordering, local editing and render lifetime; AC-015. Original `tasks/TASK-027-view-layout.md`, prior three verdicts and FIND-027-1..5, and R3 reactive-reset scope were inspected as review inputs.
- Root `AGENTS.md`, `changes/spec-driven-development.md`, `agent_docs/index.md`, `agent_docs/maintainer-style.md`, architecture authority map, operations readiness/dependency ownership, `TESTING.md`, and UI `brand/DESIGN.md` were consulted. There is no CodeGraph index or nested UI/example AGENTS owner. This browser-only layout change does not modify service readiness, database coordination or durable storage.

## Deployed paths and ownership

| Path | Runtime and state owner | Dependency and consequence boundary |
|---|---|---|
| Gallery `src/routes/_gallery/+page.svelte:38-52,102` | Svelte page keeps current document for the visit; `WyrdView` owns layout and reads | Fixture executor; layout edits do not call server writes |
| Example `/view` `examples/ui-app/src/routes/view/+page.svelte:7-11` | Svelte page retains edited document; same renderer | `remoteLoader` POSTs validated panel input and range to example BFF, forwarding AbortSignal |
| Public direct Svelte `WyrdView` (`view.ts:13`, component Props) | Host owns authored `doc`; renderer owns current loaded panels, retained edits and draft | Host may replace a document or author nested reactive values; this is the existing replacement contract exercised by TASK-003 and R3 |
| Public `mountView` `view.ts:44-46` | One mounted component per invocation, with destroy/unmount | Unmount disposes renderer effects, aborts reads and releases active pointer listeners |
| MCP artifact `src/app/main.ts:23-26` | Each tool result destroys old view and mounts new view; no-op layout callback enables local edits | `mcpExecutor` forwards reads and cancellation to existing host tools; credentials remain with host |
| Shared renderer `WyrdView.svelte:56-139,231-263` | Parsed authored document, layout-stripped load content, current loaded panel array and AbortControllers | Panel failures settle into panel state; stale generations cannot write; layout changes use no IO |
| Chart fill `Chart.svelte:20-28`, renderer panel dimensions/styles | Native dimensions redraw chart; `.fill` contains overflow | Resize is local presentation, not query retry/restart |

The existing renderer already owns panel load lifecycle; no second loader, transport, retry owner, persistence surface or process supervisor was added by TASK-027. `layout.ts` is pure caller-owned geometry and does not retain process state. Existing native dimension bindings cover responsive measurement; no layout dependency is introduced. TASK-003's adjacent canonical errors, query cancellation, chart rendering and flat Card references were inspected only at these layout seams.

## Failure and recovery traces

| Credible event | Source trace and observed/required outcome | Assessment |
|---|---|---|
| Pointer Escape or pointercancel | `drag:199-229` installs per-gesture move/up/cancel and window Escape; `cancel:173-175` calls release, clears draft; stop removes all installed listeners and capture | No abandoned producer can revive the cancelled draft. New gesture remains available; sibling completed edits survive |
| Blur or Enter during held drag, even without movement | `commit:163-169` releases before draft early return, then commits once; pointer end also stops before commit | Completion cannot leave the prior producer listening and emit again |
| Width narrows or host removes layout callback mid-gesture | editability derives at 93-94; pre-effect 117-119 cancels before controls disappear | Draft is discarded; retained completions survive; no IO or sibling service impact |
| Same-reference host reactive authored write | parsed observes nested input, authored signature 105 includes full parsed document; existing reset pre-effect 107-114 observes both identity and signature and cancels before clearing kept | R3 closes range/input mid-gesture survival and stale retained layout override; layout-only writes do not change load content |
| Host echoes layout output or passes unchanged document | content excludes layout at 73-75; loader identity derives from `load ?? executor` at 62-69; full authored reset is independent | Echo and layout-only updates do not abort/reload panels. A genuinely new equal-content document resets local edits without a new read |
| Host replaces document with other panel identities at narrow width | placements derive immediately from parsed new document at 129-134; `ordered` sorts old effect-owned panel array at 137-139; load effect at 83-89 updates that array later through start 236-254 | **FAIL: SYS-027-1**, reproduced below. Read-only narrow view can throw on valid replacement before normal recovery |
| Read rejection, upstream outage, invalid panel input | validation failure returns panel problem at 244-245; each returned Promise has rejection-to-problem handler at 248-250; settle updates only its panel at 258-262 | Failed panel remains visible without blocking sibling panel completion. No added automatic retries, shared-server crash, readiness changes or permission bypass |
| Authored content/range changes while reads pending | load effect cleanup invokes abortAll 231-233; start assigns new controllers and panels; settle rejects aborted/non-current controllers at 259 | Delayed old responses cannot overwrite current content; fresh reads occur only at existing content/range/source boundaries |
| Component unmount, navigation, or new MCP result | load-effect cleanup returns abortAll at 88; separate cleanup at 115 invokes release; mount destroy and MCP replacement use unmount | Local layout is discarded as required; no persistent state changes. Read cancellation limits UI work; the existing transport decides cancellation of remote operations |
| Browser refresh or process interruption | No layout write operation exists; input is copied for output; gallery/example state is page-local and MCP state is render-local | Authored layout returns on reload, satisfying non-persistence. No impact on durable evidence or unrelated server capabilities |

## Proposed material finding

### SYS-027-1 — REGRESSION: narrow document replacement mixes panel generations and throws

**Location:** UI `packages/ui/src/lib/WyrdView.svelte:137-139`, with producer split at `56`, `73-89`, `129-134`, `236-254`.

**Violated obligation:** REQ-030's public `WyrdView` document replacement/load lifecycle and REQ-036's narrow view rendering in authored `(y,x)` order. The original renderer already accepted changed documents and aborted stale loads. TASK-003's existing `Dependent loads and safe actions` replacement case changes panel membership. The layout task's add/remove-panel *UI* non-goal does not prohibit a host from passing the next complete document to the public renderer.

**Producer-to-consumer trace:** `doc` is a public unknown input, validated by `parsed`. Layout and placements consume this current validated value synchronously. The separate `panels` array is written only by `start`, which runs in a post-render `$effect` and also owns read generations. In narrow mode `ordered` immediately sorts the prior `panels` array using the new `placements` map. If at least two prior panels lack matching identities in the next document, the comparator dereferences `undefined.y`. The assertion `placements.get(id)!` removes no runtime uncertainty. The normal loading effect has not yet established a same-generation panel array.

**Reachability and sibling consumers:** direct Svelte hosts can replace `doc` through its existing prop; replacement is documented by the component and exercised in the existing TASK-003 test at `ui-library.test.ts:400-407`. R3 explicitly supports deep reactive direct hosts. Gallery/example callbacks normally preserve IDs, and MCP replaces the whole mount, so those normal paths do not demonstrate this particular failure. Public direct hosts remain in scope independently. Desktop ordering returns `panels` without sorting and avoids this dereference. `boxStyle` already tolerates an absent placement, but cannot protect `ordered`, which executes first. `current` consumes placement only behind interactive controls; narrow mode has none. Read result settling writes props on generation-owned panel objects and never creates placements. Thus the root cause is the renderer's mixed-generation presentation, not server response ordering, pointer cleanup, or TASK-003's transport.

**Observable consequence:** at 500 CSS px, replacing valid `[a,b]` document with valid `[c,d]` document throws `TypeError: Cannot read properties of undefined (reading 'y')` during synchronous rendering. It affects the whole mounted view rather than one error panel. There is no component-local error boundary in this renderer. Server APIs and unrelated clients remain available; destroying/remounting the view creates coherent initial state. No claim is made that the host framework recovers automatically after this unhandled render error.

**Smallest correction outcome:** retain the existing `WyrdView` load-generation and layout owners, but make the renderable panel collection and placement lookup use a coherent current document before narrow sorting. Correct the producer/generation synchronization rather than introducing a new transport, layout cache or per-host remount workaround. Preserve existing abort-before-reload and stale-settle rejection, isolated failed panels, no layout-only reload, R3 authored resets, and render-only collision behavior. No new public or durable decision is needed. The exact minimal local synchronization mechanism requires independent source validation; do not fix it by fabricating default coordinates for obsolete panels.

**Focused closure proof:** mount the actual renderer at width 500 with two valid panel IDs and a loader; replace the document with two different valid IDs; flush Svelte; assert no thrown render error and only the new IDs in resolved `(y,x)` order. Hold a first-generation read across replacement and settle it afterwards; verify cancellation/stale rejection and current generation still renders. This extends the existing document-replacement/width test facilities and needs no new harness or dependency. Preserve the existing steady-state narrow and R3 focused tests.

## Reproducible memory-only evidence

Executed at candidate with installed compiler/runtime and jsdom. This is a DOM/reactivity probe, not a real-browser geometry measurement. All module compilation and bundling stayed in memory; no test/source/generated file was written. The first probe printed a data-URL stack; this bounded repeat suppresses that incidental stack output and produces the exact result below.

Working directory: `crates/wyrd/wyrd-server/wyrd-ui`. Command:

```sh
mise exec -- node --input-type=module <<'JS'
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { JSDOM } from 'jsdom';
import { compile } from 'svelte/compiler';
Error.prepareStackTrace = (error) => `${error.name}: ${error.message}`;
process.on('unhandledRejection', error => console.log('unhandled:', error.message));
const require = createRequire(import.meta.url);
const { rolldown } = await import(createRequire(require.resolve('vite')).resolve('rolldown'));
const dom = new JSDOM('<html><body></body></html>', { url: 'http://localhost' });
for (const key of ['window', 'document', 'navigator', 'Element', 'HTMLElement', 'HTMLButtonElement', 'Node', 'Text', 'Comment', 'Event', 'CustomEvent', 'getComputedStyle']) Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} };
Object.defineProperty(HTMLElement.prototype, 'clientWidth', { get: () => 500 });
Object.defineProperty(HTMLElement.prototype, 'offsetHeight', { get: () => 100 });
const entry = `import WyrdView from './packages/ui/src/lib/WyrdView.svelte';
import { mount, flushSync, unmount } from 'svelte';
import { proxy } from 'svelte/internal/client';
const panel = id => ({ id, component: 'Chart', layout: { x: 0, y: 0, w: 6, h: 4 }, input: { type: 'stat', sql: 'SELECT 1 AS n', y: ['n'] } });
const doc = { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: { last: '6h' }, panels: [panel('a'), panel('b')] };
const props = proxy({ doc, load: async () => ({ code: 'test', title: 'offline' }) });
const view = mount(WyrdView, { target: document.body, props }); flushSync();
console.log('before:', [...document.querySelectorAll('[data-panel]')].map(n => n.dataset.panel).join(','));
try { props.doc = { ...doc, panels: [panel('c'), panel('d')] }; flushSync(); console.log('after:', [...document.querySelectorAll('[data-panel]')].map(n => n.dataset.panel).join(',')); }
catch (error) { console.log('replacement:', error.name + ': ' + error.message); }
try { await unmount(view); } catch (error) { console.log('unmount:', error.name + ': ' + error.message); }`;
const bundle = await rolldown({ input: '\0probe', resolve: { conditionNames: ['browser', 'import', 'default'] }, onwarn() {}, plugins: [{ name: 'memory-probe', resolveId(id) { if (id === '\0probe') return id; if (id === './packages/ui/src/lib/WyrdView.svelte') return process.cwd() + '/packages/ui/src/lib/WyrdView.svelte'; }, async load(id) { if (id === '\0probe') return entry; if (id.endsWith('.svelte')) return compile(await readFile(id, 'utf8'), { filename: id, generate: 'client' }).js.code; } }] });
const out = await bundle.generate({ format: 'esm' }); await bundle.close();
await import('data:text/javascript;base64,' + Buffer.from(out.output[0].code).toString('base64'));
JS
```

Observed output, exit status 0 because the probe deliberately catches the error:

```text
before: a,b
replacement: TypeError: Cannot read properties of undefined (reading 'y')
```

## Prior finding closure against current source

| Stable prior ID | Current-source closure evidence | Result for diagnosed cause |
|---|---|---|
| FIND-027-1 | commit overlays the entire own-entry `kept` map into all authored panels at 163-169; non-echo two-panel test 761-784 verifies retained siblings | CLOSED |
| FIND-027-2 | release runs before commit early return; cancel removes listener producer; window Escape, pointercancel, editable-loss pre-effect and unmount cleanup all converge on release; cancellation test 850-1010 covers completion and reactive writes | CLOSED |
| FIND-027-3 | `rowsFor` computes intrinsic occupancy; `place` consumes measured heights; panel offsetHeight binding continuously writes; fixed 40px CSS grid tracks and rowsHeight use one pitch | CLOSED |
| FIND-027-4 | retained/measurement reads use Object.hasOwn, raw records use immutable computed-key writes; tests cover constructor/toString/__proto__ including geometry | CLOSED |
| FIND-027-5 | full parsed authored signature plus original-reference dependency at 102-114 cancel and drop kept on valid nested updates; R3 tests at 817-847 and 983-1010 cover authored layout, range/input and later fresh gesture | CLOSED |

SYS-027-1 is separate from prior reset dependency/gesture-lifetime causes: the reset does run, but narrow ordering mixes independently scheduled panel and geometry generations.

## Verification limits

Supplied candidate check (zero errors), test 40/40, build, and real-Postgres integration 2/2 are credible reported evidence, not reruns by this reviewer. Existing steady-state narrow tests do not replace panel identities at narrow width. Prior packets supply historical browser geometry/capture and human acceptance; none is claimed as fresh browser proof here. This review performed source tracing and the actual memory-only compiler/runtime reproduction above. No dependency outage injection, server load, browser visual run, broad aggregate or source change was performed. The new finding is limited to the proven browser-component generation boundary and does not allege shared service failure.
