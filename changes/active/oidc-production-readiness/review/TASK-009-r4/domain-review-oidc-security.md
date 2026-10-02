# OIDC, security, and trust-boundary domain review

## Result

**PASS** for immutable candidate
`0bd3686e8bb763b07376f84661946aadc6200bfb` (tree
`86fb8e11681b4d2e60bba2aa7ee3bf2c4f134153`) against base
`35a53faa216b10651d85c96ce12e34f382cac637`.

The cumulative candidate satisfies TASK-009's OIDC relying-party, security,
RBAC, and trust-boundary obligations. The round-three remediation accurately
declares and proves the existing platform-configuration discovery/JWKS `503`.
No material domain finding remains.

`FIND-TASK-009-5`, `FIND-TASK-009-14`, and `FIND-TASK-009-15` are withdrawn by
binding lead direction. This review did not reopen, rename, or require an
indirect implementation of any of them. The surviving public
`PlatformLogin::relying_party` accessor is therefore not a finding; it has no
behavioral, security, tenancy, durability, or public-contract defect.

## Reviewed boundary

The review traced the complete base-to-candidate path, using the latest
remediation diff only to locate the changed platform contract:

- `ScreenedHttp` from effective URL through DNS resolution, address rejection,
  pinned connection, proxy/redirect refusal, timeout, and decoded-body bound;
- `RelyingParty` discovery/JWKS loading, cache behavior, library-generated
  state/nonce/S256 PKCE, client authentication, code redemption, ID-token
  verification, and the single unknown-`kid` rediscovery;
- tenant begin/callback/connection-test flows through server-owned, bounded,
  single-use state and the exact bound connection revision;
- platform connection setup and platform begin/callback through the separate
  process-owned relying party, state table, principal pinning, and platform
  session issuer;
- RFC 9207 response-issuer handling before token-endpoint traffic on both human
  callback paths;
- provider-secret, authorization-code, PKCE-verifier, token, error, trace, and
  audit handling; and
- the distinct workload RFC 7523 path through `JwtBearer` and
  `ExternalVerifier`, with no human caller left on that verifier.

## Authority and source coverage

| Authority | Boundary applied | Source evidence | Result |
|---|---|---|---|
| Approved spec revision 11: REQ-003 through REQ-008, REQ-013, REQ-016, REQ-017; INV-001, INV-003, INV-004, INV-007; AC-007 | Human RP behavior, server-bound selection, tenant/platform/workload separation, fail-closed lifecycle and audit | `relying_party.rs`, `login.rs`, `callback.rs`, `platform_login.rs`, `jwt_bearer.rs` | PASS |
| TASK-009 plus R1/R2/R3 remediation authority | Replace handwritten human RP mechanics with `openidconnect`; retain one screened transport, one unknown-key refresh, RFC 9207, and workload-only external verification | `RelyingParty::{cached,discover,authorize,redeem}`, both callback owners, platform configuration | PASS |
| `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-security-posture.md` | SSRF/DNS pinning, secrets, tenant authority, public errors, and fail-closed verification | `screening.rs:88-335`; tenant and platform state/callback owners | PASS |
| OpenID Connect Core 1.0 §§2, 3.1, 3.1.3.7; Discovery 1.0 §4 | Standard discovery and authorization-code RP; issuer, audience, `azp`, algorithm, signature, key, time, nonce, and subject validation | `relying_party.rs:406-435,450-476,503-666` | PASS |
| RFC 6749 §2.3.1 and RFC 7636 §§4.3-4.6 | Basic/body/public client authentication, exact recorded redirect, S256 PKCE | `relying_party.rs:503-535`; state writers/readers | PASS |
| RFC 9207 §2.4 | Exact response `iss` match; required when advertised; validation before code redemption | `callback.rs:180-198,406-429`; `platform_login.rs:286-318` | PASS |
| RFC 8725 applicable JWT controls | Explicit asymmetric provider-advertised algorithms and discovered keys; no symmetric human-token acceptance | `relying_party.rs:620-666` | PASS |
| Human standing conventionality direction | Use library/standard behavior without extra profiles, knobs, checks, or duplicate verification | Installed `openidconnect` path and existing native repository mechanisms | PASS |

## Trust-boundary assessment

