---
id: TASK-010-R2
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-011, REQ-021, AC-004, AC-007]
depends_on: []
parent_task: TASK-010
remediates: [FIND-TASK-010-10]
---

# Put device-code attempt admission at the public edge

## Authority and immutable review subject

- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- R1 remediation:
  `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- Review base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Reviewed candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Validated ledger:
  `changes/active/oidc-production-readiness/review/TASK-010-r2/findings-validation.md`

Implement this task through `$wyrd-implement`. Reassess the complete original
TASK-010 range in the next review; do not review only this remediation diff.
`FIND-TASK-010-1` remains routed to TASK-011 and is not part of this task.

## Issue diagnosis

### FIND-TASK-010-10 — user-code attempt limiting remains downstream of replica selection

RFC 8628 section 5.1 requires effective rate limiting for attempts against the
short device user code. The approved R1 correction selected Wyrd's existing
deployment gateway boundary because it observes public clients and selects
backend replicas.

The candidate instead adds `map`, `limit_req_zone`, and `limit_req` to
`docker/official/extras/nginx/nginx.conf.template`. That NGINX is bundled into
every official application image and proxies only to the Rust server in the
same image. In the documented production path, the public edge gateway has
already selected a pod before this NGINX receives the request. Its shared
memory is therefore local to one replica, and `$binary_remote_addr` identifies
the edge or mesh peer rather than the external client.

The focused assertions in `scripts/server/test-startup.sh` exercise one image
directly. They prove that the rule matches only `POST /auth/device` within one
NGINX instance, but they cannot prove one allowance across backend selection
or distinct public-client budgets behind the supported edge.

The observable consequence is that an attacker can obtain a fresh allowance
through another replica or replacement pod, while unrelated clients collapsed
to the same downstream peer can throttle one another. The limit changes with
routing and fleet size, so the candidate does not close the approved security
obligation.

## Intended correction outcome

The deployment-owned public edge makes the one conventional rate-limit
decision for `POST /auth/device` before choosing a Wyrd backend. It keys the
decision by the client identity or address the edge itself observes. Backend
replicas own no independent device-verification allowance, and unrelated auth
routes do not share this budget.

## Decision-complete recommendation

Use the existing public-edge deployment integration described by
`architecture/operations/deployment-and-release.md` and the supported Gateway
API/Istio deployment surface as the sole owner of device-verification
admission. Attach that platform's conventional native rate-limit policy only
to `POST /auth/device`, before backend references select a replica, using the
edge-observed client identity/address rather than a forwarded value interpreted
inside Wyrd.

Delete the image-local `limit_req` mechanism and the claim that its per-image
shared-memory zone is common to backend replicas. Adjust the focused startup or
rendered-deployment proof so it validates the real owner without turning the
application server into a second limiter.

This is the smallest correction because the public edge already owns rate-limit
integration, public addressability, and backend selection. It uses a standard
gateway facility and introduces no Wyrd protocol, state store, or coordination
mechanism.

## Constraints and preserved behavior

- Preserve device-code entropy, expiry, one-use redemption, and mint-at-
  redemption behavior.
- Preserve RFC 8628 token polling cadence and `slow_down`; it is separate from
  verification-form attempt admission.
- Preserve availability of `GET /auth/device`, `/auth/token`, authorize,
  callback, refresh, revoke, metadata, and other unrelated routes.
- Preserve removal of the broad Rust-local governor and its dependencies.
- Preserve the explicit single-tenant `test:server:startup` fixture and its
  owner-only signing-key/file-KEK setup; independent review found it legitimate
  rather than a weakened gate.
- Preserve all closed R1 findings and the standing routing of
  `FIND-TASK-010-1` to TASK-011.

## Explicit non-goals

- No Wyrd application limiter, trusted-forwarded-header parser, database or
  cache counter, distributed rate-limit service, public setting, fallback, or
  compatibility path.
- No new grant, parameter, response shape, durable authority, or tenant rule.
- No new Vault/startup journey; multi-tenant Operator-key validation remains
  owned by its existing focused config and integration tests.
- No full identity, cross-language, or every-topology journey in this task;
  integrated public-edge/two-backend behavior runs at change review.

## Acceptance criteria

| Finding | Acceptance criterion |
| --- | --- |
| `FIND-TASK-010-10` | The supported deployment's public edge applies one conventional native limit to `POST /auth/device` before backend selection, keyed by edge-observed client identity/address. Alternating backend replicas cannot reset one client's allowance; a distinct client retains its own allowance; `GET /auth/device`, `/auth/token`, and other auth routes are unaffected; no image-local or Rust-local second limiter remains. |

## Focused proof and narrow verification

Validate the rendered/native public-edge configuration in the narrowest
gateway/deployment lane. The focused proof must establish that the policy:

1. attaches before backend references and covers only `POST /auth/device`;
2. keys the edge-observed client rather than trusting a value parsed by Wyrd;
3. returns `429` after one client's allowance is exhausted while a distinct
   client remains admitted; and
4. leaves `GET /auth/device`, `/auth/token`, and unrelated routes admitted.

Run every new or changed named test with its exact `mise exec --` selector or
the repository-owned setup wrapper. Then run only the narrow owner lanes for
the deployment, documentation, and script write set, including the focused
gateway/render check, `mise run test:server:startup` when the official-image
script remains touched, `mise run docs:check` when deployment documentation is
touched, `mise run fmt` for Rust changes, and `mise run lints` only when Rust
is changed. Do not run or require the full identity suite, broad language
sweeps, or `mise run gate`; those belong to integrated change review.
