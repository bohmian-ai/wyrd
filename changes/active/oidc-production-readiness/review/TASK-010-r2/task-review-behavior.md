# TASK-010 R2 behavior review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Prior candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`

The candidate remained at the stated commit before and after review. `.codegraph/`
is absent, so source navigation used repository-native search and Git diffs.
`FIND-TASK-010-1` was not reopened: the standing lead direction routes its
`openid-client` proof to TASK-011 and integrated change review.

The cumulative review followed the public paths from authorization and device
initiation through callback, token redemption, refresh, revocation, OpenAPI,
the official image, and the documented public deployment gateway. The
remediation diff was used only to locate the R1 corrections.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Authorization code + S256, exact registered redirect, hashed 60-second one-use code, client/tenant/principal binding | `wyrd-server/src/auth/authorize.rs`; `wyrd-auth/src/callback.rs::redeem_code`; login-state SQL owner | Recorded filtered `tenant_human_login_journey` and `tenant_callback_refusal_journey` | PASS |
| `openid-client` 6.8.8 real-server interoperability | Standing direction in `review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md` assigns the production BFF client and proof to TASK-011 | Runs at TASK-011/change review, not reopened here | PASS / routed |
| Wrong, expired, replayed or mismatched code/PKCE/redirect and wrong secret issue no token | Redemption deletes and validates the code inside the tenant transaction; `OAuthClients` owns confidential authentication | Recorded callback-refusal journey and OAuth unit tests | PASS |
| Device callback stores approval only; a live device grant issues once at redemption | `CliLogins::redeem_in` locks, revalidates, deletes, issues, audits, and commits; callback uses `approve_device_authorization` only | Recorded device refusal journey and exactly-once SQL tests | PASS |
| Routed pause/terminal/resume device race closes `FIND-TASK-004-12` | `cli_logins.rs::pg_tests::approval_racing` parks `CliLogins::approve` at the existing login-state uniqueness boundary, then deny or expire/delete wins before callback completion | Recorded `a_denial_during_approval_wins` and `an_expiry_deleted_during_approval_wins` | PASS |
| RFC 8628 pending, slow-down, denial and expiry responses | Existing device state owner and token route remain the only path | Recorded device SQL and filtered identity journeys | PASS |
| Public refresh rotates; rotated replay contains only that chain; non-rotated inactive rows do not trigger theft containment | `RefreshTokens::execute` re-reads under the family lock and calls existing `revoke_refresh_chain` only for `revoked_reason = 'rotated'` | Recorded `rotated_replay_revokes_only_its_chain`, `inactive_rows_are_refused_without_containment`, and overlapping-rotation test | PASS |
| Confidential `wyrd-ui` refresh does not rotate and stops at its absolute lifetime | `active_refresh` branch under the family lock; database-clock expiry | Recorded human login and refresh tests | PASS |
| RFC 8693 unsupported audience returns `invalid_target`; malformed exchange remains `invalid_request` | `OAuthErrorCode::InvalidTarget`; `OAuthForm::token_request`; both tenant and platform routes use it | Independently reran exact server/spec tests; both passed | PASS |
| Safe non-binding authorization errors redirect only after unique registered client and redirect binding | `authorize.rs::unique_param` establishes the safe redirect before `OAuthForm::parse`; ambiguous binding fields stay local | Recorded authorize PostgreSQL tests | PASS |
| Provider denial consumes state once and returns a downstream error without authority | `CallbackQuery::response`; `ProviderResponse`; `AuthorizationCodeExchange::complete` consumes the canonical state then produces `LoginCompletion::Refused` | Independently reran callback-query contract test; recorded provider-error callback test and filtered journey | PASS |
| RFC 6749 `client_secret_basic` accepts case-insensitive authentication-scheme tokens | `OAuthClients::identify` uses `eq_ignore_ascii_case` and the existing credential decoder/check | Independently reran `basic_scheme_matches_case_insensitively`; passed | PASS |
| `TenantConn` RLS remains the only selector for `active_refresh` | `ACTIVE_REFRESH_SQL` has no manual tenant predicate or second binding | Recorded cross-tenant active-refresh SQL test and `test:sql` | PASS |
| Served OpenAPI describes public `client_id` and confidential Basic alternatives | `ClientForm<T>`, `oauthClientBasic`, and the token/device/revoke operation declarations | Recorded served-document tests and `test:principals:integration` | PASS |
| RFC 8628 user-code verification attempts are limited at the shared public gateway without limiting unrelated auth routes | The candidate adds an NGINX shared-memory zone inside each official application image (`docker/official/extras/nginx/nginx.conf.template:40-74`). The official image contains that NGINX (`docker/official/Dockerfile:3-36`), while supported deployments place image pods behind a separate public gateway (`architecture/v1/04-surfaces/deployment.md:3-7`; `docs/.../kubernetes-production.svx:499-538`). | `test:server:startup` proves one image-local NGINX only and expressly has one backend; it cannot prove shared admission across replicas or public-client separation behind the outer proxy. | **FAIL — `BEHAVIOR-TASK-010-R2-1`** |
| Callback completion loses to connection deactivation/replacement and leaves no user/role/code/approval effect | Final tenant transaction takes family lock, then existing connection-slot lock, then rechecks the exact active binding before roles or completion commit | Recorded multi-replica cutoff journey and callback SQL tests | PASS |
| Every successful non-test callback records one canonical login outcome and audit failure rolls back the callback | `append_auth_audit(login_event)` is in the final transaction after code/approval creation; connection-test path remains excluded | Recorded unchanged-role device and audit-failure rollback tests | PASS |
| RFC 7009 unknown-token success and login-local revocation | Existing revoke owner and standard form route remain intact | Recorded grant and journey lanes | PASS |
| Form-only requests, RFC success/error bodies, no-store, and `invalid_client` challenge | `OAuthForm`, `OAuthError`, `no_store`, and route adapters remain shared | Recorded principals integration and exact OAuth tests | PASS |
| RFC 8414 metadata and existing RFC 7523, API-key, delegation and workload semantics | Static metadata and existing grant dispatch owners remain in place | Recorded metadata, platform, CLI, identity, and workload lanes | PASS |
| Invented BFF channel, browser sessions, sealed completion, related migrations, and compatibility routes stay deleted | Cumulative diff deletes the named owners and does not add an alias | Recorded SQL migration and compile/check lanes | PASS |
| No change to tenancy, token contents, role mapping, audit ownership, or durable authority | Corrections reuse `TenantConn`, canonical audit, connection locks, existing issuer, and existing refresh-chain query | Source trace plus recorded tenant-isolation, SQL, principals and journey lanes | PASS, except the deployment-boundary failure above |
| No unrequested grant, option, store, limiter service, retry mask, compatibility path, or new dependency | Server-local `tower_governor` and its dependencies are deleted; the client retry mask is removed | Cumulative dependency/source inspection | PASS |
| Mandatory rustdoc correction | The four cited R1 items now document their field/associated type and fallible behavior | Recorded `mise run lints` | PASS |

