# Domain Review — Security and OAuth

## Overall Result

**FAIL** — the OAuth/OIDC remediation closes the client-authentication, safe-redirect, callback-denial, token-exchange, refresh-containment, lifecycle-fencing, audit, and OpenAPI findings, but the device user-code limiter is still attached to each application replica rather than the public gateway that selects a replica. It therefore does not provide the one client-address budget across replicas required by `FIND-TASK-010-10`.

`FIND-TASK-010-1` was routed to TASK-011 by `review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md` and was not reopened.

## Reviewed Boundary

Immutable subject:

- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Candidate remained `HEAD` at the start and end of this review.

The security/OAuth review traced:

- confidential `wyrd-ui` and public `wyrd-cli` identification at token, device-authorization, and revocation endpoints;
- exact authorize client/redirect binding, duplicate-parameter handling, PKCE S256, and downstream error redirects;
- provider success/error callback parsing, one-time state consumption, issuer binding, connection lifecycle fencing, and transactional login audit;
- RFC 8693 audience classification and RFC error responses;
- public refresh rotation, replay classification, chain-only containment, confidential non-rotation, and tenant-RLS lookup;
- device authorization, approval/denial/redemption races, user-code verification limiting, and the documented deployment path;
- served OpenAPI client-identification alternatives; and
- the changed startup-image proof, including its production profile, secret-file permissions, tenant mode, and Operator KEK source.

## Authority and Source Coverage

| Authority or source | Coverage and conclusion |
|---|---|
| Approved `SPEC-oidc-production-readiness` revision 11 | Applied REQ-005, REQ-006, REQ-009, REQ-011, REQ-012, REQ-016, REQ-017, REQ-021; INV-001, INV-005, INV-007; AC-004, AC-007, AC-009. |
| Original TASK-010, prior r1 verdict/ledger, r1 remediation, and lead routing | Rechecked closure of `FIND-TASK-010-2` through `-13`; excluded routed `FIND-TASK-010-1`. |
| `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md` | Applied trust-boundary validation, tenant authority, secret handling, canonical audit, test-gate integrity, narrow verification, and multi-tenant production KEK rules. |
| RFC authority named by TASK-010 | Checked RFC 6749 §§2.3.1, 3.1, 3.1.2, 4.1, 5.1, 5.2, 6; RFC 7636 §§4.3–4.6; RFC 7009 §2; RFC 8628 §§3.1–3.5 and 5.1; RFC 8693 §2; RFC 7523 §§2.1 and 3; RFC 8414 §§2–3; RFC 9700 §4.14.2. |
| Contract and HTTP owners | Reviewed `wyrd-spec/src/auth/{oidc,token}.rs`, generated callback/token schemas, `wyrd-server/src/auth/{authorize,callback,cli_login,oauth}.rs`, `components/auth/routes.rs`, `http/openapi.rs`, and the served OpenAPI test. |
| Durable auth owners | Reviewed `wyrd-auth/src/{callback,cli_logins,refresh,audit}.rs` and `wyrd-sql/src/queries/auth/refresh_tokens.rs`, including lock order, RLS, revocation-chain traversal, and commit behavior. |
| Deployment and proof | Reviewed `docker/official/Dockerfile`, `docker/official/extras/{entrypoint.sh,nginx.conf.template}`, `scripts/server/test-startup.sh`, `mise.toml`, the documented Kubernetes edge/mesh topology, and the dedicated multi-tenant Operator-key boot test. |

## Prior-Finding Closure

| Finding | Security/OAuth conclusion |
|---|---|
| `FIND-TASK-010-1` | Routed to TASK-011 by standing lead direction; not reviewed as a TASK-010 defect. |
| `FIND-TASK-010-2` | **CLOSED.** The deterministic PostgreSQL interleavings cover deny and expiry/delete winning against approval; redemption remains the only mint point. |
| `FIND-TASK-010-3` | **CLOSED.** A syntactically valid unsupported token-exchange audience is `invalid_target`; malformed exchange remains `invalid_request`. |
| `FIND-TASK-010-4` | **CLOSED.** `client_id` and exact registered `redirect_uri` are uniquely established before non-binding parse failures can redirect; ambiguous binding stays local. |
| `FIND-TASK-010-5` | **CLOSED.** Provider denial consumes the existing state once, applies issuer binding, reflects no provider description, and returns only the registered downstream error. |
| `FIND-TASK-010-6` | **CLOSED.** Basic scheme matching is ASCII case-insensitive while decoding, secret verification, conflicting form-client handling, status, and challenge remain unchanged. |
| `FIND-TASK-010-7` | **CLOSED.** The four cited items carry the required substantive rustdoc without suppression or a new check. |
| `FIND-TASK-010-8` | **CLOSED.** `active_refresh` relies on `TenantConn` RLS and retains hash, revocation, and database-clock expiry checks. |
| `FIND-TASK-010-9` | **CLOSED.** The served document publishes optional form `client_id`, ordinary HTTP Basic, and the public-form/confidential-Basic alternatives on all three client endpoints. |
| `FIND-TASK-010-10` | **OPEN — `SEC-OAUTH-R2-001`.** The limiter is still replica-local and observes the application pod's peer rather than the public edge's client identity in the supported proxied topology. |
| `FIND-TASK-010-11` | **CLOSED.** Only a row marked `rotated` enters replay containment, which reuses `revoke_refresh_chain`; expired/logout/admin/already-contained rows write no containment or theft audit. |
| `FIND-TASK-010-12` | **CLOSED.** Final callback work takes the family lock, then the existing connection-slot lock and exact active-binding predicate before roles, code/approval, audit, and commit. |
| `FIND-TASK-010-13` | **CLOSED.** Every successful non-test callback appends `auth.login` in the final tenant transaction independently of role changes; append failure rolls back all final effects. |

