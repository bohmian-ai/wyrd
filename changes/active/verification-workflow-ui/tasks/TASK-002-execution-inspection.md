---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 3
requirements: [REQ-002, REQ-004, REQ-005, REQ-006, REQ-007, REQ-009, REQ-014, AC-002, AC-003, AC-005, AC-008, AC-009]
depends_on: [TASK-001, TASK-001A]
---

# Exact execution, publications, session and drift inspection

## Outcome and prerequisite

Opening `/t/[tenantKey]/verify/executions/[executionId]` shows that execution's
workflow, current recorded state, judgments and retained outputs. Selecting a
node shows its recorded details; refresh preserves graph position, viewport and
exact producer selection. Session and drift use the same inspector. Cancel
uses the existing action. TASK-003 supplies discovery later; no list or PR
admission is required to open an existing execution.

Consume TASK-001's graph and TASK-001A's merged backend baseline, with existing
operations recorded in the
[finalized read map](../design/contract-alignment-research.md). Record their
integrated revision and real host proof. This is UI work: components, Bifrost
queries, typed UI adapters, navigation and tests. No server fields/endpoints,
SDK wire-contract changes, query services, storage or execution logic. An
undelivered read/host capability remains an explicit prerequisite, never a
fixture substitute or permission to implement the backend here.

## Data and existing owners

| Content | Existing source | UI change and consumer |
|---|---|---|
| Graph and description | Exact Verifier Card from the Postgres-backed registry through existing Card APIs | Resolve its exact reference/UID, then reuse TASK-001's graph and exact Card content. No direct browser database access. |
| Current lifecycle/error and selected judgment | Existing execution status; compact VerificationResult | Shared execution context and requirement presentation; lifecycle and verdict stay separate. |
| Completed steps/checks/provenance/evidence | Existing explicit Execution.details operation | Selected-node inspector; do not assume compact result contains these fields. |
| Attempts and accepted progress | Existing bounded invocation pages | Exact step/item/iteration/attempt selection and recorded speculative disposition. |
| Outputs | Existing publication pages, exact publication/manifest and file/content/download operations | Equal named metadata first; selected body only, with bounded safe previews and normal host download. |
| Correlation, retained lookup and history | Existing permitted Bifrost results/inputs/outputs/events queries | Parameterized bounded UI queries, preserving server-selected result identity rather than choosing the latest result row. |
| Session/drift/citations | Recorded group snapshot, requirement drift detail, executed provenance/window, existing Bifrost history and exact trace reads | Contextual inspector with recorded members, features, scores and thresholds; never recompute a verdict or membership. |

Extend `packages/ui/src/lib/{contract,loaders,node,mcp}.ts`, catalog/view exports
and host dispatch. Deliver the missing runtime requirement/step/session/drift
panels within that existing family; they are not already implemented components.
Reuse Chart, generic panels, existing exact trace inspection and TASK-001's
projection/layout rather than creating a second loader or graph owner.

For a completed execution, its result supplies exact Card references. Before
that, existing Bifrost execution metadata exposes `verifier_card_uid`; existing
HTTP/MCP Card reads accept kind and UID. Wire the missing UI host adapter to
that existing operation. TypeScript Cards.get currently accepts a versioned
reference, not a bare UID: do not pass an unsupported selector. Reuse an exact
retained CardRef where available or the existing UID-capable host operation.
Until identity/definition is visible or authorized, explain that availability
without erasing readable execution state or inventing a different declaration.

Adapt the shared graph projection to integrated Step contracts: foreach, repeat
and speculative controls. TASK-001A's compatibility repair already projects
dependency lists and `any_succeeded` joins (an edge to each predecessor plus an
`any succeeded` chip), approval steps (their message) and pull request extracts
(no input read); extend that projection, not a second one. Include declared
external references and annotations while retaining one node per authored step.
Runtime state never owns layout. Preserve the existing exact Verifier Card view
with no runtime coloring; TASK-001 stays complete.

## Interaction and failure behavior

