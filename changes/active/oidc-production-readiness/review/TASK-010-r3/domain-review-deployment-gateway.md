# TASK-010 R3 deployment and gateway domain review

## Subject and reviewed boundary

Reviewed the immutable cumulative range
`fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa..1f4466a9ae482eb1311f6e5206484758bc272ad8`
for TASK-010's deployed edge, official image, startup lane, and operator
documentation. The focused remediation delta is
`7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29..1f4466a9ae482eb1311f6e5206484758bc272ad8`.

The controlling remediation authority is
`review/TASK-010-r2/lead-direction-FIND-TASK-010-10.md`. It supersedes
`TASK-010-R2-public-edge-device-admission.md`: Wyrd must delete the ineffective
image-local limiter and its startup assertions, keep the accepted owner-only
key and single-tenant startup corrections, and tell operators to rate-limit
`POST /auth/device` per client address at their public ingress. It expressly
forbids an application limiter, forwarded-header parser, edge manifests,
settings, or state. `FIND-TASK-010-1` remains routed to TASK-011 and was not
reopened.

`.codegraph/` is absent, so this review used Git history, the complete diff,
and repository source directly. Candidate `HEAD` was
`1f4466a9ae482eb1311f6e5206484758bc272ad8` before and after inspection.

## Boundary, authority, and source coverage

| Boundary | Authority and source inspected | Assessment |
|---|---|---|
| Rate-limit ownership | `architecture/operations/deployment-and-release.md:6-24`; `architecture/v1/04-surfaces/deployment.md:1-18`; controlling lead direction lines 3-28 | The public gateway owns rate-limit integration, but the operator owns that gateway and Wyrd ships no rate-limit manifest. Documentation of the endpoint and key is the conventional boundary fixed by the lead direction. |
| Official-image request path | `docker/official/Dockerfile:1-12,23-52`; `docker/official/extras/entrypoint.sh:1-46`; `docker/official/extras/nginx/nginx.conf.template:1-81` | The image still bundles NGINX only as the local API/UI/gRPC proxy. The prior `map`, `limit_req_zone`, `limit_req_status`, and `limit_req` configuration is fully deleted; no image-local device admission mechanism remains. |
| Public-ingress topology | `docs/src/content/docs/self-hosting/kubernetes-production.svx:499-568`; `docs/src/content/docs/self-hosting/docker.svx:9-22` | The supported replicated path has one operator-controlled public edge before Service/backend selection. The image NGINX is not that edge, so deleting its per-process budget closes the diagnosed proxy-collapse and replica-local behavior without inventing a Wyrd-owned edge surface. |
| RFC 8628 user-code entry | TASK-010 lines 29-60, 98-118, 166-180; `crates/wyrd/wyrd-server/src/components/auth/routes.rs:43-68`; `crates/wyrd/wyrd-server/src/auth/cli_login.rs:125-203`; lead direction lines 16-26 | `POST /auth/device` is the anonymous verification-page form that accepts the short user code. The operator note names this exact route, RFC 8628 section 5.1, the per-client-address key, and the public-ingress placement. Existing in-server entropy, expiry, one-use redemption, and token-poll `slow_down` remain separate protections. |
| Startup-lane preservation | `scripts/server/test-startup.sh:1-219`; R2 `followup-review.md`; R2 `verdict.md`; lead direction lines 16-20 | Only the obsolete device-limit assertions and their case-list wording were removed in R3. The accepted owner-only signing/KEK volume, `0600` key delivery, explicit single tenant, migration refusal/retry, image routing, and production restart checks remain. |
| Scope and drift | Focused remediation diff; `git grep` over `docker/official`, `scripts/server`, `docs`, `architecture`, and auth route source | The implementation adds no application limiter, trusted-header logic, rate-limit dependency, manifest, setting, durable state, or second admission mechanism. The only product-tree addition is the operator note; the other changes delete the rejected mechanism and proof. |

## Closure assessment

| Controlling obligation | Source evidence | Result |
|---|---|---|
| Delete the image NGINX device limit | `docker/official/extras/nginx/nginx.conf.template:35-68` contains only the existing upgrade map and proxy routes; the focused diff deletes all 13 device-limit lines. Repository search finds no remaining `device_verification_client`, device `limit_req_zone`, device `limit_req_status`, or device `limit_req`. | PASS |
| Delete only the startup assertions for that limit | `scripts/server/test-startup.sh:11-14,168-190`; the focused diff deletes the attempt loop and cross-route/cross-address assertions and removes only that behavior from the case comment. | PASS |
| Preserve accepted owner-only key and single-tenant fixes | `scripts/server/test-startup.sh:25,52-62,115-124,213-216` retains the dedicated key volume, `WYRD_SERVER_TENANT_SLUG=acme`, file KEK, owner-only key creation, and production restart. | PASS |
| Document operator ingress limiting | `docs/src/content/docs/self-hosting/sso-and-oidc.svx:19-24` tells the operator to rate-limit `POST /auth/device`, identifies it as RFC 8628 section 5.1 user-code entry, specifies per client address, and places the control at the public ingress. | PASS |
| Add no replacement mechanism or configuration | Focused product diff is limited to the NGINX deletion, startup-test deletion, and two-line documentation note. The fourth changed file is the controlling lead record. | PASS |

## Deployed-path and recovery assessment

The corrected deployed path is client -> operator public ingress -> selected
Wyrd replica -> bundled NGINX -> loopback Rust server. Device-entry admission
now belongs only to the first hop, which observes the client address and sits
before backend selection. Wyrd no longer claims or implements a shared budget
inside independent application images. A replica restart or replacement
therefore cannot reset a Wyrd-owned device limiter because no such limiter
exists; the operator's ordinary ingress control has its own deployment
lifecycle.

Removing the image rule does not change process startup, readiness, routing,
or recovery. The official image retains its existing local proxy, and the
startup lane still exercises image boot, migration refusal and retry, API/UI/
MCP routing, owner-only mounted keys, persistence, and a production-profile
restart. No unrelated capability now depends on a new limiter, service, store,
or configuration option.

## Verification evidence and limits

- `mise run docs:check` was run during this review and exited `0`, including
  generation, command checks, link checks, the production docs build, and
  accessibility checks.
- The controlling lead record reports `mise run test:server:startup` passing
  after the focused correction. Source inspection confirms that the lane still
  covers its accepted image/startup responsibilities and no longer purports to
  prove operator-owned ingress admission.
- Static diff and repository search establish deletion of the rejected NGINX
  mechanism and absence of a replacement application limiter, edge manifest,
  setting, or state.
- This review did not run full identity, multi-replica, or every-language
  journeys. They remain change-review evidence under the standing
  narrowest-lane direction. Wyrd does not ship the operator ingress, so an
  in-repository edge-manifest or cross-replica rate-limit journey is neither a
  TASK-010 requirement nor a missing closure proof.

## Material proposed findings

None. `FIND-TASK-010-10` is closed exactly by the controlling lead direction,
and no deployment/gateway regression or unearned mechanism remains. No
placement, naming, structure, or wording observation is promoted to a blocking
finding.

## Result

**PASS.** The candidate deletes the invalid in-image admission mechanism,
preserves the accepted startup corrections, documents the conventional
operator-owned public-ingress control, and adds none of the mechanisms or
configuration surfaces the lead direction prohibits.
