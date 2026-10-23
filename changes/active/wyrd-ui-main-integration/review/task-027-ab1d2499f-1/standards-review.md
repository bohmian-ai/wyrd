# TASK-027 repository standards review

Result: **PASS**. No material repository-rule findings proposed. This is the
repository-standards audit, not the independent task-acceptance verdict.

## Immutable subject and coverage

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Base: `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`.
Candidate: `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`.
Approved authority: `changes/active/wyrd-ui-main-integration/spec.md`, revision
12; original task `tasks/TASK-027-view-layout.md` in that packet.

Inspected the complete range's changed-file inventory and the complete task
implementation in `d33de26e4` and `d90d98a12`. The cumulative range also contains
merged upstream Rust, dependencies, build, CI, deployment and planning changes.
Per the established subject and orchestrator confirmation, those are provenance
context, not TASK-027 implementation and are not represented as audited Rust
changes in this report. TASK-003's existing components, executor and loaders were
read as dependencies rather than reassessed as new TASK-027 implementation.
There is no `.codegraph/`; navigation used `rg`, source and Git history. HEAD
remained the candidate and tracked source remained unchanged.

UI paths below are relative to
`crates/wyrd/wyrd-server/wyrd-ui` unless otherwise stated.

## Authority coverage

| Changed surface | Applicable authorities | Source/consumer coverage |
|---|---|---|
| `packages/ui/src/lib/schemas.ts` | AGENTS §§2–3, 9, 12, 15–16; agent-rules generated-artifact rule; design Client model; doctrine Public surfaces; TypeScript guide Types/Declarations; UI skill typed boundaries | Full schema owner, `LayoutSchema`, strict `PanelSchema`, `ViewDocumentSchema`; `view.ts::validateView`; renderer and generator |
| `packages/ui/src/lib/catalog.json` | AGENTS §§8, 11–12; agent-rules source-generated artifacts; testing-workflows Contract/generated-output | Complete diff, `scripts/catalog.ts`, generator invocation in `mise.toml` `codegen:check`; component schemas unchanged |
| `packages/ui/src/lib/layout.ts` | AGENTS §§15–16; TypeScript guide Types/API/Style; maintainer-style workflow ownership; UI Svelte patterns strict typing | Entire file, all production callers in `WyrdView.svelte`; component test assertions |
| `packages/ui/src/lib/WyrdView.svelte` | AGENTS §§2, 9, 11, 15–16; UI skill; Svelte5 patterns; developer UX; theming; testing-verification; `brand/DESIGN.md` | Entire renderer, loader/source identity, derived state, event paths, callbacks and CSS; surrounding Chart/CardHeader owners and public host entrypoints |
| `packages/ui/src/lib/view.ts` | TypeScript guide API/Types; maintainer-style typed public contract; UI skill client boundary | Full owner, `MountViewOptions`, `mountView`, browser-only imports; gallery, app and example callers |
| `packages/ui/src/app/main.ts` | UI skill and design client/server boundary; agent-harness surface boundary; AGENTS §2 MCP scope separation | Full artifact bootstrap, existing MCP executor delegation, `mountView` callback and destroy path |
| `src/routes/_gallery/+page.svelte` | UI skill; Svelte5 patterns; developer UX; brand modes; testing-verification | Full changed state and markup; development-only fixture executor and both mode consumers |
| `examples/ui-app/src/panels.ts`, `src/routes/view/+page.svelte` | AGENTS §§11–12 examples; TypeScript typed contract; UI skill; Svelte5 patterns | Full files; shared component-page values; existing remoteLoader/BFF transport owner |
| `src/lib/ui-library.test.ts` | AGENTS §11, §16; agent-rules tests/no gate weakening; testing-workflows; spec-driven-development scenario/command precision; UI testing-verification | Four full added scenarios, fixture helper, existing suite and harness imports, exact commands and verification evidence |
| Active task/evidence packet | AGENTS §14; spec-driven-development; approved revision 12 | Task authority, scope, non-goals, ordered scenarios, acceptance matrix and evidence; cumulative planning changes identified separately |

Reference selection followed `architecture/references/README.md`. Also consulted
`architecture/agent-rules.md`, `languages/maintainer-style.md`,
`languages/spec-driven-development.md`, `languages/typescript-guide.md`,
`languages/testing-workflows.md`, UI skill and its Svelte5, developer-UX,
theming and testing references, and the relevant client/surface sections of
`wyrd-design.md` and `wyrd-doctrine.mdx`. Brand DESIGN and current application
styles/layout/root page supplied the applicable local styling context.

