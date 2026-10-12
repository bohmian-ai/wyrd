---
id: TASK-013
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-live-data
spec_revision: 3
requirements: [REQ-003, REQ-008, REQ-009, REQ-011, REQ-017, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011]
acceptance: [AC-011, AC-018]
depends_on: [TASK-012]
---

# TASK-013 — Live logs facets, search, pagination and detail

## Outcome and Value

Users find actual exported logs, inspect matching details and follow correlation where it exists.

Owns AC-011, AC-018. AC-018 is a per-page obligation; AC-017 typed consumer/schema evidence also applies.
The preceding task is an acceptance-order dependency: the user requires one page gate before the next. Shared software dependencies are identified in the reuse map; this order does not authorize unrelated rewrites.

## Owners, Scope, Consumers, and Prohibited Changes

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`; server paths are relative to `crates/wyrd/wyrd-server`. Named symbols and tests describe inspected ownership, not a private-file allowlist. Use `wyrd-implement` and, for UI edits, `wyrd-ui`.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Live logs facets, search, pagination and detail | `WyrdClient.observeLogs`, logs server load/components; `RecordsTable/LOG_FIELDS` | `ObserveJourney.test.ts`, logs page/record selectors and trace links; SDK OTEL signal journey owners | Mock log facets/pages need real records, optional correlations and stable truthful record selection. | Read canonical log records through the existing analytical path and retain existing URL/page controls. | None: no persisted log ID or new log registry. A private selection representation must respect the actual record grain. |
| Authenticated loading and presentation | `hooks.server.ts`, `BrowserSession.api/context`, existing `+page.server.ts` loads and `WyrdClient` | Production auth integration tests; current route/component consumers; generated operation types | Current mock domain methods do not provide this page’s authoritative asynchronous live data | Extend the current server-only facade/load and retain SvelteKit-generated page types | No duplicate session, transport, contract owner or page-data schema. |

Keep native Card/Verifier identity and exact attribution. Effective tenant/permissions come from the current verified session, never from URL scope; secrets remain server-only. Preserve actual server permission decisions and non-blocking audit staging. The BFF never reads databases/object storage directly or replaces registry/verification/query engines.
Do not add public/persisted contracts, migrations, compatibility routes, signal read APIs, a global service selector, Changes, custom dashboards, raw Query functionality or paid/external provider requirements. Existing foundation tasks are historical context only. The audited extra Changes/dashboard/GenAI/styleguide routes are outside this page-gate scope.

## Required Server and BFF Contracts

[Source-backed route audit](../route-audit.md) defines operation inputs/responses, permissions, structured errors, canonical query schemas and the route-to-task matrix. Reuse those mounted contracts; do not generate types from this planning table or create a parallel `/ui/*` API family.

| Boundary | Required behavior and contract |
|---|---|
| Existing BFF route | GET /t/{tenantKey}/observe/logs — existing server load. |
| URL/action inputs | service/range/level/trace/q/record/page/per. |
| Existing server operations | POST /v1/query through SDK; no log-specific Rust or BFF JSON endpoint. |
| Required integration gap | Await live reads; records/facets/count/page-size/detail use equivalent scope. record is private presentation selection, not a persisted log_id. |
| Analytical result contract | Q-LOGS: vala.logs.records severity/body/resource/time and optional binary trace/span identities. Stable settled pagination must retain repeated equal-valued records rather than deduplicating evidence. |

Use generated HTTP operation types and SvelteKit-generated route data types. Protected domain reads remain server-only with current session authority. SvelteKit loads call the facade directly; no internal proxy HTTP hop or page-specific JSON handler is currently required. Browser OAuth redirects/device approval retain their existing protocol paths. Test inputs, returned shape/status/media type, forbidden/missing identities and URL restoration—not merely that a route returns 200.

## Approach

1. Emit distinct correlated and uncorrelated logs for both Services using public OTEL support and normal flush/shutdown.
2. Map severity/resource/body/optional IDs from the actual schema and validate supported facets/search.
3. Implement deterministic settled pagination/page-size and contextual detail/trace links through server loading.

## Ordered Implementation Scenarios

### Scenario 1 — Actual records and optional correlation

**Behavior.** Expected severities/bodies/resources appear under service/time/search/trace selectors; uncorrelated records remain usable. Mapped obligations: REQ-003, REQ-008, REQ-009, REQ-011, REQ-017, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024; AC-011, AC-018.

**RED.** The planned live acceptance case must detect fixture records or assumed required correlation. First add the smallest failing projection/load/filter assertion to planned focused case `live UI logs projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts -t '^live UI logs projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI logs`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Map canonical log schema through the shared facade and current components. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

### Scenario 2 — Paging and safe detail selection

**Behavior.** Pages cover matching retained records without overlaps, page-size/scope changes behave coherently, and direct record selection never bypasses active scope/access. Mapped obligations: REQ-003, REQ-008, REQ-009, REQ-011, REQ-017, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024; AC-011, AC-018.

**RED.** Add multi-page, unknown record and restricted-user checks before repairing paging/selection. First add the smallest failing projection/load/filter assertion to planned focused case `live UI logs projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts -t '^live UI logs projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI logs`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Keep selection private and deterministic without inventing persisted identity; preserve URL-backed controls. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

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

Logs load/projection/facets/pagination/detail links, signal emission support and browser case.

Close callers, tests, fixtures/setup, package/lock/config and runtime consumers of each changed owner. Shared/private helpers may move only when that earns a smaller cohesive implementation. The existing identity host must discover and forward the planned exact case selector; adding a split test file includes discovery changes so the command cannot select zero tests.
When changing a shared projection or method to asynchronous live IO, update every in-scope load/caller and retain mock/component behavior. Analytical result schemas are not HTTP DTOs: validate canonical columns/types before interpretation and reuse existing SDK decoding.

## Verification and Evidence

Focused RED/GREEN iteration:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts -t '^live UI logs projection$'
```

The planned focused case extends the inspected existing component/projection test owner. Existing module setup remains intact; tests needing only typed values/load mapping do not start Keycloak/Postgres.

Page acceptance (run the filtered real browser case after the local behavior is coherent; it must still cover every user-visible scenario):

The following exact journey case is planned under the inspected test owner/lane; introduce its assertions before implementing missing behavior. Correct existing branches are verification-only evidence. This is a mixed task when a generic implementation already satisfies part of the page.

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI logs' mise run test:identity:journey
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

