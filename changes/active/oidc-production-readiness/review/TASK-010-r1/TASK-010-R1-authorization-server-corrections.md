---
id: TASK-010-R1
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-005, REQ-006, REQ-009, REQ-011, REQ-012, REQ-013, REQ-016, REQ-017, REQ-021, INV-001, INV-005, INV-007, AC-004, AC-005, AC-007, AC-009]
depends_on: []
parent_task: TASK-010
remediates: [FIND-TASK-010-1, FIND-TASK-010-2, FIND-TASK-010-3, FIND-TASK-010-4, FIND-TASK-010-5, FIND-TASK-010-6, FIND-TASK-010-7, FIND-TASK-010-8, FIND-TASK-010-9, FIND-TASK-010-10, FIND-TASK-010-11, FIND-TASK-010-12, FIND-TASK-010-13]
---

# Close TASK-010 authorization-server gaps

## Authority and immutable review subject

- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Review base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Reviewed candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Validated ledger:
  `changes/active/oidc-production-readiness/review/TASK-010-r1/findings-validation.md`

Implement this task through `$wyrd-implement`. Reassess the complete original
TASK-010 range in the next review; do not review only the remediation diff.

## Outcome

Finish the approved authorization-server surface without adding another
protocol, compatibility path, security mechanism, or durable authority. A
standard client can complete and diagnose code, device, refresh, revocation,
token-exchange, and provider-denial flows; callback state changes remain fenced
and audited; refresh replay affects only the compromised rotation chain; the
served contract describes the wire; device brute-force admission works at the
existing gateway boundary; and the task's named focused proofs exist.

## Issue diagnoses and required corrections

### FIND-TASK-010-1 — the required `openid-client` interoperability proof is absent

TASK-010 explicitly names `openid-client` 6.8.8 against a real server. The
current identity journey under `crates/wyrd/wyrd-server/tests/identity_e2e.rs`
constructs discovery, authorize, PKCE, Basic authentication, token forms, and
replay requests itself. That proves server-to-server agreement with its own
test code, not interoperability with the named off-the-shelf client.

Add one focused real-server `openid-client` 6.8.8 driver within the existing
identity-test lifecycle. It uses RFC 8414 discovery and the public authorize and
token endpoints as `wyrd-ui`, completes S256 code redemption, and exercises the
task-named wrong, expired, replayed, PKCE-mismatched, redirect-mismatched, and
wrong-secret outcomes. Reuse the current server/provider fixture and client
registration. Do not build BFF session behavior, restore `/internal/bff/v1`, or
create a second OAuth implementation.

### FIND-TASK-010-2 — the routed device-terminal race lacks its focused proof

The source now has the correct authority boundary: callback completion updates
only a still-live device row, and token redemption locks, revalidates, deletes,
issues, audits, and commits in one transaction. Existing tests cover sequential
denial/expiry and ordinary one-use redemption, but none pauses after
`CliLogins::approve` reads the row, terminates the grant from another actor,
then resumes provider completion. That exact interleaving is the accepted
closure proof for routed `FIND-TASK-004-12`.

Use the existing Postgres lock/waiter pattern at the current real-server/device
test owner. Deterministically park approval after its live-row lookup and before
the bound login-state insert. In separate cases, deny the grant and expire then
poll-delete it from another actor. Resume provider completion and prove no code,
device approval, access token, refresh token, session, or refresh row survives.
Retain the live exactly-once redemption proof. Add no sleep, production test
hook, lease, marker, cleanup process, or new concurrency harness.

### FIND-TASK-010-3 — unsupported exchange audiences lose RFC 8693 classification

`TokenRequest::TokenExchange` deserializes `audience` directly into the closed
`TokenAudience`; `OAuthForm::decode` maps any serde error to `invalid_request`,
and `OAuthErrorCode` cannot represent `invalid_target`. A syntactically valid
unsupported target therefore receives the wrong registered response.

Add the RFC 8693 `invalid_target` error to the existing OAuth error contract
and preserve enough request context to classify a present token-exchange
audience against the existing Wyrd/Bifrost set before generic request-decoding
failure. Keep malformed requests and unusable subject/actor tokens as
`invalid_request`. Preserve both supported audiences, token contents, the
approved API-key subject-token type, and deletion of `grant_type=wyrd_api_key`.
Do not add an audience registry, setting, or alternate error mapper.

