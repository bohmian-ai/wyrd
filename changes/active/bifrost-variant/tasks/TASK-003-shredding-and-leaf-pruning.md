---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-bifrost-variant
spec_revision: 10
requirements: [REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027, INV-003, INV-004, INV-006, INV-007, INV-008, AC-006, AC-007, AC-008, AC-009, AC-010]
depends_on: [TASK-001]
parent_task:
remediates: [BVR-FRESH-001, BVR-FRESH-002, BVR-FRESH-003, BVR-RR2-001, BVR-RR2-002, BVR-RR2-003, BVR-RR3-001, BVR-RR3-002, BVR-RR3-003]
---

## Outcome and Value

Scribe recovery-stage Parquet runs remain unshredded; every final Scribe hot
object and Forge output independently infers and writes a standard Parquet
Variant shredded layout. Struct stays exact `get_field` and
Variant stays semantic Arrow-backed `variant_get`; both declare nested fields
to DataFusion's shared projection, decoder-filter, and conservative statistics
pruning machinery. Partial, incompatible, and unshredded files use residual
Variant evaluation. Distributed followers carry the same logical leaf
predicate in the existing unsigned, mTLS-authenticated, digest-protected
assignment.

TASK-002 may run in parallel after TASK-001. Final integrated verification waits
for both and runs `mise run verify:bifrost` once, as TASK-003's last command.

## Owners, Scope, Consumers, and Prohibited Changes

- `encode_batch` keeps Scribe recovery-stage runs on the stable unshredded
  logical Variant schema. The pinned iceberg-rust fork owns one pure analyzer
  and standard Arrow schema-application wrapper beside `ParquetWriterBuilder`.
  Scribe `encode_ordered_claim` calls them only for final hot objects.
- The same fork owns one concrete deferred `VariantParquetWriterBuilder`
  implementing `FileWriterBuilder`. Each `build(output)` creates a fresh writer
  with no open Parquet encoder and independent sample state; clones carry only
  the internal policy constants. Compaction-core installs it in the existing
  `RollingFileWriterBuilder`, preserving the five-field Forge handoff.
- Each final Scribe/Forge writer retains at most 4,096 rows or 67,108,864 bytes
  of Arrow backing memory, avoids unnecessary Variant copies, and releases or
  transfers every charge once. Scribe uses writer admission; Forge's existing
  task reservation includes one 64-MiB prefix per concurrently open output.
  Wyrd passes the row, byte, 10% frequency, 1,000-candidate, 300-child, and
  depth-50 values as internal constructor policy; none is public configuration.
- Arrow 59.3 `ShreddedSchemaBuilder`, `shred_variant`, `VariantArray::try_new`,
  `unshred_variant`, and `variant_get` own standard Variant encoding and
  reconstruction. Keep the analyzer and write-side experimental calls behind
  the one fork-local wrapper so an upstream rename stays local; query kernels
  stay in `vala-bifrost-redux`. Wyrd adds no parallel codec.
- The pinned Iceberg fork lets `ParquetWriterBuilder` accept a validated
  per-file Arrow physical schema while retaining logical Iceberg field identity
  and `DataFile` metadata. Its reader validates standard shredded layouts,
  unshreds whole-Variant projections before cross-file union, and evaluates
  pushed literal paths before union. It never unions incompatible table-wide
  `typed_value` schemas.
- TASK-003 alone owns the writable
  `https://github.com/bohmian-ai/datafusion` fork, based on immutable v55 commit
  `d5552342012888b7d1a3ab88d92e3d292fc0cde0`. The reviewed source remote is
  `https://github.com/peterxcli/datafusion`, containing `#25013` head
  `cfc4298af54ee1301c38b675372288d1b385c6e9` and nested-statistics patch
  `0b0506a9acab9d5892ecf7e89243c3b34664bcc6`, rebased together. Record the
  exact tested Wyrd-fork revision before the workspace repin.
