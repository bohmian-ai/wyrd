# Authentication and authorization security domain review

## Subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `eb9b2f69cb883fa508ed168f21cb868451e61b82`
- Scope: tenant and platform authentication, token issuance and verification,
  RBAC, Bifrost object authorization, credential/principal/grant lifecycle,
  plane and tenant isolation, and authorization audit behavior.

## Authority and source coverage

| Boundary | Authority and source traced | Result |
|---|---|---|
| Tenant issuance | Approved spec revision 12 `REQ-005`, `REQ-012`, `INV-009`, `INV-013`, `AC-018`; R7 `R7-AUTH-1`; `wyrd-auth/src/{issuance,exchange_api_key,callback,refresh,jwt_bearer}.rs`; all production route callers | **FAIL** for delegated permission attenuation (`AUTH-R8-1`); otherwise all five grants converge on `TenantTokenIssuer`, re-read current tenant/principal/grants, and mint the same five-minute access-token shape |
| Tenant request authentication | R7 `R7-AUTH-2`, `R7-AUTH-8`, `R7-AUTH-9`; `wyrd-auth-verify::TokenVerifier`; HTTP token extraction/middleware; gRPC Gate authentication | PASS: synchronous Ed25519, issuer, fixed audience, expiry, and tenant checks construct the runtime principal from `permissions` without a database read or positive verification cache |
| Tenant RBAC and Bifrost | Security posture authorization rules; `PermissionSet`; HTTP permission checks; Oracle stable-table resolution and `authorize_resolved_tables`; Gate admission | PASS: roles are informational, request authorization consumes claimed permissions, and Bifrost query authorization checks every catalog-resolved stable table before admission or source IO |
| Tenant revocation and renewal | Spec `REQ-005`, `REQ-007`, `INV-013`; API-key, principal, tenant, grant, refresh, and client renewal paths | PASS: lifecycle changes stop new issuance immediately while existing signed tenant tokens retain their snapshot only through expiry; refresh rotation re-enters the shared issuer |
| Platform plane | Spec `REQ-014` through `REQ-019`, `REQ-041` through `REQ-046`, `INV-004`; platform extractor, `PlatformSessions::confirm`, grant resolution, `PlatformAuthorization`, `OperatorPool` queries | PASS: platform scope is structurally distinct and every request re-reads the credential or principal plus current grants without an authority cache |
| Delegation | Security posture delegation rules; `DelegateToken::execute`; `TenantGrant::Delegation`; `TenantTokenIssuer::issue`; `IssuingKey::issue_access_token`; token-exchange route | **FAIL**: delegated authority may widen (`AUTH-R8-1`) and a denied delegation permission decision is not durably audited (`AUTH-R8-2`) |
| Audit and secrecy | `AGENTS.md` audit rules; `architecture/agent-rules.md`; auth audit helpers and issuance events; trace annotations; secret-bearing DTOs | **FAIL** only for `AUTH-R8-2`; successful issuance is transactionally coupled to its audit, bearer/credential values are skipped or redacted, and ordinary route authorization remains fail closed |
| Deleted hybrid design and dependencies | R7 `R7-AUTH-7`; active source/manifests; epoch-drop migration; dependency graph | **FAIL** for one unused direct cache dependency (`AUTH-R8-3`); no request-time tenant admission checker, authorization epoch, permission resolver, verifier cache, or database-backed tenant verifier remains |

## Security Audit

### Critical

- None.

### High

- **`AUTH-R8-1` — INCORRECT — delegated tokens can widen the caller's authority.** `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:213-238` checks only that the caller holds `delegation:issue`, then asks the shared issuer to mint for the target principal; `crates/wyrd/wyrd-auth/src/issuance.rs:281-340` resolves and signs the target's complete current `PermissionSet` without intersecting it with the caller's claimed permissions. **Exploit path:** grant a constrained principal `delegation:issue` plus access to table A, give a Card-bound target access to tables A and B (or another stronger operation), and the constrained principal can exchange its token for the target and receive a valid token authorized for B. **Impact:** token exchange becomes privilege escalation, violating `architecture/wyrd-security-posture.md:147-151` (every hop authorized and effective authority can only narrow) and the R7 requirement to preserve delegation rather than silently widen it. **Required correction:** in the existing shared issuance path, attenuate a delegated token's resolved target permissions to the authority covered by both the verified caller snapshot and the target's current grants; retain the existing delegation-depth and `delegation:issue` checks, add no policy engine or second issuer, and prove with a production token-exchange path that a target's broader table/operation permission is absent while the shared narrower permission remains usable.

