# TASK-001-008-R7 — Replace rejected tenant auth and close remaining findings

## Route and authority

Implement this packet with `$wyrd-implement`. The next `$wyrd-task-review`
must reassess the complete cumulative candidate, not only this remediation.

- Approved specification: `changes/active/admin-principals/spec.md`, revision
  12, status `approved`, SHA-256
  `1a2fd760de9012767499cf9ca41d41c21d502ffe6e13613ddf5cdd2328bcf856`.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md` through
  `TASK-008-*.md`.
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed candidate: `ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-07/findings-validation.md`.
- Human architecture decision: 2026-09-21. Revision 11 supersedes every
  revision-10 requirement for tenant authorization epochs, next-request token
  revocation, request-time permission resolution, and verified-token caching.

Implementation of the original R7 packet has already begun in the worktree.
Inspect and adapt that work; do not discard unrelated valid edits. Work that
adds admission verdicts, epoch ordering, a Postgres-backed revocation checker,
or checker-owned `WyrdPostgres` implements the rejected design and must not be
completed merely because it is already present.

## Outcome

Use one authority per authentication plane:

- Tenant and Bifrost requests use a standard five-minute self-contained JWT.
  Issuance resolves current identity and permissions once; requests verify the
  JWT locally and authorize from its `permissions` claim.
- Platform requests remain database-backed and revalidate the current
  credential, principal, and grants on every request because that plane is
  privileged and low volume.

Delete the hybrid tenant flow instead of repairing it. Add no cache,
revocation list, authorization epoch, introspection service, listener,
middleware, connection wrapper, or replacement abstraction.

## Required flow

### Tenant token issuance

```text
API-key exchange, OIDC login, human refresh, workload jwt-bearer, or delegation
  -> verify the grant-specific credential, identity, refresh token, or assertion
  -> enter the one shared tenant-token issuance workflow
  -> load the current tenant and principal
  -> reject an inactive tenant, principal, or credential
  -> resolve the principal's current grants to one PermissionSet
  -> mint a five-minute JWT
```

These are the complete tenant access-token issuance paths. Each entry path may
validate only its own grant-specific evidence before calling one concrete
issuance owner; none may independently load authorization, construct claims,
choose a TTL, sign an access token, or record a successful issuance. Reuse one
workflow rather than five similar helpers, a trait, or a factory.

The tenant JWT is the authority snapshot for its lifetime. It carries the
existing principal identity, kind, tenant, Card scope, credential attribution,
delegation data, `iss`, `aud = "wyrd"`, `iat`, `exp`, and `jti`, plus one
`permissions: PermissionSet` claim. `permissions` is the only tenant authority
claim. A retained `roles` claim is informational metadata only; no request may
resolve or authorize from it. Keep the existing
bearer-size limit and normal signed-JWT validation. Add no compression, opaque
fallback, or second token format.

### Tenant request and stream establishment

```text
JWT
  -> verify Ed25519 signature, issuer, audience, and expiry locally
  -> construct Principal/AuthContext directly from verified claims
  -> check the route's Permission against the claimed PermissionSet
  -> audit the decision through the existing owner
  -> execute under the existing tenant boundary