### FIND-TASK-010-4 — safe duplicate authorize errors stay on a local page

`OAuthForm::parse` rejects every repeated parameter before `authorize` can
retain a valid client and exact redirect. A duplicate non-binding parameter
such as `state` therefore produces local HTML instead of the RFC 6749
authorization error redirect, although the destination was safe.

At the existing authorize/form owners, establish that `client_id` and
`redirect_uri` are each unique and exactly registered before validating the
rest of the request. After that binding is established, route duplicate or
invalid non-binding fields through the existing `ClientAuthorization` /
`client_redirect` path as `invalid_request`; echo `state` only when one
unambiguous value exists. Missing, duplicated, unknown, or mismatched client or
redirect remains a local refusal with no `Location`. Add no permissive redirect
fallback or compatibility parser.

### FIND-TASK-010-5 — upstream provider denial bypasses downstream completion

`CallbackQuery` requires `code` and has no standard `error` variant. Axum
rejects `error=access_denied&state=...` before
`AuthorizationCodeExchange::complete` can consume server-bound state and
recover the registered downstream redirect. The authorization attempt remains
unresolved and the normal one-time-state owner is skipped.

Extend the existing callback wire and `AuthorizationCodeExchange` owner to
accept exactly one success code or one standard provider error with required
state. The error path consumes the same login state, applies the current
optional-issuer binding, and returns through `client_redirect`. Map provider
denial to downstream `access_denied`; use the existing mappings for server or
unavailable cases. Never reflect provider descriptions, accept query tenant
authority, mint a code/token, or create a second callback/state path.

### FIND-TASK-010-6 — `client_secret_basic` scheme matching is case-sensitive

`OAuthClients::identify` uses `strip_prefix("Basic ")`, although HTTP
authentication scheme tokens are case-insensitive. Valid `basic` or `BASIC`
credentials are refused before the existing decoding and secret verification.

Split the existing Authorization value once, compare only its scheme with
`eq_ignore_ascii_case("basic")`, and pass the unchanged credentials into the
current Base64/form-component decoder and constant digest comparison. Preserve
conflicting form-client handling, invalid-client status/challenge, and the one
supported confidential authentication method. Do not broaden whitespace rules
or add another method.

### FIND-TASK-010-7 — four new OAuth items violate mandatory rustdoc

The new public `OAuthError` tuple field, `FromRequest::Rejection`, fallible
`OAuthForm::from_request`, and fallible token test helper `parse` omit the
field/associated-type documentation or required `# Errors` sections mandated
by `AGENTS.md` and `architecture/agent-rules.md`.

Document only their existing behavior: the tuple field's registered OAuth code,
the rejection type, the extractor's content-type/body/form failures, and the
test helper's serde failure. Add no wrapper, lint suppression, allowlist, or
new repository check.

### FIND-TASK-010-8 — `active_refresh` duplicates tenant selection under RLS

The new `ACTIVE_REFRESH_SQL` manually filters `data_tenant_id` and
`active_refresh` obtains/binds the same tenant from `TenantConn`. Forced RLS is
already the load-bearing tenant authority, so the duplicate predicate adds a
second expression that can drift without adding isolation.

Delete the manual tenant predicate, tenant extraction, second bind, and stale
claim that justifies them. Keep the token hash, revocation, database-clock
expiry checks, and existing `TenantConn` transaction. Add no replacement
helper, allowlist, or tenant-isolation check.

### FIND-TASK-010-9 — served OpenAPI omits OAuth client identification

Runtime client identification occurs before decoding the grant-specific
request structs. Those structs intentionally exclude public `client_id` and
confidential HTTP Basic authentication, yet the utoipa operations publish them
as complete request bodies and clear security. Generated clients cannot learn
how to call token, device-authorization, or revocation endpoints.

At the existing utoipa/OpenAPI owner, describe the complete form with optional
`client_id`, register ordinary RFC 7617 HTTP Basic for the confidential OAuth
client, and declare each route's accepted public-form or confidential-Basic
alternatives. Keep runtime extraction centralized in `OAuthClients`, preserve
the existing Wyrd access-token scheme on protected APIs, and publish no JSON or
Wyrd-specific client-auth mechanism. Narrowly update the served-document test
that currently rejects every second scheme.

### FIND-TASK-010-10 — the shared server governor is proxy-collapsed and replica-local

