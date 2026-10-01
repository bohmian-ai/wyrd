# TASK-002 R6 Security/OIDC/RBAC Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `0ca117a744ddcb7414b104c4382027970531b608`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation inputs: TASK-002 R1 through R5

The candidate was checked at the start and end of this review and remained
`0ca117a744ddcb7414b104c4382027970531b608`. Lead-directed reuse and test
commits identified in the evidence tables were treated as authorized work, not
scope drift, and were still inspected for security regression.

## Reviewed boundary

This review traced the reachable tenant-human authentication boundary from
`POST /auth/login` through state creation, the common callback, screened
discovery/token/JWKS traffic, ID-token verification, tenant User resolution,
group-to-role replacement, session issuance, sealed completion, and refresh
rotation/replay containment. It also checked the seams with platform OIDC and
machine API-key/workload federation to ensure no fallback or authority-plane
convergence was introduced.

The review specifically covered state entropy and single use, PKCE and nonce,
exact redirect and connection-revision binding, `iss`/`aud`/`exp`/`nbf`/`iat`,
OIDC `azp`, algorithm and key selection, SSRF resolution and connection
pinning, exact `(issuer, sub)` identity, tenant role mapping, canonical audit,
refresh-family locking, connection provenance, and the closure of prior
`FIND-TASK-002-1` through `FIND-TASK-002-13`.

## Authority and source coverage

| Surface | Governing authority | Source and caller coverage | Result |
|---|---|---|---|
| Tenant and principal authority planes | `architecture/wyrd-security-posture.md` principal lifecycle; `architecture/wyrd-design.md` Runtime identity; SPEC INV-001–004 | `wyrd-auth/src/login.rs`, `callback.rs`, `platform_login.rs`, `pg_resolvers.rs`; server auth adapters; machine-independence journey | PASS |
| Login state, PKCE, nonce, callback and completion | TASK-002 packet-local contract; REQ-006/007; INV-001/004 | `HumanConnections::begin_login`; `AuthorizationCodeExchange::{execute,complete,finish_id_token_exchange,bound_connection}`; login-state SQL/migration; callback route | PASS |
| Provider network trust and SSRF | `architecture/agent-rules.md` SSRF rule; security posture Source/SSRF defense; architecture patterns External Network Pattern | `ScreenedHttp::client_for`; discovery, token-exchange, and `JwksCache` callers; redirect-disabled, proxy-free, bounded IO | PASS |
| ID-token cryptographic and claim verification | REQ-007; INV-004; security posture federation and signing-key rules | `verify_id_token_algorithm`; `ExternalVerifier::{verify_external_against,verify_id_token_against}`; nonce and `azp` checks; tenant/platform/workload caller split | PASS |
| Tenant identity and RBAC | REQ-008/014/015; INV-002/003; security posture authorization rules | exact-`sub` request and stored-row validation; `ensure_user_identity`; `role_names_to_refs`; `replace_user_roles`; tenant issuance | PASS |
| Audit coupling | REQ-017; `AGENTS.md` and agent-rules canonical audit requirements; architecture patterns Audit Pattern | role sync, token issuance, refresh containment, connection mutations, and refusal audit paths; canonical `vala.audit_staging` append | PASS |
| Human refresh security | REQ-016; TASK-002 renewal contract; security posture access/refresh rules | `RefreshTokens::execute`; refresh SQL; family advisory lock; connection-slot ordering; route-owned commit; deterministic overlap proof | PASS |
| Public token exposure and retired bypass | TASK-002 callback/completion contract; REQ-006/007 | `CallbackQuery` secret wrapper; fixed redirect/static response; removed authorization-code token grant and CLI bypass | PASS |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by the approved task.

### Positive Controls

- `login.rs:84-138` creates a 256-bit opaque state, nonce, and PKCE verifier,
  persists only the state digest, binds the deployment-controlled callback and
  exact connection revision, and returns no authorization URL before commit.
- `callback.rs:96-167` resolves only the tenant owner of the opaque state,
  consumes and commits it before provider IO, then uses only the stored issuer,
  client, redirect, PKCE verifier, and nonce. There is no host, path, email, or
  platform-connection fallback.
- `callback.rs:201-267` verifies the fresh advertised algorithm, shared
  signature/key/claim policy, nonce, and OIDC authorized party before opening
  the identity/role/issuance transaction; it rechecks the exact Active
  connection and commits role audit, issued session, refresh state, and sealed
  completion together.
- `wyrd-auth-verify/src/lib.rs:508-601` rejects HMAC, requires a `kid`, verifies
  through the screened JWKS owner, requires `exp`/`iss`/`aud`, validates a
  present `nbf`, and applies the ID-token-only numeric non-future `iat` rule.
  Workload assertions remain on the generic verifier instead of inheriting a
  human ID-token contract.
- `screening.rs:148-187` performs bounded DNS resolution, rejects any blocked
  answer, pins the screened addresses, disables redirects and proxies, and
  applies a total request timeout. Discovery, token exchange, and JWKS refresh
  all use this capability; provider bodies are capped after decoding.
