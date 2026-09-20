# Security and RBAC domain review

## Review subject

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `a9a7c9c1e8502ccf3befa74c4283c78e5d70c132`
- Scope: the complete cumulative candidate, including the approved specification,
  all eight task packets, both earlier whole-branch reviews, the R2 remediation
  packet, active architecture/security authorities, and live callers.
- Result: **FAIL**. Two security defects and one narrowed prior error-disclosure
  defect remain reachable through served routes.

I started from the immutable commits rather than the remediation claims. I traced
both control planes from credential or federated login through token/session
verification, typed caller construction, authorization, audit append, mutation,
commit, revocation and recovery. Per the review instruction, I ran no Cargo or
`mise` command and changed no product source.

## Security Audit

### Critical

None.

### High

- **`SEC-R3-1` — INCORRECT — refresh-token replay rolls back the family
  revocation and its security audit.**

  **Contract.** The active security authority requires replay of a rotated
  refresh token to revoke its token family and emit a security audit event
  (`architecture/wyrd-security-posture.md:124-126`). The branch deliberately
  extends this contract to the newly usable `tenant_admin` refresh path.

  **Exact evidence and reachable exploit.** On a stale token,
  `crates/wyrd/wyrd-auth/src/refresh.rs:179-218` updates the family through
  `revoke_refresh_family`, appends `RefreshFamilyRevocation` on the same
  `TenantConn`, and then returns `Err(RefreshError::Reused)`. The served
  `/auth/token` handler at
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:209-243` commits only
  the `Ok` branch; every error returns at lines 228-241 and drops the transaction.
  SQLx therefore rolls back both the revocation and the specific audit event.
  An attacker who steals a tenant administrator refresh token can rotate it first
  and keep the successor. When the legitimate holder later presents the stolen
  original, the server reports replay but rolls back the theft response, leaving
  the attacker's successor valid for further privileged rotations. The same flaw
  affects other refresh-capable principal kinds, but this candidate newly makes
  the path reachable for the tenant administrative credential it provisions.

  The tests do not disprove this path. Unit tests at
  `wyrd-auth/src/refresh.rs:514-624` inspect revocation through the still-open
  transaction and never commit the `Reused` result. The new journey at
  `wyrd-server/tests/platform_admin_e2e.rs:3825-3837` asserts only that replay
  receives a client error; it neither tries the successor refresh token after
  replay nor reads committed audit state.

  **Smallest correction.** Make reuse a typed terminal outcome whose staged
  family revocation and canonical audit are committed before rendering
  `WYRD_AUTH_401_REFRESH_REUSED`; other failures must still roll back. Prove it
  through the real route: rotate a tenant-admin token, replay the original, then
  assert the successor refresh token is refused and the family-revocation audit
  is durable from a separate transaction.

### Medium

- **`SEC-R3-2` — INCORRECT — platform OIDC stores a noncanonical issuer that
  cannot match its own login verifier.**

  **Contract.** `IssuerUrl::new` explicitly makes issuer equality canonical by
  removing trailing slashes (`crates/wyrd-spec/src/auth/oidc.rs:83-105`), and
  REQ-043/REQ-044 require the configured platform connection and pre-registered
  identity to form a working human-administration path.

  **Exact evidence and reachability.** `configure_connection` parses and
  normalizes the supplied issuer at
  `crates/wyrd/wyrd-server/src/components/platform/identity.rs:206-215`, but
  persists and returns the original `request.issuer_url` at lines 229-246.
  `register_admin` then copies that raw stored value into the identity row at
  lines 388-414. At login, `platform_connection_from_row` reparses the connection
  through `IssuerUrl::new` (`crates/wyrd/wyrd-auth/src/pg_resolvers.rs:460-480`),
  and `PlatformLogin::resolve_principal` queries and pins identities using the
  normalized value (`crates/wyrd/wyrd-auth/src/platform_login.rs:277-304`). Thus
  the accepted and common configuration `https://idp.example/` registers the
  administrator under the slash form but searches under
  `https://idp.example`; first login is always refused as an unknown identity.
  This is reachable through `/platform/oidc/connection`, `/platform/admins`, and
  `/auth/platform/login` without database tampering and can leave OIDC
  administration unavailable until a credentialed platform administrator
  repairs the configuration.

  **Smallest correction.** Persist and return `issuer.as_str()` from the parsed
  `IssuerUrl`; use that single canonical value for connection and identity
  writes. Add a served journey that configures a trailing-slash issuer,
  pre-registers an administrator, and completes first-login pinning.

