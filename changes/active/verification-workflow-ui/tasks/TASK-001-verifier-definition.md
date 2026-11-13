---
id: TASK-001
kind: implementation
status: complete
spec: SPEC-verification-workflow-ui
spec_revision: 2
requirements: [REQ-004, REQ-014, AC-002, AC-009]
depends_on: []
---

# Shared graph and exact Verifier definition

Marked complete at the user's request on 2026-11-11 after the
[passing remediation review](../review/TASK-001-2026-11-11-02/verdict.md)
of candidate `9c38fdfc3`. `FIND-TASK-001-1` is closed. Human visual
acceptance and a green full integration aggregate remain uncertified;
completion does not change the verification evidence recorded below.

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

The specification is approved and its [scope transfer](../spec.md#dependencies-authority-and-scope-transfer)
is synchronized. Existing exact Card owners are the input; no dependency on GitHub connection, admission,
wait or delivery is introduced. Changed Card/catalog contracts regenerate
normally. No editor, new Verifier type or general Cards redesign is included.

## Execution and verification

This task is against approved spec revision 2, whose scope transfer is
synchronized. Activation requires only this task's named contract/authority
and predecessor outputs. Unrelated backend delivery
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

## Obligations

| ID | Promise (source) | Required cases | Planned proof |
|---|---|---|---|
| REQ-004.topology | The graph renders the exact declared inputs, dependency edges, steps, attached requirements and whole-Verifier requirements with stage headings, arrowheads and dependency labels; parallel branches stay parallel and simple graphs stay compact (spec REQ-004; task "Render inputs/dependencies…") | ordinary, session-shaped, drift-shaped, parallel, repeated declared topology; fan-in/out | `ui-library.test.ts` projection, layout and rendered-graph assertions |
| REQ-004.read-only | A definition graph shows no execution colour or state and offers no authoring gesture (spec REQ-004; task Outcome) | no runtime colouring; drag, reconnect and delete disabled | `ui-library.test.ts` rendered-graph assertions; source inspection of the Svelte Flow options |
| REQ-004.interaction | Node selection, focus, keyboard movement and pan/zoom work, including narrow screens; layout follows topology, never selection (spec REQ-004; task "Node selection, focus and pan/zoom…") | click and keyboard selection, off-screen reveal, zoom in/out/fit/100%, narrow viewport | `ui-library.test.ts` interaction assertions; real-browser check in `ui-pages.integration.test.ts` |
| REQ-004.definition | The exact Verifier Card route shows the graph, Graph/YAML, description, exact read-only version navigation and where-it-runs through the existing Card header and route; no Results link ships before TASK-003 (spec REQ-004; task Outcome and "Existing owner") | exact version, older version, no binding, Results link absent | `ui-pages.test.ts` page assertions; `ui-pages.integration.test.ts` real Card → BFF → Chromium |
| REQ-014.shared | One component/loader implementation serves the conventional page, gallery, custom consumer and built MCP artifact through the existing catalog, loaders, view renderer and executors (spec REQ-014; task "Inputs are…") | catalog entry, view document, Node executor, MCP executor, `examples/ui-app`, built `wyrd-view.html` | `ui-library.test.ts` catalog/view assertions; `ui-library.integration.test.ts` host proof |
| REQ-014.states | Foreign, denied, missing and failed reads stay truthful; a denied prerequisite issues no dependent read; a superseded load never overwrites the newer one (spec REQ-014; task Proof) | denied, missing, foreign tenant, error, non-Verifier Card, refused owner read, superseded version | `ui-library.test.ts` loader assertions; `ui-library.integration.test.ts` real refusals |
| AC-009.visual | Each introduced component and page is compared with the approved mock in both themes and viewports, with keyboard/focus/touch, long content and error states, from real `dev:full` input, and individually accepted by a human (spec AC-009, AC-002 graph comparison; task "Execution and verification") | desktop/mobile, light/dark, long content, error states, human acceptance | recorded screenshots against mock regions; human acceptance record |

## Evidence

Integrated source revisions: base `7b36f8fc6`; this task's commits `428ec5673`
through `64769eeac` on `task/wyrd-ui-main-integration/TASK-005`. Dependency
journey consumed: the existing exact Card read (`cards.get_by_ref`) through the
Node executor, the MCP `cards.get` tool and the BFF panel loader, against a
server, `@wyrd/testing` addon and SDK addon rebuilt from this tree.

Focused selectors: `ui-library.test.ts` — `verifier graph projects and lays out
the declared topology`, `verifier graph is read-only, selectable and keyboard
and zoom operable`, `verifier definition and bindings stay truthful across
hosts and states`; `ui-pages.test.ts` — `an exact Verifier Card page shows its
definition, versions and where it runs`; `ui-library.integration.test.ts` — `an
exact Verifier definition and where it runs load through every authorized
host`; `ui-pages.integration.test.ts` — `an exact Verifier Card page draws its
declared graph and where it runs`.

Exact invocations, each from the repository root. The unit selectors need no
services:

```bash
pnpm --dir crates/wyrd/wyrd-server/wyrd-ui package
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-library.test.ts -t "verifier graph projects and lays out the declared topology"
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-library.test.ts -t "verifier graph is read-only, selectable and keyboard and zoom operable"
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-library.test.ts -t "verifier definition and bindings stay truthful across hosts and states"
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-pages.test.ts -t "an exact Verifier Card page shows its definition, versions and where it runs"
```

The live selectors need Docker for the managed Postgres and, first, the SDK,
its testing addon and the server built from this tree:

```bash
mise run ts:build && mise run ts:build:testing && mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-library.integration.test.ts -t "an exact Verifier definition and where it runs load through every authorized host"
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-pages.integration.test.ts -t "an exact Verifier Card page draws its declared graph and where it runs"
```

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| REQ-004.topology — ordinary, session-shaped, drift-shaped, parallel, repeated declared topology; fan-in/out | `packages/ui/src/lib/verifier.ts` `definitionGraph` derives inputs, steps, attached and whole-Verifier requirements and edges from the spec's references, `when.path` and `depends_on`; `verifier-layout.ts` `layoutDefinition` lays out stages, parallel branches and routed edges; `VerifierGraph*.svelte` draw stage headings, arrowheads and labels | `ui-library.test.ts` `verifier graph projects and lays out the declared topology`; integration test on the registered `policy-review` (3 inputs, 3 chained steps, 2 attached and 6 whole requirements, 13 edges) and `investigation-drift` | `pnpm test` 60 passed; both integration selectors passed against a real server. Removing the gate-only dependency fails the unit test. Declared repetition is the `retries` chip and the gate the `when` chip: `VerifierSpec` has no fan-out or repeat field, so no repeated node is drawn | PASS |
| REQ-004.read-only — no runtime colouring; drag, reconnect and delete disabled | `VerifierGraph.svelte` sets every node and edge non-draggable, non-connectable, non-deletable and disables the Svelte Flow selection and delete keys; nodes carry kind and declared chips only | `ui-library.test.ts` `verifier graph is read-only…`; `ui-pages.integration.test.ts` real Chromium mouse drag and Delete | Unit and integration selectors passed. Making nodes draggable fails the Chromium assertion (node gap 156 → 231) and the unit option assertion. Reconnect rests on the unit assertion that no handle is connectable and no edge-update anchor is rendered. The Delete checks cannot fail: Svelte Flow selects nothing here, so Delete has nothing to remove with or without `deleteKey={null}`. "No runtime colouring" is checked by absent state words and classes, not by computed colour | PASS for drag and connect; delete and colour by construction only |
| REQ-004.interaction — click and keyboard selection, off-screen reveal, zoom in/out/fit/100%, narrow viewport | `VerifierGraph.svelte` selection context, arrow-key movement over layout targets with `reveal`, and the Zoom group; `VerifierDefinition.svelte` shows the selected node's declaration | `ui-library.test.ts` interaction test; `ui-pages.integration.test.ts` click, ArrowRight/ArrowLeft with focus, Zoom in/100%/Fit, 390px viewport at the 60% minimum with keyboard selection | Selectors passed. Off-screen reveal is asserted in the unit test only; touch panning was not exercised by any test | PASS for the listed keyboard, click and zoom cases; touch unverified |
| REQ-004.definition — exact version, older version, no binding, Results link absent | `src/routes/t/[tenantKey]/cards/detail/+page.server.ts` and `+page.svelte` load `VerifierDefinition` and `RunsOn` for a Verifier beside the existing `CardHeader`, `RelationshipList` and `CardFacts`; `cards-host.ts` `VERIFIER_TABS` Graph/Files/YAML | `ui-pages.test.ts` page test; `ui-pages.integration.test.ts` real declaration → BFF → Chromium: description, graph, YAML tab, Runs on opening the exact Service, older-version note and exact version switch, 404 and denied | Selectors passed. No Results link is rendered; it belongs to TASK-003 | PASS |
| REQ-014.shared — catalog entry, view document, Node executor, MCP executor, `examples/ui-app`, built `wyrd-view.html` | `schemas.ts`, generated `catalog.json`, `loaders.ts` `LOADERS`, `WyrdView.svelte` `VIEWS`, `index.ts`; no new executor, route or Card lookup. Reused `loadPanel`, `nodeExecutor`, `mcpExecutor`, `CardHeader`, `CardFacts`, the detail route | `ui-library.test.ts` catalog and view assertions; integration test runs the same loaders over Node and MCP, the BFF `ui/load`, the example app's `view/load`, and the built HTML behind an AppBridge host | Selectors passed; `pnpm check` 0 errors; `pnpm build` done; `mise run codegen:check` in sync. The integration test found the built HTML threw `process is not defined` once Svelte Flow was bundled; `vite.app.config.ts` now replaces `NODE_ENV` at build time. `wyrd-view.html` grew from about 623 kB to 869 kB. `brand/components.json` carries the three new contracts in the working tree only (see Notes) | PASS |
| REQ-014.states — denied, missing, foreign tenant, error, non-Verifier Card, refused owner read, superseded version | `verifier.ts` `readVerifier`, `loadVerifierDefinition`, `loadRunsOn`: a refused Verifier read issues no owner read, a refused owner keeps its row, a Card of another kind is a validation error | `ui-library.test.ts` `verifier definition and bindings stay truthful…`; integration test: missing version 404, Service ref refused, scoped key denied after one read each, foreign-tenant BFF session 404 | Selectors passed. Refused owner read and superseded load are proven with the fixture executor only, not against a real server | PASS |
| AC-009.visual — desktop/mobile, light/dark, long content, error states, human acceptance | Gallery checkpoints for `VerifierDefinition` and `RunsOn`; `scripts/dev/example-app.sh` setup registers `policy-desk` with its multi-step Verifiers | Gallery and `dev:full` screenshots (light/dark × 1440/390, selected node, missing version) and the mock's `verifier-policy-{light,dark}` shots, held in the session scratchpad, not in the repository | `mise run dev:full` reached `batch passed`; `policy-review`, `scored-review` and `whole-trace` pages drew with no console error and no horizontal overflow. Compared with the mock's `policy-review@1.0.0` page; differences listed in Notes. No human has reviewed or accepted any of it | FAIL — human acceptance outstanding |

### Notes

- Process exception: production code was written before its tests, so there
  was no RED run. Mutation checks stand in: draggable nodes, the Verifier page
  branch, the exact version pin and the gate-only dependency each fail a test
  when broken.
- Differences from the approved mock that remain: the page keeps the existing
  Card header and Graph/Files/YAML tabs instead of the mock's header with a
  Results action; the unselected inspector reads "Declaration" without the
  input/step/requirement counts; an input's detail does not list schema fields
  or size limits; a step's "needs" line names steps only; the real
  `policy-review` declares more references than the mock's sample, so it draws
  more arrows. A binding without `runs_on` reads "explicit invocation only",
  following the Card contract, where the mock's sample reads "every call".
- The gallery checkpoints name the mock as their reference in a note and carry
  no board regions, because the gallery's region links point at canvas boards
  this mock does not have.
- `859b55fb8` adds the `VerifierDefinition`, `RunsOn` and `VerifierGraph` contracts
  to the committed `brand/components.json`, in that file's committed entry
  format and as 102 added lines only. The uncommitted rewrite of that file in
  the working tree is not part of the commit. Checked at that commit: each
  entry is `built`, its props match the component's, its tokens match the
  ones the component uses, and `catalog` is true exactly for the two entries
  in the generated catalog.
- Every test the task's integration command selects has passed, but not in one
  invocation. `observe boards match approved canvas` failed on the Metrics
  legend because the Metrics default now merges series; `d8bfff7c2` asks the
  board for every series and it passes. In the full command 13 of 14 tests
  pass and `development environment lifecycle` fails in its refused-setup
  phase: `wyrd-server setup` is refused with `WYRD_AUTH_401_UNAUTHENTICATED`.
  The same test passes when run alone with
  `vitest run src/lib/ui-pages.integration.test.ts -t "development environment lifecycle"`,
  and `scripts/postgres/dev-server.sh` sets up cleanly with the same bad
  ambient `WYRD_API_KEY`. The launcher's ports are fixed, and a `wyrd-server`
  from another checkout on this machine was found listening on
  `127.0.0.1:8080` after the failing run; that contention is the suspected
  cause and was not proven for the failing moment. The full command needs a
  rerun on a machine where nothing else uses ports 8080, 3000, 8000 and 1080.
- The `dev:full` screenshots predate the arrow-label change in `f677bd329`
  and the narrower nodes in `64769eeac`.
- An independent read-only check of the diff against this ledger found, and
  these were fixed: a selection `box-shadow`, node widths reserved for
  execution text, an agent-mode marker in a comment, and no reconnect
  assertion. Left open from the same check: the 50-owner cap in `loadRunsOn`
  has no test; "No binding runs this exact version." trusts the server's
  inbound references to be complete; a touch drag on the graph pans it and may
  hold the page's vertical scroll (not exercised); the gallery cases have no
  test; a selected node id stays selected when the version changes; owners are
  keyed by reference and assume the server lists each once, as
  `RelationshipList` already does.
