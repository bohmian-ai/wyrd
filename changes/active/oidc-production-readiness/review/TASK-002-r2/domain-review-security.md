# Tenant OIDC security domain review

## Subject and boundary

Reviewed the immutable range `3fc085acf5b3a710d5dc80892bd2e664b3db6174..8b201627c0a957dccf46649d00c8c205689bc5de` against approved `SPEC-oidc-production-readiness` revision 4, `TASK-002`, and `TASK-002-R1`. Scope was tenant human OIDC only: discovery and SSRF controls; code, PKCE, state, nonce, signature, issuer, audience, authorized party, algorithm, key, and time verification; `(issuer, sub)` identity binding; group-to-tenant-role authority; canonical audit atomicity; refresh provenance; machine-plane separation; and secret exposure.

Applicable authority included `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-security-posture.md`, `architecture/references/architecture/patterns.md`, `architecture/references/doctrine/architecture-constraints.md`, and `architecture/references/languages/spec-driven-development.md`.

## Boundary coverage

| Boundary | Source and proof inspected | Result |
|---|---|---|
| Discovery and outbound provider IO | `wyrd-auth/src/{login,callback,connections}.rs`; `wyrd-auth-oidc/src/{provider,screening,jwks}.rs`; screened DNS resolution, address pinning, no proxy/redirect, bounded bodies and timeouts, fresh callback discovery, and issuer equality | PASS |
| State, PKCE, nonce, and completion | `wyrd-auth/src/{login,callback}.rs`; `wyrd-sql/src/queries/auth/login_state.rs`; migration and Postgres tests; 256-bit opaque state, hashed lookup, exact redirect and PKCE binding, consume-before-provider-IO, nonce equality, fixed callback response, sealed binding redemption, and one-use behavior | PASS |
| ID-token trust | `wyrd-auth/src/callback.rs`; `wyrd-auth-verify/src/lib.rs`; `wyrd-auth-oidc/src/jwks.rs`; issuer, audience, signature, `kid`, expiry/skew, nonce, `azp`, asymmetric/provider-advertised algorithm, and bounded key refresh all fail closed before identity/session persistence | PASS |
| Human identity and RBAC | `wyrd-spec/src/auth/human_connection.rs:204-249`; `wyrd-auth/src/pg_resolvers.rs:505-558`; `wyrd-auth/src/callback.rs:227-255,632-655`; exact `sub` is enforced both at authoring and stored-row decode, identity remains tenant-local `(issuer, sub)`, email is non-authoritative, and only mapped existing tenant roles are persisted | PASS |
| Audit transactionality | `wyrd-auth/src/callback.rs:235-265,551-568`; `wyrd-sql/src/queries/auth/role_assignments.rs`; role-sync and token-exchange events share the issuance transaction, while an audit append failure rolls back roles, refresh/session issuance, and completion | PASS |
| Refresh provenance and lifecycle | `wyrd-auth/src/refresh.rs:117-175`; issuance rechecks the exact active connection id/revision, current User status and grants, refuses unbound/non-User families, and preserves replay-family containment | PASS |
| Human/machine/platform separation | token route dispatch, workload verifier/binding paths, platform login, and `tenant_machine_independence_journey`; human callback accepts only human connection trust and creates only tenant `User` principals, while API-key/workload paths remain independent | PASS |
| Secret exposure | redacted connection views; `SecretBearer`/`SecretString` boundaries; `LoginState.code_verifier: SecretString` at `login_state.rs:95-115`; sealed provider secrets and sealed completions; callback responses contain neither provider code nor Wyrd tokens | PASS |

## Prior-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` | `ConnectionInput::validate` accepts only exact `sub`; `human_connection_trusted_issuer` independently rejects stored non-`sub` mappings; distinct signed subjects with the same email resolve distinct Users. | CLOSED |
| `FIND-TASK-002-2` | `verify_authorized_party` requires matching string `azp` for multi-audience tokens and validates any present `azp`; it runs after signature/audience verification and before identity resolution. | CLOSED |
| `FIND-TASK-002-3` | `replace_user_roles` reports a real set change; changed assignments append exactly one `auth.user.roles.sync` event in the same transaction as issuance and completion; unchanged assignments append none; injected audit failure rolls everything back. | CLOSED |
| `FIND-TASK-002-4` | Fresh discovery's `id_token_signing_alg_values_supported` reaches `verify_id_token_algorithm`; unknown, HMAC, or unadvertised algorithms are rejected before JWKS verification and persistence. | CLOSED |
| `FIND-TASK-002-7` | The single public `LoginState` holds the verifier as `SecretString`; the plain SQL decode value is private and immediately wrapped; debug regression proof excludes a sentinel verifier. | CLOSED |

## Material findings

No material security findings. No Critical, High, Medium, or Low/defense-in-depth item is required to satisfy the approved task.

## Positive controls

- Tenant and connection selection after initiation comes only from hashed, server-owned one-use state; request host, path, email, and provider claims cannot redirect authority.
- Provider network calls resolve, screen, and pin the effective address and disable proxies and redirects, closing DNS-rebinding and redirect-based SSRF paths.
- The callback rechecks the exact active connection revision before identity work and issuance, and issuance checks it again under the tenant transaction.
- Unknown groups and mapped names without tenant roles grant nothing; no default human role or provider-named permission exists.
- PKCE verifier, client secret, refresh/access tokens, and completed credentials remain redacted or sealed at their persistence and diagnostic boundaries.

## Verification performed and limits

Fresh focused verification passed:

- `human_subject_must_be_exactly_sub`
- `login_state_debug_redacts_the_pkce_verifier`
- the six callback Postgres tests for `azp`, unadvertised algorithm refusal, same-email/distinct-sub identity, changed/unchanged role audit, and audit-failure rollback
- `stored_human_connection_requires_the_sub_subject_claim`
- `tenant_machine_independence_journey`

The candidate also records successful full identity, principals, SQL, codegen, boundary, format, and lint lanes. This review did not independently rerun every recorded lane or the other three real-provider journeys; source inspection and the focused tests above cover the security boundary and remediation findings, while the broader recorded results remain supporting evidence.

## Overall result

**PASS** — the tenant human OIDC security/RBAC/trust boundary satisfies the original task and closes prior findings `FIND-TASK-002-1` through `-4` and `-7`; the review found no new material in-scope defect.
