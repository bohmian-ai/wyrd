# TASK-003 R2 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Approved specification: revision 5 at
  `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20:changes/active/oidc-production-readiness/spec.md`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Prior review: `changes/active/oidc-production-readiness/review/TASK-003-r1/`
- Remediation task:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/TASK-003-R1-production-ui-remediation.md`
- Human amendment:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  replaces `FIND-TASK-003-1` and `R1-AC-01`.

The cumulative base-to-candidate behavior was reviewed. The remediation diff
was used only to locate the nine prior correction boundaries. `HEAD` remained
the candidate above during this review.

## Navigation and caller map

| Flow | Owners and consumers inspected |
|---|---|
| Provider callback | `CallbackQuery` -> Axum callback -> `AuthorizationCodeExchange::execute` -> state consume -> fresh discovery -> response-issuer check -> token endpoint -> ID-token verification -> sealed completion |
| OIDC-off sign-in | SvelteKit login action -> internal BFF API-key route -> `BrowserSessions::exchange_api_key` -> shared `ExchangeApiKey` / fixed-cost verifier -> sealed session |
| Protected UI and switching | hooks -> `ServerSessions::read` -> internal session read -> `BrowserSessions::read/current` -> tenant context, metadata chooser, switch, action and API paths |
| Sealing lifecycle | browser-session writers/renewal/logout -> `SealedSecretTable` inventory and CAS -> `SealedSecretRewrap` -> boot keyless decision and operator runbook |
| Deployment channel | `serverUrl` -> all BFF internal calls -> real two-BFF identity host and Vitest journeys |
| Provider and replacement journeys | `identity_ui_e2e` tenant/provider setup -> production BFF processes -> multi-provider/switch and replacement/settings Vitest paths |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003: tenant settings project the authorized headless connection contract | Settings actions use the normal `/v1/identity/oidc/*` API through `ServerSessions.api`; server remains mutation authority | Original production journeys plus `production provider replacement settings` exercise stage, test, activate, and remove/retire with allowed and denied principals | PASS |
| REQ-005: browser credentials are sealed, key rotation covers their durable envelopes, and keyless boot refuses while such envelopes remain | Session creation seals access, refresh/API key, and CSRF. The canonical table inventory adds four browser-session columns | Live-session K1/K2 journey and live-session keyless test pass, but the inventory excludes expired non-null rows and therefore does not prove the amended R1 criterion | FAIL (`BEH-R2-001`) |
| REQ-006: tenant login uses the deployment origin, server state selects tenant, and callback carries no tenant selector | Login route sends only route context before state creation; callback resolves the state and redirects to the fixed completion URL | SSO journeys begin at one BFF, callback through the server, and complete at the other | PASS |
| REQ-007 and amended R1-AC-01: apply RFC 9207 conditionally without refusing non-advertising providers | Typed optional `iss`; fresh discovery support flag; exact comparison before token endpoint; absent accepted only when support is not advertised | Contract/unit tests and `tenant_callback_issuer_binding_journey` cover matching, mismatch, advertising absence, non-advertising absence, zero token calls, consumed state, and activation of a non-advertising provider | PASS |
| REQ-009: replica-safe production BFF session with safe cookie/page data, CSRF, expiry, binding, renewal, and logout | Postgres-backed session, host-only Secure/HttpOnly/SameSite cookie, same-origin and constant-time CSRF checks, server-only authority call | Original focused UI tests and real `production SSO crosses replicas` / `OIDC-off credential UI` journeys are retained | PASS |
| REQ-010 and R1-AC-02: OIDC-off credential entry reuses shared exchange with one expensive verification per presented class | Browser session path resolves the route tenant, then invokes `ExchangeApiKey`; unknown route invokes the shared dummy verifier once | Postgres verification-count tests cover malformed, unknown-route, cross-tenant, unknown-prefix, wrong-secret, revoked/expired, and valid inputs | PASS |
| REQ-015: switching uses an independently verified target session and does not rebind the current session | `ServerSessions.switch` reads both current and target through the server; chooser metadata resolves every cookie hint through `read` | Real switch journey establishes separate sessions and switches in both directions; forged hint and crossed session-id cases are refused | PASS |
| REQ-016: an old-connection session stops renewing after replacement/deactivation | Browser renewal returns refusal and revokes the session when the connection-bound refresh no longer issues | Original deactivation case and replacement journey show the old session refused on both replicas | PASS |
| REQ-018 / AC-009: docs, UI, and generated contracts agree | Callback schema includes optional `iss`; security and self-hosting docs describe conditional issuer binding and session-key rotation | Recorded `codegen:check` and `docs:check` are green; source/generated callback schemas agree | PASS |
| INV-001: path, cookie hints, and foreign callback data cannot select effective tenant authority | State/session rows select tenant; cookie suffixes are hints only; session read returns authoritative tenant id | Chooser tests and real cross-cookie cases pass. The purported wrong/same-issuer callback cases omit the provider's `iss`, so they do not prove a valid mixed callback reaches the intended tenant/PKCE defenses | FAIL (`BEH-R2-003`) |
| INV-003 / INV-005: UI projects server-owned principal, roles, permissions, and tenant id without creating a second authority | `BrowserSessionView` -> private `ReadResponse` -> `ServerSession` carries authoritative tenant UUID; context and page metadata stay separate | Focused tenant-context test plus real no-secret page checks | PASS |
| AC-001: OIDC-off deployment reaches a real UI with an existing Wyrd credential and no mock auth | Production login exposes API-key action only when SSO is absent | `OIDC-off credential UI` real journey | PASS |
| AC-002: controlled real provider UI login maps roles and proves allowed/denied action | Keycloak connection and two users traverse the production BFF | `production SSO crosses replicas` | PASS |
| AC-003 / R1-AC-06: two tenants with different real providers operate concurrently, switch successfully, and refuse wrong-tenant and same-issuer callback mixing | The host configures two realms of the same Keycloak process plus a same-issuer peer; no Dex-backed active tenant is used | The multi-provider journey proves independent issuers/sessions and switching, but not the required different-provider topology; its callback-mix helper also drops `iss` from both callbacks | FAIL (`BEH-R2-003`) |
| AC-006: replacement preserves recovery, creates a distinct user, and does not inherit old authority by email | Settings stage/test/activate; new realm login; headless recovery credential retires old connection | Replacement journey proves distinct principal, empty roles, denied UI administration, working recovery key, and old-session cutoff | PASS |
| AC-007 / R1-AC-04: sealing rotation, absent keys, two replicas, flow binding, replay and old-connection cutoff | Live browser envelopes participate in the canonical CAS pass; callback/session negative flows remain | Live K1/K2 rotation and live keyless refusal are covered, but an expired row with non-null envelopes is outside both inventory and keyless proof | FAIL (`BEH-R2-001`) |
| R1-AC-03: only server-verified tenant choices render | `metadata` deduplicates cookie suffixes and calls `read` for each | Focused forged/expired/duplicate/mismatch tests and real forged-hint journey | PASS |
| R1-AC-05: HTTPS and loopback HTTP posture, including a real production-shaped TLS channel | `serverUrl` permits HTTPS and loopback HTTP only | URL/fetcher unit tests prove validation; the only real BFF host still points both BFF processes at a loopback HTTP server, so no TLS request carrying the BFF key/session material is exercised | FAIL (`BEH-R2-002`) |
| R1-AC-07: cited Rust items have substantive rustdoc | `Debug::fmt`, `CreatedResponse::from`, `env_opt`, and hash parser docs are corrected | Recorded lint result is green | PASS |
| R1-AC-08: authoritative tenant UUID reaches server context but not page metadata | Rust and TypeScript private session projections carry `tenant_id`; `context` uses it | Focused context test; layout returns only key/name; real leak checks retained | PASS |
| R1-AC-09: dead dual-lifetime branch is gone | `BrowserSessionWrite` carries one `Duration`; SQL computes expiry from `statement_timestamp()` | Symbol inspection finds no `SessionLifetime`; recorded SQL lane is green | PASS |
| Non-goals: no browser bearer storage, local password authority, UI role mapper, hosted signup, compatibility route, or new auth path | Cumulative diff keeps credentials server-side and reuses existing owners | Source inspection and real leak tests | PASS |

## Proposed findings

### BEH-R2-001 — Expired browser-session ciphertext is omitted from the canonical inventory

- **Classification:** INCORRECT
- **Prior finding:** `FIND-TASK-003-4` is not fully closed.
- **Violated obligation:** R1-AC-04 requires keyless boot to refuse while any
  session envelope remains, and the remediation diagnosis/correction requires
  every non-null `auth_browser_sessions` sealed column to participate in the
  canonical inventory. The `SealedSecretRewrap` contract also says keyless
  boot counts every stored ciphertext.
- **Exact location:**
  `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:478-504`;
  `crates/wyrd/wyrd-sql/src/queries/auth/browser_sessions.rs:21-24,225-233`;
  `crates/wyrd/wyrd-server/src/boot/mod.rs:1515-1524,2872-2924`.
- **Evidence:** every browser-session inventory query requires
  `absolute_expires_at > statement_timestamp()`. Expired rows are purged only
  by `insert_browser_session`, scoped to the tenant creating another session;
  boot performs no purge. An expired row for a tenant that never signs in
  again can therefore retain access/API-key/refresh/CSRF envelopes forever,
  contribute nothing to `remaining`, and let keyless boot succeed. The focused
  boot test covers a live row and then a revoked row whose ciphertext was
  wiped; it never covers an expired row whose columns remain non-null.
- **Observable consequence:** the operator can receive `remaining == 0` or
  successfully boot without a key while the database still holds browser
  credential envelopes excluded from the claimed canonical inventory.
- **Required testable correction:** use the existing canonical operator
  inventory for every non-null browser-session envelope, as the approved
  remediation specifies; the minimum source change is to remove the absolute
  expiry predicates (revoked rows already have null columns). Add one focused
  boot/inventory case with an expired, non-null session row and require it to
  count as remaining and refuse keyless boot.

### BEH-R2-002 — The required real TLS BFF channel proof is still absent

- **Classification:** MISSING
- **Prior finding:** `FIND-TASK-003-5` has an implemented guard but incomplete
  closure proof.
- **Violated obligation:** R1-AC-05 requires a real production-shaped TLS
  channel exercise in addition to URL validation; the BFF channel carries the
  raw deployment key, API key, session ids, CSRF token, and access-token
  response.
- **Exact location:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.test.ts:8-32`;
  `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:27-30,54-59,302-307`.
- **Evidence:** the HTTPS test supplies a recording function that returns an
  in-memory `Response`; it opens no TLS connection and validates no server
  certificate. The real two-BFF journey passes the bound server's loopback
  `http://` URL to both processes. Source search finds no TLS listener or
  certificate setup in that journey.
- **Observable consequence:** certificate trust, HTTPS transport, and the
  production BFF-to-Wyrd request path can regress while every recorded check
  remains green; the proof stops at URL syntax.
- **Required testable correction:** run an existing production BFF session
  operation through a repository-managed trusted TLS endpoint in the current
  identity journey (no new harness), and retain the loopback HTTP case for
  local topology. The proof must perform an actual HTTPS request carrying the
  BFF credential and receive the server response.

### BEH-R2-003 — The AC-003 browser journey does not exercise its required provider and mix-up topology

- **Classification:** MISSING
- **Prior finding:** `FIND-TASK-003-6` is not fully closed.
- **Violated obligation:** original AC-003 and TASK-003 require concurrent
  tenants with different real providers; R1-AC-06 additionally requires
  credible wrong-tenant and same-issuer refusal through the real browser
  journey. The remediation selected the already-running Keycloak and Dex
  providers for that proof.
- **Exact location:**
  `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:27-36,43-51,290-300`;
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts:193-205,460-466,510-525`.
- **Evidence:** both concurrently active "multi-provider" tenants use two
  realms of the same Keycloak process; Dex is not an active tenant provider.
  For the two mix-up cases, `providerLogin` returns the provider's complete
  callback URL, but the test extracts only `code` and constructs a new callback
  from `{code,state}`, dropping the actual `iss`. For an advertising provider
  this is refused by the missing-issuer rule before the foreign code can reach
  the tenant/PKCE path, so it does not prove a valid wrong-tenant or
  same-issuer callback is refused for the intended binding.
- **Observable consequence:** a regression specific to cross-provider BFF
  switching or to a standards-conformant mixed callback containing `iss`
  remains outside the primary journey while the test name reports coverage.
- **Required testable correction:** use the existing second provider service
  for the different-provider tenant (fix its existing connection-test fixture
  incompatibility rather than adding a provider or harness), and preserve the
  provider-produced callback parameters when substituting only the victim
  state. For the same-issuer case, pass the genuine matching `iss` so issuer
  validation succeeds and the test demonstrates refusal by the existing
  state/PKCE/tenant binding. Assert no token/session and retain successful
  switching on both replicas.

## Prior-finding closure

| Prior finding | Behavior-review status |
|---|---|
| FIND-TASK-003-1, as replaced by human direction | CLOSED |
| FIND-TASK-003-2 | CLOSED |
| FIND-TASK-003-3 | CLOSED |
| FIND-TASK-003-4 | OPEN — `BEH-R2-001` |
| FIND-TASK-003-5 | OPEN — `BEH-R2-002` (proof) |
| FIND-TASK-003-6 | OPEN — `BEH-R2-003` |
| FIND-TASK-003-7 | CLOSED |
| FIND-TASK-003-8 | CLOSED |
| FIND-TASK-003-9 | CLOSED |

## Verification assessment

The remediation record reports all requested focused and broader commands
green after code commit `1cf0124f6`, including UI unit/check, the four filtered
and unfiltered identity journeys, Wyrd and SQL lanes, codegen, tenant isolation,
docs, format, lints, and `git diff --check`. This review was static as directed
and did not rerun overlapping Cargo/mise work. The limits above are coverage
and acceptance gaps visible in source, not unexplained red commands.

## Overall result

**FAIL** — three bounded acceptance gaps remain. The amended issuer-binding
behavior is correctly implemented, but prior findings 4, 5, and 6 are not
fully closed by the source and proof at the immutable candidate.
