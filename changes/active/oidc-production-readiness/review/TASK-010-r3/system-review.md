# System-resilience review — TASK-010 round 3

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `1f4466a9ae482eb1311f6e5206484758bc272ad8`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Superseding remediation direction: `changes/active/oidc-production-readiness/review/TASK-010-r2/lead-direction-FIND-TASK-010-10.md`
- Overall result: **PASS**

The candidate remained at the stated commit throughout this review. The
complete cumulative base-to-candidate range was reviewed, with
`7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29..1f4466a9ae482eb1311f6e5206484758bc272ad8`
used to locate the round-3 correction. `FIND-TASK-010-1` remains routed to
TASK-011 and is not reopened. The lead direction supersedes the earlier R2
task: Wyrd must remove the image-local limiter, document the operator-owned
public-ingress limit, and must not add edge manifests, application limiting,
header parsing, configuration, or coordination state.

## Deployed-path and failure-path evidence

| Changed runtime path | Deployment and durable boundary | Failure and recovery behavior | Proof assessment |
|---|---|---|---|
| Authorization request, provider callback, and authorization-code redemption | Login state, the verified principal and roles, the hashed one-minute code, and canonical audit staging are Postgres-backed and usable from any replica. Callback state is consumed before provider IO; final User/role/code work is fenced by the User-family and connection-slot locks and commits with its audit (`crates/wyrd/wyrd-auth/src/callback.rs:156-215`, `:367-469`). Redemption deletes and validates the code and issues the session in one tenant transaction (`:492-579`). | A provider refusal, ten-second provider timeout, cancellation, replica loss, or dependency outage affects one login attempt, not the process or unrelated auth capabilities. Once callback state was consumed, the person starts a new login; no Wyrd credential was committed. A database, issuance, or required-audit failure rolls back final callback or redemption writes. A committed code or session survives restart, and a committed code redemption cannot issue twice. | Cumulative focused callback, refusal, connection-cutoff, audit-rollback, and code-replay evidence closes the prior lifecycle defects. Provider HTTP is screened, pinned, redirect-free, response-bounded, and request-bounded (`crates/shared/wyrd-auth-oidc/src/screening.rs:20-32`, `:139-199`). |
| Device approval and redemption | The device row is the cross-replica authority. Callback completion records only a still-live approval; redemption locks and revalidates the row, deletes it, issues the human session, appends audit, and commits once (`crates/wyrd/wyrd-auth/src/cli_logins.rs:276-384`). No credential waits in a browser or pod. | Pending and `slow_down` responses remain retryable. Denial or expiry deletes the row. Cancellation, replica loss, database failure, issuance failure, or audit failure before commit leaves no partially issued credential; a live approval remains retryable until expiry. A committed redemption survives restart and the deleted row prevents double issuance. | Focused terminal-race and exactly-once Postgres tests recorded in the task evidence exercise denial/expiry overlap and successful redemption. Fleet admission is now correctly left to the operator-owned public ingress, as assessed below. |
| Refresh and revocation | Refresh hashes, connection binding, rotation ancestry, and audit are durable in Postgres. A tenant-qualified family advisory lock serializes refresh, replay containment, logout, administrative revocation, and connection retirement. Only a predecessor marked `rotated` triggers chain containment; other inactive rows do not revoke sibling sessions (`crates/wyrd/wyrd-auth/src/refresh.rs:137-266`). Revocation uses the same family lock and chain owner (`crates/wyrd/wyrd-auth/src/cli_logins.rs:408-453`). | A database, signing, or required-audit failure rolls the request transaction back. A committed public-client rotation survives restart; a replay contains the compromised rotation chain without taking unrelated CLI or UI sessions offline. Confidential-client renewal leaves its refresh row stable until absolute expiry. Connection deactivation/replacement remains an immediate renewal fence across replicas. | Recorded narrow concurrency tests cover same-token races, ancestor replay overlapping rotation, deactivation overlapping rotation, inactive rows, and revocation. No new process-local authority or retry loop was introduced. |
| RFC token exchange and machine grants | API-key exchange, delegation, and JWT bearer grants remain ordinary stateless HTTP handlers over the shared tenant issuer, verifier, tenant transaction, and canonical audit owners (`crates/wyrd/wyrd-server/src/components/auth/routes.rs:113-384`). | Store, verifier, or audit failure refuses only the request and commits no credential-side state. Human-provider availability does not gate machine grants, so an IdP outage does not remove workload access. | The cumulative task evidence records the narrow principals, platform, CLI, workload, and contract checks. Full integrated journeys remain change-review work. |
| Public device user-code admission | The supported topology has an operator-owned public ingress before one or more Wyrd replicas; repository authority assigns rate-limit integration to that ingress (`architecture/operations/deployment-and-release.md:8-24`). The candidate removes every `limit_req` mechanism from the official image, leaving its NGINX as a reverse proxy only (`docker/official/extras/nginx/nginx.conf.template:25-68`), and documents `POST /auth/device` as the route the operator limits per client address (`docs/src/content/docs/self-hosting/sso-and-oidc.svx:19-23`). | Scaling, rolling replacement, or backend selection no longer creates per-replica Wyrd budgets or collapses all external callers onto the edge peer seen by an image-local proxy. Loss or replacement of a Wyrd replica therefore cannot reset an in-image allowance, because Wyrd owns none. Operator ingress failure and recovery remain deployment concerns at the authority-assigned boundary. | This exactly implements the superseding lead direction and the conventional comparable-project boundary. The startup lane no longer pretends one image can prove an operator-ingress property (`scripts/server/test-startup.sh:1-14`, `:170-190`). No edge manifest, application limiter, option, state, or parallel mechanism was added. |

