# TASK-003 invariant review, remediation round 3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `8289fa298ed33d21f2568558bc0a02905fd0b218`
- Approved authority: `SPEC-oidc-production-readiness` revision 7 at
  `changes/active/oidc-production-readiness/spec.md`
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Prior verdict, validation, and remediation:
  `changes/active/oidc-production-readiness/review/TASK-003-r2/`
- Human directions:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  and
  `changes/active/oidc-production-readiness/review/TASK-003-r2/human-direction-connection-test.md`

The complete base-to-candidate range was audited. The latest remediation range
`622a77028..8289fa298ed33d21f2568558bc0a02905fd0b218` was used only to locate
changed owners and prove prior-finding closure. CodeGraph is not configured in
this repository. The candidate resolved to the stated commit before and after
review.

## Proposed findings

None. The producer-to-sink audit found no remaining task-scoped invariant
violation in the cumulative candidate.

## Invariant navigation map

| Producer / authority | Consumers and sinks traced | Invariant |
|---|---|---|
| Candidate revision, authorized tester, PKCE/state/nonce, provider discovery | `test_candidate` -> `HumanConnections::begin_test` -> `auth_login_state` -> common callback -> `tested_candidate` / `stamp_test_sign_in` | Only the exact candidate revision can be marked tested, after a real verified login and a current permission decision; a test issues no identity, credential, completion, or browser session. |
| Login-state hash and initiation columns | definer tenant lookup -> tenant RLS consume -> callback exchange -> completion or test stamp | State is single-use, bounded, tenant-owned, and exactly one of browser, CLI, or connection-test binding; paths and provider returns never choose tenant authority. |
| `CallbackQuery.iss`, discovery support flag, recorded issuer | callback -> `verify_response_issuer` -> token endpoint | Present issuer is exact-matched before token IO; absence is refused only for an advertising provider under the approved human direction. |
| Browser access-token expiry and renewal result | `lock_browser_session` -> `BrowserSessions::current` -> read/authority | A refused proactive renewal commits no mutation and preserves only the already-issued token until PostgreSQL says it expired; post-expiry refusal revokes the session. |
| Browser-session sealed columns | `SealedSecretTable::ALL` -> `sealed_tenant_secrets` -> rewrap CAS -> keyless boot | Every stored non-null envelope, including expired-session ciphertext, remains in the canonical key inventory until wiped or purged. |
| Session cookie hints and stored session id | `ServerSessions.read` / sequential `metadata` -> chooser/switch/routes | Browser hints select only bounded lookup work; rendered tenant identity and effective authority come from the server-returned session. |
| `WYRD_SERVER_URL` | readiness, private BFF channel, ordinary upstream API calls | Absence alone selects the local default; invalid explicit input refuses, non-loopback plaintext refuses, and a production BFF uses the native client across trusted TLS. |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-006 / human direction: testing performs one real sign-in through the exact candidate and only that revision becomes activatable | `HumanConnections::begin_test` performs screened discovery and JWKS usability checks, writes ordinary PKCE/state/nonce login state bound to candidate revision and `ConnectionTester`; `AuthorizationCodeExchange` verifies the real code and ID token; `stamp_test_sign_in` rechecks current stored permission and atomically audits/stamps the still-matching candidate (`wyrd-auth/src/connections.rs:316-520`, `callback.rs:144-289`). The former side-effect probes and fake-code constants are absent. | `tenant_connection_test_sign_in_journey`; callback PG tests for allowed, unauthorized, and failed-audit completion; UI replacement and Dex activation paths. | PASS |
| REQ-003 / AC-006: a test creates no `User`, Wyrd credential, completion, or browser session, and replay, expiry, cross-tenant state, wrong callback, and wrong secret fail | Test initiation has mutually exclusive persisted tester columns and a database constraint forbidding `completion_sealed`; the callback returns immediately after `stamp_test_sign_in` before user/session issuance (`20261001000002_auth_connection_test_state.sql:10-22`, `callback.rs:241-290`). | `tenant_connection_test_sign_in_journey` measures all durable issuance tables around success and covers replay, expiry, cross-tenant mix-up, and wrong callback; rotation journey covers wrong secret. | PASS |
| REQ-005 / R2-AC-01: canonical sealing inventory covers expired stored browser ciphertext and keyless use refuses | All four browser queries select every non-null envelope without liveness or expiry predicates; exact-byte CAS remains the sole writer (`human_connections.rs:381-543`). | `keyless_boot_refuses_while_an_expired_browser_session_envelope_remains`; retained browser-session sealing rotation journey. | PASS |
| REQ-005 / REQ-009 / R2-AC-02: the secret-bearing BFF channel is TLS-protected outside loopback and works through native production fetch | `serverUrl` admits only HTTPS or literal loopback HTTP and now preserves explicit empty configuration for rejection (`upstream.ts:3-26`). One built BFF receives a trusted `https://localhost` endpoint and repository-generated CA; the Node TLS terminator forwards to the same Wyrd listener (`identity_ui_e2e.rs:104-209,452-458`). | `production SSO crosses replicas` drives completion, reads, authority, actions, and logout across both replicas; focused upstream tests retain loopback and pre-fetch plaintext refusal. | PASS |
| REQ-006 / INV-001: callback tenant and connection come only from server-owned one-use state | The raw state is hashed; the definer lookup yields only its owning tenant; RLS consumption commits before provider IO; the stored exact connection/candidate revision, issuer, client, redirect, verifier, and nonce govern the rest (`login_state.rs:29-95,221-340`, `callback.rs:114-215`). | Issuer-binding, ordinary login, connection-test, replay, expired-state, cross-tenant, and mixed-callback journeys. | PASS |
| Human issuer direction / R2 preserved behavior: conditional RFC 9207 behavior is unchanged | Discovery projects absent support as false; `verify_response_issuer` exact-matches present `iss` and requires it only when advertised, before code exchange (`provider.rs:18-105`, `callback.rs:189-205,613-640`). | Provider projection and callback unit tests; `tenant_callback_issuer_binding_journey`; browser mixed-callback cases retain genuine `iss`. | PASS |
| REQ-009: session authority is replica-safe, one-use, tenant-bound, CSRF-protected, and absent from browser-visible data | Postgres/RLS owns session state; only hashes are stored for IDs and CSRF comparison; credentials are sealed; BFF revalidates server tenant, expiry, origin, and constant-time CSRF; page projection omits tenant UUID and credentials. | Production two-BFF journeys cover flow mismatch/replay, forged and cross-named cookies, CSRF/origin, logout, concurrent access, and page/URL/cookie leak checks. | PASS |
| REQ-010: OIDC-off entry reuses existing API-key authority and fixed-cost refusal | `BrowserSessions::exchange_api_key` routes valid-tenant input through shared `ExchangeApiKey`; missing/unknown routing still performs the single dummy verification and every refusal is indistinguishable. | Exhaustive fixed-cost API-key test and `OIDC-off credential UI`. | PASS |
| REQ-015 / AC-003 / R2-AC-03: independent tenants on different real providers switch only through their own sessions; wrong-provider and same-issuer callbacks fail | Host activates Keycloak, Dex, and a same-Keycloak peer as separate tenant connections. Mixed callback helper preserves the provider's complete query and changes only `state` (`identity_ui_e2e.rs:419-442`; `production-auth.integration.test.ts:190-211,481-566`). | `production multi-provider tenant switch` establishes Keycloak and Dex sessions across both BFFs and proves wrong-provider and same-issuer cross-tenant refusal with no completion/session. | PASS |
| REQ-016 / R2-AC-04: refused early renewal preserves current authority to exact expiry and commits no tentative state | On `Renewal::Refused` for an unexpired row, `current` rolls back, relocks once, and serves only the stored token; an expired refusal revokes and commits. Infrastructure failures still fail closed (`browser_sessions.rs:435-585`). | `proactive_renewal_refusal_preserves_authority_until_expiry`; production SSO and provider-replacement journeys poll through pre-expiry use to first post-expiry refusal. | PASS |
| REQ-018 / AC-009: UI, schemas, and setup docs describe interactive candidate testing and production session behavior | `ConnectionTestResponse.authorization_url` is typed and documented; settings test action redirects to it; setup docs describe the real sign-in and no-issuance behavior. | Recorded codegen, docs, UI typecheck/unit, and identity journey evidence; source/schema comparison agrees. | PASS |
| INV-003: platform administrators, tenant users, and workloads remain distinct | Candidate tester kinds re-resolve only tenant User/service-account stores; `GlobalAdmin` and `System` are refused; provider claims never mint platform authority (`connections.rs:856-898`). | Authorized/unauthorized tester PG tests plus tenant role allow/deny journeys. | PASS |
| INV-005: UI projects server-owned identity and permissions | Session read returns the authoritative tenant/principal/permissions; TS maps them without local durable role or membership logic. Chooser hints are resolved sequentially through authenticated server reads (`server-sessions.ts:186-230,283-309`). | Tenant-context, chooser-bound, settings authorization, cross-tenant, and multi-provider journeys. | PASS |
| R2-AC-05: discovery additions have substantive rustdoc | Raw RFC 9207 field documents absent-as-false semantics; fallible parser documents its workflow and malformed URL errors (`provider.rs:18-105`). | Direct source inspection and recorded lint/provider test evidence. | PASS |
| R2-AC-06: browser cookie hints cannot create unbounded concurrent private reads | `metadata` deduplicates valid-looking hints and awaits each `read` sequentially, bounding authenticated private/store work to one in flight (`server-sessions.ts:283-309`). | `production chooser bounds server verification`; retained chooser and switching tests. | PASS |
| R2-AC-07: empty upstream configuration does not silently target loopback | Nullish-only fallback applies only when the variable is absent; an empty string reaches native URL parsing and fails before any request (`upstream.ts:16-26`). | `empty upstream value is refused before fetch`; retained HTTPS, loopback, and plaintext cases. | PASS |
| Explicit non-goals and prohibited changes | No browser bearer storage, UI role mapper, local password authority, hosted-signup stub, new callback route, provider-specific test branch, soft-pass test status, compatibility alias, or second sealing/session owner entered the cumulative diff. | Complete base-to-candidate diff and owner/caller inspection. | PASS |

