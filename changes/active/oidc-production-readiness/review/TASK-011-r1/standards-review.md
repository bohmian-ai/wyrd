# Repository standards review — TASK-011 r1

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Candidate observed at review start and completion: `0b8919fff090d4b0109a506711cb119214d231f2`
- Scope inspected: the complete 46-file base-to-candidate diff, the owning modules and callers, package and lock metadata, the task/spec/research packet, the routed closure direction for `FIND-TASK-010-1`, the recorded verification, and the applicable repository authorities below.
- CodeGraph: not used because the repository has no `.codegraph/` directory.

I did not read another TASK-011 reviewer report.

## Authority coverage

| Changed surface | Files covered | Governing authority read and applied | Coverage result |
|---|---|---|---|
| Approved task record | `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md` | `AGENTS.md` §§11–16; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md`; approved `spec.md`; `research/auth-standards-recommendation.md`; routed lead direction for `FIND-TASK-010-1` | Complete |
| BFF OAuth/session owner | new `src/lib/server/auth/browser-sessions.ts`; deleted `server-sessions.ts`; `session.ts`; `hooks.server.ts`; `app.d.ts` | `AGENTS.md` §§2, 3, 9, 15, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` runtime identity; `architecture/wyrd-security-posture.md` security principles, credential lifecycle, access/refresh tokens, federation; `architecture/wyrd-doctrine.mdx` public surfaces; `architecture/references/architecture/patterns.md`; `architecture/references/languages/typescript-guide.md`; `wyrd-ui` skill and its SvelteKit/Svelte references | Complete |
| OAuth callback, routine SSO, recovery sign-in, logout, and tenant switch routes | `routes/login/callback/+server.ts`; `routes/t/[tenantKey]/login/+page.server.ts`; `+page@.svelte`; new `login/api-key/+page.server.ts` and `+page@.svelte`; root `+page.server.ts` and `+page.svelte`; tenant `+layout.server.ts` | Same BFF/security authorities; approved REQ-010 lead decisions; RFC/library boundary recorded in the approved task; `wyrd-ui/references/wyrd-developer-ux.md` and `wyrd-testing-verification.md` | Complete |
| Authenticated UI API projection and canonical errors | `src/lib/server/upstream.ts`; `problem.ts`; `changes/actions.ts`; `views.ts`; tenant settings server/page | `AGENTS.md` §§9, 15, 16; security posture; architecture patterns server/client rules; TypeScript guide error and boundary rules; SvelteKit server/client split | Complete |
| Removal of custom CSRF fields across UI | `+layout.server.ts`, `+layout.svelte`; `Shell.svelte`; `TenantChooser.svelte`; change feature components and six change routes; settings page; root page | Approved task; SvelteKit built-in `csrf.checkOrigin`; `wyrd-ui` SvelteKit architecture; UI accessibility/interaction rules; TypeScript guide | Complete |
| UI component and route tests | `Shell.test.ts`; `ChangesJourney.test.ts`; `session.test.ts`; `routing/journey.test.ts`; `routing/tenant.test.ts`; `production-auth.integration.test.ts` | `AGENTS.md` §11; agent-rules test placement and gate integrity; testing workflows; `wyrd-ui/references/wyrd-testing-verification.md` | Complete |
| UI dependencies and generated lock | `package.json`; `pnpm-lock.yaml` | `AGENTS.md` §§1, 15; TypeScript guide; `wyrd-ui` rule to reuse approved dependencies; task's exact `openid-client`/`jose` decision | Complete |
| Real-server journey host | `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs` | `AGENTS.md` §§4–6, 11, 16; agent-rules rustdoc, async, external-test, and gate-integrity rules; testing workflows | Complete |
| Shared test harness | `crates/wyrd/wyrd-testing/src/server.rs::WyrdTestServer::start_bound_replica` | `AGENTS.md` required struct-centered Rust style, async rules, test ownership, and rustdoc rules; architecture patterns; testing workflows | Complete |
| Identity lane wiring | `mise.toml` | `AGENTS.md` §11; agent-rules exact-selection and repository-managed-environment rules; testing workflows | Complete |
| Styling and visual changes | routine-login and recovery-page Svelte changes; removal-only form edits elsewhere | `wyrd-ui` skill; light/dark style guides; theming, Svelte 5, developer UX, and verification references; current app/brand conventions | Complete |
| SQL capability boundary | No SQL production signature, query, migration, or pool field changed | `architecture/agent-rules.md`; architecture patterns storage/registry section | Not applicable; no `PgPool`, `TenantConn`, or `OperatorPool` surface entered the diff |

