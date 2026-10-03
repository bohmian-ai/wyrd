# TASK-010 R2 deployment and gateway domain review

## Subject and boundary

Reviewed the immutable cumulative range
`fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa..7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
and the remediation delta
`04366e7fc28c466fcdcbc7279885cfee82a988a2..7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
for the deployment/gateway part of TASK-010 and TASK-010-R1. The review covers
closure of prior `FIND-TASK-010-10`, removal of the in-process auth governor,
the official-image NGINX limiter, proxy/client-address and replica behavior,
and the changed `test:server:startup` deployment mode. Per lead direction,
`FIND-TASK-010-1` is routed to TASK-011 and was not reopened.

`.codegraph/` is absent. Candidate `HEAD` was
`7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29` before and after inspection.

## Authority and source coverage

| Boundary | Authority and source inspected | Assessment |
| --- | --- | --- |
| Gateway ownership and replica topology | `architecture/operations/deployment-and-release.md:6-24`; `architecture/v1/04-surfaces/deployment.md:3-7,52-57`; `docs/src/content/docs/self-hosting/kubernetes-production.svx:499-568` | One logical edge gateway owns rate-limit integration before requests are routed to the serving fleet. The supported production topology has an Istio edge gateway in front of Wyrd pods. |
| Official image proxy topology | `docker/official/Dockerfile:1-12,23-46`; `docker/official/extras/entrypoint.sh:1-46`; `docker/official/extras/nginx/nginx.conf.template:25-32,40-75` | Every application image starts its own NGINX and proxies to its own loopback Rust server. The added zone is therefore behind the production edge, per pod, and keys the edge peer address rather than the original client. |
| RFC 8628 admission obligation | TASK-010 Scenario 2 and acceptance criteria; TASK-010-R1 `FIND-TASK-010-10` and acceptance table; RFC 8628 section 5.1 | The short user-code verification attempt is the narrowly correct route to limit. Removing the all-auth application governor is correct, but the replacement must live at the gateway that observes the real client and precedes backend selection. |
| Startup lane and production key authority | `scripts/server/test-startup.sh:1-15,53-63,87-98,116-125,159-203,228-233`; `architecture/wyrd-design.md:1325-1340`; `crates/wyrd/wyrd-server/src/config.rs:1726-1732,1782-1811,3129-3137`; `crates/wyrd/wyrd-server/src/boot/mod.rs:1421-1453` | The lane previously selected multi-tenant mode by omitting `auth.tenant_slug`; production therefore required HTTPS Vault and boot-time validation of every active tenant's active KEK. The candidate explicitly selects single-tenant `acme` and a file KEK, which bypasses both multi-tenant checks. |
| Gate integrity and verification scope | `AGENTS.md:440-518,546-571`; TASK-010-R1 focused proof section | Narrow task verification is correct and full journeys remain change-review work. It does not authorize changing an established lane's deployment topology to avoid a red production-startup condition. |

The NGINX `map` plus `limit_req_zone`/`limit_req` mechanism is conventional and
native to the selected proxy. No custom rate-limit service, distributed cache,
trusted-header parser in Rust, or new public setting is required or recommended.

## Deployed path and proof assessment

The implemented request path in the startup lane is host client -> the one
container's NGINX -> that container's loopback Rust server. In this topology,
the test at `scripts/server/test-startup.sh:190-203` credibly proves that one
NGINX process limits `POST /auth/device`, leaves the token and GET routes
outside that bucket, and distinguishes the Docker-host peer from a loopback
peer inside the container.

That is not the supported replicated production path. The documented path is
client -> Istio edge gateway -> Service -> Wyrd pod -> that pod's bundled
NGINX -> loopback Rust server. At the bundled NGINX, `$binary_remote_addr` is
the edge-gateway peer, not the original client. Each Wyrd pod also has a
different NGINX shared-memory zone. Consequently the change retains both
causes diagnosed in prior `FIND-TASK-010-10`: unrelated clients collapse onto
the gateway address within a pod, while routing to another pod selects a fresh
bucket. The static comment that "backend replicas see one budget" is contradicted
by the image and entrypoint topology.

The reported green `mise run test:server:startup` is also not credible evidence
for the lane's former multi-tenant production restart. Setting
`WYRD_SERVER_TENANT_SLUG=acme` makes `auth.tenant_slug.is_none()` false, so
configuration accepts a file KEK and `verify_operator_keys` returns before
enumerating and validating active tenant keys. This is a real lost property,
not merely a different fixture: the lane no longer proves that the official
image can restart in multi-tenant production with the required Vault source
and boot-time tenant-key verification.