### Medium

- **`AUTH-R8-2` — VIOLATION — denied delegation authorization leaves no durable decision and does not fail closed on audit loss.** `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:213-217` returns `PermissionDenied` immediately after evaluating `delegation:issue`; `crates/wyrd/wyrd-server/src/components/auth/routes.rs:200-216` then calls `audit_scope_mint_failure_best_effort`, but `crates/wyrd/wyrd-auth/src/card_scope.rs:213-239` emits only errors carrying a Card-scope root, so this denial produces no row, and even eligible failures swallow append/commit errors. **Impact:** an attacker can repeatedly attempt unauthorized delegation with no authorization-decision trail, contrary to the repository rule that every permission evaluation records allowed and denied outcomes and that unauditable decisions fail closed. **Required correction:** append the denied `delegation:issue` decision through the canonical audit path on the already-open `TenantConn`, commit that denial before returning it, and substitute the stable audit-unavailable failure if append or commit fails; prove one denied decision with the caller, permission, requested subject resource, tenant, and outcome, plus an injected audit failure that still refuses and claims no recorded denial.

### Low / Defense In Depth

- **`AUTH-R8-3` — VIOLATION — the removed tenant verifier cache still leaves an unused direct cache dependency.** `crates/wyrd/wyrd-server/Cargo.toml:54` declares `moka = { version = "0.12", features = ["future"] }`, while no `wyrd-server` source uses `moka`; the dependency was introduced with the old authorization-epoch cache and remains in the package graph after that cache was deleted. **Impact:** this preserves unnecessary supply-chain and build surface and leaves `R7-AUTH-7`'s explicit cache-dependency deletion incomplete. **Required correction:** remove only the unused `wyrd-server` direct dependency and regenerate `Cargo.lock`; retain `moka` where the live OIDC JWKS cache uses it, and prove the server package no longer lists it directly while the normal lint and auth lanes still pass.

### Positive Controls

- All five tenant grants reach one concrete `TenantTokenIssuer`; current tenant,
  principal, and role state are resolved on its `TenantConn` before signing.
- `TokenVerifier` contains only local keys, issuer, and clock policy; it rejects
  wrong signature, issuer, audience, expiry, tenant, malformed permission, and
  invalid principal/Card shapes without SQL or a verified-token cache.
- Tenant JWT authority comes only from `permissions`; retained role names are
  informational and no request-time role resolver remains.
- Bifrost query authorization derives exact table scopes from catalog-resolved
  stable identities and refuses one uncovered table before admission or IO.
- Platform requests structurally reject tenant tokens and re-read live
  credential/principal/grant state through `OperatorPool` on every request.
- API-key refusal remains fixed-cost and indistinguishable, refresh replay
  containment is durable, and bearer/credential material is not logged or
  placed in audit payloads.

## Verification limits

- This was a review-only static audit; candidate source was not edited and the
  recorded verification lanes were not re-run.
- The R7 evidence credibly covers shared issuance, local verification,
  revocation-window semantics, platform current-state checks, scoped Bifrost
  reads, and the named negative JWT cases, but it contains no proof that
  delegation attenuates permissions or durably audits a permission denial.
- Dependency inspection used the committed manifest and Cargo metadata; the
  unused direct `wyrd-server` `moka` declaration is distinct from the live OIDC
  JWKS cache dependency.
- Candidate identity remained
  `eb9b2f69cb883fa508ed168f21cb868451e61b82` after the report was written.

## Overall result

**FAIL** — one reachable privilege-escalation path, one missing fail-closed
authorization audit, and one explicit rejected-cache cleanup item remain.
