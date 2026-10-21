# TASK-008 — Service signal and chart authoring component family

Spec: [revision 9, approved](../spec.md). Status: Proposed; platform prerequisite holds completion.  
Dependencies: [TASK-003](TASK-003-shared-ui-foundation.md), [TASK-004](TASK-004-observe-components.md), [TASK-005](TASK-005-card-components.md), [TASK-006](TASK-006-query-components.md).  
Mapped obligations: REQ-030, REQ-023, REQ-025, REQ-020, AC-011, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

External dependency: REQ-023 correlation (including verification results) and REQ-025 Service charts, with separately approved platform implementation and actual SDK/MCP read/action projections. This task does not authorize implementation of that platform prerequisite. Contract-independent work may proceed, but fixtures/unavailable controls cannot close it.

## Outcome and Value

Service signal breakdown and versioned chart editing work from real Service identity and chart contracts.

## Owners, Scope, Consumers, and Prohibited Changes

Own SignalBreakdownTable and ChartEditor. Use service_card_uid for deployment scope and card_uid for producer breakdown; never relabel owner_card_uid or use service_name/component UID lists. Save chart emits an intent whose authorized host registers a new Service version. REQ-023 and REQ-025 are external platform work, not UI schema invention.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Service sources | `crates/wyrd-spec/src/card/service.rs::ServiceSpec` currently has tables but no charts; `crates/vala/vala-bifrost-redux/src/tables/gateway/calls.rs::CallsTable` and canonical signal owners lack required service_card_uid | Existing Cards registration/SDK query workflows; TASK-003/004/006 adapters | Service correlation and chart contracts not shipped | Consume separately shipped generated schemas and existing registration/query owners | No new durable UI state or guessed platform fields |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Confirm external correlation and chart contracts and their host projections before live integration.
2. Complete signal breakdown plus chart SQL/bind/schema editing and preview from reused Query/Chart components.
3. Complete new-version save actions and pending/error/denied handling per host.
4. Pass T1–T5 and inspect both components separately.

## Ordered Implementation Scenarios

### Scenario 1 — Isolated Service signals and chart preview

**Behavior.** Shared component telemetry from two Services stays separate; chart preview honors all REQ-025 shapes, range/bucket binds, schema errors and series folding.

**RED.** Add family loader/interaction assertions using contract-backed fixtures; live proof waits for the platform dependency. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-008: Isolated Service signals and chart preview$'
```

**GREEN.** Use real service_card_uid and chart schema; no inferred cross-Service membership. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Authorized versioned chart save

**Behavior.** Supported hosts save through actual registration as the viewer, creating a new Service version; duplicate pending submission is prevented and denied/error remain visible.

**RED.** Add focused action completion and live Service journey after prerequisite delivery. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-008: Authorized versioned chart save$"'
```

**GREEN.** Use current registration authority and generated contract without UI-owned persistence or automatic unsafe retries. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- Both components pass T1–T5, action-host closure and individual visual inspection.
- REQ-025 shape validation and exact SQL/binds are preserved at rendering; no blank chart success on incompatible schema.
- Task cannot complete with a fixture-only Service correlation or unavailable Save action.

## Expected Write Set and Consumer Closure

Package Service components/loaders/catalog/actions, gallery/fixtures/tests and generated projections of delivered prerequisites; no ingest/schema/registration-validator implementation. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` after focused tests. Run `mise run codegen:check` when catalog/generated inputs change. Record T1 states, T2 actual requests/shape, T3 view validation/render, T4 actual built AppBridge HTML, T5 real authorized Node/server data and T6 only for assembled pages. Data-free primitives have no invented loader/live proof. Preserve per-component fixture provenance and interaction/action-host evidence.

### Human visual checkpoint

Service, ServiceCharts and Home signal breakdown. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md).
- [Execution](../../../../changes/implementation-execution.md), [TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
