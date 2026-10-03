# TASK-010 R2 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Prior candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- Prior verdict and ledger: `changes/active/oidc-production-readiness/review/TASK-010-r1/verdict.md` and `findings-validation.md`
- Standing direction: `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

The candidate remained at the stated commit throughout this review. The
repository has no `.codegraph/` directory. I reviewed the complete
base-to-candidate range and used the prior-to-candidate diff only to locate the
R1 corrections. `FIND-TASK-010-1` is routed to TASK-011 and was not reopened.

## State and invariant trace

- The authorize request first establishes one registered `wyrd-ui` client and
  exact redirect. Duplicate or invalid non-binding parameters then return
  through that registered redirect, while an ambiguous client or redirect stays
  local.
- The callback wire now carries exactly one provider code or provider error.
  Both consume the same server-bound state. Provider refusal neither selects a
  tenant from the query nor creates authority, and only server-owned client
  state reaches the downstream redirect.
- A successful callback resolves the `(issuer, subject)` User, takes the
  tenant-qualified principal-family lock, then the existing connection-slot
  lock, rechecks the exact active connection revision, replaces mapped roles,
  records a code or live device approval, appends `auth.login`, and commits all
  of those effects together. A lifecycle mutation or audit failure rolls the
  final transaction back.
- Device approval still creates no credential. The new deterministic
  interleavings park approval on the existing device/login-state uniqueness
  boundary, let denial or expiry/deletion win, then prove callback completion
  leaves no User, role, code, approval, refresh row, or login audit.
- Public-client refresh still serializes on the tenant/principal family lock.
  Only a predecessor marked `rotated` is classified as replay; containment uses
  the existing `rotated_from` chain. Expired, logged-out, administratively
  revoked, and already-contained rows return the ordinary inactive-token
  refusal without revoking sibling CLI chains or confidential UI sessions.
- OAuth request classification preserves RFC 8693 `invalid_target`, Basic
  scheme matching is case-insensitive, active confidential refresh relies on
  `TenantConn` RLS, and served OpenAPI now describes public `client_id` and
  confidential HTTP Basic alternatives.
- The application-level all-auth governor and its retry masking are deleted.
  The replacement `limit_req` is in the NGINX bundled into every official Wyrd
  application image. In the supported Kubernetes topology, the real Istio edge
  gateway selects a pod before the request reaches that pod-local NGINX. Thus
  each replica has a separate zone and `$binary_remote_addr` is the edge
  gateway hop, not the public client. The original replica-bypass and
  proxy-collapse invariant is therefore not closed.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| RFC 6749 authorization endpoint safely returns post-binding request errors to the registered client | `crates/wyrd/wyrd-server/src/auth/authorize.rs:75-143,213-245` validates unique client/redirect before `OAuthForm::parse` and preserves only an unambiguous state | `duplicate_parameters_redirect_back_to_the_registered_client`; `an_unregistered_redirect_is_never_followed`; recorded principals lane | PASS |
| Provider denial consumes the existing state and completes the downstream authorization attempt without authority | `wyrd-spec/src/auth/oidc.rs` `CallbackQuery::response`; `wyrd-auth/src/callback.rs:172-239,733-744`; route callback adapter | `callback_query_carries_exactly_one_provider_response`; `a_provider_error_consumes_state_and_refuses_to_the_client`; filtered callback journey | PASS |
| REQ-009 / AC-007: hashed, 60-second, single-use authorization code remains bound to tenant, principal, client, exact redirect, connection revision, and S256 verifier | Existing code issue/redeem owners remain; R1 fences final callback through the exact active connection | Existing refusal/replay journey evidence plus `tenant_connection_session_cutoff_journey` | PASS |
| RFC 6749 §2.3.1 confidential client authentication accepts HTTP authentication-scheme case variants without adding another method | `wyrd-server/src/auth/oauth.rs` splits the header once and uses `eq_ignore_ascii_case("basic")`, retaining the existing Base64, form decoding, digest, and client checks | `basic_scheme_matches_case_insensitively`; recorded principals lane | PASS |
| RFC 8693 unsupported target is `invalid_target`, while malformed exchange remains `invalid_request` and supported audiences are unchanged | `OAuthErrorCode::InvalidTarget`; `OAuthForm::token_request` classifies a present token-exchange audience before generic decode | Contract response test and `token_exchange_audience_is_classified_before_decoding` | PASS |
| Routed device terminal race: denial or expiry/delete winning during approval leaves no authority; one live approval issues once | Existing production insert/approval/redemption owners; deterministic uniqueness-lock interleavings in `wyrd-auth/src/cli_logins.rs:900-1123` | `a_denial_during_approval_wins`; `an_expiry_deleted_during_approval_wins`; existing exact-once tests | PASS |
| REQ-012 / RFC 9700: only replay of a rotated predecessor triggers theft containment, and containment affects only that login chain | `wyrd-auth/src/refresh.rs:103-187`; existing `revoke_refresh_chain`; inactive non-rotated rows return `NotFound` without writes | `rotated_replay_revokes_only_its_chain`; `inactive_rows_are_refused_without_containment`; overlapping-rotation test | PASS |
| REQ-016: connection lifecycle mutation winning before callback commit leaves no User, roles, code, approval, or successful-login audit | `finish_id_token_exchange` takes family then slot lock and calls `human_connection_is_active` in its final tenant transaction | Callback Postgres rollback cases and recorded multi-replica connection cutoff journey | PASS |
| REQ-017: every successful non-test callback records one canonical login outcome transactionally and role-sync remains separate | `LOGIN_OPERATION`; `login_event`; `append_auth_audit` before the final commit | `finish_issues_seals_and_audits_the_session`; `an_unchanged_role_device_login_is_audited_once`; `a_failed_login_audit_rolls_back_the_whole_login` | PASS |
| Tenant isolation uses the existing RLS owner rather than a duplicate selector | `ACTIVE_REFRESH_SQL` and `active_refresh` removed the manual tenant predicate and bind while retaining `TenantConn` | `active_refresh_resolves_only_this_tenants_active_row`; recorded tenant-isolation and SQL lanes | PASS |
| Served OpenAPI exposes the actual public-form and confidential-Basic client identification alternatives | `ClientForm<T>`; `oauthClientBasic` security scheme; token/device/revoke operation security alternatives | Served OpenAPI contract tests and recorded principals lane | PASS |
| RFC 8628 §5.1: user-code verification attempts are limited at the real gateway boundary across supported replicas and by the public client address, without throttling sibling auth routes | `docker/official/extras/nginx/nginx.conf.template:40-74` keys `POST /auth/device` on `$binary_remote_addr`, but `docker/official/Dockerfile:3,35,51-52` packages that NGINX inside each application replica; the supported edge gateway routes to pod port 8080 (`kubernetes-production.svx:499-565`) | `test:server:startup` exercises one application container and two direct TCP peers only (`scripts/server/test-startup.sh:190-203`); it cannot prove the supported edge-gateway/multi-replica path | **FAIL — INV-REV-001; prior `FIND-TASK-010-10` remains open** |
| Production startup lane keeps its declared one-tenant official-image journey valid without weakening a task assertion | The lane creates only tenant `acme`; it now declares `WYRD_SERVER_TENANT_SLUG=acme`, supplies the architecture-approved file KEK, and mounts both keys owner-only (`test-startup.sh:53-63,116-125`) | Recorded `mise run test:server:startup` exit 0; config tests retain the multi-tenant Vault-only rule | PASS — legitimate lane repair, not a weakened gate |
| Required Rust documentation on the four prior OAuth items | Substantive field, associated-type, and `# Errors` documentation in the cited owners | Recorded `mise run lints` | PASS |
| Form-only OAuth wire, RFC error/success bodies, metadata, revocation, API-key exchange, JWT bearer, delegation, and workload semantics remain with their existing owners | Cumulative diff introduces no alternate issuer, tenant selector, token store, audit path, JSON route, or legacy API-key grant | Recorded focused identity, principals, SQL, codegen, boundary, format, and lint lanes | PASS, except the user-code limiter above |
| Deleted private BFF/browser-session/sealed-completion protocols remain deleted with no compatibility alias | Complete cumulative source and route diff | SQL/codegen evidence and source inspection | PASS |
| TASK-011 owns the real `openid-client` BFF journey | Standing lead direction explicitly routes `FIND-TASK-010-1`; no duplicate test-only client was added here | Integrated proof remains for TASK-011/change review | PASS / routed; not reopened |