- The fork exposes `PerFileParquetReadPlanner::plan(input) ->
  Result<PerFileParquetReadPlan>` from `datafusion-datasource-parquet`. Its input
  carries logical projection expressions, an optional logical filter, file
  Arrow schema, Parquet schema/metadata, and `PhysicalExprAdapterFactory`. Its
  owned output carries the `ProjectionMask`, projected Arrow schema, optional
  decoder `RowFilter`, optional row-group `PruningPredicate`, optional page
  predicate, and conservative-fallback reason. The facade opens no reader,
  performs no IO, and owns no Wyrd metrics. `HotParquetExec` and the pinned
  Iceberg `ArrowReader` apply that exact plan after footer discovery.
  Struct uses `struct_field_access()`; Variant retains its UDF and declares
  `required_input_fields`. Missing or invalid requirements mean full-root
  decode and no pruning.
- One immutable `[patch.crates-io]` override pins `datafusion`,
  `datafusion-common`, `datafusion-datasource-parquet`, `datafusion-expr`,
  `datafusion-functions`, `datafusion-physical-expr`,
  `datafusion-physical-expr-adapter`, and `datafusion-pruning` to the same
  tested fork revision. Wyrd, iceberg-rust, iceberg-datafusion, compaction, and
  datafusion-distributed resolve one source/type universe.
- `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto` owns the in-place
  `ScanLeafRef`/`ScanPredicateOp::IN` wire. `private_conversion.rs` is its sole
  domain boundary. Peer mTLS authenticates the unsigned v8-digested assignment,
  and authorization remains rooted at the logical column before provider
  construction or file I/O.
- Hot/published, Interactive/Analytical, leader/follower, distributed codec,
  generated contract, MCP/HTTP, documentation, and architecture consumers are
  in scope.
- Do not add a top-128 policy, byte weights, Wyrd Variant footer keys, summary
  merging, sidecar, spill, table-wide shredded schema, configuration, strategy
  abstraction, array shredding, Bloom filters on Variant leaves, signature,
  second reader, `ParquetSource`, or Wyrd-specific physical Variant optimizer.
  Do not expose the row or memory sample bounds as public/table/wire/persisted
  configuration.

## Approach

1. Keep `encode_batch` recovery runs unshredded. Put the revision-10 pure
   analyzer and Arrow schema wrapper in the Iceberg fork; Scribe calls them
   only from `encode_ordered_claim`.
2. Add the fork's deferred `VariantParquetWriterBuilder` to the existing Forge
   rolling writer so each rollover starts fresh, with prefix memory charged to
   the existing Scribe admission or Forge task reservation.
3. Bootstrap the Wyrd DataFusion fork reproducibly, backport the two reviewed
   capabilities and owned-plan facade, test and push it, then apply the eight
   workspace source overrides at the one recorded revision.
   Route both readers through it while keeping Struct `get_field` and Variant
   `variant_get` distinct.
4. Replace the existing protobuf predicate fields in place with the specified
   leaf oneof and `IN`; update conversion, validation, digest, and peer tests
   while retaining mTLS and logical-column authorization.
5. Fix full-path hot matching and Bloom `IN`, update architecture/docs, and add
   the resource-adaptive optimized-only nested-field benchmark.

## Ordered Implementation Scenarios

### Scenario 1 — Only final output files infer a bounded standard layout

**Behavior.** `encode_batch` always writes recovery-stage runs with the stable
unshredded logical schema. The Iceberg fork's one analyzer and schema wrapper
serve `encode_ordered_claim` and its deferred Forge writer. Each final hot
object or Forge output retains a prefix until 4,096 rows or 67,108,864 bytes
(64 MiB) of accounted Arrow backing memory arrives first. It stops before
a later row or batch slice would cross the byte limit; an oversized first row
is retained and fully charged as the progress exception. Reaching either limit
infers, opens, replays once, and streams. Short close infers from all retained
rows, empty output creates no file, and rollover starts fresh. Charges release
exactly once on success, error, cancellation, and retry. Fields follow the 10%
frequency, compatible-family, 300-emitted, 1,000-tracked, depth-50,
alphabetical tie-break/order policy; arrays remain residual. This proves
REQ-020, REQ-021, INV-007, and AC-006.

