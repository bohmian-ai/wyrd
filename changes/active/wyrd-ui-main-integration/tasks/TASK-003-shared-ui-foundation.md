# TASK-003 — Reusable UI foundation across the four hosts

Spec: [revision 10, approved](../spec.md). Status: Proposed.
Dependencies: [TASK-002](TASK-002-remove-old-ui.md).  
Mapped obligations: REQ-030, REQ-020, AC-006, AC-008, AC-011, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

Revision-10 clarification: REQ-033 and TASK-001-R1 supersede the original
browser-bearer premise only. Consume the Rust-owned session boundary after
remediation; no new Node client secret/cookie key or credential-bearing cookie.
Other scope and task dependencies remain unchanged.

## Outcome and Value

`wyrd-ai-ui` is consumable as a package and view document, with complete Chart and CardHeader, shared primitives, executors, gallery and T1–T5 proof. This is the first real domain/analytical consumer.

## Owners, Scope, Consumers, and Prohibited Changes

Own the approved six package exports, workspace, generated wire roots, Zod catalog/codegen check, view validation/WyrdView/mountView, portable dependent loaders, Node/MCP executors and session-bound POST /t/[tenantKey]/ui/load. Own all Chart types and CardHeader states/interactions, plus EmptyState, CardRefChip, VerdictPill, StatusPill, Sparkline, DurationBar and Drawer. Build the complete available-component browser artifact for T4 through scripts/build.sh; production MCP registration belongs to TASK-011. No per-component routes, generic write dispatcher, A2UI adapter, saved-view store or Ask service.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Exact Card reads | `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Cards.get`, `Cards.list`; server `src/mcp/cards.rs::GET_BY_REF` | `sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts::registered`; current Card envelopes | No portable UI projection | Extend the approved component loaders over these reads | No new registry, identity lookup or lineage owner |
| Bounded analytical reads | `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Bifrost.stream`; `crates/wyrd/wyrd-server/src/query/collect.rs::BoundedQuery` | `sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts`, `query-results.test.ts`; MCP `bifrost.rs::query_tool` | UI props and host parity missing | Consume foundation normalization and shared query owner | No second query service or transport |
| Session-bound SDK | Existing `BrowserSession.api` request boundary and shared `wyrd-client`/TypeScript client transport; REQ-033 transfers authority to Rust in TASK-001-R1 | UI hooks/session and SDK client construction consumers; revalidate against completed remediation | Request-scoped opaque browser-session transport is not currently projected into the SDK | Extend only the shared HTTP transport/TypeScript projection when needed to forward the selected session cookie and trusted origin; keep user-server credential flows unchanged | No Node OAuth mint/refresh, duplicate client/session store, browser token or domain integration in TASK-001-R1 |
| Component test/build ownership | UI package scripts, vite.config.ts, vitest-setup.ts; `scripts/gen_ts_types.py::WIRE_ROOTS`; mise codegen:check | SDK tests/support/server.ts::serverTest and @wyrd/testing startTestServer | No shared package, catalog, renderer or T1–T5 harness | Add approved component infrastructure under current workspace/build/test owners | Package/renderer/executors expressly required for four real hosts; no extra framework |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Establish workspace/package exports and generated contracts; use existing brand tokens and finish primitives, Chart and CardHeader.
2. Implement closed read union, loaders and exact host adapters; project optional shared QueryParam values on existing MCP bifrost.query.
3. Add catalog generation, panel validation, renderer/mountView, supersession/cancellation and typed action handoff.
4. Close session-bound BFF loading and runnable custom-app example with direct-component and view-document pages.
5. Build self-contained renderer artifact and dev gallery; extend existing verification owners for T1–T5 and record real sanitized fixtures.

## Ordered Implementation Scenarios

### Scenario 1 — Validated portable rendering

**Behavior.** Chart and CardHeader and every primitive render all applicable states and interactions in light/dark desktop/mobile; invalid view input fails one panel without blocking valid siblings; mountView works in a non-Svelte DOM host.

**RED.** Create the focused foundation rendering proof against public exports; the package and renderer are absent. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
```

**GREEN.** Implement the specified minimal layout/catalog/schema contract and complete these components, not provisional stubs. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Identical authorized reads

**Behavior.** All six approved reads execute with exact identities; bound queries work on SDK/MCP, returning equivalent positional normalized results after valid terminal only. Denial, malformed binds, duplicate names/nulls, supported encodings, degraded terminals, failure and limits remain honest.

**RED.** Run the new foundation live journey over real SDK/server and MCP; current MCP QueryArguments rejects params and Node has no normalization adapter. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-003: Identical authorized reads$"'
```

**GREEN.** Extend QueryArguments/input_schema/to_request with shared QueryParam and adapt existing SDK/MCP results without backend routes or SQL interpolation. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 3 — Dependent loads and safe actions

