---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 5
requirements: [REQ-003, REQ-005, REQ-007, REQ-009, REQ-010, REQ-015, REQ-018, INV-001, INV-005, AC-001, AC-002, AC-003, AC-006, AC-007, AC-009]
depends_on: [TASK-003]
parent_task: TASK-003
remediates: [FIND-TASK-003-1, FIND-TASK-003-2, FIND-TASK-003-3, FIND-TASK-003-4, FIND-TASK-003-5, FIND-TASK-003-6, FIND-TASK-003-7, FIND-TASK-003-8, FIND-TASK-003-9]
---

# Production UI review remediation

## Authority and immutable inputs

- Approved specification:
  `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20:changes/active/oidc-production-readiness/spec.md`
  (`SPEC-oidc-production-readiness`, revision 5)
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Reviewed base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Reviewed candidate: `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
- Review verdict:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/verdict.md`
- Validated ledger:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/findings-validation.md`

## Outcome

Close the nine validated TASK-003 gaps without changing the approved product
or security model: bind every authorization response to its issuer, preserve
fixed-cost API-key refusal, derive tenant UI state only from server-verified
sessions, include browser sessions in the canonical sealing-key lifecycle,
enforce the existing TLS channel contract, complete the required real browser
journeys, restore mandatory Rust documentation, project the authoritative
tenant id, and remove the unused lifetime branch.

## Issue diagnoses and required corrections

### FIND-TASK-003-1 — Authorization response issuer binding

`CallbackQuery` in `crates/wyrd-spec/src/auth/oidc.rs` discards RFC 9207 `iss`,
the callback route forwards only `code` and `state`, and the exchange cannot
compare the response issuer before sending the grant to the token endpoint.
The login state already records the exact selected issuer. ID-token validation
happens after the disclosure boundary and cannot close the mix-up path. The
current callback test proves parameter tolerance by requiring the standard
issuer field to be ignored, so the recorded green proof establishes the wrong
behavior.

Preserve tolerance for unrelated provider parameters, but make `iss` a typed
callback field. Extend the existing provider metadata projection with
`authorization_response_iss_parameter_supported`. Because Wyrd's approved
topology uses one common callback for multiple issuers, connection testing and
activation must refuse a provider that cannot supply issuer identification.
The existing `AuthorizationCodeExchange` owner must require `iss`, compare it
by exact string equality with `LoginState.issuer` after resolving state and
before token-endpoint IO, and audit/refuse missing or mismatched values without
calling a token endpoint. Reuse the existing state and exchange owners; do not
add a second callback, verifier, or per-provider redirect.

### FIND-TASK-003-2 — Fixed-cost browser API-key refusal

`BrowserSessions::exchange_api_key` cheaply parses and rejects malformed or
route/key-tenant-mismatched input before the shared verifier. The normal
`ExchangeApiKey` path already owns parsing, tenant comparison, lookup, and one
real or dummy password verification, while the public token route already
shows how to pay one dummy verification when no tenant connection can be
opened. Equal HTTP status alone does not remove the timing distinction.

Resolve the route tenant first. When it exists, open that tenant's
`TenantConn` and pass the presented key directly through the existing
`ExchangeApiKey::execute` path. When no tenant connection can be opened, reuse
the existing `verify_presented(..., None)` path exactly once before returning
the same refusal. Only successful exchange may create a browser session. Do
not add a parser, verifier, timing delay, or second verification.

### FIND-TASK-003-3 — Server-verified tenant chooser

`ServerSessions.metadata` turns every syntactically valid session-cookie suffix
into a rendered tenant choice. `switch` later revalidates the target, so the
bug does not grant authority, but the original task separately prohibits a
tenant selector based on untrusted browser data. Forged and stale cookie names
therefore become misleading UI state before being refused.

Keep cookie names only as lookup hints. Reuse `ServerSessions.read` to resolve
each distinct hinted tenant before it enters `SessionMetadata`, render only the
server-returned tenant key/name, and clear or omit unknown, expired, duplicate,
or mismatched hints. Preserve the existing server-backed switch check. Do not
add a UI membership store or a new endpoint.

### FIND-TASK-003-4 — Canonical browser-session key rotation

Session creation persists sealed access, refresh or bootstrap API-key, and
CSRF values, but `SealedSecretRewrap` inventories only the earlier provider
stores. Its `remaining == 0` and keyless-boot decisions can therefore ignore
live eight- or twelve-hour browser-session ciphertext. Renewal does not repair
the CSRF or API-key fields. Removing K1 after the documented successful pass
ends live sessions and may prevent clean refresh-family revocation. The
candidate has no K1-to-K2 browser-session proof.

Extend the existing `SealedSecretRewrap` and its existing operator SQL/CAS
mechanism to inventory and rewrap every non-null sealed column in live
`auth_browser_sessions` rows. Fence each swap with the exact old bytes. A
concurrent renewal or logout that wins must leave `remaining` nonzero for the
next canonical pass. Include these rows in the same report and keyless-boot
decision; do not create a session-specific rotation engine. Update the existing
self-hosted authentication and SSO guidance plus rewrap log contract so zero
truthfully covers provider and browser-session stores, retaining only the
existing short-lived login-completion exception.

### FIND-TASK-003-5 — TLS at the shared upstream boundary

`serverUrl()` accepts arbitrary `WYRD_SERVER_URL` values, including
`http://wyrd.internal`, and every `ServerSessions` call sends the deployment
key and session secrets to that origin. The task fixes a TLS private channel;
the service key authenticates the caller but supplies neither confidentiality
nor server authentication. Loopback HTTP is needed only for the existing local
and test topology.