Normative write sequence:

```text
before retain: charge the candidate prefix memory
if a later row/slice would cross 64 MiB: stop before it
if the first row alone crosses 64 MiB: retain and fully charge that one row
at either bound or close: infer -> open ordinary writer -> replay once
after replay: release/transfer the prefix charge once -> stream
on Forge roll: build(output) creates a fresh deferred writer and sample
on error/cancel/retry: close or preserve existing output evidence and release once
```

Scribe uses writer admission. Forge's task reservation includes one prefix for
every concurrently open output; existing concurrent close completion remains.

**RED.** Add
`scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_final_objects_infer`
and
`scribe::parquet_writer::tests::variant_inference_memory_releases_on_every_terminal`,
plus
`writer::file_writer::parquet_writer::tests::variant_builder_infers_each_rolled_output`
in iceberg-rust.
Cover exact-schema staged merge for values that would infer different layouts,
then cover row-first, byte-first, oversized-first-row, short close, empty
output, rollover, 10% boundary, candidate/emitted limits, widening,
incompatible values, arrays, depth, order, existing-accounting charges, and
release on success/error/cancellation/retry at final-object boundaries. Run:
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_final_objects_infer) | test(=scribe::parquet_writer::tests::variant_inference_memory_releases_on_every_terminal)'`
and
`mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-rust/Cargo.toml -p iceberg --lib -E 'test(=writer::file_writer::parquet_writer::tests::variant_builder_infers_each_rolled_output)'`.

**GREEN.** Leave `encode_batch` unchanged. Put the pure analyzer and schema
wrapper beside the Iceberg Parquet builder. Call them from
`encode_ordered_claim`; make each deferred Forge `build(output)` use them before
opening its ordinary encoder. Rerun both focused commands.

**REFACTOR.** Share only the pure inference policy needed by both owners. Add no
strategy trait, footer codec, or operational state.

### Scenario 2 — Recovery and compaction preserve standard logical values

**Behavior.** After staged durability retires the WAL, a stop before
publication restores and merges the unshredded runs, produces one shredded hot
object, and publishes exactly once without duplicates. Standard unshredded,
fully shredded, partially shredded,
differently shredded, missing/null, and later-incompatible files reconstruct
identical values on hot and published reads and after two Forge rewrites.
Whole-Variant projections unshred before union; pushed literal paths evaluate
before union; v3 row lineage remains stable. This proves REQ-020–REQ-022,
INV-003, INV-006, and AC-006.

**RED.** Add
`recovery::variant_staging_restores_and_publishes_once` to the Scribe journey
and
`forge::managed_rewrite::standard_variant_layouts_round_trip_per_file`, plus
`executor::datafusion::tests::rolled_outputs_infer_independent_variant_layouts`
in compaction-core.
The recovery journey stages Variant rows whose values would infer different
layouts, confirms the WAL is retired after staged durability, stops before
publication, restores, and asserts one final object and one copy of each row.
The Forge journey forces multiple actual outputs with different inferred
schemas, compares logical rows on hot, published, and twice-compacted reads,
permits existing concurrent close completion, asserts unchanged `DataFile` and
five-field handoff evidence, and asserts no table-wide `typed_value` union. Run:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=recovery::variant_staging_restores_and_publishes_once)"'`
and
`scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::standard_variant_layouts_round_trip_per_file)'`
and
`mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-compaction/Cargo.toml -p iceberg-compaction-core --lib -E 'test(=executor::datafusion::tests::rolled_outputs_infer_independent_variant_layouts)'`.