Compact results retain requirement judgments, actual/citations/custom data and
check-specific drift detail. Load full details explicitly when needed for the
inspector, using authored recorded data without compiling Prompts or invoking
providers. Details are completed records, not the progress source or a wait loop.
Keep state, judgment, details and output availability independent. A live/direct
result does not certify retained visibility; normal refresh may reveal records
later. Unknown, foreign, denied, unavailable and not-yet-visible responses must
remain truthful. Do not treat every 404 as running, block a BFF load on wait,
resubmit work or issue dependent sensitive reads after a denied prerequisite.

Use generated producer identity, including nullable item/iteration and one-based
attempt. Preserve exact execution/result, step/requirement, producer and output
in navigation/copy links; a stale response cannot replace newer selection.
Respect opaque cursors and bounded pages. Do not replace selected publications
with StepResult.output, a final-step shortcut or a primary-report heuristic.
Preview bounded JSON, safe text/Markdown and directory manifests; never execute
HTML or download a directory merely to list it. File access stays with hosts.

Use the selected execution's recorded group snapshot for ordered session members
and restore selection when returning. Keep drift window/method/baseline facts,
all recorded features, scores/thresholds and bounded history from the existing
reads; if context is missing or refused, say so instead of adding a server read
or inventing it from current Card status. Later spans do not revise judgments.
Exact citations use current trace permissions independently of list filters.

Host cancellation guards duplicate pending actions and reads back existing
status; terminal completion can win. Retained progress/outputs stay readable.
Waiting/approval decisions belong to TASK-006; provider delivery to TASK-007.

## Observable proof

At the existing UI unit owners, prove graph controls and stable selection,
compact-only judgment rendering, explicitly loaded details and independent
refusals, exact producer/output changes and safe/oversized previews. Prove that
the shared projection still renders the definition page after Step adaptation.

Through existing public SDK/server setup and the production BFF/Chromium page,
open an exact execution while unfinished, select an intermediate publication,
refresh and reopen after settlement. The page and shared host component must
show the same recorded identities. Include no-result and delayed-details states;
loading or refusing details must not hide other readable content. Inspection
must not invoke a provider or create replacement work.

Exercise owner-distinct pinned sessions, drift history, out-of-range/exact trace
citations, later spans, foreign/denied/unavailable reads and cancel/completion
races. Repeat affected adapters in Node/MCP/custom/built-artifact hosts. Compare
ordinary/session/drift and wide graph/inspector regions with the approved mock,
both themes and narrow/wide layouts, keyboard/focus/touch. Fixtures supplement
these real journeys. Existing scoped operations and Bifrost permissions must
produce the refusals; UI filters are not authorization.

## Execution and verification

This task is against approved spec revision 3, whose scope transfer is
synchronized. Activation requires only this task's named contract/authority
and predecessor outputs. Unrelated backend delivery
or authority amendments do not block it. This packet consumes existing read/action
capabilities and adds no underlying server changes; a missing required capability
remains an explicit external dependency. Record the integrated source revisions
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

Use the existing UI integration owners to prove real Bifrost/domain inputs through
Node/MCP/BFF/custom/built-artifact adapters. Do not add or change server/SDK wire
contracts or their generators. Regenerate the UI catalog when schemas change and
run `mise run codegen:check` where applicable. Full shipped-surface journey lanes
belong to integrated change review; focused UI/host journeys prove this task.

## Obligations

Recorded before the first edit. Proof owners are the four existing UI test
files; selectors are recorded under Execution evidence before their first run.

