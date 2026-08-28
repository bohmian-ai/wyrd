# Architecture Constraints

Use this as the fast doctrine checklist before loading deeper architecture,
language, or domain references.

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
- Public Wyrd contracts are exhaustive by default. Add an extension point only
  when the protocol requires runtime extensibility that a closed type cannot
  represent honestly.

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

## Identity And Tenant Isolation

- The verified principal supplies `tenant_id`; callers never choose tenancy
  through an untrusted header or payload field.
- Durable identifiers use typed domain identities. A string prefix is not a
  substitute for a discriminated identity type.
- `TenantConn` and Postgres row-level security are the tenant data boundary.
  Tenant-scoped code must not add hand-written tenant filters or accept raw
  pools, connections, or transactions.
- The caller owns a `TenantConn` transaction. A callee must not commit or roll
  it back, so related durable operations can remain atomic.
- Cross-tenant work requires `OperatorPool` under explicit platform authority;
  it never widens a tenant query into an administrative query.
- Tenant identity qualifies registry, audit, Bifrost table, object-store,
  cache, WAL, spill, generated-artifact, and telemetry boundaries.
- Observation `card_ref` values are asserted per row and authorized against
  the verified principal's card scope. `run_id` remains opaque and does not
  encode card or tenant identity.

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
- MCP reads and writes use the same typed contracts and authorization checks as
  HTTP and SDK calls. Read tools are always available; write tools require
  explicit scopes. No MCP path may bypass tenancy, policy, audit, or stable
  errors.

## Audit Boundaries

- Audit cardinality follows auditable domain operations and independently
  durable transitions, not endpoint or request count. Correlate related rows
  with request and typed domain identifiers.
- Every auditable Postgres transition appends its audit record in the same
  transaction as the transition. Workflows spanning transactions or external
  effects audit each meaningful commit boundary independently.
- Oracle read admission is the sole durability exception: a versioned,
  CRC-framed local WAL record is fsynced before rows are permitted, then relayed
  at least once into the canonical tenant audit outbox.
- Forge may write tenant-owned audit only through the tenant-bound,
  fence-checked `OperatorAudit` capability in the same operator transaction.
  It is not a general SQL or audit escape hatch.

## External Network Safety

- `Source` adapters and operator HTTP actions are server-owned, read or act
  through typed contracts, and never expose credentials in Cards.
- Before using a tenant-supplied URL, resolve DNS once, reject every disallowed
  address, and pin the connection to the screened address. Re-resolution after
  validation is an SSRF and DNS-rebinding vulnerability.
- Cloud metadata and link-local addresses are always forbidden. Production
  also rejects loopback, private, carrier-grade NAT, and unique-local ranges.
- Redirects repeat the same resolve, screen, and pin process before the next
  request.

## Deployment Topologies

Wyrd runs three ways; every design must support all three:

- **Self-hosted** — one or more operator-selected tenants in one deployment.
- **Cloud SaaS** — single-server multi-tenant with full tenant separation
  for identity, authz, storage, registry, policy, audit, observability,
  evaluation, and generated artifacts.
- **Enterprise cloud** — single-server single-tenant.

Enterprise is a deployment topology, not a commercial edition. Wyrd is open
source and independently publishable. Proprietary extensions may depend on
public Wyrd crates; Wyrd never depends on a private extension repository.

**"Single-server" denotes one logical serving surface and deployment
authority — not a single process or pod.** `wyrd-server` stays the only serving
surface, while a topology may run multiple horizontally scaled replicas and
targeted pods that activate selected subsystems through `WYRD_TARGET`. All
replicas remain one logical surface behind one gateway.

## Observation Identity

- One JWT can carry multiple component cards (nested service).
- Each observation row carries `card_ref` (per row, server-authorized,
  not trusted from the client) plus opaque client-generated `run_id`.
- Run IDs are opaque client-side execution records, not server-persisted.
- See `architecture/wyrd-design.md` §Observation identity.

## Verification Contract

- Every user- or agent-facing capability ships a real client → server → client
  journey for every public surface it exposes.
- Journeys cover happy, edge, and negative behavior, including authorization,
  replay, schema conflict, backpressure, and rejection paths where applicable.
- Integration and unit tests prove narrower seams; neither substitutes for a
  missing user journey.
- Generated contracts are regenerated from their owning sources and checked
  for drift. Generated artifacts are never edited by hand.
