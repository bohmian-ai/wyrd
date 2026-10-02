# TASK-010 R2 focused follow-up review

## Uncertainty reviewed

This fresh follow-up resolves two conflicting discovery claims against the
immutable subject:

- base `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`;
- prior candidate `04366e7fc28c466fcdcbc7279885cfee82a988a2`;
- candidate `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`.

The first conflict is whether changing `test:server:startup` to the explicit
single-tenant `acme` profile with an owner-only file Operator KEK legitimately
repairs the lane fixture or weakens a multi-tenant production/Vault gate. The
second, separate uncertainty is whether the new bundled-NGINX device limiter
occupies the public gateway boundary in supported replicated deployments.
`FIND-TASK-010-1` remains routed to TASK-011 by standing lead direction and was
not reopened.

`.codegraph/` is absent, so the review used repository source, Git history and
the immutable diffs directly.

## Paths and authority inspected

- `AGENTS.md` sections 11-12 and `architecture/agent-rules.md`, including the
  prohibition on weakening a red gate and the narrowest-lane rule.
- `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`,
  especially Scenario 2 and its verification scope.
- `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`,
  especially `FIND-TASK-010-10`, its focused proof, non-goals and recorded lane
  diagnosis.
- `mise.toml:818-820` and the complete current, base and prior-candidate forms
  of `scripts/server/test-startup.sh`, plus the file's Git history.
- `crates/wyrd/wyrd-server/src/config.rs:1662-1849,3125-3137` and
  `crates/wyrd/wyrd-server/src/boot/mod.rs:1347-1367,1421-1453`.
- `crates/wyrd/wyrd-server/tests/pg_operator_connection_routes.rs:685-789`
  and the `test:principals:integration` ownership in `mise.toml`.
- `architecture/wyrd-design.md:1325-1340`,
  `architecture/operations/deployment-and-release.md:6-24`, and
  `architecture/v1/04-surfaces/deployment.md:1-7,52-57`.
- `docker/official/Dockerfile:3-12,23-52`,
  `docker/official/extras/entrypoint.sh:19-46`, and
  `docker/official/extras/nginx/nginx.conf.template:25-75`.
- `docs/src/content/docs/self-hosting/kubernetes-production.svx:499-568` and
  `deploy/kubernetes/kind/wyrd.yaml:1-174`.
- Every R2 discovery report supplied to this follow-up.

## Resolution 1: the single-tenant startup fixture is a legitimate lane repair

The candidate does **not** weaken a previously established multi-tenant
production/Vault assertion.

The lane's declared contract is the official image against external Postgres:
migration/refusal behavior, setup, the image's HTTP/gRPC routes, a development
write, a production-profile restart, and persistence
(`mise.toml:818-820`; `scripts/server/test-startup.sh:1-18`). Neither the task,
the remediation task, the mise description, nor the script comments declare a
multi-tenant deployment or a live Vault integration as an assertion. The
fixture provisions exactly one tenant, `acme`, and verifies that tenant's data
across restart (`scripts/server/test-startup.sh:159-167,204-230`).

Git history rules out treating the old omission as a proven gate property. The
multi-tenant production Vault requirement and `verify_operator_keys` readiness
check entered the repository on 2026-09-24 (`98b298faab` and related follow-up
commits). `test:server:startup` was introduced later, on 2026-09-29
(`4ae6a1992`), without a tenant slug, Vault address, Vault token or Vault key
fixture. Under the already-existing config validator, its production restart
therefore could not pass. The base and prior-candidate scripts are identical on
that point. There was no working multi-tenant official-image proof for the
candidate to delete.

The remediation record diagnoses both failures before changing the fixture:
the bind-mounted signing key violated the owner-only file rule, and the
undeclared deployment mode selected the Vault-only branch
(`TASK-010-R1-authorization-server-corrections.md:375-381`). The candidate then
makes the fixture's actual one-tenant topology explicit, uses the
architecture-approved single-tenant file source, and mounts both key files as
the image user with mode `0600` (`scripts/server/test-startup.sh:53-63,116-125`).
That changes configuration, not a behavioral assertion, skip, timeout, retry,
or expected result. It is the smallest conventional repair for the lane the
repository actually declared.