- **`FIND-admin-principals-3` / `SEC-R3-3` — VIOLATION — conflict responses
  still disclose internal database constraint names.**

  **Exact evidence and reachable path.** The remediation correctly routed many
  database/provider causes through `internal_failure`, but
  `crates/wyrd/wyrd-server/src/components/admin/routes.rs:699-705` and
  `:721-734` interpolate SQLx's `constraint` string into both the public message
  and JSON details. Authenticated tenant administrators can deliberately create
  the same trusted issuer or workload binding twice, or delete a referenced
  issuer without cascade, and receive internal PostgreSQL schema identifiers
  from the served `/v1/admin/*` routes. These names are implementation details,
  aid schema reconnaissance, and make the supposedly stable problem contract
  change when a constraint is renamed. The R2 verification claim covered raw
  backend failures but did not close these explicit source-derived strings.

  **Smallest correction.** Preserve the useful `409` classifications while
  returning operation-specific static messages/details; record the constraint
  only in structured server-side diagnostics. Add route tests for duplicate and
  referenced-delete conflicts that assert the response contains no injected or
  physical constraint name.

### Low / Defense In Depth

None beyond the open question below.

### Positive Controls

- Platform and tenant routes use distinct typed extractors; platform JWTs carry
  platform scope, tenant JWTs carry an RLS tenant, and neither token satisfies
  the other plane's extractor.
- Platform sessions preserve the stored principal kind and originating
  non-secret credential id. Credential-backed tenant tokens similarly propagate
  `cid`; federated and refresh-derived sessions intentionally use `None`.
- Same-plane platform authorization returns the audited `TenantConn`, and the
  inspected identity, credential and tenant-status mutations now commit their
  effect with the allowed decision. Tenant issuer, binding and principal-revoke
  mutations use the same coupling. Denials are independently durable and audit
  failure denies the action.
- Tenant admission is read before the principal-epoch cache on every request, so
  suspension and resume are not delayed by the cache TTL.
- Both API-key planes now perform exactly one real or dummy Argon2 verification
  for malformed, missing, unusable and wrong-secret inputs and return one public
  credential-refusal shape.
- Platform OIDC discovery, token and JWKS traffic uses screened resolution,
  address pinning, disabled redirects and bounded I/O. First login requires a
  verified email, nonce validation and one-time pinning; unknown subjects are
  denied without JIT provisioning.
- Secret-bearing structures use `SecretString`/redacted wrappers, handlers skip
  request bodies in tracing, client secrets are sealed before persistence, and
  list/read views omit plaintext verifiers.
- The last-administrator query counts only active granted principals with a
  usable credential or a pinned identity against the current connection, under
  an advisory transaction lock.
- Provisioning retry reuses the administrative principal and retires abandoned
  credentials in the tenant transaction before returning one new plaintext.
  Operator-only root recovery creates no principal or grant and is not served
  over HTTP.
- New manifest entries reuse workspace dependencies already present in the
  lockfile; this branch introduces no new third-party package or unpinned source.

## Open Question (not a finding)

- `PlatformLoginRequest.redirect_uri` is accepted from the anonymous caller and
  stored/forwarded without a server-side allowlist
  (`components/platform/identity.rs:606-615` and
  `wyrd-auth/src/platform_login.rs:150-183`). A correctly configured OIDC
  provider should enforce exact registered redirects, so the reviewed source is
  insufficient to establish an exploitable redirect. Confirm deployment docs
  require exact provider registration; if wildcard redirects are supported,
  this becomes a session-delivery risk and the server should derive or allowlist
  callback URIs.

## Authority and boundary coverage

