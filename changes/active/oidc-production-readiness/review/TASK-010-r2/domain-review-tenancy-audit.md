# Domain Review: Tenancy and Canonical Audit

## Immutable subject

- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- Overall result: **FAIL**

`FIND-TASK-010-1` is routed to TASK-011 by
`review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md` and was not reopened.

## Reviewed boundary

This review traced the cumulative authorization-server changes through the
tenant-routing value, `TenantConn`/forced-RLS authority, exact human-connection
binding, callback transaction, refresh lifecycle queries, canonical audit
append, OAuth client binding, and production startup profile. It used the R1
fix diff to locate the remediated owners but reassessed the complete
base-to-candidate behavior.

Applicable authority included `AGENTS.md` sections 2, 9, 11, and 12;
`architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`;
`architecture/references/doctrine/architecture-constraints.md`;
`architecture/references/architecture/patterns.md`;
`architecture/references/languages/testing-workflows.md`; the approved spec,
original task, R1 verdict and validation, remediation task, and lead direction.
`.codegraph/` is absent, so repository source and Git diffs were inspected
directly.

## Authority and source coverage

| Boundary | Source trace | Assessment |
| --- | --- | --- |
| Callback tenant and connection authority | `AuthorizationCodeExchange::finish_id_token_exchange` derives the tenant from verified `TrustedIssuer`, opens `TenantConn`, acquires the User refresh-family lock, then calls `fence_bound_connection`; that helper takes the existing human-connection slot lock and checks the exact connection id/revision before roles, code, or device approval can commit (`crates/wyrd/wyrd-auth/src/callback.rs:367-468,615-644`). | **PASS.** The pre-provider state consumption remains separate, but the final durable effects now share the lifecycle fence. If deactivation/replacement wins, the new User/role/code/approval writes are in the same uncommitted transaction and roll back. |
| Callback audit | The final callback transaction conditionally appends `auth.user.roles.sync`, always appends one redacted `auth.login` event after a successful code/approval write, and commits only afterward (`callback.rs:415-468,743-776`). Tests cover authorization-code success, unchanged-role device success, distinct role-sync evidence, and injected `auth.login` append failure rolling back User, roles, code, and approval (`crates/wyrd/wyrd-server/src/auth/callback.rs:318-381,512-666`). | **PASS.** R1 `FIND-TASK-010-13` is closed through the one canonical `vala.audit_staging` append and the owning tenant transaction; no second audit path was added. |
| Multi-replica callback cutoff | `tenant_connection_session_cutoff_journey` parks the callback on the User-family lock, commits deactivation on another replica, resumes, and asserts downstream `access_denied` plus no code, role change, login audit, or refresh row (`crates/wyrd/wyrd-server/tests/identity_e2e.rs:3395-3577`). | **PASS.** This is the required reachable interleaving for R1 `FIND-TASK-010-12`, not a timing-only healthy-path assertion. |
| Refresh tenant isolation | New `ACTIVE_REFRESH_SQL` selects only by hash and lifecycle state; `active_refresh` binds no tenant value and executes only on `TenantConn` (`crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:46-55,159-176`). The focused test creates the same hash across two tenants and proves the other tenant remains invisible while revoked/expired local rows remain inactive (`crates/wyrd/wyrd-auth/src/refresh.rs:891-957`). | **PASS.** R1 `FIND-TASK-010-8` is closed: the new lookup relies on forced RLS instead of duplicating tenant selection. Existing older explicit predicates in sibling refresh queries were not created by this remediation and are not reopened one caller at a time. |
| Refresh client and chain binding | `RefreshTokens::execute` gets the stored row under tenant RLS, rejects a different OAuth client before taking the principal-family lock, leaves the confidential `wyrd-ui` row unconsumed, and treats only `revoked_reason = 'rotated'` as replay; containment uses the existing rotation-chain owner and canonical audit (`crates/wyrd/wyrd-auth/src/refresh.rs:127-222`). | **PASS.** Tenant/client binding remains server-owned, and the correction does not broaden a tenant or principal family. |
| Authorization-code and device binding | Authorization code redemption treats the tenant prefix as routing only, deletes the hashed row under `TenantConn`, and checks stored client, exact redirect, PKCE, expiry, principal, and connection binding before issuance. Device approval remains an in-transaction update of the bound pending device row; redemption owns token minting and audit. | **PASS.** No request tenant or OAuth client value becomes durable authority, and no callback-time credential is stored. |
| Production startup profile | `test:server:startup` previously restarted the official image with `APP_ENV=production` and no tenant slug, which is the multi-tenant profile. The remediation adds `WYRD_SERVER_TENANT_SLUG=acme`, `WYRD_OPERATOR_KEK_SOURCE=file`, and a mounted file KEK to every serve invocation (`scripts/server/test-startup.sh:53-63,116-125,228-230`). `verify_operator_keys` expressly skips its active-tenant readiness check when `auth.tenant_slug` is set (`crates/wyrd/wyrd-server/src/boot/mod.rs:1421-1454`). | **FAIL — TA-R2-001.** Single-tenant production with an owner-only file KEK is a legitimate supported deployment, but selecting it to clear this already-red lane removes the official-image multi-tenant production/Vault/readiness proof. |

