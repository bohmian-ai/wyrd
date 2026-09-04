---
id: TASK-019
title: Single-address gateway and production UI container
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-069, REQ-100, REQ-101, REQ-102, REQ-103, REQ-104, REQ-105, REQ-106, INV-001, INV-002, INV-009, INV-021, AC-006]
depends_on: [TASK-003, TASK-018]
parent_task:
remediates: []
---

# Outcome and operator value

Ship the adapter-node UI behind the repository's one Nginx gateway so browser,
HTTP, gRPC, Oracle, and Scribe clients use one Wyrd address without learning
internal topology.

# Owner and write set

- Own the existing Dockerfile/runtime entrypoint, one Nginx template, SvelteKit
  production packaging, focused gateway check, and only directly required build
  task adjustments.
- Route UI to SvelteKit and canonical `/auth/*`, `/v1/*`, and public gRPC methods
  to their owning Wyrd roles for all-in-one and split deployment.
- Preserve host/correlation, `X-Wyrd-Access-Token`, RFC 9457 errors, streaming,
  cancellation, bounded buffering, and no retry of non-idempotent writes.
- Expose public gateway health plus private target-aware role probes.

# Locked decisions and non-goals

- Nginx routes only; SvelteKit is the BFF and Wyrd services own identity/authz.
- Cross-pod upstreams require authenticated encryption and explicit trust roots;
  plaintext is limited to loopback/equivalent local boundaries.
- No Helm/operator/service mesh/certificate issuer/cloud ingress, new gateway,
  `/api` alias, browser proxy tree, or public Forge-worker route.

# Ordered test scenarios

1. All-in-one config contains only loopback upstreams and target `all`.
2. UI, auth, ordinary v1, Oracle, Scribe/OTLP, and gRPC requests reach correct
   owners from one address.
3. Split config uses encrypted role Services and exposes no Forge route.
4. Credentials/correlation/errors/streams pass safely; cookies never become
   tenant or Wyrd credentials at Nginx.
5. Health distinguishes process liveness from traffic readiness.
6. The production image starts and terminates its process group when a required
   process exits.

# Red-Green-Refactor

Exercise one rendered topology at a time. Keep one template and parameterize
only upstream destinations; do not duplicate topology-specific configurations.

# Exact verification

```bash
mise exec -- bash scripts/checks/gateway-topology.sh all
mise exec -- bash scripts/checks/gateway-topology.sh split
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run docker:build
```

# Evidence and stop conditions

Record rendered topology, route ownership, header/error/stream behavior, health,
and container smoke results. Stop if gateway configuration would become tenant
or authorization authority.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
