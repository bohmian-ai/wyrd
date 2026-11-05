# TASK-027 independent behavior review

Result: **FAIL**. Proposed active finding: **B027-1**, a narrow-view document-replacement crash. This is a discovery proposal requiring independent validation, not an assigned stable ledger ID.

## Immutable subject and coverage

Repository and subject are recorded in `subject.md`. Reviewed full base `5e57416d562386066c1aadda9dd01783b9cd4eb5` through candidate `7579a2f603c023daae0d15b9d5cc3013eec6f00d`, approved specification revision 13, original TASK-027 and prior FIND-027-1..5/remediation packets. HEAD remained `7579a2f603c023daae0d15b9d5cc3013eec6f00d`. Read cumulative source/diff rather than R3 alone. No source or test files were changed.

Applied root AGENTS.md, changes/spec-driven-development.md, agent_docs/index.md, maintainer-style.md, agent-surfaces.md, TESTING.md, architecture/README.md and wyrd-design.md client boundary, UI skill and brand/DESIGN.md. Shared navigation establishes no `.codegraph/` or applicable nested AGENTS.md. TASK-003 changes in the range were inspected at renderer/schema/chart/error/host seams, not treated as independent layout findings. PR #205 flat identity is current authority and unrelated upstream content is excluded. The orchestrator's range-diff evidence maps reviewed implementation commits unchanged; packet link changes do not change behavior.

UI-relative paths below start at `crates/wyrd/wyrd-server/wyrd-ui/`.

Expanded owner/caller map: `schemas.ts` validates document shape/layout; generated catalog projects it; `view.ts` validates and forwards mount options; WyrdView owns parsed authored input, layout overrides, pointer lifetime, native size measurements, panel load generation and rendering; `layout.ts` owns pure placement/edit arithmetic; Chart owns plot measurement and resize; gallery/example echo layouts; MCP App deliberately does not echo and destroys each prior render. Existing tests exercise schema, grid, editing, cancellation, arbitrary IDs, reactive input, dependent loads and narrow layout. Every production caller reaches the one shared renderer.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Strict whole-unit x/y/w/h bounds; reject partial/fractional/out-of-range and width naming field | `schemas.ts:181-200`, existing inputProblem and validateView; layout present in generated catalog | Layout document contract includes bounds and component-entry size exclusion | PASS |
| 12 columns/40px rows; later collisions push down; mixed/unplaced full-width content-height occupancy; no render mutation | complete `layout.ts::place/rowsFor/rowsHeight`; renderer placements/boxStyle, fixed auto rows, native offsetHeight | Grid placement pure/DOM cases; prior physical geometry evidence | PASS |
| Existing no-layout documents remain ordered and intrinsic-height | place full-width auto boxes; align-self:start; renderer load/content keys | Grid placement plain-document and measured-content cases | PASS |
| Components fill resized box; overflow stays inside; chart redraw; no component size input | renderer `.fill` and fixed panel box; Chart native width/height measurement | historical gallery/artifact chart geometry; catalog component schemas unchanged for sizing | PASS |
| Optional handler, bounded snapped keyboard/pointer move and corner resize, Escape cancel | editable/controls; current/edit; key/drag/cancel; shared release | Layout editing and Pointer edit cancellation source assertions; prior native capture/focus evidence | PASS |
| One whole updated document per completion, retaining prior completed edits and untouched authored fields; input immutable | `commit:163-170` overlays own kept entries, never effective collision boxes | non-echo sibling output, arbitrary-ID, feedback and immutability assertions | PASS |
| Layout editing and host layout-only feedback do not reload or reset brushed range | stable source/loader; layout-stripped content; separate authored reset | recording-call counts and reactive layout-only test | PASS |
| Narrow full-width `(y,x)` order, explicit h retained, no editing | narrow/editable; ordered; boxStyle; stacked auto rows | Narrow views case verifies initial render only; B027-1 breaks supported replacement while narrow | **FAIL** |
| New authored input wins over local edits; held producers cancelled; unchanged reparse/toggle retains edits | `given` + full parsed-value `authored` at 103-114; availability pre-effect; shared cancel/release | R3 native proxy tests for layout write, range/input write, stale events, replacement identity and handler toggle | PASS for prior FIND-027-5; replacement-generation seam is B027-1 |
| Gallery/example/MCP show placed editable layouts; themes, desktop/mobile; per-render MCP lifetime | gallery state+handler; example page state+handler; AppBridge destroys old mount and supplies handler | original browser and human acceptance; supplied current integrated check/test/build | PASS for delivered hosts; custom Svelte replacement has B027-1 |
| No persistence, add/remove/duplicate controls, collapsible rows, per-viewer memory, layout dependency or shipped-page layout takeover | cumulative diff/manifest and every host | static inspection | PASS |
| Preserve TASK-003 load/error/action isolation and cancellation | renderer start/settle/abort/act; stable loader source; chart behavior seams | supplied integrated 40/40 and real-server 2/2; dependent loads assertions | PASS except new narrow replacement render crash |

