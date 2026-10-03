# TASK-010 R2 structured findings validation

## Immutable subject and inputs

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Prior candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- Prior verdict and ledger: `changes/active/oidc-production-readiness/review/TASK-010-r1/{verdict.md,findings-validation.md}`
- Standing lead direction: `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

`HEAD` was the candidate before and after validation. `.codegraph/` is absent,
so repository source, Git history, and both immutable diffs were inspected
directly. All required R2 discovery and follow-up reports were present.
`FIND-TASK-010-1` is routed to TASK-011 and was not reopened.

Applicable authority included `AGENTS.md`, `architecture/agent-rules.md`, the
spec-driven-development and maintainer-style references,
`architecture/wyrd-design.md`,
`architecture/operations/deployment-and-release.md`,
`architecture/v1/04-surfaces/deployment.md`, the approved task/remediation,
and RFC 8628 section 5.1. RFC 8628 requires effective rate limiting of user-code
attempts but does not prescribe a custom limiter, store, option, or Wyrd
protocol. Repository architecture assigns ordinary rate-limit integration to
the deployment-owned public gateway.

## Independent source trace

### Device-verification admission

The only candidate mechanism for user-code attempt admission is the NGINX
`map`/`limit_req_zone`/`limit_req` in
`docker/official/extras/nginx/nginx.conf.template:40-74`. The previous Rust
`tower_governor` layer and its dependencies are deleted; repository search
found no sibling device-attempt limiter or shared writer. The server router is
mounted once from `crates/wyrd/wyrd-server/src/http/router.rs:47-105`, and the
reachable `POST /auth/device` handler calls the existing `CliLogins` owner.
The durable device-code expiry and token-poll `slow_down` behavior are separate
controls and do not bound guesses submitted to the verification form.

`docker/official/Dockerfile:3-52` packages one NGINX and one loopback Rust
server in every application image; `docker/official/extras/entrypoint.sh:19-46`
starts those processes in that image. NGINX shared memory is therefore shared
only by workers of one image's NGINX instance. In the documented supported
production path, the Istio `wyrd-edge` Gateway selects a Service backend before
the request reaches a Wyrd pod
(`docs/src/content/docs/self-hosting/kubernetes-production.svx:499-568`). The
pod-local `$binary_remote_addr` is consequently the gateway/mesh peer, not the
public client, and another pod owns another zone. The comment that backend
replicas share one budget is contradicted by the deployed path.

The focused assertions in `scripts/server/test-startup.sh:190-203` drive one
image directly. Their two keys are the Docker-host peer and an in-container
loopback peer. They prove route selectivity in one NGINX instance, but neither
public-client separation behind the supported edge nor one budget before
replica selection. The path is reachable and security-relevant: replica
selection can reset an attacker's allowance, while public clients sharing the
same immediate edge peer can consume one another's pod-local allowance.

The mechanism itself is conventional NGINX behavior, so it is not human-
directed `DRIFT`. It is attached to the wrong deployed owner and therefore
does not satisfy the already-approved R1 acceptance criterion. The retained
classification is `INCORRECT`.

### Startup fixture and Operator KEK

`test:server:startup` declares an official-image journey against one freshly
created `acme` tenant: migration/refusal checks, setup, routes, SDK traffic,
production-profile restart, and persistence. Neither the mise description nor
the script's assertion list declares a multi-tenant Vault integration journey.
The candidate makes that existing one-tenant subject explicit with
`WYRD_SERVER_TENANT_SLUG=acme`, uses the architecture-approved single-tenant
file KEK, and delivers the signing key and KEK as owner-only files.

The no-slug state was not credible prior proof of a multi-tenant production
image. Git history shows the startup lane was created before the Operator-key
branch containing the later multi-tenant Vault rule was integrated. When that
rule became reachable, the lane had no Vault address, token, HTTPS fixture, or
tenant keys and could not pass its production restart. The remediation record
diagnoses that invalid fixture plus the independently invalid `0644` signing
key before changing it. No assertion, expected result, timeout, retry, skip, or
negative case was removed.

The multi-tenant production invariant remains owned by
`OperatorKeysConfig::validate` and `verify_operator_keys`, with focused config
coverage and
`pg_operator_connection_routes::production_boot_requires_every_active_tenant_key`
covering unavailable, missing, malformed, wrong-length, readable, and explicit
single-tenant cases. Requiring this OAuth/device remediation to add an HTTPS
Vault service and a second deployed-image proof would create a new proof
surface not promised by the lane, task, RFC, or remediation. Under the
narrowest-lane direction, that is out-of-scope drift rather than restoration
of a removed gate.

The startup change is therefore a legitimate fixture repair, not a weakened
gate under `AGENTS.md` section 12. Its green result remains insufficient for
the separate gateway finding above.

## Proposal validation

| Discovery proposal | Result | Source-based disposition |
|---|---|---|
| `BEHAVIOR-TASK-010-R2-1` | **REVISED** | Reachable failure confirmed; deduplicated into open prior `FIND-TASK-010-10` and classified `INCORRECT`, not new drift. |
| `INV-REV-001` | **REVISED** | Same producer and consequence as `FIND-TASK-010-10`; the image-local zone is not the pre-selection public edge. |
| `SYS-R2-1` | **REVISED** | Same failed closure; replica restart/selection changes the effective allowance because state belongs to each image. |
| `SEC-OAUTH-R2-001` | **REVISED** | Same failed closure; the supported proxied path collapses public clients at the downstream peer and resets state per pod. |
| `DOMAIN-DEPLOY-001` | **REVISED** | Same failed closure; retained once under the stable prior ID. |
| `REPO-R2-001` | **REJECTED** | It treats an undeclared, never-valid default as a preserved multi-tenant/Vault assertion. The lane's declared one-tenant journey and results remain intact. |
| `MAINT-010-R2-1` | **REJECTED** | Same unsupported premise as `REPO-R2-001`; adding a live Vault topology here would broaden the lane and maintenance surface. |
| `TA-R2-001` | **REJECTED** | The change selects the supported mode the fixture actually provisions; focused owners retain the multi-tenant fail-start invariant. |
| `DOMAIN-DEPLOY-002` | **REJECTED** | The old omission did not establish a working official-image Vault path, and no asserted behavior was removed to make the lane green. |

The follow-up proposed no new finding. Placement, naming, structure, wording,
and the inaccurate NGINX comment are not independent blockers; the retained
finding is behavioral and security-relevant.

## Final deduplicated finding ledger

### FIND-TASK-010-10 — REVISED / INCORRECT — user-code attempt limiting remains downstream of replica selection

- **Discovery source IDs:** `BEHAVIOR-TASK-010-R2-1`, `INV-REV-001`,
  `SYS-R2-1`, `SEC-OAUTH-R2-001`, `DOMAIN-DEPLOY-001`.
- **Violated obligation:** RFC 8628 section 5.1 requires effective limiting of
  attempts against the short user code. The approved R1 acceptance criterion
  requires the conventional existing gateway boundary across backend replicas
  and client addresses, with no shared bucket for unrelated auth routes.
  Deployment authority assigns rate-limit integration to the public gateway
  before it selects a serving replica.
- **Exact location:**
  `docker/official/extras/nginx/nginx.conf.template:40-74`, composed inside
  every image by `docker/official/Dockerfile:3-52` and
  `docker/official/extras/entrypoint.sh:19-46`; the incomplete proof is
  `scripts/server/test-startup.sh:190-203`. The actual supported public edge
  and its backend selection are documented at
  `docs/src/content/docs/self-hosting/kubernetes-production.svx:499-568`.
- **Evidence:** each application image owns a separate NGINX shared-memory
  zone and proxies only to its own loopback Rust server. In the supported
  proxied path, `$binary_remote_addr` identifies the immediate edge/mesh peer,
  not the external client. The startup lane has one application image and two
  direct peers, so it cannot establish the public path's key or shared
  pre-replica budget. No sibling limiter closes the gap.
- **Observable consequence:** an attacker can obtain a fresh guess allowance
  by reaching another Wyrd replica or replacement pod, while unrelated public
  clients observed as the same edge peer can throttle one another. The
  effective brute-force bound therefore changes with routing and fleet size.
- **Decision-complete correction:** use the existing deployment-owned public
  edge as the sole owner of this admission decision. Attach its conventional
  native rate-limit policy only to `POST /auth/device`, before backend
  selection, keyed by the downstream client identity/address the edge itself
  observes. Remove the image-local `limit_req` rule and the server/router claim
  that a per-image zone is shared across replicas. Preserve the existing
  device-code entropy and expiry, token polling cadence/`slow_down`, all
  unrelated auth routes, and the explicit single-tenant startup fixture. Add
  no Wyrd header parser, application limiter, database/cache state, rate-limit
  service, public option, fallback, or compatibility path.
- **Focused closure proof:** validate the rendered/native public-edge
  configuration in the narrow gateway/deployment lane: the rule is attached
  before backend references, matches only `POST /auth/device`, and keys the
  edge-observed client rather than an untrusted forwarded value. Retain one
  focused request proof that excess attempts receive `429` while a distinct
  client, `GET /auth/device`, and `/auth/token` remain admitted. Do not require
  the full identity or cross-language journeys here; the integrated
  public-edge/two-backend journey runs at change review.

## Prior-finding closure

- `FIND-TASK-010-1` remains routed to TASK-011 by standing lead direction and
  is not reopened.
- `FIND-TASK-010-10` remains open as revised above.
- `FIND-TASK-010-2` through `-9` and `-11` through `-13` are source-closed by
  the candidate evidence traced in the discovery reports; no proposal against
  those closures survived independent validation.

## Validation result

**COMPLETE.** One deduplicated finding remains: `FIND-TASK-010-10`. All four
startup-gate proposals are rejected. No missing source, caller trace, report,
or unresolved authority conflict requires `BLOCKED`, and the retained
correction needs no new product, public API, custom security mechanism,
persistent state, or specification decision.
