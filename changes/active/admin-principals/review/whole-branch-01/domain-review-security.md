# Security Domain Review

## Subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `072cf8b30c7135e8cf15f92da3e371a9c999703c`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 7
- Original tasks: `TASK-001` through `TASK-008` and their committed remediation packets
- Reviewed boundary: durable principal and credential state; tenant and platform token issue and verification; platform OIDC registration, login, and session validation; platform and tenant authorization; audit durability; RLS/operator separation; HTTP, CLI, MCP, and generated contract reachability; secrets and error projection.

The candidate remained at `072cf8b30c7135e8cf15f92da3e371a9c999703c` while this report was prepared.

## Authority and source coverage

| Boundary | Authorities | Source and consumer coverage | Result |
|---|---|---|---|
| Control-plane identity and RBAC | `AGENTS.md` §§2, 3, 9; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; spec REQ-012a–019, REQ-041–047, INV-001–004b, INV-011–015 | `wyrd-runtime::{principal,permission}`; `wyrd-auth::{platform_authz,platform_sessions}`; server platform extractor, identity and tenant routes; client/CLI/MCP callers; OpenAPI | **FAIL** — SEC-01 and SEC-02 |
| Credential lifecycle and secret handling | Security posture “Principal and credential lifecycle”, “Cryptography and secret handling”; spec REQ-005–011, REQ-020–023, REQ-032–033, INV-002, INV-005, INV-008–009, INV-012–013 | platform and tenant migrations; `platform_credentials`; `exchange_api_key`; initialization; principal routes; SQL credential queries; public routes and clients | **FAIL** — SEC-03 and SEC-06 |
| Audit integrity | `AGENTS.md` and `architecture/agent-rules.md` canonical single audit path and transactional-decision rules; security posture “Audit integrity and privacy”; spec REQ-037, INV-010–011, AC-009 | `platform_authz`; `platform.audit_authz` migration/query; platform identity, provisioning and recovery; tenant principal audit path | **FAIL** — SEC-04 |
| OIDC, callbacks, SSRF, and federation | Security posture “Delegation and federation” and “Source credentials and SSRF defense”; TASK-007 constraints; spec REQ-043–046, INV-004b, INV-011 | platform OIDC configuration, login and callback; shared OIDC provider/JWKS cache; existing screened discovery path; platform identity routes | **FAIL** — SEC-05 |
| Tenant and cross-plane isolation | `AGENTS.md` TenantConn/OperatorPool rules; security posture “Tenant and data isolation”; spec REQ-003–004, REQ-013–018, REQ-025–031, INV-003–007 | schema checks and privileges; TenantConn paths; OperatorPool paths; token claim shapes; both extractors; platform/tenant negative journeys | PASS for the inspected isolation mechanism; the separate Card-name/space handoff is noted below, not promoted into a security finding |

## Material findings

### SEC-01 — MISSING (High): registered human platform administrators receive no platform authority

- **Violated obligation:** REQ-041 requires human platform principals to be able to hold platform authority; REQ-046 requires an already-authorized platform principal to control creation and elevation; AC-015 requires the first human platform administrator to become an independently usable administrator. TASK-007 says platform authority is a grant held by both the root and human platform principals.
- **Location:** `crates/wyrd/wyrd-server/src/components/platform/identity.rs:333-389`; `crates/wyrd/wyrd-server/src/boot/init.rs:55-69,126-129`; `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs:101-126`; `crates/wyrd/wyrd-sql/src/queries/platform/principal_grants.rs:25-79`.
- **Evidence:** `register_admin` transactionally inserts only `platform.principals` and `platform.principal_identities`. The only production call to `set_platform_grant_tx` is deployment initialization. There is no served grant operation and no other production call to `set_platform_grant`. On every later request the extractor resolves an absent grant to an empty `PermissionSet`, so the federated session is valid but every protected platform operation is denied.
- **Plausible scenario and consequence:** the root configures OIDC and pre-registers the on-call operator. The operator completes login and receives a correctly signed platform session, but cannot create or inspect tenants, recover tenant administration, manage OIDC, or manage platform identities. The promised independent human administration path is nonfunctional, and loss of the shared root credential still requires database recovery.
- **Required testable correction:** in the already-authorized registration transaction, install the approved fixed platform-administration grant for the human principal through the existing `set_platform_grant_tx` mechanism. Reuse one authoritative platform grant definition rather than duplicating permission lists. Add a real-provider or equivalent verified-provider journey in which the registered human logs in and successfully performs a platform operation, plus a negative proof that a tenant-plane principal cannot create or grant a platform principal.