I did not rerun the Docker lane. The implementation report records it green,
and both defects are established from the rendered topology and branch
conditions; rerunning the same single-container, single-tenant path cannot
exercise either missing property. Full journeys are correctly deferred to
change review, but the remediation still needs the narrow gateway/startup proof
it names.

## Material findings

### DOMAIN-DEPLOY-001 — INCORRECT — the replacement limiter is still proxy-collapsed and replica-local

- **Violated obligation:** TASK-010-R1 `FIND-TASK-010-10` requires device
  user-code attempts to be limited at the existing gateway across backend
  replicas and by real client address, without sharing a bucket with unrelated
  auth routes. Deployment authority assigns rate-limit integration to the one
  logical gateway before backend routing.
- **Exact location:**
  `docker/official/extras/nginx/nginx.conf.template:40-50,65-75`, composed by
  `docker/official/extras/entrypoint.sh:26-38` inside every image described by
  `docker/official/Dockerfile:3-12`; the production edge and Service routing
  are documented at
  `docs/src/content/docs/self-hosting/kubernetes-production.svx:499-568`.
- **Evidence:** The limiter key is `$binary_remote_addr`. In the supported
  production path that address is the Istio edge peer. The zone is NGINX
  process shared memory, while every Wyrd pod starts its own NGINX and proxies
  only to `${WYRD_SERVER_BIND}`, which defaults to the same container's
  loopback server. The startup proof drives one such container directly and
  never traverses the edge gateway or a second backend.
- **Observable consequence:** One external caller can evade the intended
  user-code attempt budget by being routed to another Wyrd pod, while clients
  routed through the same edge identity can consume one another's per-pod
  budget. After removal of the Rust governor, the supported replicated path
  has no effective per-client, pre-replica control for this attack surface.
- **Required testable correction:** Put the native route-specific limit on the
  actual existing deployment-edge gateway that observes the client and acts
  before backend selection; remove the ineffective per-pod NGINX limit. Keep
  the rule limited to `POST /auth/device` and preserve token polling's existing
  persisted cadence/`slow_down`. Prove through the narrow official gateway
  path that one client's excess attempts receive `429`, a second client and
  unrelated routes remain admitted, and backend selection cannot reset the
  budget. Add no application limiter, trusted-forwarded-header parser,
  database/cache service, or public tuning surface.

### DOMAIN-DEPLOY-002 — VIOLATION — the startup lane was weakened from multi-tenant production to single-tenant file-key mode

- **Violated obligation:** `AGENTS.md:560-571` forbids weakening a gate to make
  it pass and requires the diagnosed cause of a red lane to be fixed or
  reported as a blocker. Wyrd design requires multi-tenant production to use
  HTTPS Vault and to fail startup unless every active tenant's active key is
  readable (`architecture/wyrd-design.md:1325-1340`).
- **Exact location:** `scripts/server/test-startup.sh:53-63,116-125` adds
  `WYRD_SERVER_TENANT_SLUG=acme`, selects `WYRD_OPERATOR_KEK_SOURCE=file`, and
  mounts one local `v1` key for both development and the production restart at
  `:228-233`.
- **Evidence:** `OperatorKeysConfig::validate` requires Vault only when
  production and `auth.tenant_slug.is_none()`
  (`config.rs:1791-1811,3129-3137`). `verify_operator_keys` uses the same
  condition and otherwise returns immediately
  (`boot/mod.rs:1421-1453`). Before this remediation delta, the lane set no
  tenant slug and its production restart therefore selected the multi-tenant
  branch; the remediation record itself diagnoses that the restart was red on
  the Vault-only rule. The candidate makes the branch unreachable instead of
  supplying its required provider and keys.
- **Observable consequence:** `test:server:startup` can be green when the
  official image's multi-tenant production configuration cannot initialize its
  required Vault provider, cannot read an active tenant KEK, or no longer
  performs the boot-time all-active-tenant key check. The live multi-tenant
  production startup/persistence protection formerly selected by this lane is
  absent.
- **Required testable correction:** Preserve the established multi-tenant
  production restart and exercise its existing Vault-backed KEK path with the
  repository's ordinary local dependency/fixture pattern, including the
  active-tenant key verification needed for readiness. If the device limiter
  needs a smaller proof, keep it as a focused gateway check rather than
  changing this lane's deployment mode. Do not add another key provider,
  fallback, option, or repository check.

## Non-blocking notes

None. Placement, naming, structure, and wording observations were not promoted
to findings.

## Result

**FAIL.** Prior `FIND-TASK-010-10` is not closed in the supported proxied,
replicated topology, and the candidate obtains a green startup lane by
removing its multi-tenant production/Vault startup property. Both are bounded
behavioral/security verification defects; neither requires a new bespoke
mechanism.
