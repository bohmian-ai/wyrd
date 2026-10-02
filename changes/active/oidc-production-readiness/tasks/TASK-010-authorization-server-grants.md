---
id: TASK-010
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-005, REQ-006, REQ-009, REQ-011, REQ-012, REQ-013, REQ-016, REQ-017, REQ-021, INV-001, INV-005, INV-007, AC-004, AC-005, AC-007, AC-009]
depends_on: [TASK-009]
---

# Wyrd authorization server: standard grants and RFC wire format

## Outcome and Value

Off-the-shelf OAuth clients complete every Wyrd login and renewal flow
unchanged: the BFF signs people in with the authorization code grant, the CLI
with the device grant, and both refresh and revoke through standard
endpoints. The invented protocols (private BFF channel, server-side
browser-session rows, sealed login completion) are deleted, and tokens are
minted only when a code or device code is redeemed. This is report item T2 in
[`research/auth-standards-recommendation.md`](../research/auth-standards-recommendation.md)
and absorbs the deleted TASK-008 (wire format), adding the
`authorization_code` grant and RFC 8414 metadata.

Sequenced after TASK-009 because both touch `callback.rs`.

## Owners, Scope, Consumers, and Prohibited Changes

Owners: `wyrd-server` auth routes (`/auth/authorize`, `/auth/token`,
`/auth/platform/token`, `/auth/revoke`, `/auth/device_authorization`, RFC 8414
metadata), `wyrd-auth` (`callback.rs`, `refresh.rs`, `cli_logins.rs`,
`cli_login.rs`), `wyrd-sql` auth queries and migrations, `wyrd-spec` wire
types, and the sealing rewrap. Consumers: the BFF (TASK-011), `wyrd-client`
and the CLI (TASK-012), journeys, generated schemas, and docs.

No new library: use `axum`, `jsonwebtoken`, and `sqlx`, which are already in
use. No vetted Rust authorization-server library exists (report §1). Implement
exactly these sections and nothing more:

| Surface | Sections |
|---|---|
| `GET /auth/authorize` (replaces `/auth/login` and the handoff) | RFC 6749 §3.1, §3.1.2 (exact `redirect_uri`), §4.1.1–4.1.2 (code, single use), §4.1.2.1 errors; RFC 7636 §4.3, §4.6 (S256 only); RFC 9700 §2.1. Tenant is the one extension parameter `tenant=<key>` (RFC 6749 §3.1), pre-login routing context only (INV-001). |
| `authorization_code` at `/auth/token` | RFC 6749 §4.1.3–4.1.4, §5.1, §5.2; RFC 7636 §4.5–4.6. The code is stored hashed, expires within 60 seconds, is single-use, and is bound to `client_id`, exact `redirect_uri`, PKCE S256 challenge, tenant, and principal. |
| Client authentication | RFC 6749 §2.3.1 `client_secret_basic` for the confidential client `wyrd-ui`; the deployment configures its secret where `WYRD_BFF_SERVICE_KEY_SHA256` is today. `wyrd-cli` is a public client. |
| Device authorization (trim) | RFC 8628 §3.1–3.5, §5.1 (user-code entropy and rate limiting). Mint at redemption: the device row stores only "approved by principal P, tenant T, connection C". |
| `refresh_token` grant | RFC 6749 §6; RFC 9700 §4.14.2 rotation and family revocation on reuse for the public client only. Confidential `wyrd-ui` refresh tokens do not rotate and have a bounded absolute lifetime (REQ-012). |
| `/auth/revoke` | RFC 7009 §2.1–2.2 (200 for unknown tokens). |
| RFC 8693 delegation, RFC 7523 jwt-bearer, API-key exchange | Keep semantics; wire format only: RFC 8693 §2.1–2.3; RFC 7523 §2.1, §3; API-key exchange as an RFC 8693 `subject_token_type` per the existing contract. |
| Request and error wire | Form bodies (RFC 6749 §4.1.3, §6; RFC 7009 §2.1; RFC 8628 §3.1); RFC 6749 §5.1 success with `Cache-Control: no-store`; RFC 6749 §5.2 errors, 401 with `WWW-Authenticate` for `invalid_client`. These endpoints are the one exception to `WyrdError` problem+json; server logs and audit keep Wyrd error codes. |
| Authorization server metadata | RFC 8414 §2–3: one static JSON document. |

