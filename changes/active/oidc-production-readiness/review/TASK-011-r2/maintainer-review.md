# TASK-011 R2 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `4d468b33e49de4dd9df30c5dd046a334569465bf`
- Candidate tree: `87acdce15e3ca6ea2b6016969695db398ab2d598`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Prior verdict and findings: `changes/active/oidc-production-readiness/review/TASK-011-r1/`
- Remediation: `changes/active/oidc-production-readiness/review/TASK-011-r1/TASK-011-R1-browser-session-standards-and-logout.md`
- Lead direction: `changes/active/oidc-production-readiness/review/TASK-011-r1/lead-direction-FIND-TASK-011-2.md`
- Remediation localization: `0b8919fff090d4b0109a506711cb119214d231f2..4d468b33e49de4dd9df30c5dd046a334569465bf`

I reviewed the complete base-to-candidate range and used the remediation diff only to localize the corrected owner and proof. The candidate commit and tree resolved to the supplied identities before and after review. I did not use another TASK-011-r2 reviewer's conclusions.

## Governing maintainer authority

- `AGENTS.md`, especially ownership, async/runtime, testing, and completion rules.
- `architecture/agent-rules.md`.
- `architecture/references/languages/spec-driven-development.md`.
- `architecture/references/languages/maintainer-style.md`.
- `architecture/references/languages/typescript-guide.md` and `testing-workflows.md`.
- `architecture/references/architecture/patterns.md`.
- `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and `architecture/wyrd-security-posture.md` for the BFF, tenant, and credential boundaries.
- `.agents/skills/wyrd-ui/SKILL.md` and its SvelteKit, Svelte 5, styling, and verification references.
- The approved task decisions: routine sign-in remains SSO with API-key recovery on its own page; an API-key session's authority is the exchanged token's tenant.
- The lead reversal of `FIND-TASK-011-2`: browser logout always clears local state, and RFC 7009 revocation is best-effort. I did not reopen the superseded retry-session direction.

## Changed-surface coverage

| Surface | Changed symbols and owners inspected | Callers, consumers, and proof inspected | Maintainer assessment |
|---|---|---|---|
| Production browser-session owner | `BrowserSession`; every `BrowserSessions` field and method in `src/lib/server/auth/browser-sessions.ts`, including discovery, key derivation, cookie sealing, login, callback, API-key exchange, establishment, cache/renewal, read, logout, switch, and metadata | `hooks.server.ts`; root switch/logout actions; login, callback, recovery, tenant-layout, and settings routes; `App.Locals`; focused browser-session tests; production journey | One concrete owner holds the OAuth configuration, cookie key, and access cache. Its methods expose the session lifecycle directly, while `openid-client`, `jose`, and platform primitives retain their standard responsibilities. There is no parallel protocol abstraction, retry mechanism, session store, or configuration surface. |
| R1 remediation | `sessionLifetimeSeconds`, `BrowserSessions.establish`, `BrowserSessions.logout` | `accepts an opaque refresh token and forwards it unchanged`; `failed refresh-token revocation still signs out`; route logout action; successful logout, API-key logout, and sibling-login journey paths | Refresh credentials are now treated as opaque and forwarded unchanged. Both credential kinds use the already-approved application-session bound. Logout clears the selected cookie and cache before best-effort revocation, logs only tenant and error class, and leaves API keys unrevoked. The remediation is local to the existing owner and adds no second lifecycle mechanism. |
| Replaced custom session/CSRF surface | Deletion of `server-sessions.ts`, `/login/complete`, flow-cookie behavior, and custom CSRF fields; retained development-only `LocalSessions` | All imports and references under `wyrd-ui/src`; Change Request actions and forms; routing/session/component tests; production cross-origin POST journey | Callers were closed rather than preserved behind aliases. Production browser identity and local development identity remain visibly distinct. SvelteKit's conventional origin check plus SameSite cookie replaces custom CSRF state without a compatibility layer. |
| SvelteKit boundary and typed route contracts | `app.d.ts`, `hooks.server.ts`, `problem.ts`, `upstream.ts`, `views.ts`, root and tenant layouts, login/callback/recovery/settings handlers | Generated route `$types`; `BrowserSession.api`; settings mutation wrapper; routing tests and production journey | Secrets and backend calls stay in server-only code. Route handlers are thin, use typed SvelteKit contracts, and delegate session mechanics to `BrowserSessions`. `SessionMetadata` is the single browser-safe projection. Safe problem normalization remains centralized. |
| Components and UI routes | `Shell`, `TenantChooser`, login and API-key recovery pages, settings, and changed Change Request components/routes | Component tests, Change Request journey, every removed `csrf` prop/form consumer, route data consumers | Prop and form cleanup is complete. The routine login retains one primary SSO action and links to a separate recovery page. The recovery page follows the existing login-page structure and theme tokens; it does not add client-side credential handling or a new UI dependency. |
| Dependency and declaration parity | `package.json`, `pnpm-lock.yaml`, `app.d.ts`, `SessionMetadata` | Direct imports and lock entries for `openid-client@6.8.8` and `jose@6.2.12`; `App.Locals` readers/writers; component prop consumers | Manifest and lockfile agree. Ambient locals, safe session metadata, route consumers, and component props agree. No generated declaration, schema, or golden artifact was hand-edited. |
| Production identity journey | `identity_ui_e2e.rs`; `Browser`, gateway/provider helpers, and all four scenarios in `production-auth.integration.test.ts` | Two BFFs, two Wyrd replicas, shared Postgres, TLS terminator, Keycloak/Dex, selected UI journey wiring | Helper names state their outcome and keep provider, browser, and process responsibilities findable. The journey covers cross-replica session use, browser secrecy, conventional CSRF, API-key recovery, token-tenant authority, switch/mix-up refusal, replacement, and per-login logout. The remediation tightens the mix-up helper to the server's one actual response rather than keeping an unused alternative branch. |
| Rust test harness and lane | `WyrdTestServer::start_bound_replica`; `production_ui_bff_journey`; the UI branch of `test:identity:journey` in `mise.toml` | Existing `start_replica` and `bind` methods; sole new out-of-process replica caller; exact host-test and Vitest selectors | The new method is a cohesive operation on the existing harness owner, composes existing methods, and has substantive purpose and error rustdoc. Lane selection remains explicit and rejects zero-test filters. No separate fixture abstraction was introduced. |
| Task and remediation evidence | TASK-011 evidence table, R1 remediation evidence, prior finding ledger, lead reversal | Exact focused commands and recorded narrow lanes | Evidence names the changed tests and commands. Full identity and every-language journeys remain correctly deferred to change review under the human direction. |

## Owner and caller trace

`browserSessions` is the sole production BFF session owner. `hooks.server.ts` calls `read` for protected tenant requests and places the resulting `BrowserSession` in `locals`; the tenant layout asks the same owner for the safe page projection. The tenant login and callback call `begin` and `complete`; the API-key recovery route calls `signInWithApiKey`; root actions call `switch` and `logout`; settings uses `BrowserSession.api`. Repository search found no sibling production cookie writer, credential renewer, or alternate revocation path.

The R1 change removes refresh-token decoding at the producer, `establish`, so downstream renewal receives the original opaque credential. The focused test reaches that consumer through `read` and `refreshTokenGrant`, rather than asserting only the cookie write. The reversed logout behavior remains equally localized: `logout` deletes the selected cache entry and cookie, conditionally invokes the installed library's revocation operation, and absorbs only that best-effort failure. The root action therefore has one normal redirect outcome and no retry-state branch.

The `BrowserSession` access token remains private, and only `context`, `api`, and safe metadata expose behavior. API-key recovery stays off the primary login page. The route tenant remains routing context while `BrowserSession` derives `tenantId`, principal, and permissions from the server-issued access token, matching the recorded lead decision without adding a mapping endpoint or claim.

On the Rust side, `start_bound_replica` extends the existing `WyrdTestServer` owner and has one direct journey caller. It reuses `start_replica` and `bind` rather than duplicating fixture or listener setup. The host and Vitest journey names, topology documentation, and selection checks make the two-replica proof findable.

## Generated and typed-contract parity

- `SessionMetadata` no longer contains custom CSRF state, and every changed Svelte prop, form, test fixture, and route consumer agrees.
- `App.Locals.browserSession` matches the production hook, tenant layout, and settings route.
- New route modules import their generated `$types`; no generated route declaration is checked in or hand-edited.
- `package.json` and `pnpm-lock.yaml` both resolve `openid-client` 6.8.8 and `jose` 6.2.12.
- The remediation changes no public generated schema or SDK declaration.

## Prior-finding closure assessment

- `FIND-TASK-011-1`: closed from a maintainer perspective. The private refresh-token representation assumption is deleted from `BrowserSessions.establish`; the existing application-session bound owns cookie lifetime, and the exact opaque credential reaches the standard refresh operation. The focused test covers establishment, forwarding, and terminal refusal without adding a new harness layer.
- `FIND-TASK-011-2`: superseded by lead direction and specification revision 8's standing convention. The candidate implements local-clear-always plus best-effort revocation in the existing `logout` method and proves that outcome. No retry state, option, file, store, endpoint, or error-classification abstraction was added.

## Verification evidence assessed

The task and remediation record successful narrow proof for the write set:

- exact focused tests `accepts an opaque refresh token and forwards it unchanged` and `failed refresh-token revocation still signs out`;
- the complete UI `check` and test lanes, with 179 tests in the remediation record;
- the filtered `production_ui_bff_journey`, with all four UI journeys passing;
- repository format, lints, and `git diff --check`.

I independently ran `git diff --check` over the immutable base-to-candidate range; it returned no errors. Per task direction, I did not require or rerun the full unfiltered identity or every-language journeys during task remediation review.

## Material findings

None.

## Non-blocking notes

- `production-auth.integration.test.ts` repeats the sentence “Without an Active connection, SSO comes back `access_denied` to the sign-in problem page” on two adjacent comment lines. This is wording-only, has no behavioral or public-contract consequence, and cannot block under the standing direction.
- The recovery route's comment includes the label `REQ-010`. The adjacent prose already states the contract, so the label may age independently, but this is also wording-only and non-blocking.

## Uncertain preferences

None. `browser-sessions.ts` is a large module, but its methods share the same OAuth configuration, cookie-encryption key, and access-token cache. Splitting it would separate one cohesive lifecycle without a demonstrated maintenance benefit.

## Overall result

**PASS**

The cumulative candidate has one discoverable production browser-session owner, conventional library-backed OAuth and JWE operations, thin typed route consumers, complete caller cleanup, synchronized declarations, focused remediation proof, and no material maintainer defect. The remediation deletes the private refresh-token assumption and implements the lead-directed standard logout behavior without adding nonstandard mechanisms or configuration.
