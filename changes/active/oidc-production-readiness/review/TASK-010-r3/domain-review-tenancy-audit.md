# Domain Review: Tenancy and Canonical Audit

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `1f4466a9ae482eb1311f6e5206484758bc272ad8`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Prior remediation: `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- Current remediation direction: `changes/active/oidc-production-readiness/review/TASK-010-r2/lead-direction-FIND-TASK-010-10.md`, which supersedes `TASK-010-R2-public-edge-device-admission.md`

`HEAD` was the candidate before and after this review. `.codegraph/` is absent,
so the cumulative diff, current source, callers, migrations, and tests were
inspected with repository navigation tools. `FIND-TASK-010-1` remains routed
to TASK-011 by `review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md` and was
not reopened.

## Reviewed boundary

This review traced tenant selection and canonical audit end to end across:

- authorization initiation, provider callback, authorization-code redemption,
  device authorization/approval/redemption, refresh, RFC 7009 revocation, API-key
  exchange, and delegation;
- route-key, opaque-state, authorization-code, device-code, refresh-token, and
  access-token tenant routing into `TenantConn`;
- forced-RLS query ownership, the narrow login-state owner lookup, and the
  limited cross-tenant `OperatorPool` uses;
- exact human-connection revision fencing, principal/refresh-family locking,
  token issuance, role replacement, and rollback;
- the sole `vala.audit_staging` append path for login outcomes, role changes,
  token issuance, device redemption, refresh-reuse containment, and logout
  revocation; and
- the lead-directed deletion of the image-local device limiter. No application
  limiter, edge manifest, rate-limit setting, alternate audit sink, or durable
  admission state was added.

Applicable authority was `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, the
spec-driven-development and maintainer-style references, approved spec revision
11, TASK-010, the R1 remediation and verdict, the R2 verdict and validated
ledger, and both standing lead directions. The review applied the standing
rule that mechanisms absent from the RFCs and comparable widely used projects
are drift and cannot be required as remediation.

## Authority and source coverage

| Boundary | Source and caller trace | Assessment |
| --- | --- | --- |
| Callback tenant selection | `AuthorizationCodeExchange::execute` hashes opaque state, obtains only the owner tenant through `WyrdPostgres::login_state_tenant`, then consumes the row on that tenant's `TenantConn` (`crates/wyrd/wyrd-auth/src/callback.rs:156-217`; `crates/wyrd/wyrd-sql/src/postgres.rs:207-227`). The definer function returns only the tenant of one unconsumed, unexpired hash, has a fixed search path, and grants execution only to `wyrd_app`; the row itself remains under forced RLS (`crates/wyrd/wyrd-sql/migrations/20260925000001_auth_login_state_binding.sql:24-85`). | **PASS.** State is server-generated and stored hashed. Headers, paths, provider claims, and callback query tenant data cannot become tenant authority. |
| Final callback effects and connection lifecycle | After verified provider identity, callback opens one tenant transaction, resolves `(issuer, subject)`, takes the User refresh-family lock, takes the existing connection-slot lock, and verifies the exact active connection revision before replacing roles or recording a code/device approval (`callback.rs:367-468,623-667`). | **PASS.** `FIND-TASK-010-12` remains closed. A deactivation/replacement that wins leaves no User, role, code, approval, or success-audit effect; a callback that wins commits before lifecycle mutation. |
| Successful callback audit and rollback | Role mutation conditionally appends `auth.user.roles.sync`; every successful non-test callback independently appends `auth.login` after writing its authorization code or device approval, and the transaction commits only afterward (`callback.rs:415-468`). The focused callback tests cover authorization-code success, unchanged-role device success, distinct role-sync evidence, and injected login-audit failure rollback (`crates/wyrd/wyrd-server/src/auth/callback.rs:319-381,513-666,794-850`). | **PASS.** `FIND-TASK-010-13` remains closed. Required audit failure cannot establish a User, role change, code, or approval. |
| Authorization-code redemption | The unverified tenant prefix routes only to `TenantConn`; the code hash is deleted under RLS, then the stored OAuth client, exact redirect, S256 verifier, expiry, principal, and connection revision govern issuance (`callback.rs:472-605`). Human issuance reuses the family-then-connection lock order and appends canonical token-exchange audit before the caller commits (`crates/wyrd/wyrd-auth/src/issuance.rs:646-755`). | **PASS.** No request tenant or client value is durable authority, and no token exists before redemption. |
| Device authorization and redemption | Route key resolves one tenant, and the device row stores a hash plus tenant-owned user code (`crates/wyrd/wyrd-auth/src/cli_logins.rs:126-169`). Approval begins the ordinary server-bound login; redemption routes by the device-code prefix, locks and classifies the hashed row under RLS, deletes it, mints once for the stored principal/connection, appends `auth.device_code.grant`, and commits (`cli_logins.rs:263-385`; `crates/wyrd/wyrd-sql/src/queries/auth/device_authorizations.rs:28-104,231-271`). | **PASS.** Tenant, principal, connection, one-use issuance, and audit share the owning transaction. The lead-directed operator ingress note does not add a second tenant or audit authority. |
| Refresh routing, rotation, and containment | The unverified refresh JWT tenant claim routes to `TenantConn`; the stored token hash under RLS is authority. Client binding is checked before the family lock. The confidential client uses the RLS-only `active_refresh`; the public client atomically consumes an active row. Only a row marked `rotated` triggers replay handling, and containment uses the existing rotation chain with canonical audit (`crates/wyrd/wyrd-server/src/components/auth/routes.rs:330-370`; `crates/wyrd/wyrd-auth/src/refresh.rs:112-229`; `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:46-55,134-176,207-241`). | **PASS.** `FIND-TASK-010-8` and `FIND-TASK-010-11` remain closed. The active lookup has no duplicate tenant predicate, and replay does not revoke unrelated login chains. |
| RFC 7009 logout revocation | The token claim routes only; the row hash under RLS and its stored OAuth-client binding decide the operation. The owner locks the principal family, revokes only the presented rotation chain, appends `auth.token.revoke` on the same `TenantConn`, and commits (`crates/wyrd/wyrd-auth/src/cli_logins.rs:387-454`). Unknown tokens retain RFC 7009's idempotent success and establish no authority. | **PASS.** A successful durable revocation cannot commit without its canonical audit row. |
| Other tenant grant consumers | API-key exchange parses the embedded tenant only to open `TenantConn`, then verifies the stored prefix/hash and mints through `TenantTokenIssuer`; delegation opens the subject token's routed tenant and verifies both tokens for that tenant (`crates/wyrd/wyrd-server/src/components/auth/routes.rs:220-322`; `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:119-221`). `TenantTokenIssuer::issue` derives tenant from the connection, reloads current principal/roles/scope, and appends `auth.token.exchange` before return (`crates/wyrd/wyrd-auth/src/issuance.rs:405-515`). | **PASS.** Existing workload/API-key/delegation semantics remain tenant-bound and use the shared audit owner. |
| SQL capability boundary | Changed login, device, refresh, role, user, issuance, and audit functions accept `TenantConn`. The only `OperatorPool` functions in the reviewed auth query surface are the named cross-tenant sealing-key rotation reads/writes in `human_connections.rs`; no public grant handler receives or propagates a raw `PgPool`. | **PASS.** INV-007's `TenantConn`/`OperatorPool` separation is preserved. |
| Canonical audit owner | `wyrd-auth::audit::append_auth_audit` delegates only to `vala_sql::queries::audit_staging::append_audit` on the caller's `TenantConn` (`crates/wyrd/wyrd-auth/src/audit.rs:126-140`). That append owns the per-tenant chain-head lock, immutable staging insert, and head update inside the caller transaction (`crates/vala/vala-sql/src/queries/audit_staging.rs:20-143`). Refusal records use the same append in a separate tenant transaction and introduce no alternate table or publisher (`wyrd-auth/src/audit.rs:142-164`). | **PASS.** The candidate has one audit write path and no parallel WAL, relay, table, or log authority. |
| FIND-TASK-010-10 lead correction | Candidate `1f4466a9a` removes the image NGINX `map`, zone, status, and `limit_req` rule plus its one-image assertions, and adds only the operator note at `docs/src/content/docs/self-hosting/sso-and-oidc.svx:19-23`. | **PASS for this domain.** The correction adds no tenancy or canonical-audit mechanism. Per lead direction, no image limiter, edge manifest, application limiter, setting, or persistent state is required. |

