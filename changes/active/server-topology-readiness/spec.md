---
id: SPEC-server-topology-readiness
revision: 1
status: draft
---

# Server topology readiness

## Human intent and user value

Every supported `wyrd-server` process topology must expose one consistent
Kubernetes readiness contract. Operators must be able to use the same
`GET /readyz` probe for API-serving and headless worker pods while receiving a
result derived only from the roles that process actually owns.

## Scope

- HTTP readiness for `all`, `server`, `oracle`, `scribe`, and `forge-worker`
  targets.
- Role-aware readiness projection through the existing health contract.
- Kubernetes readiness-probe configuration for every supported topology.
- A health-only operations surface for targets that do not serve the public
  Wyrd API.

## Non-goals

- Adding another readiness model, supervisor, worker protocol, or durable
  health registry.
- Exposing public Wyrd APIs from headless worker targets.
- Treating liveness as readiness or using readiness failure to define process
  restart policy.
- Changing Bifrost task, lease, recovery, query, ingest, or maintenance
  semantics.

## Definitions

**Supported process target.** One closed `WYRD_TARGET` value: `all`, `server`,
`oracle`, `scribe`, or `forge-worker`.

**Operations health surface.** The restricted HTTP surface containing the
existing `/healthz` and `/readyz` routes. It is not the public Wyrd API.

## Required behavior

<a id="req-001"></a>
### REQ-001 — Expose one readiness route on every target

Every supported process target must expose HTTP `GET /readyz`. A target that
does not serve the public Wyrd API must still expose the operations health
surface.

<a id="req-002"></a>
### REQ-002 — Project only applicable readiness

The route must reuse one readiness contract and evaluate common dependencies,
public-serving dependencies when applicable, and exactly the Bifrost roles
selected by the current target. Unselected roles must not affect the result.

<a id="req-003"></a>
### REQ-003 — Preserve readiness semantics

`GET /readyz` must return success only after every applicable dependency and
recovery gate is ready. It must return the existing bounded not-ready response
before startup completes, while required recovery is unresolved, after an
applicable dependency becomes unhealthy, and during shutdown. `/healthz` and
process liveness must not substitute for readiness.

<a id="req-004"></a>
### REQ-004 — Configure Kubernetes to probe the route

Every supported Kubernetes workload must configure an HTTP readiness probe
against `GET /readyz` on its pod. A missing or unreachable route is not a
successful readiness result.

<a id="req-005"></a>
### REQ-005 — Keep headless targets headless

The dedicated `forge-worker` target must expose the operations health surface
without exposing authenticated public API, ingest, query, CLI, MCP, or gRPC
application routes.

## Invariants and prohibited outcomes

<a id="inv-001"></a>
### INV-001 — One readiness implementation

Targets must not implement separate readiness handlers or status models. The
existing readiness snapshot and role projection remain authoritative.

<a id="inv-002"></a>
### INV-002 — No false readiness

No target may report ready while one of its applicable checks is unavailable,
unhealthy, unresolved, or shutting down. A check for an unselected role must
not make that target unready.

<a id="inv-003"></a>
### INV-003 — No public worker API

Making `/readyz` reachable from a headless worker must not make any public Wyrd
application route reachable from that worker.

## Externally observable behavior and failure modes

- Kubernetes can probe the same `GET /readyz` path on every supported target.
- A ready process returns the existing successful readiness response.
- An unready process returns the existing bounded not-ready response and never
  exposes raw dependency errors.
- A dedicated Forge worker exposes `/readyz` and `/healthz`; public Wyrd API
  routes remain unavailable.
- An unreachable readiness listener or incorrect probe port prevents the
  Kubernetes workload from becoming ready.

## Material constraints

- `wyrd-server` remains the only serving binary.
- Health and administrative surfaces remain restricted to the operations
  network and expose no secrets or tenant data.
- Existing readiness response shapes, reason codes, and `/healthz` behavior are
  preserved unless a separate specification changes them.
- Readiness controls availability; liveness or process exit controls restart.

## Required system boundary and flow

```text
Start one supported wyrd-server target
  -> bind its operations health surface
  -> publish not-ready while applicable startup and recovery gates run
  -> answer GET /readyz from the shared role-aware snapshot
  -> publish ready only when every applicable check succeeds
  -> remove readiness before applicable dependency loss or shutdown
```

## Acceptance obligations

<a id="ac-001"></a>
### AC-001 — Every target serves the shared route

Production-topology integration evidence starts each supported target and
proves `GET /readyz` is reachable, returns the shared response contract, and
evaluates exactly that target's applicable checks.

<a id="ac-002"></a>
### AC-002 — Headless worker isolation

Integration evidence proves a dedicated Forge worker changes from not ready to
ready across its real startup/recovery boundary and back to not ready on
dependency loss or shutdown, while representative public API routes remain
unreachable.

<a id="ac-003"></a>
### AC-003 — Kubernetes probe closure

Deployment verification proves every supported Kubernetes workload declares a
readiness probe using `/readyz` and the port actually bound by that target.

## Open material decisions

None.

## Planning-decision inventory

`$wyrd-plan` must resolve the shared health-listener ownership and bind
configuration for API-serving and headless targets, reuse of the existing
health router and readiness snapshot, shutdown ordering, Kubernetes manifest
projection, and the smallest production-topology test set that proves every
target without duplicating readiness tests.

## Revision history

- Revision 1 (`draft`, 2026-09-04): created from explicit human direction that
  every supported server topology expose the same Kubernetes `/readyz` route.
  Pending explicit approval.

## Material authority links

- [`AGENTS.md`](../../../AGENTS.md)
- [`architecture/agent-rules.md`](../../../architecture/agent-rules.md)
- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md)
- [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx)
- [`architecture/bifrost-design.md`](../../../architecture/bifrost-design.md)
- [`architecture/operations/README.md`](../../../architecture/operations/README.md)
- [`architecture/operations/deployment-and-release.md`](../../../architecture/operations/deployment-and-release.md)
- [`architecture/references/languages/spec-driven-development.md`](../../../architecture/references/languages/spec-driven-development.md)
- [Kubernetes readiness probes](https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#container-probes)