| ID | Promise (required cases) | Contract source | Planned proof |
|---|---|---|---|
| REQ-002 | An execution opens at its exact URL; execution, node, producer (step, nullable item/iteration, one-based attempt) and output survive refresh, copy link and return. A stale response never replaces a newer selection; the viewport and layout do not move on refresh. | Outcome; Interaction ¶2; spec REQ-002 | Page unit test for URL state and stale responses; Chromium refresh/reopen journey |
| REQ-004 | The shared projection draws foreach, repeat and speculative controls, declared external references and annotations, one node per authored step. Runtime state colours nodes and never owns layout. The exact Verifier Card page keeps no runtime colouring. | Data ¶4; spec REQ-004 | Component unit test on the projection, node positions and viewport unchanged when recorded state changes, and the unchanged definition tests |
| REQ-005 | Lifecycle and verdict are separate. Compact results render judgment, actual, citations, custom data and drift detail alone; full details load only on request and are never a wait loop. State, judgment, details and outputs each fail independently: unknown, foreign, denied, unavailable and not-yet-visible stay distinct, a 404 is not "running", and no dependent sensitive read follows a denied prerequisite. Verifier identity comes from the result's exact reference, else the retained `verifier_card_uid` through the existing UID Card read; without it the page says so and keeps readable state. | Data ¶3; Interaction ¶1; spec REQ-005 | Component unit test per refusal and for request planning; host integration through Node and MCP |
| REQ-006 | Every authorized retained publication is listed as a peer with its producer and disposition, from bounded pages with opaque cursors. Only the selected body is read. JSON, text/Markdown and directory manifests preview bounded and inert: HTML never executes, a directory is never downloaded to list it, oversized content says so. Download stays a host operation. No StepResult.output, final-step or primary-report substitute. | Data table; Interaction ¶2; spec REQ-006 | Component unit test for previews and paging; host integration on a held execution's publications; Chromium open and download |
| REQ-007 | A session judgment lists its ordered members from the recorded group snapshot and restores selection on return. A drift judgment shows recorded window, method, baseline, every feature (drifted first), scores, thresholds and bounded history from existing reads, with the viewed window marked. Nothing is recomputed; missing or refused context says so. Exact citations use current trace permissions. | Interaction ¶3; spec REQ-007 | Component unit test on recorded session and drift fixtures; host integration on a real group execution and a real drift result |
| REQ-009 | Cancel is the existing action, issued by the host: a pending cancel cannot be submitted twice, the page reads status back, a terminal completion can win, and retained progress and outputs stay readable. A viewer's refusal is shown as denied. | Interaction ¶4; spec REQ-009 | Page unit test for the duplicate guard and readback; host integration for cancel, repeat, completion race and denied cancel |
| REQ-014 | The runtime panels live in the existing component family: contract, loaders, Node and MCP executors, catalog, view exports and BFF dispatch, with no second loader or graph owner and no server, SDK or storage change. | Outcome ¶2; Data ¶2; spec REQ-014 | Catalog and contract gate (`brand/gen-theme.test.mjs`); the same loader through Node, MCP, BFF, custom app and built artifact |
| AC-002 | Through public setup and the production BFF in Chromium: open an unfinished execution, select an intermediate publication, refresh, reopen after settlement; page and shared component show the same recorded identities; no-result and delayed-details states keep other content readable; inspection invokes no provider and creates no work. | Observable proof ¶2 | `ui-pages.integration.test.ts` journey |
| AC-003 | Owner-distinct pinned sessions, drift history, out-of-range and exact trace citations, and later spans leave recorded membership and judgments unchanged; denied evidence is visible. | Observable proof ¶3 | `ui-library.integration.test.ts` and Chromium journeys |
| AC-005 | Cancel unfinished work, race completion, reopen: the authoritative terminal state wins and publications remain readable. Provider delivery belongs to TASK-007. | Observable proof ¶3 | Host integration and Chromium cancel journey |
| AC-008 | Foreign ids, restricted Verifier scopes, denied prerequisites and unavailable reads through real Node, MCP, BFF, custom and built-artifact adapters: no foreign content, no follow-on sensitive read, no denial shown as empty. | Observable proof ¶3 | `ui-library.integration.test.ts` refusal cases |
| AC-009 | `mise run dev:full` produces this task's real inputs where needed; each introduced component and page is compared with the approved mock in both themes, narrow and wide, keyboard, focus and touch, long content and error states, with screenshots and individual human acceptance. | Execution ¶5 | Gallery entries, recorded screenshots; human acceptance is recorded only when given |

## Execution evidence

Consumed revisions: TASK-001A baseline merge `d118ee49f` (upstream `f6fac677c`,
repair `0d337a006`). Candidate: `08c3ce39f..HEAD` on
`task/wyrd-ui-main-integration/TASK-005`. Paths below are under
`crates/wyrd/wyrd-server/wyrd-ui/`; `U` is `src/lib/ui-library.test.ts`, `P`
`src/lib/ui-pages.test.ts`, `UI` `src/lib/ui-library.integration.test.ts`, `PI`
`src/lib/ui-pages.integration.test.ts`.