Validate the existing URL once at the shared upstream boundary with the native
URL parser. Require `https:` for every non-loopback host and allow `http:` only
for `localhost` and literal loopback addresses. Keep every server-session call
on the same origin and HTTP client. Add no bypass flag, second BFF URL, or
alternate transport.

### FIND-TASK-003-6 — Complete real browser journeys

The current host activates one Keycloak tenant and one OIDC-off tenant. The
browser test exercises only the missing-target-session switch branch and only
stage/deactivate settings actions. It does not prove two active provider
tenants, successful switching, same-issuer refusal, test/activate/remove,
replacement login, recovery preservation, or identity non-inheritance. The
lane already starts Keycloak, Dex, two BFF replicas, Wyrd, and Postgres, so a
new harness is unnecessary.

Extend the existing `identity_ui_e2e` host and real HTTP Vitest journey. Reuse
Keycloak and Dex and the same two BFF replicas. Establish independent sessions
for two active provider tenants in one browser, exercise successful switching,
and prove wrong-tenant and same-issuer cross-tenant refusal. Through the public
settings actions, test and activate a replacement, preserve an authorized
recovery route, log in through the replacement without inheriting the old
principal's authority or email identity, and remove or retire the old
connection. Retain the existing OIDC-off, stage, deactivate, denial, replica,
flow, CSRF, logout, and leak assertions.

### FIND-TASK-003-7 — Mandatory Rust documentation

The new `BrowserSessions` `Debug::fmt` and BFF response `From::from` methods
lack rustdoc. In `config.rs`, `env_opt`'s description was attached to
`parse_bff_service_key_hashes`, so the parser is misdescribed and `env_opt` is
undocumented. This violates the repository's hard documentation rule even
though compilation and lint evidence is green.

Add concise workflow-specific rustdoc to the two trait methods. Move the
environment-reading contract back to `env_opt`, leaving only hash parsing and
rotation-overlap behavior on `parse_bff_service_key_hashes`. Add no wrappers or
documentation abstraction.

### FIND-TASK-003-8 — Authoritative tenant id projection

`BrowserSessions::read` already has the authoritative `DataTenantId`, but
`BrowserSessionView`, the internal `ReadResponse`, and the TypeScript session
shape drop it. `ServerSessions.context` then constructs a valid-looking
`TenantContext` with `tenantId: ''` on every authenticated production request.
Current guards prevent a demonstrated authorization or storage consequence,
but the required private `SessionRead` contract includes `tenant_id`, and the
typed placeholder makes invalid state representable.

