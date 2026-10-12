---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-live-data
spec_revision: 3
requirements: [REQ-001, REQ-002, REQ-003, REQ-016, REQ-017, REQ-018, REQ-019, REQ-023, REQ-024, REQ-025]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011]
acceptance: [AC-001, AC-018]
depends_on: [TASK-001]
---

# TASK-002 — Fresh human login, CLI authentication and shared browser setup

## Outcome and Value

A single disposable local setup proves UI login creates exactly one human principal and lets that same person authorize the CLI, while supporting every later page gate.

Owns AC-001, AC-018. AC-018 is a per-page obligation; AC-017 typed consumer/schema evidence also applies.
Task 001 establishes generated HTTP contracts before live page acceptance.

## Owners, Scope, Consumers, and Prohibited Changes

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`; server paths are relative to `crates/wyrd/wyrd-server`. Named symbols and tests describe inspected ownership, not a private-file allowlist. Use `wyrd-implement` and, for UI edits, `wyrd-ui`.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Fresh human login, CLI authentication and shared browser setup | `hooks.server.ts`, `BrowserSession.api/context`, `BrowserSessions`, `identity_ui_e2e.rs::production_ui_bff_journey`, `mise.toml::test:identity:journey` | `production-auth.integration.test.ts`; server identity UI host, CLI device-login journey; official nginx path routing; SDK integration workload owners | Production HTTP identity journeys exist, but no hydrated real browser exercises fresh-user auto-registration, same-person CLI approval or subsequent live page gates. | Extend the existing environment-owning identity lane, server lifetime and BFF launches to host browser cases and shared public-SDK workloads. | Real browser automation is justified: existing fetch/cookie tests cannot prove hydration or user interaction. Reuse Vitest discovery/lifetime; do not introduce a second environment controller. |
| Authenticated loading and presentation | `hooks.server.ts`, `BrowserSession.api/context`, existing `+page.server.ts` loads and `WyrdClient` | Production auth integration tests; current route/component consumers; generated operation types | Current mock domain methods do not provide this page’s authoritative asynchronous live data | Extend the current server-only facade/load and retain SvelteKit-generated page types | No duplicate session, transport, contract owner or page-data schema. |

Keep native Card/Verifier identity and exact attribution. Effective tenant/permissions come from the current verified session, never from URL scope; secrets remain server-only. Preserve actual server permission decisions and non-blocking audit staging. The BFF never reads databases/object storage directly or replaces registry/verification/query engines.
Do not add public/persisted contracts, migrations, compatibility routes, signal read APIs, a global service selector, Changes, custom dashboards, raw Query functionality or paid/external provider requirements. Existing foundation tasks are historical context only. The audited extra Changes/dashboard/GenAI/styleguide routes are outside this page-gate scope.

## Required Server and BFF Contracts

[Source-backed route audit](../route-audit.md) defines operation inputs/responses, permissions, structured errors, canonical query schemas and the route-to-task matrix. Reuse those mounted contracts; do not generate types from this planning table or create a parallel `/ui/*` API family.

| Boundary | Required behavior and contract |
|---|---|
| Existing BFF route | GET /; POST /?/tenant, /?/switch, /?/logout; GET /t/{tenantKey}/login; POST /t/{tenantKey}/login?/sso; GET/POST /t/{tenantKey}/login/api-key; GET /login/callback. |
| URL/action inputs | tenantKey form/route, PKCE/state and authorization response in the established confidential-client flow. Rust GET/POST /auth/device handles CLI approval. |
| Existing server operations | S-OAUTH: GET /.well-known/oauth-authorization-server, GET /auth/authorize, GET /auth/callback, POST /auth/token, POST /auth/revoke. S-DEVICE: POST /auth/device_authorization and GET/POST /auth/device. S-IDENTITY setup reads/writes and S-READY; S-PRINCIPALS for privileged assertion only. |
| Required integration gap | Reuse existing actions/handler/session. Provision a fresh Keycloak user without pre-creating its Wyrd principal. CLI token exchange uses the configured server origin directly, not discovery or Keycloak; verify /auth/* routing through the real public proxy. Published discovery reachability is a separate configuration check. |
| Analytical result contract | No analytical read required for login. Generated existing Settings/API-key/auth consumers are included without adding Settings behavior. |

Use generated HTTP operation types and SvelteKit-generated route data types. Protected domain reads remain server-only with current session authority. SvelteKit loads call the facade directly; no internal proxy HTTP hop or page-specific JSON handler is currently required. Browser OAuth redirects/device approval retain their existing protocol paths. Test inputs, returned shape/status/media type, forbidden/missing identities and URL restoration—not merely that a route returns 200.

## Approach

1. Extend repository-managed Keycloak/Postgres/server/BFF setup with a fresh identity never pre-created in Wyrd, production sessions and trusted local HTTPS.
2. Provide real combined origin routing using the existing gateway pattern; verify CLI auth calls only the configured Wyrd origin and discovery routing independently; preserve certificate verification and production cookie/CSRF semantics.
3. Extend the identity lane selector/discovery and Rust host forwarding so each new browser case selects exactly one test and cleans up its resources.
4. Reuse public SDK registration/execution/export helpers on the same bound server; make verification runtime support available when later gates need it.
5. Drive browser SSO, repeat login, CLI device approval and authenticated read; record SSR/navigation/network evidence without credentials.

## Ordered Implementation Scenarios

### Scenario 1 — Fresh login and CLI approval

**Behavior.** A new Keycloak subject signs in through the rendered UI, becomes one Wyrd principal, repeats login without duplication and approves CLI login as the same person. Mapped obligations: REQ-001, REQ-002, REQ-003, REQ-016, REQ-017, REQ-018, REQ-019, REQ-023, REQ-024, REQ-025; AC-001, AC-018.

**RED.** The focused live login case fails because the existing HTTP harness cannot exercise this complete browser/CLI story. Grow the exact focused case `live UI login and CLI` in this order, using:

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI login and CLI' mise run test:identity:journey
```

**GREEN.** Extend the existing host and browser case; use the real CLI credential writer and authenticated read, not shell environment mutation. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

### Scenario 2 — Session failures and reusable page setup

**Behavior.** Logout, failed login, expired nonrenewable sessions and manipulated tenant URLs expose no protected data; a second tenant/restricted identity can run independently in the same setup. Mapped obligations: REQ-001, REQ-002, REQ-003, REQ-016, REQ-017, REQ-018, REQ-019, REQ-023, REQ-024, REQ-025; AC-001, AC-018.

**RED.** Add these negative browser branches to the focused case; observe missing browser coverage or any actual session defect. Grow the exact focused case `live UI login and CLI` in this order, using:

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI login and CLI' mise run test:identity:journey
```

**GREEN.** Repair only demonstrated boundary defects and close cleanup/readiness/selector plumbing; later gates reuse this lifecycle. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

## Acceptance Criteria

- CLI device/token/refresh/revocation calls use only the configured Wyrd server origin; Wyrd owns provider interaction and token issuance. Browser provider redirects are expected. No CLI Keycloak client/discovery requirement is introduced.
- Both ordered scenarios pass using the applicable owners, including exclusions and edge/failure branches; the separate live page acceptance gate proves all user-visible branches against real data.
- Every required BFF input reaches the identified upstream operation/query with correct scope; generated response/schema mapping and denied/missing/error behavior are recorded in the page evidence.
- The page is accepted independently against SDK/HTTP/Bifrost evidence in the shared disposable setup with mock identity/content disabled. Domain evidence is registered/emitted/evaluated through public pathways, never inserted as prepared results.
- Primary content or truthful empty/error state exists in initial HTML before hydration. URL controls invoke SvelteKit server loads; hydration does not redundantly fetch initial data. Protected upstream calls remain server-only. Preloads are read-only/bounded and every navigation revalidates session authority.
- Context is visible, changeable and clearable where applicable. Continuation links preserve applicable service/time/selection state; relationship links select the target’s identity. Filters, pagination and counts agree with authoritative reads under equivalent settled scope.
- Missing, denied, unavailable and schema-invalid data never silently falls back to fixtures. Work remains bounded and supported terminal/visibility semantics are preserved.
- Record a human usability pass or concrete corrected issues with scrubbed local screenshots; review control discoverability, scope clarity, back/refresh behavior, accessible interaction and error recovery.

## Expected Write Set and Consumer Closure

Identity integration host/tests, existing mise selector/setup, UI auth/session/hooks only where proven necessary, browser dev dependency/lock and trusted gateway setup. Shared workload support may grow in later page tasks.

Close callers, tests, fixtures/setup, package/lock/config and runtime consumers of each changed owner. Shared/private helpers may move only when that earns a smaller cohesive implementation. The existing identity host must discover and forward the planned exact case selector; adding a split test file includes discovery changes so the command cannot select zero tests.
When changing a shared projection or method to asynchronous live IO, update every in-scope load/caller and retain mock/component behavior. Analytical result schemas are not HTTP DTOs: validate canonical columns/types before interpretation and reuse existing SDK decoding.

## Verification and Evidence

The following exact journey case is planned under the inspected test owner/lane; introduce its assertions before implementing missing behavior. Correct existing branches are verification-only evidence. This is a mixed task when a generic implementation already satisfies part of the page.

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI login and CLI' mise run test:identity:journey
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run build
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='production SSO crosses replicas' mise run test:identity:journey
WYRD_IDENTITY_TARGET=cli WYRD_IDENTITY_FILTER=cli_device_login_journey mise run test:identity:journey
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

**Reuse map.**
- Owner: `identity_ui_e2e::production_ui_bff_journey`, which hosts the existing server replicas, BFFs and TLS hop.
- Gap: no listener served the public origin, so the HTTP journey hand-routed `/auth/*` and no browser could reach the deployment.
- Extension:
  - A Node gateway on the origin port routes by the official nginx template's own `location ~` regex (`include_str!`).
  - The case `live UI login and CLI` is added to the existing `production-auth.integration.test.ts`, which reuses its fixture, `asKey` and Vitest discovery.
  - No second environment controller.
- Additions:
  - `playwright` 1.64.0 is a dev dependency; the lane installs `chromium-headless-shell`.
  - The lane seeds Keycloak user `dana`, who has no Wyrd principal.
  - nginx now forwards `/.well-known/oauth-authorization-server` (route-audit discovery gap).

**Local decision: loopback HTTP origin.** The browser-facing origin stays `http://localhost:<port>`, not HTTPS:
- Dex's public client accepts only loopback `http` callbacks, and the shared host's Dex tenant needs the same public origin.
- Chromium treats `localhost` as a secure context, so production `Secure`/`HttpOnly`/`SameSite=Lax` cookies and CSRF behave as deployed.
- The trusted, certificate-verified TLS hop remains BFF 1 → server replica B.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Focused case command: `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI login and CLI' mise run test:identity:journey` (1 passed, 4 skipped) | — | — | PASS |
| Fresh Keycloak user signs in through the rendered UI and becomes exactly one principal | `browserLogin`, `usersWithEmail` (admin `GET /v1/principals?kind=user&email=`): `[]` before login, `[shown principal]` after | `live UI login and CLI` | PASS |
| Repeat login creates no duplicate | Sign out, then sign in again (provider session, no form): same principal, list length 1 | same | PASS |
| CLI device approval as the same person; only the configured Wyrd origin is used | Real `wyrd auth login --server <origin> --no-browser`; the verification URL origin is the configured origin; the browser approves; the saved `credentials.toml` access token's `principal.id` equals the UI principal; the output has no JWT | same | PASS |
| Authenticated CLI read | `wyrd list --server <origin>` exits 0 with the saved login | same | PASS |
| Discovery is reachable on the public origin, and the endpoints advertise that origin | `GET <origin>/.well-known/oauth-authorization-server` through the nginx-template routing | same | PASS |
| Failed login exposes nothing | A wrong password stays at Keycloak (`#input-error`), with no session cookie and no principal; a forged callback goes to `/t/{tenant}/login?login=failed` and shows an alert | same | PASS |
| Logout and nonrenewable session | Sign out clears the session; a protected URL goes to login. The signed-out cookie, replayed in a new context, is sent to login within 45 s and shows no principal | same | PASS |
| Manipulated tenant URL | `/t/ui-oidc-off/settings` and `/t/ui-switch-peer/settings` go to that tenant's login with no principal in the HTML | same | PASS |
| Second tenant / restricted identity runs independently | Reader `bob` signs into `ui-switch-peer` in a separate context: a different principal, only that tenant's cookie, and `/t/test-tenant-1` goes to login; dana is unaffected | same | PASS |
| AC-018 loading gate | Settings' initial HTML contains the principal, and hydration makes no `__data.json` request. Client navigation to Home fetches `/t/{tenant}/__data.json`. The browser issues no `/v1/*` request | same | PASS |
| Existing HTTP journey unaffected | The host now also starts the gateway | `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='production SSO crosses replicas' mise run test:identity:journey`: 1 passed | PASS |
| Types / build | — | `pnpm run check`: 741 files, 0 errors; `pnpm run build` succeeds | PASS |

**Generated operation types consumed.** None new. The case reads `list_principals` and the authorization server metadata over raw HTTP as a privileged test assertion, not as a BFF consumer. `codegen:check` was not rerun because no HTTP contract changed.

**Usability pass.** Reviewed scrubbed local screenshots in `target/debug/wyrd-ui-live-login/`: `settings.png`, `home.png`, `cli-approved.png` and `session-ended.png`. Issues for later page tasks:
- The identity control shows the raw principal UUID. The session carries only the principal id, so a readable name needs a source the session does not have yet.
- Tenant Home for a viewer renders the shell-less "Permission denied (RBAC)" page, because Home is still mock-backed. TASK-003 replaces it. Recovery is via "Return to tenant entry"; the session stays valid.
- After a session ends, the login page gives no "your session ended" notice.
- The server's CLI approval completion page is unstyled plain text ("Sign-in complete. You can return to your terminal."). It is clear but bare, and it is server-owned.

**Not run, at the user's direction ("only run tests and verification related to the UI").**
- `WYRD_IDENTITY_TARGET=cli WYRD_IDENTITY_FILTER=cli_device_login_journey`.
- Rust fmt/lints and `check:deps`.

The Rust change is test-host-only (`identity_ui_e2e.rs`). The CLI journey's own server and realm configuration are unchanged.

**Non-goals.** No public or persisted contract, migration, BFF route or page behavior changed.
