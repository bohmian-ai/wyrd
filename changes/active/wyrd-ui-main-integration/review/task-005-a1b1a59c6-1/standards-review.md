# Repository standards review

Result: **PASS** for the source standards scope below. This is not task acceptance or a claim that every required execution/visual gate has run.

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `4f6bc10d4967b1cd19b5c2b314d9fd9d0b42d2d6`.
- Candidate: `a1b1a59c6f28b63d08973e2eccce168bbe80bd92`; HEAD still matched it at final source inspection.
- First review of the full 46-file cumulative diff, using `git diff <base>...<candidate>`. No prior standards acceptance was carried forward.
- Approved authority: revision 28 of `changes/active/wyrd-ui-main-integration/spec.md`, original `tasks/TASK-005-cards-and-prompts.md`, and this round's `scope.md`. Packet identities and available executed checks are recorded in `verification.md`.
- CodeGraph was consulted before code discovery and again for `hydrate_card`, its callers, artifact reads and tenant connections. Actual diff and surrounding source, consumers, test assertions, package/generator configuration and applicable authorities were inspected. Only this report was written.

## Authority and changed-surface coverage

UI-relative paths below are under `crates/wyrd/wyrd-server/wyrd-ui/`.

| Changed surface | Applicable authorities | Source evidence and assessment | Result |
|---|---|---|---|
| `architecture/wyrd-design.md`, Prompt authored forms/read projection | Root `AGENTS.md` Authority/Cards/Client-server; `architecture/README.md`; approved revision 28 clarification; design Prompt section | The amendment describes derived `status.prompt`, preserves both authored forms, and adds no durable variable semantics. It does not introduce delivery history into normative architecture. | PASS |
| `crates/wyrd-spec/src/card/prompt/{mod,spec}.rs`, `src/envelope.rs` | Root Code/Dependencies/Errors/rustdoc; `agent_docs/{index,rust,maintainer-style,agent-surfaces}.md`; owning design Prompt/Card status sections | `PromptSpec::status` projects existing compiled values; `PromptStatus` and both fields have substantive rustdoc; `Status::prompt` explains optional read-side meaning. No IO/async/dependency/feature or error-catalog change. The changed authored-round-trip test already documents its purpose and panic conditions, including variable assertions. | PASS |
| `crates/wyrd/wyrd-server/src/components/cards/service.rs` | Root server, SQL-capability, tenancy, time and rustdoc rules; Rust/maintainer guides; security principles and Card-file boundary | `hydrate_card` retains its typed error, existing integrity checks and tenant-qualified blob read, then projects the compiler result. Its changed rustdoc describes the new state. The modified immutable-blob test retains documented setup/panic behavior. Actual surrounding reads use `registry_tenant_conn(caller.data_tenant_id)` and committed tenant transactions; no added `PgPool`, SQL query, coordination timestamp or privileged capability. | PASS |
| `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts` | Root generated-artifact/client rules; nested `sdks/wyrd-sdk-ts/AGENTS.md`; agent-surfaces generated-output rules | Added `PromptStatus` and optional `Status.prompt` mirror Rust names/nullability/documentation and use readonly generated fields. Existing generator header/owner remains; no native bridge, transport, runtime API or durable behavior added. Round `codegen:check` passes. There are no changed Python files or PyO3 items. | PASS |
| `packages/ui/src/lib/{cards,loaders,contract,schemas,format}.ts` | Root current-contract/client rules; maintainer style; UI skill Product Boundary/Frontend Rules; design kind catalog/Prompt contract; approved REQ-030 and INV-001/002/004 | Wire imports are type-only except in the executor owner; domain response/identity types come from generated SDK contracts. Twelve selectable registrable kinds are explicit, with no Policy/Source/External registration path. Status and relationships come from returned server values; variables come from `status.prompt`. Existing loader/validation registration remains the package boundary. New requests, pages, versions and Agent reads have explicit bounds. Formatting is presentation-only. | PASS |
| `packages/ui/src/lib/{node,mcp}.ts` | Root SDK/agent-first rules; security credential/tenant principles; UI skill server-client boundary; REQ-030 executor split | Node extends the existing `Cards` owner with `listArtifacts`; MCP extends the existing tool mapping and carries actual refusals. No second transport, credential cache, browser-native SDK import or authorization policy added. The intentionally unavailable MCP file tool is a behavior/acceptance issue for the other assigned reviewers, not hidden success in this source. | PASS |
| `packages/ui/src/lib/{WyrdView.svelte,index.ts,catalog.json}` | Root generated-artifact rules; agent-surfaces; UI skill; REQ-030 package/catalog/view contract | All eight new semantic components are present in exports, renderer dispatch, loader registration and schema-derived catalog entries. Existing package entry points remain. Catalog parity was independently checked without regeneration and passed; round full `codegen:check` also passed. | PASS |
| `components/{CardInventory,CardFacts,ArtifactTable,CardHeader,Panel,PromptMessages,PromptVariables,RelationshipList,UsedBy,VersionDiff,StatusPill}.svelte` | UI skill; `brand/{DESIGN.md,components.json,brand-skill.md}` and palette/theme tokens; root client ownership; maintainer style | Components receive typed props, render Svelte runes/snippets, and use brand tokens; no credential access, domain fetch, new UI library, unsafe HTML injection or decorative elevation was introduced. Controls and non-color state text are present; focus styles are defined. Native Prompt fields remain rendered rather than interpreted as executable markup. This source pass does not replace required visual/container/accessibility acceptance. | PASS |
| `src/lib/cards-host.ts`, Cards inventory/detail server and Svelte routes, tenant rail | Root client/server/security rules; UI skill and SvelteKit architecture reference; REQ-030 server boundary | URL fields are independently encoded using `URLSearchParams`; server loads use the existing authenticated `browserSession.client()`/Node executor and package validation. Browser routes hold URL/action state and import browser-safe package entry points. Existing authenticated rail enables Cards. No credentials or protected-table writes enter page data. Ownership of dependent planning is also covered by the separate maintainer review. | PASS |
| Observe `+layout.svelte`, `+page.server.ts`, `+page.svelte` | Root server-client/error rules; UI skill SvelteKit/runes/error-state rules; applicable Observe regression boundary | Existing authenticated executor remains; promises stay in SvelteKit's server-data channel and rendering has loading/then/catch paths. Direct SSR awaits both reads together. No new process lifecycle, auth capability or transport. Failure-propagation correctness belongs to the selected system reviewer. | PASS |
| `src/lib/ui-fixtures.ts`, `routes/_gallery/+page.svelte` | Root testing/client rules; UI skill testing/verification; REQ-030 gallery boundary | Gallery state executors and fixture Cards stay in existing fixture/gallery/test consumers. New conventional Cards route code imports no fixture executor. Gallery adds each new semantic component's cases to the existing owner rather than a new harness. | PASS |
| `src/lib/ui-library.test.ts`, `ui-library.integration.test.ts`, `ui-pages.integration.test.ts` | Root testing and permanent-code rules; `TESTING.md`; UI testing guide; approved layered proof | Tests extend existing Vitest, real SDK/server, Node/MCP/AppBridge and production-BFF/browser owners. Assertions retain exact refs, stable refusals and foreign-tenant absence. No ignore/allow/skip, test-only production configuration, dependency or replacement harness was added. Existing expected columns/paging were updated; their diagnosis and regression acceptance are for behavior/orchestrator reconciliation. | PASS |
| `fixtures/README.md`, `fixtures/cards/ui_cards/*.yaml`, `scripts/dev/example-app.sh` | Root vocabulary/Cards/testing rules; approved REQ-062 seed boundary; existing dev lifecycle owner | New fixtures use `wyrd/v1`, current Prompt and Agent forms, and an exact Prompt pin. Setup extends the existing public `wyrd declare` loop under its existing failure behavior; no protected-table write, alternate setup controller, secret or new environment setting. Fixture provenance is listed in the existing README. | PASS |