| Boundary | Source/caller path traced | Result |
|---|---|---|
| Platform and tenant authentication separation | token extraction, JWT verification, session confirmation, typed callers, router handlers | PASS |
| RBAC and stored principal kinds | grants/roles, platform and tenant contexts, decision-event construction | PASS |
| Credential attribution | API-key row -> `cid` -> context -> canonical event/projection | PASS for live fresh-state propagation; retained audit evolution is covered by the data-domain review |
| Canonical audit atomicity | `PlatformAuthorization`, `audit::append_on`, identity/credential/status/admin/revoke callers | PASS for inspected authorization decisions and mutations; **FAIL for refresh replay security effect (`SEC-R3-1`)** |
| Tenant API-key timing/error equality | parsing, tenant admission, row lookup, shared dummy verifier, public mapping | PASS |
| Platform API-key timing/error equality | prefix parsing, usable-row filtering, shared dummy verifier, session issue | PASS |
| Refresh issue, rotation and replay | API-key exchange, card-free tenant-admin issue, rotate, HTTP commit/error path | **FAIL — `SEC-R3-1`** |
| Tenant suspension freshness | per-request admission read outside epoch cache, suspended/resumed route path | PASS |
| Last-admin safety | advisory lock, usable-admin SQL, status route | PASS |
| Platform and tenant OIDC | configuration, screened discovery, secret sealing, login/callback, identity pinning | **FAIL — `SEC-R3-2`** |
| Provisioning and recovery | claim/resume workflow, tenant credential cleanup, root/tenant recovery callers | PASS |
| Public error leakage | route mappers, stable problem rendering, internal diagnostics | **FAIL — `SEC-R3-3`** |
| Injection and unsafe input | parameterized SQL/query helpers, typed URL/UUID/card inputs, no changed command/template/deserialization sink | PASS in reviewed paths |
| Secrets and telemetry | secret wrappers, trace skips, storage representations, response projections, CLI one-time output | PASS |
| Supply chain | changed manifests and lockfile entries | PASS; only already-workspace dependencies were added to crate cones |

## Prior-finding disposition

| Prior finding | Disposition from current source |
|---|---|
| `FIND-admin-principals-1` | **CLOSED for live authorization paths.** One canonical staging path remains and inspected same-plane allowance/effect writes share a transaction. |
| `FIND-admin-principals-2` | **Security concern closed; data-domain qualification applies.** Exported query APIs no longer expose raw SQLx transactions. The separate retained-`WyrdPostgres` rule dispute is documented by the data reviewer. |
| `FIND-admin-principals-3` | **OPEN / narrowed as `SEC-R3-3`.** Generic backend causes are redacted, but explicit constraint names remain public. |
| `FIND-admin-principals-4` | **CLOSED.** Design and security authorities now describe both planes, the closed kind set, card-free administrators and credential attribution consistently. |
| `FIND-admin-principals-8` | **CLOSED.** The guard requires a usable credential or pinned identity on the current connection and serializes concurrent status changes. |
| `FIND-admin-principals-13` | **No security finding.** Served operations are registered in the OpenAPI owner; contract fidelity remains the contract reviewer's boundary. |
| `FIND-004-3` | **CLOSED.** Retry retires any undisclosed credential and issues one replacement on the reused principal. |
| `FIND-005-1` | **CLOSED.** Tenant admin mutations and allowed audit events share one `TenantConn`. |
| `FIND-003-2` | **CLOSED.** The journey drives the shipped initialization process and one-time output contract. |
| `FIND-004-5` | **CLOSED.** `recover-root` is an operator-only process path over the existing root. |
| `FIND-TASK-001-10` | **WAIVED by the owner.** Co-author/provenance concerns are not findings in this review. |
| `FIND-admin-principals-R2-2` | **CLOSED.** Stored platform kind reaches the session, context and decision event. |
| `FIND-admin-principals-R2-3` | **CLOSED for live security attribution.** Optional credential id reaches staging/projection without secret material; upgrade durability is separately assessed by the data reviewer. |
| `FIND-admin-principals-R2-4` | **OPEN / revised as `SEC-R3-1`.** TenantAdmin rotates successfully, but its required replay containment is rolled back. |
| `FIND-admin-principals-R2-5` | **CLOSED.** Admission is no longer hidden behind the principal cache. |
| `FIND-admin-principals-R2-6` | **CLOSED.** Tenant invalid-key paths perform one shared real/dummy verification. |

The owner-approved verified-change-contract work is accepted as cumulative scope
and is not a finding. The base-reproduced auth red, Card-name/spec-owner handoff,
stale testing comment, and rustdoc-lane decision do not change this security
verdict.

## Verification limits

This is a static audit. I did not execute Cargo or `mise`, per instruction. I
inspected the committed verification code and prior command claims but did not
treat them as proof where their assertions stop before a transaction boundary.
In particular, the refresh unit tests observe uncommitted state and the served
TenantAdmin journey proves only the replay response, which is why they cannot
detect `SEC-R3-1`. No current platform OIDC journey uses a trailing-slash issuer,
and the admin conflict tests do not establish that physical constraint names are
absent from served bodies.

## Overall result

**FAIL**

Proposed validation IDs: `SEC-R3-1`, `SEC-R3-2`, and the stable
`FIND-admin-principals-3` for `SEC-R3-3`.
