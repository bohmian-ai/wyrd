# TASK-003 repository standards review

Overall result: **FAIL** (one proposed public-error contract finding; independent validation remains required).

## Immutable subject and scope

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; base `695ca914df61a8683059f6f32279ec2148424e06`; candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`. HEAD remains the candidate. This is an independent `repo-rev`; no peer reports were read. Source was not edited.

Inspected the cumulative range and the relevant implementation commits, including `7acf4b55e` and `e8b0c42f2`. The merged main changes to the Monty type checker, SQL dependency stack, gateway and Forge tests are not TASK-003 implementation. The caller-excluded Linux builder commit `45797e42e`, identity-test change `2884da447`, llms regeneration `62966bbdd`, and CLI lane change `fc0d28fda` were identified separately. TASK-027's authorized grid/layout additions are successor behavior, not TASK-003 drift.

This report audits repository compliance; it does not replace the implementation reviewers' task acceptance matrices or the domain review.

## Authority coverage

The router is `architecture/references/README.md`. Applicable foundation authorities are `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md` (doctrine, client model, runtime identity, Bifrost, error catalog), `architecture/wyrd-doctrine.mdx`, `architecture/wyrd-security-posture.md` (principles, trust boundaries, browser sessions), `architecture/bifrost-design.md` (read terminal and public surface), and `architecture/operations/deployment-and-release.md` (browser-session deployment and local bundles). Domain-internal storage, Iceberg writer, provider, Python and N-API implementation rules do not govern new code in this task: it changes none of those implementations.

| Changed surface | Applicable routed references and local authority | Source/consumer coverage |
|---|---|---|
| Shared Rust `Bifrost` construction and browser-session discrimination | `languages/rust-core.md`, `architecture/patterns.md`, `doctrine/architecture-constraints.md`, `domain/olap-serving.md`, security/browser-session and Bifrost public-surface authority | Entire changed methods in `src/bifrost/facade.rs` and `src/client.rs`; existing `QueryOnlySink`, `WriterPool`, `QueryClient`, credential discrimination, browser-session regression test; TypeScript `Bifrost.connect` consumed by Node executor |
| Browser-safe package contracts, Zod schemas and portable loaders | `languages/typescript-guide.md`, `languages/errors.md`, `languages/agent-harness.md`, design doctrine and approved component/document authority | `contract.ts`, `schemas.ts`, `loaders.ts`, `plot.ts`, `format.ts`, index exports; chart/header consumers and fixture/executor call sites |
| Node and MCP executors | Same TypeScript/error/agent references, Bifrost terminal contract, shared-client boundary | `node.ts`, `mcp.ts`, underlying SDK facade and MCP calls, live integration test operations and limits/denials |
| View document validation, mount and remote loading | TypeScript/agent/error references; repo-local UI Svelte 5 and SvelteKit references | `view.ts`, `WyrdView.svelte`, AppBridge entrypoint, gallery, custom-app view; layout-only content key and abort generation guards inspected as candidate context |
| Components and primitives | `.agents/skills/wyrd-ui/SKILL.md`, all applicable bundled Svelte, theming, developer UX, data-performance and testing references; `brand/DESIGN.md`, palette and component authority | Chart, CardHeader, ActionButton, EmptyState, CardRefChip, VerdictPill, StatusPill, Sparkline, DurationBar, Drawer; props/state/actions, keyboard/focus, CSS and plot helpers |
| Session-bound BFF endpoint | Security browser-session authority; SvelteKit server boundary; `languages/errors.md`; agent/reference validation rules | `src/routes/t/[tenantKey]/ui/load/+server.ts`, `BrowserSession.client`, existing hooks/session resolution and generated problem owner |
| Gallery, fixtures and tests | `languages/testing-workflows.md`, `languages/spec-driven-development.md`, `languages/implementation-execution.md`, `languages/maintainer-style.md`, UI verification reference | Gallery production 404 guard; fixture executor; unit scenarios and integration scenarios; existing Vitest selector and test setup; task's exact command/evidence record |
| Package/build/code generation | AGENTS generated-artifact and dependency ownership rules; deployment/release authority; TypeScript declaration rules | Six package entrypoint families plus HTML artifact export, manifests, workspace and lock, svelte-package/Vite build, catalog generator, theme generator extension, mise codegen and ts:build changes, in-scope SDK packing change in `scripts/build.sh` |
| Custom SvelteKit example and README | UI/server boundary and TypeScript rules; deployment credentials rule; maintainer documentation principle | Every example route, executor, panel fixture, app HTML, package/config, README; direct and view-document consumers |

## Applicable-rule results

| Rule | Result | Evidence |
|---|---|---|
| Durable domain/auth/registry/query behavior remains Rust-owned; SDK consumes shared `wyrd-client` | PASS | Node executor invokes `Cards`, `Bifrost`, and `verify.Execution`; MCP executor maps onto existing tools. The Rust change extends the existing Bifrost construction owner and shares `from_sink`, rather than adding a second transport. |
| Rust owner shape, sync/async boundary, imports and dependency cost | PASS | `read_only`, `from_sink` and `is_browser_session` are synchronous methods on actual state-owning handles; network construction remains async. No Cargo dependency or feature introduced by the in-scope change; no specialized server dependency added to the client. |
| Rustdoc on new/materially modified items, including tests | PASS | `connect_with`, `with_sink`, `query_only`, `read_only`, `from_sink`, `assemble`, `QueryOnlySink::send`, `connected`, `is_browser_session`, and the new browser-session test have substantive docs; fallible production operations have Errors sections. The construction seam has no newly introduced durable write/cancellation protocol. |
| No raw SQL capabilities or manual tenant-filter substitute | PASS / N/A | In-scope Rust methods and fields introduce no `PgPool`, `PgConnection`, transaction, `TenantConn` or `OperatorPool` API; there is no SQL implementation in this change to excuse behind a green tenant check. |
| Session/credential isolation and server audit authority | PASS for inspected structure | BFF uses hook-resolved `locals.browserSession`; its client forwards the private selected cookie/origin. No body tenant selector, browser SDK runtime import, Node credential renewal, new audit writer or policy owner. Existing permission/audit decisions remain at the Rust endpoints. Domain security review supplies deeper end-to-end assessment. |
| Typed public contracts and browser/native separation | PASS for inspected structure | `WireRequest` is a closed union over generated domain shapes; browser-facing modules use type-only SDK imports; only the Node entry imports runtime `wyrd-ai`. Each component takes typed props; untrusted document/panel/action inputs meet Zod validation before dispatch. |
| Public error catalog is derive-backed; no TypeScript-only Wyrd identities or handwritten HTTP problems | **FAIL — REP-003-01** | Local errors use uncatalogued `WYRD_UI_*` codes, and BFF/example rejection helpers serialize handwritten reduced problems rather than the existing generated canonical projection. See finding below. |
| Generated declarations/catalog/theme are source-driven and drift checked | PASS | Catalog is emitted from Zod schema owners; theme generator adds a package projection of the same palette; codegen checks catalog and theme; N-API declarations are not edited. The approved Zod catalog is not an unauthorized second durable server contract. |
| Platform/build owner is preserved | PASS structure; limited runtime proof | Existing `scripts/build.sh` remains owner and calls existing `ts:build`, packs SDK and copies native addon beside its loader. No new build framework. Darwin import smoke test is recorded; Linux runtime closure is unverified here. |
| Test runtime and fixture ownership | PASS structure | New Rust regression uses only Rust. Native Node behavior runs under Vitest/SDK `serverTest` using `@wyrd/testing`; no Python/Node runtime embedded into Rust tests. `WYRD_UI_INTEGRATION` is present in the base Vite config and existing identity harness, not a newly invented setting. |
| Test/gate integrity | PASS for task-attributable changes | Added tests assert rendered outcomes, exact reads, denial, invalid input, limits and supersession. The caller-excluded CLI/identity selector/assertion edits are not attributed here. No in-scope allow/ignore weakening was found. |
| Brand tokens, semantics, accessibility and responsive structure | PASS for inspected structure | Package theme is palette-generated; status uses words/glyphs; time-series styles pair dash/marker; controls expose labels and focus; table scroll region is keyboard reachable; Drawer uses the native modal dialog owner. No material repository-style-only finding is proposed. Visual acceptance remains the separately approved human checkpoint. |
| No legacy vocabulary, compatibility API or server-only durable field projection | PASS | Components consume current Card kinds/refs and generated query values. No legacy resource alias, backend route family or durable store introduced. |

## Material proposed finding

### REP-003-01 — A second, non-catalogued Wyrd problem contract is exposed by the UI foundation

Classification: **VIOLATION**. Severity: medium. This is one shared error-authority cause, including the HTTP projection below.

Governing rules: AGENTS §4/§9 require public boundary errors to use derive-backed Wyrd error metadata; `architecture/agent-rules.md` forbids handwritten parallel problem logic; `languages/errors.md` explicitly says not to invent TypeScript-only codes and to use canonical problem projections for HTTP. `languages/agent-harness.md` makes the catalog the error authority. `architecture/wyrd-design.md` Error catalog makes it the source of truth across surfaces.

Exact source:

- `packages/ui/src/lib/loaders.ts:105` and `:119`: `WYRD_UI_CHART_RANGE_REQUIRED`, `WYRD_UI_CHART_SHAPE`.
- `packages/ui/src/lib/mcp.ts:29`: `WYRD_UI_MCP_TOOL_FAILED`.
- `packages/ui/src/lib/WyrdView.svelte:69`, `:201`, `:237`: `WYRD_UI_NO_EXECUTOR`, `WYRD_UI_LOAD_FAILED`, `WYRD_UI_ACTION_FAILED`.
- `packages/ui/src/lib/view.ts:63`: another `WYRD_UI_LOAD_FAILED` producer.
- UI BFF `src/routes/t/[tenantKey]/ui/load/+server.ts:33-34` and `examples/ui-app/src/routes/view/load/+server.ts:15-16`: handwritten `application/problem+json` serialization of the four-field UI `Problem`.

Paths above under `packages/` and `src/` are rooted in `crates/wyrd/wyrd-server/wyrd-ui`.

Producer-to-consumer evidence: an authored chart requesting range binds without a range returns the first new identity from `chartParams` via `loadChart`; a mismatched server result returns the second via `shapeChart`. The exported portable loader and direct components expose these values publicly. The same shape error travels through `loadPanel` at the BFF into browser JSON and `EmptyState`, which labels/displays it as a stable Wyrd code. `mountView` explicitly permits an omitted executor and then emits the no-executor code; failed remote loads and host-action exceptions generate other sibling codes. Searching the canonical Rust catalog supplies none of these identities. For malformed component input, `inputProblem` uses a real Validation code but the endpoint's `refuse` emits only `code/title/status/detail`, dropping canonical `type`, `details` and `remediation`.

Observable consequence: package/agent consumers receive error identities absent from generated Wyrd catalogs, and malformed-panel HTTP responses omit the published validation remediation/type metadata. The same class of error therefore has a UI-specific contract which codegen cannot verify. Green checks do not detect it because the tests explicitly expect UI-only codes or only subset-match the validation response.

Existing owner to reuse: `crates/wyrd-spec/src/error.rs::WyrdError` already defines Validation/Internal and their metadata; `crates/wyrd-spec/schemas/ui_problem_examples.json` is its browser-safe generated projection; `$lib/server/problem.ts::problem` already consumes those values in the BFF. Existing server/SDK failures should retain their actual canonical metadata. No new error catalog or durable UI-error domain is needed.

Smallest testable correction: source locally generated validation/internal failures from those existing canonical projections and distribute the necessary browser-safe generated data with the package; keep component-specific diagnostic context in detail rather than introducing new Wyrd codes. At the BFF/example HTTP boundary, reuse the generated canonical problem projection and preserve diagnostic context instead of serializing the reduced UI shape as the problem authority. Do not change backend permission/terminal semantics or add backend routes. Cover missing range, incompatible chart shape, failed host/remote operation, and malformed-panel HTTP response with focused existing UI tests, asserting catalog-backed code/title and canonical HTTP metadata. Catalog regeneration/typing/package checks are sufficient additional lanes for that correction's actual write set.

## Non-blocking calibration and verification limits

Task IDs occur in new permanent test names/comments, contrary to the agent-rule wording; the task itself prescribes those exact titles. This has no behavioral or public-contract consequence and is not proposed as a blocking finding or a separate cleanup round.

The task records fmt/lints/dependency/codegen/SDK pack and typing checks, live Postgres/SDK/MCP/BFF scenarios, and Darwin bundle smoke evidence. The orchestrator's `verification.md` independently records 7 passing tests in `ui-library.test.ts`, both exact TASK-003 unit scenario reruns, package/app Svelte checks at zero errors/warnings, artifact build, and clean cumulative `git diff --check`. I independently ran only the whitespace check, not runtime tests.

No live degraded-terminal parity was produced, and Linux native bundles/images were not exercised for this review. Neither is represented as green. No broad Cargo lane, live Postgres journey or new visual inspection was rerun by this reviewer. These limits remain visible to the acceptance orchestrator; the standards finding above is source-backed and does not depend on missing platform evidence.
