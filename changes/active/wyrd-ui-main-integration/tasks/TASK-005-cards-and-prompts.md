# TASK-005 — Cards and Prompt definitions

Spec: [revision 28, approved](../spec.md). Status: Proposed; delivers the approved Cards and Prompt UI outcomes below.
Dependencies: completed TASK-003 foundation, TASK-004 live shell/Observe and TASK-027 view layout. No full-library or production-MCP gate precedes this task.
Mapped obligations: REQ-008/009/011/020/022/024/030/034/062; AC-006/007/010/011/013/041; INV-001–INV-007 where applicable.
Consolidates former TASK-013 and TASK-015, plus former TASK-005's inventory, generic facts/relationships, Prompt components and ArtifactTable. CompositionGraph/ModelInterface transfer to TASK-008; CardInventorySummary transfers to TASK-011. Former task numbers in this sentence identify provenance, not dependencies.

## Outcome and Value

A developer runs `mise run dev:full`, signs in normally, opens Cards, searches the registry and opens an exact version. Generic detail shows its facts, relationships, a Files tab and YAML. A Prompt additionally shows authored messages, derived variables, response format, using Agents and a comparison with another exact version. The same components are available through the package, gallery and view-document renderer for custom apps and MCP Apps.

For example, opening Prompt version 1.0.0 must continue to show that version after following its relationships or comparing it with 1.1.0; a newer registration never silently replaces the selected Card.

## What Changes and What Must Stay True

Own `CardInventory`, `CardFacts`, `RelationshipList`, `PromptMessages`, `PromptVariables`, `UsedBy`, `VersionDiff` and `ArtifactTable`, with portable loaders, catalog/schema entries, fixture view documents, package exports and applicable host actions. Reuse `CardHeader`, `CardRefChip`, existing state components and the existing renderer/executors. These are shared package components, not app-private copies.

Enable Cards in the existing tenant rail. The conventional entrypoints are `/t/[tenantKey]/cards` and `/t/[tenantKey]/cards/detail?kind=...&name=...&version=...&uid=...`; `uid` is optional and all query values are encoded independently. The detail route requires the exact kind, name and version. Missing or invalid identity is a stable validation error; an absent Card remains not found, never a latest-version lookup. Version selection changes the complete reference and clears version-specific content before loading the new version. Preserve inventory filters/cursor on Back and preserve the selected reference when switching detail tabs. Later kind workspaces extend this same route.

Inventory includes search, current supported filters, server cursors, labels/annotations and server-owned lifecycle status across the 12 registrable kinds in the architecture kind catalog. External is not registrable. Policy and Source are removed; this task does not reopen that decision or add a separate kind-reconciliation prerequisite.

Generic detail supports all registrable kinds without inventing kind-specific content. Display returned facts and server-derived relationships, copy the exact UID/reference, select a registered version and render the raw selected spec as YAML. Every Card page has a Files tab using ArtifactTable to list the files already recorded during registration for that exact version. Show relative path, size, digest and content type from the existing authorized file-listing operation. A Card with no files shows an empty Files tab; a refused or failed read shows its actual problem. Preserve this tab when later tasks add kind-specific layouts, including Model. Reuse the existing registered-file records and authorized reads.

Prompt handling follows the current authored portable/native wire forms. Preserve native extension fields; messages/media, provider/model and response format reflect that selected form. Variables are derived under the existing Prompt contract rather than redefined by a browser parser. `UsedBy` consumes server relationships. VersionDiff compares any two registered versions of the same Prompt; a single registered version has an explanatory comparison empty state. Long and multimodal content remain readable. Prompt has no telemetry strip.

Components receive typed props and emit validated JSON-safe action intents. Portable loaders own request planning and shaping; Node/MCP executors own authorized IO. Conventional pages own URL/tab/layout state and host completion only. Keep credentials server-side and retain stable error code/title, tenant separation and server audit behavior. A failed prerequisite issues no dependent reads. Superseded loads cannot overwrite the newer selection. Copy/navigation/artifact actions report pending/success/denied/error or a truthful unsupported capability; an unavailable control cannot satisfy a required production action.

The existing Prompt compiler extracts and validates variable names during Card
decoding, including registration. PromptVariables displays those existing derived
values through the existing UI data-loading boundaries. Connect the approved Prompt content to
the shared components and their conventional/custom-app/MCP hosts as part of this
task's UI implementation.

## Approved Canvases

These approved canvases define the visual target for this task. Follow their layout, component placement, hierarchy and interactions while using current Wyrd contracts and brand tokens. Apply the same component design in the gallery and the working UI. Record a separate comparison and human acceptance for each named canvas on desktop/mobile in light/dark.