Delete: `bff.rs` (`/internal/bff/v1/{login/options,sessions/*}` and
`x-wyrd-bff-key`); `browser_sessions.rs`, `queries/auth/browser_sessions.rs`,
and migrations `20261001000001` and `20261001000003` (unreleased; deleting
them is sanctioned by lead direction FIND-TASK-004-11);
`auth_login_state.completion_sealed`, the server side of `/login/complete`,
and the device sealed-completion path; the sealed rewrap of session,
completion, and bootstrap-key columns (the keyring stays only for provider
and workload-issuer client secrets, REQ-005); JSON request bodies on these
endpoints.

Prohibited: changing tenancy, authorization, token contents, role mapping, or
audit ownership; a JSON alternative or compatibility route; a stored issued
credential awaiting pickup; any grant, parameter, or document the listed
sections do not define.

## Approach

1. Add `/auth/authorize` and the `authorization_code` grant with the bound,
   hashed, 60-second, single-use code; register `wyrd-ui` (confidential) and
   `wyrd-cli` (public).
2. Trim the device grant so the provider callback records only the approval
   and the token endpoint mints at redemption.
3. Make refresh rotate for the public client only; keep family revocation.
4. Move every listed endpoint to form bodies and RFC 6749 §5 success/error
   bodies; publish RFC 8414 metadata.
5. Delete the BFF channel, browser sessions, sealed completion, and the
   narrowed rewrap columns.

## Ordered Implementation Scenarios

### Scenario 1 — Authorization code grant with PKCE

**Behavior.** `openid-client` 6.8.8, used as an off-the-shelf client against a
real server, completes code + PKCE login as `wyrd-ui`. A wrong, expired, or
replayed code, a PKCE mismatch, a wrong `redirect_uri`, and a wrong client
secret (`invalid_client`, 401) yield no token (REQ-009, REQ-021, AC-007).

**RED.** A journey driving `openid-client` against `/auth/authorize` and
`/auth/token` fails because neither the endpoint nor the grant exists.

**GREEN.** Implement the authorize endpoint and code grant to the listed
sections until the journey and its negatives pass.

**REFACTOR.** Reuse the existing login state, issuer, and audit owners; no
second code store.

### Scenario 2 — Device grant minted at redemption

**Behavior.** The `oauth2` crate's device flow completes a CLI login; the
device row records only the approval; `authorization_pending`, `slow_down`,
`access_denied`, and `expired_token` are returned per RFC 8628 §3.5; a
denied, expired, deleted, or already-redeemed device grant produces no access
token, refresh token, session, or surviving refresh row, and a live approved
grant issues exactly once at token redemption (REQ-011, AC-004, AC-007,
FIND-TASK-004-12).

**RED.** A journey that pauses after approval lookup, denies or expires the
grant from another actor, then resumes provider completion, observes a
refresh row created by the callback; the `oauth2` device journey also fails
on JSON-shaped requests.

**GREEN.** The callback writes only the approval; the token endpoint locks
and revalidates the live approved row and issues once in one tenant
transaction with canonical audit. Rerun Scenario 1.

**REFACTOR.** Delete the sealed-completion path and any device-only state it
leaves behind.

### Scenario 3 — Refresh rotation by client type

**Behavior.** CLI refresh rotates and reuse revokes the family; `wyrd-ui`
refresh does not rotate and stops at its absolute lifetime; an
old-connection token cannot refresh after replacement (REQ-012, REQ-016).

**RED.** A journey refreshing twice as `wyrd-ui` with the same token fails
because the first refresh rotated it.

**GREEN.** Branch rotation on the authenticated client type only. Rerun
Scenarios 1–2.

**REFACTOR.** Keep one refresh path with one client-type decision.

### Scenario 4 — RFC wire format, revocation, and metadata

**Behavior.** Every listed endpoint accepts form bodies and refuses a JSON
body with `invalid_request`; successes and errors use RFC 6749 §5.1/§5.2 with
`Cache-Control: no-store`; `/auth/revoke` answers 200 for unknown tokens; the
RFC 8414 document lists the endpoints; RFC 8693, RFC 7523, API-key, and
workload journeys keep their semantics (REQ-013, REQ-021, AC-005, AC-009).

**RED.** Off-the-shelf client calls and a JSON-body negative fail against the
current endpoints.

**GREEN.** Convert request parsing and responses; add the metadata document.
Rerun Scenarios 1–3.

**REFACTOR.** One RFC error mapping shared by these endpoints.

### Scenario 5 — Deleted protocols stay deleted

**Behavior.** `/internal/bff/v1/*`, `/login/complete`, browser-session tables,
and `completion_sealed` no longer exist; a clean database and the migration
set reach the schema current queries require (REQ-005, decision 7).

