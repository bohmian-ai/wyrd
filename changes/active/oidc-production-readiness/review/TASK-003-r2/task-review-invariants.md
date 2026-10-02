# TASK-003 invariant review, remediation round 2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Approved authority: `SPEC-oidc-production-readiness` revision 5 at
  `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20`
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Prior verdict and validation:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/{verdict,findings-validation}.md`
- Remediation task:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/TASK-003-R1-production-ui-remediation.md`
- Human amendment:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  replaces prior `FIND-TASK-003-1` and `R1-AC-01`.

The complete base-to-candidate range was audited. The remediation-only range
`4e0ca8d2e..05ff68fb4` was used only to locate the changed owners. CodeGraph is
not configured in this repository. The candidate remained at the stated commit
through this review.

## Invariant navigation map

| Producer / authority | Consumers and sinks traced | Invariant |
|---|---|---|
| `CallbackQuery.iss`, discovery metadata, `LoginState.issuer` | public callback route -> `AuthorizationCodeExchange::execute/complete` -> token endpoint | A response issuer is checked against server-bound state before token IO, under the human amendment's conditional RFC 9207 rule. |
| `ExchangeApiKey` and the fixed dummy verifier | `/auth/token`, `BrowserSessions::exchange_api_key`, later API-key-mode renewal | Every presented key class pays one shared expensive verification and every refusal is indistinguishable. |
| Browser flow/session digests and RLS rows | BFF complete/read/authority/logout, hooks, tenant context, chooser, switch, settings actions | Tenant and principal come from verified server state; cookie names and paths are hints only. |
| Browser session access/refresh/API-key/CSRF envelopes | canonical `SealedSecretRewrap`, keyless boot, renewal, read, logout | Every live recoverable credential participates in one inventory and survives supported rotation races/restarts. |
| `serverUrl()` | login, all internal BFF calls, ordinary `/v1` calls, readiness | Secrets leave the BFF only over HTTPS, except the explicitly allowed literal-loopback test/local topology. |
| `access_expires_at` and connection/key lifecycle | `lock_browser_session` -> `BrowserSessions::current/renew` -> BFF read/authority | A failed renewal cannot extend authority, but an already issued access token retains its bounded authority until its own expiry. |
| Server `DataTenantId` | `BrowserSessionView` -> internal `ReadResponse` -> TS `Read`/`ServerSession` -> `TenantContext` | No empty or browser-derived tenant identifier enters server-side UI context, and the UUID stays out of page metadata. |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-006: authorized settings project test, activate, replace, deactivate, and remove without cross-tenant administration | Settings actions call the existing server contract with session authority; replacement remains server-owned. | `production provider replacement settings` exercises stage/remove/restage/test/activate, wrong recovery credentials, replacement login, denial, and retired-connection removal. | PASS |
| REQ-005: recoverable browser credentials are sealed, keyless use refuses, and rotation does not strand sessions | `SealedSecretTable::ALL` includes all four live browser-session columns; byte-exact CAS prevents overwriting renewal/logout. | `browser_session_sealing_rotation_journey` covers both modes, a lost CAS, later zero, keyless refusal, K2-only read/renew/action/logout. | PASS |
| REQ-006 / INV-001: login and callback tenant selection remains server-bound | Flow/state digest resolves the tenant; callback route carries `iss`; `verify_response_issuer` runs after state/connection binding and before token exchange. | Contract/unit and `tenant_callback_issuer_binding_journey` cover matching, mismatched, advertiser-missing, non-advertiser-missing, state consumption, and zero token calls on refusal. | PASS under the explicit human amendment |
| REQ-009: production session is replica-safe, one-use, tenant-bound, CSRF-protected, and browser-safe | Postgres row authority, row locking, secure cookies, fixed completion route, server-returned tenant comparison, same-origin/constant-time CSRF check, and sealed tokens are present. | Unit and real two-BFF journeys cover replay, mismatched flow, forged/cross-named cookie, CSRF, leaks, logout, and replica crossing. The production-shaped private channel itself is not exercised over TLS. | FAIL — `INV-R2-01` |
| REQ-010 / R1-AC-02: OIDC-off entry reuses existing API-key authority with fixed-cost refusal | Known tenants call `ExchangeApiKey::execute`; unknown route tenants call exactly one `verify_presented(_, None)`. | `every_browser_api_key_sign_in_costs_exactly_one_verification` covers malformed, unknown route/prefix, cross-tenant, wrong secret, expired/revoked, and valid classes. | PASS |
| REQ-015 / AC-003: independently authenticated tenants switch only through their own verified sessions | `metadata` resolves each cookie hint through `read`; `switch` re-reads the target session; session tenant comes from the row. | `production multi-provider tenant switch` covers two different issuers, an independently authenticated target, successful switching on both replicas, wrong-issuer/state, same-issuer cross-tenant refusal, forged hint removal, and distinct tenant principals. | PASS |
| REQ-016: an old-connection session cannot renew, while its already issued access token remains valid until its bounded expiry | `RefreshTokens` rechecks the exact connection binding. However `BrowserSessions::current` classifies any token inside the one-minute margin as stale, attempts renewal, and revokes the browser row immediately on refusal even while `access_expires_at` is still future. | The UI host deliberately uses a 30-second access TTL, so every read is inside the margin; the deactivation assertions expect immediate redirect and do not prove survival to the issued expiry. | FAIL — `INV-R2-02` |
| REQ-018 / AC-009: docs, UI, generated contracts, and failure behavior agree | Rotation and issuer-binding documentation were updated; callback schemas carry optional `iss`; UI exposes production rather than mock login. | Recorded `codegen:check` and `docs:check`; source/schema comparison agrees. | PASS |
| INV-003: platform administrators, tenant users, and workloads remain distinct | SSO uses User refresh authority; OIDC-off uses tenant API-key exchange; neither provider groups nor the BFF key create platform authority. | Role allow/deny, wrong-tenant, and replacement non-inheritance journeys. | PASS |
| INV-005 / R1-AC-08: UI projects server-owned identity and permissions | `DataTenantId` flows through `BrowserSessionView`, private JSON, TS session, and `TenantContext`; page metadata maps only key/name. | `production tenant context carries the server tenant id outside page metadata` and real browser leak checks. | PASS |
| R1-AC-01, as replaced by human direction: conditional RFC 9207 issuer binding | Present `iss` is exact-matched; absent `iss` is refused only when discovery advertises support; connection activation is not gated by the flag. | Focused contract/provider/callback tests and the issuer-binding journey. | PASS |
| R1-AC-03: chooser entries are server-verified | `metadata` treats suffixes as hints and calls `read`; unknown, expired, duplicate, and mismatched hints are omitted/cleared. | Focused chooser tests plus multi-provider journey. | PASS |
| R1-AC-04: canonical rotation covers browser sessions | Existing rewrap owner and operator CAS cover access, refresh, API key, and CSRF envelopes. | Focused keyless boot checks and full sealing rotation journey. | PASS |
| R1-AC-05: HTTPS and loopback HTTP are admitted appropriately, and the real production-shaped channel has TLS proof | `serverUrl()` rejects non-loopback HTTP before a fetch and accepts HTTPS syntactically. | `upstream.test.ts` uses a recording fake for HTTPS; `identity_ui_e2e.rs` still gives both production BFF processes the loopback HTTP server URL. No test performs the authenticated BFF exchange over TLS. | FAIL — `INV-R2-01` |
| R1-AC-06: complete browser journey coverage | Existing host and two real BFF processes now cover multi-provider switching and replacement. | Both added filtered scenarios plus retained original scenarios are recorded green. | PASS |
| R1-AC-07: cited Rust documentation is accurate | The prior misplaced/missing rustdoc is corrected on the changed items. | Recorded lints and direct source inspection. | PASS |
| R1-AC-09: no speculative lifetime variant remains | `BrowserSessionWrite` carries one `Duration`; SQL derives absolute expiry from the PostgreSQL clock. | No `SessionLifetime` symbol remains; recorded SQL lane. | PASS |
| Explicit non-goals and prohibited changes | No browser bearer storage, UI role mapper, password authority, hosted-signup stub, new dependency, feature, callback route, or compatibility alias was added. | Complete diff and caller review. | PASS |

