---
id: TASK-009
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-live-data
spec_revision: 4
requirements: [REQ-003, REQ-007, REQ-008, REQ-009, REQ-016, REQ-017, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024, REQ-025, REQ-026]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011]
acceptance: [AC-008, AC-017, AC-018]
depends_on: [TASK-008]
addresses_plan_findings: [PR-001]
---

# TASK-009 — Live Service composition and analytical BFF integration

## Outcome and Value

Each real Service shows exact components and truthful operational evidence, and its investigative links scope subsequent pages correctly.

Owns AC-008, AC-017, AC-018. AC-018 is a per-page obligation; AC-017 typed consumer/schema evidence also applies.
The preceding task is an acceptance-order dependency: the user requires one page gate before the next. Shared software dependencies are identified in the reuse map; this order does not authorize unrelated rewrites.

## Owners, Scope, Consumers, and Prohibited Changes

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`; server paths are relative to `crates/wyrd/wyrd-server`. Named symbols and tests describe inspected ownership, not a private-file allowlist. Use `wyrd-implement` and, for UI edits, `wyrd-ui`.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Live Service composition and analytical BFF integration | `service-state.ts::readServiceScope/observeHref`, Service workspace, `WyrdClient`; SDK `WyrdClient.connect/Bifrost.connect/stream/sql`; shared Rust Bifrost client | `ServiceWorkspace.test.ts`; service Card/status relationships; SDK support-desk/OTEL/verification journeys; official Dockerfile and nginx runtime | Service mock composition/health must become authoritative; production BFF lacks canonical analytical client use and runtime packaging. | Resolve exact declared composition via existing Card operations and query retained evidence with the existing first-class SDK/shared Bifrost transport under the current private session. | Adding the shipped SDK dependency is justified because the UI has no Arrow/gRPC decoder. Do not implement a decoder, second transport, per-signal API, lifecycle owner or request credential cache. |
| Authenticated loading and presentation | `hooks.server.ts`, `BrowserSession.api/context`, existing `+page.server.ts` loads and `WyrdClient` | Production auth integration tests; current route/component consumers; generated operation types | Current mock domain methods do not provide this page’s authoritative asynchronous live data | Extend the current server-only facade/load and retain SvelteKit-generated page types | No duplicate session, transport, contract owner or page-data schema. |
| Shipped analytical BFF runtime | `docker/official/Dockerfile`, `docker/official/extras/entrypoint.sh`; `scripts/server/test-startup-prod.sh` and `startup-common.sh`; `mise.toml::test:server:startup:prod` | Official-image startup write/verify callers in `crates/shared/wyrd-client/tests/integration/startup_image_journey.rs`; UI host integration tests; SDK native package/platform dependencies | Startup checks readiness, unauthenticated HTML and Rust SDK reads; host-built UI checks cannot prove the pruned native SDK loads or connects inside the deployed Node image | Add one selectable authenticated analytical BFF smoke to the existing production startup owner, using the real image, public evidence writes and established BFF sign-in | No new image/environment controller or test family. Existing startup ownership builds by immutable image ID and owns Postgres, serving processes and cleanup; only the missing BFF assertion/selection support is needed. |

Keep native Card/Verifier identity and exact attribution. Effective tenant/permissions come from the current verified session, never from URL scope; secrets remain server-only. Preserve actual server permission decisions and non-blocking audit staging. The BFF never reads databases/object storage directly or replaces registry/verification/query engines.
Do not add public/persisted contracts, migrations, compatibility routes, signal read APIs, a global service selector, Changes, custom dashboards, raw Query functionality or paid/external provider requirements. Existing foundation tasks are historical context only. The audited extra Changes/dashboard/GenAI/styleguide routes are outside this page-gate scope.

## Required Server and BFF Contracts

[Source-backed route audit](../route-audit.md) defines operation inputs/responses, permissions, structured errors, canonical query schemas and the route-to-task matrix. Reuse those mounted contracts; do not generate types from this planning table or create a parallel `/ui/*` API family.

| Boundary | Required behavior and contract |
|---|---|
| Existing BFF route | GET /t/{tenantKey}/cards/{uid} — shared load with existing Service workspace. |
| URL/action inputs | version, view (overview/composition/definition), range, sel. Existing state is mock-only and must not set live health. |
| Existing server operations | S-CARD-UID/S-CARD-REF/S-VERSIONS; GET /v1/verification/bindings/{binding_id} → VerificationBindingStatus; GET /v1/verification/runs/{run_id} → VerificationRunStatus for known declared control IDs; POST /v1/query → canonical stream via SDK. |
| Required integration gap | The current shared load reads only version and calls card(uid,version). Make operational selectors participate in server loading/typed facade input so navigation does not retain stale evidence. Resolve exact declared composition and bounded bindings; package the shipped SDK/native/runtime path. |
| Analytical result contract | Q-TRACES/Q-METRICS/Q-RESULTS for supported operational charts/summaries. Result service scope uses exact declared owner/subject/binding attribution; telemetry service_name is a distinct resource filter. |

Use generated HTTP operation types and SvelteKit-generated route data types. Protected domain reads remain server-only with current session authority. SvelteKit loads call the facade directly; no internal proxy HTTP hop or page-specific JSON handler is currently required. Browser OAuth redirects/device approval retain their existing protocol paths. Test inputs, returned shape/status/media type, forbidden/missing identities and URL restoration—not merely that a route returns 200.

## Approach

1. Connect server-only analytical access through the current session and existing SDK; preserve actual token audience/object/column permissions and bounded cancellation/deadlines.
2. Validate analytical schemas before mapping values; handle terminal success/degraded/failure and serialize supported large integers/timestamps safely.
3. Close package/lock, native SDK availability and production Node/image runtime consumers together.
4. Extend shared workload setup with distinct actual executions for two Services, normal export/shutdown and bounded visibility checks; reuse existing deterministic provider/verification support. Revision 4 (REQ-026): the setup takes the number of Services as an input. It defaults to the two this page gate needs; TASK-019 drives the same setup at twenty. Service identities, names and evidence derive from that count rather than being hard-coded to two, and its readiness checks wait for the whole requested population.
5. Resolve selected Service version/composition and truthful operational sections; preserve service-state URL links.
6. Extend the existing official-image production startup proof with a focused authenticated analytical BFF case, preserving the default startup cases and their assertions.

## Ordered Implementation Scenarios

### Scenario 1 — Composition and scoped operational reads

**Behavior.** Two Services sharing some components still show exact versioned composition and only selected service/time evidence; clearing scope broadens authorized evidence. Mapped obligations: REQ-003, REQ-007, REQ-008, REQ-009, REQ-016, REQ-017, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024, REQ-025; AC-008, AC-017, AC-018.

**RED.** The planned live acceptance case is expected to fail because production domain/analytical reads are unconnected. First add the smallest failing projection/load/filter assertion to planned focused case `live UI Service workspace projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/service/ServiceWorkspace.test.ts -t '^live UI Service workspace projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI Service workspace`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Use existing Card relationships and canonical Bifrost client queries with truthful presentation. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

### Scenario 2 — Schema, authority and query failures

**Behavior.** Wrong/missing columns, denied query objects, degraded/error terminals and unavailable upstreams produce explicit safe states, release resources and never substitute mock health. Mapped obligations: REQ-003, REQ-007, REQ-008, REQ-009, REQ-016, REQ-017, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024, REQ-025; AC-008, AC-017, AC-018.

**RED.** Add focused fault/schema branches at the BFF seam plus restricted-browser assertions before repairing mappings. First add the smallest failing projection/load/filter assertion to planned focused case `live UI Service workspace projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/service/ServiceWorkspace.test.ts -t '^live UI Service workspace projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI Service workspace`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Keep schema checks/projection in the existing facade and transport/lifecycle in the SDK; package the same path in production. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

### Scenario 3 — Authenticated analytical read through the shipped image

**Behavior.** The official image's production Node BFF loads its installed/pruned SDK and performs an authenticated, schema-checked, terminal-safe analytical read using the deployment's real endpoint configuration. An existing Service operational page renders known retained evidence emitted through public SDK/OTLP workflows, with mock identity/content disabled. Mapped obligations: REQ-003, REQ-009, REQ-017–019; INV-003–004, INV-006–008, INV-011; AC-008, AC-018. Addresses independent plan finding PR-001.

**RED.** Add the planned startup case `authenticated_analytical_bff` to the inspected `scripts/server/test-startup-prod.sh` ownership before repairing image/native/configuration gaps. Introduce exact case selection through the existing lane; the selector below is planned and does not work until this task implements it. Require exactly one selected case, and fail unknown selectors or a skipped/missing case. Run:

```bash
WYRD_STARTUP_FILTER=authenticated_analytical_bff mise run test:server:startup:prod
```

Expect failure from the missing live BFF/native packaging/connection behavior, not merely test discovery or setup. Build and run the actual official image rather than a host Node process. Complete the minimal real migration/setup/evidence/sign-in prerequisites; the focused invocation does not run the unrelated migration-refusal/retry cases. A successful native import, readiness check, unauthenticated page or direct Rust query alone is insufficient RED/GREEN proof for the BFF path.

**GREEN.** Close the production dependency/native binary/Node runtime and HTTP/gRPC endpoint configuration consumers necessary for the existing authenticated load to return the known evidence. Use the established BFF sign-in/session pathway with production cookie behavior and trusted local HTTPS where required; existing API-key recovery sign-in is sufficient for this packaging smoke because task 002 separately owns human SSO. Keep credentials server-only, write domain evidence through public clients, and use bounded visibility readiness. Assert actual analytical identities/values and service/time exclusions in the existing page's initial rendered response, not just HTTP 200. Preserve schema/terminal error handling and verify that an unavailable analytical dependency produces a truthful failure without fixture fallback. Rerun this focused case and the earlier projection/page assertions affected by a packaging or configuration repair.

**REFACTOR.** Reuse the startup lane's immutable-image build, Postgres/deployment setup and cleanup, and the already-owned auth/workload support where appropriate. Private support may be shared, but do not add a second lifecycle owner, test-only production endpoint, injected session shortcut or host-mounted replacement for the image's SDK/node_modules. Preserve existing default startup coverage; filtered execution is selection, not removal of required tests.

## Acceptance Criteria

- Both ordered scenarios pass using the applicable owners, including exclusions and edge/failure branches; the separate live page acceptance gate proves all user-visible branches against real data.
- Every required BFF input reaches the identified upstream operation/query with correct scope; generated response/schema mapping and denied/missing/error behavior are recorded in the page evidence.
- The page is accepted independently against SDK/HTTP/Bifrost evidence in the shared disposable setup with mock identity/content disabled. Domain evidence is registered/emitted/evaluated through public pathways, never inserted as prepared results.
- Primary content or truthful empty/error state exists in initial HTML before hydration. URL controls invoke SvelteKit server loads; hydration does not redundantly fetch initial data. Protected upstream calls remain server-only. Preloads are read-only/bounded and every navigation revalidates session authority.
- Context is visible, changeable and clearable where applicable. Continuation links preserve applicable service/time/selection state; relationship links select the target’s identity. Filters, pagination and counts agree with authoritative reads under equivalent settled scope.
- Missing, denied, unavailable and schema-invalid data never silently falls back to fixtures. Work remains bounded and supported terminal/visibility semantics are preserved.
- The focused official-image case proves the production BFF's native SDK loads, uses the configured real endpoints, returns known retained analytical evidence in initial HTML, and exposes analytical dependency failure without fixture fallback. Record immutable image provenance and the actual production runtime/dependency layout; host check/build or host browser success cannot substitute for this proof.
- The startup selector selects exactly the planned case and rejects unknown/missing/skipped selections. Default production startup behavior still retains all existing assertions. The smoke runs against the official image without host SDK/module mounts and without exposing credentials in output or captures.
- Record a human usability pass or concrete corrected issues with scrubbed local screenshots; review control discoverability, scope clarity, back/refresh behavior, accessible interaction and error recovery.
- Revision 4 (REQ-026): the shared workload setup accepts a Service count, with this gate running the default of two. Larger counts reuse the same public registration, execution, flush and readiness path; no second setup or seeding shortcut exists. Proving twenty Services is TASK-019's obligation, not this gate's.

## Expected Write Set and Consumer Closure

Service load/workspace/facade, server-only SDK consumers, UI package/lock, native-package build/image/runtime closure, shared execution support and focused tests. No durable service health resource or invented publication contract.

Production-runtime proof also reaches the existing `scripts/server/test-startup-prod.sh`/`startup-common.sh` ownership, its public SDK workload consumers, `mise.toml::test:server:startup:prod` selection/description if needed, and affected official image/entrypoint configuration. These paths are inspected guidance, not a private implementation allowlist. No new public API or task family is authorized.

Close callers, tests, fixtures/setup, package/lock/config and runtime consumers of each changed owner. Shared/private helpers may move only when that earns a smaller cohesive implementation. The existing identity host must discover and forward the planned exact case selector; adding a split test file includes discovery changes so the command cannot select zero tests.
When changing a shared projection or method to asynchronous live IO, update every in-scope load/caller and retain mock/component behavior. Analytical result schemas are not HTTP DTOs: validate canonical columns/types before interpretation and reuse existing SDK decoding.

## Verification and Evidence

Focused RED/GREEN iteration:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/service/ServiceWorkspace.test.ts -t '^live UI Service workspace projection$'
```

The planned focused case extends the inspected existing component/projection test owner. Existing module setup remains intact; tests needing only typed values/load mapping do not start Keycloak/Postgres.

Page acceptance (run the filtered real browser case after the local behavior is coherent; it must still cover every user-visible scenario):

The following exact journey case is planned under the inspected test owner/lane; introduce its assertions before implementing missing behavior. Correct existing branches are verification-only evidence. This is a mixed task when a generic implementation already satisfies part of the page.

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Service workspace' mise run test:identity:journey
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/service/ServiceWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run build
```

Production-runtime acceptance (planned focused case under the existing inspected image/startup lane):

```bash
WYRD_STARTUP_FILTER=authenticated_analytical_bff mise run test:server:startup:prod
```

This command owns the image build and disposable dependency setup; do not prescribe an additional `docker:build` or full startup aggregate for the same focused proof. The selector must be implemented in this task. Record the selected case, immutable official-image ID, production Node/native dependency load, actual analytical evidence, bounded readiness and no-fallback failure result. Preserve default startup assertions and add focused selection regression coverage in the existing owner. Run the ordinary production startup lane only if changes to its shared/default control flow require that regression; it is not a replacement for the named BFF smoke.

Run `mise run codegen:check` when HTTP generation/consumption changes, and record which generated operation types the page consumes. Identify canonical analytical columns/types and prove schema/terminal mapping where used.
For actual Rust changes, run `mise run fmt` and `mise run lints`; for changed dependency/owner boundaries run `mise run check:deps`. Add only the narrow owning format/lint/test checks justified by the actual write set.
Do not run full identity/SDK journey suites or a broad aggregate as this task’s gate. Integrated verification belongs once to change review. Existing authentication/audit behavior needs regression proof, not manufactured RED.
Retain initial-response/network evidence, expected data identities and exclusions, readiness bounds and a scrubbed usability review. Record page pass/fail and mapped obligations; do not claim an isolated page proves the whole change.

## Material Stop Conditions

Stop for specification revision if the required outcome genuinely needs a new public/persisted contract, migration, permission/audience change, durability/freshness guarantee, engine behavior or ownership exception. Do not silently substitute a new read API or unbounded scan for a missing contract.
Report a concrete blocker if repository-managed setup cannot supply the required real evidence safely or a canonical schema cannot support truthful required presentation. Reversible private mapping/tooling choices remain implementation-owned; correct mock assumptions rather than inventing domain state.

## Authority Links

- [Independent plan review, PR-001](../plan-readiness-review.md)
- [Server/BFF route audit](../route-audit.md)

- [Approved specification, revision 4](../spec.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Protocol doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