`auth_router` places every auth route behind one default `tower_governor`
instance. Its peer-IP key is the gateway in supported proxy deployments, while
each Wyrd replica owns a different in-memory bucket. One caller can throttle
unrelated tenants and routes on a replica, and replica selection bypasses the
intended device user-code control. The identity helper retries every 429, hiding
both effects. RFC 8628 requires limiting attempts to verify the short user
code, not a bespoke budget for metadata, callbacks, token, refresh, revoke, and
key issuance.

Remove the server-local all-auth governor and retry masking. Use the existing
gateway rate-limit integration boundary and its native limiter only for
device user-code verification attempts, keyed by the real client address the
trusted gateway observes. Preserve high-entropy, short-lived codes and the
token endpoint's polling cadence / `slow_down` behavior. Do not add a database
limiter, trusted-header parser inside Wyrd, distributed cache, public option,
or fleet-coordination layer.

### FIND-TASK-010-11 — inactive refresh rows revoke unrelated sessions

After the existing principal-family lock, every failure to consume a public
refresh row—rotation, expiry, logout, administration, or earlier
containment—is classified as reuse. `revoke_refresh_family` then revokes every
active row for the principal, including independent CLI chains and non-rotating
UI sessions. The repository already has the RFC-shaped
`revoke_refresh_chain`, which follows `rotated_from` and preserves unrelated
logins.

Under the current lock, classify the stored row explicitly. Only a predecessor
whose `revoked_reason` is `rotated` enters replay containment and canonical
theft audit. Reuse `revoke_refresh_chain(stored.id, "reuse_detected")` so its
active descendants are retired. Expired, logout-revoked, administratively
revoked, or already-contained rows return the ordinary indistinguishable
inactive-token refusal without collateral write or theft event. Preserve
unknown/client mismatch behavior, confidential-client non-rotation, and lock
order. Add no grace knob, family table/id, option, or second state machine.

### FIND-TASK-010-12 — callback completion is not fenced by connection lifecycle

Callback verification reads the active connection in a separate committed
transaction. The final tenant transaction can then ensure a User, replace
roles, and commit a code or device approval without joining the connection
slot lock used by activation/deactivation/removal. The current multi-replica
cutoff test demonstrates that a stale callback returns a code after
deactivation; later redemption refusal does not undo the committed User/role
effects.

In the existing final callback transaction, after the User family lock, acquire
the existing human-connection slot lock and call the existing exact
`human_connection_is_active(connection_id, revision)` predicate before roles,
code, or approval can commit. On mismatch, return the current inactive/invalid
login refusal and roll back the transaction. Preserve pre-provider state
consumption, provider validation, family-before-slot lock order,
connection-test behavior, and the redemption recheck as defense in depth. Add
no lock or lifecycle state.

### FIND-TASK-010-13 — successful unchanged-role callbacks lack login audit

The final callback transaction appends `auth.user.roles.sync` only when role
rows change. It can commit a new authorization code or device approval with no
successful login-outcome event. Later token/device redemption audit cannot
cover an abandoned or expired pending grant. Removing callback-time issuance
correctly eliminated premature credentials but also removed its former audit
coverage.

Append one redacted successful-login outcome through the existing
`append_auth_audit` owner for every successful non-test callback, in the same
final tenant transaction as User/roles and code/device approval. Keep the
conditional role-sync event separate and keep redemption events as issuance
evidence. If the login append fails, roll back all final callback writes. Add
no audit sink, table, relay, or best-effort path.

## Constraints and preserved behavior

- Preserve API-key exchange exclusively as RFC 8693 with
  `subject_token_type=urn:wyrd:oauth:token-type:api_key`; do not restore
  `grant_type=wyrd_api_key` or any alias.
- Preserve deletion of `/internal/bff/v1`, browser-session state, sealed login
  completion, and their migrations/rewrap columns. Consumer replacement remains
  TASK-011.
- Preserve exact registered redirect binding, S256-only PKCE, hashed and
  60-second one-use codes, tenant/principal/client/connection binding, and
  form-only OAuth endpoints.
- Preserve mint-at-redemption for device grants, one-use locked redemption,
  RFC 8628 token polling cadence, and no stored issued credential awaiting
  pickup.
- Preserve public-client rotation, confidential-client non-rotation with
  absolute lifetime, old-connection cutoff, and ordinary idempotent RFC 7009
  revocation.
- Preserve server-owned tenant/principal/role/token authority, `TenantConn`
  RLS, canonical transactional audit, one issuer, and existing lock order.
