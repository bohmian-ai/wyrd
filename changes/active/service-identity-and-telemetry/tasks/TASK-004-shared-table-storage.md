---
id: TASK-004
kind: implementation
spec: SPEC-service-identity-and-telemetry
spec_revision: 12
status: proposed
depends_on: []
requirements: [REQ-024, REQ-025, REQ-027, REQ-031, REQ-035, REQ-036, REQ-037, REQ-038, REQ-042, REQ-044, REQ-047, REQ-048]
invariants: [INV-001, INV-002, INV-006, INV-007, INV-008]
acceptance: [AC-007, AC-008, AC-009, AC-012, AC-013, AC-016, AC-018, AC-021]
---

# Shared physical table storage

## Outcome and Value

System and dynamic tables share one physical engine supporting time plus up to two explicitly declared typed partition dimensions. For example, an hourly customer-bucket table preserves its distinct buckets, including null, through restart and compaction. Accepted batches retain their original schema/layout; readers combine historical runtime specifications without falsely claiming that files were rewritten.

## What Changes and What Must Stay True

Carry complete typed partition tuples and retained schema/spec/sort identity throughout storage. Preserve independent time bounds, source field IDs, tenant binding, accepted origin, checksums and batch conflict checks. Nulls, literal values, types and different specifications must never alias.

