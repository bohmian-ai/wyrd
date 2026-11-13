# TASK-009 — Changes and repository workflows

Spec: [revision 28, approved](../spec.md). Status: Proposed; **externally gated**, not executable to completion against current main. Repository presentation and GitHub connection moved to [verification-workflow-ui](../../verification-workflow-ui/spec.md#dependencies-authority-and-scope-transfer) revision 2 (2026-11-11); the Change Request scope remains here and is deferred.
Dependencies: completed TASK-003/TASK-004/TASK-027, [Cards and Prompts](TASK-005-cards-and-prompts.md), [Verification](TASK-007-verification.md).
Mapped obligations: REQ-020/021/022/029/030/034/062; AC-010/011/013/041; INV-001–INV-007.

External activation dependency: separately approved and shipped Change Request, repository, webhook and Verifier-folder read/write contracts, their generated types, SDK/MCP projections and real dependency tests. This assignment consumes those contracts; it does not authorize their design or backend implementation. Contract-independent presentation may progress, but neither this task nor its live routes can be accepted with fixtures or unavailable actions standing in for the prerequisite.

## Outcome and Value

A signed-in user can browse Change Requests, investigate one exact revision, comment and review it, request verification, and manage repository connections. The same twelve shared components render these workflows in the gallery, custom applications and the MCP App host. After completion, normal development startup exposes real Changes and Repos destinations with representative data.

Before this task, those destinations and component consumers are absent. Afterwards, selecting a different revision changes its subjects, cause, Verifiers and discussion consistently; a denied write leaves the recorded revision intact and explains the refusal.

## What Changes and What Must Stay True

Own `ChangeRequestList`, `ChangeHeader`, `SubjectsTable`, `RevisionList`, `ActivityFeed`, `ChangeVerifierList`, `RequestVerificationForm`, `ReviewSubmission`, `Participants`, `RepositoryList`, `WyrdTomlView` and `VerifierFolderTable`, including portable loaders, catalog/view support, intents, gallery states and their first conventional consumers. Enable Changes and add Repos to the existing tenant rail. Entry routes are `/t/[tenantKey]/changes` and `/t/[tenantKey]/repos`; a Change Request has a shareable `/t/[tenantKey]/changes/[id]` destination with selected revision and tab preserved in navigation. Retain Overview, Verification, Audit and Files changed tabs, including authorized subject/file detail navigation. Exact server identities, not display names or guessed URL values, resolve their data.

A Change Request may span multiple PRs and repositories. Every recorded update has a server-owned revision and cause. Verifiers show origin (`wyrd.toml` or user request), start condition (open or evidence arrival), status and exact recorded verification links. The Verification tab lists Verifiers. Overview contains intent, Subjects, revisions, conversation/comments, reviews and Participants. Repos shows connected repositories, actual webhook health/configuration, the `wyrd.toml` location and its Verifier folder. Comments, review submission, verification requests and connection/configuration writes complete through the signed-in caller's approved platform actions.

Components render generated domain props without IO or credentials. Shared loaders own read planning, shaping, revision consistency and cancellation; page files own layout and host state. The existing Node/MCP executors and authenticated BFF own authorized IO. A revision switch or navigation supersedes older loads; failed prerequisites issue no dependent reads. Host actions validate JSON-safe intents, prevent duplicate pending submissions and return pending/success/denied/error. Do not mutate the displayed revision into success before confirmed server readback. Do not automatically retry an ambiguous write. A component-local failure does not terminate the BFF or unrelated pages; stable denial/error stays visible and explicit retry/navigation can recover.

No Claims, Policy gates, carry-forward, override, mock store, browser-owned CR workflow, frontend webhook controller, new backend subsystem or Ask runtime. Historical mock references establish visual/interaction evidence only, never durable schemas or public APIs. Credentials never enter page props, fixtures, view documents or screenshots. Server audit and permission boundaries remain intact.

### Required visual coverage

Changes list; ChangeDetail header/Overview/intent/Subjects/revision/conversation/review/Participants; ChangeVerification Verifiers/request/evidence; Audit and Files changed; Repos connected/configuration/folder regions. Map each of the twelve components to its region. The linked ChangeDetail canvas is the visual target for its tabs, including Audit and Files changed; any historical source reads are supporting evidence only and do not replace the approved canvas.

## Approved Canvases

These approved canvases define the visual target for this task. Follow their layout, component placement, hierarchy and interactions while using current Wyrd contracts and brand tokens. Apply the same component design in the gallery and the working UI. Record a separate comparison and human acceptance for each named canvas on desktop/mobile in light/dark.

- [Changes.dc.html](../references/canvas/Changes.dc.html)
- [ChangeDetail.dc.html](../references/canvas/ChangeDetail.dc.html)
- [ChangeVerification.dc.html](../references/canvas/ChangeVerification.dc.html)
- [Repos.dc.html](../references/canvas/Repos.dc.html)

If a required visual detail is absent or contradictory in these approved canvases, stop that affected visual work and ask the maintainer for clarification. Do not guess a layout, invent a replacement design, or treat a generic directory link as a visual specification.

## Approach

1. Verify prerequisite activation against approved generated contracts and actual SDK/MCP actions; record exact owners and projections before defining domain inputs or fixtures.
2. Deliver revision-aware package components and gallery documents alongside Changes list/detail/tab routes.
3. Deliver repository components and real Repos route/configuration actions using the same host boundaries.
4. Extend the existing development setup with public workflows producing representative repository and revision data; print or document exact navigation.
5. Prove shared renderer/host behavior and deployed-browser journeys, then obtain individual component and page acceptance in sequence.

## Ordered Implementation Scenarios

### Scenario 1 — Revision-consistent reusable collaboration

**Behavior.** Populated components show one selected revision's subjects, cause, origins/start/status, conversation, reviews and participants. Moving between revisions never mixes old subjects with new verification results. Empty states explain the missing registration or activity; denied/error states preserve stable problems; loading, long lists and narrow screens remain usable. Exact Card, revision and verification navigation/copy intents contain the recorded identities. Every owned component validates/renders through the catalog and `WyrdView`.

**RED.** Add the planned [revision component test](#v1-revision-components), using contract-shaped fixtures only after prerequisite approval. Assert identities/content/intents, invalid inputs and stale-load rejection, not component existence. It fails because the shared collaboration family and projections are absent.

**GREEN.** Implement pure components and portable loaders over the activated contract, with gallery view documents and conventional list/detail consumers in this task. Rerun this scenario after each change.

**REFACTOR.** Consolidate only repeated presentation/read shaping within the existing package owners while all scenario assertions remain green; no replacement CR store or lifecycle.

### Scenario 2 — Authorized collaboration and connection actions across hosts

**Behavior.** Node and MCP reads represent the same caller and exact revision. Comment/review/request-verification and connect/configure actions complete through the approved host projection, then refresh authoritative state. Pending submissions are guarded; denied/error/ambiguous failures do not invent a revision, participant, webhook connection or successful review. Another tenant's IDs are never disclosed or writable. Every new component renders from the actual built AppBridge HTML; fixture host evidence remains distinct from live reads/actions.

**RED.** Add the planned [authorized host journey](#v2-authorized-hosts). Produce data using the approved public APIs, exercise real Node/MCP consumers plus action dispatch, and assert exact outcomes/refusals and no cross-tenant content. Record actual selected cases; unavailable projection or skipped setup is a dependency, not GREEN.

**GREEN.** Extend existing consumer adapters with approved reads/actions, retain request-scoped credentials and refresh from confirmed server data. Rerun Scenario 1 and this journey.

**REFACTOR.** Reuse existing SDK/MCP and action-state owners without widening the read executor to perform writes or adding optimistic durable state.

### Scenario 3 — Repository configuration is recorded state

**Behavior.** RepositoryList, WyrdTomlView and VerifierFolderTable expose connection health, current configuration path and configured Verifiers from one server view. Empty/disconnected/error/denied cases remain distinct. Editing the location or connection sends one authorized intent; a failure preserves the recorded configuration and safe draft, and a successful refresh shows server-confirmed state. No webhook secret is rendered.

**RED.** Add the planned [repository component test](#v3-repository-components). Assert all three components' states, configuration intent payloads and failure preservation, with rejected malformed identifiers.

**GREEN.** Build the repository projections and components from generated prerequisite types, and mount them on Repos using the shared loaders. Rerun Scenarios 1–3.

**REFACTOR.** Reuse typed form/action/state patterns; no browser webhook status cache or second configuration owner.

### Scenario 4 — Real browser investigates and updates Changes

**Behavior.** A real browser signs in through public human auth, enters Changes from the rail, filters/pages the list, opens a revision, visits every tab, follows a subject and recorded verification, posts a comment/review and requests verification. Navigate back with revision/tab/list position intact. Include multi-PR/repository data, an empty list, denied reads/writes, upstream error and foreign-tenant IDs. Confirm visible and server-recorded results; failure in one panel leaves other destinations reachable. Changing revision while a read is delayed cannot restore stale content.

**RED.** Add the planned [Changes browser journey](#v4-changes-browser). Reuse current production BFF/Chromium lifecycle and public signup/login support; absent routes/actions must fail in the browser. Imported handlers, simulated cookies and jsdom are supporting proof only.

**GREEN.** Wire the same shared components/loaders into the authenticated routes and host navigation/actions. Reuse current shell and session behavior; rerun Scenarios 1–4.

**REFACTOR.** Keep only route/layout/URL state in pages and share the existing deployed test lifecycle; do not add a runner.

### Scenario 5 — Real browser manages Repos

**Behavior.** A real signed-in browser enters Repos from the rail, inspects connected repositories and webhook status, opens `wyrd.toml`/Verifier folder detail and performs approved configuration/connection writes. Assert successful readback, pending guard and visible stable refusal/error without credentials in HTML or data. Exercise empty and another-tenant repository IDs; no foreign configuration appears.

**RED.** Add the planned [Repos browser journey](#v5-repos-browser), driving the production BFF and real prerequisite service through current browser support.

**GREEN.** Complete Repos mounting and host dispatch; rerun all earlier changed scenarios and this journey.

**REFACTOR.** Reuse existing auth, error and navigation helpers, keeping repository behavior server-owned.

### Scenario 6 — Normal development startup demonstrates both workflows

**Behavior.** A fresh `mise run dev:full` run provisions representative data through the real public development setup: at least one connected repository/configuration/Verifier folder and one multi-revision Change Request with subjects, recorded verification, comment/review, audit and changed-file detail supported by the delivered contract. Sign in normally, inspect both routes and perform one safe configured action with authoritative readback. Use a second identity/tenant for negative journeys; gallery supplies deterministic loading/error cases. Missing seed or setup failure is reported as failed readiness rather than a successful empty demo.

**RED.** Extend the existing launcher/browser support with the planned [development journey](#v6-development-workflow). It must fail when normal startup has no representative Changes/Repos content; a second private test-only application/server does not satisfy this check.

**GREEN.** Extend existing setup once through supported APIs/SDKs, preserving fresh-database behavior, canonical workload, production wheel, MockServer and failure diagnostics. Document sample identities/routes; rerun earlier affected scenarios and this check.

**REFACTOR.** Remove duplicate setup within the existing dev workflow; do not add a new launcher, seeding API or production fixture switch.

## Acceptance Criteria

| Obligations | Observable acceptance and proof |
|---|---|
| REQ-029; INV-001/004/007 | All twelve components and all Changes/Repos regions consume the activated server contract. Revision/multi-repository consistency, exact links and actual writes pass V1–V5. Fixture-only state or an unavailable action cannot complete the task. |
| REQ-030; AC-011; INV-002/005 | Per-component T1 states, T2 requests/shaping/cancellation, T3 catalog/view, T4 built AppBridge and T5 real authorized Node/MCP data/actions are recorded by V1–V3. No credentials or foreign content enter client output. |
| REQ-021/034; AC-013 | Both rail destinations and shareable routes pass V4/V5 through real browser auth, navigation and supported interactions; denial/error/tenant cases retain usable shell and stable problems. |
| REQ-062; AC-041 | V6 and the human checkpoint show both new workflows using normal `dev:full` setup, with documented representative data and no hidden manual seed step. |
| REQ-020/022; AC-010; INV-003/006 | Exact board/component references, screenshot/source/theme/viewport, interaction/accessibility evidence and separate human acceptance exist for every component and page checkpoint. No copied legacy domain implementation or invented verdict/state. |

## Implementation Reference

Paths guide owner discovery rather than prescribe private module structure. Revalidate current owners at activation; delivered platform contracts may add their own narrower instructions.

| Existing owner and inspected consumer/test | Missing behavior and selected extension |
|---|---|
| `packages/ui/src/lib/contract.ts::Executor`, `ActionIntent`, `ActionState`; `loaders.ts::loadPanel/validateIntent`; `node.ts::nodeExecutor`; `mcp.ts::mcpExecutor`. Current `ui-library.integration.test.ts::appBridgeView/mcpOver` exercise built HTML and real host reads. | CR/repository operations are absent from the current closed read union. Add only projections of separately approved shipped operations and catalog intents, closing Node, MCP, built resource and custom-app consumers together. No new transport/authorization owner. |
| `src/routes/t/[tenantKey]/observe/+page.server.ts::load/actions` and `src/routes/t/[tenantKey]/ui/load/+server.ts::POST`; `BrowserSession.client` supplies the viewer client. `ui-pages.integration.test.ts::startBff/humanSignIn` prove the deployed host. | Add authenticated Changes/Repos route consumers and server action adapters. Reuse session-bound load validation and the existing production-browser lifecycle; page files perform no domain queries or joins. |
| `src/routes/t/[tenantKey]/+layout.svelte::groups` currently disables Changes and has no Repos entry. | Enable the actual Changes route and add Repos in the existing rail, with current tenant prefix, active state, keyboard and mobile behavior. |
| `mise.toml::dev:full`, `scripts/dev/example-app.sh::setup`, `ui-pages.integration.test.ts::devFull`. | Extend the same public development setup and launcher proof with representative delivered CR/repository data; current startup does not establish that coverage. Preserve startup cleanup and diagnosis behavior. |
| Current REQ-029 explicitly records no CR server contract on main. The approved historical `features/changes/Overview.svelte`, `Verification.svelte`, `ChangesJourney.test.ts` and `review-actions.test.ts` at `7bfc3a8a` are interaction evidence only. | Activation requires concrete platform owner/symbols, generated inputs/outputs, required permissions, revision/action semantics and test projections. Record them before live work. Never port historical `server/mock/changes/store.ts` or infer its payloads as authority. |

Expected consumer closure: package components/loaders/catalog/intents/fixtures/gallery and exports, Node/MCP/BFF action adapters, tenant routes/rail, generated contract consumers, runtime built App resource, current dev setup and focused tests. No platform CR subsystem is part of this write set.

## Verification and Evidence

All tests below are **planned additions** to existing files/runners, not claimed existing selectors. Confirm the actual title/selection after adding each test and before RED. UI `package.json::test` builds the package then runs Vitest; `vite.config.ts` already includes these files and gates integration with `WYRD_UI_INTEGRATION`. Existing integration owners are `ui-library.integration.test.ts::appBridgeView/mcpOver` and `ui-pages.integration.test.ts::serverTest/startBff/humanSignIn/devFull`; reuse them. Install current workspace dependencies, use `mise run ts:build`, `mise run ts:build:testing`, `mise run build:server`, and ensure the existing Playwright Chromium installation before real-server checks. Current browser fixture builds/starts the production BFF. Setup details are in [verification owners](../references/verification.md); no replacement selector/configuration is authorized.

### V1 Revision components

T1–T3: exact revision props/intents/states, input validation and loader sequencing.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^revision-consistent collaboration components$'
```

### V2 Authorized hosts

T4 built AppBridge plus T5 genuine Node/MCP data and caller-authorized actions for all twelve components, including cross-tenant and write failures. Requires activated platform contracts.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^authorized collaboration and repository hosts$"'
```

### V3 Repository components

T1–T3 repository state, configuration intents and secret exclusion.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^repository configuration components preserve recorded state$'
```

### V4 Changes browser

T6 public-auth Changes list/revision/tabs/actions and negative flows against production BFF/server.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated change revision collaboration$"'
```

### V5 Repos browser

T6 repository connection/configuration journey and read/write refusal.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated repository configuration$"'
```

### V6 Development workflow

Reuse the existing `devFull` process/browser lifecycle with isolated local config; launcher owns its database and services. Record normal startup setup completion and seeded content, not merely a responding login page.

```bash
mise exec -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t '^development changes and repository workflow$'
```

After focused checks, run UI `check` and `build`; run code generation drift proof when catalog/generated inputs change:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run codegen:check
```

V6 owns its fresh development database/services through the launcher and therefore does not use the test-Postgres wrapper. Record actual running selections; no skipped/zero-selected test passes. Changes to Rust or SDK contract owners require their own focused lanes in the separately approved prerequisite, not invented coverage here.

### Human visual checkpoint

Run `mise run dev:full`; sign in at the printed tenant login, open `/_gallery`, then Changes and Repos. Inspect sequentially: every shared component; Changes list; Overview; Verification; Audit; Files changed; Repos. Each has its own populated/empty/denied/error/loading, desktop/mobile, light/dark, keyboard/focus, touch, long-content and overflow evidence. Fixture states supplement real routes, never replace them. Record reference region, implementation screenshot, exact revision, source identity, theme and viewport; differences need a fix or explicit human exception. Wait for individual human acceptance before the next dependent slice; combined task acceptance does not waive a component/page checkpoint.

## Material Stop Conditions

Do not activate contract-dependent implementation until the approved platform packet fixes exact request/response types, permissions, revision/write/failure semantics, generated SDK/MCP projections and their real tests. Report the missing named projection and finish independent work; no guessed payload, fixture success or disabled control closes it. Stop for changed public/persisted contracts, tenant/credential boundaries, new durable owners, unsupported development seeding actions. Ordinary private component organization and fixture layout are implementation-owned. Missing live/platform/human proof remains unverified.

## Authority Links

[Approved spec](../spec.md), [plan and ownership map](../plan.md), [approved canvases](#approved-canvases), [visual/source audit](../references/task-audit-2026-10-29.md), [AGENTS](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [security](../../../../architecture/wyrd-security-posture.md), [execution](../../../../changes/implementation-execution.md), [spec-driven development](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md), and the repository `wyrd-ui` skill govern this task. Current generated contracts win over historical layouts.