The UI editing non-goal excludes adding/removing panels through reader controls. It does not prohibit a host loading another valid document, already supported by WyrdView and explicitly exercised by TASK-003 replacement tests.

## Prior finding closure

| ID | Current source closure |
|---|---|
| FIND-027-1 | commit overlays the complete `done` retained-layout record into parsed panels, so second non-echo completion retains the first. Collision positions and auto heights never enter untouched callbacks. |
| FIND-027-2 | commit calls release before its early return, cancel calls release, stop removes target/window listeners and capture; editable loss cancels in a pre-effect; new document/unmount terminate the producer. Tests retain terminal paths and fresh gestures. |
| FIND-027-3 | fixed CSS auto rows remain 40px; intrinsic panel offsetHeight reaches rowsFor/place; auto aligns to start without fixed height; current uses measured occupancy with edit height bound. |
| FIND-027-4 | kept reads/callback overlay/measurement reads use Object.hasOwn; immutable computed-key raw height replacement supports constructor/toString/__proto__. Positive and changed measured height assertions are present. |
| FIND-027-5 | full validated authored JSON plus replacement identity reaches the same cancellation/kept-clear owner. Loading key stays independently layout stripped. Native proxy test now proves same-reference layout writes displace local edits and subsequent output preserves authored y7; held range/input changes end both gesture paths. |

B027-1 is not a reopened FIND-027-5: no retained edit or active gesture is required. Its source is the newly introduced narrow reading-order consumer mixing document generations.

## B027-1 — REGRESSION: narrow replacement sorts stale panels against new-document placements

**Obligation:** REQ-030 supported custom Svelte host/new-document replacement; REQ-036 and AC-015 narrow stacking. **Location:** `packages/ui/src/lib/WyrdView.svelte:129-139`, especially comparator at 138; load-generation writer at 83-89 and 236-254.

**Required reachable path:** A custom Svelte host displays a valid document containing panels a,b at width 500px. It replaces `doc` with another valid document containing c,d, or removes one prior ID in a new document. Both documents are within schema bounds and the host need not opt into editing. `parsed` and `placements` immediately describe the new document. `panels` still describes the previous document until the post-render `$effect` invokes start. The narrow `ordered` comparator dereferences `placements.get(oldId)!` as p/q and reads `.y`; a missing ID produces TypeError before new loads/render can settle. At least two prior panels cause comparator evaluation. Wide boxStyle already tolerates a missing placement; narrow sort introduces this unguarded dereference. The effect-order mismatch is attributable to TASK-027's new ordering, not PR #205 or independent TASK-003 changes.

**Evidence:** Complete producer/consumer functions were read. Only start writes the panel array; settle writes existing panel props. Schema permits different valid ID sets and uniqueness within each document. All direct WyrdView hosts share this implementation. `mountView` is static and MCP recreates mounts, so those hosts avoid this transition; custom Svelte apps do not. Installed Svelte scheduler `reactivity/batch.js:493-527` queues user effects while processing dirty render effects, then flushes queued effects at 443-444. Thus start cannot run before the failing render consumer merely because its source declaration appears earlier.

