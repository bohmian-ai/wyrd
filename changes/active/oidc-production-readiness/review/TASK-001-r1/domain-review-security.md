# Security Domain Review

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `e126cdca7d4bf5bc467279df05cc3e199eb7fdf2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`

The candidate remained at the stated commit while this review was performed.

## Reviewed boundary

This review traced the tenant OIDC administration trust boundary from verified bearer extraction through `identity_connections:write`, tenant-scoped persistence, recovery-key verification, canonical audit, provider discovery and probes, and the Active-connection login/callback consumers. It also inspected secret sealing/rewrap and redacted public projections.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Bearer-derived tenancy and tenant isolation | `AGENTS.md` §§2, 9; `architecture/agent-rules.md` (`TenantConn`/RLS); security posture, tenant isolation | `components/auth/caller_extractor.rs`; `components/admin/identity.rs`; `connections.rs`; `human_connections.rs`; migration RLS/indexes; `tenant_connection_admin_journey` | PASS |
| Dedicated administration permission | Task packet-local contract; security posture authorization rules | `Permission::identity_connections_write`; built-in roles; `decide`; route middleware; runtime-admin denial journey | PASS |
| Recovery authority | Task packet-local activation contract | `connections.rs:341-378,741-775`; `api_key_by_prefix`; service-account role resolution; rotation journey invalid/valid recovery keys | PASS |
| Canonical allowed/denied audit and fail-closed mutation | REQ-017; `AGENTS.md` §2; `architecture/agent-rules.md` audit rules | `audit/mod.rs`; `identity.rs:55-94,231-250`; `connections.rs:141-159,219-433,476-492,778-785`; audit-failure rollback and retained-audit journey assertions | PASS |
| Authorization before provider IO | Task packet-local contract; security posture receiving-boundary rule | `identity.rs:231-250`; `connections.rs:287-316`; unauthorized route journey | PASS |
| Discovery, issuer and client-auth qualification | REQ-004; INV-004; task Scenario 2 | `connections.rs:541-659`; `provider.rs:93-157`; wrong-secret journey | **FAIL** (SEC-002) |
| Resolved-address SSRF screening and pinning | INV-004; `architecture/agent-rules.md`; security posture, Source credentials and SSRF defense | `wyrd-auth-oidc/screening.rs:79-128`; all `connections.rs` provider call sites; callback/login/JWKS consumers; unsafe metadata-address journey | **FAIL** (SEC-001) |
| Active issuer and callback consumption | REQ-004; task consumer closure | `connections.rs:436-467,705-737`; `callback.rs`; server login/callback adapters | PASS for Active issuer selection; see verification limit for runtime redirect ownership |
| Secret redaction and sealing-key rotation | REQ-005; security posture secret rules | `human_connection.rs`; `connections.rs` tracing/error/view paths; `sealing.rs`; `SealingKeyring`; OpenAPI/redaction/rewrap journey assertions | PASS |

## Material findings

### SEC-001 — VIOLATION: screened clients can bypass address pinning through ambient proxy configuration

- **Violated obligation:** INV-004, the task's required screened provider IO, `architecture/agent-rules.md` (resolve once, validate, act on that exact result), and the security posture requirement to connect to the screened address without re-resolution.
- **Location:** `crates/shared/wyrd-auth-oidc/src/screening.rs:97-124`; reachable from `crates/wyrd/wyrd-auth/src/connections.rs:543-570,591-592,621-639,662-673`.
- **Evidence:** `ScreenedHttp::client_for` resolves and passes screened addresses to `reqwest::ClientBuilder::resolve_to_addrs`, but the builder does not call `no_proxy()`. The pinned reqwest version enables the system proxy automatically unless it is explicitly disabled (`reqwest 0.12.28` `ClientBuilder`: system proxies are added by default; `no_proxy` disables them). When `HTTP_PROXY`/`HTTPS_PROXY` is present, the client may connect to that proxy and give it the original hostname, so the proxy performs the effective destination resolution rather than using the screened `resolve_to_addrs` result.
- **Reachable scenario and consequence:** In a hosted deployment with an ambient egress proxy, a tenant administrator supplies an issuer or discovered endpoint whose address is public under Wyrd's resolver but resolves or routes to an internal target from the proxy. Candidate testing, later discovery, token exchange, or JWKS refresh can then reach a destination Wyrd never screened. This defeats the required DNS-rebinding/SSRF pin, exposing internal HTTPS services where the proxy can establish or terminate the connection.
- **Required testable correction:** In the shared `ScreenedHttp` owner, disable ambient/system proxies on every screened client before applying the pinned addresses. Do not add a separate connection-specific client. Add a focused controlled-proxy check that sets an ambient proxy, requests a screened hostname, and proves the proxy receives no request while the screened pinned endpoint does; keep the existing blocked-range and rebinding checks.