## Prior-finding closure

| Prior finding | Closure result | Source evidence |
|---|---|---|
| `FIND-TASK-003-1` | CLOSED as amended by human direction | Optional typed `iss`, discovery support flag, and pre-token exact comparison implement the replacement decision; residual exposure is documented. |
| `FIND-TASK-003-2` | CLOSED | Browser API-key entry uses the shared verifier or exactly one dummy verification; exhaustive focused counting test exists. |
| `FIND-TASK-003-3` | CLOSED | Chooser hints are resolved through server `read` before projection and invalid hints are omitted/cleared. |
| `FIND-TASK-003-4` | CLOSED | Canonical rewrap inventories all live browser envelopes and the rotation journey proves both modes plus CAS recovery and keyless/K2-only behavior. |
| `FIND-TASK-003-5` | **OPEN** | Scheme enforcement exists, but its decision-complete closure criterion also required a real TLS channel proof; the production BFF journey still uses loopback HTTP and the HTTPS test never opens a connection (`INV-R2-01`). |
| `FIND-TASK-003-6` | CLOSED | Multi-provider switch and provider-replacement settings journeys cover the previously absent seams. |
| `FIND-TASK-003-7` | CLOSED | The cited trait/environment items now carry correctly attached substantive rustdoc. |
| `FIND-TASK-003-8` | CLOSED | The authoritative tenant UUID reaches server-only context and no empty sentinel remains. |
| `FIND-TASK-003-9` | CLOSED | The unused enum and nullable dual-lifetime branch were deleted. |