- Preserve RFC 7523, delegation, workload, token contents, permission
  resolution, role mapping, and platform/tenant-plane separation.
- Placement, naming, structure, and wording alone are non-goals. The four
  rustdoc omissions remain required only because the repository explicitly
  makes them a blocking completeness gate.

## Explicit non-goals

- No new grant, compatibility route, JSON alternative, browser session, stored
  pickup credential, identity store, role mapper, issuer, audit path, or tenant
  selector.
- No BFF/UI implementation, cookie session, or repair of a consumer whose only
  failure is deletion of `/internal/bff/v1`.
- No authorization-server library, audience registry, rate-limit service,
  distributed cache, refresh-family table/id, cleanup worker, lease, retry
  journal, production test hook, or new repository check.
- No full unfiltered identity or every-language journey in this remediation;
  those run once at change review.
- No architecture, product, public API, compatibility, security,
  concurrency-semantics, or persistent-data decision beyond approved revision
  11 and the existing owners named above.

## Acceptance criteria

| Finding | Acceptance criterion |
| --- | --- |
| `FIND-TASK-010-1` | `openid-client` 6.8.8 completes discovery and code + S256 against the real server and observes every named refusal without using deleted private routes. |
| `FIND-TASK-010-2` | In-flight approval loses to deny and expire/delete, commits no approval or credential authority, while one live approval still issues exactly once. |
| `FIND-TASK-010-3` | Unsupported valid exchange audience returns `invalid_target`; malformed exchange remains `invalid_request`; Wyrd/Bifrost and API-key exchange remain unchanged. |
| `FIND-TASK-010-4` | Duplicate non-binding authorize parameters redirect only to an already validated registered URI; ambiguous/mismatched client or redirect stays local. |
| `FIND-TASK-010-5` | Provider denial consumes state once and returns downstream `access_denied` with exact client state and no Wyrd authority. |
| `FIND-TASK-010-6` | Valid Basic scheme case variants authenticate identically; malformed schemes and wrong secrets remain `401 invalid_client`. |
| `FIND-TASK-010-7` | All four cited items have substantive required rustdoc and `# Errors`, with no suppression or new check. |
| `FIND-TASK-010-8` | `active_refresh` relies on `TenantConn` RLS only and still refuses revoked, expired, or cross-tenant rows. |
| `FIND-TASK-010-9` | Served OpenAPI fully describes public form client identification and confidential Basic alternatives for token, device authorization, and revoke. |
| `FIND-TASK-010-10` | User-code attempts are conventionally limited at the existing gateway across replicas and client addresses; unrelated auth routes share no bucket. |
| `FIND-TASK-010-11` | Rotated-token replay revokes only its current chain; expired/logout/admin rows and unrelated CLI/UI sessions are unaffected and no false theft audit is written. |
| `FIND-TASK-010-12` | Deactivation/replacement winning before final callback commit leaves no User/role/code/approval/login-audit effects; unchanged active connection still succeeds. |
| `FIND-TASK-010-13` | Every successful non-test authorize/device callback appends one login outcome; role changes append their separate event; required audit failure rolls back all final writes. |

## Focused proof and narrow verification

Run every new or changed named test with its exact `mise exec --` selector and
the repository-managed Postgres/server setup it needs. At minimum the focused
proof must include:

- the new exact `openid-client` identity filter;
- exact device pause/deny and pause/expire-delete interleaving tests;
- exact `wyrd-spec` and route tests for `invalid_target`;
- exact authorize duplicate-field and callback provider-denial tests;
- exact OAuth client-auth case tests;
- exact SQL tests for RLS-only `active_refresh` and rotation-chain containment;
- the exact served-OpenAPI contract test;
- the exact multi-replica callback cutoff and callback audit tests; and
- the rendered official-gateway/startup check plus the focused device
  verification route proof.

Then run only the narrow owner lanes covering the remediation write set:

```text
mise run test:principals:integration
mise run test:sql
mise run codegen:check
mise run check:client-tier
mise run check:tenant-isolation
mise run check:unwrap-audit
mise run fmt
mise run lints
```

Run the filtered identity journey commands required by the new named tests and
the narrow official gateway/server startup check selected from `mise.toml`.
Do not run or require the unfiltered identity suite, broad language sweeps, or
`mise run gate`; those belong to integrated change review.