A read-only installed-runtime scheduling probe reproduced the state split and exception. This models the exact source dependency shape using actual installed Svelte primitives; it is **not** a mounted WyrdView/browser reproduction. Exact command, from UI directory, created no files:

```sh
mise exec -- node --input-type=module <<'NODE'
import { effect_root, user_effect, render_effect } from './node_modules/svelte/src/internal/client/reactivity/effects.js';
import { source, set } from './node_modules/svelte/src/internal/client/reactivity/sources.js';
import { derived } from './node_modules/svelte/src/internal/client/reactivity/deriveds.js';
import { get } from './node_modules/svelte/src/internal/client/runtime.js';
import { flushSync } from './node_modules/svelte/src/internal/client/reactivity/batch.js';
const doc = source(['a','b']);
const panels = source([]);
let log=[];
const stop=effect_root(()=>{
 const placements=derived(()=>new Map(get(doc).map((id,i)=>[id,{x:0,y:i}])));
 user_effect(()=>{const ids=get(doc); log.push('load '+ids);set(panels,ids)});
 render_effect(()=>{const sorted=get(panels).toSorted((a,b)=>{const [p,q]=[get(placements).get(a),get(placements).get(b)]; return p.y-q.y||p.x-q.x}); log.push('render '+sorted)});
});
flushSync();
try {set(doc,['c','d']);flushSync();console.log('result',log)}catch(e){console.log('error',e.message,'events',log)}
stop();
NODE
```

Observed `error Cannot read properties of undefined (reading 'y') events [ 'render ', 'load a,b', 'render a,b' ]`. No `load c,d` ran before the exception. Initial probe mistakenly imported flush_sync instead of installed flushSync, failed only at module import, and was corrected; it is not an application test failure.

**Smallest testable correction:** Keep the current renderer, loader generation and native scheduling. Narrow reading-order derivation must join only panels present in the current validated placement authority before dereferencing their coordinates, using existing Map membership, so obsolete previous-document panels cannot enter its comparator. This local join belongs to ordered because it combines asynchronous loaded panels with synchronous authored placement. Do not invent arbitrary coordinates for removed IDs, change document IDs, add a second loader, move network work into pre-render lifecycle, or weaken generation cancellation. Existing wide behavior, loading/error panel state, valid narrow sorting/heights, authored reset and layout-only no-load behavior must remain. The new generation's existing start effect still creates its loading panels and settle fills them.

**Focused proof:** Extend existing narrow component test using native width 500 and actual WyrdView rerender/native reactive host update. Replace a two-panel valid document with distinct IDs, and with a valid document removing one old ID; assert no render exception, only new IDs, correct `(y,x)` stacking/heights, read-only controls, new loads and old aborted loads/late-result rejection. Include invalid-document then valid replacement if the correction touches invalid handling; preserve same-ID layout-only no-reload and wide replacement regression cases. Existing facilities suffice; no dependency or browser harness is needed. Independent validation should confirm the actual mounted transition before retaining the proposal.

## Reuse and complexity audit

The new pure layout module isolates one cohesive geometry responsibility and has no retained lifecycle. Existing WyrdView owns all overrides and gesture cleanup; no parallel store/renderer/parser/transport was introduced. Native size bindings, pointer capture/listeners, Svelte runes, Map and own-entry records implement the required mechanisms without new dependencies. JSON value signatures reuse the existing loading-signature pattern for a distinct reset boundary; unifying them would reintroduce layout-only reads. The four-line .svelte.ts fixture exists to create a real native proxy unavailable in plain test TypeScript. No unearned machinery requiring deletion was found.

Verification was supplied at candidate: package/app check zero errors, test 40/40, build, real-Postgres integration 2/2. Historical physical geometry/focus/capture and human acceptance are referenced, not freshly repeated. Existing narrow test never changes its document; green checks do not cover B027-1.

Skipped source changes, broad reruns and fresh mounted/browser execution. Risk: proposed narrow replacement failure needs independent validation and blocks acceptance if confirmed.
