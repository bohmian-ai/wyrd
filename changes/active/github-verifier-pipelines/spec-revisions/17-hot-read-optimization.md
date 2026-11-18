---
id: SPEC-github-verifier-pipelines
revision: 17
status: approved
approved_on: 2026-11-18
amends: ../spec.md
design: ../design/task-012-r2-hot-reads.md
---

# Hot-read optimization — revision 17 amendment

The maintainer selected the [hot-read design](../design/task-012-r2-hot-reads.md)
on 2026-11-18 and explicitly approved this scope amendment the same day. It is
incorporated into the current specification as revision 17. Approval does not
claim implementation or verification.

## Amended scope

**REQ-024 — Compatibility and qualification** is extended. Beyond revision 16's
shared pressure/CPU-dispatch integration, Bifrost may change how its existing
captured hot sources execute, through
[TASK-012-R2](../tasks/012-R2-hot-read-optimization.md):

- captured memtable/staged sources open from retained descriptors and
  statistics, and filter, project and reconstruct bounded batches only as the
  execution stream pulls;
- adaptation and recorded facts are reused per generation, keeping zero-copy
  buffers; and
- existing scalar statistics gain conservative UTF8/fixed-binary identity
  bounds, with their append and memory cost measured.

Everything else REQ-024 preserves remains: pools, memory/workspace policy,
query semantics and results, catalog, WAL/fence/durability contracts,
configuration names, wire/SDK contracts, no schema/migration, no new
dependency, pool, scheduler, index service, cache, source mode or durable
projection. REQ-018's no-regression standard applies unchanged, and the
original-SQL baseline is recorded before any production read edit. No absolute
UI latency target or hot/Parquet ratio is agreed.
