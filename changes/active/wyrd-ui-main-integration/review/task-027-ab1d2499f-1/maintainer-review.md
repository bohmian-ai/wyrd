# TASK-027 maintainer review

Result: **FAIL**

## Immutable subject and authority

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda` (HEAD remained this candidate during inspection). Reviewed the cumulative UI/example diff and the task implementation commits `d33de26e4` and `d90d98a12`, distinguishing the unrelated upstream/dependency/release changes listed in the cumulative diff from this task's owned write set. Those changes are not attributed to TASK-027.

Applied `AGENTS.md`, `architecture/agent-rules.md`, the protocol doctrine/client ownership portions of `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, the reference router, `languages/spec-driven-development.md`, `languages/maintainer-style.md`, `languages/typescript-guide.md`, the local UI skill and Svelte 5 patterns, approved spec revision 12 REQ-036/AC-015, and original `tasks/TASK-027-view-layout.md`. No `.codegraph/` exists; used repository source and ripgrep. No peer conclusions were read.

## Changed-surface coverage

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui` unless otherwise stated.

| Changed surface/symbols | Ownership, callers and evidence inspected | Assessment |
|---|---|---|
| `packages/ui/src/lib/schemas.ts`: `LayoutSchema`, `Layout`, `PanelSchema` | Existing strict `ViewDocumentSchema`, `validateView`, renderer parse, schema generator and invalid-bound tests | Extends the existing contract owner; no second validator. Named typed fields express the contract, including the cross-field bound. |
| `packages/ui/src/lib/catalog.json` | Diff against schema, `packages/ui/scripts/catalog.ts`, published catalog contract test | Generated projection changes the view schema and preserves component size independence. Existing generator reused. |
| `packages/ui/src/lib/layout.ts`: constants, `Placement`, `overlaps`, `place`, `edit`, `rowsHeight` | Complete new module; sole production importer `WyrdView`; collision/placement and keyboard-bound tests | Pure geometry earns its separate module under the task's explicit testability requirement. Caller retains state; a stateful service or new layout dependency is unnecessary. Documentation describes collision and auto-height semantics. |
| `WyrdView.svelte`: `Props.onlayout`, `source`, `loader`, `content`, effects | Complete owning component, pre-task source, `start`, `abortAll`, `settle`, `act`; direct gallery, example and `mountView` consumers | Existing renderer remains the lifecycle and arrangement owner. Load identity and layout identity are separately observable. Existing read/action boundaries remain reused. |
| `WyrdView.svelte`: width/narrow/editable, kept/draft, layouts/placements/ordered, `boxStyle`, `current`, `commit`, `KEYS`, `key`, `drag` | Complete editing functions, input document/retained-state/callback producers and consumers, pointer and keyboard markup | One material state-ownership defect: callback serialization fails to include retained edits of other panels (MNT-027-01). |
| `WyrdView.svelte`: panels/fill, move/corner markup and CSS | Component box CSS; existing Chart and CardHeader fill/layout behavior; mobile stacking test | Local renderer owns grid and overflow, while components retain their own presentation. CSS constants match current geometry constants (12 columns, 40px rows, 12px gaps); 720px breakpoint comes from one TS constant. Explicit keyboard label and focus treatment are present. No new general-purpose layout framework. |
| `packages/ui/src/lib/view.ts`: `MountViewOptions.onlayout` | `mountView` spreading options to existing component, public typed document export and generated package declaration mechanism | Same callback signature and optionality as component props; no parallel mount implementation. |
| `packages/ui/src/app/main.ts`: `ontoolresult` call | Full App/MCP executor setup and `mountView` teardown; no-op layout handler | Existing per-render host lifecycle reused. Its no-op handler makes internal retained state a real production path for MNT-027-01. |
| `src/lib/ui-library.test.ts`: `laidOut`, `panel`, four TASK-027 cases | Complete cases and existing recording helper, TASK-003 view and mount tests | Tests cover input bounds, resolved coordinates, no input mutation, callback shape, cancel/bounds, loader identity on rerender and narrow ordering. The edit case edits only panel `a`; it cannot detect callback loss across different panels. Pointer/browser proof is documented as task evidence. |
| `src/routes/_gallery/+page.svelte`: document state and handler | Complete gallery document/handler usage in light/dark instances | Local document replacement follows existing renderer API. No durable persistence added. |
| Repository `examples/ui-app/src/panels.ts`, `src/routes/view/+page.svelte` | Complete dashboard and view host, `remoteLoader`, component counterpart and `/view/load` route relationship | Examples extend the same contract, with four-row wrapped header and local document replacement. Checked built example client output: the loader expression is compiler-memoized, so no finding is proposed merely for its inline syntax. |

No task-owned Rust, SQL, SDK native binding or dependency change exists. The subject's later merged changes outside these task-owned surfaces require their owning review and do not justify widening this maintainer review into unrelated code.

## Material finding

### MNT-027-01 — Retained edit state and callback document disagree

- **Classification:** INCORRECT.
- **Changed location:** `packages/ui/src/lib/WyrdView.svelte:134-139` (`commit`); state/read side at lines 95-109; real opt-in consumer at `packages/ui/src/app/main.ts:26`.
- **Governing obligation:** REQ-036 and the public `onlayout` documentation require each completed change to report the whole updated document. Maintainer style requires one clear owner for retained state, discoverable operations over that state, and public signatures/documentation describing the same operation. The current split is material because the value crossing the public callback disagrees with the renderer's current document.
- **Producer-to-consumer evidence:** `commit` stores each completed edit in `kept[id]`. `layouts` overlays **all** entries of `kept` when rendering. However, `commit` constructs its callback value from `parsed.data.panels` and replaces only the currently edited panel. A host can observe callbacks without echoing them into `doc`; that is permitted by the callback contract, and the MCP host deliberately passes a no-op handler. Thus, after completing an edit of A followed by B without replacing `doc`, the screen retains A+B, but callback two carries original A+edited B. A later host retaining callback two loses A's completed edit. No asynchronous race or invalid input is required.
- **Sibling writers/callers inspected:** `key` (Enter), grip blur, and pointer-up all call this same `commit`; pointer-cancel/Escape clear only the draft. Both gallery and custom example echo callback documents synchronously, which masks this defect in those hosts. `mountView` supports observing handlers without prop updates, and the MCP App exercises internal retention. There is no separate document serialization helper that already applies `kept`.
- **Concrete maintenance cost/consequence:** The renderer has two effective layout authorities: `parsed + kept` for display and `parsed + current draft` for public output. A host must know undocumented echo timing to obtain the advertised complete document. Correctness of subsequent hosts and changes cannot be inferred from the public type or callback documentation.
- **Smallest correction boundary:** Keep the existing renderer-owned `kept` mechanism and construct callback panel layouts from the same effective completed state used for display, including all previous kept edits and the current completed edit. Preserve document order, unchanged panel data, collision resolution as render-only behavior, input immutability, and no-reload behavior. Do not add a new store, persistence path, callback protocol, or downstream host guards.
- **Focused proof:** Extend the existing editing case (or one adjacent focused case) with two different panels and an observing handler that does not rerender after callback one. Complete A then B; assert callback two contains both edits, matches displayed effective layouts, leaves input untouched, and does not reload. Then echo that whole document and assert both edits remain. This is a local renderer test; no new harness or dependency is needed.

## Existing alternatives and scope

The pre-task `WyrdView` already owned parsed input, load cancellation and panels; extending it was appropriate. `ViewDocumentSchema` and the catalog generator already owned validation and published schemas; the candidate extends them. No existing placement/edit owner was found in package consumers. The pure helper module adds the task-required geometry without duplicating transport, authorization, server behavior or persistence. `kept` itself is justified by the per-render MCP requirement; deleting it would remove approved behavior. MNT-027-01 is corrected at its serializer/owner rather than requiring every host to echo updates or defensively merge documents.

## Verification and limits

The orchestrator's `verification.md` records fresh passing exact commands for all four TASK-027 cases and related UI checks; task evidence records broader tests/build/codegen and human visual approval. These are credible evidence for the covered paths but do not exercise cross-panel retained callbacks. This reviewer performed source/declaration/consumer inspection and did not rerun those commands or browsers. No implementation was changed, and no optional style-only corrections are requested.

Skipped unrelated upstream changes and fresh browser execution; the retained callback inconsistency is the material outstanding risk.
