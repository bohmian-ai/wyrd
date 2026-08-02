# Architecture Constraints

Use this as the fast doctrine review checklist before loading deeper
planning files.

## Product Boundaries

- `wyrd` is the control plane: registry, storage, lineage, policy,
  install, audit, server, CLI, MCP.
- `vala` owns observability and analytical storage: observations, traces,
  drift, eval execution, archival query, background data-plane behavior,
  the Bifrost engine (Iceberg + DataFusion).
- `skald` owns LLM runtime behavior: providers, prompt execution, agents,
  workflows, orchestration, provider-specific wire handling.
- Skald must remain independent of Vala. Vala **may** depend on Skald to
  implement reusable agent evaluation (including offline evaluation
  independent of `wyrd-server`). `wyrd-server` consumes the Vala
  evaluation engine but does not own evaluation-engine logic.
- **`wyrd-server` is the only serving surface.** `vala-*` crates are
  engine/data-plane libraries — never HTTP/gRPC serving crates.
- User code owns user application execution, training loops, app servers,
  and arbitrary inference or agent loops.

## Foundation And Contract Boundaries

- `wyrd-spec` is PyO3-free, IO-free, async-free, tokio-free, SQL-free,
  cloud-SDK-free, object-store-free, Arrow-free, DataFusion-free,
  Iceberg-free, and Delta-free.
- Foundation types include the card envelope, metadata, `CardRef`,
  relationships, status, versioning, identifiers, serialization, schemas,
  and stable error codes.
- Public identifiers use domain types, not raw strings.
- Public Wyrd contracts are exhaustive by default; add extension points
  only when the plan documents why.

## Client-Tier Constraints

Enforced by `check:client-tier`:

- Client-tier crates do not depend on `sqlx`, cloud SDKs, `datafusion`,
  or `iceberg`.
- Shared shells stay `pyo3`- and `sqlx`-free.
- Skald crates keep locked Wyrd edges — no server-tier imports.

## Service Ownership

- Registration, version assignment, relationship derivation, policy
  checks, and audit writes belong to the control plane.
- Storage owns durable bytes, object keys, hashing, upload/download
  orchestration, and byte persistence.
- Lineage reads `CardRef`s and derived relationships; it is not a
  separate authored graph model.
- Runtime outputs persist as runs, observations, artifact cards,
  relationships, audit records, or status updates through service-owned
  paths.

## Surface Alignment

- HTTP, Python SDK, Rust SDK, TypeScript SDK, CLI, UI, MCP, IDE
  integrations, generated schemas, and docs must project the same Wyrd
  contract.
- Surfaces may add ergonomics, defaults, and validation messages. They
  must not rename durable fields, expose server-internal state, or create
  a second vocabulary.
- Management-plane writes require request IDs, trace context, actor
  identity, auth/policy decisions where relevant, redacted payload
  summaries, and durable audit.

## Deployment Topologies

Wyrd runs three ways; every design must support all three:

- **Self-hosted** — single tenant, single deployment.
- **Cloud SaaS** — single-server multi-tenant with full tenant separation
  for identity, authz, storage, registry, policy, audit, observability,
  evaluation, and generated artifacts.
- **Enterprise cloud** — single-server single-tenant.

Enterprise is a deployment topology, not a commercial edition. Wyrd is
open source and independently publishable; a future private
`wyrd-enterprise` repo may depend on and extend public Wyrd crates, but
Wyrd never depends on that private repo.

**"Single-server" denotes one logical serving surface and deployment
authority — not a single process or pod.** `wyrd-server` stays the only
serving surface (see Product Boundaries), but a topology may run it as
multiple horizontally-scaled replicas and as targeted pods that each
activate a subset of its subsystems (selected by `WYRD_TARGET`; see the
targeted-deployments spec). Multi-pod targeted deployment is an intended
elaboration of these topologies — the same binary scaled out behind one
gateway — not a departure from the single-serving-surface rule.

## Observation Identity

- One JWT can carry multiple component cards (nested service).
- Each observation row carries `card_ref` (per row, server-authorized,
  not trusted from the client) plus opaque client-generated `run_id`.
- Run IDs are opaque client-side execution records, not server-persisted.
- See `architecture/wyrd-design.md` §Observation identity.
