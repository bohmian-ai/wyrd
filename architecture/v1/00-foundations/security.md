# Security Foundation

This is the reader path for Wyrd's Auth and Policy plane foundation. The
repository-level security contract is
[`../../wyrd-security-posture.md`](../../wyrd-security-posture.md); the pages
below define the narrower v1 types and service boundaries.

## Reader Path

1. [`principal-extraction.md`](principal-extraction.md) - verified principal
   extraction at request boundaries.
2. [`permission-model.md`](permission-model.md) - typed permissions and
   resolution rules.
3. [`permission-check.md`](permission-check.md) - the authorization
   chokepoint.
4. [`policy-hook.md`](policy-hook.md) - policy evaluation and fail-closed
   composition.
5. [`service-identity.md`](service-identity.md) - card-bound non-human runtime
   identity.
6. [`errors.md`](errors.md) - wire-visible error foundations.

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

## Security contract

Production composition supplies a real permission resolver, policy decision
point, canonical audit writer, and Oracle audit relay. Permit-all, no-op,
in-memory, and test substitutes cannot satisfy production readiness.

Mesh `ext_authz` uses the same `X-Wyrd-Access-Token` contract; no additional
Wyrd identity header is introduced. JWT verification uses configured issuer,
audience, algorithm, key identity, JWKS refresh, revocation epoch, delegation,
and expiry rules from the repository-level security posture. Uncertainty in
identity, policy, or revocation state fails closed.