## Rule assessment

| Applicable rule | Result | Source evidence and assessment |
|---|---|---|
| Server owns durable semantics; frontend owns presentation; preserve headless surfaces | PASS | New data is panel geometry. `commit` reports a document; no registry/status/policy/storage behavior or credential handling is introduced. Existing loader/executor paths still supply data. |
| Reuse owning schemas and behavior; do not fork durable contracts | PASS | `PanelSchema.layout` composes `LayoutSchema`; renderer still parses `ViewDocumentSchema`; no parallel document parser or transport is added. `layout.ts` contains pure geometry called by the existing renderer. |
| Strict TypeScript and typed callbacks; unknown at untrusted boundaries | PASS | `doc: unknown`, Zod parsing, inferred `Layout`, `ViewDocument` and callback types; explicit `place`/`edit` return types. No new `any`, untyped host callback or native import. One nonmaterial return-annotation note below. |
| Declarative component props and local reactive state | PASS | `$derived` computes content, placements, reading order and editability; `$state` holds local draft/kept geometry; input document is spread when emitting. No global store or persisted browser state. |
| Generated outputs remain source-driven and drift-checked | PASS | Catalog changes correspond to schema additions and `scripts/catalog.ts` remains generator owner. Fresh `mise exec -- node crates/wyrd/wyrd-server/wyrd-ui/packages/ui/scripts/catalog.ts --check` exited 0. No generated theme or N-API declaration changes. |
| Browser-safe exports and dependency boundaries | PASS | New layout module imports `Layout` as a type; new `onlayout` shape uses existing `ViewDocument`; app continues through existing `mountView`/`mcpExecutor`. No task dependency, feature, manifest or lockfile change. |
| Use theme semantics and preserve flat geometry | PASS | Added controls use `--muted`/`--declare`; active outline and focus are visible; no palette literals, elevation, animation or font source. Grid dimensions are task-defined presentation geometry. CSS columns/row/gap currently match `COLUMNS=12`, `ROW=40`, `GAP=12`. |
| Keyboard accessibility and responsive composition | PASS | Move button has an accessible name, keyboard move/resize, visible focus and instructions; pointer corner shares the keyboard-accessible operation. `narrow` uses measured container width, disables controls, and full-width ordering retains panel height. This does not certify every browser interaction. |
| Tests belong to owning runtime and do not bypass checks | PASS | Added tests are Vitest/Svelte component tests, requiring no foreign runtime emulation or live credentials. No removed/weakened assertion, skipped test or lint allowance in the task diff. |
| Exact commands and narrow verification | PASS | Four exact anchored `-t` commands in task and fresh `verification.md`, each selecting one scenario; TASK-003 regression tests and UI/package checks rerun. Historical app check/build, UI build, codegen and human visual acceptance recorded separately. |
| Rust/SQL/Python native rules and tenancy/audit capabilities | N/A | No TASK-027 Rust, SQL, Python, native binding, auth, admission, audit or durable-state change. No production pool signature exists in these ten task files. Upstream Rust is outside the explicitly established task scope. |
| Documentation and permanent-source language | PASS with nonblocking notes | New public callback and geometry behavior have focused comments; no new public Card noun or compatibility alias. Existing README's example without layout remains valid. See notes below. |

## Nonblocking notes

- `packages/ui/src/lib/layout.ts:47` infers the exported `rowsHeight` return
  instead of declaring `: string`, contrary to the TypeScript guide's explicit
  exported-return convention. Its actual return is unambiguous and does not
  permit an invalid call, so this is not a blocking signature defect.
- Added test names at `src/lib/ui-library.test.ts:376`, `:388`, `:412`, `:459`
  carry task IDs, following the existing scenario naming and the task's exact
  command selectors. `architecture/agent-rules.md` generally prohibits task
  references in permanent code. The names have no behavioral consequence;
  they do not justify a style-only remediation round under the task-review
  threshold.

## Evidence and limits

Read orchestrator `verification.md` and task implementation evidence; did not
read peer discovery conclusions. Fresh independent proof in this role was the
catalog drift check and read-only compilation of the example page with the
installed Svelte compiler. The compiler emits a dependency-free derived value
for `remoteLoader('/view/load')`, so the inline spelling alone does not establish
loader recreation on layout changes.

No fresh browser execution, complete UI/application build, Linux execution or
full codegen aggregate was run by this standards role; historical visual and
build evidence and the orchestrator's fresh exact tests/checks remain distinct.
No implementation or source changes were made. Material findings: **none**.
