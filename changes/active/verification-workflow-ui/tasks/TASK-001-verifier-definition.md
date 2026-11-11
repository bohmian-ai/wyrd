---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 2
requirements: [REQ-004, REQ-014, AC-002, AC-009]
depends_on: []
---

# Shared graph and exact Verifier definition

## Outcome

Opening an exact Verifier Card shows its declared workflow using the approved
styled Svelte Flow graph, Graph/YAML, description, version and where-it-runs.
This is a reusable graph and a working conventional definition page together.
The graph has no execution colors or authoring gestures.

Inputs are the normal exact Card read and its `VerifierSpec`; output is typed
graph/definition props consumed by the exact Card page, gallery, custom consumer
and built MCP artifact. [TASK-003](TASK-003-verify-discovery.md) connects Results
in Verify to the real filtered list; do not ship a broken link before it exists.

## Existing owner and implementation

Extend `packages/ui/src/lib` component/catalog/loader/view owners. Reuse
`loadCardHeader`, CardHeader/CardFacts and the exact
`/t/[tenantKey]/cards/detail` route rather than adding a Cards index or another
Card lookup. Extend the current `catalog.json`, `loaders.ts`, shared renderer
and exports in the same way as existing Card components. Credentials and reads
stay in existing host executors. No new backend read is required for topology.

Use installed `@xyflow/svelte`, custom Wyrd node/edge styling and the approved
automatic layered composition from [graph design](../design/notes.md#svelte-flow-graph-and-contextual-inspector)
and [routing rationale](../design/mock-brief.md#graph-presentation-requirements).
Dagre is the demonstrated starting layout choice in the mock; use the approved
preview's dependency through normal manifests/lockfile if needed, not another
graph framework. Private routing/layout internals remain implementation-owned.
Do not import executable mock code as a production transport or data source.

Render inputs/dependencies, authored steps, attached and whole-Verifier
requirements with readable labels/arrows and stage headings. Keep parallel
branches parallel, simple graphs compact, and fan-in/out readable. Disable
drag/reconnect/delete. Node selection, focus and pan/zoom work on narrow screens;
topology dimensions drive layout, not future progress. Definition version
navigation is read-only and exact.

## Proof and dependencies

Use the verification owners and commands below.
Use UI unit/interaction owners for ordinary, session-shaped, drift-shaped,
parallel and repeated declared topology, exact identities, no runtime coloring,
no editing, and keyboard/zoom behavior. Real Card declaration → authorized
shared loader → exact conventional Card route must render the same definition;
foreign/denied/missing/error states stay truthful. Exercise the same component
through Node/MCP/custom and built-artifact consumers at the existing host test
owner. Compare graph regions with the approved mock in both themes/viewports.

Activation requires specification approval and the applicable scope transfer
recorded in [spec.md](../spec.md#dependencies-authority-and-scope-transfer).
Existing exact Card owners are the input; no dependency on GitHub connection, admission,
wait or delivery is introduced. Changed Card/catalog contracts regenerate
normally. No editor, new Verifier type or general Cards redesign is included.

## Execution and verification

This task is proposed against draft spec revision 2. Activation requires
explicit spec approval, the applicable scope transfer, and only this task's
named contract/authority and predecessor outputs. Unrelated backend delivery
or authority amendments do not block it. Record the integrated source revisions
and actual dependency journeys it consumes; upstream task status is not proof.

Read the repo-local `wyrd-ui` skill before UI edits. Reuse the shared component,
loader, catalog, Node/MCP executor and host action owners. Test shared components
in `src/lib/ui-library.test.ts` and pages in `src/lib/ui-pages.test.ts`;
authorized host/built-artifact proof belongs to `ui-library.integration.test.ts`
and production BFF/Chromium/public-login proof to `ui-pages.integration.test.ts`.
Record actual focused selectors for added tests before their first run; follow
the existing TDD discipline without inventing test names in this task.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts src/lib/ui-pages.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts src/lib/ui-pages.integration.test.ts
```

Narrow test files with actual relevant selectors. The integration command needs
the existing `ts:build`, `ts:build:testing`, `build:server` and UI-build
prerequisites; inspect `mise.toml` and the managed Postgres wrapper before
running it. Missing prerequisites are not passing or skipped proof.

Extend normal `mise run dev:full` public setup where needed to demonstrate this
task's real inputs. Independently compare each introduced component/page with
the approved mock on desktop/mobile, light/dark, keyboard/focus/touch, long
content and error states. Record reference regions, screenshots and individual
human acceptance. Fixtures and automated checks alone do not prove fidelity.

Regenerate changed catalog/contracts through their owning generators and run
`codegen:check` when applicable. Changed Rust/SDK owners require their narrow
format/lint/type and real boundary journeys; preserve their existing tenancy,
Python stub and N-API checks. Do not add a runner or substitute fixtures for
an unavailable backend output. Integrated change review joins the delivered
flows and runs the required full shipped-surface journey lanes.