```

Token verification performs no database query. Normal route work may still use
Postgres for its own catalog, audit, or durable operation; those are not token
introspection.

A Bifrost request or stream must present a token valid at admission. Once
admitted, its already-bounded work may finish under the existing Bifrost
deadline even if the token expires; expiry refuses the next request or stream
establishment. Do not add mid-stream reauthentication, an expiry watcher, or a
new cancellation protocol.

Bifrost keeps its existing object authorization. After the catalog resolves a
requested table to its stable table identity, query or ingest builds the exact
required Bifrost permission and checks it against the JWT `PermissionSet`.
`All`, schema, and exact-table scope subsumption remain unchanged, as do RLS,
object-prefix isolation, and the tenant tripwire.

### Revocation and renewal

Revoking a tenant credential, suspending or deleting its principal, suspending
its tenant, or changing its grants prevents issuance of a new token
immediately. An already-issued tenant token remains valid until its five-minute
expiry. `wyrd-client` continues to cache the access token, re-exchange before
expiry, and re-exchange once after an authentication refusal. The public client
flow remains:

```text
API key -> exchange -> JWT -> requests -> exchange again before expiry
```

Do not add refresh tokens for machines. Human refresh keeps its existing
rotation and replay-containment behavior, but it MUST use the shared issuance
workflow so current tenant/principal status and grants govern the successor
access token.

### Platform requests

Keep the existing platform-plane current-state model. Every platform request
revalidates its current credential, principal status, and platform grants from
the platform store through `OperatorPool`, with no authority cache. Do not move
tenant JWT semantics into the platform plane and do not give platform
principals tenant data access.

## Delete the rejected tenant-auth design

Delete the implementation and active architecture for all of the following;
renaming or leaving unused compatibility shells does not satisfy this task:

- `tokens_not_before` columns, row fields, queries, updates, timestamp-ordering
  logic, migrations introduced by this change, tests, and documentation;
- `RevocationCheck`, `SqlRevocationCheck`, `RevocationVerdict`,
  `RevocationVerdictFuture`, `NoRevocation`, `with_revocation`, and the tenant
  revocation-resolver module and assembly;
- tenant `user_admission`, `service_account_admission`, `PrincipalAdmission`,
  and every per-request admission query used only by token verification;
- the positive verified-token cache and its settings and dependency when no
  other owner uses that dependency;
- request-time `PermissionResolver` use by the tenant verifier. Resolve roles
  at issuance through the existing tenant SQL boundary, then carry the
  resulting `PermissionSet` in the JWT. Delete resolver traits or concrete
  wrappers left with no real caller rather than preserving them for shape;
- the partially implemented epoch-aware `issued_at` plumbing and successor
  ordering. Standard `iat` remains, but it has no revocation comparison;
- stale active architecture, foundation, security, operator, and API prose
  that prescribes tenant epochs, next-request revocation, revocation checking,
  verified-token caching, per-request auth admission SQL, or request-time role
  resolution. Historical completed records remain historical and need not be
  rewritten.

The documentation audit explicitly includes `AGENTS.md` and
`architecture/wyrd-doctrine.mdx`. They currently delegate authentication detail
to the design and security authorities and contain no rejected mechanism to
rewrite; keep them unchanged unless the final implementation exposes a concrete
conflict. Do not copy the full auth flow into either file merely to touch it.

After deletion, one concrete tenant access-token verifier owns only local keys,
issuer, audience, and clock-skew policy, and synchronously returns verified
claims. External OIDC verification stays on the issuance side with its existing
database-backed resolvers. Do not introduce a common verifier trait, factory,
or checker. In particular, no verifier or checker stores
`PgPool`, `WyrdPostgres`, `TenantConn`, or `OperatorPool`. Database access at
issuance and on the platform plane must continue to use only `TenantConn` and
`OperatorPool`; introduce no third connection abstraction.

## Remaining validated findings

The architecture decision supersedes the original corrections for
`FIND-admin-principals-R7-1`, `R7-2`, `R6-1`, `R7-3`, and `R7-4`; close
those roots by deleting the epoch/admission/checker design above. Close these
independent findings as originally validated:

### `FIND-admin-principals-13` — local transfer extractor failures

Map missing or undecodable local-download queries and undecodable local-upload
paths through the canonical `WyrdErrorResponse`, and make the served OpenAPI
list the stable codes and `application/problem+json` responses that are
actually reachable. Preserve query-based nested download paths, binary bodies,
authentication, and storage validation. Add no rejection middleware,
hand-written parser, route alias, or second error catalog.

### `FIND-admin-principals-R5-2` — principal-revoke miss audit

Propagate principal lookup failures instead of converting them to not-found.
For an authorized unknown-id or wrong-kind revoke, commit the already-appended
allowed decision before returning the existing stable not-found response and
commit no effect. Real store failures roll back the decision and effects. Add
no second transaction, audit helper, sink, or error.

### `FIND-admin-principals-R7-5` — focused evidence

Confirm every specifically named Rust test with `cargo nextest list`, then run
its exact nonzero selector through `mise exec -- cargo nextest run --locked`
with the repository-managed environment wrapper where required. Record the
exact command, selected count, and result.

## Constraints and preserved behavior

- Preserve the five principal kinds, platform/tenant plane separation,
  credential secrecy, fixed-cost credential refusal, overlap rotation,
  delegation, Card scope, and durable machine re-exchange.
- Preserve the existing `Permission`, `PermissionSet`, `RbacCheck`, and Bifrost
  scope-subsumption model; do not invent scopes, policy engines, role types, or
  route-specific authorization systems.
- Preserve `TenantConn` RLS, `OperatorPool`, Bifrost tenant binding and
  tripwire, and the canonical audit append and publisher.
- Preserve fail-closed signature, issuer, audience, expiry, plane, tenant, and
  permission checks. The accepted five-minute revocation window is not licence
  to weaken any other validation.
- Preserve MCP UUID schemas, local nested artifact paths, relative local URLs,
  binary transfer bodies, shared-client authentication, and runtime OpenAPI.
- Do not add compatibility claims or dual verification for the superseded JWT
  shape. This branch is unshipped and every old tenant token expires within
  five minutes.

## Acceptance criteria

| ID | Acceptance |
|---|---|
| `R7-AUTH-1` | API-key exchange, OIDC login, human refresh, workload `jwt-bearer`, and RFC 8693 delegation all call one issuance workflow that checks current state, resolves current grants, and mints the same five-minute JWT with `permissions: PermissionSet`, fixed Wyrd audience, and existing identity/scope/audit claims. Any retained `roles` claim is informational only. |
| `R7-AUTH-2` | Tenant request and stream authentication verify signature, issuer, audience, and expiry locally and construct the runtime principal directly from claims, with no auth-store read, request-time role resolution, or positive verifier cache. |
| `R7-AUTH-3` | A restricted service-principal JWT is allowed for a covered Bifrost table and denied for an uncovered table after normal stable-table resolution; schema and exact-table scope behavior remains correct. |
| `R7-AUTH-4` | Revoking credential A immediately prevents A from exchanging again; a token A already minted remains usable until expiry; surviving credential B exchanges and spends its token immediately without epoch ordering or sleep. |
| `R7-AUTH-5` | Suspending/deleting a tenant principal or changing its grants immediately affects new token issuance, while an existing tenant token retains its immutable authority only until its five-minute expiry. |
| `R7-AUTH-6` | Platform requests still observe credential revocation, principal suspension/deletion, and grant changes on the next request through current-state database revalidation with no cache. |
| `R7-AUTH-7` | No active code or authoritative prose retains the deleted epoch, revocation-checker, per-request tenant admission, verifier-cache, or request-time permission-resolution design; the recorded authority audit includes `AGENTS.md` and `architecture/wyrd-doctrine.mdx`. |
| `R7-AUTH-8` | A Bifrost request or stream requires an unexpired token when admitted; admitted work may finish only under its existing bounded deadline, and expiry refuses the next admission without mid-stream reauthentication machinery. |
| `R7-AUTH-9` | One concrete synchronous Wyrd access-token verifier owns only local cryptographic validation state; external OIDC verification remains issuance-side and no verifier trait, factory, checker, or database dependency joins them. |
| `FIND-admin-principals-13` | Invalid local transfer path/query extraction returns the documented status, problem media, standard shape, and operation-listed stable code. |
| `FIND-admin-principals-R5-2` | Unknown and wrong-kind principal revokes commit exactly one allowed decision and no effect; lookup failure returns internal failure and commits neither. |
| `FIND-admin-principals-R7-5` | Every named closure test has an exact nonzero selector and owning-lane result in the implementation evidence. |

## Focused proof

- Unit-test the `permissions` claim round-trip, informational-only `roles`, wrong
  signature, wrong issuer, wrong audience, expiry, malformed permissions, and
  direct runtime construction without a resolver.
- Prove API-key exchange, OIDC login, human refresh, workload `jwt-bearer`, and
  RFC 8693 delegation all reach the same issuance owner and emit the same claim
  shape. Through production routes, prove current inactive credential,
  principal, and tenant refusal and current grant resolution before issuance.
- Through a real server, mint with A, revoke A, prove A cannot exchange again,
  prove A's existing token remains valid before expiry, and prove B exchanges
  and spends immediately. Prove expiry with a bounded test clock or short test
  token, never a five-minute sleep.
- Through real Bifrost gRPC serving, prove exact-table or schema-scoped
  permission allows the covered table and refuses another table, and prove an
  unexpired token is required at stream establishment. Structural inspection
  must also show no auth-store dependency on request/stream establishment.
- Through the platform route, prove current-state revocation, suspension, and
  grant withdrawal are observed on the next request.
- Exercise invalid local transfer extractors through the assembled authenticated
  router and principal-revoke no-effect/error cases through real Postgres.
- Search the cumulative active tree for every deleted symbol and obsolete
  architecture phrase; classify any residual occurrence as historical evidence,
  an unrelated platform concept, or a defect.
- Re-read `AGENTS.md` and `architecture/wyrd-doctrine.mdx` against the final
  flow, record that each is current or update only the exact stale statement.

## Verification

Run Cargo-backed commands sequentially. Use the narrowest existing task that
owns each changed surface; do not substitute `mise run gate`, `test:rust`, a
whole-workspace test aggregate, or an ad hoc `--all-features` test lane. The
canonical `mise run lints` command is the required all-features lint exception.

At minimum:

- `mise run fmt:check`
- `mise run lints`
- `mise run check:client-tier`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:tenant-isolation`
- `mise run check:from-pools-allowlist`
- exact nonzero selectors for every named auth, Bifrost authorization, local
  transfer, and principal-revoke test
