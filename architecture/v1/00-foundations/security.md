# Security Foundation

This is the reader path for Wyrd's Auth plane foundation. It cross-references
the per-stage architecture notes landed by PR 5.3.

## Reader Path

1. `principal.md` - identity types, principal ids, and delegation chain shape.
2. `permission-model.md` - typed permissions and resolution rules.
3. `permission-check.md` - the authorization chokepoint trait.
4. `token-issue.md` - issuing JWTs, delegated tokens, and API keys.
5. `token-verify.md` - verifying JWTs, resolver seam, and chain flattening.
6. `builtin-roles.md` - five builtin roles and seed lifecycle.
7. `service-identity.md` - card-bound non-human runtime identity and token exchange.
8. `errors.md` - wire-visible error-code foundations.

Some per-stage notes may land after this index. When a file is missing, use the
corresponding plan stage as the temporary source until the architecture page is
added.

## Boundary Diagram

```text
wyrd-runtime
  Principal, PrincipalKind, Permission, PermissionSet,
  PermissionCheck, builtin roles

wyrd-auth-issue
  issuing keys, JWT claims, API-key hash helpers,
  delegation depth validation

wyrd-auth-verify
  JWT verifier, key map, PermissionResolver trait,
  verified token promotion

wyrd-sql
  tenant-bound role lookup, service-account lookup,
  credential rows, audit rows

wyrd-server
  HTTP handlers, TenantConn acquisition, preview gate,
  transactional credential/audit writes, error mapping
```

Shared auth crates do not depend on SQL. The resolver seam is
`PermissionResolver`: `wyrd-auth-verify` asks for roles and permissions, while
`wyrd-server` provides the SQL-backed implementation.

## Cross-Cuts

Access-token TTL is short by design: revocation and role changes are bounded by
token expiry and verifier-cache expiry. The default access-token TTL is 15
minutes, refresh-token TTL is 30 days, and API-key TTL is 365 days.

Tenant isolation is enforced at every boundary: JWT tenant claims, tenant-bound
database connections, tenant-bearing service-account lookup, and role
resolution from the caller's tenant only.

`MAX_DELEGATION_DEPTH` is five. Issuance refuses deeper chains and verification
rejects deeper chains so non-conforming tokens do not enter request context.

Public errors are mapped through `wyrd_spec::error::WyrdError`. Auth verifier
unavailability is `WYRD_AUTH_503_VERIFY_UNAVAILABLE` and is retryable with
backoff; permission denial is `WYRD_PERMISSION_403_DENIED_RBAC`.

## Deferred Work

Policy-plane authorization for richer card and runtime decisions lands in a
later PR. Mesh `ext_authz` uses the same `X-Wyrd-Access-Token` contract; no
additional Wyrd identity header is introduced. JWKS discovery, hot key reload,
enterprise OIDC login, and user impersonation are also later work.