**GREEN.** Preserve the existing staged-run schema/recovery path. Install the
fork's deferred builder in compaction-core's existing rolling writer, including
one prefix per concurrent writer in its task reservation. Prove two rolls can
select different layouts while retaining exact values, `DataFile` evidence,
and the five-field handoff; record both fork revisions, then repin Wyrd.

**REFACTOR.** Keep logical and physical schemas distinct; delete the old
shredded-layout rejection and add no compatibility wrapper.

### Scenario 3 — Distinct logical semantics share physical pushdown

**Behavior.** Struct access remains `get_field`; literal Variant access remains
`variant_get`. The Variant UDF declares its typed, residual, and metadata input
fields. The shared per-file facade chooses compatible leaves or full-root
residual fallback and returns an owned plan containing the projection mask,
projected schema, optional decoder row filter, optional row-group predicate,
optional page predicate, and fallback reason. Missing/invalid requirements
return a successful full-root/no-prune plan; genuine schema/metadata failures
return typed DataFusion errors. Both readers apply the plan and retain their
existing metrics. Typed
Variant statistics prune only with all-null matching residual statistics and a
matching predicate type. Dynamic paths and invalid requirements read the full
root without pruning. Full Parquet paths prevent top-level/leaf name collisions,
and hot Bloom `IN` skips impossible groups. This proves REQ-023, REQ-024,
REQ-026, REQ-027, INV-008, AC-007, and AC-009.

**RED.** Add
`physical_plan::tests::per_file_plan_covers_projection_filter_and_pruning` and
`physical_plan::tests::invalid_requirements_fall_back_without_pruning` in the
DataFusion fork, then
`oracle::nested_pushdown::tests::both_readers_use_shared_per_file_plan` and
`published::struct_and_variant_share_physical_pushdown` to the Oracle journey.
Assert distinct logical expressions, equivalent physical field requirements,
all six plan fields, typed errors versus conservative success, selected-leaf bytes, decoder-filter use,
row-group/page metrics, correct partial/incompatible/unshredded fallback,
dynamic-path full-root reads, full-path matching, and Bloom `IN` on both
readers. From a clean sibling checkout, bootstrap the fork exactly once before
editing or testing:

```bash
git clone https://github.com/bohmian-ai/datafusion.git /home/thorrester/Documents/GitHub/datafusion
git -C /home/thorrester/Documents/GitHub/datafusion remote add upstream https://github.com/apache/datafusion.git
git -C /home/thorrester/Documents/GitHub/datafusion remote add patch-source https://github.com/peterxcli/datafusion.git
git -C /home/thorrester/Documents/GitHub/datafusion fetch upstream refs/tags/55.0.0
git -C /home/thorrester/Documents/GitHub/datafusion fetch patch-source refs/heads/feat/struct-field-access-capability:refs/remotes/patch-source/struct-field-access refs/heads/feat/struct-field-row-group-pruning:refs/remotes/patch-source/struct-field-row-group-pruning
test "$(git -C /home/thorrester/Documents/GitHub/datafusion rev-parse refs/remotes/patch-source/struct-field-access)" = cfc4298af54ee1301c38b675372288d1b385c6e9
test "$(git -C /home/thorrester/Documents/GitHub/datafusion rev-parse refs/remotes/patch-source/struct-field-row-group-pruning)" = 0b0506a9acab9d5892ecf7e89243c3b34664bcc6
git -C /home/thorrester/Documents/GitHub/datafusion switch -c wyrd-v55-variant-read-plan d5552342012888b7d1a3ab88d92e3d292fc0cde0
git -C /home/thorrester/Documents/GitHub/datafusion cherry-pick ee8b516e827fb69366a8df44a972b8f09c28b436 1180ae33d9903de607cd8dd5cd04c3efbbfa10b4 816480147a0bda13e43dc13e2d52caf4def65530 431f601b816f7f21947138c6bb68b42e32f6a83e 42d45892cdf29ef510fd5b368cbf01661fa1cb7b cfc4298af54ee1301c38b675372288d1b385c6e9 0b0506a9acab9d5892ecf7e89243c3b34664bcc6
```