## Startup-Lane Judgment

Changing `test:server:startup` to set `WYRD_SERVER_TENANT_SLUG=acme` and use an owner-only file KEK is a **legitimate lane fixture correction, not a weakened security gate**.

- The lane's declared subject is the official single-image startup journey against one provisioned `acme` tenant; it does not claim to be the multi-tenant Vault qualification lane.
- Explicit single-tenant production with an owner-only file KEK is an approved architecture mode (`architecture/wyrd-design.md:1323-1337`, `config.rs:1726-1811`), not a bypass of a runtime check. `APP_ENV=production` remains exercised.
- The candidate also fixes the real loose-file failure by delivering both keys as mode `0600` files owned by the serving user.
- Multi-tenant production's Vault-only rule and active-key fail-start behavior remain directly covered by `pg_operator_connection_routes::production_boot_requires_every_active_tenant_key`, including unavailable, missing, malformed, wrong-length, and readable Vault results. Requiring the unrelated full Vault environment again in this OAuth gateway proof would not be the narrowest task lane.

The lane is still insufficient evidence for `FIND-TASK-010-10`, but for a different reason: it drives one embedded nginx/Rust pair directly and cannot represent the documented public-edge-to-replica topology.

## Verification Limits

- `git diff --check fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa..7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29` passed.
- Focused checks run during this review passed:
  - `auth::oauth::tests::basic_scheme_matches_case_insensitively`
  - `auth::oauth::tests::token_exchange_audience_is_classified_before_decoding`
  - `auth::oidc::tests::callback_query_carries_exactly_one_provider_response`
- The remediation records green focused PostgreSQL, OpenAPI, SQL, codegen, boundary, lint, startup, and filtered identity lanes. Source inspection, not that table alone, established the closure conclusions above.
- No broad unfiltered identity, every-language, or full change journey was run; those remain change-review work by standing direction.
- The startup limiter assertion proves route selectivity and two peer keys inside one container. It does not prove one real-client budget before replica selection or behavior behind the documented edge gateway/mesh.

## Security Audit

### Critical

None.

### High

None.

### Medium

- **[SEC-OAUTH-R2-001] INCORRECT — `docker/official/extras/nginx/nginx.conf.template:40-49,65-74`: device user-code limiting remains proxy-collapsed and replica-local.**
  - **Violated obligation:** RFC 8628 §5.1 requires limiting attempts against the short user code. R1 `FIND-TASK-010-10` and its acceptance criterion require the existing public gateway's native limiter, keyed by the real client address it observes, with one budget before backend replica selection. The remediation explicitly rejected another replica-local limiter.
  - **Evidence:** the added `limit_req_zone` is inside the official application image. That image starts its own nginx and one loopback Rust server, and every deployed Wyrd pod runs that image. `$binary_remote_addr` therefore names the immediate peer of that pod-local nginx. In the documented Kubernetes production path, the public Istio edge gateway and mesh sidecar reach port 8080 (`kubernetes-production.svx:443-488,499-538`), so the embedded nginx does not own the public client identity. Each application pod also owns a different nginx shared-memory zone. The comment that "backend replicas see one budget" is false for this topology.
  - **Exploit path and impact:** an attacker submits guesses to `POST /auth/device` through the public edge. Replica selection spreads requests over independent per-pod zones, multiplying the allowed guess rate by the number of serving replicas. Where the embedded nginx sees the edge/sidecar peer, unrelated users instead share one key and can exhaust its six-request burst, denying device approval to everyone routed through that peer. Either outcome violates the short-code protection: brute-force resistance is bypassable across replicas, or one caller can throttle unrelated callers.
  - **Testable correction:** put the native device-verification limit at the already documented public edge, before it chooses a Wyrd replica, and key it from that edge's authenticated client address. Remove the pod-local limit from the replicated path so it neither resets per replica nor collapses all edge traffic. Preserve the exact `POST /auth/device` scope; do not add a Wyrd header parser, database/cache limiter, public tuning option, or fleet-coordination service. Focused proof must place at least two Wyrd backends behind that one edge, show one client cannot reset its budget by alternating backends, show a second client remains independent, and show `GET /auth/device` plus `/auth/token` are unaffected.

### Low / Defense In Depth

None. Placement, naming, structure, wording, and optional hardening were not promoted to findings.

### Positive Controls

- Authorization redirects are issued only after unique client and exact registered redirect binding; ambiguous or mismatched binding never produces `Location`.
- Provider error callbacks consume server-bound state once, retain RFC 9207 issuer enforcement, mint no authority, and do not reflect provider descriptions.
- Confidential client Basic authentication accepts case variants without broadening the supported method or weakening wrong-secret refusal.
- Authorization/device codes remain random, digest-only at rest, expiry-bound, one-use, and bound to the server-owned tenant, principal, client, redirect, PKCE, and connection context.
- Refresh replay containment is now chain-scoped and only a rotated predecessor triggers it; unrelated CLI/UI sessions and ordinary inactive rows are preserved.
- Callback lifecycle fencing and `auth.login` audit share the final tenant transaction, so deactivation or audit failure leaves no User/role/code/approval effects.
- OAuth responses retain registered error codes, no-store headers, form-only input, and `401 invalid_client` with the Basic challenge.
- The OpenAPI document now accurately distinguishes Wyrd caller authentication from OAuth public-form and confidential-Basic client identification.
- Startup secret files are mode `0600`, owned by the serving user; the single-tenant file KEK is an explicitly approved deployment mode, while multi-tenant Vault fail-start remains separately tested.
