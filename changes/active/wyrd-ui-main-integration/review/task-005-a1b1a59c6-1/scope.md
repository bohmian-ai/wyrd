# Task 005 review subject and coverage

First review; no prior Task 005 verdict or acceptance evidence located.
Root: `/home/thorrester/Documents/GitHub/wyrd`.
Base: `4f6bc10d4967b1cd19b5c2b314d9fd9d0b42d2d6` (parent of first task implementation commit).
Candidate: `a1b1a59c6f28b63d08973e2eccce168bbe80bd92`.
Read range using `git diff <base>...<candidate>`; the full 46-file range is in scope, including Observe regressions and trace streaming.
Authority: `changes/active/wyrd-ui-main-integration/spec.md`, approved revision 28; original task `tasks/TASK-005-cards-and-prompts.md`. These packet files are currently uncommitted; SHA-256 identities are recorded in verification.md. Do not alter them or implementation source. Existing untracked `.g.mjs` and `zz-shots.integration.test.ts` are not candidate source and must not be deleted by review.

## Shared navigation map

- Prompt contract producer: `crates/wyrd-spec/src/card/prompt/{mod,spec}.rs`, `envelope.rs`; server projection: `crates/wyrd/wyrd-server/src/components/cards/service.rs`; consumer generated SDK `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts`.
- Portable reads and action contracts: UI `packages/ui/src/lib/{cards,loaders,contract,schemas,node,mcp}.ts`; existing loader/executor owners remain comparison baseline. Shared presentation: components CardInventory/CardHeader/CardFacts/RelationshipList/ArtifactTable/PromptMessages/PromptVariables/UsedBy/VersionDiff, Panel/StatusPill/format, WyrdView/catalog/index.
- Hosts: `src/lib/cards-host.ts`, `src/routes/t/[tenantKey]/cards/{+page.server.ts,+page.svelte,detail/+page.server.ts,detail/+page.svelte}`, tenant layout; gallery and ui-fixtures.
- Regression surface: Observe layout, page server load, page Svelte; promise streaming, navigation supersession, refusal and direct SSR.
- Proof owners: `src/lib/ui-library.test.ts`, `ui-library.integration.test.ts`, `ui-pages.integration.test.ts`; development setup `scripts/dev/example-app.sh` and fixtures/cards/ui_cards.
- Rules: root AGENTS.md; SDK nested AGENTS.md; maintainer guide; architecture owning Card/Prompt/security sections; UI skill and brand DESIGN.md; TESTING.md and task verification setup. CodeGraph exists and must be consulted first for code discovery.

## Specialist applicability

| Contract / invariant | Producer → consumer / existing owner | Selection and rationale |
|---|---|---|
| Full task acceptance, exact versions, variables, files, view portability, accessibility and visual fidelity | Server contracts → portable loaders → shared components → conventional/gallery/AppBridge hosts | behavior-rev comprehensive discovery, including loader supersession. No separate invariant reviewer: local request selection and Svelte navigation reuse existing owners, no new materially complex shared lifecycle owner. |
| Cross-tier contract parity and mandatory repository gates | Rust spec/server → generated TS → UI → dev seed shell | repo-rev required: Rust, generated declarations, Svelte/TS, shell/fixtures and architecture all changed. |
| New loader/component ownership and reuse | Existing loadPanel/executors/CardHeader → new cards.ts and shared components | maintainer-rev required for substantial new package surfaces and request shaping. |
| Authorization, tenant separation and sensitive Prompt/files exposure | BrowserSession/Node/MCP executor → Card and file reads → display/action hosts | domain-rev-security required; include unchanged authorization dependencies. No persistent-data specialist: projection/registered reads and public registration seed reuse existing storage, no changed writes/transactions/schema. |
| Failure propagation, streamed promises, direct SSR and interrupted navigation | Observe server load → SvelteKit serialization → drawer await blocks | system-rev required, scoped to streaming and changed load failure paths plus new Card prerequisite handling. |

All reviewers receive this map as a starting point and must expand against source. No implementation summary or intended verdict is supplied. Independent validation follows discovery and checks specialist selection too.

## Visual inspection limitation

Computer-use inventory returned no apps or browsers. `cua.createBrowserTab('iab', 'http://localhost:3000', {visible:true})` failed with `Browser is not available: iab`. No visual acceptance is inferred from tests or implementation claims. Continue static review and available narrow checks; visual sign-off remains unavailable unless the browser surface becomes available.