Focused selectors (vitest `-t`):

- `U`: `verifier graph projects fan-out, repetition and speculative controls on one node per step`; `execution reads run as the existing SDK, HTTP and MCP operations`; `execution loader reads each part once and keeps refusals apart`; `execution states mark the declared graph with recorded progress`; `execution inspector shows recorded state, judgments and outputs as separate facts`; `a retained text file previews as inert text within the bound, and says when it cannot`.
- `P`: `an exact execution page keeps its selection in the URL and cancels once through the host`; unchanged `an exact Verifier Card page shows its definition, versions and where it runs`.
- `UI`: `an unfinished execution shows recorded progress and retained outputs through every authorized host, and cancels once`; `a judged execution shows its judgments, accepted inputs, completed details, drift and session through the Node and MCP hosts`.
- `PI`: `an exact execution page keeps its selection through refresh, downloads a retained file and cancels once`.

RED and process exceptions:

- RED recorded: `a retained text file previews…` failed before `verification.read_publication_text` existed (`expected { …(5) } to deeply equal { …(6) }`: the last read was `verification.get_publication`), then passed.
- Process exception: the `UI` and `PI` execution journeys, and the later-turn, cancel-after-completion and text-preview cases added to them, passed on first run; the production code already existed. The `U`/`P` tests of the first three commits were written with their production code, not before it.
- Ledger mechanics correction: REQ-004 planned "layout equality with and without runtime state". The execution graph now reserves a fixed room for the recorded-state word on each id row (`STATE_ROOM`, `packages/ui/src/lib/verifier-layout.ts`), because the Chromium screenshots showed a requirement id truncated to "d…" beside its state. The contract (runtime state never moves a node) is unchanged and proven by the rerender assertions; the row's planned proof now says so.

Diagnosis: `PI` `development environment lifecycle` fails in the full file.