## Startup-lane judgment

Changing `test:server:startup` to set `WYRD_SERVER_TENANT_SLUG=acme`, use the
supported file operator KEK, and mount owner-only key files is a legitimate
lane repair, not a weakened gate under `AGENTS.md` section 12. The lane has
always created and exercised exactly one `acme` tenant and its declared contract
is the official image, external Postgres, migration/setup, routes, SDK traffic,
production-profile restart, and persistence (`scripts/server/test-startup.sh:1-18`).
It never claimed a multi-tenant SaaS/Vault topology. The previous absence of a
tenant slug accidentally selected multi-tenant validation, while the 0644 bind
mount contradicted the existing owner-only secret rule. The remediation records
both diagnoses, and the new file source is an already-supported production
choice for an explicitly single-tenant deployment.

That repair does not make the lane evidence sufficient for the new rate-limit
claim. The lane starts one application image, so its green result proves only
one image-local NGINX zone. This is a proof gap attached to the incorrect
deployment boundary in `BEHAVIOR-TASK-010-R2-1`, not a reason to restore the
accidental multi-tenant/Vault setup in this lane.

## Proposed findings

### BEHAVIOR-TASK-010-R2-1 — INCORRECT — the device limiter is still replica-local and proxy-collapsed in the supported public topology

- **Violated obligation:** TASK-010's RFC 8628 section 5.1 requirement and the
  R1 acceptance criterion for `FIND-TASK-010-10` require conventional
  user-code attempt limiting at the existing public gateway across backend
  replicas and client addresses, without sharing a bucket with unrelated auth
  routes.
- **Exact location:**
  `docker/official/extras/nginx/nginx.conf.template:40-74`,
  `docker/official/Dockerfile:3-36`, and
  `scripts/server/test-startup.sh:190-203`. The supported production topology
  puts the official-image pods behind the separate edge Gateway and Service at
  `docs/src/content/docs/self-hosting/kubernetes-production.svx:499-538`.
- **Evidence:** `limit_req_zone` is process/shared-memory state in the NGINX
  embedded in each application image. Scaling application images creates one
  independent zone per replica. In the documented proxied deployment,
  `$binary_remote_addr` is the immediate proxy hop unless NGINX is configured
  to recover a trusted client address; this template has no `real_ip_header`,
  trusted proxy, or equivalent edge ownership. The startup test reaches one
  image directly and its “second client” is an in-container loopback request,
  so it proves neither behavior behind the public edge nor two replicas.
- **Observable consequence:** public callers can be collapsed into one bucket
  behind the edge proxy, while requests distributed to another application
  replica receive a fresh budget. The short device user code therefore lacks
  the required effective brute-force admission in a supported topology.
- **Required testable correction:** enforce the user-code attempt limit at the
  actual deployment-owned public edge that observes the client and selects the
  backend, using that gateway's conventional native rate-limit facility. Keep
  the limit scoped only to `POST /auth/device`; remove the false claim that an
  image-local zone is shared by backend replicas. Prove through one public
  entry that repeated attempts remain limited while traffic can reach two
  backend replicas, a distinct client retains its own budget, and token/GET
  routes remain unaffected. Do not add an application limiter, database state,
  distributed cache, trusted-header parser, or public Wyrd option.

## Non-blocking notes

None. Placement, naming, structure, and wording-only observations were not
promoted to findings.

## Verification assessment

- Independently ran the exact `wyrd-server` OAuth audience and Basic-scheme
  tests: 2 passed.
- Independently ran the exact `wyrd-spec` callback-query and OAuth-error tests:
  2 passed.
- `git diff --check base..candidate` passed.
- The candidate records green narrow owner lanes for principals integration,
  SQL, code generation, client tier, tenant isolation, unwrap audit, format,
  lints, the startup image, and the listed filtered identity journeys.
- Full unfiltered and cross-language journeys remain change-review work, as
  directed. Their absence is not a task-review limit.

## Overall result

**FAIL**

All reviewed R1 corrections except the gateway-rate-limit boundary are closed
in behavior and focused proof. `BEHAVIOR-TASK-010-R2-1` leaves the supported
device-verification path without effective per-client admission across the
documented public proxy/replica topology.