All 46 changed paths are represented by the groups above. No Python, PyO3, SDK declaration, generated schema, MCP, Bifrost, or durable SQL contract changed, so their focused authorities and gates are not applicable.

## Applicable-rule audit

| Rule | Source evidence | Result |
|---|---|---|
| Durable identity and authorization remain server-owned; the UI is a BFF/client projection | `BrowserSession.api` forwards the server-issued access token to `/v1`; `BrowserSession.context` projects claims for UI behavior, while settings mutations still receive their authoritative permission decision from Wyrd. No principal store, role mapper, grant writer, or durable session store was added. | PASS |
| Tenant authority comes from verified credentials, not route/browser data | The sealed cookie binds its route hint to `sealed.tenant`; every upstream call uses the exchanged Wyrd access token. The approved API-key recovery behavior deliberately permits a key from another tenant, and the real-server journey proves the token's tenant cannot read or mutate the route tenant's connection. | PASS |
| Standard OAuth/OIDC mechanisms use the approved libraries, with no parallel protocol implementation | `browser-sessions.ts` uses `openid-client` 6.8.8 `discovery`, `buildAuthorizationUrl`, S256 PKCE/state, `authorizationCodeGrant`, `refreshTokenGrant`, `genericGrantRequest`, and `tokenRevocation`; `jose` 6.2.12 provides `EncryptJWT`/`jwtDecrypt`. `server-sessions.ts` and `/login/complete` are deleted. The pinned manifest and lock agree. | PASS |
| Browser secrets remain server-only | `BrowserSession.#token` is private; page metadata contains only subject, expiry, and tenant keys. Cookies are host-only, `Secure`, `HttpOnly`, `SameSite=Lax`, and JWE-encrypted with `dir`/`A256GCM`. `production-auth.integration.test.ts` checks page/data bodies, locations, cookies, API keys, and clear JWT patterns. | PASS |
| CSRF uses the approved native boundary | Custom CSRF fields/checker are removed. The SvelteKit configuration does not disable `csrf.checkOrigin`; all relevant actions remain form POSTs. The built-server journey proves a cross-origin settings mutation and API-key sign-in receive 403. | PASS |
| SvelteKit server/client split is preserved | Discovery, token exchange, refresh, revocation, cookie encryption, and Wyrd API calls live under `src/lib/server` or server routes. Svelte components receive typed safe page data and contain no token/network authority. | PASS |
| TypeScript boundary quality | New exported owners have explicit types and return types where required, boundary inputs are narrowed, `unknown` is used for request bodies, no `any` was introduced, and top-level named functions/classes follow existing style. | PASS |
| Errors expose canonical safe Wyrd projections | `problemKind` maps only known generated problem examples or safe status fallbacks; arbitrary upstream diagnostics are not serialized. OAuth library failures become canonical BFF problems without token/error-body leakage. | PASS |
| Secrets and transport are handled at the server boundary | `WYRD_UI_CLIENT_SECRET` is read from private environment state, never browser data; the internal Wyrd URL retains HTTPS-off-loopback enforcement and bounded requests. No secret was added to logs, errors, page data, or task-generated artifacts. | PASS |
| UI behavior and accessibility follow local patterns | The SSO action remains primary, recovery is a semantic link to a separate form, labels are bound to inputs, alerts use `role="alert"`, controls are keyboard-native, and new styles use existing tokens, zero-radius variables, 2px borders, and hard-offset shadows. No UI library or parallel styling system was added. | PASS |
| Changed Rust follows struct ownership, async, and documentation rules | `start_bound_replica` is an inherent method on `WyrdTestServer`, reuses `start_replica`, awaits real start/bind IO, and has intent plus `# Errors` rustdoc. The materially changed journey module/test/helper documentation describes the two-replica topology and panic conditions. | PASS |
| External test placement is earned | `identity_ui_e2e.rs` starts real server replicas, BFF child processes, TLS, Postgres-backed state, Keycloak/Dex, and a compiled UI. It therefore satisfies the external-test exception. | PASS |
| Journey proof is primary and exact selection cannot silently select zero tests | The identity lane counts every required Vitest journey and runs the exact ignored Rust host expression. The focused task command selects `production_ui_bff_journey`; the host runs the four named real-wire BFF journeys. | PASS |
| Gate integrity | The diff adds no lint/check suppression, weakens no boundary script, and does not remove/ignore a failing test to clear a gate. The existing ignored journey remains explicitly executed with `--run-ignored=all`. | PASS |
| Verification is scoped to the task write set | Recorded green evidence covers the filtered identity journey (four UI journeys), exact changed Vitest cases, UI `check`, all UI tests (32 files/177 tests), Rust format and lints, and `git diff --check`. Per standing direction, unfiltered identity and every-language journeys belong to change review rather than this task gate. | PASS |
| No legacy/private BFF compatibility path remains | The deleted custom owner and completion route have no production references; the recorded grep found no `WYRD_BFF_SERVICE_KEY`, `internal/bff`, `x-wyrd-bff-key`, `login/complete`, or `wyrd_flow` outside change records. | PASS |
| Permanent code does not mention task artifacts | New recovery-route JSDoc says `(REQ-010)` at `src/routes/t/[tenantKey]/login/api-key/+page.server.ts:15`. This violates the literal wording rule in `architecture/agent-rules.md`, but has no behavioral, security, tenancy, durability, public-contract, or rustdoc-completeness consequence. Under the review's standing direction, this wording-only issue is non-blocking. | FAIL — non-blocking wording note |