- **Symptom:** the refused-setup step waits for `batch        (not sent|passed|FAILED)`; `dev:full` ends with `wyrd-server: fatal error: [WYRD_AUTH_401_UNAUTHENTICATED]` and `wyrd-server exited before it was ready`.
- **Evidence:** vitest stderr shows `ERROR wyrd_auth::platform_sessions error=invalid platform session` from the in-process harness server seven seconds after MockServer is ready; exit 70 is `wyrd-server setup`, not the serve process. TASK-001 recorded the same failure in the full command and a pass alone.
- **Cause:** the file-scoped SDK harness stubs `WYRD_SERVER_URL` (`sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts`, `deploy`), the test spawns `dev:full` with `...process.env`, and `wyrd-server setup` reads `WYRD_SERVER_URL` (`crates/wyrd/wyrd-server/src/main.rs`, `SetupArgs.server`), so tenant setup presents its new root credential to the harness server. Independent read-only diagnosis; all three ingredients predate `152bf76d6`.
- **Fix site:** `scripts/postgres/dev-server.sh` (only caller: `dev:full`), which starts and probes `127.0.0.1:8080` itself: `setup` now passes `--server http://127.0.0.1:8080`, as its next line already did. Outside the UI write set; no server, SDK or contract change.
- **Second and third occurrence, same cause:** with setup fixed, sign-in stayed on the login page because the `dev:full` UI reads `WYRD_SERVER_URL` (`src/lib/server/upstream.ts`) and so signed in against the harness server; then `dev:batch` timed out waiting for its probe span for the same reason. Fix site: the two spawns in `PI` now share `devEnv`, which leaves the harness's stubbed `WYRD_SERVER_URL`, `WYRD_GRPC_URL` and `WYRD_API_KEY` out; no assertion or timeout changed. A developer who exports `WYRD_SERVER_URL` still points the `dev:full` UI elsewhere; that is not changed here.
- **Result:** the task's full integration command passes: 2 files, 20 tests, twice in a row at `f28152c73`.
- **Fourth occurrence, the test runner's marker in the development UI:**
  - **Symptom:** in two of four full runs the lifecycle test timed out after clicking a Card row in the `dev:full` UI (no navigation). Separately, the new step that opens a real queued execution page in that UI showed an empty graph and no status header on every run.
  - **Evidence:** the browser reported `lifecycle_outside_component` from `setContext` in `VerifierGraph`, with every frame served from `node_modules/.vite-vitest/deps/`. `vite.config.ts` selects that cache directory, and `svelteTesting()` changes Svelte resolution, when `VITEST` is set. The test spawns `dev:full` with the Vitest worker's environment, so the development UI ran as a test build and shared this run's dependency cache.
  - **Cause:** `VITEST` leaked into the development UI through `devEnv`. Established for the empty graph: with `VITEST` left out the page draws, the console is clean, and the lifecycle test passes. For the Card-row timeout the same leak is the likely cause (a failed hydration leaves that JS-only button inert, as the read-only diagnostician found) but that step's failure was not observed directly; an earlier note here blaming files edited during the run was wrong.
  - **Fix site:** `devEnv` in `PI`, which already leaves the harness's server address and key out; it now leaves `VITEST` out too. `mise run dev:full` run by a developer never had the marker and is unaffected.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| REQ-002: exact URL; execution, node, producer and output survive refresh, link and return; stale response never replaces newer selection; viewport and layout still | `src/routes/t/[tenantKey]/verify/executions/[executionId]/+page.{server.ts,svelte}`; `packages/ui/src/lib/schemas.ts` `ExecutionInput` | `P:566`; `PI:1137` | `P` selector passes (URL state, stale response); `PI` passes in Chromium: `{node, attempt, output}` in the query through Refresh and reload, zoom and node rectangles equal before and after After the check, `PI` asserts Back restores the earlier selection and its preview and Forward the later one; the copied link is the page URL, which the same journey reloads **Second check, not proven:** a stale response replacing a newer selection is not exercised with older data (`P` resolves the navigation only); Copy link is asserted in jsdom only; a non-null iteration is never round-tripped through the URL **Closed after the second check:** `PI` `an exact execution page keeps its selection through refresh, downloads a retained file and cancels once` holds the answer for an older selection in Chromium until a newer selection is shown, lets it finish, and asserts the newer URL and pressed node stand; the same test clicks Copy link, reads the clipboard, and opens the copied address in a new tab to the same producer attempt and output. `P` `an exact execution page keeps its selection in the URL and cancels once through the host` round-trips a non-null iteration and asserts the publication read names that iteration | PASS |