- update `test:principals:unit` if its deleted `into_verified_*` selector is
  stale, then run `mise run test:principals:unit`
- `mise run test:principals:integration`
- `mise run test:shared`
- `mise run test:sql`
- `mise run test:platform:journey` twice consecutively
- `mise run test:identity:journey`
- `mise run test:cli:journey`
- `mise run test:bifrost:journey:mcp`
- `mise run test:bifrost:integration:server`
- `mise run test:bifrost:journey:server`
- `mise run codegen:check`
- `mise run docs:check`
- strict rustdoc for each affected crate
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f <final-candidate>`

Append one evidence table mapping every acceptance row to implementation
commits, exact focused commands, owning lanes, and results.

## Implementation evidence

Candidate: `a9706766` (branch `claude/admin-principals-spec-qfsmjc`), R7 range
`ddd80c7a..a9706766`. Focused tests ran on the final candidate through
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && …'`
with `mise exec -- cargo nextest run --locked -p <crate> <target> -E 'test(=<name>)'`;
the platform and CLI journeys set their lane gates (`WYRD_AUTH_E2E=1`,
`WYRD_CLI_E2E=1`), and the Bifrost server journeys use `-P journey --run-ignored=all`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `R7-AUTH-1` one issuance workflow, same five-minute `permissions` JWT | `5bb23ab1`, `8b259afe`; `wyrd-auth/src/issuance.rs` `TenantTokenIssuer` | `wyrd-auth --lib`: `issuance::pg_tests::issue_signs_current_grants_and_follows_grant_changes`, `issuance::pg_tests::a_suspended_principal_is_refused_by_every_grant`, `issuance::pg_tests::a_human_session_carries_current_grants_and_a_refresh_token`; `wyrd-auth-verify --lib`: `tests::access_token_claims_round_trip_permissions`, `tests::token_verifier_treats_roles_as_informational`; lanes `test:principals:unit`, `test:principals:integration`, `test:identity:journey` (20/20) | PASS |
| `R7-AUTH-2` local tenant verification, direct runtime construction | `a7d9960f`, `dbfa0e29`, `8b259afe` | `wyrd-auth-verify --lib`: `tests::token_verifier_builds_principal_from_permissions_claim`, `tests::token_verifier_rejects_wrong_signature`, `tests::token_verifier_rejects_wrong_issuer`, `tests::token_verifier_rejects_wrong_audience`, `tests::token_verifier_rejects_expired_token`, `tests::token_verifier_rejects_malformed_permissions`; `vala-bifrost-redux --lib`: `gate::tests::gate_authenticates_before_reading_frames` | PASS |
| `R7-AUTH-3` restricted service JWT allowed/denied per Bifrost table | `dbfa0e29`; `issuance.rs` scoped grant decode | `wyrd-testing --test server`: `query::tenant_scoped_roles_reach_only_their_granted_bifrost_tables`; `wyrd-auth --lib`: `issuance::tests::decodes_scoped_bifrost_grants_and_rejects_unscoped_rows` | PASS |
| `R7-AUTH-4` revoke A stops exchange, A's token lapses at expiry, B immediate | `5bb23ab1`, `8b259afe` | `wyrd-server --test platform_admin_e2e`: `a_revoked_credential_mints_nothing_and_its_token_lapses_at_expiry`, `a_tenant_rotates_an_automation_credential_without_an_outage`; lane `test:platform:journey` twice (38/38, 38/38) | PASS |
| `R7-AUTH-5` principal status and grant changes govern new issuance only | `5bb23ab1`, `8f06e6b1` | `issuance::pg_tests::a_suspended_principal_is_refused_by_every_grant`, `issuance::pg_tests::issue_signs_current_grants_and_follows_grant_changes`; `wyrd-cli --test cli`: `principal_journey::principal_revoke_cli_journey`; lane `test:cli:journey` | PASS |
| `R7-AUTH-6` platform plane observes changes next request | `3b85cc33` | `platform_admin_e2e`: `a_live_platform_session_observes_grant_withdrawal_and_suspension_on_its_next_request`, `revoking_a_platform_credential_ends_its_live_sessions`; `wyrd-auth --lib`: `platform_credentials::pg_tests::revocation_takes_effect_on_the_next_request` | PASS |
| `R7-AUTH-7` no epoch/checker/cache/request-time resolution remains; authority audit | `a7d9960f`, `a5cd1a5f`, `ae5010c8` | `git grep` over the active tree for `tokens_not_before`, revocation epoch, authorization epoch, `RevocationChecker`, verifier/token cache, `PermissionResolver`: no code hit; `architecture/wyrd-security-posture.md:139` states the absence; remaining hits are historical review packets. `AGENTS.md` and `architecture/wyrd-doctrine.mdx` re-read: neither describes tenant-token verification, so both are current and unchanged | PASS |
| `R7-AUTH-8` admission-time expiry, admitted stream finishes | `3e81efd4` | `wyrd-testing --test server`: `query::an_expired_bearer_finishes_its_admitted_stream_but_opens_no_other`; lane `test:bifrost:journey:server` (14/14) | PASS |
| `R7-AUTH-9` one concrete synchronous verifier, OIDC issuance-side | `a7d9960f`; `wyrd-auth-verify/src/lib.rs` `TokenVerifier` (keys, issuer, settings; sync `verify`) | Structural: no verifier trait or factory; `ExternalVerifier` is a separate type used only by `wyrd-auth` callback, `jwt_bearer`, `platform_login` and server boot/issuance state; `wyrd-auth-verify` has no `sqlx` dependency | PASS |
| `FIND-admin-principals-13` local transfer extractor problems | `7fa66d64` | `wyrd-server --test pg_openapi_contract`: `an_unextractable_local_transfer_locator_answers_with_a_documented_problem` | PASS |
| `FIND-admin-principals-R5-2` revoke miss commits one allowed decision | `e3919db6`, `7cd4ddf3`, `ce30430d` | `wyrd-server --lib`: `components::admin::routes::pg_tests::a_revocation_that_finds_nothing_records_one_allowance`; `wyrd-auth --lib`: `revoke::pg_tests::a_lookup_failure_is_internal_rather_than_not_found` | PASS |
| `FIND-admin-principals-R7-5` exact nonzero selectors | this table | every selector above ran nonzero (`N tests run: N passed`) on `a9706766` | PASS |

