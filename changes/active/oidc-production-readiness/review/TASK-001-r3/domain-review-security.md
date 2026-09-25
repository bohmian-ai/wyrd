# Security / RBAC / OIDC Domain Review

## Result

**FAIL**

The immutable candidate closes the previously validated callback-response,
provider-body, production-scheme-at-fetch, proxy, session-provenance, and
refresh-cutoff findings, but three material security obligations remain open.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `eb4142cc95fa24b0525c71f66bfc978577cd4b7c`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation inputs: `TASK-001-R1-production-readiness-gaps.md` and
  `TASK-001-R2-remaining-production-readiness-gaps.md`

The candidate remained at the stated commit during this review.

## Reviewed boundary

The review traced the tenant connection administration handlers through RBAC
decision and canonical audit append, candidate qualification, activation's
recovery credential, durable connection ownership, login-state creation and
single-use consumption, callback verification and session issuance, exact
connection-revision binding, refresh rotation, discovery/JWKS/token HTTP
screening, DNS pinning, TLS/scheme enforcement, response bounds, and public
error redaction. All production callers of `ScreenedHttp::client_for`,
`HumanConnections::activate`, `recovery_key_authorizes`, `begin_login`, and the
authorization-code exchange were included.

## Authority and source coverage

| Authority / surface | Coverage |
|---|---|
| `AGENTS.md` §§2, 9, 11-12 | Server-owned auth, stable errors, tenant isolation, audit, journey proof |
| `architecture/agent-rules.md` | Every permission evaluation is audited; provider DNS must be bounded, screened, and pinned |
| `architecture/wyrd-security-posture.md` | Authentication planes, audit attribution, fail-closed security, TLS, SSRF and DNS bounds |
| `architecture/wyrd-design.md` runtime identity | Tenant/platform separation and canonical authorization evidence |
| `architecture/references/architecture/patterns.md` | Trust-boundary validation and screened provider IO |
| Approved spec rev. 4 | REQ-003-005, REQ-017, INV-003-004, AC-003/007/009 |
| Original task and R1/R2 remediation tasks | Recovery route-back proof, authorization-before-provider-IO, exact callback qualification, TLS, pinning, bounds, connection/session cutoff |
| Administration and contract | `components/admin/identity.rs`, `wyrd-spec/auth/human_connection.rs`, `wyrd-auth/connections.rs` |
| Login, callback, issuance, refresh | `wyrd-auth/login.rs`, `callback.rs`, `issuance.rs`, `refresh.rs`; server login/callback adapters |
| Provider boundary | `wyrd-auth-oidc/screening.rs`, `provider.rs`, `jwks.rs`; production boot policy selection |
| Durable trust | human-connection, login-state, and refresh-token SQL queries and migration |
| Proof | Focused unit/integration/journey source and the implementation evidence recorded in TASK-001/R1/R2; no runtime lanes were executed |

## Prior security-finding closure

| Prior finding | Security disposition |
|---|---|
| `FIND-TASK-001-1` callback response ambiguity | **Closed.** `callback_redirect_qualifies` accepts exactly one code or recognized error arm and rejects mixed/duplicate arms. |
| `FIND-TASK-001-2` client-auth qualification | **Closed.** Only a 4xx `invalid_grant` proves the deliberately invalid-code probe. |
| `FIND-TASK-001-3` request-derived callback | **Closed.** Login state and exchange use the deployment-controlled callback. |
| `FIND-TASK-001-4` ambient proxy bypass | **Closed.** The screened client disables ambient proxies. |
| `FIND-TASK-001-5` old-connection session renewal | **Closed for runtime and migration.** New families carry the exact connection revision, issuance/refresh recheck it under the slot lock, and provenance-free legacy user rows remain unbound and fail closed. |
| `FIND-TASK-001-7` candidate-test decision | **Closed.** Provider IO follows an audited permission decision and the stamp follows a fresh transactional decision. |
| `FIND-TASK-001-13` Public-client secret presence | **Closed.** Any supplied secret is refused for `Public`; secret methods require nonempty material. |
| `FIND-TASK-001-18` cleartext discovered endpoints | **Incomplete.** Server fetches now reject cleartext in production, but live login returns a freshly discovered authorization endpoint without applying that check (`SEC-R3-002`). |
| `FIND-TASK-001-19` unbounded provider bodies | **Closed.** Discovery, JWKS, token exchange, and candidate token probe use the shared 1 MiB decoded-body reader. |

## Material findings

### SEC-R3-001 — VIOLATION / Medium: recovery-key authorization is not audited

- **Violated obligation:** `AGENTS.md` and `architecture/agent-rules.md` require
  one canonical audit row for every decision that evaluates a principal's
  permission, allowed and denied. REQ-017 requires security-significant
  connection mutations to produce attributable canonical evidence.
- **Location:** `crates/wyrd/wyrd-auth/src/connections.rs:451-497` and
  `:918-952`.
- **Evidence and reachability:** the activation handler first records the bearer
  caller's `identity.oidc.candidate.activate` decision, then the live production
  `HumanConnections::activate` path verifies a second credential, loads its
  principal roles, resolves permissions, and calls
  `permissions.contains(identity_connections:write)`. Neither the allowed nor
  valid-but-underprivileged recovery principal produces an audit event; the
  only activation event names the bearer caller. `activate_candidate` is the
  production caller. Journey assertions likewise expect only that outer row.
- **Observable consequence:** an activation can use a distinct recovery
  credential as its route-back proof without retained evidence identifying the
  recovery principal, its non-secret credential id, permission, or outcome.
  Repeated use of a valid but underprivileged recovery key is also invisible as
  a permission denial. This breaks accountability at the exact control meant
  to prevent tenant lockout.