## Prior-finding closure

- `FIND-TASK-010-1` is owned by TASK-011 and was not reassessed.
- `FIND-TASK-010-8` remains closed by the RLS-only `active_refresh` lookup and
  the cross-tenant same-hash focused proof.
- `FIND-TASK-010-11` remains closed by rotated-only classification and
  rotation-chain containment; ordinary expiry/logout/revocation does not
  trigger principal-wide theft handling.
- `FIND-TASK-010-12` remains closed by callback's final-transaction
  family-then-connection fence and the multi-replica cutoff proof.
- `FIND-TASK-010-13` remains closed by the unconditional successful-login
  append and rollback proofs for authorization-code and device completion.
- R2's startup-profile tenancy proposal was independently rejected by the R2
  validation ledger: the lane provisions one tenant and now explicitly selects
  the supported single-tenant profile. This review found no new source evidence
  that would reopen it.
- `FIND-TASK-010-10` is corrected by the standing lead direction. This review
  does not replace that deployment finding with a nonstandard tenancy, audit,
  edge, or application mechanism.

## Material proposed findings

None.

No reachable behavioral, security, tenancy, durability, or public-contract
defect was found in this boundary. No placement, naming, structure, wording,
or optional-hardening note is promoted to a finding.

## Verification limits

This was an independent static audit of the immutable cumulative range and the
current source. I did not rerun Docker, Postgres, provider, or full journey
lanes. The task and R1 remediation records provide green evidence for the
narrow owner lanes relevant here: focused callback and refresh tests,
`test:principals:integration`, `test:sql`, the filtered connection-cutoff and
device journeys, `codegen:check`, tenancy/boundary checks, format, and lints.
The lead-direction record provides green `test:server:startup` and
`docs:check` evidence for the final three-file correction. Full unfiltered and
cross-language journeys belong to change review under the standing direction,
not this task remediation review. No unavailable required reviewer or missing
report was converted into a verification limit.

The candidate remained
`1f4466a9ae482eb1311f6e5206484758bc272ad8` throughout this review.

## Overall result

**PASS.** Tenant routing remains non-authoritative until an RLS-bound stored
binding is resolved; final login and grant effects are fenced and transactional;
successful durable auth changes fail closed on the sole canonical audit append;
and the lead-directed device-admission correction adds no alternate tenancy,
audit, rate-limit, or persistence mechanism.