- [Cards.dc.html](../references/canvas/Cards.dc.html)
- [Prompt.dc.html](../references/canvas/Prompt.dc.html)

If a required visual detail is absent or contradictory in these approved canvases, stop that affected visual work and ask the maintainer for clarification. Do not guess a layout, invent a replacement design, or treat a generic directory link as a visual specification.

## Approach

1. Confirm current Card/Prompt types and reuse the existing exact-read/version and registered-file listing operations.
2. Add each shared component, portable loader, catalog/view entry and gallery states together with its first live route region.
3. Compose inventory, generic detail and Prompt detail in the authenticated shell, completing typed action handling through existing capabilities.
4. Extend existing normal development setup with representative registered Cards/versions/artifacts where the canonical app does not already provide them.
5. Prove component/host parity and the real-browser journey, then obtain the separate inventory, detail and Prompt visual checkpoints.

## Ordered Implementation Scenarios

### Scenario 1 — Registry discovery preserves filters and exact identity

**Behavior.** Cards navigation opens the live registry. Search/filter changes start a fresh cursor chain; Next uses the server cursor without duplicating or dropping rows. Selecting a summary opens its exact reference. Empty results explain registration/filter context; denied and failed listing stay distinct from empty. Labels, annotations, status and kind choices use the authoritative contract.