Carry the existing `DataTenantId` through `BrowserSessionView`, the existing
internal read response, TypeScript `Read`, and `ServerSession`, then populate
`TenantContext.tenant.tenantId` from it. Keep the identifier in server-only
session state and out of `SessionMetadata` and page data. Do not add a second
tenant lookup.

### FIND-TASK-003-9 — Delete the unused lifetime branch

Both browser-session constructors use `SessionLifetime::For`; no repository
caller constructs `Until`. The unused variant alone creates a second expiry
authority, nullable bind combinations, `COALESCE`, a public re-export, and
misleading comments.

Delete `SessionLifetime` and its re-export. Make `BrowserSessionWrite` carry the
one real fixed `Duration`, and compute absolute expiry only from PostgreSQL's
clock plus that duration. Preserve the separate provider-issued refresh-token
expiry. Add no replacement abstraction.

## Constraints and preserved behavior

- Preserve approved spec revision 5 and the original TASK-003 public behavior.
- Keep durable identity, credentials, permissions, role mapping, tenant
  binding, sealing, and session lifecycle server-owned.
- Keep the common callback, one active connection per tenant, exact public
  origin, server-owned one-use state, and existing provider/network screening.
- Keep browser tokens, API keys, refresh tokens, provider secrets, and the BFF
  service key out of page data, URLs, JavaScript storage, logs, traces, errors,
  audit payloads, and generated artifacts.
- Preserve Secure, HttpOnly, SameSite=Lax, host-only bounded cookies, exact
  origin and CSRF checks, replica-safe Postgres authority, row-lock refresh,
  tenant RLS, audit coupling, and old-connection renewal cutoff.
- Keep API-key invalid responses indistinguishable and perform exactly one
  expensive verification for every presented key class.
- Keep OIDC-off self-hosted entry, machine authentication, current access-token
  lifetime, development-only local sessions, and independently authenticated
  SDK/CLI paths working.
- Reuse existing owners and the existing identity journey lane. Add no new
  dependency, Cargo feature, test harness, callback route, credential path,
  role mapper, local password authority, UI membership store, session-specific
  rewrap engine, TLS bypass flag, or compatibility alias.
- Do not weaken, ignore, or delete a gate or existing assertion to obtain green
  evidence.

## Explicit non-goals

- No SAML, SCIM, LDAP, social login, hosted signup, or commercial onboarding.
- No change to the five-minute access-token contract or provider replacement
  identity semantics.
- No general UI, design-system, auth architecture, database, or test-harness
  refactor beyond the nine corrections.
- No instantaneous revocation claim for already issued access tokens.
- No browser-visible tenant UUID, Wyrd bearer, refresh token, or bootstrap API
  key.

## Acceptance criteria and finding closure