## Affected capabilities and recovery assessment

- Postgres remains the only cross-replica authority for login state,
  authorization codes, device approvals, refresh chains, connection lifecycle,
  and audit staging. Process restart or rolling replacement does not discard or
  fork grant state.
- Provider discovery, token, and JWKS failures are bounded to the affected
  login attempt. The provider transport's ten-second timeout and bounded body
  prevent a stalled or oversized provider response from holding a serving
  replica indefinitely.
- Database and canonical-audit failures fail the affected issuance, renewal,
  or revocation request closed. Transaction cancellation/drop releases row and
  advisory locks and does not leave a partially committed credential.
- Removing the image-local limiter restores route and tenant independence in
  the official image. Metadata, authorize, callback, token, refresh, revoke,
  machine grants, and unrelated tenants no longer share a local admission
  bucket.
- The operator note preserves RFC 8628 section 5.1's conventional deployment
  responsibility without making Wyrd ship or own infrastructure it does not
  otherwise provide. Device-code entropy, short expiry, one-use redemption,
  and durable poll-interval enforcement remain the in-server protections.
- The deleted browser-session and private-BFF authorities remain absent. No
  replacement pod-local session store, compatibility route, cleanup worker, or
  recovery protocol entered the cumulative diff.

## Prior system finding closure

### `FIND-TASK-010-10` — closed by the superseding lead direction

The R2 candidate put `limit_req_zone` inside each application image, after the
operator's public edge had selected a replica. That mechanism both keyed the
edge peer rather than the external client and reset its budget per replica.
Candidate `1f4466a9ae482eb1311f6e5206484758bc272ad8` deletes the map, zone,
status, and location limiter and deletes their one-container assertions. The
existing owner-only key, single-tenant production-profile restart, migration
refusal/retry, route, persistence, and SDK assertions remain in the startup
lane. The existing self-hosting page now tells the operator to rate-limit the
exact user-code entry route per client address at the public ingress.

Requiring a Wyrd edge manifest, application limiter, trusted-forwarded-header
mechanism, shared limiter store, or setting would contradict the lead direction,
the repository's operator-owned gateway boundary, and the standing rule to use
the conventional comparable-project mechanism without extra Wyrd machinery.
None is required.

## Material proposed findings

None.

No reachable crash, restart, dependency-outage, cancellation, rolling-replacement,
state-survival, or sibling-availability defect required by TASK-010 remains in
the cumulative candidate. No extra mechanism or option beyond the listed RFCs
and the conventional operator-ingress responsibility was found.

## Verification assessment

The remediation direction records `mise run test:server:startup`,
`mise run docs:check`, `mise run fmt`, and `mise run lints` as passing for the
round-3 write set. The cumulative task record supplies narrow owner-lane and
focused Postgres/concurrency evidence for callback fencing and audit rollback,
device terminal races and exactly-once redemption, refresh rotation and replay
containment, revocation, OAuth request classification, served OpenAPI, SQL, and
generated contracts.

Those are the appropriate task-review proofs. The startup lane proves the
official image's surviving startup, restart, migration, route, persistence,
and SDK behavior; it correctly no longer claims to prove an operator-owned
ingress limit. Full multi-replica, BFF `openid-client`, every-language, and
integrated identity journeys are deferred to change review by standing
direction and are not treated as a verification defect here.

The system-resilience result is **PASS**.