| REQ-004: foreach, repeat, speculative controls, external references, annotations, one node per step; runtime state never owns layout; definition page uncoloured | `packages/ui/src/lib/{verifier.ts,verifier-layout.ts}`, `components/VerifierGraph*.svelte` | `U:2404`, `U:2780`, `U:2821`; `P:498` | `U` selectors pass (controls projected; positions and viewport equal after a state rerender; no state drawn without `states`); `P:498` unchanged and passing. **Annotations, decided by the maintainer on 2026-11-16:** the Step contract has no annotation field, and "annotations" means the control chips drawn on a step node (when, any succeeded, foreach, repeat, retries, speculative), which the same `U` selectors assert. No Step field is added | PASS |
| REQ-005: lifecycle and verdict separate; compact result alone; details on request only; independent refusals; 404 is not running; no dependent read after a denied prerequisite; identity from result reference else retained UID | `packages/ui/src/lib/execution.ts` `loadExecution`; `components/VerificationExecution.svelte` | `U:2670`, `U:2821`; `UI:837`, `UI:992` | `U` pass (read plan per case; each refusal in place). `UI` pass on a real server through Node and MCP: running with no result and definition by UID; failed result by exact reference; query-only key denied after exactly one read; `verifier:read`-only key reads status with definition and inputs each `denied`; unknown id is the server's 404 After the check: the rendered `Actual` and `Data` of a judgment are asserted in `U` (`execution inspector shows a recorded actual value…` and the requirement case); an unavailable read is the `error` part state, asserted per part in `U` beside `denied` and `empty` **Second check, not proven:** no test injects an unavailable (5xx or transport) read, so unavailable is not shown distinct from unknown; progress (`list_invocations`) is never refused on its own; the not-yet-visible definition reason is asserted on loader output, not rendered **Closed after the second check:** `U` `an unavailable read, a refused progress read and context not yet retained each say so in place` renders a 503 execution read and a 404 one and asserts each shows its own code and not the other's title; refuses `list_invocations` alone and asserts the progress part names the problem while status, graph and outputs stay drawn; renders the not-yet-visible Verifier reason. The restricted principal's single read is now asserted through MCP as well (`UI` `an unfinished execution shows recorded progress and retained outputs through every authorized host, and cancels once`) | PASS |
| REQ-006: every publication a peer with producer and disposition, bounded pages; only the selected body read; JSON, text/Markdown, manifests bounded and inert; oversized says so; host download; no primary-output substitute | `execution.ts` `readPublication`, `TEXT_PREVIEW`; `contract.ts`/`node.ts`/`mcp.ts` `verification.read_publication_text`; `download/+server.ts` | `U:2821`, `U:3000`; `UI:837`; `PI:1137` | `U` pass (peers, cursors, inert JSON, manifest, text as escaped source, non-text, oversized never read, refusal beside manifest). `UI` pass: 4 publications, exact `tree` manifest, `report` text `# Report\n` through Node, MCP host refusal shown as that part's error, download route `attachment` + `nosniff`, JSON output 400. `PI` pass: preview, text, browser download of `nested/b.md` After the check, `U` (`execution inspector shows a recorded actual value, a discarded output, a cut preview and the next page of outputs`) asserts a `discarded` output, a JSON preview cut at 64 KiB with the remainder stated, and the next page of outputs sent with the server's cursor. Paging is proven at the unit tier, as planned; the real held execution has one page | PASS |
| REQ-007: session members from the recorded snapshot, selection restored; drift window, method, baseline, features, scores, thresholds, bounded history, viewed window marked; nothing recomputed; missing or refused context says so; exact citations use current trace permissions | `execution.ts` members, drift context; `VerificationExecution.svelte` session, drift and evidence sections | `U:2821`; `UI:992` | `U` pass on recorded session and drift fixtures. `UI` pass: real group execution (3 pinned members in recorded order, a later fourth turn changes nothing), real scheduled drift result (score, history, window). After the check: `UI` `a session judgment cites an exact trace: the citation leads to its pinned member and trace, a later span revises nothing, and trace reads keep their own permission` proves on a real server, through Node and MCP, a judgment citing `traces` at `/0`, the recorded membership, the cited member's accepted input naming the exact trace, that trace read by ID alone, and a `verifier:read`-only key that still sees the citation while the trace read is `denied`. `U` `a drift judgment shows its declared baseline and says when its window and history are refused` (RED recorded: no declared baseline was drawn) proves the baseline from the exact Verifier's declaration and refused context said in place. Returning to a session restores its selection because the member opens as a pushed navigation (`P`) and the selection is URL-held (`PI` Back and Forward) **Second check, not proven:** no test opens a member and returns to the session with its selection restored; a drift context with no rows (as opposed to refused) is not asserted; the later span and the restricted key are asserted through Node only, not MCP **Closed after the second check:** `PI` `session, drift and cited judgments open as pages: members in recorded order, selection kept on return, declared baseline, and refused context said in place` opens the second pinned member from a real session page in Chromium, goes Back, and asserts the session URL, the pressed source and the same ordered members. `U` `an unavailable read, a refused progress read and context not yet retained each say so in place` asserts a drift context with no retained row says so. `UI` `a session judgment cites an exact trace: …` now asserts the later span and the restricted key through both Node and MCP | PASS |
| REQ-009: existing cancel through the host; no duplicate pending cancel; status read back; terminal completion can win; progress and outputs stay readable; viewer refusal shown as denied | `cancel/+server.ts`; `+page.svelte` action state | `P:566`; `UI:837`, `UI:992`; `PI:1137` | `P` pass (duplicate guard, readback). `UI` pass: viewer 403, foreign 404, owner cancel twice → `cancelled` both times, publications still 4; cancel on a completed execution answers `completed` and the failed result stays readable. `PI` pass: double click sends one POST **Closed after the third check:** `UI` `an unfinished execution shows recorded progress and retained outputs through every authorized host, and cancels once` asserts recorded progress is still populated after the cancel, through Node and the custom app | PASS |
| REQ-014: panels in the existing family; no second loader or graph owner; no server, SDK or storage change | `packages/ui` contract, loaders, executors, `catalog.json` (generated), `brand/components.json`; host `ui/load` | `brand/gen-theme.test.mjs`; `UI:837` | `mise run ui:check` and `mise run codegen:check` pass; `UI` pass through Node, MCP, BFF, custom app and built artifact. Diff outside `wyrd-ui/` and this file: `scripts/postgres/dev-server.sh` only | PASS |
| AC-002: public setup and production BFF in Chromium: unfinished, intermediate publication, refresh, reopen after settlement; same identities; no-result and delayed-details keep content readable; no provider call or new work | page and component above | `PI:1137`; `UI:837` | `PI` pass: running with no verdict, publication selected, refresh, details hint while unfinished, reopen at phone width after cancel with 4 outputs. The server runs without the verification runtime, so inspection cannot invoke a provider or start work. After the check, `PI` `a completed execution page shows the recorded judgment, loads details only when asked, and stays completed` runs a real failed verification and asserts in Chromium: status and verdict apart, the same execution id, Verifier reference, outcome and reason the shared loader reads for that execution, details absent until asked then shown with the judgment still readable, reopen after settlement, and that the tenant holds only the one submitted execution afterwards. Passed on first run after an import fix (process exception: no RED) **Second check, not proven:** no assertion that opening the page invokes no provider or judge; a visible result with details pending or refused is unit-only, not Chromium; the unfinished journey is not compared with the loader, and producer and output identity are not compared **Closed after the second check:** `PI` `a completed execution page shows the recorded judgment, loads details only when asked, and stays completed` counts requests to the judging provider and asserts none arrived while the page was inspected, and holds the details read in Chromium while asserting the compact judgment and graph stay readable and no details are drawn. `PI` `an exact execution page keeps its selection through refresh, downloads a retained file and cancels once` compares the unfinished page's status and each listed output with its producer step against the shared loader's read of the same execution. The page renders the shared `VerificationExecution` component; the comparison is page DOM against the shared loader's data. Refused details hiding nothing is proven at the unit tier, where the task places independent refusals | PASS |
| AC-003: owner-distinct pinned sessions, drift history, out-of-range and exact trace citations, later spans; membership and judgments unchanged; denied evidence visible | as REQ-007 | `UI:992` | Pinned session, later turn, drift history and denied inputs proven on a real server. After the check: `UI` `a session judgment cites an exact trace: the citation leads to its pinned member and trace, a later span revises nothing, and trace reads keep their own permission` proves an exact trace citation read outside any list range, a span exported after the judgment that appears in the trace while judgments and membership compare equal before and after, and denied trace evidence for a restricted key. Real drift history holds the one recorded window; several windows are proven in `U`. These journeys are host-level (Node and MCP); the Chromium journeys cover the unfinished and the completed judged execution, not a session or drift page Owner-distinct sessions: the same test ends a second Service's session under the same group id on the same server and asserts, through Node and MCP, that each judged execution names its own owner and pins only its own members **Second check, not proven:** no Chromium assertion for a session, drift or citation page; the trace is read by ID with no range but nothing shows it lies outside a list range; denied evidence is asserted as loader state, not rendered; real drift history is one point **Closed after the second check:** `PI` `session, drift and cited judgments open as pages: members in recorded order, selection kept on return, declared baseline, and refused context said in place` is the Chromium journey for a session, a drift judgment and a cited judgment, and shows a restricted principal the drift window and history as refused in place. `UI` `a session judgment cites an exact trace: …` lists traces over a later day, asserts the cited trace is absent from that list and still opens by ID, and draws the trace refusal from the real denied read through Node and MCP. `UI` `a judged execution shows its judgments, accepted inputs, completed details, drift and session through the Node and MCP hosts` runs a second real window of the same binding and asserts its history holds both windows in order while the first window's history is unchanged | PASS |
| AC-005: cancel unfinished work, race completion, reopen: terminal state wins, publications readable | as REQ-009 | `UI:837`, `UI:992`; `PI:1137` | Pass, as REQ-009; reopen after cancel shows `cancelled` with all outputs | PASS |
| AC-008: foreign ids, restricted scopes, denied prerequisites, unavailable reads through Node, MCP, BFF, custom and built-artifact adapters | loader refusal handling | `UI:837`, `UI:992`; `PI:1137` | Pass: foreign tenant 404 through Node, MCP, BFF panel, download and cancel routes and the server-rendered page; restricted keys denied per part with no follow-on read. The custom app answers an unknown execution with the server's 404 problem, and the built artifact shows the MCP server's 404 after exactly one tool call (`verification.get_execution`) After the check, `UI` asserts another tenant's 404 for the real execution through Node, MCP, the custom app (run as that tenant's key) and the built artifact (answered by MCP as that tenant, one tool call), and the `verifier:read`-only key through Node and MCP with definition and inputs each denied **Second check, not proven:** restricted scopes and denied prerequisites are asserted through Node and MCP only, not BFF, custom app or built artifact; an unavailable read is asserted through no adapter; the `verifier:read` case records no call list **Closed after the second check:** `UI` `an unfinished execution shows recorded progress and retained outputs through every authorized host, and cancels once` sends the restricted principal through BFF, custom app, MCP and the built artifact: each is `denied`, and MCP and the artifact record exactly the one execution read. It points Node, BFF and the custom app at a loopback address nothing listens on, uses a closed MCP session, and an artifact host whose tool call fails: each is an `error` problem that is not the unknown-execution one, and the artifact draws no recorded status | PASS |
| AC-009: `dev:full` real inputs; comparison with the approved mock in both themes, narrow and wide, keyboard, focus, touch, long content, errors; screenshots; individual human acceptance | `_gallery` entries for `VerificationExecution`; `PI` `capture` | screenshots (below) | Light and dark at 1280 and 390 captured from the real page. `PI` `development environment lifecycle` now asks the running `dev:full` app one question, takes the `follows-policy` execution it queues, and opens that execution's page in the development UI with recorded status, its Verifier and its graph; `dev:full` needed no extension for that. It produces no session, drift or publication-bearing execution. **Not done:** comparison with the approved mock and individual human acceptance, which only a person can give | FAIL |

