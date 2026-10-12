---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-live-data
spec_revision: 3
requirements: [REQ-003, REQ-006, REQ-008, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011]
acceptance: [AC-004, AC-018]
depends_on: [TASK-004]
---

# TASK-005 — Model detail and shared exact Card reads

## Outcome and Value

A real Model opens directly and from inventory, shows authoritative declaration/metadata, and switches to an exact registered version.

Owns AC-004, AC-018. AC-018 is a per-page obligation; AC-017 typed consumer/schema evidence also applies.
The preceding task is an acceptance-order dependency: the user requires one page gate before the next. Shared software dependencies are identified in the reuse map; this order does not authorize unrelated rewrites.

## Owners, Scope, Consumers, and Prohibited Changes

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`; server paths are relative to `crates/wyrd/wyrd-server`. Named symbols and tests describe inspected ownership, not a private-file allowlist. Use `wyrd-implement` and, for UI edits, `wyrd-ui`.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Model detail and shared exact Card reads | UI `WyrdClient.card`, `projectCard`, `CardDetail`, `buildRegistry`; server `get_card_http/get_card_by_ref_http/list_versions_http` | Card UID `+page.server.ts`, `CardDetail.test.ts`, generated GetCardResponse and versions contract; inventory links | Generic detail rendering exists but live kind-qualified identity/exact-version resolution is absent; version metadata in mocks is richer than server responses. | Extend existing Card reads and generic shell with bounded identity resolution and exact by-reference version retrieval. | No new lookup endpoint or whole-registry scan. Existing by-UID/by-ref/version operations support the workflow; generic workspace fallback already exists. |
| Authenticated loading and presentation | `hooks.server.ts`, `BrowserSession.api/context`, existing `+page.server.ts` loads and `WyrdClient` | Production auth integration tests; current route/component consumers; generated operation types | Current mock domain methods do not provide this page’s authoritative asynchronous live data | Extend the current server-only facade/load and retain SvelteKit-generated page types | No duplicate session, transport, contract owner or page-data schema. |

Keep native Card/Verifier identity and exact attribution. Effective tenant/permissions come from the current verified session, never from URL scope; secrets remain server-only. Preserve actual server permission decisions and non-blocking audit staging. The BFF never reads databases/object storage directly or replaces registry/verification/query engines.
Do not add public/persisted contracts, migrations, compatibility routes, signal read APIs, a global service selector, Changes, custom dashboards, raw Query functionality or paid/external provider requirements. Existing foundation tasks are historical context only. The audited extra Changes/dashboard/GenAI/styleguide routes are outside this page-gate scope.

## Required Server and BFF Contracts

[Source-backed route audit](../route-audit.md) defines operation inputs/responses, permissions, structured errors, canonical query schemas and the route-to-task matrix. Reuse those mounted contracts; do not generate types from this planning table or create a parallel `/ui/*` API family.

| Boundary | Required behavior and contract |
|---|---|
| Existing BFF route | GET /t/{tenantKey}/cards/{uid} — shared detail server load, Model shell. |
| URL/action inputs | uid and optional version; outgoing relationship target identity. |
| Existing server operations | GET /v1/cards/by-uid/{kind}/{card_uid}; GET /v1/cards/by-ref?kind=…&space=…&name=…&version=… → GetCardResponse. GET /v1/cards/{kind}/{space}/{name}/versions?include_prerelease=… → ListVersionsResponse. |
| Required integration gap | Await exact live Card reads. Current URL omits kind; resolve it with bounded existing reads, not a new lookup route/registry scan. Retain the existing URL; encode route parameters safely. Version response lacks mock author/note/time data. |
| Analytical result contract | No telemetry query is necessary for the Model declaration gate; preserve valid contextual Observe links. |

Use generated HTTP operation types and SvelteKit-generated route data types. Protected domain reads remain server-only with current session authority. SvelteKit loads call the facade directly; no internal proxy HTTP hop or page-specific JSON handler is currently required. Browser OAuth redirects/device approval retain their existing protocol paths. Test inputs, returned shape/status/media type, forbidden/missing identities and URL restoration—not merely that a route returns 200.

## Approach

1. Resolve existing UID links using bounded supported kind-qualified reads; retain resolved identity for this request only as needed.
2. Read exact selected version and its server relationships/status; adapt version presentation to actual response metadata.
3. Keep generic detail rendering and outgoing context; verify SSR and client version changes.

## Ordered Implementation Scenarios

### Scenario 1 — Exact identity and versions

**Behavior.** Two Model versions render their own declarations, metadata and linked identities; direct UID navigation works. Mapped obligations: REQ-003, REQ-006, REQ-008, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024; AC-004, AC-018.

**RED.** The planned live acceptance case must fail on the live facade or selected-version fixture projection. First add the smallest failing projection/load/filter assertion to planned focused case `live UI Model detail projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts -t '^live UI Model detail projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI Model detail`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Connect existing operations to the shared detail shell and typed page data. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

### Scenario 2 — Missing, denied and linked identities

**Behavior.** Unknown UID/version and denied reads show safe truthful states; relationship links select the target identity rather than the previous Card. Mapped obligations: REQ-003, REQ-006, REQ-008, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024; AC-004, AC-018.

**RED.** Extend the focused case with these identity failures and links; unsupported resolution or stale context fails. First add the smallest failing projection/load/filter assertion to planned focused case `live UI Model detail projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts -t '^live UI Model detail projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI Model detail`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Bound discovery/fan-out and preserve existing structured server errors and navigation semantics. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

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

Shared Card facade/detail load, generic shell/version presentation, Model browser case and public SDK registrations; no dedicated workspace unless existing shell demonstrably cannot render required fields.

Close callers, tests, fixtures/setup, package/lock/config and runtime consumers of each changed owner. Shared/private helpers may move only when that earns a smaller cohesive implementation. The existing identity host must discover and forward the planned exact case selector; adding a split test file includes discovery changes so the command cannot select zero tests.
When changing a shared projection or method to asynchronous live IO, update every in-scope load/caller and retain mock/component behavior. Analytical result schemas are not HTTP DTOs: validate canonical columns/types before interpretation and reuse existing SDK decoding.

## Verification and Evidence

Focused RED/GREEN iteration:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts -t '^live UI Model detail projection$'
```

The planned focused case extends the inspected existing component/projection test owner. Existing module setup remains intact; tests needing only typed values/load mapping do not start Keycloak/Postgres.

Page acceptance (run the filtered real browser case after the local behavior is coherent; it must still cover every user-visible scenario):

The following exact journey case is planned under the inspected test owner/lane; introduce its assertions before implementing missing behavior. Correct existing branches are verification-only evidence. This is a mixed task when a generic implementation already satisfies part of the page.

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Model detail' mise run test:identity:journey
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts
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