## Prior-finding closure

- R1 `FIND-TASK-010-8` is closed by the RLS-only `active_refresh` lookup and
  cross-tenant focused test.
- R1 `FIND-TASK-010-12` is closed by the final-transaction connection-slot
  fence and deterministic multi-replica cutoff journey.
- R1 `FIND-TASK-010-13` is closed by transactional `auth.login` append and
  rollback tests for both code and device completion.
- The routed `FIND-TASK-010-1` was not reassessed.

## Material proposed findings

### TA-R2-001 — VIOLATION: the startup remediation clears a red gate by changing it from multi-tenant production to the single-tenant exemption

- **Violated obligation:** `AGENTS.md` section 12 forbids weakening a gate to
  make it pass and requires the underlying failure to be fixed. The security
  authority requires multi-tenant production to use Vault over HTTPS and to
  verify every active tenant's active Operator KEK before readiness. The R1
  remediation required the existing official-gateway/startup check as proof of
  the device-verification limiter; it did not authorize changing the
  deployment topology that the lane already exercised.
- **Exact location:** `scripts/server/test-startup.sh:53-63,116-125,228-230`;
  the skipped production gate is
  `crates/wyrd/wyrd-server/src/boot/mod.rs:1421-1454`.
- **Evidence:** Before the R1 fix diff, the production restart supplied no
  `WYRD_SERVER_TENANT_SLUG`, so `auth.tenant_slug.is_none()` selected
  multi-tenant production. The implementation record says that restart became
  red because multi-tenant production correctly refused a non-Vault Operator
  KEK. The candidate resolves that refusal by setting
  `WYRD_SERVER_TENANT_SLUG=acme` and a file KEK. That makes
  `verify_operator_keys` return before enumerating tenants or reading their
  active keys. The reported green lane therefore proves a different supported
  profile; it does not fix or preserve the red profile.
- **Observable consequence:** the repository's official-image restart gate can
  remain green while multi-tenant production cannot start with its required
  Vault configuration, cannot read one provisioned tenant's active key, or
  bypasses the readiness behavior through integration drift. Unit/in-process
  tests of `verify_operator_keys` do not preserve the deleted image/config/
  boot integration coverage.
- **Required testable correction:** restore the startup lane's multi-tenant
  production restart and supply the already-approved conventional HashiCorp
  Vault source over HTTPS with readable active-version keys for every tenant
  provisioned by the lane. Keep the owner-only mounted signing-key correction
  and the new NGINX device-verification assertions. Do not add a new key
  provider, setting, fallback, or parallel startup harness. If single-tenant
  official-image startup also needs proof, it may be a separate focused case;
  it cannot replace the existing multi-tenant case.
- **Focused closure proof:** `mise run test:server:startup`, showing the same
  official image completes its production restart without
  `WYRD_SERVER_TENANT_SLUG`, reaches ready only with the configured HTTPS Vault
  tenant keys, preserves persisted tenant data, and still proves the device
  route's per-client limit without throttling sibling auth routes.

This finding requires no novel mechanism. HashiCorp Vault is already the
approved multi-tenant production boundary and an existing repository
dependency/test facility. The finding does not require full journeys or a
broad aggregate.

## Verification limits

This was an independent static source-and-diff audit. I did not rerun Docker,
Postgres, Keycloak, or the recorded narrow lanes. The remediation artifact
records green focused callback, refresh, SQL, tenant-isolation,
`test:principals:integration`, filtered identity, and startup lanes. Source and
focused tests credibly close the callback/audit/RLS findings above, but a green
startup result cannot establish the removed multi-tenant profile after the
script selects the single-tenant branch. Full unfiltered and cross-language
journeys remain change-review work, as directed; no missing required reviewer
was converted into a verification limit.

The candidate commit remained
`7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29` throughout this review.

## Overall result

**FAIL.** The callback transaction, canonical login audit, refresh RLS lookup,
and tenant/client binding are correct, but `TA-R2-001` is a material security
verification violation: the remediation made a red production startup lane
green by switching it to the single-tenant file-KEK exemption instead of
preserving and satisfying the multi-tenant Vault-backed production path.