## Proposed findings

### INV-R2-01 — MISSING: the required production TLS channel proof is still absent

- **Violated obligation:** TASK-003 fixes the BFF service-key channel over TLS;
  R1-AC-05 explicitly requires that “the real production-shaped channel has
  TLS proof.” Repository journey rules require a real client-to-server path for
  a user-facing capability; a fake fetch does not substitute for that path.
- **Exact location:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.test.ts:8-32`;
  `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:91-105,257-267,302-303`.
- **Evidence:** The HTTPS test only returns the configured string and passes a
  recording function that fabricates a response; it establishes no TLS
  connection, certificate validation, or authenticated request. The real
  production `node build` BFFs receive `srv.base_url()`, which the host starts
  as loopback HTTP. All service-key, API-key, session-id, CSRF, and access-token
  journey traffic therefore exercises the local exemption, not the required
  production transport.
- **Observable consequence:** The lane remains green if the production Node
  runtime cannot establish or validate the intended HTTPS connection, or if a
  later change routes the secret-bearing channel differently only under HTTPS.
  The parser-level refusal closes the plaintext configuration bug but not the
  remediation's required end-to-end proof, so prior `FIND-TASK-003-5` is not
  closed.
- **Required testable correction:** Reuse the existing identity UI host and BFF
  journey, but expose the real Wyrd test listener through a repository-managed,
  trusted TLS endpoint and point at least the production-shaped secret-bearing
  BFF path at its `https:` origin. Prove a real session operation succeeds with
  server authentication and retain the focused assertion that non-loopback
  HTTP is refused before the fetcher sees a request. Add no bypass flag,
  alternate client, or second journey harness.

### INV-R2-02 — INCORRECT: proactive renewal can revoke valid browser authority before access-token expiry

- **Violated obligation:** Approved REQ-016 says an old-connection BFF session
  cannot renew and requires login **once its current access token expires**;
  already issued access tokens retain bounded snapshot authority unless an
  existing stronger principal/tenant guard applies. TASK-003 repeats that a
  failed, revoked, or old-connection refresh ends the session at access expiry,
  and the R1 remediation preserves the current access-token lifetime and rejects
  instantaneous revocation claims.
- **Exact location:** `crates/wyrd/wyrd-auth/src/browser_sessions.rs:51-55,435-488`;
  proof at
  `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:246-264` and
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts:385-392`.
- **Evidence:** `lock_browser_session(..., RENEW_MARGIN)` marks an access token
  stale throughout its final minute. `current` then attempts renewal and, for
  every policy refusal including an inactive/replaced connection or revoked API
  key, wipes and revokes the browser row immediately. It never distinguishes an
  actually expired token from a still-valid token renewed early. The real UI
  host sets a 30-second access lifetime—always inside the one-minute margin—and
  the journey asserts immediate redirect after deactivation, so the existing
  proof positively pins the early cutoff instead of the approved expiry
  boundary.
- **Observable consequence:** A provider replacement/deactivation, API-key
  revocation, or another refused proactive renewal can log a browser out as
  much as one minute before its already issued authority expires. That changes
  the approved lifecycle semantics and makes the UI behave like instantaneous
  revocation for short-lived tokens even though the underlying access token is
  still valid.
- **Required testable correction:** Keep renewal and lifecycle ownership in
  `BrowserSessions`. Distinguish “inside the proactive margin” from “expired”
  using the same PostgreSQL-clock authority. When an early renewal is refused,
  preserve/serve the existing access token only until its stored expiry and do
  not commit partial refresh-side mutations; after expiry, revoke/refuse exactly
  as today. Keep infrastructure failures fail closed and do not retry another
  credential. Add a focused real-store test that deactivates or replaces the
  bound connection while the current token is still valid, proves authority
  remains usable only to its exact expiry, and proves the first post-expiry use
  yields no token/session or renewal. Retain concurrent row-lock and ordinary
  renewal coverage.

## Verification assessment

- Reviewed the full cumulative diff, remediation locator diff, runtime owners,
  SQL/migration, TypeScript BFF boundary, hooks/routes, and focused/real journey
  source.
- `git diff --check
  63c5bffc93cd2f7b5ed558e610a213efcc34fd49..05ff68fb47572e7d8e5fa34037042559bcfeac83`
  passed.
- Per the assigned review constraints, no Cargo, mise, or pnpm lane was rerun.
  The candidate records green UI unit/typecheck, identity journey, Rust/SQL,
  codegen, tenant-isolation, docs, format, and lint lanes. Those results do not
  supply the missing TLS connection or the approved pre-expiry lifecycle proof.

## Overall result

**FAIL**

Eight prior findings are closed from source. Prior `FIND-TASK-003-5` remains
open because its required real TLS proof is absent, and the cumulative candidate
also ends a still-valid browser access token early when proactive renewal is
refused.
