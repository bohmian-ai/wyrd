# Wyrd Doctrine Reference Library

`architecture/references/` is Wyrd's reusable technical knowledge library. It
translates the repository's architecture and engineering rules into focused,
durable guidance. Start here and load only the slices required by the design or
implementation concern.

## Authority hierarchy

The references do not create independent architecture. Apply them under the
following authorities:

| Authority | Governs |
|---|---|
| `AGENTS.md` | Repository-wide ownership, coding, testing, verification, and contribution rules |
| `architecture/agent-rules.md` | Mandatory implementation boundaries, including SQL tenancy, audit, SSRF, and repository gates |
| `architecture/wyrd-design.md` | Wyrd protocol, Card model, identities, cross-system contracts, and public surfaces |
| `architecture/bifrost-design.md` | Bifrost ingest, storage, maintenance, and query architecture |
| `architecture/wyrd-security-posture.md` | Trust boundaries, authorization, tenant security, audit integrity, credentials, and security operations |
| `architecture/operations/` | Deployment, release, capacity, backup, recovery, and incident contracts |
| `architecture/wyrd-doctrine.mdx` | Product rationale and design doctrine |
| `architecture/references/` | Focused application guidance derived from the authorities above |

All applicable authorities must be satisfied. A focused reference yields to
the governing architecture or repository rule when they disagree. Reference
prose describes the required architecture; implementation that differs is
drift, not precedent.

## Layout

```text
references/
  doctrine/     product framing and canonical boundary facts
  architecture/ implementation ownership and structural patterns
  languages/    Rust, PyO3, Python, TypeScript, testing, and agent surfaces
  domain/       Vala, telemetry, evaluation, drift, and analytical systems
```

## Canonical routes

| Reference | Load when the question or change touches |
|---|---|
| `doctrine/positioning-and-vocabulary.md` | Card vocabulary, envelope, `CardRef`, v1 kinds, or removed concepts |
| `doctrine/architecture-constraints.md` | Wyrd/Vala/Skald boundaries, deployment, tenant isolation, or observation identity |
| `architecture/patterns.md` | Ownership, contract placement, server/client/storage/provider/audit structure |
| `languages/spec-driven-development.md` | Approved change specs, task decomposition, TDD execution, remediation, or final evidence mapping |
| `languages/implementation-execution.md` | Execution authority, adaptation, verification recovery, or completion evidence |
| `languages/rust-core.md` | Rust ownership, async, traits, allocation, or API shape |
| `languages/pyo3-boundaries.md` | PyO3 classes, GIL, lifetimes, conversion, or module registration |
| `languages/python-api-and-stubs.md` | Python exports, stubs, package layout, or typing |
| `languages/typescript-guide.md` | TypeScript SDK, declaration files, or napi boundaries |
| `languages/testing-workflows.md` | Journey, integration, unit, or repository verification gates |
| `languages/agent-harness.md` | MCP, agent-facing contracts, structured validation, or audit |
| `languages/errors.md` | Stable errors and Rust/Python/TypeScript/HTTP/CLI mapping |
| `domain/vala-architecture.md` | Broad Vala architecture, ownership, Bifrost orientation, or cross-domain advice |
| `domain/telemetry-observations.md` | OpenTelemetry signals, correlation, observation identity, or payload sensitivity |
| `domain/evaluation.md` | Eval-backed Verifiers, scenarios, judge quality, scoring, or evidence |
| `domain/drift-monitoring.md` | Drift signals, baselines, thresholds, alert noise, or monitoring policy |
| `domain/olap-serving.md` | Bifrost tables, ingest/query serving, admission, tenant safety, or analytical APIs |
| `domain/iceberg.md` | Iceberg snapshots, catalogs, schemas, partitions, object storage, or compaction |
| `domain/datafusion.md` | Logical/physical plans, provider pushdown, pruning, stats, memory, or spills |
| `domain/arrow-analytical-interop.md` | Arrow, RecordBatch, Parquet, PyArrow, FFI, or Python analytical boundaries |
| `domain/analytical-operations-reliability.md` | Backpressure, durability, leases, repair, retention, SLOs, or failure recovery |

## Compound routing examples

Use the smallest complete set; compound concerns should load each named slice:

| Request shape | Route |
|---|---|
| “Should Wyrd own this analytical behavior or expose it through a client?” | `doctrine/architecture-constraints.md` + `domain/vala-architecture.md` + `architecture/patterns.md` |
| “How should we evaluate an agent and turn failures into a safe release signal?” | `domain/evaluation.md` + `domain/telemetry-observations.md` + `domain/drift-monitoring.md` + `doctrine/positioning-and-vocabulary.md` |
| “Why did latency drift page after a trace pipeline change?” | `domain/telemetry-observations.md` + `domain/drift-monitoring.md` + `domain/analytical-operations-reliability.md` |
| “Which query shape will prune files and stay tenant-safe?” | `domain/olap-serving.md` + `domain/datafusion.md` + `domain/iceberg.md` |
| “When should compaction refresh the catalog, and what can fail?” | `domain/iceberg.md` + `domain/datafusion.md` + `domain/analytical-operations-reliability.md` + `domain/olap-serving.md` |
| “How do Python callers receive analytical data without copies or hidden IO?” | `domain/arrow-analytical-interop.md` + `languages/pyo3-boundaries.md` + `languages/python-api-and-stubs.md` |
| “How should an MCP or HTTP surface expose this capability?” | `languages/agent-harness.md` + `doctrine/positioning-and-vocabulary.md` + `domain/vala-architecture.md` |
| “How should this change move from approved behavior to TDD tasks and evidence?” | `languages/spec-driven-development.md` + `languages/implementation-execution.md` + `languages/testing-workflows.md` |
| “This is a non-Vala Card, SDK, Rust, or server question.” | Start with the matching `doctrine/`, `architecture/`, or `languages/` slice; do not load domain references unless the evidence crosses into Vala. |

## Reading rules

- Read every selected file completely and follow its Wyrd anchors back to the
  governing architecture where necessary.
- Use implementation inspection to measure conformance, never to redefine an
  architectural requirement.
- When a reference includes a `Primary grounding` section, treat those links as
  technical sources rather than Wyrd authority. Revalidate version-sensitive
  API behavior and standards against primary sources.
- Keep this library concise and nonredundant. Add or extend a reference only
  when a durable knowledge gap is demonstrated.