## Material repository-rule findings

None.

## Non-blocking notes

### NB-REPO-011-1 — task identifier in permanent JSDoc

- Location: `crates/wyrd/wyrd-server/wyrd-ui/src/routes/t/[tenantKey]/login/api-key/+page.server.ts:15`
- Rule: `architecture/agent-rules.md` says permanent code must not mention plans or task IDs.
- Evidence: the new JSDoc includes `(REQ-010)`.
- Assessment: wording-only. The surrounding JSDoc independently explains the recovery boundary and tenant-authority behavior, and the identifier changes no runtime or public contract. Per the governing review direction, it does not block acceptance and is not a material finding.

## Verification assessment

The candidate records these successful, correctly scoped proofs:

- Focused real identity host: `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER=production_ui_bff_journey mise run test:identity:journey` (host passed; four BFF journeys passed).
- Exact changed UI tests for routing and Changes behavior.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` (0 errors, 0 warnings).
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test` (32 files, 177 tests).
- `mise run fmt`, `mise run lints`, and `git diff --check`.

This is the narrowest complete task-level proof for the write set. The intentionally deferred unfiltered identity and cross-language journeys are change-review gates, not a TASK-011 repository-standards deficiency. No missing applicable authority or unavailable required evidence blocks this standards pass.

## Overall result

**PASS**

The complete diff complies with the material repository rules governing the BFF, SvelteKit UI, OAuth library boundary, security/tenancy projection, Rust test harness, test taxonomy, and scoped verification. The sole literal rule miss is a non-blocking task identifier in JSDoc; under the required placement/naming/structure/wording policy it cannot produce a failing result.