## Proposed finding

### INV-REV-001 — INCORRECT: the device limiter remains proxy-collapsed and replica-local in the supported topology

- **Prior finding:** This is failed closure of `FIND-TASK-010-10`, not a new
  obligation.
- **Violated obligation:** The R1 acceptance criterion requires conventional
  user-code attempt limiting at the existing gateway across replicas and client
  addresses. Architecture assigns rate-limit integration to the one logical
  gateway in front of the serving fleet. RFC 8628 §5.1 requires limiting
  attempts against the short user code.
- **Exact location:** `docker/official/extras/nginx/nginx.conf.template:40-74`
  adds the zone to the NGINX bundled by
  `docker/official/Dockerfile:3,35,51-52`. The supported Kubernetes route sends
  the edge gateway to Service port 8080 and then a Wyrd pod
  (`docs/src/content/docs/self-hosting/kubernetes-production.svx:499-565`).
  `scripts/server/test-startup.sh:190-203` runs one such image directly.
- **Evidence:** Replica selection happens at the Istio edge gateway before the
  selected pod's image-local NGINX sees the request. Its shared-memory zone is
  shared only by workers in that one container, so another Wyrd replica starts
  a fresh budget. On the gateway-to-pod hop, `$binary_remote_addr` is the edge
  gateway address, so different public clients routed through that gateway
  share the same key. The configuration comment claiming backend replicas see
  one budget is false for the documented topology. The startup test's host and
  in-container loopback requests prove two direct peers to one container, not
  public clients through the edge gateway or one budget across replicas.