## Prior-finding closure

| Prior finding | Closure result | Current source evidence |
|---|---|---|
| `FIND-TASK-003-1` | CLOSED under the explicit human replacement | Optional typed response `iss`, projected discovery support, and the pre-token exact comparison remain intact. |
| `FIND-TASK-003-2` | CLOSED | Browser API-key entry retains the shared fixed-cost verifier/dummy path. |
| `FIND-TASK-003-3` | CLOSED | Chooser hints are server-resolved before rendering; remediation further bounds their resolution to one in flight. |
| `FIND-TASK-003-4` | CLOSED | Canonical inventory now includes every stored non-null browser envelope regardless of expiry. |
| `FIND-TASK-003-5` | CLOSED | A built BFF uses native fetch through a trusted real TLS terminator; transport policy remains enforced at the common origin owner. |
| `FIND-TASK-003-6` | CLOSED | The two-BFF journey now uses Keycloak and Dex and preserves genuine callback parameters in mix-up attempts. |
| `FIND-TASK-003-7` | CLOSED | Previously cited Rust items remain substantively documented. |
| `FIND-TASK-003-8` | CLOSED | Authoritative tenant UUID remains server-side and no fabricated sentinel returns. |
| `FIND-TASK-003-9` | CLOSED | The speculative session-lifetime variant and dead SQL branch remain absent. |
| `FIND-TASK-003-10` | CLOSED | Refused proactive renewal rolls back and preserves current authority only until stored expiry. |
| `FIND-TASK-003-11` | CLOSED | Raw discovery support field and parser now meet the private-item and fallible-function rustdoc rules. |
| `FIND-TASK-003-12` | CLOSED | Cookie-hint verification is sequential and keeps existing deduplication and clearing behavior. |
| `FIND-TASK-003-13` | CLOSED | Explicit empty upstream configuration is no longer replaced by loopback. |

## Verification assessment

- Reviewed the complete cumulative diff, the remediation locator diff, current
  implementation bodies, SQL migration and queries, BFF routes/session owner,
  provider fixture, and focused and journey test source.
- `git diff --check
  63c5bffc93cd2f7b5ed558e610a213efcc34fd49..8289fa298ed33d21f2568558bc0a02905fd0b218`
  passed.
- Per assignment, no Cargo, mise, pnpm, provider, browser, or Postgres command
  was run. The remediation packet records green focused tests, identity/UI
  journeys, codegen, docs, SQL, format, lint, and boundary lanes. Source review
  found those proofs aligned with the changed owners; this report does not
  independently reproduce their runtime results.

## Overall result

**PASS**

All original TASK-003 obligations, prior findings, both human directions, and
the revision-7 real-sign-in connection-test addition are closed in current
source. The proposed finding ledger is empty.