- Human connection authoring and stored-row decode both require exact OIDC
  `sub`. User identity is tenant-RLS scoped and keyed only by `(issuer, sub)`;
  email is display data. Provider groups grant only explicitly mapped current
  tenant roles, with no default privileged role.
- Callback responses contain no Wyrd or provider token: browser completion is
  a fixed same-origin redirect without a query capability and the CLI response
  is static HTML. The retired `authorization_code` arm cannot exchange
  caller-supplied code/state at `/auth/token`.
- `refresh.rs:121-226` looks up the stored family, takes the tenant-qualified
  transaction advisory lock before active/stale classification, keeps the
  family-before-connection order, rejects non-human or unbound rows, and
  transactionally audits and revokes the family on replay.
- Sensitive inputs use `SecretBearer`/`SecretString`; callback instrumentation
  skips all inputs, PKCE state redacts under `Debug`, and errors/audit contain
  closed codes rather than bearer, authorization code, verifier, client secret,
  or token material.

## Prior-finding closure

| Prior finding | Current closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` exact human subject | Request validation and stored-row decode require `sub`; identity lookup uses verified subject, never email. | CLOSED |
| `FIND-TASK-002-2` OIDC `azp` | `verify_authorized_party` enforces matching `azp`, including multi-audience tokens, before persistence. | CLOSED |
| `FIND-TASK-002-3` role-change audit | Changed mapped roles append one canonical event in the issuance transaction; audit failure prevents commit. | CLOSED |
| `FIND-TASK-002-4` advertised algorithm | Fresh discovery-set membership precedes the shared verifier's HMAC rejection and JWKS signature check. | CLOSED |
| `FIND-TASK-002-5` duplicated tenant selection | Login-state transitions rely on forced RLS; only the least-disclosure owner lookup crosses RLS to select the transaction. | CLOSED |
| `FIND-TASK-002-6` raw pool propagation | The cross-tenant state-owner query remains the narrow inherent `WyrdPostgres::login_state_tenant` capability. | CLOSED |
| `FIND-TASK-002-7` printable PKCE verifier | Durable login state holds `SecretString`; decoded storage is wrapped immediately and not logged. | CLOSED |
| `FIND-TASK-002-8` incomplete Rust documentation | The cited security-relevant items retain substantive workflow, error, panic, and invariant documentation. | CLOSED |
| `FIND-TASK-002-9` hidden imports | The cited imports remain in module import blocks; no security behavior changed. | CLOSED |
| `FIND-TASK-002-10` false algorithm-helper contract | Current rustdoc accurately assigns advertised-set membership locally and symmetric/signature verification to the shared verifier. | CLOSED |
| `FIND-TASK-002-11` optional binding/time claims | Required issuer/audience/expiry and present-`nbf` checks plus ID-token-only required non-future `iat` are implemented on the two OIDC callers. | CLOSED |
| `FIND-TASK-002-12` refresh replay race | Family lock precedes classification and connection locking; the recorded deterministic overlap proof establishes successor containment. | CLOSED |
| `FIND-TASK-002-13` stale refresh lookup rustdoc | Candidate `0ca117a74` now describes lookup → family lock → classification and agrees with `RefreshTokens::execute`; runtime behavior is unchanged. | CLOSED |

## Verification evidence and limits

- Static review covered the complete cumulative base-to-candidate diff, all
  R1–R5 task/verdict/finding inputs, complete security-critical bodies and
  their production callers, the migration and RLS/definer boundary, public
  contracts, and relevant journey/focused tests. The repository has no
  `.codegraph/` index, so Git, `rg`, and direct source inspection were used.
- Fresh `git diff --check
  3fc085acf5b3a710d5dc80892bd2e664b3db6174..0ca117a744ddcb7414b104c4382027970531b608`
  passed. The final R5 commit changes only the remediation evidence and
  `refresh_by_hash` rustdoc; it adds no executable security behavior.
- Recorded cumulative evidence is green for the focused OIDC binding/time
  claim test, deterministic ancestor-replay overlap test, four required tenant
  identity journeys, full 27-test identity lane, principals unit/integration,
  SQL, tenant isolation, codegen/docs, format, lints, and diff hygiene.
- This bounded review did not rerun Docker/Postgres/Keycloak/Dex or broad Cargo
  lanes. Live Okta and Entra qualification belongs to the change-level
  production qualification obligation, not this task's mock-provider
  implementation acceptance.
- TASK-003 BFF session completion and TASK-004 CLI handoff persistence remain
  intentional downstream non-goals; this review assessed only the primitives
  TASK-002 owns.

## Proposed findings

None. The independently reviewed security/OIDC/RBAC ledger is empty.

## Overall result

**PASS** — the cumulative candidate satisfies the TASK-002 security, OIDC,
RBAC, audit, refresh, tenant-isolation, and authority-plane obligations. No
reachable exploitable regression or bounded task finding remains.