Lanes: `fmt:check`, `lints`, `check:client-tier`, `check:unwrap-audit`,
`check:clippy-allow-audit`, `check:tenant-isolation`,
`check:from-pools-allowlist`, `test:principals:unit`,
`test:principals:integration`, `test:shared`, `test:sql` (246 passed),
`test:platform:journey` ×2, `test:identity:journey`, `test:cli:journey`,
`test:bifrost:journey:mcp` (9/9), `test:bifrost:integration:server` (67/67),
`test:bifrost:journey:server` (14/14), `codegen:check`, `docs:check`,
`check:docs`, and `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f a9706766`
all pass.

Strict rustdoc (`-D missing_docs -D rustdoc::broken_intra_doc_links
-D rustdoc::private_intra_doc_links --document-private-items`) ran for each
affected crate. Every reported location was attributed with `git blame`: the
three introduced by this change were fixed in `a9706766`; the remaining 224
predate `c5c20754` in files this change did not introduce them into, and are
outside this task.

Out-of-task fix recorded: `67b4d0ba` makes the audit publisher's chain-head
freeze `FOR UPDATE NOWAIT`, so one locked tenant can no longer stall every
tenant's publication sweep, which had deterministically failed
`audit_publication::a_stalled_tenant_does_not_block_another_tenants_history`.
`frozen_audit_range_replays_once_while_its_tail_waits` setup was rewritten to
commit its freeze before competitors start; both pass. Residual risk: that replay
journey races the server's own 5-second sweep in a millisecond window and has no
retry.

Non-goals: no compatibility alias, epoch replacement, verifier trait, or
configuration was added; `changes/active/admin-principals/spec.md` was not
modified by this task.

Status: `IMPLEMENTED` — route to `$wyrd-task-review`.