### SEC-02 — INCORRECT (High): the “last active principal” guard can lock the deployment out

- **Violated obligation:** REQ-033 preserves deployment-level recovery as the last resort, while TASK-007 explicitly requires revocation of platform administrators without silently destroying the served administrative path; INV-011 requires fail-closed outcomes. The implementation itself promises that suspension cannot leave “no way in.”
- **Location:** `crates/wyrd/wyrd-server/src/components/platform/identity.rs:471-544`; `crates/wyrd/wyrd-sql/src/queries/platform/principals.rs:185-231`; `crates/wyrd/wyrd-sql/migrations/20260601000023_platform_identity.sql:85-100`.
- **Evidence:** the guard counts every row with `status = 'active'`. It does not require a platform grant, a live credential, or a pinned federated subject, and the count and status update are separate statements without a transaction or lock. A newly pre-registered, unpinned, grantless human therefore makes the count exceed one; the root can then suspend itself. Concurrent suspension requests can also both observe a count above one and commit a zero-active outcome.
- **Plausible scenario and consequence:** immediately after pre-registering an operator—before their first successful login—the root suspends its own principal. The only remaining “active” row cannot authenticate as a pinned human and, under SEC-01, holds no authority. No served credential can administer the deployment. Two legitimate administrators can reach the same lockout concurrently even after grants exist.
- **Required testable correction:** make suspension and the recovery-path check one atomic database operation. Count only principals that actually preserve a usable platform administration path (current platform authority plus an authentication anchor), and serialize competing suspensions so two requests cannot both consume the final path. Prove (1) an unpinned or ungranted registration does not permit root suspension and (2) concurrent suspensions leave at least one independently usable platform administrator.

### SEC-03 — MISSING (High): platform credentials cannot be issued, listed, rotated, or revoked through any served surface

- **Violated obligation:** REQ-006–010 require uniform principal-generic credential lifecycle; REQ-036 requires the administrative HTTP/CLI/SDK/MCP projection; AC-005 requires overlapping rotation, independent revocation, metadata-only listing, and immediate invalidation. The objective explicitly includes credential rotation.
- **Location:** `crates/wyrd/wyrd-server/src/components/platform/routes.rs:37-53`; `crates/wyrd/wyrd-server/src/components/platform/identity.rs:59-87`; `crates/wyrd/wyrd-auth/src/platform_credentials.rs:137-211`; `crates/wyrd/wyrd-sql/src/queries/platform/credentials.rs:197-240`.
- **Evidence:** the platform HTTP routers expose tenant creation/recovery, OIDC/admin identity operations, and anonymous credential exchange only. SQL and domain methods for issuing/listing/revoking platform credentials exist, but production callers use only authentication; list and revoke are test-only, and `PlatformCredentials::issue` has no production caller. No shared-client, CLI, or MCP projection can reach a platform credential lifecycle operation.
- **Plausible scenario and consequence:** the initialization credential is copied into an unsafe environment or suspected compromised. The operator cannot issue B, verify B, revoke A, or list the root's credential metadata through Wyrd. The leaked credential continues minting privileged sessions until an operator performs out-of-band database surgery, turning ordinary rotation into an incident-recovery event.
- **Required testable correction:** expose the existing platform credential owner through separately authorized, transactionally audited HTTP operations for issue, metadata-only list, and revoke, then project them only through the already-required shared client and operator/agent surfaces. Preserve one-time plaintext return and next-request session invalidation. A journey must issue B, authenticate with B, revoke A, prove A's existing session is dead, prove B remains usable, and prove listing never returns either plaintext.

### SEC-04 — VIOLATION (High): platform authorization bypasses the canonical audit pipeline and commonly commits before the operation