Implement the owned-plan facade, then run before the workspace repin:
`mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/datafusion/Cargo.toml -p datafusion-datasource-parquet --lib -E 'test(=physical_plan::tests::per_file_plan_covers_projection_filter_and_pruning) | test(=physical_plan::tests::invalid_requirements_fall_back_without_pruning)'`.
Commit and push the tested head with
`git -C /home/thorrester/Documents/GitHub/datafusion push origin HEAD:refs/heads/wyrd-v55-variant-read-plan`,
record its immutable revision, and use that revision for all eight overrides.
Then run:
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=oracle::nested_pushdown::tests::both_readers_use_shared_per_file_plan)'`
and
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=published::struct_and_variant_share_physical_pushdown)"'`.

**GREEN.** Complete the deterministic bootstrap, add the exact owned-plan
interface, push and record the tested revision, apply the eight source
overrides, pass the focused source-universe check, then route both readers
through the facade after footer discovery.

**REFACTOR.** Delete duplicate projection/filter/pruning logic; retain the
semantic UDF and add no Variant-specific DataFusion node.

### Scenario 4 — Unsigned distributed predicates preserve authority

**Behavior.** The actual protobuf replaces `ScanPredicate.column` and scalar
`literal` with the revision-10 `ScanLeafRef` oneof and repeated literals, and
adds `SCAN_PREDICATE_OP_IN = 9`. Scalar comparisons accept exactly one literal,
`IN` a non-empty same-typed list, and null tests none. Conversion rejects
unknown/unspecified tags, missing leaves, empty columns, empty paths or path
segments, wrong cardinality, and mixed types; resolved-schema validation rejects a leaf kind
incompatible with its logical root. Valid predicates round-trip through the v8
digest and execute on a peer. Tampering, peer-version mismatch, and
unauthenticated origin fail before plan decode, provider construction, or I/O.
This proves REQ-025, INV-004, INV-006, AC-007, and AC-008.

**RED.** Add
`private_conversion::tests::leaf_predicates_round_trip_and_reject_malformed`,
`distributed::unsigned_leaf_predicates_round_trip_and_execute`, and
`query::pg_tests::sensitive_variant_leaf_is_denied_before_io`. The conversion
test covers all three leaf variants, every operator, every malformed case
above, and logical-root mismatch. The peer journey serializes Struct and
Variant predicates including `IN`, digests, decodes, executes them on a worker,
and asserts exact rows; it also checks one-byte tamper, missing mTLS, version
mismatch, and zero provider/read counters. Run:
`mise exec -- cargo nextest run --locked -p wyrd-tonic --lib --features server,client -E 'test(=private_conversion::tests::leaf_predicates_round_trip_and_reject_malformed)'`,
`mise run check:proto-drift`,
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::unsigned_leaf_predicates_round_trip_and_execute)"'`
and
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::sensitive_variant_leaf_is_denied_before_io)"'`.

**GREEN.** Change the protobuf, regenerate it, update only
`private_conversion.rs` at the wire/domain boundary, then change assignment
validation, codec, and digest in place while keeping authorization at the
logical source column.

**REFACTOR.** Keep one leaf predicate representation from leader to follower;
delete flat compatibility code and never sign it.

## Acceptance Criteria

- Scribe recovery-stage runs remain unshredded and restartable after WAL
  retirement. Each final Scribe hot object and Forge output independently
  follows revision-10 bounded inference through the one fork-owned policy,
  accounts and releases retained Arrow memory, and writes only the standard
  Parquet Variant layout. Two Forge rolls may select different layouts without
  changing logical values, `DataFile` evidence, or the five-field handoff.
- Standard unshredded, partial, incompatible, and differently shredded files
  preserve values and row lineage across hot, published, and repeated Forge
  reads without table-wide typed-schema union.
