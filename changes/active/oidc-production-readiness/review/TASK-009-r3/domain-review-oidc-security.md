# OIDC and security domain review

## Result

**PASS** for immutable candidate
`04597909203463820b2033c12956f5fe6fcfe1f4` against base
`35a53faa216b10651d85c96ce12e34f382cac637`.

The cumulative candidate satisfies the TASK-009 OIDC/security boundary. The
round-two remediation restores the relying-party library's ordinary
additional-audience refusal, removes the residual human `ExternalVerifier`
ownership, and routes platform human setup through the same process-owned
relying party used by platform begin and callback. No material security finding
remains.

`FIND-TASK-009-5` and `FIND-TASK-009-14` are withdrawn by lead direction. This
review did not reopen either finding and requires no correction associated with
them.

## Domain boundary

This review traced the complete base-to-candidate OIDC trust boundary and used
the latest remediation diff to locate the changed owners:

- `wyrd-auth-oidc::RelyingParty` and `ScreenedHttp`: discovery, authorization
  request construction, token redemption, ID-token verification, provider
  caching, and the one unknown-key refresh;
- tenant login, candidate connection testing, and the common callback through
  `HumanConnections` and `AuthorizationCodeExchange`;
- platform connection setup, platform begin/callback, verified-email pinning,
  and platform-session issuance through the boot-owned `PlatformLogin`;
- server boot composition and deployment-profile selection of the screened
  outbound policy;
- tenant and platform SQL identities and single-use login-state consumption;
- the separate workload RFC 7523 path through `ExternalVerifier`; and
- focused relying-party, callback, identity, platform-administration, workload
  discovery, and negative-flow evidence.

The reviewed runtime paths converge on `RelyingParty::{cached, discover,
authorize, redeem}` and its private `verify`/`id_token_verifier` owner. Every
provider request made by that path uses `ScreenedHttp`; the workload verifier
continues to use its separate screened `JwksCache` path.

## Authority and source coverage

| Authority | Applied boundary | Result |
|---|---|---|
| Approved `SPEC-oidc-production-readiness` revision 11: REQ-003, REQ-004, REQ-006, REQ-007, REQ-016, INV-001, INV-004, AC-002, AC-003, AC-006, AC-007, AC-008 | Tenant/platform separation, exact server-owned login binding, standard human OIDC validation, screened network calls, fail-closed lifecycle | PASS |
| TASK-009 and TASK-009-R2 | Library-owned discovery, PKCE/state/nonce, token exchange, ID-token validation, one screened adapter, one unknown-key rediscovery, platform setup through its relying party | PASS |
| `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-security-posture.md` | Tenant isolation, secret handling, SSRF/DNS pinning, no redirects, workload/human separation, redacted logging | PASS |
| OIDC Core 1.0 §§3.1 and 3.1.3.7; Discovery 1.0 §4 | Authorization-code flow; issuer, audience, `azp`, signature, key, algorithm, expiry, issued-at, nonce, subject, and discovery issuer equality | PASS |
| RFC 7636 §§4.3–4.6; RFC 6749 §2.3.1 | S256 PKCE and standard Basic/body client authentication | PASS |
| RFC 9207 §2.4; RFC 8725 §3 | Pre-token response-issuer binding and explicit asymmetric algorithm/key validation | PASS |
| `research/auth-standards-recommendation.md` §3.1/T1 and comparable-project standing direction | Use `openidconnect` defaults and existing native mechanisms; reject extra profiles, settings, checks, or transports | PASS |

## Trust-boundary assessment