- **Violated obligation:** `AGENTS.md` and `architecture/agent-rules.md` require one audit write path through `vala.audit_staging` and the `AuditPublisher`; every authorization decision must be appended in the transaction that performs or refuses the operation. REQ-037 and AC-009 require the same transaction, allowed and denied coverage, and retained attributable history.
- **Location:** `crates/wyrd/wyrd-sql/migrations/20260601000021_platform_authz_audit.sql:1-40`; `crates/wyrd/wyrd-sql/src/queries/platform/audit_authz.rs:1-66`; `crates/wyrd/wyrd-auth/src/platform_authz.rs:90-143`; `crates/wyrd/wyrd-server/src/components/platform/identity.rs:111-133,154-302,418-544`; `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:129-181`; `crates/wyrd/wyrd-server/src/components/platform/recovery.rs:74-123`.
- **Evidence:** the candidate creates `platform.audit_authz`, a second retained audit table that is never staged or published through the canonical pipeline. The generic identity `authorize` helper commits the allow before OIDC configuration/read/delete, listing, or status mutation. Provisioning commits the allow with only the directory claim and then performs tenant initialization separately. Recovery commits the allow before opening the tenant transaction. Failures can therefore leave durable “allow” records for operations that did not occur, while all platform decisions are absent from `vala.system.audit_log`.
- **Plausible scenario and consequence:** a platform administrator attempts recovery or suspension, the tenant/database step fails after the standalone allow commits, and incident responders later see an authorization allowance without the corresponding mutation. Conversely, anyone monitoring the canonical audit history sees no platform action at all. This breaks attribution and tamper-evident retained audit for the most privileged plane.
- **Required testable correction:** delete the alternate platform audit authority and route platform decisions through the repository's canonical staging/publisher owner. Couple each decision to the mutation transaction when they share a boundary. For deliberately cross-boundary workflows, preserve truthful per-boundary authorization semantics rather than recording a whole-operation allow before the second boundary; the approved specification must be revised if it truly requires impossible cross-database atomicity. Closure proof must query canonical audit history/staging, inject each post-authorization write failure, and show no false successful decision or unaudited committed mutation.

### SEC-05 — VIOLATION (High): platform login re-fetches OIDC URLs without the repository's SSRF screening or address pinning

- **Violated obligation:** the security posture requires parse/normalize, bounded DNS, blocked-address rejection, pinned connection, redirect re-screening, and IO bounds for every supplied URL fetch. TASK-007 requires reuse of existing SSRF screening. INV-011 requires uncertainty to fail closed.
- **Location:** `crates/wyrd/wyrd-server/src/components/platform/identity.rs:153-193`; `crates/wyrd/wyrd-server/src/components/admin/routes.rs:589-703`; `crates/wyrd/wyrd-auth/src/platform_login.rs:145-175,207-246`; `crates/wyrd/wyrd-auth/src/login.rs:145-163`; `crates/wyrd/wyrd-auth/src/callback.rs:216-234`; `crates/shared/wyrd-auth-oidc/src/provider.rs:109-151`; `crates/shared/wyrd-auth-verify/src/lib.rs:679-706`.
- **Evidence:** configuration performs one screened and DNS-pinned discovery through `discover_jwks_uri`. Every anonymous platform login then calls `discover_authorization_endpoint`, and every callback calls `discover_provider`; both build a fresh ordinary `reqwest::Client`. External verification subsequently fetches the stored JWKS URL through the generic cache/client. Those runtime fetches re-resolve DNS and are not shown to apply the deployment address policy or connection pinning. The comment claiming that the platform path reuses screening is therefore false after the initial configuration request.
- **Plausible scenario and consequence:** a once-valid configured issuer domain is compromised or DNS-rebound after registration. An unauthenticated caller triggers `/auth/platform/login` or `/auth/platform/callback`; Wyrd re-resolves the issuer to a cloud-metadata, loopback, or private address and performs the server-side request. This turns the anonymous login surface into an SSRF trigger capable of probing internal services or metadata endpoints.
- **Required testable correction:** reuse the repository's existing screened, bounded, redirect-disabled, DNS-pinned fetch policy for every platform discovery, token-endpoint, and JWKS request; the configuration-time screen alone is insufficient. Do not add a second URL policy. Add a test that changes resolution after configuration and proves anonymous login/callback/JWKS refresh rejects the newly blocked address without making an internal request.

### SEC-06 — INCORRECT (Medium): platform credential rejection is distinguishable by Argon2 timing

- **Violated obligation:** INV-012 requires invalid-credential cases to be publicly indistinguishable and resistant to timing/enumeration inference; AC-010 requires security evidence for that property.
- **Location:** `crates/wyrd/wyrd-auth/src/platform_credentials.rs:190-210`; `crates/wyrd/wyrd-server/src/components/platform/routes.rs:75-104`; `crates/wyrd/wyrd-auth/src/platform_credentials.rs:346-421`.
- **Evidence:** malformed and unknown prefixes return before any Argon2 work, while a known live prefix with the wrong tail runs memory-hard verification. The test proves only identical variants and strings, not work-factor equivalence. The anonymous `/auth/platform/token` endpoint exposes the timing difference directly.
- **Observable consequence:** an attacker who obtains candidate prefixes from operational metadata, screenshots, or partial leakage can remotely determine which still resolve to live rows. The 128-bit secret remains resistant to brute force, so this is an enumeration oracle rather than direct credential recovery, but it contradicts an explicit acceptance property.
- **Required testable correction:** run the same Argon2 verification work for malformed/unknown and known-invalid credentials using one process-owned dummy verifier, while keeping the existing public error unchanged. Add a focused check that instruments verifier invocation (not a flaky wall-clock threshold) and proves all invalid shapes take the verification path exactly once.

