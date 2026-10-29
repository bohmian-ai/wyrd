# TASK-007 — Generic verification component family

Spec: [revision 11, approved](../spec.md). Status: Proposed; platform prerequisite holds completion.
Dependencies: [TASK-003](TASK-003-shared-ui-foundation.md), [TASK-004](TASK-004-observe-components.md), [TASK-005](TASK-005-card-components.md).
Mapped obligations: REQ-030, REQ-018, REQ-026, REQ-027, REQ-020, AC-009, AC-011, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

External dependency: Separately specified non-sensitive requirement-outcome read contract; REQ-029 for AttentionList CR rows. Current sensitive outcome is not reclassified here. This task does not authorize implementation of that platform prerequisite. Contract-independent work may proceed, but fixtures/unavailable controls cannot close it.

## Outcome and Value

Generic Verifier definitions and authoritative recorded run/session/drift results can be rendered and investigated through reusable components.

## Owners, Scope, Consumers, and Prohibited Changes

Own VerifierGraph, VerificationResult, RequirementDetail, DriftFeatureTable, VerificationBindings and AttentionList. Resolve selected result through execution readback before coloring its exact Verifier definition. Separate `subject_card_uid`, `verifier_card_uid`, `execution_id`, application `run_id` and binding root `workload_card_uid`. No new session model, evidence upload read or frontend judgment calculation.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Recorded verification | `sdks/wyrd-sdk-ts/wyrd/src/index.ts::verify.Execution.status`; `crates/wyrd-spec/src/verification/execution.rs::VerificationExecutionState`; `crates/vala/vala-bifrost-redux/src/tables/verification/results.rs::ResultsTable` | `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts`; server `tests/integration/pg_verification_groups.rs` | Component presentation and selected-result resolution missing | Existing execution readback, exact Verifier Card and retained results | No group lifecycle, judgment computation or evidence-upload read |
| Drift detail | `crates/vala/vala-bifrost-redux/src/tables/drift/result_features.rs::ResultFeaturesTable`; `crates/wyrd-spec/src/card/verifier/mod.rs::VerifierSpec` | SDK verifier-building-blocks/verify-in-real-time sources; results table sensitive outcome policy | Reusable graph, feature projection and requirement detail missing | Join by result_id/requirement_id and exact baseline_ref; preserve sensitive reads until approved prerequisite | No baseline distribution or new drift Card |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Build the definition graph from declared inputs, dependencies, output reads and requirements.
2. Add result and requirement projection with current execution selected-result and pinned GroupSnapshot semantics.
3. Complete drift/bindings/attention interactions and typed navigation actions.
4. Prove T1–T5 and individual gallery checks; keep unmet platform variants explicitly incomplete.

## Ordered Implementation Scenarios

### Scenario 1 — Definition and result identities

**Behavior.** Plain graph and result-colored graph share exact VerifierSpec; later group reviews remain distinct, scoped to owner/tenant, and pinned membership excludes abandoned results and unrelated same-key groups.

**RED.** Add family graph/identity loader tests; shared result components are absent. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-007: Definition and result identities$'
```

**GREEN.** Read execution, selected result and exact Verifier in dependent stages; reuse current group contracts. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Recorded outcomes and drift detail

**Behavior.** Requirement reasons/actual/citations/judge and all feature scores/thresholds/verdicts are recorded facts; drifted features first, window navigation correct, no computed requirement verdict.

**RED.** Add component projection and host navigation assertions, including denied sensitive outcome and missing detail. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-007: Recorded outcomes and drift detail$'
```

**GREEN.** Consume actual authoritative reads; requirement and Service variants wait for their external contracts. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 3 — Real authorized generic verification

**Behavior.** T1–T5 show populated/empty/denied against real produced results and scheduled drift; unauthorized payload or tenant access never becomes empty/mock success.

**RED.** Add focused real SDK verification-to-component journey and built-artifact proof. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-007: Real authorized generic verification$"'
```

**GREEN.** Use registered bindings/verification runtime and retained result readback, never public writes to protected result tables. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- Every named component is complete with per-component evidence; sensitive details are never exposed by a standard read grant.
- Group quiet-period/review behavior uses current main and no replacement platform work.
- Non-sensitive per-requirement outcomes, Service filter data and CR attention rows hold their applicable completion gates until real contracts ship.

## Expected Write Set and Consumer Closure

Package Verify family/loaders/catalog, generated wire roots, gallery/fixtures/tests; existing execution/Card reads only. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` after focused tests. Run `mise run codegen:check` when catalog/generated inputs change. Record T1 states, T2 actual requests/shape, T3 view validation/render, T4 actual built AppBridge HTML, T5 real authorized Node/server data and T6 only for assembled pages. Data-free primitives have no invented loader/live proof. Preserve per-component fixture provenance and interaction/action-host evidence.

### Human visual checkpoint

Verifier, Verify, VerifySession, VerifyDrift, Data, Agent, Home and Service uses; individual graph/result/requirement/drift checks. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md).
- [Execution](../../../../changes/implementation-execution.md), [TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
