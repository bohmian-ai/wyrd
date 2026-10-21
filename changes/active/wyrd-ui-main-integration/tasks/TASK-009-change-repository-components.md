# TASK-009 — Change Request and repository component family

Spec: [revision 9, approved](../spec.md). Status: Proposed; platform prerequisite holds completion.  
Dependencies: [TASK-003](TASK-003-shared-ui-foundation.md), [TASK-005](TASK-005-card-components.md), [TASK-007](TASK-007-verify-components.md).  
Mapped obligations: REQ-030, REQ-029, REQ-020, AC-011, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

External dependency: REQ-029 separately approved and shipped CR/repository/webhook/verifier-folder read/write contracts with SDK/MCP projections. This task does not authorize implementation of that platform prerequisite. Contract-independent work may proceed, but fixtures/unavailable controls cannot close it.

## Outcome and Value

Change Request collaboration and repository connection controls are reusable and complete against shipped server contracts.

## Owners, Scope, Consumers, and Prohibited Changes

Own ChangeRequestList, ChangeHeader, SubjectsTable, RevisionList, ActivityFeed, ChangeVerifierList, RequestVerificationForm, ReviewSubmission, Participants, RepositoryList, WyrdTomlView and VerifierFolderTable. Preserve REQ-029 revision causes, multi-PR/repository scope, verification origins/starts, comments/reviews and webhook settings. No Claims, gates, carry-forward, override or browser-owned CR store. Historical mock is visual/behavior reference only.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Change behavior reference | Historical 7bfc3a8a features/changes/Overview.svelte, Verification.svelte, ChangesJourney.test.ts and review-actions.test.ts | Historical routes and server/mock/changes/store.ts; current main lacks CR/repository durable contract | Reusable components and live contract absent | Consume separately specified platform CR/repository contract through normal host boundaries | Do not port mock store or invent its API as production authority |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Wait for approved prerequisite schemas/actions before defining live loader inputs; build only contract-independent presentation in the meantime.
2. Complete list/detail/revision/verification/comment/review and repository components as one collaboration family.
3. Complete typed host action dispatch, scope refusal and revision consistency.
4. Prove T1–T5 and inspect every component individually against the relevant boards.

## Ordered Implementation Scenarios

### Scenario 1 — Revision-aware collaboration

**Behavior.** Components show current-revision subjects, causes, Verifier origins/start/status, comments and participants; revisions never silently mix.

**RED.** Add family state/document/interaction assertions against delivered contract-shaped fixtures. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-009: Revision-aware collaboration$'
```

**GREEN.** Implement pure components and shared loaders over server-owned CR state. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Real authorized collaboration and connections

**Behavior.** Comment/review/request-verification/connect actions complete as each host caller; unauthorized or failed writes remain visible and do not mutate displayed revision optimistically into false success.

**RED.** Add focused real-server family journey and action-host proof once the prerequisite ships. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-009: Real authorized collaboration and connections$"'
```

**GREEN.** Use existing platform action contracts and scopes; no frontend workflow engine. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- All twelve named components pass applicable T1–T5 and individually accepted desktop/mobile states/interactions.
- Server owns every CR revision and connection; fixture-only or unavailable actions keep the task incomplete.
- No removed legacy terms or mock-only policy are reintroduced.

## Expected Write Set and Consumer Closure

Package CR/repository family/loaders/catalog/actions, gallery/fixtures/tests; generated contract consumers only. No new server CR subsystem. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` after focused tests. Run `mise run codegen:check` when catalog/generated inputs change. Record T1 states, T2 actual requests/shape, T3 view validation/render, T4 actual built AppBridge HTML, T5 real authorized Node/server data and T6 only for assembled pages. Data-free primitives have no invented loader/live proof. Preserve per-component fixture provenance and interaction/action-host evidence.

### Human visual checkpoint

Changes, ChangeDetail, ChangeVerification and Repos. Check every component; a combined family approval cannot replace those checks. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md).
- [Execution](../../../../changes/implementation-execution.md), [TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
