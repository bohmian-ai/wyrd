---
id: TASK-017
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-live-data
spec_revision: 3
requirements: [REQ-003, REQ-008, REQ-009, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011]
acceptance: [AC-014, AC-018]
depends_on: [TASK-016]
---

# TASK-017 — Live drift baselines, reports and feature history

## Outcome and Value

Users distinguish healthy/shifted evidence and investigate feature history while missing baseline/insufficient data/failed execution remain visibly nonhealthy.

Owns AC-014, AC-018. AC-018 is a per-page obligation; AC-017 typed consumer/schema evidence also applies.
The preceding task is an acceptance-order dependency: the user requires one page gate before the next. Shared software dependencies are identified in the reuse map; this order does not authorize unrelated rewrites.

## Owners, Scope, Consumers, and Prohibited Changes

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`; server paths are relative to `crates/wyrd/wyrd-server`. Named symbols and tests describe inspected ownership, not a private-file allowlist. Use `wyrd-implement` and, for UI edits, `wyrd-ui`.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Live drift baselines, reports and feature history | `WyrdClient.observeDrift`, drift server load/charts; `ResultsTable/ResultFeaturesTable`; existing Drift binding/runtime/baseline owners | `ObserveJourney.test.ts`; SDK scheduled-drift/real-time verification journeys; feature/Verifier/service/time selectors | Mock drift ignores some scope; real baseline readiness, drift reports and nullable feature outcomes need live proof. | Register supported Drift bindings and fit real baselines; read canonical retained reports/features through the existing analytical path. | None: no drift engine, baseline store, new alert resource or external notification requirement. |
| Authenticated loading and presentation | `hooks.server.ts`, `BrowserSession.api/context`, existing `+page.server.ts` loads and `WyrdClient` | Production auth integration tests; current route/component consumers; generated operation types | Current mock domain methods do not provide this page’s authoritative asynchronous live data | Extend the current server-only facade/load and retain SvelteKit-generated page types | No duplicate session, transport, contract owner or page-data schema. |

Keep native Card/Verifier identity and exact attribution. Effective tenant/permissions come from the current verified session, never from URL scope; secrets remain server-only. Preserve actual server permission decisions and non-blocking audit staging. The BFF never reads databases/object storage directly or replaces registry/verification/query engines.
Do not add public/persisted contracts, migrations, compatibility routes, signal read APIs, a global service selector, Changes, custom dashboards, raw Query functionality or paid/external provider requirements. Existing foundation tasks are historical context only. The audited extra Changes/dashboard/GenAI/styleguide routes are outside this page-gate scope.

## Required Server and BFF Contracts

[Source-backed route audit](../route-audit.md) defines operation inputs/responses, permissions, structured errors, canonical query schemas and the route-to-task matrix. Reuse those mounted contracts; do not generate types from this planning table or create a parallel `/ui/*` API family.

| Boundary | Required behavior and contract |
|---|---|
| Existing BFF route | GET /t/{tenantKey}/observe/drift — existing server load. |
| URL/action inputs | service/range (default 30d)/driftCard/feature; complete supported history pagination. |
| Existing server operations | POST /v1/query; exact Verifier/owner Card reads; GET /v1/verification/bindings/{binding_id} and GET /v1/verification/runs/{run_id} where declared/known. |
| Required integration gap | All supported selectors affect real reads. Baseline/readiness is control/declaration state, not an invented healthy inference from absent reports. |
| Analytical result contract | Q-RESULTS/Q-DRIFT: completed report and vala.drift.result_features joined by result_id; exact owner/subject/binding/Verifier/window/feature scope. Optional observation series from vala.drift.observations. Missing baseline/insufficient data/failed execution stay nonhealthy. |

Use generated HTTP operation types and SvelteKit-generated route data types. Protected domain reads remain server-only with current session authority. SvelteKit loads call the facade directly; no internal proxy HTTP hop or page-specific JSON handler is currently required. Browser OAuth redirects/device approval retain their existing protocol paths. Test inputs, returned shape/status/media type, forbidden/missing identities and URL restoration—not merely that a route returns 200.

## Approach

1. Use public registrations and actual supported runtime to fit baselines and compare deterministic healthy/shifted observations.
2. Wait with bounded readiness for baseline and retained reports; reuse existing setup lifecycle rather than inserting results or replacing scheduling.
3. Map exact result/Verifier/subject/binding/window/feature identity and nullable scores/thresholds.
4. Connect current selectors/history/charts/pagination and linked evidence with truthful empty/nonhealthy states.

## Ordered Implementation Scenarios

### Scenario 1 — Healthy and shifted comparisons

**Behavior.** Real baseline-backed comparisons generate retained feature judgments; changing service/Verifier/window/feature updates the expected report/history. Mapped obligations: REQ-003, REQ-008, REQ-009, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024; AC-014, AC-018.

**RED.** The planned live acceptance case must fail on fixture reports or ignored selectors. First add the smallest failing projection/load/filter assertion to planned focused case `live UI drift projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts -t '^live UI drift projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI drift`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Read canonical result/report/feature data and reuse existing runtime setup/public workflows. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

### Scenario 2 — Baseline and execution edge states

**Behavior.** Missing baseline, insufficient observations and failed executions never render healthy; absent scores/thresholds stay absent and missing/denied selectors are safe. Mapped obligations: REQ-003, REQ-008, REQ-009, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024; AC-014, AC-018.

**RED.** Add these real execution/control and browser states before repairing mapping. First add the smallest failing projection/load/filter assertion to planned focused case `live UI drift projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts -t '^live UI drift projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI drift`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Represent supported baseline/control evidence and unavailable results accurately; do not infer judgments from silence. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

## Acceptance Criteria

- Both ordered scenarios pass using the applicable owners, including exclusions and edge/failure branches; the separate live page acceptance gate proves all user-visible branches against real data.
- Every required BFF input reaches the identified upstream operation/query with correct scope; generated response/schema mapping and denied/missing/error behavior are recorded in the page evidence.
- The page is accepted independently against SDK/HTTP/Bifrost evidence in the shared disposable setup with mock identity/content disabled. Domain evidence is registered/emitted/evaluated through public pathways, never inserted as prepared results.
- Primary content or truthful empty/error state exists in initial HTML before hydration. URL controls invoke SvelteKit server loads; hydration does not redundantly fetch initial data. Protected upstream calls remain server-only. Preloads are read-only/bounded and every navigation revalidates session authority.
- Context is visible, changeable and clearable where applicable. Continuation links preserve applicable service/time/selection state; relationship links select the target’s identity. Filters, pagination and counts agree with authoritative reads under equivalent settled scope.
- Missing, denied, unavailable and schema-invalid data never silently falls back to fixtures. Work remains bounded and supported terminal/visibility semantics are preserved.
- Record a human usability pass or concrete corrected issues with scrubbed local screenshots; review control discoverability, scope clarity, back/refresh behavior, accessible interaction and error recovery.

## Expected Write Set and Consumer Closure

Drift load/projection/history/charts/filters, supported baseline/comparison workload and browser case; engine/storage/public contracts unchanged.

Close callers, tests, fixtures/setup, package/lock/config and runtime consumers of each changed owner. Shared/private helpers may move only when that earns a smaller cohesive implementation. The existing identity host must discover and forward the planned exact case selector; adding a split test file includes discovery changes so the command cannot select zero tests.
When changing a shared projection or method to asynchronous live IO, update every in-scope load/caller and retain mock/component behavior. Analytical result schemas are not HTTP DTOs: validate canonical columns/types before interpretation and reuse existing SDK decoding.

## Verification and Evidence

Focused RED/GREEN iteration:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts -t '^live UI drift projection$'
```

The planned focused case extends the inspected existing component/projection test owner. Existing module setup remains intact; tests needing only typed values/load mapping do not start Keycloak/Postgres.

Page acceptance (run the filtered real browser case after the local behavior is coherent; it must still cover every user-visible scenario):

The following exact journey case is planned under the inspected test owner/lane; introduce its assertions before implementing missing behavior. Correct existing branches are verification-only evidence. This is a mixed task when a generic implementation already satisfies part of the page.

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI drift' mise run test:identity:journey
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run build
```

Run `mise run codegen:check` when HTTP generation/consumption changes, and record which generated operation types the page consumes. Identify canonical analytical columns/types and prove schema/terminal mapping where used.
For actual Rust changes, run `mise run fmt` and `mise run lints`; for changed dependency/owner boundaries run `mise run check:deps`. Add only the narrow owning format/lint/test checks justified by the actual write set.
Do not run full identity/SDK journey suites or a broad aggregate as this task’s gate. Integrated verification belongs once to change review. Existing authentication/audit behavior needs regression proof, not manufactured RED.
Retain initial-response/network evidence, expected data identities and exclusions, readiness bounds and a scrubbed usability review. Record page pass/fail and mapped obligations; do not claim an isolated page proves the whole change.

## Material Stop Conditions

Stop for specification revision if the required outcome genuinely needs a new public/persisted contract, migration, permission/audience change, durability/freshness guarantee, engine behavior or ownership exception. Do not silently substitute a new read API or unbounded scan for a missing contract.
Report a concrete blocker if repository-managed setup cannot supply the required real evidence safely or a canonical schema cannot support truthful required presentation. Reversible private mapping/tooling choices remain implementation-owned; correct mock assumptions rather than inventing domain state.

## Authority Links

- [Server/BFF route audit](../route-audit.md)

- [Approved specification, revision 3](../spec.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Protocol doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

