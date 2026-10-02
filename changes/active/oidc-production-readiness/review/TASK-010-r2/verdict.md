# TASK-010 R2 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Prior candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- R1 remediation: `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`

The candidate remained unchanged throughout review. `.codegraph/` is absent,
so reviewers used repository source, immutable Git diffs, and existing
navigation tools directly. `FIND-TASK-010-1` remains routed to TASK-011 by
`review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md` and was not reopened.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
| --- | --- | --- |
| Authorization code + S256, exact redirect, one-use bounded code, and refusal negatives | Existing authorize, callback, code-redemption, client-auth, and filtered identity evidence; R1 corrections preserve the shared owners. | PASS |
| Named `openid-client` interoperability proof | Standing lead direction assigns the production BFF proof to TASK-011 and integrated change review. | PASS / routed |
| Device approval records no credential and redemption issues exactly once | `CliLogins` callback/redemption owners plus deterministic denial and expiry/delete race proofs. | PASS |
| Standard RFC error and callback behavior | `invalid_target`, safe authorize error redirect, provider refusal, case-insensitive Basic, form bodies, OAuth errors, and metadata are implemented and focused tests pass. | PASS |
| Refresh behavior and containment | Public rotation, confidential non-rotation, database-clock lifetime, and rotation-chain-only replay containment are source-closed and tested. | PASS |
| Tenant, connection, role, and audit invariants | RLS-only `active_refresh`, final callback lifecycle fence, canonical `auth.login`, and audit rollback are source-closed and tested. | PASS |
| Served OpenAPI and generated contracts | Public form client identification and ordinary HTTP Basic alternatives are published; codegen and served-document checks are recorded green. | PASS |
| RFC 8628 user-code attempts limited at the public gateway across replicas and client addresses | The only limiter is in each application image's bundled NGINX, after the documented public edge has selected a pod. Its zone is replica-local and `$binary_remote_addr` is the edge/mesh peer in that path. The one-image startup proof cannot establish the required property. | **FAIL — `FIND-TASK-010-10`** |
| Deleted private BFF/browser-session/sealed-completion protocols remain deleted | Cumulative source and dependency inspection found no compatibility path or replacement durable authority. | PASS |
| No unapproved mechanism, option, store, or protocol | The Rust-local all-auth governor was deleted. The retained finding requires only the deployment gateway's conventional native limiter. | PASS |
| Mandatory Rust documentation and repository boundaries | Prior rustdoc, tenant-selector, and boundary findings are closed; recorded targeted checks are green. | PASS |

## Independent review results

| Report | Result | Material claim |
| --- | --- | --- |
| `task-review-behavior.md` | FAIL | Device limiter remains proxy-collapsed and replica-local. |
| `task-review-invariants.md` | FAIL | Prior `FIND-TASK-010-10` remains open. |
| `standards-review.md` | FAIL | Proposed startup-gate weakening; rejected after focused follow-up and validation. |
| `maintainer-review.md` | FAIL | Proposed startup-gate weakening; rejected after focused follow-up and validation. |
| `system-review.md` | FAIL | Device limiter remains replica-local. |
| `domain-review-security-oauth.md` | FAIL | Device limiter remains proxy-collapsed and replica-local. |
| `domain-review-tenancy-audit.md` | FAIL | Proposed startup-gate weakening; rejected after focused follow-up and validation. |
| `domain-review-persistence-concurrency.md` | PASS | Persistence and concurrency corrections are closed. |
| `domain-review-deployment-gateway.md` | FAIL | Limiter defect confirmed; separate startup-gate proposal rejected after validation. |

## Follow-up decision

`followup-review.md` was required because discovery materially disagreed about
the startup fixture. It resolved the conflict from source and history:

- setting `WYRD_SERVER_TENANT_SLUG=acme` and using the supported owner-only
  file KEK is a legitimate repair of this declared one-tenant official-image
  fixture, not a weakened gate under `AGENTS.md` section 12;
- the lane never contained a working Vault fixture or declared a multi-tenant
  Vault journey, while config and Postgres integration tests own the
  multi-tenant fail-start invariant; and
- this does not close the separate gateway finding because the lane still
  exercises only one bundled NGINX instance.

The final independent validation rejected `REPO-R2-001`,
`MAINT-010-R2-1`, `TA-R2-001`, and `DOMAIN-DEPLOY-002` on that shared premise.

## Validated finding ledger

### FIND-TASK-010-10 — REVISED / INCORRECT

The user-code limiter is attached to NGINX inside each official application
image. In the supported production topology, the deployment-owned public edge
selects a backend before the request reaches that NGINX. Each replica therefore
owns a separate allowance, and the downstream peer address collapses external
clients. Scaling, replacement, or ordinary replica selection changes the
effective brute-force bound.

The correction is bounded: make the existing deployment-owned public edge the
sole owner of the conventional native limit for `POST /auth/device`, before
backend selection, keyed by the client identity/address observed by that edge;
remove the image-local rule and its false shared-replica claim. Do not add an
application limiter, trusted-header parser, database/cache state, service,
public option, or compatibility path.

## Prior-finding closure

- `FIND-TASK-010-1` remains routed to TASK-011 and is not reopened.
- `FIND-TASK-010-10` remains open as revised above.
- `FIND-TASK-010-2` through `-9` and `-11` through `-13` are closed by the
  cumulative candidate and the source/test evidence recorded in the reports.

## Verification evidence and limits

The candidate records green narrow owner lanes for principals integration,
SQL, code generation, client tier, tenant isolation, unwrap audit, format,
lints, filtered identity journeys, and startup. During this review,
`mise run test:server:startup` completed successfully against the immutable
candidate, and discovery reviewers reran focused OAuth, callback, persistence,
and concurrency tests recorded in their reports.

The green startup lane proves the supported single-tenant official-image
fixture and one image-local NGINX instance. It cannot prove public-edge client
identity or a shared pre-selection budget across replicas. Full integrated
identity, every-language, and public-edge/two-backend journeys remain change
review work under the standing narrowest-lane direction.

## Verdict

**FIX_REQUIRED**

One bounded implementation finding remains: `FIND-TASK-010-10`.

