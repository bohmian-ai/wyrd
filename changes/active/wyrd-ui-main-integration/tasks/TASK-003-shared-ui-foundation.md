# TASK-003 — Reusable UI foundation across the four hosts

Spec: [revision 9, approved](../spec.md). Status: Proposed.  
Dependencies: [TASK-002](TASK-002-remove-old-ui.md).  
Mapped obligations: REQ-030, REQ-020, AC-006, AC-008, AC-011, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

## Outcome and Value

`wyrd-ai-ui` is consumable as a package and view document, with complete Chart and CardHeader, shared primitives, executors, gallery and T1–T5 proof. This is the first real domain/analytical consumer.

## Owners, Scope, Consumers, and Prohibited Changes

Own the approved six package exports, workspace, generated wire roots, Zod catalog/codegen check, view validation/WyrdView/mountView, portable dependent loaders, Node/MCP executors and session-bound POST /t/[tenantKey]/ui/load. Own all Chart types and CardHeader states/interactions, plus EmptyState, CardRefChip, VerdictPill, StatusPill, Sparkline, DurationBar and Drawer. Build the complete available-component browser artifact for T4 through scripts/build.sh; production MCP registration belongs to TASK-011. No per-component routes, generic write dispatcher, A2UI adapter, saved-view store or Ask service.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Exact Card reads | `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Cards.get`, `Cards.list`; server `src/mcp/cards.rs::GET_BY_REF` | `sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts::registered`; current Card envelopes | No portable UI projection | Extend the approved component loaders over these reads | No new registry, identity lookup or lineage owner |
| Bounded analytical reads | `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Bifrost.stream`; `crates/wyrd/wyrd-server/src/query/collect.rs::BoundedQuery` | `sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts`, `query-results.test.ts`; MCP `bifrost.rs::query_tool` | UI props and host parity missing | Consume foundation normalization and shared query owner | No second query service or transport |
| Session-bound SDK | `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts::BrowserSession.api` retains a private bearer; `sdks/wyrd-sdk-ts/wyrd/src/index.ts::WyrdClient.connect` already accepts credential | UI hooks/session tests; SDK client instantiation tests | No safe session-to-SDK construction path in BFF | Extend existing session boundary to construct per-request SDK clients without browser serialization | No duplicate auth/session store or token field in props |
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
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx).
- [Execution](../../../../architecture/references/languages/implementation-execution.md), [TDD](../../../../architecture/references/languages/spec-driven-development.md), [testing](../../../../architecture/references/languages/testing-workflows.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
