# Authentication and security domain review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 13
- Original tasks: `TASK-001` through `TASK-008`
- Cumulative remediation: `whole-branch-08/TASK-001-008-R8-close-validated-findings.md`

The candidate remained at `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`
through this review.

## Reviewed boundary and authority coverage

| Boundary | Authority | Source and flow inspected | Result |
|---|---|---|---|
| Five tenant-token grants converge on one current-state issuer | `REQ-005`, `REQ-012`, `REQ-012a`, `REQ-012b`, `AC-018` | `wyrd-auth/src/{issuance,exchange_api_key,callback,refresh,jwt_bearer}.rs`; `TenantTokenIssuer::issue` reloads tenant, principal, roles and permissions for API key, OIDC, refresh, workload assertion and delegation | PASS |
| JWT authority and request-time verification | `REQ-012a`, `REQ-012b`, `REQ-017`, `INV-009`, `INV-013` | `wyrd-auth-issue::AccessGrant`; `wyrd-auth-verify::{TokenVerifier,VerifiedToken}`; tenant request context is built from signed `permissions`, with no auth-store read, epoch, checker or positive verifier cache | PASS |
| Delegated authority attenuation | `REQ-012c`, `INV-013a`, `AC-020` | `PermissionSet::intersection`; `DelegateToken::{execute,mint}`; `TenantGrant::Delegation`; target permissions are intersected with the verified caller permissions and the permission decision fails closed on an audit append failure | PASS |
| Delegation decision and credential attribution | `REQ-012c`, `REQ-037`, `INV-010`, `INV-013a`, `AC-009`, `AC-020` | `exchange_api_key.rs:235-348`; `issuance.rs:96-144,279-385,503-549`; failure decisions carry `verified.principal.credential_id`, but successful delegation discards it from both the exchange row and delegated token | FAIL (`AUTH-SEC-02`) |
| Platform current-state authentication and RBAC | `REQ-005`, `REQ-012a`, `REQ-017`, `REQ-018`, `INV-004a`, `INV-013` | `platform_extractor.rs`; `platform_sessions.rs`; each request verifies the local token, then re-reads the credential/principal anchor and current grant through `OperatorPool`; federated sessions re-read the principal; no cache or tenant-plane fallthrough found | PASS |
| Administrative and required auth CLI secret handling | `INV-002`, `REQ-036`, `REQ-043`, `REQ-047`, `REQ-048`, `AC-010`, R8-2 acceptance | All changed CLI auth/admin argument structs and dispatchers, including principal revoke, key issue, trusted issuer, workload binding, platform/tenant administration and refresh | FAIL (`AUTH-SEC-01`) |

## Security Audit

### Critical

None.

### High

- **`AUTH-SEC-01` — secret-valued CLI options remain reachable on in-scope administrative and session commands.**
  - **Classification:** `INCORRECT`.
  - **Violated obligation:** R8-2 requires tenant and platform CLI administration to accept no secret-valued option; `INV-002` and `AC-010` prohibit credential plaintext in diagnostic surfaces; `REQ-048` makes refresh-token continuation part of this change.
  - **Locations:** `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:16-42`, `crates/wyrd/wyrd-cli/src/principal/revoke.rs:27-42`, `crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs:31-57,83-113`, `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:25-79`, and `crates/wyrd/wyrd-cli/src/auth/refresh.rs:11-19`.
  - **Evidence and reachability:** all structs derive `Debug`; every listed access or refresh token remains a `String` accepted by a `#[arg(long, ...)]`, and trusted-issuer registration still accepts `--client-secret`; these are live subcommands dispatched by `auth/mod.rs` or `principal/mod.rs`, not dormant helpers. An environment fallback does not remove the long option. The R8 proof checks only the newly added platform and principal-credential argument structs, so it cannot detect these reachable siblings.
  - **Plausible exploit:** an operator follows the advertised `--token`, `--refresh-token`, or `--client-secret` interface; another same-host observer reads shell history or `/proc/<pid>/cmdline`, or an error/debug capture renders a `String` field, and replays the platform/tenant administrator token, refresh token, or OIDC client secret.
  - **Impact:** theft of an administrative bearer can perform every operation its principal holds; theft of a refresh token can mint a successor human session; theft of the provider client secret compromises the deployment's OIDC client.
  - **Required correction:** remove the secret-valued long options from the in-scope admin/auth commands; reuse the existing ambient `ClientConfig` chain for tenant access, the existing platform environment source for platform access, `WYRD_REFRESH_TOKEN` for refresh, and the already present file/environment sources for the issuer client secret. Carry resolved material as `SecretString`; add no credential-source abstraction or second client builder. Preserve the server URL and once-only printing of newly created credentials.
  - **Focused closure proof:** CLI parser tests must prove each removed flag is rejected without echoing its supplied value, parsed argument `Debug` contains no ambient secret, missing ambient material fails, and the real CLI journey still issues/revokes credentials, configures trusted issuers/workload bindings, and rotates a human refresh token through the existing shared client.