- **Observable consequence:** A caller can multiply allowed guesses by the
  number of serving replicas, while ordinary users behind the same edge
  gateway can throttle one another on any selected replica. Rolling replacement
  resets the per-container budget. The exact production failure retained by
  `FIND-TASK-010-10` therefore remains reachable.
- **Required testable correction:** Put the standard device-verification rate
  limit on the existing public edge-gateway integration, before it selects a
  Wyrd replica, keyed by the client identity/address that gateway natively
  observes. Remove the duplicate image-local limiter. Preserve the current
  route-only scope and the token endpoint's durable polling cadence. Prove the
  rendered edge-gateway configuration selects only `POST /auth/device` and
  uses the gateway's native client key; keep full multi-replica journeys for
  change review. Do not add a Wyrd header parser, database limiter, distributed
  cache, public setting, or new rate-limit service.

## Prior-finding closure

| Prior finding | Closure assessment |
|---|---|
| `FIND-TASK-010-1` | Routed to TASK-011 by standing lead direction; not reopened. |
| `FIND-TASK-010-2` | Closed by deterministic deny and expiry/delete interleavings at the existing device owner. |
| `FIND-TASK-010-3` | Closed by registered `invalid_target` classification. |
| `FIND-TASK-010-4` | Closed by unique binding validation before non-binding request parsing. |
| `FIND-TASK-010-5` | Closed by the success-or-error callback wire and one-time state owner. |
| `FIND-TASK-010-6` | Closed by case-insensitive Basic scheme matching. |
| `FIND-TASK-010-7` | Closed by the mandatory rustdoc additions. |
| `FIND-TASK-010-8` | Closed by deleting duplicate tenant selection from `active_refresh`. |
| `FIND-TASK-010-9` | Closed by served public-form/Basic OpenAPI alternatives. |
| `FIND-TASK-010-10` | **Open.** The mechanism moved from Rust to the per-replica image proxy, not to the public edge gateway. |
| `FIND-TASK-010-11` | Closed by rotated-only classification and existing chain containment. |
| `FIND-TASK-010-12` | Closed by the final-transaction connection-slot fence. |
| `FIND-TASK-010-13` | Closed by transactional `auth.login` evidence. |

## Non-blocking notes

- Changing `test:server:startup` to explicit single-tenant production with a
  file Operator KEK is not itself a weakened gate. The lane provisions only
  `acme`, the official self-hosted deployment supports this mode, file KEKs are
  expressly allowed there, and owner-only key delivery fixes the actual secret
  permission failure. Requiring a Vault-backed multi-tenant fixture in this
  task would expand the narrow remediation beyond the lane's declared journey.
- Placement, naming, structure, wording, and the inaccurate NGINX comment alone
  are non-blocking. `INV-REV-001` blocks because the same production security
  consequence remains reachable, not because the limiter lives in a particular
  file.

## Verification assessment

The remediation records green exact tests and the narrow owner lanes required
by the task: principals integration, SQL, codegen, client-tier,
tenant-isolation, unwrap audit, format, lints, filtered identity journeys, CLI
journey, and `test:server:startup`. `git diff --check` also passed during this
review. These results credibly close the callback, refresh, device-state,
contract, RLS, audit, and documentation findings.

The startup lane is credible for its one-container official-image contract but
cannot prove the deployed rate-limit invariant because it omits the documented
edge gateway and additional application replicas. That is source-established
failed closure, not a request for a full task-level journey. Full integrated
journeys remain correctly deferred to change review.

## Overall result

**FAIL**

Twelve R1 findings are closed or routed, including the authorization, callback,
device, refresh, RLS, OpenAPI, lifecycle, and audit invariants. The device
user-code limiter still occupies a per-replica downstream proxy and keys the
edge-gateway hop, so `FIND-TASK-010-10` remains open.
