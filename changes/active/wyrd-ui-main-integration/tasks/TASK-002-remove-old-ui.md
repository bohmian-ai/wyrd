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
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md).
- [Execution](../../../../changes/implementation-execution.md), [TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).

## Implementation Evidence

Candidate: commits `a701d4ef0` (deletions) and the follow-up rewrite commit on this task branch. UI paths are relative to `crates/wyrd/wyrd-server/wyrd-ui/`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-032 delete list: `src/lib/components/`, `src/lib/features/`, `src/lib/server/{wyrd.ts,mock/,changes/}`, `src/lib/{registry,views}.ts` + tests, routes `styleguide/`, `t/[tenantKey]/{cards,changes,observe}/` | `git rm` in `a701d4ef0`; also removed the domain loader `t/[tenantKey]/+page.server.ts` and the chart-only ResizeObserver polyfill in `vitest-setup.ts`. `views.ts` types moved to their owners: `WyrdProblem` → `src/lib/server/problem.ts`, `Tenant`/`SessionMetadata` → `src/lib/server/auth/session.ts` | `rg` inventory below: no hit | PASS |
| REQ-032 mock switch removed: `mockDataEnabled`, `mockCookie`, `mockTenantKey`, `WYRD_UI_MOCK_DATA`, `WYRD_UI_MOCK_TENANT`, `locals.mockData`, `locals.wyrd` | `development.ts`, `hooks.server.ts` (mock + `WyrdClient` lines only), `app.d.ts`, root `+page.server.ts` (`mockData` action removed; local login/scenario/reauth gate on `localAuthEnabled()` alone; single-tenant scenario uses the development principal's first membership), root `+layout.*` (dev control is now only **DEV · Test login** when local auth is on), `mise.toml` `dev:frontend`, `README.md` | TASK-002 Vitest asserts no `mock*` export; `rg` inventory | PASS |
| REQ-032 keep list unchanged in behavior (auth, problem, upstream, routing, `localAuthEnabled` + login-scenario helpers, hooks, kept routes, `brand/`, `app.css`, `theme.svelte.ts`, `format.ts`) | Only type moves / mock-line removals in kept server modules; `brand/DESIGN.md` and `brand/components.json` `$comment` lost only their pointers to deleted paths | `pnpm test` (auth/session/problem/upstream/routing tests), identity journey | PASS |
| Kept routes rewritten as plain markup, no shared component imports; `t/[tenantKey]/+page` is a plain signed-in page | `+layout.svelte` (mode root), `+page.svelte` (inline tenant chooser), `+error.svelte`, `t/[tenantKey]/+layout.svelte` (tenant, tenant switch form, principal, sign out, Settings link, mode toggle), `t/[tenantKey]/+page.svelte`, `login/`, `login/api-key/`, `settings/`; shared `.app-problem` notice style in `src/app.css` | `src/lib/ui-library.test.ts` `TASK-002: Retained pages without mocks` | PASS |
| UI check, unit tests, production build pass | — | `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` → 0 errors 0 warnings; `... test` → 8 files, 24 tests passed; `... build` → done | PASS |
| Unchanged identity journey remains passing | — | `mise run test:identity:journey` (owner setup: Postgres, migrate, Keycloak/Dex, seed, UI build) ran `cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E 'test(=identity_ui_e2e::production_ui_bff_journey)'` → PASS (57.1s; inner Vitest 4/4). Lane stopped afterwards by me (the later CLI device-login step was SIGKILLed by that stop, not a failure); unrelated lane steps not run | PASS |
| No new package, shell or domain integration | No dependency change (`package.json`/lockfile untouched) | `git diff d2e2d25a8 --stat` | PASS |
| Human visual checkpoint (tenant chooser, login/API-key recovery, signed-in page, settings; states, desktop/mobile, light/dark) | — | Not performed by the implementing agent | UNVERIFIED — pending human |

Scenario 1 RED/GREEN: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-002: Retained pages without mocks$'` — RED before the change (1 selected, 1 failed: retained routes imported `$lib/components/`), GREEN after (1 selected, 1 passed).

Directly affected tests: `routing/tenant.test.ts` dropped the mock-Home/`WyrdClient` cases (their subject was deleted); `routing/journey.test.ts` (local-auth dev server) now asserts the plain signed-in page, tenant switch and sign-out instead of mock Home content and the mock toggle.

Removal inventory (excluding `changes/`, `node_modules`, build output):

```bash
rg -n --hidden "wyrd-ui/src/lib/(components|features)|\$lib/(components|features)|lib/server/(wyrd|mock|changes)|\$lib/(registry|views)|component-contracts|registry\.test|routes/styleguide|t/\[tenantKey\]/(cards|changes|observe)|mockDataEnabled|mockCookie|mockTenantKey|WYRD_UI_MOCK_(DATA|TENANT)|wyrd_ui_mock_data|locals\.(mockData|wyrd)|\?/mockData|ModeProvider|TenantChooser|StateBlock" -g '!changes/**' -g '!**/node_modules/**' -g '!.git/**' .
```

Remaining hits are not deleted paths: `docs/` site's own `$lib/components` (separate app) and component *names* inside `brand/components.json` contracts (brand is kept). `brand/gen-theme` tests 5/5 and `gen-theme.mjs --check` in sync. `git diff --check` clean.

Limits: `@xyflow/svelte`, `micromark`, `micromark-extension-gfm` are now unused by the UI source; removing them made pnpm re-resolve the `latest` dev dependencies (unrelated lockfile churn), so they were left for the package owner. Non-goals held: no `/auth` forwarding, no new components, no `crates/wyrd-spec/src/vala/trace/*` change.