Apply the [approved exhaustive layout/type bounds](../supplements/table-storage-and-correlation.md#supported-layout-types-and-schema-edits) through one validator: required UTC time partition, at most two eligible extra dimensions, bounded sort/Bloom declarations and Iceberg transform/null semantics. Count complete tuples before WAL acceptance: 32 succeeds; 33 leaves no durable or visible residue. More dimensions can create smaller files; compaction cannot cross partition boundaries to meet a target.

Schema IDs, partition-spec IDs, sort-order IDs and configuration revision remain distinct. Retries retain accepted binding; renames preserve IDs/values; nullable additions read null in older sources. Old files keep truthful ordering claims, and query pins protect retained metadata/files. Keep agreed writer/default/retention policy; resolved complete or empty Bloom sets have no hidden floor. Missing filters preserve exact reads. No automatic history rewrite, release migration, new row TTL, tuning or second engine.

**Boundaries.** This task owns creation/layout validation, physical revision preservation and necessary layout/partition wire-consumer updates. Its shared catalog/storage capability resolves retained layouts, new admission under a committed revision and previously accepted work under its original revision. TASK-005 owns PATCH, configuration CAS, legal schema-edit orchestration, committed-edit cache refresh and startup overrides, using this capability. Internal evolution proofs here may establish revisions through the catalog/Iceberg seam. TASK-003 owns system definitions/default sets, envelope production, catalog renaming and ledgers. Preserve the admitted per-record origin. Update owning Bifrost architecture prose here; TASK-008 owns final docsite work. No Card/auth prerequisite is needed.

## Approach

1. Extend shared declarations/validation and complete typed partition identity.
2. Retain accepted revisions through admission, WAL, recovery, seals and staging.
3. Publish actual schema/spec/sort metadata and extend Oracle local/peer mixed-spec reads.
4. Extend Forge through the same retained-metadata owner and verify sorting/Bloom output.
5. Close real-client lifecycle/compaction journeys and focused metadata/corruption/pruning proofs.

## Ordered Implementation Scenarios

### Scenario 1 — A declaration resolves one bounded typed layout

**Behavior.** Create/describe defaults or valid hour/day plus identity/bucket/truncate layouts, nullable extra dimensions and supported scalar sort/Bloom sources. Refuse every unsupported type/bound, including nullable/timezone-naive time axes, without registration or physical-table residue. Same-declaration registration remains idempotent; a changed declaration never silently alters it.

**RED.** Add [layout validation](#v1-layout-validation) and [creation journey](#v2-creation-journey) with case tables and exact errors: current time-only contracts reject valid composite declarations.

**GREEN.** Extend existing declaration/schema/layout and creation consumers to the approved matrix/defaults; expose only canonical `partition_by`, remove replaced `partition_granularity` configuration spelling, and run both proofs.

**REFACTOR.** Consolidate shared validation inside its current owner; keep both proofs green without a parallel registration path.

### Scenario 2 — Composite fanout is bounded before acceptance

**Behavior.** Distinguish extra dimensions within one hour, null/literal/type/spec identities and Iceberg bucket/truncate values. Repeated tuples count once; 32 distinct tuples succeed; 33 refuses the whole batch before append/fence, leaving no rows or WAL/fence residue.

**RED.** Add [partition/WAL proof](#v3-partition-identity) and [atomic fanout journey](#v4-fanout-journey), including corrected-write success after refusal: time-only grouping merges tuples or misses composite fanout.

**GREEN.** Extend shared planning/routing/seals with complete-batch validation before WAL; preserve row ordering/conflict inputs and rerun scenarios 1–2.

**REFACTOR.** Reuse time-axis/split/admission owners and remove projections that collapse identity.

### Scenario 3 — Accepted identity survives lifecycle and runtime evolution

**Behavior.** Accept composite/null partitions, then commit compatible revisions through the shared catalog seam; preserve accepted schema, IDs, partition/order, time bounds, origin and batch identity across these cases:

| Boundary | Expected observation |
| --- | --- |
| Active, immutable, staged, published and peer | Public reads return each accepted row once; authenticated fragments never read another pod's paths. |
| Restart | WAL/manifests recover full tuples/origin; corrupt or contradictory evidence fails closed. |
| Retry across alteration/restart/publication | Original acceptance instant/binding persists without duplication; changed payload refuses. |
| New admission | Uses committed revision; failed metadata changes never redirect accepted work. |
| Nullable addition and ID-preserving rename | Old values resolve by stable field ID; additions read null; historical partition/order claims remain true. |
| Mixed-spec time/equality/null/membership filters | Exact results across local/hot/published/peer sources, including missing historical filters. |
| Tenant refusal and pinned read during revision/cleanup | Foreign sources remain inaccessible; pins retain metadata/files until release. |

**RED.** Add [lifecycle journey](#v5-lifecycle-journey), [accepted revisions](#v6-accepted-revisions), [mixed specifications](#v7-mixed-specifications) and corruption cases in [partition/WAL proof](#v3-partition-identity): current time-only/latest-layout/default-schema assumptions lose dimensions, reinterpret retries or omit/misproject history.

**GREEN.** Extend existing catalog/fence/WAL/staging/publication/cut/peer consumers with retained identity; resolve accepted batches before current-write rules and each source under its own metadata, retaining exact filters, independent time bounds, tenant footers, checksums, authority and pins. Supply TASK-005's shared capability and rerun scenarios 1–3.

**REFACTOR.** Remove current-metadata reconstruction through existing catalog/cut-cache/codecs/query owners; no second ledger, transport, cache or query engine.

### Scenario 4 — Forge preserves partitions and historical truth

**Behavior.** Promote/compact old/new specs and nullable composite partitions without changing exact tenant-bound rows or crossing physical partitions. Promotion accepts valid retained specs; rewrites truthfully record their selected output schema/spec/order. Metadata-only edits never rewrite history; pins, retention and ambiguity reconciliation remain safe.

**RED.** Add [Forge journey](#v8-forge-journey), comparing physical metadata/partitions and public rows: latest-only promotion checks and time-only grouping reject or merge valid history.

**GREEN.** Extend existing promotion/compaction/publication/retention owners with retained identities, preserving fences, retries, reconciliation, targets and authorization; rerun scenarios 1–4.

**REFACTOR.** Remove fixed-ID/latest-only assumptions through the shared owner; no automatic historical rewrite orchestration.

### Scenario 5 — Physical sorting and optional Blooms preserve exact reads

**Behavior.** Write/compact mixed-Service/environment rows and inspect actual scalar sort direction/null order, Parquet metadata and pruning. Complete/partial/empty Bloom sets match resolved policy and return identical authorized rows with missing historical filters; Bloom-list ordering does not affect membership. Keep agreed encoding, page/row-group limits, compression, statistics, offsets and compaction targets.

**RED.** Add [writer/reader proof](#v9-writer-and-reader-proof) and extend [Forge output checks](#v8-forge-journey) to expose hidden floors, stale ordering claims, unsupported Bloom handling or pruning errors in actual encoded files.

**GREEN.** Reuse shared writer and bounded sort/merge owners; fix observed layout-application gaps, keep missing-filter fallback exact, rerun all scenarios and record representative file/group/page/byte and planning evidence.

**REFACTOR.** Delete duplicated recipe/floor logic while retaining existing constants and exact filters; add no tuning knobs.

## Acceptance Criteria

This task supplies the physical portions of the mapped obligations; system
schema/default inventory is proved by TASK-003, public edits by TASK-005, and
public language-surface closure by the SDK task.

| Obligations | Proof and unacceptable result |
| --- | --- |
| REQ-036, 047–048; AC-012, 021 | Scenarios 1–2 prove exhaustive supported matrices, creation defaults, complete lists and the 32/33 composite boundary. Accepting an unsupported declaration, merging null/type distinctions, or leaving refused-batch residue violates the contract. |
| REQ-024–025, 035, 037; AC-007–008, 012 | Scenario 3 proves real-client active/staged/published/peer/restart equivalence and tenant/source authority. A dropped dimension, altered origin, duplicate row or cross-tenant source violates it. |
| REQ-027, 031, 038; AC-009, 013 | Scenarios 3–4 prove future in-baseline evolution: stable field IDs, accepted-retry binding, mixed-spec visibility and safe pins. Reinterpreting an accepted batch under current metadata, losing history or claiming an unperformed rewrite violates it. No historical release migration is required. |
| REQ-035, 038, 042, 044; AC-008, 016, 018 | Scenarios 4–5 prove actual sort enforcement and exact complete/empty Bloom policy across writes and compaction. Query results must be equal with absent historical filters. Report physical file sizes/counts, scan/pruning evidence and planning overhead; no invented latency threshold or default tuning. |
| INV-001–002, 006–008 | All scenarios preserve authenticated tenant binding, table permissions, Rust-owned durable decisions, accepted origin and typed data. Existing snapshot/fence/recovery regressions remain green; no broader credential may hide a refusal. |

## Implementation Reference

Inspected workflow evidence supports extending existing owners. Paths guide changes, not a private implementation allowlist.

| Existing owner and inspected consumers/tests | Gap and selected extension |
| --- | --- |
| `wyrd-spec/src/vala/api.rs::{PhysicalLayoutWire,TimePartitionWire}`; `catalog/layout.rs::{PhysicalLayout,TimeAxis,TimePartition}` | Time-only wire/partition identity cannot distinguish dimensions/revisions. Extend shared validation, normalization, stable-ID transforms and codecs; preserve time bounds. Inline layout tests own the pure matrix. |
| `catalog/bifrost_catalog.rs::BifrostCatalog::{create_table,register_dataset,ensure_builtin,table_registration}`; `create_physical_table`, `validate_physical_table`, `pin_iceberg_snapshot` | Creation/cache/default-spec checks and manifest time bounds assume one current spec. Extend retained metadata/source-bound resolution in this owner; TASK-005 supplies edit orchestration. |
| `scribe/preprocess.rs::prepare_append`; `seal_key.rs::{plan_time_partitions,SealKey}`; WAL/replay, `hot_stage.rs::ScribeHotStage`, `assembly.rs::StagingAssembler` | Planner counts time buckets and codecs retain time-only identity. Extend the existing atomic acceptance/routing/memtable/staging/claim workflow; preserve fences, checksums and deterministic claims. Existing `scribe/tests/time_partition.rs` and inline seal-key tests cover the current seam. |
| `scribe/file_list_writer.rs`; `vala-sql/src/row_types/file_list.rs`, `queries/{file_list,oracle_reader_authority}.rs` | Fixed spec/sort IDs and granularity/start columns lose retained tuple/revision identity. Extend Vala metadata/migrations/query/reader-authority consumers with actual IDs and full tuple plus independent time bounds. Preserve RLS/fences; inspect existing migration owner and never rewrite applied migrations. |
| `oracle/{live,tail_discovery,follower,exec}.rs`; catalog cuts | Time-only peer routes, partition comparisons and installed order can collapse dimensions or misstate history. Extend discovery/local-peer scans and projection through retained specs; keep authenticated peers, table permissions, footer checks, exact filtering, cut cache and pins. |
| `forge/{scribe_promotion,worker,live_reconcile,publication}.rs`, `forge/managed/policy.rs` | Promotion compares latest default IDs and planning uses time-only identity. Extend existing fenced planning/promotion/compaction/publication/retention, keeping ambiguity reconciliation. |
| `scribe/parquet_writer.rs::ParquetBatchEncoder`, `scribe/claim_merge.rs`; `parquet/writer_properties.rs::{bifrost_writer_properties,bifrost_rewrite_writer_properties}` | Existing bounded merge/shared recipe is the correct owner; remove floor assumptions and apply resolved sorting/Blooms consistently, without new policy. Inline writer tests inspect physical output. |
| Redux `tests/integration/main.rs`; `wyrd-testing` targets `scribe` and `forge`, inspected `scribe/write_read.rs`, `forge/{scribe_promotion,production_closeout}.rs` | Reuse real-client WyrdTestCluster lifecycle and metadata/held-read fixtures. Archived integration siblings are not registered automatically; register new modules normally. Keep layout-wire Rust/Python/TS/HTTP/gRPC/generated consumers coherent; later SDK task owns full public-surface journeys. |

Unqualified production paths above are under `crates/vala/vala-bifrost-redux/src/`; SQL paths are under `crates/vala/`; contracts under `crates/`; journey paths under `crates/wyrd/wyrd-testing/tests/bifrost/`.

## Verification and Evidence

All names below are **planned**. Manifests confirm redux library/`integration` (self dev-dependency enables test-support) and `wyrd-testing` `scribe`/`forge` targets; mise confirms nextest journey profile, ignored-test selection, Postgres wrapper and server migration setup. Confirm each actual selector after adding it, before RED; update changed selectors and reject empty selections. Run Cargo commands sequentially. Reuse existing local storage/cluster fixtures without live cloud credentials.

### V1 Layout validation

Pure schema/layout matrix and default resolution, without database setup:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::layout::tests::bounded_composite_layout_validation)'
```

### V2 Creation journey

Real-client create/describe/idempotency/refusal through a booted server:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=describe::composite_layout_creation_and_refusal)"'
```

### V3 Partition identity

Pure transform/null/codec and local WAL round-trip/corruption evidence:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::seal_key::tests::composite_partition_identity_and_wal)'
```

### V4 Fanout journey

Public 32/33 acceptance boundary plus no partially visible rows:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=ingest_bounds::composite_partition_fanout_is_atomic)"'
```

### V5 Lifecycle journey

Client writes/queries through real retained-root restart and multi-pod reads:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=write_read::composite_partitions_survive_lifecycle_and_peers)"'
```

### V6 Accepted revisions

Real Postgres/storage seam proves retry identity and retained revision recovery:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=scribe::accepted_batch_revision_survives_alteration_and_restart)'
```

### V7 Mixed specifications

Real catalog/storage/Oracle seam proves projection, pruning, tenant refusal and
pinned-read lifetime across retained revisions:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=oracle::mixed_runtime_specifications_preserve_rows_and_pins)'
```

### V8 Forge journey

Real-client readback with physical output evidence across promotion/compaction:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all -E "test(=scribe_promotion::mixed_specifications_preserve_partition_and_sort_truth)"'
```

### V9 Writer and reader proof

Actual encoded-file sort/Bloom/recipe assertions and exact filtering fallback:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::resolved_sort_and_blooms_preserve_exact_reads)'
```

Record observed RED/GREEN results and selected-test counts, stable IDs/retry evidence, actual output metadata and reproducible query geometry/pruning/file sizes. TASK-005 adds public PATCH-to-engine proof; internal metadata setup here does not claim its API is tested.

After focused proof run `mise run test:bifrost:integration:redux`, `mise run fmt`, `mise run lints`, `mise run check:deps`, `mise run check:tenant-isolation`, and `mise run codegen:check`. Regenerate changed sources with `mise run codegen:regen`; never hand-edit outputs. Add the narrow owning binding check for actual consumer changes. Full language journeys/aggregate run once at integrated review.

## Material Stop Conditions

- The pinned dependencies cannot implement approved Iceberg/type/null/field-ID semantics: report the concrete gap instead of reducing the matrix or coercing values.
- Correctness requires weaker batch conflict checks, tenant/authority proofs, pins or publication fences: seek corrected design; never fall back to latest metadata.
- New storage/query/lifecycle ownership, automatic history rewrite, release migration, snapshot guarantees, retention or writer policy exceeds this task.
- Coordinate shared files with TASK-003/005. File overlap alone is not a dependency; record newly discovered semantic prerequisites. Different wyrd-spec edits do not make Task001 a prerequisite.

## Authority Links

- [Approved specification, revision 12](../spec.md) and
  [storage/layout supplement](../supplements/table-storage-and-correlation.md).
- [Bifrost table identity](../../../../architecture/bifrost-design.md#table-and-row-identity),
  [ingest](../../../../architecture/bifrost-design.md#ingest-scribe),
  [Oracle](../../../../architecture/bifrost-design.md#query-oracle), and
  [Forge](../../../../architecture/bifrost-design.md#maintenance-forge).
  Amend the owning sections with this task to remove time-only/fixed-revision
  conflicts; all unrelated ownership/durability rules remain in force.
- [Postgres tenant binding](../../../../architecture/wyrd-security-posture.md#postgres-tenant-binding),
  [repository rules](../../../../AGENTS.md),
  [spec-driven development](../../../spec-driven-development.md),
  [implementation execution](../../../implementation-execution.md), and
  [testing](../../../../TESTING.md).

## Implementation Evidence

Status: **IMPLEMENTED**. Commits `3bd495ce7`…`96fe37ee3` on `task/service-identity-and-telemetry/TASK-004`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| V1 bounded layout matrix and defaults (REQ-036, 047–048) | `catalog/layout.rs` single bounded validator (`3bd495ce7`) | `--lib catalog::layout::tests::bounded_composite_layout_validation` RED→GREEN | PASS |
| V2 creation, describe, idempotency, refusal | catalog create/describe through the validator (`3bd495ce7`) | `--test scribe describe::composite_layout_creation_and_refusal` | PASS |
| V3 composite partition identity, nulls, WAL round-trip/corruption | `catalog/physical_partition.rs`, `scribe/seal_key.rs`, WAL v7 (`f8d28ff1e`) | `--lib scribe::seal_key::tests::composite_partition_identity_and_wal` RED→GREEN | PASS |
| V4 32/33 fanout boundary, no residue | `scribe/preprocess.rs` counts complete tuples before WAL append/fence (`f8d28ff1e`) | `--test scribe ingest_bounds::composite_partition_fanout_is_atomic` | PASS |
| V5 lifecycle, restart, peers | full tuple in `file_list.partition_dimensions` and promotion records (`7e0b5039d`, `f38c2efaf`) | `--test scribe write_read::composite_partitions_survive_lifecycle_and_peers` | PASS |
| V6 accepted revision survives alteration and restart | `LayoutRevision` through admission, WAL, staging, seals, claims, publication (`141dae9d4`, `9ebd9b9e0`, `3001a98c3`); `scribe/write_recipe.rs` resolves the retained revision | `--test scribe write_read::accepted_batch_revision_survives_alteration_and_restart` (selector corrected: the redux `integration` scribe module is archived) | PASS |
| V7 mixed specs: field-id projection, filters, tenant refusal, pins | field-id resolution in memtable/tail RPC/follower; DataFusion fork `1eab549` (`feee0c92a`); pin across revisions (`2ca77eb9a`) | `--test scribe write_read::mixed_runtime_specifications_preserve_rows_and_pins` (selector corrected as V6); `--lib catalog::bifrost_catalog::production_pin_tests::pinned_snapshot_survives_schema_and_spec_revisions` | PASS |
| V8 Forge promotion/compaction under retained specs and sort orders | `forge/scribe_promotion.rs` validates retained spec/order/schema; iceberg fork `486273c` writes one manifest per spec; compaction fork `efb39ce` (`eedddb480`, `c1c7961c5`) | `--test forge scribe_promotion::mixed_specifications_preserve_partition_and_sort_truth` RED→GREEN: 15 compaction plans admitted in one attempt; promoted `{(spec0, order1): 24, (spec1, revised order): 24}`; 48 rows compacted; each live object's partition tuple, hour, identity and sort prefix checked from Parquet | PASS |
| V9 encoded sort keys and Bloom sets | existing `ParquetBatchEncoder` recipe; no production change (`25550077f`) | `--lib scribe::parquet_writer::tests::resolved_sort_and_blooms_preserve_exact_reads`: exact sorted rows, declared Bloom columns `[]`, `[service]`, `[service, environment]`, `[environment, service]`, every held value probes true | PASS |
| Architecture owner amended | `architecture/bifrost-design.md` table identity, Scribe, Oracle, Forge (`c6006466f`) | `docs:check` | PASS |

Final checks on `96fe37ee3`: `fmt`, `lints`, `check:deps`, `check:tenant-isolation`, `codegen:check` pass; `test:bifrost:integration:redux` 919/919; `wyrd-testing --test scribe` journeys 35/35; V8 1/1; `git diff --check` clean.

Forks (pinned in `Cargo.toml`): iceberg-rust `wyrd-production` `9a1b0ee` (`Schema::builder().with_schema_id`) and `486273c` (fast append under any retained spec, one manifest per spec, as Java `FastAppend`); iceberg-compaction `wyrd-production` `a0500b1`, `efb39ce`; DataFusion `1eab549` (field-id column resolution).

### Diagnoses

- **V8 RED 1.** Symptom: promotion of spec-1 objects failed `Data file partition spec id does not match table default partition spec id`. Evidence: iceberg `transaction/snapshot.rs::validate_added_files` compared against the default spec only. Cause: the fork's fast append accepted only the current default spec. Fix site: the fork's snapshot producer (`486273c`); its only callers are append/rewrite actions, covered by `test_fast_append_accepts_retained_partition_specs` and the 174 transaction tests.
- **V8 RED 2.** Symptom: no rewrite committed. Evidence: trace showed the mixed table's IcebergRewrite with no plans. Cause: each partition held one file per spec, so nothing qualified for compaction. Fix site: the test geometry (two batches per layout); no production change.
- **V9 first run.** Symptom: no Bloomed columns were observed. Cause: the test reader never loaded Bloom filters. Fix site: the test reader (`ReaderProperties::set_read_bloom_filter(true)`).
- **Datafusion revision pin.** Symptom: `plan_fingerprint_binds_the_pinned_datafusion_revision` failed after the fork bump. Cause: `ORACLE_DATAFUSION_REVISION` named the old revision. Fix site: `oracle/codec.rs` constant.
- **Lints.** Eight-argument `ScribeAssemblyKey::{new,from_parts}` after adding `revision`; fixed by passing the existing `StreamIdentity` instead of `(node_id, writer_epoch)`. Remaining findings were mechanical.
- **Redux integration `forge::promotion::scribe_promotion_integration_rejects_physical_datafile_mismatch_before_catalog_io`.** Symptom: worker attempt bound at `tests/integration/forge/support.rs:2489`, then an unrefused mutation. Evidence: `promotion evidence does not decode: unknown field start_utc, expected one of spec_id, time, time_index, values`; the next mutation promoted successfully. Cause: `f38c2efaf` moved the time and spec id into `partition: PhysicalPartitionWire`; the "partition value" mutation still wrote top-level `partition.start_utc`, and the "partition spec identity" mutation wrote a top-level `partition_spec_id`, which the record ignores. Fix site: the two mutations now edit `partition.time.start_utc` and `partition.spec_id`; production refuses both. An independent read-only diagnosis confirmed the cause and found no other caller with the stale shape (one doc comment in `wyrd-testing/tests/bifrost/scribe/support.rs` corrected).

### Material risks and limits

- A resend of an accepted batch id with a changed payload is refused and stores nothing; it fails closed by design.
- Revisions in tests are committed through the Iceberg catalog directly; TASK-005 owns public PATCH-to-engine proof.
- No file-size, pruning or planning-overhead thresholds were invented. V8 records the observed plan count and row geometry only.

Non-goals stayed excluded: no automatic history rewrite, release migration, new retention or writer policy, public edit API, or new storage/query ownership. No applied migration was rewritten.