All nested contributor rule files affecting the changed paths were inventoried. Only the TypeScript SDK has an additional applicable `AGENTS.md`; no narrower UI/spec/server/scripts/fixtures contributor file was present.

## Mandatory Rust documentation audit

The complete changed Rust set was inspected, including surrounding existing item docs, not just added `///` lines:

| Item | Evidence | Result |
|---|---|---|
| `PromptSpec::status` (`spec.rs:59`) | Defines Card-read purpose and compiler ordering; infallible, no panic or IO. | PASS |
| `PromptStatus` and `variables`/`media_variables` (`spec.rs:68–82`) | Describes compilation owner, read-only lifecycle, non-persistence, registration treatment and each field. | PASS |
| `Status::prompt` (`envelope.rs:468`) | Describes Prompt-only presence and server-derived read projection. | PASS |
| `portable_card_round_trips_as_authored` (`prompt/mod.rs:450`) | Existing substantive test docs cover authored form, variables and serialization failures/panics exercised by the new assertion. | PASS |
| `hydrate_card` (`cards/service.rs:380`) | Updated projection description; existing typed error/integrity description preserved. No durable mutation introduced. | PASS |
| `immutable_blob_projection_is_deterministic_and_excludes_live_edges` (`service.rs:3104`) | Existing purpose and panic docs cover the added optional status field fixture. | PASS |

## Material findings

None retained. No material repository-standard failure was established in this assigned source scope.

Minor wording/spelling deviations do not meet the review's materiality threshold: the integration-test file's changed heading adds a task ID to an existing task-number heading (`ui-library.integration.test.ts:2`), contrary to the permanent-code wording rule; the new `Status.prompt` type is qualified inline (`envelope.rs:474`), contrary to the Rust bare-type import convention. They do not change supported behavior or violate the mandatory rustdoc gate; they are not standalone remediation work.

## Verification and limits

- Independently executed: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui --filter wyrd-ai-ui catalog --check` — PASS, exit 0.
- Read this round's `verification.md`: 18/18 UI library tests, package/app type checks (zero errors/warnings), production build, codegen/brand/catalog checks, whitespace check and focused real authorized host proof passed. These are orchestrator execution evidence, not reruns by this reviewer.
- At finalization the orchestrator reported `fmt:check` and scoped `wyrd-spec` Clippy passed and recorded them in `verification.md`. Full server/workspace lints and nested SDK format/lint/type/N-API lanes were explicitly not rerun. Those remaining execution obligations stay visible for final verification; this static PASS does not waive them.
- No browser surface was available; no manual visual, desktop/mobile, light/dark or human acceptance is claimed. No live development stack was restarted. Broader behavior, sensitive-boundary, ownership and resilience acceptance remain with the other selected reviewers and final validator.

Skipped duplicate builds/journeys and source edits. Risk remains in the unverified gates and visual acceptance explicitly listed above.
