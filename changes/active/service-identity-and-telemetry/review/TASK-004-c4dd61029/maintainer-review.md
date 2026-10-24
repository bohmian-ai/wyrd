# Maintainer review

Result: **FAIL**

Subject: cumulative `3e7c59db7...c4dd61029a54c44017c63bd4ff469472c406bc13`. HEAD still equals the candidate. This is an independent static review; no other review reports were read, no source was changed, and no Cargo command was run.

## Authority and coverage

Read the shared subject, original TASK-004, approved table-storage supplement, relevant Bifrost table identity/layout/lifecycle authority, root rules, maintainer style, Rust/errors/agent-surface guides, and Python/TypeScript/docs directory rules. CodeGraph is absent. The task's verification table and diagnoses are supplied evidence, not independently repeated results.

| Changed surface | Symbols and consumer coverage | Assessment |
| --- | --- | --- |
| Wire contracts | `PartitionTransformWire`, `PartitionFieldWire`, `PhysicalLayoutWire`, `PartitionValueWire`, `PhysicalPartitionWire`, partition error fields/violations and wire round trips | Canonical spelling reaches consumers; no second contract introduced. |
| Layout and catalog | `LayoutRevision`, `TimeAxis` retained-spec lookup, `PartitionTransform`, `LayoutPartitionField`, `PhysicalLayout::{resolve,of_iceberg,to_wire,iceberg_partition_spec,iceberg_sort_order}`, canonical validators, `TimePartition` Iceberg conversion; `BifrostCatalog::{committed_revision,retained_layout,evict_committed_revision,validate_physical_table}` and pin tests | Existing registration and Iceberg metadata owners are extended. Retained recipe reconstruction has the fingerprint inconsistency below. |
| Composite identity | Entire new `physical_partition.rs`: `PhysicalPartition`, codecs, `PartitionScheme`, `RowPartitions`, `DimensionValues`, error enum; `SealKey`, `ScribeClaimIdentity`, `PartitionPlan`, split/planning functions and inline cases | Coherent invariant-owning types; Iceberg transforms are reused. No alternative engine or parallel partition planner survives. Rustdoc gaps below. |
| Admission and WAL | `LogicalFrameContract`, `RootAdmission`, `resolve_logical_frame`, `scheme_of`, `AdmittedAppend`, `NativeSliceProducer`, `prepare_rows`, `prepare_append`, slice construction; WAL revision/partition encoding and decoding, replay key indexing and SQL replay suppression | Revision is carried through existing owners. Composite/WAL, time-partition, crash, persistence, closeout and replay-suppression tests inspected. |
| Staging, authority and publication | `ScribeAssemblyKey`, layout digest, claim publisher authority calls, `StagedHotSourceRecordV1`, member staging, hot capture/supersession, persistence dependency wiring, `resolve_write_recipe`, `restore_context`, manifest conversion, file-list insert/replay/publication comparison, promotion record conversion | Existing lifecycle extended rather than duplicated. Bloom recipe and durable key identity disagree, as detailed below. |
| Oracle and sizing | Follower live-tail request, `FetchLiveTailRequest`, retained capture read, `ReadableSource::project`, `projection_indices`, `ScanSizer::row_group_bytes`, published scan schema selection, codec revision | Field-ID projection lives at the captured-source owner, with focused pure proof and mixed-source journeys. No new transport/cache/query owner. |
| Parquet and Forge | `ParquetEncoded`, `ParquetBatchEncoder` partition propagation, writer metadata and sort/Bloom tests; `PromotedObjectFooter::decode`, `validate_promoted_object`, `rewrite_group_partition` | Footer verification is reused over retained schemas; duplicate Forge time decoding deleted in favor of catalog owner. |
| Definitions and migration | `hourly_layout`, `daily_layout`, `hourly_test_layout`, `DomainTable` default, all changed audit/drift/gateway/logs/metrics/traces/verification declarations, table layout tests; new `file_list.partition_dimensions` migration | Complete Bloom sets replace the removed shared floor; canonical dimension bytes remain beside independent time bounds. |
| Harness and journeys | New `bifrost/revisions.rs` functions and `WyrdTestServer` revision methods; Scribe create/refusal, fanout, lifecycle/peers, accepted-revision and mixed-spec tests; Forge physical-truth helper/journey and shared append/live-snapshot helpers; revised Forge corruption mutations and existing Oracle/closeout/fencing fixture consumers | Reuses `WyrdTestServer`, public ingest/query helpers, held-root restarts, real catalog seam, and existing Forge observer. New revisions helper is shared by multiple journeys rather than a harness for one branch. |
| Public projections and docs | Rust client declaration docs/description fixtures, server refusal fixture, MCP discovery assertion; Python `_layout_json`, `TableConfig` entry points, `PartitionField`/`PhysicalLayout`, exports and authored re-export stub, typecheck/integration fixtures; TS `TableLayout`, `layoutJson`, imports/exports and generated wire declarations; generated errors and custom-data guide | Rust wire field names and parameter values reach JSON consistently. Python inline types are re-exported from the authored stub; there is no duplicate native signature to update. Generated TypeScript shape agrees with source declarations; actual regeneration is supplied evidence. |
| Dependency pins | Workspace/Cargo.lock revisions, Oracle fingerprint constant; local pinned DataFusion schema-rewriter diff, Iceberg scan override and retained-spec append/snapshot diff, compaction pin-only diff | Existing upstream owners extended for field-ID lookup and retained-spec append. No additional dependency or second native universe. Fork tests are supplied evidence. |