**Behavior.** Failed/denied prerequisites issue no dependent read; replaced ranges/inputs and removed panels cannot accept stale results; cancellation propagates where supported. Typed intents are validated by hosts, pending prevents duplicates and refusal remains visible.

**RED.** Add cancellation/dependent-read and intent-boundary assertions through the renderer and executors; no such shared behavior exists. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'
```

**GREEN.** Keep pure shaping in loaders and credentials/authorized action execution at host boundaries; preserve authored SQL and resolve relative range once. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 4 — Actual four-host foundation proof

**Behavior.** Chart/CardHeader pass T1–T5, including built HTML in AppBridge, real Node server data/empty/denied and custom app; browser BFF cross-tenant/malformed component inputs cannot expose credentials or another tenant.

**RED.** Add focused artifact and real-server journeys; current UI cannot provide this package or endpoint. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-003: Actual four-host foundation proof$"'
```

**GREEN.** Use the existing UI Vitest and @wyrd/testing setup; no new test environment knobs or engine substitutes. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- Every approved export builds from package contents; browser exports never import wyrd-ai/native Node code.
- Catalog is generated and drift checked; only component input/action schemas define dispatch authority.
- Chart schema errors and query failures are visible; Open in Query preserves exact SQL and typed binds.
- CardHeader preserves exact versions, paging/version failure states, copied identity and declared action results.
- All owned components/primitives have individual visual acceptance; runnable custom-app example contains no credentials in browser output.

## Expected Write Set and Consumer Closure

UI packages/ui, workspace/manifests/lock, gallery and BFF ui/load boundary; existing auth boundary only as needed; SDK generated wire roots; query/collect.rs and MCP parity tests; codegen owner; scripts/build.sh; one examples app; current UI/SDK test setup. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` after focused tests. Run `mise run codegen:check` when catalog/generated inputs change. Record T1 states, T2 actual requests/shape, T3 view validation/render, T4 actual built AppBridge HTML, T5 real authorized Node/server data and T6 only for assembled pages. Data-free primitives have no invented loader/live proof. Preserve per-component fixture provenance and interaction/action-host evidence.

Foundation also owns the bounded MCP parameter runtime/journey proof and native SDK production packaging closure. Add Rust-only argument/schema assertions in existing query owner, and retain this exact positional-result regression:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::collect::tests::query_result_is_positional_and_counts_exact_structured_json_bytes)'
```

Run `mise run fmt`, `mise run lints`, `mise run check:deps`, `mise run codegen:check` and SDK typing/pack checks if those surfaces change. Keep runtime-dependent behavior in Node. Ensure scripts/build.sh production closure now includes the real wyrd-ai addon/runtime dependencies on the three native targets and selected images; TASK-001 remains domain-free. Run the task live scenario with actual MCP query calls as well as SDK reads, including malformed binds, authorization, cancellation and query ceilings.

Changing the package is authorized; publishing to npm is not part of implementation. Complete Chart/CardHeader once here; later families consume them without re-owning their contracts.

### Human visual checkpoint

Run /_gallery against Main/QueryChart and Card boards for Chart, CardHeader and every primitive; also run both custom-app example pages and AppBridge artifact. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md).
- [Execution](../../../../changes/implementation-execution.md), [TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).

## Implementation Evidence

Commits: `9e8e14998`, `0bc456a07`, `695ca914d`, `0ccdeef86`, `26f881d6f`, `b596bfe6b`, `2091a1d09`, `56a6e3bee`, `090920617`, `e8b0c42f2`, `7acf4b55e`.