| Boundary | Source evidence | Result |
|---|---|---|
| Discovery and issuer trust | `relying_party.rs:430-435` delegates full discovery/JWKS loading to `ProviderMetadata::discover_async`; `provider_metadata` separately enforces exact issuer equality for workload setup at lines 214-246. Discovery never creates issuer trust. | PASS |
| SSRF, DNS rebinding, redirects, and response bounds | `screening.rs:163-199` screens the effective URL, rejects any blocked resolution, pins accepted addresses, disables redirects and proxies, and applies the request timeout; `read_bounded_body` caps decoded bodies. Production composition supplies `BlockInternal` through `DeploymentProfile::screened_http`. | PASS |
| State, nonce, and PKCE | `relying_party.rs:450-476` uses `CsrfToken::new_random`, `Nonce::new_random`, and `PkceCodeChallenge::new_random_sha256`. Tenant state records only the state digest and binds connection revision, issuer, client, redirect, verifier, nonce, and initiation; callback consumption commits before provider IO. Platform state is likewise bounded and atomically deleted on use. | PASS |
| RFC 9207 mix-up control | `callback.rs:180-198` and `platform_login.rs:286-309` call the shared exact `verify_response_issuer` before token redemption. A present mismatch always fails; an absent value fails when discovery advertises support. No fallback selects another tenant, tenant connection, or platform connection. | PASS |
| Token endpoint authentication and redirect binding | `relying_party.rs:503-535` maps `SecretBasic`, `SecretPost`, and public clients onto the installed library, supplies the recorded redirect URI and PKCE verifier, refuses unsupported `private_key_jwt`, and sends through the redirect-disabled screened adapter. | PASS |
| ID-token validation | `relying_party.rs:570-595,629-666` pins issuer, client ID, discovered JWKS, provider-advertised asymmetric algorithms, expiry/issued-at, nonce, subject shape, and `azp`. The library default now rejects any untrusted additional audience; the former trust-all override is absent. | PASS |
| Key rotation | `relying_party.rs:539-549` retries only the `NoMatchingKey` case after invalidating and re-entering the Moka cache, then verifies once against the refreshed set. A still-unknown key fails closed. | PASS |
| Tenant and connection isolation | `callback.rs:99-207,321-347` derives the tenant only from the state digest, consumes state once, re-reads the exact bound tenant connection, and rechecks it before issuing. All tenant persistence uses `TenantConn`; no callback input chooses tenancy after initiation. | PASS |
| Platform isolation and authority | `platform_login.rs:261-345` consumes platform state, requires the current platform issuer, verifies the ID token through the shared relying party, accepts only a verified email for first-login matching, and delegates the grant to the platform-session owner. It never creates a tenant principal or tenant session. | PASS |
| Platform setup and cache ownership | `components/platform/identity.rs:226-244` calls the boot-owned `PlatformLogin::relying_party().discover`, so standard discovery and JWKS decoding succeed before persistence and replace the same process cache used by begin/callback. The stored audience remains derived solely from `client_id`. | PASS |
| Secret and sensitive-data handling | Client secrets and PKCE verifiers use secret wrappers at Rust boundaries; secret-bearing route inputs are skipped by tracing; provider secrets are sealed before storage and omitted from views; callbacks skip request payloads and do not return provider codes or ID tokens. No SQL, command, template, path, or deserialization injection sink was introduced; changed SQL uses fixed statements with binds. | PASS |
| Human/workload separation | Human tenant and platform verification use `RelyingParty`. `ExternalVerifier`, `PgIssuerResolver`, and the metadata-only workload setup remain on the RFC 7523 `jwt-bearer` path, with no human caller or duplicate human verifier. | PASS |
| Dependency surface | `openidconnect = 4.0.1` is exact-pinned with default features disabled. Its `oauth2` dependency has no reqwest feature, so no second provider HTTP client bypasses `ScreenedHttp`; the lockfile contains checksummed registry packages. | PASS |

## Prior-finding closure

| Finding | Current candidate evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` through `FIND-TASK-009-4` | Boot-owned platform relying party, RFC 9207 platform wire support, workload metadata-only discovery, and cache coalescing remain present. | CLOSED |
| `FIND-TASK-009-5` | Lead direction explicitly withdrew the provider-error proposal. | WITHDRAWN — NOT REOPENED |
| `FIND-TASK-009-6` through `FIND-TASK-009-10` | Human audience derives from client ID; dead accessor and handwritten random generation remain absent; platform setup proof and workload discovery behavior remain intact. | CLOSED |
| `FIND-TASK-009-11` | `set_other_audience_verifier_fn(|_| true)` is deleted. `id_token_refusals_fail_closed` rejects `[client_id, other]` even with `azp = client_id`, while the ordinary single-audience matching-`azp` case succeeds. | CLOSED |
| `FIND-TASK-009-12` | `wyrd-auth-verify`, `ServerAuth`, and `AuthHandles` describe workload-only use; the callback fixture no longer constructs the workload verifier. The production RFC 7523 caller remains. | CLOSED |
| `FIND-TASK-009-13` | Platform configuration uses `PlatformLogin::relying_party().discover`; served tests refuse unavailable/undecodable JWKS without replacing the row and prove same-issuer process-cache replacement. Workload setup remains metadata-only. | CLOSED |
| `FIND-TASK-009-14` | Lead direction explicitly withdrew the migration proposal. | WITHDRAWN — NOT REOPENED |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. No nonstandard hardening mechanism, option, check, file, or profile is
required.

### Positive Controls

- One screened provider transport owns effective-address validation, DNS
  pinning, no-proxy/no-redirect behavior, timeouts, and decoded-body limits.
- Vetted library primitives own discovery, cryptographic state/nonce
  generation, S256 PKCE, code exchange, and ID-token signature/JWKS checks.
- The shared verifier fails closed on issuer, audience, `azp`, nonce,
  algorithm, signature, key, expiry, future issued-at, and invalid subject.
- Tenant state is hashed, bounded, single-use, and the sole post-initiation
  tenant/connection selector; platform state is separately stored and consumed.
- Platform first-login pinning requires provider-asserted `email_verified` and
  issues only through the platform session owner.
- Human OIDC and workload RFC 7523 verification remain separate, with no
  duplicate human token verifier.

## Verification evidence and limits

The remediation record supplies exact green selectors for the shared
relying-party refusal cases, the complete callback test target, workload boot
and administration discovery cases, the served platform administration path,
and the tenant callback refusal journey. It also records the narrow repository
lanes covering the final write set as green.

This reviewer independently ran:

```text
mise exec -- cargo nextest run --locked -p wyrd-auth-oidc --lib \
  -E 'test(=relying_party::tests::id_token_refusals_fail_closed)'
```

Result: one selected, one passed, 46 skipped.

No Python, TypeScript, SDK, or generated contract changed in the remediation;
`codegen:check` is recorded green. Under standing human direction
`518026d54`, full user-journey and every-language sweeps belong to final change
review, so their absence from a task-review rerun would not be a gap. This was
otherwise a static review of the immutable committed source; no external live
provider or deployment was contacted. No required sub-reviewer was unavailable.