| Criterion | Findings closed | Required observable result |
|---|---|---|
| R1-AC-01 | `FIND-TASK-003-1` | Provider metadata must advertise authorization-response issuer support; matching `iss` completes, while missing/mismatched `iss` consumes/refuses the flow before any token-endpoint call and creates no completion/session. Unrelated response parameters remain tolerated and schemas regenerate. |
| R1-AC-02 | `FIND-TASK-003-2` | Malformed, unknown-route, cross-tenant, unknown-prefix, wrong-secret, expired/revoked, and valid browser-login keys each perform exactly one shared verification; every invalid response is the same `401`, and only the valid route/key pair creates a session. |
| R1-AC-03 | `FIND-TASK-003-3` | Forged, expired, duplicate, and cross-tenant-named cookie hints never render; valid independently server-verified sessions render and switch across either BFF replica. |
| R1-AC-04 | `FIND-TASK-003-4` | OIDC and API-key browser sessions created under K1 survive canonical K2+K1 rewrap and K2-only restart, including CSRF, renewal/action, and mode-specific logout. Concurrent mutation is fenced and a later pass reaches zero. Keyless boot refuses while any session envelope remains. |
| R1-AC-05 | `FIND-TASK-003-5` | HTTPS and loopback HTTP origins are accepted; non-loopback HTTP is refused before the fetcher observes a request. The real production-shaped channel has TLS proof. |
| R1-AC-06 | `FIND-TASK-003-6` | The real two-BFF lane proves two active provider tenants, independent sessions, successful switch, wrong/same-issuer refusal, settings test/activate/remove, replacement recovery, replacement login non-inheritance, and all earlier UI journeys. |
| R1-AC-07 | `FIND-TASK-003-7` | Every cited Rust item has accurate substantive rustdoc and the owning documentation/lint gates pass. |
| R1-AC-08 | `FIND-TASK-003-8` | The server-known tenant UUID reaches production `TenantContext` through the private response, remains absent from browser metadata/page data, and no empty sentinel remains. |
| R1-AC-09 | `FIND-TASK-003-9` | No `SessionLifetime` symbol or nullable dual-lifetime bind remains; both session modes retain PostgreSQL-clock absolute expiry and separate refresh expiry. |

## Focused proof

Use Red-Green-Refactor for each executable correction. Preserve the expected
RED failure and final GREEN result in the implementation evidence.

1. Replace the callback regression with exact contract and callback tests for
   issuer retention, unrelated-parameter tolerance, provider-support refusal,
   matching issuer, and missing/mismatched issuer before token IO. Run every
   named Rust test with its exact `mise exec -- cargo nextest run --locked`
   package/target/expression command, then run `mise run codegen:check`.
2. Extend the existing
   `exchange_api_key::tests::every_invalid_api_key_costs_exactly_one_verification`
   coverage to the browser-session entry and run:

   ```bash
   mise exec -- cargo nextest run --locked -p wyrd-auth --lib \
     -E 'test(=exchange_api_key::tests::every_invalid_api_key_costs_exactly_one_verification)'
   ```

3. Add focused `ServerSessions` tests for verified chooser options, invalid
   hint removal, tenant-id projection, and upstream URL posture. Run their exact
   Vitest selectors through:

   ```bash
   mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run \
     src/lib/server/auth/session.test.ts -t '<exact test name>'
   mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run \
     src/lib/server/upstream.test.ts -t '<exact test name>'
   ```

4. Extend the existing sealing-rotation integration proof to cover live OIDC
   and API-key browser sessions, CAS races, the final zero result, K2-only use,
   and keyless refusal. Run the exact named Rust selector through the
   repository-managed identity setup and then the full identity lane; do not
   substitute an in-memory or mocked store.
5. Add the exact Vitest scenarios `production multi-provider tenant switch`
   and `production provider replacement settings`, and run:

   ```bash
   mise exec -- env WYRD_IDENTITY_TARGET=ui \
     WYRD_IDENTITY_FILTER='production multi-provider tenant switch' \
     mise run test:identity:journey
   mise exec -- env WYRD_IDENTITY_TARGET=ui \
     WYRD_IDENTITY_FILTER='production provider replacement settings' \
     mise run test:identity:journey
   ```

6. Retain and rerun the original exact TASK-003 UI selectors and both original
   filtered journeys so remediation cannot trade away CSRF, expiry, logout,
   SSO replica crossing, or OIDC-off behavior.

## Broader verification

After focused proof is green, run the narrow complete set for the touched
surfaces:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise run test:identity:journey
mise run test:wyrd
mise run test:sql
mise run codegen:check
mise run check:tenant-isolation
mise run docs:check
mise run fmt
mise run lints
git diff --check
```

If a named test is implemented under a different existing target, record and
run the exact repository-pinned selector for that target rather than using a
positional filter. A red gate blocks completion; diagnose and correct the
failure without weakening the gate.

Route this task directly to `$wyrd-implement`. A later `$wyrd-task-review`
must reassess the complete original base-to-remediated-candidate range against
TASK-003 and approved spec revision 5.
