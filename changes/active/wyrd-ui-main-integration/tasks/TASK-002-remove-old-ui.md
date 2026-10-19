# TASK-002 — Remove obsolete UI while retaining login

Spec: [revision 9, approved](../spec.md). Status: Proposed.  
Dependencies: None.  
Mapped obligations: REQ-032, REQ-020, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

## Outcome and Value

The obsolete UI, domain mocks and their switches are gone; retained auth, tenant chooser, signed-in page and settings work as plain branded markup.

## Owners, Scope, Consumers, and Prohibited Changes

Apply the exact delete/keep inventory in REQ-032. Delete the listed old tests with their components; preserve production auth/session/routing tests. Remove mockData/local wyrd wiring only. Do not copy historical components or recreate the deleted directories. TASK-001 may land before or after this task; preserve its login proof either way.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Auth-only app | `crates/wyrd/wyrd-server/wyrd-ui/src/hooks.server.ts::handle`, `development.ts::mockDataEnabled/mockTenantKey/localAuthEnabled`; `BrowserSessions` | Kept layouts/pages currently import Panel/StateBlock/ModeProvider and app Shell; production-auth integration driver | Old imports and domain mocks prevent a clean base | Remove listed paths/wiring and render retained routes with plain markup | No replacement shared components in this task |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Delete REQ-032 listed owners and tests, remove only the mock switches and hook/local fields.
2. Rewrite retained route markup with preserved brand tokens and auth behavior; keep signed-in tenant/principal/sign-out content.
3. Check imports/settings and exercise unchanged real login through its current production owner.

## Ordered Implementation Scenarios

### Scenario 1 — Retained pages without mocks

**Behavior.** Kept pages show tenant/principal/sign-out and settings without domain mock data or deleted component imports.

**RED.** Add the task-focused retained-route regression; existing domain-filled signed-in rendering and mock wiring violate it. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-002: Retained pages without mocks$'
```

**GREEN.** Use plain Svelte markup and keep current auth error/refusal behavior. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- Every delete and keep item in REQ-032 is accounted for; no remaining source/config reference to a deleted path or removed mock setting.
- UI check, unit tests and production build pass; unchanged identity journey remains passing.
- No new package, shell or domain integration is introduced.

## Expected Write Set and Consumer Closure

REQ-032 UI paths, kept layouts/pages, hooks/app declarations, development settings and directly affected tests; no server auth redesign. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check`, `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test`, `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build`. Check removal/import/settings inventory with `rg`, excluding preserved packet references. Reuse the exact existing identity regression after its managed setup:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E 'test(=identity_ui_e2e::production_ui_bff_journey)'
```

Deletion, removed-import/settings search and unchanged login are static/regression proof; no manufactured RED for file removal.

### Human visual checkpoint

Tenant chooser, login/API-key recovery, signed-in landing page and settings, including unauthenticated/expired session and error states. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx).
- [Execution](../../../../architecture/references/languages/implementation-execution.md), [TDD](../../../../architecture/references/languages/spec-driven-development.md), [testing](../../../../architecture/references/languages/testing-workflows.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
