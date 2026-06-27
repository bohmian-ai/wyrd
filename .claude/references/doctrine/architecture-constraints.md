# Architecture Constraints

Use this as the fast doctrine review checklist before loading deeper planning
files.

## Product Boundaries

- `wyrd` is the control plane: registry, storage, lineage, policy, install, and
  audit.
- `vala` owns observability: observations, traces, drift, eval execution,
  archival query, and background data-plane behavior.
- `skald` owns LLM runtime behavior: providers, prompt execution, agents,
  workflows, orchestration, and provider-specific wire handling.
- Vala and Skald must not depend on each other directly. Shared contracts belong
  in Wyrd foundations or shared crates.
- User code owns user application execution, training loops, app servers, and
  arbitrary inference or agent loops.

## Foundation And Contract Boundaries

- `wyrd-spec` is PyO3-free, IO-free, async-free, tokio-free, SQL-free,
  cloud-SDK-free, object-store-free, Arrow-free, DataFusion-free,
  Iceberg-free, and Delta-free.
- Foundation types include the card envelope, metadata, `CardRef`,
  relationships, status, versioning, identifiers, serialization, schemas, and
  stable error codes.
- Public identifiers should use domain types, not raw strings.
- Public Wyrd contracts are exhaustive by default; add extension points only
  when the plan documents why.

## Service Ownership

- Registration, version assignment, relationship derivation, policy checks, and
  audit writes belong to the control plane.
- Storage owns durable bytes, object keys, hashing, upload/download
  orchestration, and byte persistence.
- Lineage reads CardRefs and derived relationships; it is not a separate
  authored graph model.
- Runtime outputs persist as runs, observations, artifact cards, relationships,
  audit records, or status updates through service-owned paths.

## Surface Alignment

- HTTP, Python SDK, CLI, UI, MCP, IDE integrations, generated schemas, and docs
  must project the same Wyrd contract.
- Surfaces may add ergonomics, defaults, and validation messages. They must not
  rename durable fields, expose server-internal state, or create a second
  vocabulary.
- Management-plane writes require request IDs, trace context, actor identity,
  auth/policy decisions where relevant, redacted payload summaries, and durable
  audit.