| Boundary | Evidence | Result |
|---|---|---|
| Provider network access | `ScreenedHttp::client_for` screens the scheme and every resolved address, pins the accepted addresses with `resolve_to_addrs`, disables proxies and redirects, and applies a whole-request timeout. `read_bounded_body` caps decoded bytes. `ScreenedHttp::send` is the `openidconnect` async transport for discovery, JWKS, and token requests. | PASS |
| Discovery is metadata, not trust | Full human discovery uses `ProviderMetadata::discover_async`, checks exact issuer equality, and fetches the advertised JWKS only through the screened adapter. Workload setup's metadata-only path also checks exact issuer equality and leaves key retrieval to the workload verifier. | PASS |
| State, nonce, PKCE, and redirects | `openidconnect` generates state, nonce, and S256 PKCE. Tenant state stores only the state digest and binds tenant connection revision, issuer, client, redirect, verifier, nonce, and initiation. Platform state is separate, bounded, and consumed once. Redemption reuses the recorded redirect and verifier. | PASS |
| RFC 9207 mix-up defense | Tenant and platform callbacks compare a present `iss` exactly with the state-bound issuer and require it when discovery advertises support, before any token request. Absent unadvertised `iss` follows conventional OIDC behavior and retains state, PKCE, and verified ID-token issuer binding. | PASS |
| ID-token acceptance | The `openidconnect` verifier pins exact issuer, the connection client ID as sole trusted audience, provider-advertised asymmetric algorithms, discovered key, signature, expiry, future `iat`, and nonce. The adjacent check enforces `azp`; subject shape is bounded before claim mapping. | PASS |
| Unknown signing key | Only `NoMatchingKey` invalidates the issuer entry and re-enters the coalescing cache once. Verification against the refreshed set is attempted once; a still-unknown key fails closed. | PASS |
| Tenant and connection selection | Tenant callback derives the tenant only from the state digest, consumes the row before provider IO, validates the exact active/candidate connection binding, and rechecks the active revision before issuance. Callback inputs cannot switch tenant, issuer, or connection. | PASS |
| Platform separation and RBAC | Platform login reads only the deployment connection, consumes only platform state, verifies through its own relying party, requires provider-verified email for first pinning, resolves only a pre-registered platform principal, and mints only a platform session. It neither creates nor grants a tenant principal. | PASS |
| Secret and sensitive-data handling | Provider secrets and PKCE verifiers cross owners in secret wrappers; secret-bearing request/code values are skipped by tracing; provider secrets are sealed before storage and absent from views. Errors and audits do not carry Wyrd secrets, codes, tokens, or ID-token claims. Binding lead direction permits standard provider `error_description`/`error_uri` diagnostics and is not reopened. | PASS |
| Human/workload verifier separation | Production caller search leaves `ExternalVerifier::verify_external` on `JwtBearer::verify_workload_assertion`; it requires `IssuerTokenPolicy::Workload`, tenant-qualified issuer resolution, audience verification, and a workload binding. Tenant and platform human ID tokens use only `RelyingParty`. | PASS |
| Platform configuration failure contract | Full metadata-plus-JWKS discovery occurs before persistence. Unavailable, undecodable, or issuer-mismatched discovery maps to the existing `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`; the route declaration, served OpenAPI, and served negative cases now agree. | PASS |
| Injection and unsafe decoding | Changed SQL remains fixed/bound; provider JSON and JWT inputs go through typed library decoders and bounded HTTP/token inputs. No command, template, path traversal, or unsafe-deserialization sink was introduced. | PASS |

## Prior-finding closure

| Finding | Current candidate evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` through `FIND-TASK-009-4` | Shared platform RP ownership, platform RFC 9207, workload metadata-only setup, and coalesced cache refresh remain present. | CLOSED |
| `FIND-TASK-009-5` | Binding lead direction requires no provider-error redaction mechanism beyond keeping Wyrd's own secrets absent. | WITHDRAWN — NOT REOPENED |
| `FIND-TASK-009-6` through `FIND-TASK-009-10` | Client-ID-derived audience, library generators, corrected ownership/docs, and exact focused evidence remain present. | CLOSED |
| `FIND-TASK-009-11` | No trust-all additional-audience override exists; ordinary library audience rejection and `azp` validation remain. | CLOSED |
| `FIND-TASK-009-12` | Human paths no longer call `ExternalVerifier`; its production consumer is the workload RFC 7523 exchange. | CLOSED |
| `FIND-TASK-009-13` | Platform setup performs full discovery through the same process cache consumed by platform begin/callback, with failure and same-issuer replacement proof. | CLOSED |
| `FIND-TASK-009-14` | Binding lead direction accepts the unreleased audience-column migration as shipped and forbids an overlap mechanism. | WITHDRAWN — NOT REOPENED |
| `FIND-TASK-009-15` | Binding lead direction accepts the accessor placement because it has no behavioral effect. | WITHDRAWN — NOT REOPENED |
| `FIND-TASK-009-16` | Route rustdoc and OpenAPI declare the existing discovery/JWKS `503`; served failures assert its status and stable code. | CLOSED |

## Material findings

None.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. No extra mechanism, setting, check, file, or option beyond standard
practice is required.

### Positive Controls

- One outbound transport owns address screening, DNS pinning, no-proxy and
  no-redirect behavior, timeout, and decoded-body bounds.
- Vetted library primitives own discovery, random state/nonce, S256 PKCE, code
  exchange, and ID-token cryptographic verification.
- Human token acceptance fails closed across issuer, sole trusted audience,
  `azp`, advertised algorithm, discovered key, signature, time, nonce, and
  subject constraints.
- Server-owned one-time state is the sole post-initiation tenant/connection
  selector, and platform state and authority remain separate.
- Human and workload federation retain distinct verifier and grant paths.

## Verification evidence and limits

The TASK-009 and remediation records provide focused green evidence for the
shared relying-party negative cases, tenant callback behavior, workload
discovery, platform setup/cache behavior, and boundary lanes. For the final R3
write set, the orchestrator independently ran the repository-managed exact
selectors on the immutable candidate:

```text
platform_admin_e2e::federated_platform_sign_in_runs_through_the_served_callback: 1 passed
pg_openapi_contract::the_served_document_describes_the_composed_surface: 1 passed
```

The first proves unavailable and undecodable key sets return
`503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE` without replacing the durable row;
the second proves the served platform connection PUT contract advertises that
same response. Direct invocations lacking the repository setup wrapper were
environment/setup failures, not product failures.

This was otherwise a static review of committed source and existing evidence;
no live external provider was contacted. Per binding human direction, the
narrowest task lanes are sufficient here and full user journeys run at change
review. No domain-review limitation prevents acceptance.