### SEC-002 — INCORRECT: the token-endpoint probe treats almost any non-authentication response as proof of valid client authentication

- **Violated obligation:** REQ-004 and the task contract that testing validates the configured client authentication before stamping the exact revision activatable.
- **Location:** `crates/wyrd/wyrd-auth/src/connections.rs:606-659`.
- **Evidence:** The probe submits a deliberately invalid authorization code, but rejects only HTTP `401`, `invalid_client`, or `unauthorized_client`. It accepts HTTP success, malformed/non-JSON responses, and every other OAuth error (including `invalid_request`, `unsupported_grant_type`, or `temporarily_unavailable`) as successful client authentication. The method's own contract says a grant error proves authentication, but the implementation never requires `invalid_grant`.
- **Reachable scenario and consequence:** A configured provider or intermediary returns `400 invalid_request`, a non-JSON `403`, or even `200` without processing client credentials. Wyrd stamps the candidate tested and permits activation although neither the secret nor the selected client-auth method was proven. The old provider can then be retired into a connection that cannot complete login, violating the required tested recovery transition and causing tenant login lockout until headless recovery is used.
- **Required testable correction:** Treat only the expected OAuth `invalid_grant` response to the deliberately invalid code as proof that request parsing and client authentication succeeded; fail closed for success, undecodable bodies, and every other status/error. Add one focused provider-probe test showing `invalid_grant` passes and `invalid_request`, empty/malformed, and `200` responses do not stamp the candidate. Keep the existing real-provider wrong-secret journey.

## Positive controls

- Every administration route derives the tenant from the verified `Caller`; no route accepts a tenant identifier, and all storage work enters an RLS `TenantConn`.
- The new permission is distinct from `service_accounts:write`; only the built-in tenant `admin` wildcard covers it by default.
- Recovery-key parsing binds the embedded tenant, performs one password-hash verification on failure, requires an unexpired/unrevoked key joined to an active service-account principal, and resolves current roles inside the activation transaction.
- Mutation allows and their effects share one tenant transaction; denials and pre-transaction refusals are committed separately, and an audit append failure fails closed.
- Provider secrets use redacted boundary types, sealed storage, non-secret key identifiers, and compare-and-swap rewrap. Public views and audit events contain no ciphertext or plaintext secret.
- Discovery validates the returned issuer exactly; each discovered provider URL is independently screened; redirects are disabled; blocked answers are rejected if any resolved address is unsafe.

## Verification limits

- No verification command was rerun during this time-bounded static domain review. The task records passing focused journeys and repository gates, and their source assertions were inspected; this report does not independently attest to those executions.
- The administration test uses the configured public-origin callback, but the current login adapter still constructs its runtime redirect URI from `Host`/`X-Forwarded-Proto`, and the callback adapter resolves tenant context from `Host`. The task's implementation evidence explicitly assigns login redirect-URI and renewal changes to TASK-002, so this review does not convert that adjacent pre-existing behavior into a TASK-001 finding. Consequently, TASK-001's provider test proves the configured callback registration value, not the later end-to-end runtime callback-origin/state contract.
- Controlled Okta and Entra production qualification is outside TASK-001 and was not available here.

## Overall result

**FAIL**

The bearer, RBAC, recovery, audit, tenant-isolation, issuer-selection, and secret-redaction controls are materially sound, but the candidate does not satisfy the required provider-network pinning or client-auth qualification guarantees until SEC-001 and SEC-002 are corrected and directly proven.
