# System-resilience review — TASK-010 round 2

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Overall result: **FAIL**

The candidate remained at the stated commit while this review inspected the
complete cumulative range and used `04366e7fc28c466fcdcbc7279885cfee82a988a2..7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
to locate the remediation. `FIND-TASK-010-1` is routed to TASK-011 by the
standing lead direction and is not reopened here.

## Deployed-path and failure-path evidence

| Changed runtime path | Deployment and durable boundary | Failure and recovery behavior | Proof assessment |
|---|---|---|---|
| Authorization and provider callback | Login state, the resolved User and roles, authorization code or device approval, and audit staging remain in Postgres. Final callback completion takes the User-family lock, then the existing human-connection slot lock and exact active-revision check before committing (`crates/wyrd/wyrd-auth/src/callback.rs:367-445`, `:623-638`). | Provider refusal consumes the one-time state and returns the registered downstream refusal. A connection retired before final commit rolls back User, role, code/approval, and login-audit effects. Database or required audit failure fails the login attempt closed without crashing the process; a new login can start after provider/database recovery. | The remediation evidence names provider-denial, connection-cutoff, callback-audit, and rollback tests, including a two-replica cutoff journey. This closes the prior callback lifecycle and missing-outcome defects. |
| Device approval and redemption | The device row is the shared authority. Approval now loses if deny or expiry/delete terminates that row before completion; redemption locks the row, deletes it, issues, audits, and commits in one tenant transaction (`crates/wyrd/wyrd-auth/src/cli_logins.rs:192-216`, `:276-384`). | A process loss or cancellation before commit releases the transaction and leaves no partial issued credential. Pending and `slow_down` states remain retryable; terminal denial/expiry deletes the row. A committed redemption survives replica restart and cannot issue twice. | Focused deterministic race tests and the exactly-once test are recorded as passing. The external user-code admission boundary remains defective as `SYS-R2-1`. |
| Refresh and revocation | Refresh rows and rotation links remain in Postgres under the existing principal-family advisory lock. Only a predecessor marked `rotated` starts replay containment, and containment uses the existing chain traversal; logout/admin/expired/already-contained rows do not revoke unrelated chains (`crates/wyrd/wyrd-auth/src/refresh.rs:137-239`). Revocation uses that same family lock and chain owner (`crates/wyrd/wyrd-auth/src/cli_logins.rs:408-453`). | Database, issuance, or required audit failure rolls the transaction back. A committed replay contains only the compromised chain; independent CLI/UI sessions remain available. Connection replacement/deactivation still fences renewal. | The remediation evidence records focused inactive-row, chain-only containment, concurrent replay/rotation, deactivation overlap, and revocation journeys as passing. |
| OAuth request classification and public contract | Token, authorize, callback, device, and revoke routes remain ordinary stateless HTTP handlers over shared server owners. The candidate adds standard `invalid_target`, provider error callback handling, case-insensitive Basic scheme matching, safe duplicate-parameter redirect classification, and OpenAPI client-identification declarations without a new durable owner. | Malformed or unauthenticated requests fail at the request boundary; no process-wide failure or recovery loop is introduced. Provider/database outages remain request-scoped. | Focused route, schema, served-OpenAPI, codegen, and integration evidence is recorded as passing. |
| Device verification admission | The Rust-local all-auth governor was removed. The official image's bundled NGINX now limits only `POST /auth/device` by `$binary_remote_addr` (`docker/official/extras/nginx/nginx.conf.template:40-74`). | In one official-image container, excess attempts from one address receive 429 while other routes and another address remain available. In a multi-replica deployment, each image has a separate NGINX shared-memory zone, so distributing requests across replicas resets the budget. | The one-container startup lane proves route and client separation, but cannot prove the required fleet property. See `SYS-R2-1`. |

## Affected capabilities and recovery assessment

- Postgres remains the cross-replica authority for login state, codes, device
  approvals, refresh chains, connection lifecycle, and audit staging. No new
  pod-local credential authority was introduced.
- Callback, device, refresh, and revoke failures remain request-scoped and
  transactional. Dependency recovery permits a retry or a fresh login without
  restarting unrelated Wyrd capabilities.
- Removing the broad in-process governor restores availability of metadata,
  authorize, callback, token, refresh, revoke, and key issuance under unrelated
  device-code abuse.
- The replacement limiter is correct only for a single official-image
  instance. It does not establish one admission budget at the logical gateway
  in the supported multi-replica topology.

## Startup-lane judgment

The switch in `test:server:startup` to `WYRD_SERVER_TENANT_SLUG=acme` plus an
owner-only file KEK is a **legitimate fixture repair, not a weakened gate**.
The lane promises an official-image production-profile restart and persisted
client journey; it never claimed a multi-tenant Vault journey. At the immutable
base it configured neither a valid production Operator key source nor a Vault,
so the former no-slug/default-env combination was an invalid fixture, not
credible passing proof of multi-tenant production. The candidate now uses a
supported enterprise/single-tenant production topology, retains the production
profile restart and every prior schema, role, route, persistence, and SDK
assertion, and fixes the separate owner-only signing-key defect by writing both
keys as the image user (`scripts/server/test-startup.sh:53-63`, `:106-125`,
`:207-234`). Multi-tenant Vault validation and boot preflight have their own
focused config/key-owner and Postgres tests; making this narrow gateway lane
stand up Vault would broaden it without improving the device-route proof.

This judgment does not validate the limiter's replica behavior. The startup
lane explicitly has one application container and its second-address probe is
another connection to that same NGINX (`scripts/server/test-startup.sh:190-203`).

## Material proposed findings

### SYS-R2-1 — the bundled NGINX limiter is still replica-local

- **Classification:** INCORRECT
- **Violated obligation:** `FIND-TASK-010-10` requires the RFC 8628
  user-code verification limit at the existing gateway boundary across
  replicas and client addresses. Repository deployment authority defines one
  logical gateway in front of one or more Wyrd replicas and assigns rate-limit
  integration to that gateway
  (`architecture/operations/deployment-and-release.md:8-24`).
- **Location:** `docker/official/extras/nginx/nginx.conf.template:25-27,
  40-49,65-74`; `docker/official/Dockerfile:3-12,35,45-52`;
  `scripts/server/test-startup.sh:190-203`.
- **Evidence:** The official image contains its own NGINX and configures its
  only Rust upstream as `${WYRD_SERVER_BIND}`, whose image default is
  `127.0.0.1:8081`. Consequently every replicated application image owns a
  different `limit_req_zone`; NGINX shared memory is shared only among workers
  of that one NGINX instance, not among containers or pods. The comment that
  “backend replicas see one budget” is false for this topology. The startup
  proof starts one application container and acknowledges that it cannot show
  two replicas, so its green result cannot close the required deployment path.
- **Observable system consequence:** An attacker can multiply permitted
  short-user-code guesses by the number of replicas (and by replica churn or
  load-balancer selection). The effective limit changes during scaling and
  rolling replacement, so the RFC 8628 protection is not stable at the
  logical service boundary.
- **Smallest testable correction:** Put the same route-scoped, per-client
  native rate limit on the actual logical edge gateway that selects among
  replicas, using that gateway's observed client address, and remove the
  pod-local claim/mechanism where it would create independent budgets. Reuse
  the supported gateway's conventional limiter; add no Wyrd header parser,
  database limiter, cache, public option, or coordination service. Preserve
  the single-instance official-image behavior through its logical gateway.
- **Focused closure proof:** Send one client's `POST /auth/device` attempts
  through one logical gateway while alternating between two real backends and
  prove one shared allowance; prove another client and `GET /auth/device` plus
  `/auth/token` remain unaffected. Use the narrow gateway/deployment lane only;
  full identity journeys remain change-review evidence.

## Verification assessment

The remediation record supplies passing narrow owner lanes and focused tests
for callback fencing/audit, device terminal races, refresh containment, OAuth
classification, OpenAPI, code generation, SQL, and the one-container startup
path. Full journeys are correctly deferred to change review. No missing broad
lane is treated as a task-review defect.

The available proof cannot establish the required multi-replica admission
property because the implemented limiter's state is physically owned by each
replicated image. That reachable production failure makes this system review
**FAIL**.