- **Required testable correction:** keep the existing verifier and activation
  transaction, but have the recovery-key check return enough verified identity
  to append a second canonical allow/deny decision in that same transaction,
  naming the recovery principal and non-secret API-key id. Audit failure must
  abort activation. Malformed, unknown, cross-tenant, or hash-mismatched bytes
  remain indistinguishable and must not be logged. Prove one permitted key, one
  valid underprivileged key, and injected recovery-decision audit failure; assert
  exact audit cardinality/attribution and no promotion on either refusal.

### SEC-R3-002 — VIOLATION / High: live login can redirect to a newly discovered cleartext authorization endpoint

- **Violated obligation:** INV-004 requires TLS to remain fail closed; R2's
  `FIND-TASK-001-18` requires production screening to refuse discovered
  cleartext endpoints; the security posture requires scheme validation at the
  effective URL.
- **Location:** `crates/wyrd/wyrd-auth/src/login.rs:129-171`, especially
  `:137-154`; compare the enforced policy in
  `crates/shared/wyrd-auth-oidc/src/screening.rs:154-156,218-220`.
- **Evidence and reachability:** production `begin_login` screens and fetches the
  configured HTTPS issuer, reads a fresh discovery document, then places its
  `authorization_endpoint` directly into `LoginInitResponse`/the HTTP redirect.
  Unlike candidate qualification (`connections.rs:734-739`), this live path
  never passes the effective authorization endpoint through
  `ScreenedHttp::client_for`. A candidate test stamp is not a runtime pin: the
  provider can change discovery after activation.
- **Plausible exploit scenario:** after activation, a compromised or
  misconfigured provider discovery response changes `authorization_endpoint`
  to `http://login.example/...`. Wyrd returns that URL to every user. A network
  attacker can replace the cleartext sign-in page, capture credentials, and use
  them against the real provider, while Wyrd has already persisted a valid
  state/nonce for the attempt. The production TLS rule has therefore failed
  open at the browser-facing leg.
- **Observable consequence:** active production login can direct users to an
  unencrypted endpoint even though candidate testing and all server-side token
  and JWKS calls enforce HTTPS.
- **Required testable correction:** before persisting login state or returning
  the URL, reuse the existing `ScreenedHttp` effective-URL check for the freshly
  discovered authorization endpoint. Production must refuse cleartext and
  blocked address classes; `AllowInternal` must retain the existing local-test
  provider path. Add a live `begin_login` test whose HTTPS/configured issuer
  advertises a cleartext authorization endpoint and assert refusal with no
  login-state row, plus the existing permissive local-provider proof.

### SEC-R3-003 — VIOLATION / Medium: manual DNS screening has no resolver deadline

- **Violated obligation:** `architecture/agent-rules.md` requires a bounded DNS
  resolver before a user/tenant-supplied URL is fetched; REQ-007/INV-004 and the
  security posture require bounded provider calls and fail-closed DNS pinning.
- **Location:** `crates/shared/wyrd-auth-oidc/src/screening.rs:199-215`.
- **Evidence and reachability:** `resolve_and_screen` awaits
  `tokio::net::lookup_host` directly. The reqwest ten-second timeout is installed
  only after this await, so it does not bound manual resolution. Every
  production discovery, JWKS, token, and candidate-probe call reaches this path
  for a domain name.
- **Observable consequence:** a tenant-controlled hostname or unhealthy system
  resolver can hold login/callback/admin request tasks beyond the documented
  provider-operation bound, enabling request and worker exhaustion without any
  outbound connection.
- **Required testable correction:** wrap the existing single resolution in a
  fixed deadline (reuse the existing fetch bound; add no knob or resolver
  framework), map expiry to the same redacted unresolved/provider-unavailable
  result, and preserve the exact screened address set used by
  `resolve_to_addrs`. Add the smallest deterministic check of the deadline and
  retain the public error-redaction and pinning tests.

## Security audit summary

### Critical

None.

### High

- `SEC-R3-002` — runtime authorization-endpoint TLS enforcement is incomplete.

### Medium

- `SEC-R3-001` — the recovery credential's permission decision is unaudited.
- `SEC-R3-003` — manual DNS resolution is outside the provider-operation deadline.

### Low / Defense in depth

None reported; optional hardening was excluded.

### Positive controls

- Tenant administration derives tenancy from the verified bearer and executes
  connection storage under `TenantConn`/RLS.
- Candidate writes decode only after the bearer authorization decision and
  expose typed, redacted public errors.
- Provider clients disable redirects and ambient proxies, resolve once, reject
  any blocked answer, and pin reqwest to the screened addresses.
- Production server fetches require HTTPS; local HTTP remains isolated to the
  permissive development/test policy.
- Provider discovery, JWKS, token, and probe response bodies share a decoded
  1 MiB cap.
- Login state is single-use, bounded, and binds issuer, callback, nonce, PKCE
  verifier, and exact connection id/revision.
- Initial issuance and refresh recheck exact Active connection provenance under
  the tenant slot lock; replacement, deactivation, removal, and provenance-free
  migration rows fail closed.
- Client secrets and recovery-key bytes are skipped by tracing and omitted from
  views, audit payloads, and public errors.

## Verification limits

This was a time-bounded static review. Per assignment, no Cargo or `mise` Cargo
lane was run. Recorded candidate evidence was inspected but not independently
re-executed. No controlled Okta, Keycloak, or Entra qualification environment
was used. DNS-stall behavior was established from the live call ordering and
timeout placement, not by altering the host resolver. These limits do not block
the three source-proven findings above.
