# TASK-010 — Ask and saved-view interaction component

Spec: [revision 9, approved](../spec.md). Status: Proposed; platform prerequisite holds completion.  
Dependencies: [TASK-003](TASK-003-shared-ui-foundation.md).  
Mapped obligations: REQ-030, REQ-031, REQ-020, AC-011, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

External dependency: Separately specified/shipped Ask endpoint and REQ-031 saved-view resource/read/write/visibility contracts with applicable SDK/MCP actions. This task does not authorize implementation of that platform prerequisite. Contract-independent work may proceed, but fixtures/unavailable controls cannot close it.

## Outcome and Value

AskPanel renders a real text-plus-view answer and saves/reopens its view document as the viewer, without owning an agent service or durable saved-view store.

## Owners, Scope, Consumers, and Prohibited Changes

Own AskPanel and its typed host actions. Reuse WyrdView and BFF load endpoint. Follow owner/tenant private-or-tenant saved-view visibility; store view documents only and re-run data as the viewer. Gateway chat completions are not the Ask endpoint. No A2UI adapter or new agent runtime.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Interactive view host | `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts::BrowserSession`; TASK-003 proposed WyrdView and authenticated load endpoint | Current MCP catalog and SDK expose no Ask/saved-view owner; approved REQ-030/031 defines the prerequisite boundary | Ask and saved-view capabilities absent on main | Connect approved delivered platform endpoints using current viewer host credentials | Do not invent endpoint payloads, persistence or agent orchestration |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Obtain actual approved Ask/saved-view contracts and generated host projections.
2. Build AskPanel question/answer/loading/error interactions and renderer integration.
3. Complete save/open/share/retention behavior only to the approved saved-view contract.
4. Prove states, actions, T1–T5 and individual visual acceptance without a live model.

## Ordered Implementation Scenarios

### Scenario 1 — Answer and view remain safe

**Behavior.** Question input, text answer and validated view panels render through existing renderer; bad panels, denied/error and superseded answers remain honest; documents contain no data or credentials.

**RED.** Add focused Ask state/interaction/document tests against labeled contract fixtures. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-010: Answer and view remain safe$'
```

**GREEN.** Keep the component declarative and host-authorized, with renderer supersession. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Viewer-scoped saved view

**Behavior.** Actual authorized Ask and save/open actions complete; another viewer re-runs reads with their own scopes, visibility isolation is enforced and retention loss is explained.

**RED.** Add focused real-server Ask/saved-view-to-component journey with deterministic repository-managed provider substitute. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-010: Viewer-scoped saved view$"'
```

**GREEN.** Consume platform contracts; missing prerequisites stay explicit and block completion. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- AskPanel passes applicable T1–T5 and host action completion with no live model call.
- Saved documents contain no query results, props or secrets; sharing never widens access.
- Fixture answers/unavailable Save are progress only and cannot satisfy production completion.

## Expected Write Set and Consumer Closure

AskPanel/loaders/catalog, gallery/fixtures/tests and existing BFF host adapters to delivered contracts; no Ask or saved-view server implementation. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` after focused tests. Run `mise run codegen:check` when catalog/generated inputs change. Record T1 states, T2 actual requests/shape, T3 view validation/render, T4 actual built AppBridge HTML, T5 real authorized Node/server data and T6 only for assembled pages. Data-free primitives have no invented loader/live proof. Preserve per-component fixture provenance and interaction/action-host evidence.

### Human visual checkpoint

Rail Find or ask entry and Ask panel in gallery; all states, narrow keyboard and touch use. Canvas has no separate Ask PNG; use Main rail and REQ-030 behavior without inventing a new design system. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx).
- [Execution](../../../../architecture/references/languages/implementation-execution.md), [TDD](../../../../architecture/references/languages/spec-driven-development.md), [testing](../../../../architecture/references/languages/testing-workflows.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