## Security audit summary

### Critical

- None confirmed.

### High

- SEC-01: human platform administrators authenticate but receive no authority.
- SEC-02: platform-principal suspension can remove the deployment's last usable administrative path.
- SEC-03: a compromised root credential has no supported rotation/revocation path.
- SEC-04: platform authorization uses an alternate, noncanonical and frequently non-atomic audit path.
- SEC-05: anonymous platform OIDC flows can drive unscreened, re-resolved outbound requests.

### Medium

- SEC-06: platform credential lookup exposes a measurable valid-prefix timing oracle.

### Low / defense in depth

- Several new public error mappings include raw SQL/store strings in problem details (for example `components/auth/platform_extractor.rs:101-120,165-169`, `components/platform/identity.rs:647-689`, and `components/principals/routes.rs:544-550`). No secret-bearing error was demonstrated, so this is not retained as a task finding. The remediation should nevertheless keep backend diagnostics in structured server logs and return stable empty/redacted public details when touching these mappings.

### Positive controls

- Platform and tenant tokens have structurally distinct claim shapes and platform verification requires the signed `platform` scope marker.
- Platform sessions re-read credential/principal status and current grants on every request, so platform credential or principal revocation does not depend on a stale token cache.
- Platform and tenant persistence use `OperatorPool` and `TenantConn` respectively; platform tables are not granted to `wyrd_app`.
- Credential plaintext uses `SecretString`, Argon2id verifiers, cryptographic randomness, one-time response projection, and redacted public metadata.
- Platform OIDC client secrets are sealed at rest and omitted from read/list responses.
- First-login identity pinning requires provider-asserted `email_verified`, pins `(issuer, subject)` once, and subsequent login resolves by subject.
- Canonical `X-Wyrd-Access-Token` handling and extractor types preserve the control-plane split; `Authorization` is not consumed by Wyrd's platform extractor.

## Explicitly assessed reported issues

- **Base-red `auth_e2e::cache_ttl_path_also_flips_verdict`: not a candidate security finding.** The test body is byte-for-byte unchanged across the reviewed range. The candidate's changes in the reported `DelegateError::Database(_)` mapping are non-behavioral for this failure, and inspection of the base shows the same database-to-`WYRD_AUTH_503_VERIFY_UNAVAILABLE` projection. The user reports—and separate base reproduction establishes—that the same failure occurs at `c5c20754...`; this review found no candidate change that worsens or depends on that specific failure. It remains a verification limit and means a claim that the entire repository is green is currently false until the baseline defect is repaired or the environment cause is removed.
- **`UNIQUE (data_tenant_id, name)` and Card refs without `space`: spec-owner handoff, not retained here.** The constraint currently prevents two same-named Card-bound principals in different spaces and is the external fact that bounds `service_account_by_card_ref`'s relaxed containment predicate to one row. Removing it without first making the predicate and durable Card identity space-aware would create credential misbinding risk. The requested same-name-across-space behavior therefore needs the identified specification/persistent-data decision; it is not safe review remediation to delete the unique key in isolation.
- **Stale `wyrd-testing/src/server.rs` comment:** no security effect; leave to the repository/task review.
- **Ungated `rustdoc::private_intra_doc_links` warning:** no security effect and no current CI security gate regression; widening the rustdoc lane is a separate repository decision.

## Verification limits

- This was a read-only static security audit. No source or tests were edited.
- Existing closeout evidence was considered only as coverage context; findings are based on candidate source and reachability, not prior reviewer conclusions.
- No live external IdP or DNS-rebinding environment was available, so SEC-05 is established from the concrete unscreened `reqwest::Client` call paths and the anonymous route reachability rather than a network exploit run.
- No timing benchmark was run for SEC-06; the divergent Argon2 call graph is deterministic source evidence, and the proposed closure proof avoids unreliable wall-clock assertions.
- The reported base-red auth journey prevents an “entire repository green” conclusion. The candidate-specific platform journey closeouts do not exercise a successful federated human administrator performing an authorized operation, global credential lifecycle, canonical retained platform audit, runtime DNS rebinding, or the suspension races identified above.

## Overall result

**FAIL**

The control-plane type split and secret-storage primitives are sound, but the candidate does not yet satisfy the approved administrative security model. Six material, bounded implementation findings remain: SEC-01 through SEC-06.