Mechanically adjusted fixtures include client observe/describe, redux bench and staging/merge helpers, Wyrd workload/Forge fixtures, Rust/Python/TS time-column journeys and MCP discovery. Their changed declarations and caller expectations were inspected. Change-packet files are authority/context, not additional implementations attributed to this task.

## Material findings

### MNT-001 — Changed Rust items do not meet the explicit rustdoc gate

Classification: **VIOLATION**. Location: `crates/vala/vala-bifrost-redux/src/scribe/seal_key.rs:618` and nearby new helpers; `catalog/physical_partition.rs:356`.

Root `AGENTS.md` requires rustdoc on every new/changed Rust item and `# Errors` on fallible items; `agent_docs/rust.md` explicitly includes private helpers and tests. The new `composite_partition_identity_and_wal` test has no rustdoc at all. The new fallible `fixed` codec helper has only a one-line description and omits its error contract. New `composite_batch` (line 568) and `composite_scheme` (line 595) use `expect` without documenting their panic preconditions. New `RowPartitions::partition` (`physical_partition.rs:530`) and `DimensionValues::value` (line 573) index arrays but do not document the required in-bounds index/panic condition.

Concrete cost: a maintainer cannot determine the new combined partition/WAL proof's intended invariant and fixture constraints from its item documentation, or distinguish malformed input errors from indexing/fixture precondition failures at the new codec boundary. This is blocking because the repository explicitly makes missing rustdoc a merge gate, independently of stylistic preferences.

Smallest correction: document these existing items in place, including the named test's proof, exact error condition, and actual fixture/index panic preconditions. Do not add helpers, lint exemptions, or new tests for this documentation correction. Nearby `assert_refused` and `bounded_composite_layout_validation` in `catalog/layout.rs` demonstrate the required intent/`# Panics` shape. Closure proof is source inspection plus normal format/lint/doc checks for the touched crate.

### MNT-002 — Reconstructing a retained recipe from the current Bloom property can invalidate a durable staged key

Classification: **INCORRECT**. Changed location: `crates/vala/vala-bifrost-redux/src/catalog/layout.rs:1007`; affected shared owner: `scribe/assembly.rs:244` and `scribe/staging_runtime.rs:825`.

TASK-004 scenario 3 requires accepted work to survive compatible layout revisions and restart; the approved supplement's durable lifecycle requires retained layout binding. Maintainer style requires one owner of an invariant so future changes can be made safely.

`PhysicalLayout::of_iceberg` selects the retained schema/spec/order but takes Blooms from the **current** table property, explicitly describing Blooms as outside file identity. However, `ScribeAssemblyKey::layout_fingerprint` hashes the complete ordered Bloom list. A member staged under property `[]` retains that fingerprint in `StagedHotSourceRecordV1`; a later Iceberg `SetProperties` changing it to `["customer"]` leaves all retained schema/spec/order IDs unchanged. On restart, `restore_context` calls `resolve_write_recipe` → `retained_layout` → `of_iceberg`, rebuilds the key using `["customer"]`, and rejects it at `resolved != *key` with `the registered write recipe no longer reproduces a recovered staged key`. Even reordering the same nonempty Bloom set changes this fingerprint. Both live persistence and restore use the same recipe resolver, so adding another guard at either caller would not fix the source inconsistency.

This is reachable through the catalog revision seam deliberately provided for this task (`WyrdTestServer::commit_table_revision` can commit `SetProperties`); no public PATCH implementation is needed to expose it. The existing mixed-revision journeys change spec/order/schema but leave the Bloom property empty, so they do not exercise this path.

Concrete consequence/cost: otherwise valid durable staged work becomes unrestorable after a permitted pruning-policy change; maintaining a pruning recipe requires understanding an undocumented second identity contract in assembly. Current metadata reconstruction cannot be safely changed in isolation despite the new method's retained-revision contract.

Smallest correction boundary: reconcile optional Bloom policy and compatibility identity at the existing shared layout/assembly owner. Given the implementation's explicit choice that Blooms use current policy and are not file identity, stop binding Bloom membership/order into the staged compatibility fingerprint; preserve schema, sort, complete partition identity, tenant, checksum, and authority checks. Keep actual output Blooms driven by the resolved writer recipe. This needs no second recipe store, lifecycle owner, or caller-specific recovery exception. If accepted Bloom membership is instead part of retained identity, it must be resolved from retained facts rather than the current property; do not silently choose that larger contract.

Focused closure proof: stage real composite rows, commit a Bloom property-only change using the existing catalog seam, restart on the retained root, publish and read each accepted row once; inspect output Bloom metadata against the selected recipe. Include a same-membership reordered list to show that ordering does not strand staged work. Reuse the existing staged-recovery/journey fixture and writer metadata inspection rather than introducing a harness.

## Non-blocking notes

`TimeGranularity` documentation still says multi-column partitioning is unrepresentable, and some unchanged write-recipe prose still describes managed columns being narrowed although resolution now validates explicit complete lists. These are stale explanations near the changed owner, not additional behavioral findings. No naming/layout preference is elevated to a blocker.

## Verification limits

No tests or generators were rerun. Supplied evidence reports format/lints/dependency/isolation/codegen checks, 919 redux tests, 35 Scribe journeys and the Forge journey passing; that evidence does not cover Bloom-property revision during staged recovery. No optional refactor, new feature, or broader public API acceptance is requested.