### Medium

- **`AUTH-SEC-02` — successful delegation drops the authenticating credential identifier.**
  - **Classification:** `INCORRECT`.
  - **Violated obligation:** `REQ-037` and `AC-009` require every authorization decision to name the credential that authenticated the request; `INV-013a` requires durable attribution, and `REQ-012c` permits the successful token-exchange row to serve as the allowed decision only when it satisfies that contract.
  - **Locations:** `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:292-313`; `crates/wyrd/wyrd-auth/src/issuance.rs:96-134,333-352,503-549`.
  - **Evidence and reachability:** denied and allowed-no-effect delegation use `record_decision(...).with_credential_id(verified.principal.credential_id)`, but `DelegateToken::mint` converts the verified caller to `TokenPrincipalRef`, which has no credential field; `TenantGrant::credential_id` explicitly returns `None` for delegation; the successful `exchange_audit_event` branch never adds a credential id; and the same `None` becomes the delegated JWT's `cid`. Therefore an API-key-authenticated caller produces an unattributed successful delegation row and every later decision made with the delegated token also lacks the originating credential. The successful-delegation fixture hard-codes `credential_id: None`, so its green assertion proves only count/outcome, not attribution.
  - **Observable consequence:** audit history can show which principal delegated and which target received authority, but cannot identify which of that principal's concurrently valid credentials authorized the successful exchange or later use, defeating credential-specific investigation and revocation analysis.
  - **Required correction:** preserve the verified caller's optional credential id through the existing `DelegateToken` to `TenantTokenIssuer` grant boundary, attach it to the one successful token-exchange decision, and carry it in the delegated token's existing `cid` claim so downstream decisions remain attributable. Do not put credential data in the RFC 8693 `act` chain, add another audit row, or change subject identity, permission attenuation, chain ordering/depth, TTL, refresh behavior, or Card scope.
  - **Focused closure proof:** delegate from a real API-key token with a known credential row id and assert (1) the single successful `delegation:issue` row names that id, (2) the verified delegated token retains that id, (3) an ordinary authorized request using the delegated token records the same id, and (4) denied and allowed-no-effect delegation still commit exactly one row with the id; retain the existing audit-failure/no-token proof.

### Low / Defense In Depth

None.

### Positive Controls

- Tenant access tokens are five-minute Ed25519 JWTs whose request authority is the signed `permissions` snapshot; request verification is synchronous and database-free.
- All five tenant issuance entries converge on one concrete `TenantTokenIssuer` that re-reads current tenant, principal and grant state before signing.
- Delegation permission intersection reuses the existing `PermissionSet` coverage semantics and correctly narrows wildcard, schema and object grants.
- Delegation denial, later refusal and audit-store failure paths commit exactly one decision or fail closed without returning a token.
- Platform requests verify plane scope and then re-read the current credential/principal anchor and current grants through `OperatorPool` on every request.
- Newly added platform and principal-credential CLI commands already demonstrate the intended ambient-secret pattern and can be reused by the remaining commands.

## Verification limits

- This was a read-only static review. I inspected the candidate source, complete base-to-candidate diff for the affected boundaries, approved specification, original task obligations and R8 evidence; I did not rerun the recorded lanes.
- The supplied R8 evidence is credible for attenuation, decision count, fail-closed audit, platform/tenant CLI additions and shared verification lanes, but its successful-delegation token fixture contains no credential id and its CLI parser proofs omit the command families listed in `AUTH-SEC-01`.
- Non-administrative `query` and `eval` token options were not promoted into findings because the R8-2 acceptance boundary is administrative/auth credential handling; they remain outside this review's remediation boundary.

## Overall result

**FAIL** — `AUTH-SEC-01` and `AUTH-SEC-02` are reachable, bounded corrections required by the approved task and security invariants.
