# TASK-011 — Production MCP App and catalog discovery

Spec: [revision 9, approved](../spec.md). Status: Proposed.  
Dependencies: [TASK-003](TASK-003-shared-ui-foundation.md), [TASK-004](TASK-004-observe-components.md), [TASK-005](TASK-005-card-components.md), [TASK-006](TASK-006-query-components.md), [TASK-007](TASK-007-verify-components.md), [TASK-008](TASK-008-service-components.md), [TASK-009](TASK-009-change-repository-components.md), [TASK-010](TASK-010-ask-component.md).  
Mapped obligations: REQ-030, REQ-020, AC-011, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

## Outcome and Value

The existing MCP endpoint serves the complete UI App resource, validates and shows view documents, and exposes the finished catalog to agents.

## Owners, Scope, Consumers, and Prohibited Changes

Full-library gate first: every component/action/live prerequisite and individual visual acceptance in TASK-003–010 is complete. Own resources capability, ui://wyrd/view, ui.show_view, ui.catalog, embedded artifact/catalog and production host verification. Reuse earlier built artifact and MCP executor; do not recreate them. Current MCP protocol/auth remain unchanged.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| MCP adapter | `crates/wyrd/wyrd-server/src/mcp/mod.rs::WyrdMcpHandler.get_info/catalog/call_tool`; server Cargo.toml existing jsonschema | Existing cards/bifrost/verification descriptors and dispatch; TASK-003 T4 artifact host | Main advertises tools only and has no UI resource/tools | Extend handler and embed scripts/build.sh outputs | No second MCP endpoint, alternate auth or duplicated catalog/renderer |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Verify full-library acceptance before production MCP integration.
2. Embed existing generated catalog and complete self-contained HTML through authoritative build owner.
3. Extend resource/tool discovery and stable document validation on the current MCP handler.
4. Exercise real authenticated MCP reads/actions in AppBridge and production resource/tool paths, including host theme/display modes.

## Ordered Implementation Scenarios

### Scenario 1 — Discover and validate UI resources

**Behavior.** Current authenticated MCP discovery exposes resources and two UI read tools; invalid view documents return stable errors, valid documents return structuredContent and correct resource metadata.

**RED.** Add focused MCP resource/tool contract journey; current tools-only handler has none. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-011: Discover and validate UI resources$"'
```

**GREEN.** Use embedded generated catalog with existing jsonschema and derive-backed error catalog. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Sandboxed UI as the caller

**Behavior.** Tool-result notifications render the requested view in inline/fullscreen and host theme; tool reads/actions use current caller scopes, with denied/errors visible and no direct network/credential exposure.

**RED.** Add focused real MCP App journey over built artifact; fixture T4 alone cannot prove production wiring. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-011: Sandboxed UI as the caller$"'
```

**GREEN.** Reuse complete browser artifact/executor and current /mcp lifecycle without protocol downgrade or connectDomains. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- All catalog components are available in embedded artifact and generated catalog identities agree.
- Invalid input never gains arbitrary tool/URL/code dispatch; real read/action scope failures remain visible.
- Custom-app npm package contents and runnable example still work; no actual npm publication required.

## Expected Write Set and Consumer Closure

Existing MCP handler/descriptors/resource dispatch/tests; scripts/build.sh embedding/build closure; generated embedded artifact/catalog and exact production App journey under current UI runner. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` after focused tests. Run `mise run codegen:check` when catalog/generated inputs change. Record T1 states, T2 actual requests/shape, T3 view validation/render, T4 actual built AppBridge HTML, T5 real authorized Node/server data and T6 only for assembled pages. Data-free primitives have no invented loader/live proof. Preserve per-component fixture provenance and interaction/action-host evidence.

Run `mise run fmt`, `mise run lints`, `mise run check:deps` and `mise run codegen:check` for MCP/build/generated changes. Add any necessary handler-only Rust checks under the existing server owner; record exact selectors before naming them. Actual resource/tool journey remains the focused production App test above.

### Human visual checkpoint

Actual MCP App resource in AppBridge with real authenticated tools; inline/fullscreen, desktop/mobile light/dark. Inspect representative composed view and host-specific behaviors; individual component acceptance is already required. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md).
- [Execution](../../../../changes/implementation-execution.md), [TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
