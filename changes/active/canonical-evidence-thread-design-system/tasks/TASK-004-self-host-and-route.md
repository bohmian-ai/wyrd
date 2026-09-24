---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-canonical-evidence-thread-design-system
spec_revision: 2
requirements: [REQ-013, REQ-015, REQ-016, REQ-019, REQ-032, REQ-035, REQ-040, REQ-042, REQ-043, INV-007, INV-008, INV-011, INV-012, AC-006, AC-010, AC-011, AC-012, AC-013, AC-015]
depends_on: [TASK-002]
parent_task:
remediates: []
---

## Outcome and Value

A team operator can understand the supported self-hosted Wyrd deployment, choose Postgres and S3/GCS/Azure storage, and see how containers and Kubernetes role-targeted pods sit behind one logical Wyrd endpoint. They can tell exactly which deployment artifact or step is presently available and how to verify readiness and routing.

## Owners, Scope, Consumers, and Prohibited Changes

The deployment and operation guides under `docs/` own this outcome. Ground them in typed server configuration, `architecture/operations/deployment-and-release.md`, Bifrost design, and actual container/Kubernetes artifacts. `wyrd-server` is the one external serving surface; Scribe, Oracle, and Forge are internal responsibilities, not independent public APIs. Do not imply a separate commercial enterprise edition, a published image/chart that does not exist, new ingress behavior, or unverified production commands. Keep current public docs URLs working.

## Approach

1. Audit existing self-hosting, Docker, enterprise, Bifrost, storage, and configuration pages against current code and deployment artifacts.
2. Present the shortest supported team deployment path with its required database, object store, secrets, network/TLS, readiness, and client connection steps.
3. Explain Kubernetes-first role activation, persistent Scribe state, Oracle scratch, Forge, HTTP/gRPC paths, and one external gateway using actual Wyrd boundaries.
4. Show S3, GCS, and Azure choices at the storage decision point; keep exhaustive settings in nearby reference.
5. Give explicit status and next supported action for missing release packaging, manifests, or routing automation; connect the pages to TASK-002 navigation.

## Proof Strategy

This is documentation of existing architecture and artifacts, with no new executable behavior; a manufactured RED is not applicable. Verify each runnable deployment command or manifest against checked-in artifacts and the owning server configuration. Use a source-to-claim review for role routing, volumes, auth/TLS, and storage. An unavailable image, chart, or end-to-end deployment is recorded as a capability boundary rather than called verified.

## Acceptance Criteria

- A reader can distinguish local from durable self-hosted setup and find the required Postgres, object storage, secret, network, and readiness choices without a giant initial checklist.
- The container guide names only images and commands backed by real artifacts; any incomplete OSS production or enterprise path states the exact missing piece.
- The Kubernetes guide describes server, Scribe, Oracle, and Forge role targets, one logical gateway, HTTP versus gRPC traffic, persistent versus scratch volumes, and how to check correct routing.
- S3, GCS, and Azure examples use the server's current schemes and credential behavior; non-Kubernetes containers have a clear equivalent deployment contract.
- The related docs no longer contradict current Bifrost durability and role ownership; optional internals stay in secondary explanation or reference.

## Expected Write Set and Consumer Closure

Likely `docs/src/content/docs/self-hosting/`, relevant `docs/src/content/docs/bifrost/`, and directly linked configuration/storage reference pages. Use checked-in `docker/`, `deploy/kubernetes/`, server config, and operations docs as evidence, not as permission to alter deployment code. Additional task pages use TASK-002's site pattern.

## Verification and Evidence

- `mise run docs:check` proves generated output, commands, links, static build/search, and accessibility.
- Validate each cited container image, YAML, environment key, target, health endpoint, and route against current source or shipped artifact. Where repository-managed deployment tests exist, use their owning `mise` lane; record commands and observed results for any runnable claim.
- Review one team-deployment and one Kubernetes page at desktop and 320px in both modes, checking decision-point disclosure and visible security/durability warnings.

## Material Stop Conditions

Stop if documenting the requested result as runnable requires a new image release, Helm chart, gateway contract, production policy/audit implementation, or server change. State the unsupported boundary; implementation belongs to a separately approved change.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Design doctrine](../../../../architecture/wyrd-design.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Deployment and release](../../../../architecture/operations/deployment-and-release.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
