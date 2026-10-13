---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-live-data
spec_revision: 3
requirements: [REQ-003, REQ-006, REQ-008, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011]
acceptance: [AC-005, AC-018]
depends_on: [TASK-005]
---

# TASK-006 — Data detail with actual declaration and schema

## Outcome and Value

The Data page renders real metadata/schema and exact versions and follows valid declared relationships.

Owns AC-005, AC-018. AC-018 is a per-page obligation; AC-017 typed consumer/schema evidence also applies.
The preceding task is an acceptance-order dependency: the user requires one page gate before the next. Shared software dependencies are identified in the reuse map; this order does not authorize unrelated rewrites.

## Owners, Scope, Consumers, and Prohibited Changes

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`; server paths are relative to `crates/wyrd/wyrd-server`. Named symbols and tests describe inspected ownership, not a private-file allowlist. Use `wyrd-implement` and, for UI edits, `wyrd-ui`.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Data detail with actual declaration and schema | Shared `WyrdClient.card/CardDetail/buildRegistry`, canonical Data Card spec and get-by-ref routes | Card detail route and `CardDetail.test.ts`; Data declaration/schema presentation; linked Card/version controls | Mock generic Data rendering has not been validated against real registered Data schemas and versions. | Reuse the exact Card read and generic workspace from task 005; correct only Data-specific presentation gaps exposed by the live journey. | None: Data remains a native Card, not a new registry or analytical data browser. |
| Authenticated loading and presentation | `hooks.server.ts`, `BrowserSession.api/context`, existing `+page.server.ts` loads and `WyrdClient` | Production auth integration tests; current route/component consumers; generated operation types | Current mock domain methods do not provide this page’s authoritative asynchronous live data | Extend the current server-only facade/load and retain SvelteKit-generated page types | No duplicate session, transport, contract owner or page-data schema. |

Keep native Card/Verifier identity and exact attribution. Effective tenant/permissions come from the current verified session, never from URL scope; secrets remain server-only. Preserve actual server permission decisions and non-blocking audit staging. The BFF never reads databases/object storage directly or replaces registry/verification/query engines.
Do not add public/persisted contracts, migrations, compatibility routes, signal read APIs, a global service selector, Changes, custom dashboards, raw Query functionality or paid/external provider requirements. Existing foundation tasks are historical context only. The audited extra Changes/dashboard/GenAI/styleguide routes are outside this page-gate scope.

## Required Server and BFF Contracts

[Source-backed route audit](../route-audit.md) defines operation inputs/responses, permissions, structured errors, canonical query schemas and the route-to-task matrix. Reuse those mounted contracts; do not generate types from this planning table or create a parallel `/ui/*` API family.

| Boundary | Required behavior and contract |
|---|---|
| Existing BFF route | GET /t/{tenantKey}/cards/{uid} — shared detail load, Data shell. |
| URL/action inputs | uid/version and linked exact identities. |
| Existing server operations | GET /v1/cards/by-uid/{kind}/{card_uid}; GET /v1/cards/by-ref?kind=…&space=…&name=…&version=… → GetCardResponse; GET /v1/cards/{kind}/{space}/{name}/versions?include_prerelease=… → ListVersionsResponse. Reuse task 005 exact identity/version resolution. |
| Required integration gap | Validate Data-specific fields through the shared exact read; no separate Data HTTP route. |
| Analytical result contract | No dataset row browser or analytical query is authorized by this declaration gate. |

Use generated HTTP operation types and SvelteKit-generated route data types. Protected domain reads remain server-only with current session authority. SvelteKit loads call the facade directly; no internal proxy HTTP hop or page-specific JSON handler is currently required. Browser OAuth redirects/device approval retain their existing protocol paths. Test inputs, returned shape/status/media type, forbidden/missing identities and URL restoration—not merely that a route returns 200.

## Approach

1. Register Data versions with meaningfully different declared schema/metadata through a public SDK.
2. Render them through the shared live detail path and verify fields/relationships against server reads.
3. Exercise direct links, version changes, missing/denied states and SSR/navigation; repair only demonstrated gaps.

## Ordered Implementation Scenarios

### Scenario 1 — Data declaration and version fidelity

**Behavior.** Inventory and direct links display exact Data schema/metadata and selected registered versions. Mapped obligations: REQ-003, REQ-006, REQ-008, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024; AC-005, AC-018.

**RED.** Add the focused browser assertion before any Data-specific repair; if current shared rendering already passes, record verification-only evidence instead of manufacturing failure. First add the smallest failing projection/load/filter assertion to planned focused case `live UI Data detail projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts -t '^live UI Data detail projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI Data detail`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Make only required Data presentation corrections and rerun shared detail proof. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

### Scenario 2 — Data navigation and failure states

**Behavior.** Relationship targets, refresh/back, unknown versions and restricted identities retain correct identity and safe state. Mapped obligations: REQ-003, REQ-006, REQ-008, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024; AC-005, AC-018.

**RED.** Add edge branches before fixing any exposed gap; existing correct branches are regression evidence. First add the smallest failing projection/load/filter assertion to planned focused case `live UI Data detail projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts -t '^live UI Data detail projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI Data detail`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Preserve generic shell and server error mapping while closing any observed defect. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

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

Data-relevant generic detail presentation only if needed, shared registrations and focused browser case.

Close callers, tests, fixtures/setup, package/lock/config and runtime consumers of each changed owner. Shared/private helpers may move only when that earns a smaller cohesive implementation. The existing identity host must discover and forward the planned exact case selector; adding a split test file includes discovery changes so the command cannot select zero tests.
When changing a shared projection or method to asynchronous live IO, update every in-scope load/caller and retain mock/component behavior. Analytical result schemas are not HTTP DTOs: validate canonical columns/types before interpretation and reuse existing SDK decoding.

## Verification and Evidence

Focused RED/GREEN iteration:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts -t '^live UI Data detail projection$'
```

The planned focused case extends the inspected existing component/projection test owner. Existing module setup remains intact; tests needing only typed values/load mapping do not start Keycloak/Postgres.

Page acceptance (run the filtered real browser case after the local behavior is coherent; it must still cover every user-visible scenario):

The following exact journey case is planned under the inspected test owner/lane; introduce its assertions before implementing missing behavior. Correct existing branches are verification-only evidence. This is a mixed task when a generic implementation already satisfies part of the page.

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Data detail' mise run test:identity:journey
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


## Implementation Evidence

This is a verification-only page gate. The TASK-005 shared exact read (`WyrdClient.card`, `liveCard`, `specSections`) renders real Data declarations without any Data-specific change. RED was not manufactured, per the task.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-005 declaration and schema fidelity | Shared `specSections`/`flatten`: every schema column renders under `columns[i].*`, plus interface, stats and `card_refs` ("Declared empty.") | `live UI Data detail projection`; browser `live UI Data detail`: 2 real Data versions with the parquet artifact via `wyrd apply`. 1.0.0 declares column `det-clicked`, Snappy and label `owner=r.okafor`; 2.0.0 declares `det-converted` and Zstd. The pinned 1.0.0 HTML has `columns[0].name`, `feature`, `columns[1].dtype`, `int64`, `Parquet`, `Snappy`, `row_count` and `owner=r.okafor`, and no `Zstd` or `det-converted` | PASS |
| AC-005 versions | Shared by-ref and versions reads | Browser: inventory opens 2.0.0; the version click shows 1.0.0 only; refresh and Back hold | PASS |
| AC-005 relationships | Shared `edges` | Browser: the inbound `Verifier default/det-data-drift@1.0.0` (a drift Verifier whose `baseline_ref` is det-data 1.0.0) opens the Verifier UID page | PASS |
| AC-005 missing and denied | Shared | Browser: unknown version/UID and `not-a-uid` are 404; the other tenant's reader gets 404; no-role gets 403 | PASS |
| AC-018 SSR, nav, no browser `/v1` | Shared server load | Browser: initial HTML, `__data.json` on version click, no `/v1/`, no `__data.json` on reload | PASS |

Commands (all passed):
```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Data detail' mise run test:identity:journey   # 1 passed, 10 skipped
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts   # 11 passed
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check   # 0 errors
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run build   # run by the journey lane
```
- Generated types are as in TASK-005.
- No analytical query or row browser was added, per the non-goal.

Usability pass (`target/debug/wyrd-ui-live-login/data-*.png`, local only):
- The schema reads as one indexed key list per column field. It is legible at 2 columns; a wide schema would be long but complete.
- A columnar schema table would be a Data-specific workspace, which was declined because the generic shell renders every declared field.

Non-goals stayed excluded: there is no production change in this task; the browser case and `mise.toml` selector landed in the TASK-005 commit.