- Struct `get_field` and Variant `variant_get` remain logically distinct while
  both readers apply the one DataFusion facade's owned physical projection,
  decoder-filter, and safe-pruning plan. Conservative fallback is successful
  and explained; genuine schema/metadata failures remain typed errors.
- Unsigned follower assignments reject tampering or unauthenticated/version-
  mismatched peers before I/O, round-trip the exact protobuf leaf/`IN`
  contract, and preserve logical-column sensitivity.
- Full-path matching, Bloom `IN`, architecture/docs, dependency pins, and the
  optimized-only benchmark are complete.

## Expected Write Set and Consumer Closure

- Workspace manifests/lock; the eight exact DataFusion source overrides; and
  the recorded `bohmian-ai/datafusion` tested revision.
- `crates/vala/vala-bifrost-redux/src/{scribe,parquet,forge,oracle,schema}/`.
- `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto`,
  `crates/wyrd/wyrd-tonic/src/private_conversion.rs`, generated protobuf code,
  `crates/wyrd-spec/src/vala/{api,assignment_authority}.rs`, and distributed
  codec/dispatcher/peer/follower consumers.
- The pinned iceberg-rust analyzer, schema wrapper, and deferred
  `VariantParquetWriterBuilder`; compaction-core rolling-writer integration;
  and the bootstrapped DataFusion fork, followed by exact workspace repins.
- Existing Redux integration, Forge/Oracle journeys, MCP journey, and
  Rust/Python/TypeScript query consumers.
- One Criterion-style benchmark in `vala-bifrost-redux` and one opt-in
  `mise run bench:bifrost:nested-field-pushdown` task, independent of a server,
  Postgres, `wyrd-testing`, and the capacity harness.
- `architecture/bifrost-design.md` and relevant recovery/query documentation.

## Verification and Evidence

During iteration, run only these focused proofs:

1. `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_final_objects_infer) | test(=scribe::parquet_writer::tests::variant_inference_memory_releases_on_every_terminal)'`
2. `mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-rust/Cargo.toml -p iceberg --lib -E 'test(=writer::file_writer::parquet_writer::tests::variant_builder_infers_each_rolled_output)'`
3. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=recovery::variant_staging_restores_and_publishes_once)"'`
4. `mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-compaction/Cargo.toml -p iceberg-compaction-core --lib -E 'test(=executor::datafusion::tests::rolled_outputs_infer_independent_variant_layouts)'`
5. `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::standard_variant_layouts_round_trip_per_file)'`
6. Run Scenario 3's exact clean-checkout bootstrap, then `mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/datafusion/Cargo.toml -p datafusion-datasource-parquet --lib -E 'test(=physical_plan::tests::per_file_plan_covers_projection_filter_and_pruning) | test(=physical_plan::tests::invalid_requirements_fall_back_without_pruning)'`; push and record the tested revision before repinning Wyrd.
7. `mise exec -- cargo metadata --locked --format-version 1 | jq -e '[.packages[] | select(.name == "datafusion" or .name == "datafusion-common" or .name == "datafusion-datasource-parquet" or .name == "datafusion-expr" or .name == "datafusion-functions" or .name == "datafusion-physical-expr" or .name == "datafusion-physical-expr-adapter" or .name == "datafusion-pruning") | .source] as $sources | ($sources | length) == 8 and ($sources | all(startswith("git+https://github.com/bohmian-ai/datafusion?rev="))) and ($sources | unique | length) == 1'`
8. `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=oracle::nested_pushdown::tests::both_readers_use_shared_per_file_plan)'`
9. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=published::struct_and_variant_share_physical_pushdown) | test(=distributed::unsigned_leaf_predicates_round_trip_and_execute)"'`
10. `mise exec -- cargo nextest run --locked -p wyrd-tonic --lib --features server,client -E 'test(=private_conversion::tests::leaf_predicates_round_trip_and_reject_malformed)'`
11. `mise run check:proto-drift`
12. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::sensitive_variant_leaf_is_denied_before_io)"'`
13. `mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-rust/Cargo.toml -p iceberg --lib -E 'test(=arrow::reader::projection::tests::variant_shredding_round_trips_nested_residuals) | test(=writer::file_writer::parquet_writer::tests::accepts_per_file_variant_physical_schema)'`; record the tested Iceberg and compaction revisions before repinning.