**RED.** The existing private-channel and browser-session tests are the code
being removed; deleting them first leaves compile failures where callers
remain.

**GREEN.** Remove the callers and the narrowed rewrap columns until build,
clean-migration, and journey lanes pass.

**REFACTOR.** Remove dead error variants, config, and docs strings tied to the
deleted surfaces.

## Acceptance Criteria

- Off-the-shelf clients against a real server: the `oauth2` crate completes
  device login and refresh; `openid-client` completes code + PKCE.
- Negatives: wrong, expired, or replayed code; PKCE mismatch; wrong
  `redirect_uri`; wrong client secret → `invalid_client` 401; device
  `authorization_pending`, `slow_down`, `access_denied`, `expired_token`; CLI
  refresh reuse → family revoked; BFF refresh not rotated; JSON body →
  `invalid_request`.
- FIND-TASK-004-12: denied, expired, deleted, or already-redeemed device
  grants leave no access token, refresh token, session, or refresh row; a
  live approved grant issues exactly once at redemption.
- RFC 8414 metadata is served; all listed endpoints use form bodies and RFC
  6749 §5 bodies; the existing API-key, RFC 8693, RFC 7523, and workload
  journeys pass.
- `bff.rs`, browser-session code and migrations, sealed completion, and the
  session/completion/bootstrap rewrap columns are gone.

## Expected Write Set and Consumer Closure

`wyrd-server` auth routes and config, `wyrd-auth`, `wyrd-sql` queries and
migrations, `wyrd-spec` wire types, `wyrd-crypt` rewrap, generated schemas
from their source, and identity journeys. The BFF and shared-client consumers
are rewritten in TASK-011 and TASK-012; do not keep a compatibility path for
them here.

## Verification and Evidence

Name the off-the-shelf clients and their exact journey commands. Run every new
or changed named test with its exact `mise exec --` selector and setup,
through `mise exec -- env WYRD_IDENTITY_TARGET=<target>
WYRD_IDENTITY_FILTER=<test> mise run test:identity:journey` for identity
journeys.

Then run: `mise run test:identity:journey` unfiltered (targets `server`, `ui`,
`cli`, `rust`, `client`, `python`, `typescript`), `mise run test:shared`,
`mise run test:wyrd-sdk`, `mise run test:cli:journey`,
`mise run test:principals:integration`, `mise run py:test:integration`,
`mise run ts:test:integration`, `mise run test:wyrd`,
`mise run codegen:check`, `mise run docs:check`, `mise run fmt`,
`mise run lints`, `mise run py:format`, `mise run py:lints`,
`mise run py:test:unit`, `mise run py:typecheck`, `mise run ts:test:unit`,
`mise run ts:typecheck`, `mise run ts:napi:check`, the boundary checks
`mise run check:client-tier`, `mise run check:pyo3-scope`,
`mise run check:unwrap-audit`, and `mise run check:workspace-hack`, and
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `test`.

The BFF and shared-client consumers of the deleted surfaces are replaced by
TASK-011 and TASK-012. Lanes whose only failures are those consumers are
proven at the integrated TASK-010 + TASK-011 + TASK-012 candidate; every other
lane passes on this task alone. Record which lanes fall in each group.

## Material Stop Conditions

Stop and report if a needed behavior has no vetted library and is not covered
by a named RFC section; never write custom protocol logic. Also stop if a
listed section would change tenancy, authorization, or token contents.

## Authority Links

[Approved spec](../spec.md) REQ-021 and decision 7;
[research report](../research/auth-standards-recommendation.md) §3.2, §3.3, T2;
[lead direction FIND-TASK-004-11](../review/TASK-004-r2/lead-direction-FIND-TASK-004-11.md);
[routing](../review/TASK-004-r2/lead-direction-routing.md);
[AGENTS.md](../../../../AGENTS.md);
[RFC 6749](https://www.rfc-editor.org/rfc/rfc6749);
[RFC 7636](https://www.rfc-editor.org/rfc/rfc7636);
[RFC 7009](https://www.rfc-editor.org/rfc/rfc7009);
[RFC 8628](https://www.rfc-editor.org/rfc/rfc8628);
[RFC 8693](https://www.rfc-editor.org/rfc/rfc8693);
[RFC 7523](https://www.rfc-editor.org/rfc/rfc7523);
[RFC 8414](https://www.rfc-editor.org/rfc/rfc8414);
[RFC 9700](https://www.rfc-editor.org/rfc/rfc9700).
