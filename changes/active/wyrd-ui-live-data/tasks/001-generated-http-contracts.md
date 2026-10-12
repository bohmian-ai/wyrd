---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-live-data
spec_revision: 3
requirements: [REQ-020, REQ-021, REQ-022]
invariants: [INV-001, INV-002, INV-008, INV-010, INV-011]
acceptance: [AC-017]
depends_on: []
---

# TASK-001 — Generated HTTP contracts and breaking-change proof

## Outcome and Value

One repeatable route-to-TypeScript path catches stale artifacts and incompatible BFF consumers before any live page is accepted.

Owns AC-017.
This contract gate precedes live page acceptance.

## Owners, Scope, Consumers, and Prohibited Changes

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`; server paths are relative to `crates/wyrd/wyrd-server`. Named symbols and tests describe inspected ownership, not a private-file allowlist. Use `wyrd-implement` and, for UI edits, `wyrd-ui`.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Generated HTTP contracts and breaking-change proof | `src/http/router.rs::build_router`, `src/http/openapi.rs::WyrdApiDoc`, `scripts/gen_card_types.py`, `mise.toml::codegen:regen/codegen:check` | `pg_openapi_contract.rs::the_served_document_describes_the_composed_surface`; UI `upstream.test.ts`, `WyrdClient`, existing generated Card types | Card schema generation exists; complete mounted HTTP operation types and consumer drift proof do not. | Extend the existing generation/check workflow from a fresh locally served composed document and consume its generated operation types. | A standards-capable OpenAPI TypeScript generator is justified because the current Card JSON-schema generator does not cover HTTP operations. It is tooling, not a new transport or contract owner. |
| Typed consumers and contract checks | Existing generated Card projections, `upstream.test.ts` and UI check/build scripts | Served OpenAPI contract integration test and codegen artifact inventory | Full operation coverage and incompatible adapter proof are absent | Extend the current generator/check ownership and typed BFF mappings | No duplicate session, transport, contract owner or page-data schema. |

Keep native Card/Verifier identity and exact attribution. Effective tenant/permissions come from the current verified session, never from URL scope; secrets remain server-only. Preserve actual server permission decisions and non-blocking audit staging. The BFF never reads databases/object storage directly or replaces registry/verification/query engines.
Do not add public/persisted contracts, migrations, compatibility routes, signal read APIs, a global service selector, Changes, custom dashboards, raw Query functionality or paid/external provider requirements. Existing foundation tasks are historical context only. The audited extra Changes/dashboard/GenAI/styleguide routes are outside this page-gate scope.

## Required Server and BFF Contracts

[Source-backed route audit](../route-audit.md) defines operation inputs/responses, permissions, structured errors, canonical query schemas and the route-to-task matrix. Reuse those mounted contracts; do not generate types from this planning table or create a parallel `/ui/*` API family.

| Boundary | Required behavior and contract |
|---|---|
| Existing BFF route | No new BFF endpoint; existing generated consumers in server-only facade, BrowserSessions and Settings. |
| URL/action inputs | All mounted HTTP operations actually consumed by the included loads/auth/settings support. |
| Existing server operations | S-CONTRACT; S-CARDS/S-CARD-UID/S-CARD-REF/S-VERSIONS; S-OAUTH/S-IDENTITY; S-BINDING/S-RUN; S-QUERY binary contract. |
| Required integration gap | Extend codegen from the served composed document, not this audit table. Preserve OAuth form versus JSON/domain versus binary response distinctions. |
| Analytical result contract | Analytical query result schemas remain Q-* contracts, not generated JSON DTOs. |

Use generated HTTP operation types and SvelteKit-generated route data types. Protected domain reads remain server-only with current session authority. SvelteKit loads call the facade directly; no internal proxy HTTP hop or page-specific JSON handler is currently required. Browser OAuth redirects/device approval retain their existing protocol paths. Test inputs, returned shape/status/media type, forbidden/missing identities and URL restoration—not merely that a route returns 200.

## Approach

1. Compose and serve the real local OpenAPI surface through existing server/test ownership; keep exported JSON ephemeral.
2. Generate operation/parameter/body/status/error types for audited consumed registry, auth, Settings, verification and query operations and reconcile existing Card projections; repair schema metadata only where actual serialization proves it incomplete.
3. Extend codegen regeneration, artifact inventory checking, UI type-checking and affected CI consumers together.
4. Prove deterministic output, missing/added/stale artifact detection and a representative incompatible consumer.

## Ordered Implementation Scenarios

### Scenario 1 — Generation and runtime contract

**Behavior.** Generate twice from the composed local server and compare artifacts, operation coverage and actual response/error serialization. Mapped obligations: REQ-020, REQ-021, REQ-022; AC-017.

**RED.** The planned focused contract test fails because HTTP generation/typed operation consumers are absent. Grow the exact focused case `generated HTTP contracts detect drift and incompatible consumers` in this order, using:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/upstream.test.ts -t '^generated HTTP contracts detect drift and incompatible consumers$'
```

**GREEN.** Add the minimal generated projection and correct owning schema metadata without changing wire behavior. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

### Scenario 2 — Drift and incompatible consumers

**Behavior.** Stale, missing and unexpected generated output fails checking; after regeneration, a changed consumed operation or field rejects an unchanged adapter at type-check. Mapped obligations: REQ-020, REQ-021, REQ-022; AC-017.

**RED.** Temporarily mutate isolated test copies of generated/source inputs and demonstrate the current workflow misses drift or incompatible use. Grow the exact focused case `generated HTTP contracts detect drift and incompatible consumers` in this order, using:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/upstream.test.ts -t '^generated HTTP contracts detect drift and incompatible consumers$'
```

**GREEN.** Extend the existing check and typed adapter boundary; restore all mutation probes before handing off. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

## Acceptance Criteria

- Both ordered scenarios pass using the applicable owners, including exclusions and edge/failure branches; the separate live page acceptance gate proves all user-visible branches against real data.
- Every required BFF input reaches the identified upstream operation/query with correct scope; generated response/schema mapping and denied/missing/error behavior are recorded in the page evidence.
- Generation covers mounted operations used by in-scope UI consumers, including actual parameters, response status/media/body and structured errors; output is deterministic and mutation probes are restored.
- Generated artifacts have one owning workflow; no handwritten HTTP DTO or unsafe cast conceals a closed contract gap.
- Stale/missing/added artifacts fail the check and incompatible regenerated types fail an unchanged representative consumer.
- Missing, denied, unavailable and schema-invalid data never silently falls back to fixtures. Work remains bounded and supported terminal/visibility semantics are preserved.
- Actual serialization remains covered by the served-contract proof; source metadata repairs describe existing wire behavior only.

## Expected Write Set and Consumer Closure

Existing codegen scripts/mise ownership, route/schema metadata if inaccurate, reviewed generated TS, UI server consumers, package/lock tooling and affected CI. Do not check in a second authoritative OpenAPI document.

Close callers, tests, fixtures/setup, package/lock/config and runtime consumers of each changed owner. Shared/private helpers may move only when that earns a smaller cohesive implementation. The planned focused generator test extends the confirmed existing upstream test owner; do not treat compile-time types as runtime response validation.
When changing a shared projection or method to asynchronous live IO, update every in-scope load/caller and retain mock/component behavior. Analytical result schemas are not HTTP DTOs: validate canonical columns/types before interpretation and reuse existing SDK decoding.

## Verification and Evidence

The following exact journey case is planned under the inspected test owner/lane; introduce its assertions before implementing missing behavior. Correct existing branches are verification-only evidence. This is a mixed task when a generic implementation already satisfies part of the page.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/upstream.test.ts -t '^generated HTTP contracts detect drift and incompatible consumers$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run build
scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::the_served_document_describes_the_composed_surface)'
```

The codegen check is the existing contract owner lane, not an independent route inventory.
For actual Rust changes, run `mise run fmt` and `mise run lints`; for changed dependency/owner boundaries run `mise run check:deps`. Add only the narrow owning format/lint/test checks justified by the actual write set.
Do not run full identity/SDK journey suites or a broad aggregate as this task’s gate. Integrated verification belongs once to change review. Existing authentication/audit behavior needs regression proof, not manufactured RED.
Record generation provenance, deterministic comparison, mutation/type-break proof and served serialization checks.

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

Commits `86e68cacc..dc4d2ba99` on `wyrd-server-ui`.

**Capability map, revalidated.** Owner: `build_router` composes routes and the served document. Gap: no HTTP operation projection, and the served document was not generator-consumable. Extension: `examples/served_openapi.rs` writes the served `/openapi.json` from `WyrdTestServer` (the same composed router) to an ephemeral file. `openapi-typescript@7.13.0` (devDependency, tooling only) emits `wyrd-ui/src/lib/server/generated/wyrd-http.d.ts`. Both run under `codegen:regen` → `codegen:http` (one Postgres lifecycle). No checked-in OpenAPI document, no new transport.

**Source schema repairs (no wire change).**
- `Spec::schemas` did not forward its variants' dependencies, leaving 27 undefined `$ref`s (`DataInterface`, `NonSecretValue`, `WorkflowStep`, …). Each variant's closure is now forwarded, matching `VerifierImplementation`.
- `NonSecretValue::{List,Object}` and `ColValue::List` take `schema(no_recursion)`, following the existing `FieldSpec` precedent. Without it the forwarded collection overflowed the stack.
- Seven operations reused operationIds (`list_connections`, `remove_connection`, `list_credentials`, `issue_credential`, `revoke_credential`) across tenant, platform, gateway and operator routes. The non-tenant routes now name explicit unique ids.

**Typed consumer.** `upstream.ts` defines `V1` / `V1Method` / `V1Request` / `V1Args` / `V1Ok` / `V1Response` / `v1Path`, all derived from generated `paths`. `BrowserSession.api` takes a generated operation, its path parameters and its body. The settings page's handwritten `ConnectionView`/`Connections` DTOs and casts are gone.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Generation covers in-scope mounted operations, params, bodies, statuses/media, errors | Whole served document generated; problem bodies under `application/problem+json` | `wyrd-http.d.ts`; focused vitest compiles the settings consumer against it | PASS |
| Deterministic output | Byte-identical served JSON across two fresh servers; byte-identical TS across two generator runs | clean `mise run codegen:check` exit 0 with no tree change | PASS |
| Stale/missing/added artifacts fail the check | `codegen:http` clears the dir; `codegen:check` snapshots it | probes: stale exit 1, missing exit 1, added `extra.d.ts` exit 1, each printing `codegen drift detected`; tree restored | PASS |
| Incompatible regenerated types fail an unchanged consumer | `upstream.test.ts` mutates the generated text in-memory | renamed response field, new required body field, renamed path param, removed operation → exactly one consumer error each, naming the changed symbol | PASS |
| One owning workflow; no handwritten DTO/unsafe cast | Settings DTOs removed; `V1Response.json()` is the documented compile-time-only boundary | `pnpm run check` 0 errors | PASS |
| Served serialization stays proven; repairs describe existing wire | Served test asserts every `$ref` resolves and operationIds are unique | `pg_openapi_contract::` 21/21 pass, including the named case | PASS |
| CI includes the proof | `codegen:check` runs UI `pnpm run check`; selector picks `codegen:check` for `wyrd-server` and UI paths | `.github/scripts/tests/test-detect-changes.sh` 53/53 | PASS |

**Commands run.**
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/upstream.test.ts -t '^generated HTTP contracts detect drift and incompatible consumers$'`: pass.
- `pnpm run check`: 0 errors. `pnpm run build`: ok. Full UI vitest: 33/33 files on two runs. One earlier cold-cache run failed `CardDetail.test.ts` at suite level; it passed in isolation and on reruns, and the error was not captured.
- `scripts/postgres/with-test-postgres.sh -- … cargo nextest run --locked -p wyrd-server --test integration -E 'test(/^pg_openapi_contract::/)'`: 21/21.
- `cargo nextest run -p wyrd-spec --features server`: 910/910.
- `cargo nextest run -p wyrd-server --lib -E 'test(/openapi/)'`: pass.
- `mise run codegen:check`: pass, plus three failing probes.
- `mise run fmt`: no changes. `git diff --check`: clean.
- `cargo clippy -p wyrd-spec -p wyrd-server --all-features --all-targets -D warnings`: clean.

**Material limits.**
- `ColValue` is a self-referential primitive union that TypeScript cannot express as an indexed-access member. The artifact is a `.d.ts` (the generator's documented form), and `skipLibCheck` widens that one member. It is commented in `codegen:http`.
- `codegen:regen`/`codegen:check` now need Docker Postgres and a `wyrd-server` example build.
- `mise run lints` (workspace) is RED, but not from this change. `vala-bifrost-redux` `forge::metrics::tests::forge_operator_documentation_matches_telemetry` `include_str!`s `docs/src/content/docs/bifrost/forge.svx`, which `2044e161e` deleted in the docsite rebuild. No current doc lists the 23 Forge families, so the fix needs a decision on where Forge operator docs now live. BLOCKER for the workspace lint lane; reported, not patched.

**Non-goals held.** No new public/persisted contract, migration, compatibility route, read API, or checked-in OpenAPI document. operationId renames touch documentation only; paths, methods and bodies are unchanged.
