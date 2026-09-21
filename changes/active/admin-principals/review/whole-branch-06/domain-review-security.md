# Admin principals whole-branch review 06 — security/RBAC/auth/audit

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/home/thorrester/Documents/GitHub/wyrd` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate / reviewed HEAD | `2c0408b683f7a548cec6dd08b35698d761d33b31` |
| Approved authority | `changes/active/admin-principals/spec.md`, approved revision 10 |
| Original tasks | `TASK-001` through `TASK-008` under `changes/active/admin-principals/tasks/` |
| Prior review and remediation | `changes/active/admin-principals/review/whole-branch-05/` |

The candidate matched the requested commit before and after inspection. This
review changed only this report.

## Reviewed boundary

The review traced both authentication and authorization planes end to end:

- tenant machine exchange, human OIDC callback, refresh rotation and replay
  containment, token verification on cache hit and miss, authorization epochs,
  credential/principal/role revocation, tenant admission, `Caller`, RLS-bound
  administration, canonical audit append, and client renewal/replay;
- platform credential exchange, platform OIDC discovery and callback,
  pre-registration and first-login `(issuer, subject)` pinning, platform session
  confirmation, grant resolution, `PlatformCaller`, platform authorization and
  its operator transaction, credential/principal revocation, tenant lifecycle,
  recovery, and canonical audit attribution;
- the R5 corrections for same-second epoch ordering, atomic federated pin/grant,
  stored principal-kind and credential attribution, stable no-effect decisions,
  tenant credential-revoke renewal, MCP shared authentication policy, and the
  related real-server evidence.

## Authority coverage

| Boundary | Governing authority | Result |
|---|---|---|
| Closed platform/tenant contexts and plane separation | `REQ-012`–`REQ-019`, `INV-003`–`INV-004b`, `INV-015`; `architecture/wyrd-design.md`; `architecture/v1/00-foundations/service-identity.md` | PASS |
| Principal, credential, role and tenant revocation | `REQ-005`–`REQ-011`, `REQ-028`, `REQ-048`, `INV-013`, `AC-005`, `AC-008`, `AC-010`, `AC-018` | **FAIL — `SEC-WB06-1`** |
| Federated tenant and platform identity | `REQ-034`–`REQ-035`, `REQ-041`–`REQ-046`, `AC-011`, `AC-015`–`AC-017`; security-posture OIDC/SSRF authority | PASS |
| Authorization and canonical audit atomicity | `REQ-016`–`REQ-019`, `REQ-037`, `AC-009`; `AGENTS.md` and `architecture/agent-rules.md` audit rules | **FAIL — `SEC-WB06-2`** |
| Tenant isolation | `REQ-014`, `REQ-031`, `INV-007`, `AC-004`; `TenantConn`/RLS authority | PASS |
| Credential secrecy and token handling | `REQ-008`–`REQ-009`, `INV-002`, `INV-011`–`INV-012`; `architecture/wyrd-security-posture.md` | PASS |
| Client and MCP renewal/replay | `REQ-047`–`REQ-048`, `AC-018` | PASS for the R5 correction boundary |
| Journey and focused proof | `AC-001`–`AC-019`, `VER-001`–`VER-006`, `AGENTS.md` testing authority | FAIL for the two unproved reachable gaps below |

## Source coverage

The cumulative base-to-candidate diff and surrounding callers were inspected,
with CodeGraph used first. Security-relevant source coverage included:

- `crates/shared/wyrd-auth-issue/src/lib.rs`
- `crates/shared/wyrd-auth-verify/src/lib.rs`
- `crates/shared/wyrd-auth-oidc/src/{jwks,registry,screening}.rs`
- `crates/shared/wyrd-client/src/{auth,transport/http,platform/handle,principals/handle}.rs`
- `crates/wyrd/wyrd-auth/src/{callback,exchange_api_key,platform_authz,platform_credentials,platform_login,platform_sessions,refresh,revocation_listener,revocation_resolver,revoke}.rs`
- `crates/wyrd/wyrd-server/src/components/auth/{caller_extractor,platform_extractor,token_extract}.rs`
- `crates/wyrd/wyrd-server/src/components/{admin,platform,principals}/`
- `crates/wyrd/wyrd-server/src/auth/revoke.rs`, `boot/auth.rs`, and the auth
  token routes
- the auth/platform/tenant SQL queries and migrations, canonical Vala audit
  staging append and projection, MCP adapter/tools, and the identity, platform,
  principal, client-transport, SQL, and MCP tests named by the R5 packet.

## Verification limits

The R5 implementation packet records the focused tests and the approved narrow
lanes as passing on `5ecc8a4e`, followed by the route-regression correction at
the current candidate. Those results credibly close their named selections.
This reviewer did not rerun the environment-owning suites.

The recorded evidence does not exercise production's nonzero epoch-cache TTL,
listener boot wiring, tenant-admin invalidation, or a request against a warmed
epoch cache after role withdrawal. The same-second callback test reads the
stored epoch directly, and verifier tests use controlled/zero-TTL resolvers.
The no-effect audit journey covers the R5-named branches but not tenant lifecycle
wrong-state, platform registration without a connection, or tenant principal
issue/list/create logical refusals.

## Security Audit

### Critical

- None.

### High

- **`SEC-WB06-1` — INCORRECT — authorization removed in Postgres remains usable from the production epoch cache.**
  - **Violated obligation:** `REQ-005`, `INV-013`, `AC-008`, `AC-010`, and the
    tenant-plane revocation contract require a credential, principal, or role
    withdrawal to stop live tokens no later than the next request.
  - **Exact location:**
    `crates/wyrd/wyrd-auth/src/revocation_resolver.rs:20,34-44,67-72,126-156`;
    `crates/wyrd/wyrd-auth/src/revocation_listener.rs:48-66,113-143`;
    `crates/wyrd/wyrd-server/src/boot/auth.rs:77-94`;
    `crates/wyrd/wyrd-auth/src/callback.rs:220-253`; and
    `crates/wyrd/wyrd-server/src/components/principals/routes.rs:577-599`.
  - **Evidence:** Production constructs `SqlRevocationCheck::new`, whose cache
    retains an epoch (including `None`) for five seconds. Every verified-token
    cache hit and miss consults this resolver, so a cached old answer admits an
    old token without reading the newly committed epoch. The existing
    `RevocationListener` has no production caller—there is no
    `RevocationListener::spawn` outside its definition. The OIDC role-change
    callback commits its advanced epoch without sending any invalidation. In
    addition, the listener accepts only `user`, `service`, and `agent`, while
    this candidate adds `tenant_admin` to the same service-account epoch path;
    a tenant-admin notification is discarded as an unknown kind. The focused
    same-second test proves database ordering only and never warms the production
    cache.
  - **Plausible exploit scenario:** An attacker spends a tenant administrator or
    user token once, warming that replica's epoch cache. An operator then revokes
    the credential/principal or the identity provider withdraws an administrative
    role. The attacker immediately repeats privileged requests against the same
    replica; it continues to accept the signed authority until the five-second
    cache entry expires. No listener is running to shorten that window, and a
    tenant-admin invalidation would be ignored even if one were started.
  - **Observable consequence:** Revoked or withdrawn authority remains
    exploitable after the revocation transaction has committed, directly
    contradicting the advertised next-request guarantee.
  - **Required testable correction:** Make the existing revocation resolver
    observe the committed epoch on every request. The smallest safe boundary is
    to stop memoizing epoch results—the resolver already opens a tenant
    transaction and reads tenant admission on every request—while retaining the
    verified-token cache. Do not rely on asynchronous `NOTIFY` to satisfy a
    next-request guarantee. Add a production-settings proof that first warms an
    absent/old epoch, commits a same-second role withdrawal and tenant-admin
    credential/principal revocation, and proves the very next verification is
    refused while the ordered successor is admitted.

### Medium

- **`SEC-WB06-2` — INCORRECT — additional stable no-effect administrative outcomes still roll back authorization decisions.**
  - **Violated obligation:** `REQ-037`, `AC-009`, and the canonical audit rule
    require every evaluated permission decision—allowed and denied—to be
    committed, including a stable authorized request that changes nothing.
  - **Exact location:**
    `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:361-383`;
    `crates/wyrd/wyrd-server/src/components/platform/identity.rs:442-462`;
    and `crates/wyrd/wyrd-server/src/components/principals/routes.rs:284-321,365-374,431-446`.
  - **Evidence:** `set_suspended` appends an allowed platform decision and drops
    its transaction when the tenant is missing, already in the requested state,
    or otherwise ineligible. `register_admin` authorizes first, then returns the
    stable missing-connection validation error through `?`, dropping the
    allowance. Tenant credential issue and list authorize before
    `require_principal`; a missing/foreign principal returns the stable
    non-enumerating 404 and rolls back the appended decision. Principal creation
    inserts its tentative row before discovering a requested role does not
    exist; rollback correctly removes that row but also erases the already-made
    allowance. These are served administrative paths and are not covered by the
    R5 no-effect test.
  - **Observable consequence:** Authorized probes, replays, and invalid
    administrative attempts return stable responses without durable evidence
    that permission was evaluated, leaving gaps in the security audit trail.
  - **Required testable correction:** Reuse the existing decision transaction.
    Commit it before each stable no-effect response. Where the transaction has
    already accumulated tentative effects (the nonexistent-role creation case),
    perform the existence checks before mutation within that same transaction,
    then commit only the decision on validation refusal; store/effect failures
    must continue to roll back both effect and decision. Add real-Postgres cases
    for wrong-state tenant suspension, registration without a platform
    connection, unknown/foreign principal issue and list, and an unknown role on
    create, each asserting one decision row and no resource mutation.

### Low / Defense In Depth

- None within the approved task boundary.

### Positive Controls

- Platform and tenant tokens use one canonical header but distinct verified
  claim shapes and extractor types; neither plane can satisfy the other's
  caller type.
- Platform credential and federated session confirmation re-read durable
  identity state, and platform grants are resolved on every request.
- Federated platform first-login pinning, stored-kind attribution, session
  issuance, canonical append, and commit now share one operator transaction.
- Human refresh tokens use one-way stored hashes, atomic consume-and-rotate,
  family revocation on replay, and credential-attributed canonical audit.
- OIDC discovery, token and JWKS requests use resolved-address screening and
  pinned connections; tenant identity remains derived from verified credentials
  under RLS.
- Raw credentials are redacted from `Debug`, omitted from audit, stored only as
  memory-hard verifiers where applicable, and returned only on creation paths.
- The shared client owns Wyrd authentication decoration and bounded renewal;
  the MCP adapter retains protocol framing without a second Wyrd credential
  policy.

## Result

**FAIL**

Two material, reachable security-domain findings remain. Both are bounded
implementation defects under the approved specification; neither requires a
specification revision.