Reuse map, as revalidated:
- **Typed binds on MCP.** `QueryArguments` and `to_request` in `src/mcp/bifrost.rs` were extended with the shared `QueryParam`. No new route was added.
- **Bounded positional result.** This projection moved once into `wyrd_queue::json_result::JsonResultBuilder`, and both MCP `collect` and the TypeScript SDK `collect` call it.
- **Session-bound BFF.** It reads through `WyrdClient.forBrowserSession` with the Rust-owned session from TASK-001-R1. One gap needed a fix: `Bifrost::connect_with` dialed gRPC ingest even for a browser-session client, and that client is HTTP-only. A browser-session client now gets the existing `query_only` shape through a single private constructor, `from_sink`/`read_only`.
- **Executors.** `nodeExecutor` wraps the SDK and `mcpExecutor` wraps `App.callServerTool` (MCP `Client.callTool` in tests). Live tests reuse the SDK's `serverTest`/`declared`/`fixture`. Component tests and the gallery share `ui-fixtures.ts`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Six approved exports build from package contents; browser exports never import wyrd-ai/native code | `packages/ui/package.json` exports; `nodeExecutor` only under the Node export | `pnpm check`, `pnpm build` (UI + package + example); scenario 1 export assertions | PASS |
| Catalog generated and drift-checked; only component input/action schemas define dispatch | `packages/ui/scripts/catalog.ts`; `validatePanel`/`validateIntent` in `loaders.ts` | `mise run codegen:check`; scenarios 1 and 3 | PASS |
| Chart schema errors and query failures visible; Open in Query keeps exact SQL and typed binds | `shapeChart`, `chartParams`, `ChartProps.executed` | Scenario 1 (T1 states), scenario 3, T4 tool-call assertion on `params [from,to]` | PASS |
| CardHeader keeps exact versions, paging/version failures, copied identity, declared actions | `loadCardHeader`, `CardHeader.svelte` | Scenarios 1 and 3; T5 header populated/denied | PASS |
| Six reads identical across SDK and MCP; malformed binds, duplicate names/nulls, ceilings, cancellation, authorization | `ui-library.integration.test.ts` "Identical authorized reads" | Scenario 2 live command: 1 passed | PASS |
| T4 built HTML behind AppBridge; T5 Node populated/empty/denied; BFF cross-tenant and malformed inputs expose no credential or tenant; custom app has no credentials | `packages/ui/dist/wyrd-view.html`, `routes/t/[tenantKey]/ui/load/+server.ts`, `examples/ui-app` | Scenario 4 live command: 1 passed | PASS |
| Browser-session Bifrost connects without an ingest channel | `bifrost/facade.rs::connect_with`, `client.rs::is_browser_session` | `wyrd-client` lib `client::tests::browser_session_bifrost_connects_without_an_ingest_channel` (fails with the branch disabled, passes with it); `wyrd-client` lib 265/265 | PASS |
| MCP parameter argument/schema assertions; positional-result regression kept | `mcp/bifrost.rs` tests; `query/collect.rs` | `query_schema_is_closed_bounded_and_has_no_path_selector`, `query_result_is_positional_and_counts_exact_structured_json_bytes`: passed | PASS |
| `scripts/build.sh` production closure ships the real wyrd-ai addon and runtime deps | `bundle()`: `ts:build` in the bundle profile, then `npm pack` of the SDK, `pnpm add --no-optional`, and the host `.node` beside the SDK entry; `ts:build` honours non-dev `WYRD_PROFILE` | `WYRD_PROFILE=dev scripts/build.sh bundle` on darwin-arm64: exit 0, `import("wyrd-ai")` loads; release/ui has `apache-arrow`, the packed SDK, and the host addon, with no registry addon | PASS (darwin-arm64) |
| Gates | — | `fmt`, `lints`, `check:deps`, `codegen:check`, `ts:napi:check`, `ts:pack:check`, UI `pnpm test` (33/33), `git diff --check` | PASS |

Exact commands run:
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'`
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'`
- The scenario 2 and scenario 4 `with-test-postgres.sh` commands above.
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::collect::tests::query_result_is_positional_and_counts_exact_structured_json_bytes)'`
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=mcp::bifrost::tests::query_schema_is_closed_bounded_and_has_no_path_selector)'`
- `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=client::tests::browser_session_bifrost_connects_without_an_ingest_channel)'`

Material limits (unverified, not passes):
- **Degraded terminals.** Degraded-terminal parity is not produced by a live server. Scenario 2 proves success, failure, and limit terminals live. Degraded projection rests on the shared `JsonResultBuilder::finish`, which both surfaces call.
- **Linux bundles and images.** The production closure was proven on darwin-arm64 only. Linux x86_64/arm64 bundles and the debian/rocky images are proven by the release workflow's `scripts/build.sh bundle` and image jobs. They were not run locally.
- **Human visual checkpoint: APPROVED** by Steven Forrester on 2026-10-20 after the rework below: `/_gallery` (Chart, CardHeader and every primitive in light and dark, desktop and mobile), the custom-app example pages and the AppBridge artifact.
- **First human pass (rework applied; re-inspection approved above).** Findings: chart panels overflowed their grid cells; Chart and CardHeader diverged from the canvas anatomy; the primary action lost its fill on hover. Rework: Chart follows the canvas panel (head, body sized to its container, `source` footer), dashed gridlines with nice ticks in a label gutter, capped bars with value and category labels; CardHeader follows the canvas header (kind tag, version select, status, metadata line, Copy ref, primary Open in Observe, underline tabs); primary hover deepens `--declare` per `brand/components.json`. Self-checked by headless Chromium screenshots at 1440 and 375 px in light and dark (no horizontal overflow); `pnpm test` 33 passed, `pnpm check` 0 errors, scenario 4 live passed, `codegen:check` and `check:examples` passed.

Non-goals stayed excluded: no per-component routes, generic write dispatcher, A2UI adapter, saved-view store, Ask service, production MCP registration (TASK-011), or npm publishing.