The multi-tenant invariant is not inferred from this single-tenant lane. It is
owned directly by:

- config validation, which requires HTTPS Vault for multi-tenant production
  (`config.rs:1781-1849`), with unit coverage for the accepted/refused source
  matrix and HTTPS rule; and
- `production_boot_requires_every_active_tenant_key`, which exercises
  unavailable, missing, malformed, wrong-length and readable Vault results,
  proves that every active tenant is read, and separately proves the explicit
  single-tenant bypass
  (`pg_operator_connection_routes.rs:724-789`). That integration test belongs
  to `test:principals:integration`.

Those tests do not constitute a full official-image/Vault wiring journey, but
TASK-010 and its remediation never require such a journey, and the startup
lane never supplied one. Requiring a new HTTPS Vault service and key population
inside this unrelated image/persistence lane would add a new proof surface
rather than restore a removed assertion. Under the task-review boundary and
the human direction against unsupported bespoke checks, that is out-of-scope
DRIFT, not a remediation requirement.

Accordingly, `REPO-R2-001`, `MAINT-010-R2-1`, `TA-R2-001`, and
`DOMAIN-DEPLOY-002` are resolved against their shared premise and should be
rejected. The behavior, invariant, system, persistence/concurrency and
security/OAuth reports are correct on this issue.

## Resolution 2: the image-local limiter does not cover the supported edge path

This defect is independent of the startup fixture and remains reachable.

The candidate's NGINX is packaged and started inside every Wyrd application
image. Its upstreams point only to that container's loopback Rust server and
UI (`Dockerfile:3-12,35-52`; `entrypoint.sh:26-38`;
`nginx.conf.template:25-33`). Therefore its shared-memory rate-limit zone is
shared only by workers of one pod's NGINX process, not by Wyrd backend replicas.
The comment at `nginx.conf.template:43-44` claiming that backend replicas see
one budget is false for the deployed topology.

The supported production path is client -> Istio edge gateway -> Kubernetes
Service -> one Wyrd pod -> that pod's bundled NGINX -> loopback Rust server
(`kubernetes-production.svx:499-568`). Architecture assigns rate-limit
integration to that logical gateway before backend routing
(`deployment-and-release.md:8-24`). At the bundled NGINX,
`$binary_remote_addr` is the upstream gateway peer in that path, while the
candidate config has no trusted real-IP/proxy-protocol processing. Unrelated
external clients therefore share the gateway-peer bucket within a selected pod,
and selection of another pod reaches a different zone.

The startup assertion at `scripts/server/test-startup.sh:190-203` proves only a
direct, one-container topology. Its two observed peers are the Docker host and
the same container's loopback client. It does not traverse the documented edge
or select a second backend, so it cannot close the proxy-collapsing and
replica-reset causes diagnosed by prior `FIND-TASK-010-10`.

The mechanism required by RFC 8628 is ordinary route-specific admission at the
gateway that observes the client; no Wyrd-specific limiter, forwarded-header
parser, database/cache service, public option, or new rate-limit protocol is
needed. The existing remediation already selects that conventional boundary.
The discovery proposals concerning this defect should be retained and
deduplicated as one closure failure for prior `FIND-TASK-010-10`; this follow-up
does not assign a new finding ID.

## New proposed findings

None. The startup-gate proposals are rejected; the limiter evidence narrows and
corroborates the already-proposed unresolved closure of prior
`FIND-TASK-010-10`.

## Result

**RESOLVED.** The single-tenant file-KEK change is a legitimate correction of
an undeclared, never-working startup fixture and is not a weakened gate under
`AGENTS.md` section 12. Separately, the bundled per-image NGINX limiter does not
occupy the supported deployment's logical public gateway and does not close
prior `FIND-TASK-010-10` across proxied replicas.
