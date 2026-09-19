# Security Domain Review — whole-branch-02

## Review subject

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Scope: the complete cumulative diff, with the approved `admin-principals`
  specification, repository authorities, previous whole-branch review, and
  remediation packet treated as claims to revalidate from source.
- Verdict: **FIX_REQUIRED**. Four prior findings remain open and five additional
  security/domain findings are present.

I traced platform and tenant authentication, authorization, credential issue and
revocation, principal lifecycle, tenant suspension, OIDC discovery, audit staging,
error rendering, and the relevant HTTP callers. I did not run Cargo or mise tests,
as directed. The candidate commit was confirmed before review; no source files were
modified.

## Security Audit

### Critical

None.

### High

- **`FIND-admin-principals-8` — INCORRECT** —
  [`crates/wyrd/wyrd-sql/src/queries/platform/principals.rs:267`](../../../../../crates/wyrd/wyrd-sql/src/queries/platform/principals.rs#L267)
  still treats any `platform.principal_identities` row as an authentication path.
  It does not require a pinned `subject`, nor require the platform OIDC connection
  to exist. The regression test compounds the mistake: its helper inserts an
  awaiting-first-login identity at
  [`pg_platform_identity.rs:52`](../../../../../crates/wyrd/wyrd-sql/tests/pg_platform_identity.rs#L52),
  then calls it "pinned" at line 329 without ever pinning it. An operator can
  register a human and then suspend the root credential principal before that
  human's first login; alternatively, the connection can be removed before the
  suspension. The guard counts the unusable identity and permits the last real
  administrator to be suspended, stranding the deployment. This fails the
  last-usable-administrator lifecycle requirement and invalidates the associated
  acceptance evidence. **Smallest correction:** count an identity only when its
  subject is pinned and the matching deployment OIDC connection is live; add
  transaction/concurrency tests for awaiting-first-login and removed-connection
  identities.

- **`FIND-admin-principals-R2-5` — INCORRECT** —
  [`crates/wyrd/wyrd-auth/src/revocation_resolver.rs:106`](../../../../../crates/wyrd/wyrd-auth/src/revocation_resolver.rs#L106)
  evaluates tenant admission inside a principal-epoch cache entry whose production
  TTL is five seconds (lines 20 and 56–63). If a tenant request primes an allowed
  entry immediately before platform suspension, subsequent live-token requests use
  the cached epoch without re-reading tenant status. A malicious or compromised
  tenant can therefore continue authorized writes during the suspension window,
  contrary to REQ-028 and AC-008's requirement that a suspended tenant refuse
  authorization. Existing zero-TTL in-process coverage cannot prove this path.
  **Smallest correction:** perform the tenant-admission read outside the cached
  per-principal epoch lookup (or add a tenant-wide invalidation mechanism) and add a
  nonzero-TTL test that primes the cache, suspends the tenant, and proves the next
  request is denied.

### Medium

- **`FIND-admin-principals-1` — VIOLATION** — the canonical audit table is now
  reused, but same-plane platform mutations are still not atomic with their allowed
  decision. `PlatformAuthorization::authorize` intentionally returns the open
  audited transaction at
  [`platform_authz.rs:105`](../../../../../crates/wyrd/wyrd-auth/src/platform_authz.rs#L105),
  yet the shared helper commits it immediately at
  [`identity.rs:114`](../../../../../crates/wyrd/wyrd-server/src/components/platform/identity.rs#L114).
  Connection configuration/removal and status changes then mutate through the pool
  afterward (lines 157–209, 291–296, and 494–510); platform credential issue and
  revocation do likewise at
  [`credentials.rs:79`](../../../../../crates/wyrd/wyrd-server/src/components/platform/credentials.rs#L79),
  and tenant suspension commits the decision before the state transition at
  [`provisioning.rs:287`](../../../../../crates/wyrd/wyrd-server/src/components/platform/provisioning.rs#L287).
  A failed or conflicting effect leaves a durable Allowed row for an operation that
  never happened, so an authorized attacker can deliberately populate misleading
  forensic history with invalid mutations. Registration is the correct local
  pattern: it performs the effect on `decision.transaction()` and commits once.
  This still fails REQ-037/AC-009 and the prior remediation acceptance. **Smallest
  correction:** replace the commit-and-discard helper with one returning the
  audited `TenantConn`, execute every same-plane SQL effect on that transaction, and
  commit once. Keep separately justified cross-plane workflows explicit.

- **`FIND-admin-principals-R2-1` — VIOLATION** — tenant administrative mutations
  deliberately commit audit separately from their effects. The module contract at
  [`components/admin/routes.rs:15`](../../../../../crates/wyrd/wyrd-server/src/components/admin/routes.rs#L15)
  says so, and create/delete issuer and binding routes call `record_audit` before
  opening or committing their mutation transaction (lines 230–255, 320–355,
  369–404, and 451–477). Principal revocation repeats this at
  [`auth/revoke.rs:70`](../../../../../crates/wyrd/wyrd-server/src/auth/revoke.rs#L70).
  Conflicts, not-found requests, discovery failures, or database failures therefore
  leave Allowed audit records for effects that did not occur. This is the tenant
  counterpart of the still-open platform atomicity defect and violates REQ-037,
  AC-009, and the required architecture amendment that superseded the old
  standalone-audit stance. **Smallest correction:** append the allowed event using
  the same `TenantConn` that performs the mutation and commit them together; denied
  decisions remain independently durable because no effect is attempted.

- **`FIND-admin-principals-3` — VIOLATION** — task-touched public error paths still
  expose internal dependency text. Principal revocation maps arbitrary database
  errors into `WyrdError::Internal.message` at
  [`auth/revoke.rs:126`](../../../../../crates/wyrd/wyrd-server/src/auth/revoke.rs#L126)
  and [`wyrd-auth/src/revoke.rs:71`](../../../../../crates/wyrd/wyrd-auth/src/revoke.rs#L71).
  Admin encryption/serialization failures do the same at
  [`components/admin/routes.rs:667`](../../../../../crates/wyrd/wyrd-server/src/components/admin/routes.rs#L667).
  `WyrdErrorResponse` serializes that message into the problem body at
  [`http/error.rs:88`](../../../../../crates/wyrd/wyrd-server/src/http/error.rs#L88),
  disclosing SQL, schema, crypto-provider, or serialization details to an
  authenticated caller. **Smallest correction:** route these causes through the
  existing `internal_failure` helper (log the cause server-side, return a static
  operation message) and assert served bodies exclude injected source text.

- **`FIND-admin-principals-R2-2` — INCORRECT** — the initial card-free tenant-admin
  exchange mints a refresh token, but refresh rotation rejects that principal. The
  card-free issuer stores the refresh row at
  [`exchange_api_key.rs:394`](../../../../../crates/wyrd/wyrd-auth/src/exchange_api_key.rs#L394),
  while `RefreshTokenExchange::execute` accepts only `service`, `agent`, and an
  unfinished `user` branch at
  [`refresh.rs:124`](../../../../../crates/wyrd/wyrd-auth/src/refresh.rs#L124); a
  `tenant_admin` row becomes `InvalidPrincipalKind`. Routing it through the service
  branch would still fail because that path requires `card_ref` at lines 244–262.
  Thus the provisioned administrator receives a credential-derived session that
  cannot be renewed through the advertised refresh flow. **Smallest correction:**
  reuse the existing card-free access-token issuance in a rotation path that writes
  the successor's `rotated_from`, and add a real tenant-admin exchange → refresh →
  authenticated-request journey.

- **`FIND-admin-principals-R2-3` — VIOLATION** — tenant API-key verification does
  not perform the required exactly-one memory-hard verification for invalid
  conditions. At
  [`exchange_api_key.rs:232`](../../../../../crates/wyrd/wyrd-auth/src/exchange_api_key.rs#L232),
  malformed, cross-tenant, suspended-tenant, unknown-prefix, and disabled-account
  paths all return before Argon2; only a found active row reaches `verify_api_key`
  at lines 253–258. An anonymous `/auth/token` caller can statistically distinguish
  active prefixes from unknown or disabled credentials by the Argon2 cost, enabling
  credential/account enumeration. This violates INV-012, AC-010, and the explicit
  security-posture rule that every invalid condition performs one verification.
  **Smallest correction:** apply the already-implemented platform dummy-verifier
  pattern to tenant exchange so every terminal path performs exactly one real or
  dummy verification; test invocation counts per condition, not wall-clock timing.

- **`FIND-admin-principals-R2-4` — MISSING** — authorization audit cannot record
  the credential that authenticated the request. `PlatformCaller` carries
  `credential_id` at
  [`platform_extractor.rs:44`](../../../../../crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs#L44),
  but `PlatformAuthorization::authorize` accepts only `AuthContext`, and
  [`AuditEvent`](../../../../../crates/wyrd-spec/src/vala/api.rs#L2708) has no
  credential field. Tenant callers likewise do not propagate an originating
  credential identifier through the token/authenticated context. Consequently no
  privileged decision can satisfy REQ-037/AC-009's explicit "which credential"
  attribution, weakening incident response when one of several credentials is
  compromised. **Smallest correction:** add an optional non-secret credential ID
  to the authenticated/audit context and canonical `AuditEvent`, propagate the
  platform session `cid` and tenant credential row ID through issuance and
  verification, use `None` for federated identity, regenerate contracts, and test
  both planes while proving no secret enters audit.

### Low / Defense In Depth

- **`FIND-admin-principals-4` — MISSING** — the required security authority
  amendment is incomplete. [`architecture/wyrd-security-posture.md:40`](../../../../../architecture/wyrd-security-posture.md#L40)
  still defines the closed principal set as only `User`, `Service`, `Agent`, and
  `System` and retains the old Card-binding model, while the approved specification
  and `wyrd-design.md` now include platform and tenant administrators. This leaves
  implementors with contradictory security authority for scope and lifecycle.
  **Smallest correction:** update that section to the approved closed set, tenancy,
  card-free administrator, credential, and control-plane rules; do not create a
  parallel model.

### Positive Controls

- The alternate platform audit table/write path was removed; platform decisions
  now append to canonical `vala.audit_staging` and fail closed when append fails.
- Platform administrator registration now creates the fixed grant in the same
  transaction as the principal, identity, and allowed audit decision.
- Platform invalid-credential handling uses a process-wide dummy verifier and one
  verification per terminal path; `FIND-admin-principals-11` is closed for the
  platform plane.
- OIDC discovery/JWKS/token requests use screened resolution, address pinning,
  redirects disabled, and bounded timeouts; the prior SSRF finding is closed.
- Recovery checks tenant lifecycle before issuing a replacement credential, and
  the served platform credential issue/list/revoke lifecycle is now present.
- Tenant-scoped principal and credential CRUD generally acquires `TenantConn` from
  the verified caller tenant and relies on RLS rather than caller-controlled tenant
  predicates.

## Prior-finding disposition

| Finding | Disposition | Source-based result |
|---|---|---|
| `FIND-admin-principals-1` | Open | Canonical path fixed; same-plane allowed decision/effect atomicity remains broken. |
| `FIND-admin-principals-3` | Open | Several live admin/revocation paths still serialize raw internal causes. |
| `FIND-admin-principals-4` | Open | Security-posture principal model remains stale. |
| `FIND-admin-principals-7` | Closed | Registration installs the administrator grant transactionally. |
| `FIND-admin-principals-8` | Open | Unpinned/orphaned identities are still counted as usable administrators. |
| `FIND-admin-principals-9` | Closed | Platform credential lifecycle has served routes and shared-client/CLI projection. |
| `FIND-admin-principals-10` | Closed | Screened and pinned outbound OIDC transport is used across discovery and runtime fetches. |
| `FIND-admin-principals-11` | Closed for platform; tenant defect recorded as `R2-3` | Platform performs dummy work; tenant exchange still has early-return timing classes. |

## Acceptance impact

The candidate cannot satisfy AC-005, AC-008, AC-009, AC-010, or AC-013 in its
current form. The failures are reachable through shipped HTTP/authentication paths,
not merely unused helpers. A remediation packet should consolidate audit atomicity
across both planes, then independently repair last-admin usability, tenant-admin
refresh, suspension freshness, tenant verifier equalization, credential audit
attribution, internal error redaction, and the required authority amendment.
