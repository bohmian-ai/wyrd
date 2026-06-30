# Surface Mapping

Use this file to route review effort. It is a predecessor evidence map, not a
source of Wyrd public vocabulary.

## OpsML Evidence For Wyrd

Inspect opsml for:

- card lifecycle and local materialization patterns
- registry and storage orchestration
- CLI, UI, HTTP, auth, and MCP structure
- PyO3 extension and Python export patterns
- test organization and maturin workflow

Review risks:

- preserving old table-per-kind registry design
- client-side storage/cloud SDK leakage
- per-card registration methods
- local constructors or save/load paths with durable side effects
- legacy names, route prefixes, imports, or compatibility aliases

## Scouter Evidence For Vala

Inspect scouter for:

- drift, eval, OTel-like ingest, alerts, workers, and OLAP patterns
- DataFusion/Delta/Arrow storage and query boundaries
- hot path versus archival query behavior
- control-plane metadata and data-plane isolation

Review risks:

- high-volume observations remaining in Postgres
- weak tenant isolation
- direct Skald dependencies
- observations or eval results not modeled as Wyrd observations/runs
- redesigning mature OLAP behavior without reason

## Potatohead Evidence For Skald

Inspect potatohead for:

- providers, prompts, agents, workflows, memory, callbacks, and streaming
- mock provider and fixture patterns
- provider-specific wire handling

Review risks:

- provider details leaking into Wyrd contracts
- Skald becoming the platform runner
- live callbacks, handles, or provider blobs entering durable specs
- mock crates leaking into normal dependencies