**RED.** Add the planned [inventory component proof](#inventory-component-proof). Render every state, exercise search/filter/cursor intents, assert exact list requests and navigation identity, and validate/render its fixture document. The missing inventory behavior must fail; extend current renderer tests rather than test a replacement selector framework.

**GREEN.** Build the shared inventory projection and route consumer over current Card reads. Pass this proof and retain existing CardHeader behavior.

**REFACTOR.** Simplify within the shared loader and established route host; do not add a registry cache, traversal owner or transport.

### Scenario 2 — Generic detail retains one selected Card and its files

**Behavior.** Opening or changing an exact version loads facts, YAML, relationships and the Files tab for that version only. Relationship navigation retains its returned CardRef. Copy reports actual host completion. Artifact actions either complete through their supported authorized host capability or expose its real stable refusal. A denied/missing Card does not trigger dependent inventory reads; late old-version reads cannot replace the selected version. Missing artifacts are an empty inventory, not a missing Card.

**RED.** Add the planned [detail component proof](#detail-component-proof), including a denied prerequisite, version supersession, artifact metadata and action outcomes. The missing shared detail behavior must fail. Exercise the existing authorized registered-file inventory with populated, empty, denied and failed reads; assert that every kind retains the Files tab.

**GREEN.** Implement the shared facts/relationships/artifact projections and mount them in the exact detail route. Rerun scenario 1 and this proof; preserve the exact selected identity in every request and intent.

**REFACTOR.** Reuse CardHeader's existing version workflow and the current action-state contract. Do not add an alternate artifact store, lineage graph or handwritten domain envelope.

### Scenario 3 — Prompt definitions and differences reflect authored forms

**Behavior.** The selected Prompt renders portable and native authored forms, derived variables, response format and using Agents. Comparing two versions keeps both pins explicit and renders their actual differences; one version offers the comparison empty state. No telemetry is inferred. Long/media content and provider-native extension fields remain inspectable without changing the authored body.

**RED.** Add the planned [Prompt component proof](#prompt-component-proof), checking both authored forms, derived fields, comparison identity, single-version behavior and long content across gallery states. The absent Prompt projections must fail.

**GREEN.** Implement the shared Prompt projections and mount them in the existing exact detail owner. Rerun scenarios 1–3 and all affected CardHeader regressions.

**REFACTOR.** Use generated wire discriminants and existing Prompt contract semantics; leave private fixture and module organization to the implementer.

### Scenario 4 — Authorized components and working development UI agree

**Behavior.** The shared components load real registered data through Node and MCP and render in the actual built AppBridge artifact. An underprivileged viewer receives denial and another tenant's Cards/files/usage never appear. A real browser signs in through public auth, browses, changes versions, follows a relationship, inspects YAML/artifacts and compares Prompts. A normal `dev:full` launch provides representative data for that same workflow, with no gallery fixture fallback.

**RED.** Add the planned [authorized host proof](#authorized-host-proof) and [authenticated browser proof](#authenticated-browser-proof). Extend the existing [development lifecycle proof](#development-lifecycle-proof) with visible Card/Prompt and seed-readiness assertions. Real server/browser setup must expose the missing workflow; imported page handlers or simulated cookies cannot supply GREEN.

**GREEN.** Close shared host adapters, route consumers and development seeding together. Rerun scenarios 1–4. Include the registered Files tab and existing Prompt variable display in that same working UI checkpoint.

**REFACTOR.** Reuse the existing real-server, production-BFF, browser and development-launch lifecycle; do not create another harness or data setup path.

## Acceptance Criteria

| Obligation | Observation and proof | Violation |
|---|---|---|
| REQ-008; AC-006 | Scenario 1 and real browser show correct filtered/cursor inventory, all supported kinds and truthful states. | Client-invented kind/status, skipped cursor page or empty-on-denial. |
| REQ-009/022/024; AC-006/007 | Scenario 2 and authorized hosts retain exact identity across facts/YAML/relationships/files/version changes and real action outcomes. | Latest substitution, envelope-as-file-inventory, stale overwrite or fake action success. |
| REQ-011/024; AC-007 | Scenario 3 and browser render both Prompt forms, derived fields, usage and any-two-version diff; one-version empty state. | Reauthored native body, independent variable semantics or Prompt telemetry. |
| REQ-030; AC-011; applicable invariants | Every owned component has T1 states, T2 requests/shaping, T3 view validation/render, T4 built AppBridge evidence and applicable T5 authorized live proof; existing custom-app imports remain valid. | App-only component, browser credential import, missing catalog entry or fixture-only production claim. |
| REQ-034/062; AC-013/041 | Production-BFF T6 and normal `dev:full` expose the enabled Cards entry, seeded inventory/detail/Prompt workflow and explicit empty/denied/error cases. | Page-only screenshots, hidden manual seed step, or live route showing gallery fixtures. |
| REQ-020; AC-010 | Individual component gallery acceptance plus separate inventory, generic-detail and Prompt desktop/mobile light/dark inspection, keyboard/focus/touch/overflow proof. | One family-level approval substituted for a component/page checkpoint. |

## Implementation Reference

All UI-relative paths below are rooted at `crates/wyrd/wyrd-server/wyrd-ui/`; paths guide ownership, not a file allowlist.

| Existing owner and inspected consumers | Gap and selected extension |
|---|---|
| `packages/ui/src/lib/loaders.ts::loadCardHeader`, `validatePanel`, `loadPanel`; `node.ts::nodeExecutor`, `mcp.ts::mcpExecutor`; `contract.ts::WireRequest/Executor/Loaded/ActionState` | Exact Card reads/list/version paging already exist. Extend these UI owners for Card/Prompt rendering and the existing registered-file read; no duplicate loader/executor lifecycle. |
| `crates/wyrd/wyrd-server/src/cards/service.rs::insert_artifact_manifest_rows/list_card_artifacts/card_files`; existing `GET /v1/cards/{card_uid}/artifacts`; `src/lib/ui-library.integration.test.ts` | Registration already records the file manifest against the Card UID. Reuse its authorized listing for the shared Files tab; no new registration, storage or client-library feature. |
| `crates/wyrd-spec/src/card/prompt/spec.rs::PromptSpec/parameters/compile_prompt`; architecture Prompt authored forms | The existing compiler owns derived variable semantics. Display those values with the selected Prompt and preserve authored messages/response format. |
| `packages/ui/src/lib/schemas.ts::CardRefSchema`, component catalog; `scripts/catalog.ts`; `src/lib/ui-library.test.ts`, `ui-fixtures.ts` and `src/routes/_gallery` | Add catalog/schema/export/view/fixture closure for each new component. Use the settled current kind catalog; Policy and Source are excluded. |
| `src/routes/t/[tenantKey]/+layout.svelte`, `observe/+page.server.ts`; `src/lib/server/auth/browser-sessions.ts::BrowserSession.client`; `ui/load/+server.ts::POST` | Reuse tenant rail, session-bound shared loader and existing BFF validation for new Cards consumers; page code owns layout/navigation only. |
| `src/lib/ui-pages.integration.test.ts::startBff/signIn/humanSignIn/devFull`; `scripts/dev/example-app.sh::setup`; `mise.toml::dev:full` | Real browser and actual development launch already exist. Add assertions and representative setup through the existing owner instead of another launch mode. |

Likely changes include shared components/loaders/schema/catalog sources, generated roots only where missing, normal dev setup and its data, authenticated Cards routes/rail/action adapters, gallery and focused tests. Source-backed historical reads/boards are evidence only; no deleted UI components, layouts or mocks return.

## Verification and Evidence

The following new Vitest titles are **planned**. Confirm each actual selector after adding it and before RED; selected/skipped counts must show real execution. Package, runner and setup are current: UI `package.json::test` builds the package/artifact then runs Vitest; `vite.config.ts` admits these files and gates integration with existing `WYRD_UI_INTEGRATION`.

Before live proofs, use the repository-owned test-server build prerequisites in [verification setup](../references/verification.md), install the existing UI workspace dependencies and the current Playwright Chromium runtime. Reuse `serverTest`, built BFF and public login; do not invent an environment/selector API. Build Cargo prerequisites sequentially in the shared checkout.

### Inventory component proof

T1–T3 and exact discovery/intent assertions:
```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^card inventory preserves filters cursors and exact references$'
```

### Detail component proof

T1–T3, prerequisite/supersession and supported artifact action assertions:
```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^exact card detail preserves relationships files and action outcomes$'
```

### Prompt component proof

T1–T3 authored-form/diff/state assertions:
```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^prompt definitions and comparisons preserve authored forms$'
```

### Authorized host proof

T4 actual built AppBridge HTML plus T5 real authorized Node/MCP reads/actions; populated/empty/denied, another tenant and fixture provenance:
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^card and prompt components use authorized host reads and actions$"'
```

### Authenticated browser proof

T6 actual login, rail navigation, inventory/detail/Prompt interactions and visible outcomes against production BFF/server, including denied/error and cross-tenant absence:
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated card browsing and prompt comparison$"'
```

### Development lifecycle proof

Extend this **existing** exact selector in its existing actual-launch owner; add Card/Prompt readiness and interaction assertions while retaining service cleanup/failure checks:
```bash
mise exec -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t '^development environment lifecycle$'
```

This launches its own development Postgres/services; use free normal development ports and its isolated config home. It is distinct from managed `WyrdTestServer` T6. Verify existing launcher prerequisites rather than wrapping it in another server lifecycle.

### Scoped checks and human checkpoints

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run codegen:check
mise run dev:full
```

Codegen is required when catalog/generated inputs change, as expected here. Apply the existing owning format/lint checks to changed source; keep verification scoped to the UI and its existing data consumers. Do not run broad aggregate journeys here; integrated change review owns those.

For manual validation, use the launcher's printed login and tenant, open Cards, filter/page, select a registered exact Card, switch version, inspect relationships/Files/YAML, then open a Prompt and compare versions. The setup must supply at least two registered Prompt versions, a consuming Agent, another generic Card and a completed registered file inventory, using existing public registration/storage operations rather than protected-table writes. Document their exact identities in implementation evidence. A fresh database means the task cannot rely on data from a prior run. Legitimate absence states remain separately inspectable.

Inspect every owned component at `/_gallery` and its first live region. Record board region, source screenshot, implementation screenshot, state/theme/viewport and individual human acceptance. Check Cards inventory, generic header/version/facts/relationships/Files/YAML and the Prompt board separately. Follow the approved Cards.dc.html and Prompt.dc.html linked above; their design approval is settled. Human inspection here accepts implementation fidelity, not a new design proposal. Preserve long/mobile/light/dark/keyboard/focus/touch coverage. No next dependent slice begins before its required human checkpoint is accepted.

## Material Stop Conditions

Escalate only a demonstrated conflict with an approved public/security/durable contract. File listing, existing Prompt-variable display and exclusion of Policy/Source are part of this UI assignment, not separately approved prerequisite projects. Do not infer a backend payload from a board, add a compatibility kind, promote fixture evidence to live proof, or claim human/cross-platform acceptance that did not occur. Private module layout, fixture organization and equivalent exact-selector corrections remain implementation choices.

## Authority Links

- [Approved spec](../spec.md), [plan and ownership map](../plan.md), [verification owners](../references/verification.md), [approved canvases](#approved-canvases).
- [Repository rules](../../../../AGENTS.md), [architecture kind catalog and Prompt contract](../../../../architecture/wyrd-design.md), [security posture](../../../../architecture/wyrd-security-posture.md).
- [Execution](../../../../changes/implementation-execution.md), [spec-driven development](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md), [Wyrd UI skill](../../../../.agents/skills/wyrd-ui/SKILL.md).

## Implementation Evidence

Commits `49231a552`…`49dd619b1` on `task/wyrd-ui-main-integration/TASK-005`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Shared CardInventory, CardFacts, RelationshipList, PromptMessages, PromptVariables, UsedBy, VersionDiff, ArtifactTable with loaders, catalog/schema, exports, fixtures, gallery, WyrdView (REQ-030; AC-011) | `packages/ui/src/lib/cards.ts`, `schemas.ts`, `components/*.svelte`, `index.ts`, `WyrdView.svelte`, regenerated `catalog.json`; `src/lib/ui-fixtures.ts`; `src/routes/_gallery/+page.svelte` | `ui-library.test.ts` (18 passed); `mise run codegen:check` | PASS |
| Inventory search/filters/cursors/status across the 12 registrable kinds; Policy/Source/External excluded; Back restores filters (Scenario 1) | `loadCardInventory`, `inventoryFilter`, `CARD_KINDS`, `CardRefSchema`; `src/lib/cards-host.ts`; `routes/t/[tenantKey]/cards` | `^card inventory preserves filters cursors and exact references$`; T6 `^authenticated card browsing and prompt comparison$` | PASS |
| Exact detail: invalid identity is validation, absent is not found, never latest; version change replaces the whole reference and clears content; tabs keep the reference; Files and YAML for every kind (REQ-009/022/024; AC-006/007) | `routes/t/[tenantKey]/cards/detail/+page.server.ts`, `+page.svelte` (`switching`), `loadArtifactTable`, `CardFacts` YAML | `^exact card detail preserves relationships files and action outcomes$`; T6 asserts `WYRD_SPEC_400_VALIDATION` for missing/`latest` version, `WYRD_REGISTRY_404_CARD_NOT_FOUND` for absent and other-tenant Cards | PASS |
| Prompt: portable/native forms with extension fields, server-derived variables (`status.prompt`), response format, using Agents, VersionDiff of any two versions, single-version empty state, no telemetry | `status.prompt` projection (`crates/wyrd-spec/src/card/prompt/spec.rs`, `cards/service.rs`); `loadPromptMessages`, `loadPromptVariables`, `loadUsedBy`, `loadVersionDiff`; Open in Observe unavailable for a Prompt | `^prompt definitions and comparisons preserve authored forms$`; T5 asserts `future_knob`, `ticket`/`tone`, `reply-agent` route, diff fields, `Only version 1.0.0 is registered.` | PASS |
| T4 built AppBridge + T5 Node/MCP live reads, denied principal with no dependent reads, another tenant, BFF session | `ui-library.integration.test.ts` | `^card and prompt components use authorized host reads and actions$` passed; full file 5/5 | PASS |
| T6 production BFF: rail, filter, Back, open exact version, version switch, tabs, relationships, files, denied | `ui-pages.integration.test.ts` | `^authenticated card browsing and prompt comparison$` passed | PASS |
| `dev:full` seeds through public registration and exposes the Cards workflow (REQ-034/062; AC-013/041) | `scripts/dev/example-app.sh setup` runs `wyrd declare` on `fixtures/cards/declare_and_pull/support-model.yaml`, `ui_cards/support-reply-1.0.0.yaml`, `ui_cards/support-reply-1.1.0.yaml`, `ui_cards/reply-agent.yaml` | `^development environment lifecycle$` passed with Cards assertions | PASS |
| Scoped checks | — | `pnpm check` 0 errors; `pnpm build`; `mise run codegen:check`; `git diff --check` | PASS |

Seeded identities (UIDs are new per fresh database): Model `support-model@1.0.0` with file `support-model.bin`; Prompt `support-reply@1.0.0` and `@1.1.0` (label `team=support`; 1.1.0 adds `{{tone}}` and `response_format: verdict`); Agent `reply-agent@1.0.0`, native route, using `support-reply@1.1.0`.

Limitations and notes:

- The web UI's Files tab reads `GET /v1/cards/{card_uid}/artifacts` through the BFF. Only an MCP App host lacks it: the MCP server has no file-listing tool, so `ArtifactTable` embedded there shows the host's real refusal. Adding a read-only `cards.list_artifacts` MCP tool is a low-priority follow-up, accepted by the maintainer as outside this task.
- The canvas's per-variable "used in" column is not shown: the server derives names only, and parsing the Prompt in the browser is prohibited.
- Human visual checkpoints (Cards.dc.html, Prompt.dc.html; desktop/mobile, light/dark) have not been accepted yet; they need the maintainer.
- Out-of-scope drift fixed along the way: TASK-004 journey expectations updated for keyset paging, the sorted Start column and span key columns; the Observe grouping select is capped at 7em again (regression from `d30665ba5`, diagnosed by an independent read-only pass). `observe boards match approved canvas` still fails on its Metrics legend row count, because Metrics now aggregates across series by default (`6c1253daa`); that board belongs to TASK-004 and is left for its owner.
- Diagnosis of the first live `status.prompt` failure: the test server binary predated `49231a552`; `mise run build:server` resolved it.
