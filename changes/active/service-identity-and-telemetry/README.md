# Implementation tasks

[Specification revision 12](spec.md) and its three supplements are approved.
The eight implementation tasks below are proposed for maintainer review.
Each runtime task includes its affected consumers, focused tests and owning
architecture updates. The docsite is deliberately the last implementation task.

## Execution order

Finish each batch before starting the next:

| Batch | Tasks |
| --- | --- |
| 1 | **TASK-001 and TASK-004 in parallel** |
| 2 | **TASK-002 and TASK-005 in parallel** |
| 3 | **TASK-003** |
| 4 | **TASK-006** |
| 5 | **TASK-007** |
| 6 | **TASK-008** |

These batches preserve the agreed execution order. Individual task metadata
records its actual dependencies. Batches also keep the schema task behind
configuration changes to shared table contracts, avoiding simultaneous edits.
Parallel tasks need separate task branches/checkouts; do not run overlapping
Cargo-backed commands against one checkout or shared target directory.

## Task outcomes and boundaries

| Task | Outcome | Boundary |
| --- | --- | --- |
| [TASK-001 — Flat Card identity](tasks/TASK-001-flat-card-identity.md) | Register and reference Cards without space. | Card identity, registry, configuration and their public consumers. |
| [TASK-002 — Stable accounts and auth](tasks/TASK-002-stable-accounts-and-auth.md) | Reuse accounts and credentials across releases while retaining validated exact-workload context. | Accounts, credentials, auth, context propagation, binding authorization and frozen execution/schedule authority. |
| [TASK-003 — System schemas and attribution](tasks/TASK-003-system-schemas-and-attribution.md) | Produce and query the complete approved schemas with separate account, workload and subject attribution. | Canonical table definitions/defaults, producers/extraction, managed admission, public bifrost catalog and query-consumer compatibility. |
| [TASK-004 — Shared table storage](tasks/TASK-004-shared-table-storage.md) | Preserve composite partition and schema/layout identity from acceptance through recovery, reads and compaction. | Physical engine foundations; no public table-alteration API. |
| [TASK-005 — Tenant table configuration](tasks/TASK-005-tenant-table-configuration.md) | Configure system/dynamic layouts and make safe atomic schema edits. | Public create/describe/PATCH, type rules, configuration revisions, persistence and startup/cache behavior using Task 4 foundations. |
| [TASK-006 — Verification trace evidence](tasks/TASK-006-verification-trace-evidence.md) | Evaluate the visible whole trace consistently and retry queued missing evidence. | Trace selection/reuse and execution availability; no trace-completeness protocol. |
| [TASK-007 — Verification query performance](tasks/TASK-007-verification-query-performance.md) | Measure and improve actual verification evidence lookups without changing authorized evidence. | Existing result, trace, drift/window, group and dynamic-query paths; no invented latency promise or separate warehouse. |
| [TASK-008 — Docsite](tasks/TASK-008-docsite.md) | Explain and demonstrate the final shipped contracts. | Public documentation and its generated outputs, after all runtime tasks. |

Task 4 provides physical revision-aware storage. Task 5 exposes authorized
configuration changes through that owner. Task 3 applies the canonical schemas
and defaults. This separation does not create a second writer, catalog,
validator or maintenance lifecycle.

## Verification and handoff

Each task specifies focused proofs and exact commands for its write set.
Task 8 completes public documentation before integrated change review. Broad
cross-surface regression and final whole-change checks run on the integrated
candidate under the existing change-review workflow, rather than being copied
into every task. Planning does not execute tests or implement the change.