The benchmark is performance evidence, not a timing gate. Use exactly 262,144
rows and varying 8,192-byte string siblings. Build narrow (one string plus
`Int32`), wide (four strings plus `Int32`), and nested (inner string/`Int32`
plus outer string) fixtures as fixed Struct and equivalent standard shredded
Variant. Report wide `{select_small_field,sum_small_field,
select_one_string_field}`, nested `{select_extra_string,
select_inner_small_field,sum_inner_small_field}`, and narrow
`{select_small_field,select_id_and_small_field}`. Before timing, assert exact
results, Struct `get_field` or Variant `variant_get`, equivalent physical field
requirements, and zero reads of unselected siblings. Keep fixture/session setup
outside timing. Print only optimized medians; no full-object baseline, speedup,
host qualification, resource reservation, or absolute threshold. Run
`mise run bench:bifrost:nested-field-pushdown`; never substitute the 10-million-
row capacity benchmark.

## Final integrated verification

After TASK-001, TASK-002, and TASK-003 are integrated, run this sequence once.
Do not run `check:bifrost` or `test:bifrost` separately because the final gate
composes them.

1. `mise run fmt`
2. `mise run lints`
3. `mise run py:format`
4. `mise run py:lints`
5. `mise run py:typecheck`
6. `mise run ts:typecheck`
7. `mise run codegen:check`
8. `mise run test:principals:integration`
9. `mise run check:client-tier`
10. `mise run check:pyo3-scope`
11. `mise run check:object-store-pin`
12. `mise run check:unwrap-audit`
13. `mise run docs:check`
14. `mise run check:docs`
15. `mise run check:examples`
16. `git diff --check`
17. `mise run bench:bifrost:nested-field-pushdown`
18. `mise run verify:bifrost` — run once and as the final command.

## Material Stop Conditions

- Existing writer admission cannot account retained Arrow backing memory or
  cannot represent the oversized-first-row progress exception without an
  untracked buffer.
- Keeping recovery-stage runs unshredded cannot preserve exact-schema merge and
  restore after WAL retirement without a new staging format or schema union.
- The existing `FileWriterBuilder`/rolling-writer composition cannot host the
  deferred Variant builder without changing Forge's five-field handoff or
  commit authority.
- The reviewed DataFusion patches cannot be rebased together without changing
  their semantic contracts or adding Variant-specific DataFusion behavior.
- Both Oracle readers cannot consume the one narrow DataFusion facade, or one
  immutable source override cannot keep every consumer in one type universe.
- The pinned Iceberg fork cannot accept a standard per-file physical Variant
  schema while retaining logical field identity and `DataFile` metadata.
- A typed leaf cannot prove its corresponding residual value all-null and its
  comparison type equal; that unit must fall back rather than prune.
- Authorization cannot precede physical binding/I/O, or the follower change
  requires signatures or a channel outside authenticated peer context plus the
  canonical digest.
- The in-place protobuf cannot express and validate the locked leaf/`IN`
  contract without a second wire model or compatibility shim.

## Authority Links

- `changes/active/bifrost-variant/spec.md` revision 10
- `changes/active/bifrost-variant/architecture_ref.md`
- `AGENTS.md`
- `architecture/{agent-rules,wyrd-design,bifrost-design}.md`
- Apache Parquet `VariantShredding.md`
- Apache Iceberg writer configuration and PRs `#14297`, `#16818`
- `https://github.com/bohmian-ai/datafusion`
- Apache DataFusion PR `#25013`, `peterxcli/datafusion#2`, and issue `#20871`