Screenshots (local, not committed): `execution-{running,publication,text,cancelled}-{light,dark}-1280.png`,
`execution-settled-{light,dark}-390.png`, written by `PI` when `WYRD_UI_CAPTURE_DIR` is set.

Non-goals held: no server field, endpoint, SDK wire contract, query service,
storage, migration or execution logic changed; no new dependency. The MCP host
has no tool that reads a retained file's text, so that preview is an explicit
host prerequisite there and shows the host's refusal.

Independent read-only check (task file, ledger and diff only): no non-goal
violated; REQ-009, REQ-014 and AC-005 proven (the completion "race" is a
sequential cancel after completion; the completed execution has no
publications, so retained outputs after a terminal state are shown for the
cancelled one). Rows above carry its unproven cases. It also names task prose
the ledger does not list as its own row: the content of completed details in
the inspector, recorded speculative disposition per attempt, and preserving the
server-selected result identity in Bifrost queries.

Second independent check (read-only, task file and diff only, at `24acbd169`): REQ-006, REQ-009 and AC-005 proven; REQ-014 proven for the adapter cases, its catalog gate resting on the `ui:check` and `codegen:check` runs recorded above; six rows had required cases without an assertion at the demanded tier and are marked FAIL with those cases named in each row.

After the second check each named case was closed with an assertion (commits `ed707fa25`, `573351552`, `2f4fa840e`, `2d98735df`); the cases those tests cover behaved correctly already, so no RED was available for them and none is claimed. The task's full integration command passes at `2d98735df`: 2 files, 20 tests.

Third independent check (read-only, at `e1ee6e440`): nine of ten PASS rows proven; REQ-009 lacked an assertion that progress stays readable after cancel, now added. It also noted that line numbers cited in older rows have drifted (test names are the stable reference), that the BFF and custom app record no call list, and that the MCP and artifact unreachable cases are a closed session and a failing host call.

Status: `BLOCKED`. Open: AC-009 (the maintainer will compare the pages with the approved mock and record acceptance; nothing else is open).
