---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-bifrost-variant
spec_revision: 18
requirements: [REQ-012, REQ-014, REQ-016, REQ-028, REQ-029, AC-011, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027, INV-003, INV-004, INV-006, INV-007, INV-008, AC-006, AC-007, AC-008, AC-009, AC-010]
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

## Added Scope and Progress (record for resuming shredding)

This branch carries two pieces of work beyond the shredding outcome above.
Both were done before shredding resumed. Spec revision 18 specifies them
(REQ-028, REQ-029, REQ-014's server cast, REQ-016, REQ-012's Map, AC-011;
merge plan decision D4,
`changes/active/bifrost-variant/merge-integration-into-task-003.md`), and this
task file records them.

### Added A: three timestamp types — implemented, uncommitted

Each user column picks one of three Snowflake-named types:

| Type | Meaning | Iceberg storage |
|---|---|---|
| `TIMESTAMP_NTZ` | Wall clock | `timestamp` |
| `TIMESTAMP_LTZ` | Instant | `timestamptz` |
| `TIMESTAMP_TZ` | Instant plus the writer's wall clock | `struct<utc: timestamptz, local: timestamp>` |

- **Declaration by format.** A model declares the type through its JSON
  Schema `format`:
  - `timestamp-ntz` and `partial-date-time` → NTZ;
  - `date-time` → LTZ;
  - `timestamp-tz` → TZ.
- **Declaration from Arrow.** Any zoned timestamp, of any zone label, declares
  LTZ and is stored and read as `+00:00`. A naive timestamp declares NTZ.
- **System-owned timestamps** are always LTZ.
- **Conversion.** Users supply datetimes and Bifrost converts them to the
  column's type.
  - A naive value is refused for a zoned column.
  - An offset is refused for an NTZ column.
  - An Arrow LTZ batch in any zone is one instant; a naive batch for an LTZ
    column is refused (`FINGERPRINT_MISMATCH`).
- **Ownership.** Every type is a Wyrd type, independent of other libraries.
  - Pydantic, Zod, chrono and Arrow types convert to Wyrd types at runtime.
  - Rust owns parsing, rendering and the format table
    (`crates/shared/wyrd-types/src/timestamp.rs`, `TimestampKind`).
  - Python: `wyrd.types.TimestampNTZ/LTZ/TZ`, plus the Pydantic mapping in
    `bifrost/_pydantic.py`, backed by the native `timestamp_format`,
    `timestamp_kinds` and `QueryResult.timestamp_tz_texts`.
  - TS: the checked-text types `TimestampNTZ/LTZ/TZ` (`parse`, `schema(z)`),
    backed by the native `timestamp_format`, `timestamp_text`,
    `stored_timestamp_text` and `timestamp_kinds`.
  - Reads return each type's value. Python returns datetimes; TS returns
    Rust-rendered RFC 3339 text for top-level timestamp columns.
  - `z.date()` stays refused.
- **Tests.**
  - `wyrd-types`, `timestamp.rs` unit tests: `formats_name_their_timestamp_type`,
    `arrow_forms_are_recognized`, `each_type_parses_its_own_text`,
    `wyrd_values_round_trip_their_text`, `each_type_renders_and_checks_its_text`,
    `rust_types_declare_their_format`.
  - Rust journey `crates/shared/wyrd-client/tests/bifrost/three_timestamp_types.rs`:
    `wyrd_timestamps_read_back_as_wyrd_values`,
    `the_customers_local_hour_is_queryable`,
    `chrono_timestamps_go_through_wyrd_types`,
    `an_arrow_instant_in_any_zone_is_one_instant_and_a_naive_one_is_refused`.
  - Python journey
    `sdks/wyrd-sdk-python/tests/integration/bifrost/test_three_timestamp_types.py`:
    six tests, covering the four above plus the naive-into-LTZ and
    offset-into-NTZ refusals at insert.
  - Python unit tests `sdks/wyrd-sdk-python/tests/unit/bifrost/test_timestamp_types.py`.
  - TS journey `sdks/wyrd-sdk-ts/wyrd/tests/integration/three-timestamp-types.test.ts`:
    six tests, the same set.
  - TS unit tests `sdks/wyrd-sdk-ts/wyrd/tests/unit/timestamp-types.test.ts`:
    three tests.

### Added B: every Iceberg-supported column type usable from Rust, Python and TypeScript — implemented

- **Committed:** `d4a0cba97` adds Map.
- **Uncommitted:**
  - the shared crate `crates/shared/wyrd-types`, which owns the declared-type
    ↔ JSON Schema/Arrow mapping (moved from `wyrd-queue/src/schema.rs`);
  - TS `TableConfig.fromArrow`;
  - registration stores and fingerprints each field's Iceberg form, so the
    registered Iceberg schema is the table's single source of truth. Describing
    a narrower declaration returns the wide registered type and re-registering
    it is `AlreadyExists`;
  - Scribe casts a written column to its registered Arrow type when both are
    the same Iceberg type (narrower spellings, any-zone timestamps), before the
    fingerprint check.
- **Tests.**
  - `wyrd-types` `schema.rs` unit tests: `arrow_schema_maps_precision_types_verbatim`,
    `arrow_schema_refuses_unrepresentable_types`,
    `map_declarations_round_trip_through_arrow`, and the `fieldspec_to_arrow_*`
    round trips.
  - Rust journey `crates/shared/wyrd-client/tests/bifrost/every_iceberg_column_type.rs`:
    `every_iceberg_type_round_trips_with_nested_nulls`,
    `a_narrower_type_reads_back_the_same_values`,
    `a_narrower_declaration_is_the_table_it_describes`,
    `a_type_bifrost_cannot_store_is_refused_when_declared`.
  - Rust journey `register_a_table_from_a_model.rs`: six tests.
  - Python `test_every_iceberg_column_type.py` (four tests) and
    `test_register_a_table_from_a_model.py` (five tests).
  - TS `every-iceberg-column-type.test.ts` (three tests) and
    `register-a-table-from-a-model.test.ts` (five tests).

### Changes the merge applied to A and B

These are merge-plan decisions D1–D3, applied in the merge:

- **D1.** `check_supported` is restored in `wyrd-types` as the single
  unsupported-type decider, for the SDK declaration and server registration.
  It has no zone clause. The catalog `iceberg_form` failure is an internal
  Iceberg error. The registration-refusal journeys merged into one per
  surface, `a_type_bifrost_cannot_store_is_refused_when_declared`.
- **D2.** As implemented, any zone declares `TIMESTAMP_LTZ`, at registration as
  on writes (`tables::as_stored`). REQ-016 drops "a timestamp with a non-UTC
  zone".
- **D3.** TS typed reads keep Int64 as `bigint`.
  - The TS test "a 64-bit integer reads back as a number when it is safe" and
    `register-a-table-from-a-model.test.ts` change to `bigint`.
  - The three TS journeys above join `test:bifrost:journey:typescript`.
- **Cast pass.** Scribe's cast and the integration branch's by-name reorder
  became one pass (`conform_to_registered`), which also refuses a Variant
  identity mismatch.
- **Python dedup.** The Python `_FORMAT` table and `TimestampTZ.from_stored`
  arithmetic were removed; Python reads both from Rust as TS does.

### Shredding progress (this task's own scenarios)

Each status was found by searching the tree for that scenario's named tests.

| Scenario | Status | Evidence |
|---|---|---|
| 1. Only final output files infer a bounded standard layout | **Replaced by spec revision 21 (sampling)** | The first-rows prefix, its 4,096-row / 64 MiB bounds, its Scribe lease and Forge's `reserve_variant_prefixes` are deleted. Scribe samples each claim; Forge combines source footers. Evidence under "Sampling evidence (spec revision 21)" below. TASK-004 keeps only the parallelism change |
| 2. Recovery and compaction preserve standard logical values | **Implemented, not pushed** | iceberg-rust `fdc02e0a0847f3219e234f81ee6b7a2ff0d67418` (reader unshreds), iceberg-compaction `28f35fc9fd39b1e4879e7652f074f1630ca417cb` (test) and `a52462a2114e6912b0c7fdf903abcbb31b7d7136` (repin). Evidence below |
| 3. Distinct logical semantics share physical pushdown | **Implemented (spec revision 20)** | Commits `e76a74e09`, `740975234`, `f9fa00867`, `3aec57cec`, `f9b516a29`, `6fd3b6612`, plus the revision-20 read-core commit. Published files go through `FileReadPlan` via the iceberg fork's per-file hook (iceberg-rust `b0a88c310`). Journey `published::struct_and_variant_share_physical_pushdown` passes. Output leaf projection is still open (see the evidence below). |
| 4. Unsigned distributed predicates preserve authority | **Implemented** | Commit `47bdd88dc` (protobuf, v8 digest, `private_conversion::tests::leaf_predicates_round_trip_and_reject_malformed`). MCP `query::pg_tests::sensitive_variant_leaf_is_denied_before_io` and peer `peer_network::security::unsigned_leaf_predicates_round_trip_and_execute` implemented (evidence below). |
| Benchmark `bench:bifrost:nested-field-pushdown` | **Not started** | No mise task |

After the merge, resume shredding at Scenario 1.

#### Scenario 1 evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Recovery runs unshredded; final hot objects infer | `encode_batch` passes no policy; `encode_ordered_claim` passes `BIFROST_VARIANT_SHREDDING` (`scribe/parquet_writer.rs`) | focused command 1: 2 passed | PASS |
| Prefix charged while retained, released once on success/error/retry | Leases held beside `VariantPrefix` in `RollingArtifactWriter::pending`; dropped after replay or with the writer | focused command 1 | PASS |
| Forge outputs infer fresh per rolled file | `VariantParquetWriterBuilder` in iceberg-rust; compaction-core wraps its `ParquetWriterBuilder` | focused command 2: passed; iceberg writer+arrow 299 passed; compaction core 152 passed | PASS |
| Forge reservation covers every open output's prefix | `ForgeManagedExecutor::reserve_variant_prefixes` reserves `RUNNER_MAX_PARALLELISM` × 64 MiB before `rewrite` | `cargo check`; redux lib 857 passed (Postgres wrapper) | PASS |
| Bounds, 10%, caps, widening, incompatible, arrays, depth, order, short/empty close | fork `variant_shredding::tests` (11) | `cargo nextest -p iceberg --lib` | PASS |
| Lints and boundaries | — | `mise run fmt` 0; redux clippy `--all-features --all-targets -D warnings` 0; `check:workspace-hack` 0; `check:client-tier` 0; `git diff --check` 0 | PASS |

Known limit: a first batch that alone crosses 64 MiB is retained whole (and
charged whole), not cut to one row. Both memory measures count a slice's
shared buffers in full, so a per-row cut cannot be measured.

| New item | Owners searched | Why new |
|---|---|---|
| `VariantShreddingPolicy`, `VariantLayout`, `VariantPrefix`, `PrefixStep`, `ReadyPrefix` (iceberg-rust) | iceberg writer, Arrow `parquet::variant` | Arrow offers `shred_variant`/`ShreddedSchemaBuilder` but no inference; one shared analyzer for Scribe and Forge |
| `VariantParquetWriterBuilder`/`VariantParquetWriter` (iceberg-rust) | `ParquetWriterBuilder`, `RollingFileWriterBuilder` | the task names it; wraps the ordinary builder |
| `ParquetWriterBuilder::with_physical_schema` | iceberg writer | Iceberg had no way to write a physical schema differing from the logical one |
| `IcebergExecutionConfig::variant_shredding` (compaction) | compaction config | carries the policy into the writer |
| `BIFROST_VARIANT_SHREDDING` | `parquet/writer_properties.rs` | the one Bifrost policy value |
| `RUNNER_MAX_PARALLELISM` | `forge/managed/policy.rs` | was an inline literal; now shared with the reservation |
| `reserve_variant_prefixes`, `open_inferred`, `write_open`, `roll_if_due`, `variant_error` | `RollingArtifactWriter`, Forge executor | split of the existing write/roll path so replay rolls once |

#### Scenario 2 evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Staged runs restore after restart and publish once with logical values | Scribe recovery unchanged; journey `crates/wyrd/wyrd-testing/tests/bifrost/scribe/recovery.rs` | `recovery::variant_staging_restores_and_publishes_once`: 1 passed (exit 0) | PASS |
| Each rolled compaction output infers its own layout | iceberg-compaction `executor::datafusion::tests::rolled_outputs_infer_independent_variant_layouts` | passed; compaction core 153 passed | PASS |
| Iceberg reader returns logical Variant from shredded files | iceberg-rust `projection::unshred_variant_columns`, called in `pipeline.rs`; the old shredded-layout rejection is removed | 363 arrow/writer/scan tests passed; pinned nightly clippy 0 | PASS |
| Hot, published, rewrite and twice-compacted reads equal input; one layout per file; no union; output `DataFile` counts its object; handoffs name the base snapshot | journey `forge::managed_rewrite::standard_variant_layouts_round_trip_per_file`; fixture `PromotionIntegrationFixture::start_with` and `seal` (shared by `start` and `seal_more`), `PromotedRewriteFixture::object_batches` (shared by `object_row_values`) | focused journey: 1 passed (exit 0); whole redux integration binary: 72 passed (exit 0) | PASS |
| Lints | — | `mise run fmt` 0; redux integration clippy `-D warnings` 0; `git diff --check` 0 | PASS |

WAL diagnosis (recovery journey): the first run asserted the WAL segment was
gone immediately after drain and failed (`shard-00/0.wal` present). Cause:
only closed segments are deleted, by `retire_committed` on the shard age tick
(`ShardCommand::FlushExpired`); see `wal.rs`
`retirement_deletes_only_closed_segments`. The new test waits for retirement
with a 30 s deadline. No existing assertion changed.

| New item | Owners searched | Why new |
|---|---|---|
| `PromotionIntegrationFixture::start_with`, `seal`, `value_fields` | `start`, `seal_more`, `seal_builtin_table` | `start` and `seal_more` now delegate; one seal path |
| `PromotedRewriteFixture::object_batches` | `object_row_values` | extracted; `object_row_values` now uses it |
| `variant_batch`, `logical_rows`, `shredded_fields`, `document_keys`, `assert_per_file_layouts`, `published_rows`, `expected_rows` (journey) | `flat_builtin_batch` (fixed one-key shape per field, cannot vary layout by day), `LineageTable` (reused for the rewrite loop) | the scenario needs different document shapes per day |
| `unshred_variant_columns` (iceberg-rust) | Arrow `unshred_variant` (used), reader projection | applies Arrow's unshred to each Variant column of a batch |

#### Scenario 3 evidence (spec revision 20: one per-file read core)

Published files now go through the same per-file read core as hot files.
Iceberg still plans the scan and applies deletes, field IDs, defaults,
partition constants and lineage. For each data file it opens, the iceberg
fork calls a `ParquetFileReadPlanner` hook. Oracle implements that hook with
`PublishedFileReadPlanner`, which builds a `FileReadPlan` and hands back
row-group pruning, a page selection and a decoder `RowFilter`. The leader
installs the planner in `OracleIcebergScanExec::start_stream`; followers get
it through `route_published_reads`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-014.1: a key shredded in some files and residual in others, at any depth, reads like an all-residual read; a key no file shredded reads null and never errors | `PublishedFileReadPlanner` (`oracle/nested_pushdown.rs`); integer width fix in `VariantFileAdapter::typed_comparison` (`oracle/variant_sql.rs`) | `oracle::nested_pushdown::tests::shredded_and_residual_files_read_identically`, `typed_variant_statistics_prune_only_with_all_null_residuals` (Int64/Int16/Int8); `nested_pushdown` 6 passed | PASS |
| AC-014.2: Forge/Scribe compatibility test is committed | Scribe-shredded files rewritten by Forge read through the shared core | `forge::managed_rewrite::standard_variant_layouts_round_trip_per_file`; journey below (rewritten cut) | PASS |
| AC-014.3: `published::struct_and_variant_share_physical_pushdown` | `published.rs` journey: `PushdownCut`, `prove_pushdown_cut` | 1 passed. Files read / row groups pruned: hot 3/2, promoted 3/2, rewritten 1/5. Struct and Variant are identical in each state; the unshredded key returns 0 rows; contents are identical across all three | PASS |
| AC-014.4: Oracle position-delete and Forge position/equality-delete journeys stay green | unchanged Iceberg delete application around the core | redux lib 861 passed; `managed_rewrite::` and `publication::` journeys: `managed_rewrite::` 11 passed (including `managed_rewrite_applies_position_and_equality_deletes_to_output_rows`); `publication::` 4 passed | PASS |
| AC-014.5: tenant footer, byte range, LIMIT and follower assignment stay green | `PublishedFooterLoader` and `assigned ⊆ planned` unchanged | whole `wyrd-testing --test oracle` journey binary 53 tests: 50 passed in the first run; the other 3 passed on re-run (see the span diagnosis below), including `iceberg_filtering_mechanisms_cover_all_table_kinds`, `limit_stops_unneeded_live_fragment_without_footer`, `remote_staged_footer_refusal_fails_closed`, and the distributed and follower journeys | PASS |
| AC-014.6: Variant values and v3 lineage survive repeated Forge rewrites | `LineageTable` checks `_row_id` and sequence across two rewrites | `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite` and `standard_variant_layouts_round_trip_per_file` passed | PASS |
| Pins | iceberg-rust `b0a88c310afaee98d31fcd7a66e17036c8bf6a7c` (hook); iceberg-compaction `30e805a63a24d2b969cb9adb751ed584fab44d74` (repin) | `Cargo.lock` holds one `iceberg` | PASS |
| Lints | — | `cargo +nightly-2026-03-05 fmt` touched only changed files; clippy `--all-features --all-targets -D warnings` on `vala-bifrost-redux` 0, and on `wyrd-testing --test oracle` 0; `git diff --check` 0 | PASS |

Diagnosis: hot Variant filter did not prune.
- **Symptom:** in the journey's hot cut, the Struct filter pruned 2 row groups and the Variant filter pruned 0.
- **Evidence:** the iceberg fork's `variant_shredding.rs` narrows integers to the widest width the file holds, so Scribe wrote `typed_value: Int16`. `typed_comparison` required the leaf type to equal the cast type (`Int64`) exactly.
- **Cause:** the typed half was never formed, so no statistics predicate existed.
- **Fix site:** `VariantFileAdapter::typed_comparison`, the one place both readers rewrite Variant comparisons. An integer literal is now cast to the leaf's width when that cast is lossless. If it is not lossless, the comparison falls back to residual-only (unit case `1000` against Int8).

Diagnosis: the rewrite never ran in the journey.
- **Symptom:** `await_rewrite` timed out with no `/data/forge/` file.
- **Evidence:** the Forge leader creates a compaction track only from a commit notice that carries table settings (`forge/leader.rs` `notify_commit`).
- **Cause:** compaction was disabled at promotion. Setting the property later through the catalog sends no notice.
- **Fix site:** test only. After enabling compaction, the journey writes and promotes one more file, so a commit carries the setting. Forge is unchanged.

Diagnosis: span assertions failed in the regression run.
- **Symptom:** six tests failed with "spans … never finished" or "no Analytical attempt span": three `forge::publication` tests, `analytical_lifecycle::pg_analytical_production_telemetry_covers_every_hot_path`, `published::published_cache_pruning_and_shutdown_are_production_governed`, and `peer_network::analytical::remote_live_scribe_drop_releases_query`.
- **Evidence:** test span capture (`wyrd-telemetry` `init_test_capture`) honours the ambient `WYRD_LOG`. That run used `WYRD_LOG=warn`, which drops info spans.
- **Cause:** the run's environment, not the code.
- **Fix site:** none. Re-run without `WYRD_LOG` on the pushed pins: all six passed, and so did `published::struct_and_variant_share_physical_pushdown`.

Known gap: neither reader yet projects only the shredded leaves an output
needs. Both pass every column's whole projection to `FileReadPlan`
(`exec.rs` hot stream; `PublishedFileReadPlanner`), so leaf masks are used
for filters only. REQ-023's output leaf projection is still open.

| New item | Owners searched | Why new |
|---|---|---|
| `ParquetFileReadPlanner`, `ParquetFileReadNarrowing`, `ArrowReaderBuilder::with_parquet_file_read_planner` (iceberg-rust) | `ArrowReader` pruning, `ParquetMetadataLoader` hook | Iceberg had no per-file hook for an embedder to narrow the decode; it follows the existing loader hook |
| `PublishedFileReadPlanner` | `FileReadPlan`, hot `HotParquetExec` | adapts the one `FileReadPlan` to the Iceberg hook; it adds no second planner |
| `OracleIcebergScanExec::with_read_plan`, `file_read_planner`; `route_published_footers` renamed to `route_published_reads` | `with_footers`, follower routing | carries the closure predicates into the published scan on the leader and followers |
| `write_groups_with_a`, `typed_statistics_prune_at` (tests) | `write_groups` | `write_groups` now delegates; this parametrizes the shredded integer width |
| `PushdownCut`, `prove_pushdown_cut` (journey) | `FilterCase`, `run_case`, `expect_measure` (reused) | runs the same three cases at each of the three file states |
| `await_rewrite`, `planned_paths`, `set_table_property`, `FORGE_DATA_SEGMENT` moved to `oracle/support.rs` | `distributed.rs` copies | moved, not copied; `distributed.rs` uses them |

Commands:
- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support'`
- `mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::struct_and_variant_share_physical_pushdown)'` (under the Postgres wrapper)
- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(/managed_rewrite::/) | test(/publication::/)'` (under the Postgres wrapper)
- the whole `wyrd-testing --test oracle -P journey --run-ignored=all` binary (under the Postgres wrapper)

#### Scenario 4 evidence (MCP)

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| A sensitive Variant leaf (`->>`, `->` chain, predicate only, `to_json`) is refused without payload authority, with no rows and no scan | `authorize_payload_columns` (unchanged); journey `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs` | `query::pg_tests::sensitive_variant_leaf_is_denied_before_io`: 1 passed (exit 0). Permitted reads scan files and bytes > 0; each refusal is `WYRD_VALA_403_QUERY_FORBIDDEN` with 0 files and 0 bytes scanned | PASS |
| Shared gateway capture fixture does not change the journeys it came from | `wyrd_testing::gateway_capture::GatewayCapture` | `verification_runtime::typed_builtin_payloads_are_queryable`, `builtin_variant_columns_are_refused_before_ack`: 2 passed; all MCP journeys 16 passed | PASS |
| Lints | — | `cargo fmt`; clippy `-D warnings` on `wyrd-mcp --test mcp` and `wyrd-testing --lib --test server`: 0 | PASS |

Diagnosis (first run): the permitted read returned rows but scanned 0 files.
The trace shows `listed active Scribe tail partitions ... active_partition_count=1`:
gateway capture reaches Scribe asynchronously, so `flush_bifrost` ran before
the rows arrived and the read came from the live tail. The journey now waits,
with a 30 s deadline, for `wyrd_gateway_capture_total{outcome="delivered"}`
to reach 2 before sealing.

| New item | Owners searched | Why new |
|---|---|---|
| `wyrd_testing::gateway_capture::{GatewayCapture, CapturedCall}` | `verification_runtime.rs` `TypedPayloadJourney` (moved from there), `WyrdTestServer` gateway builders | `vala.gateway.calls` accepts only the capture principal; the mock-upstream setup is now shared instead of copied |
| `oracle_scans` (MCP journey) | `oracle/support.rs` `sum_metric` (test-local to another binary) | sums two scan families in the MCP binary |

#### Scenario 4 evidence (peer)

The task named the journey `distributed::…`; it lives in
`peer_network::security` beside the Scribe-fragment builder, mTLS dial
identities and reservation plane it reuses, so the path above is corrected.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Struct `IN` and Variant `=` leaf predicates cross the peer plane (domain → protobuf → v8 digest → Scribe decode) and execute with exact rows | `ScribeFragment::over` (`peer_network/security.rs`); Scribe `FilterExec` from signed predicates (`oracle/follower.rs`) | `peer_network::security::unsigned_leaf_predicates_round_trip_and_execute`: 1 passed (exit 0). Five live rows; `attrs.kind IN ('a','c') AND doc.n = 2` returns ids `[1, 3]`; fragment counter +1 | PASS |
| One-byte literal tamper is refused before execution | same journey | `PermissionDenied`; trace `Scribe peer assignment-authority digest mismatch`; fragment counter unchanged | PASS |
| Peer protocol version mismatch is refused before execution | same journey | `PermissionDenied`; trace `Scribe peer physical claims validation failed`; fragment counter unchanged | PASS |
| An origin without a client certificate never reaches the body | same journey | no row stream; `peer_body_polls` and fragment counter unchanged | PASS |
| Existing fragment journey unchanged by the builder refactor | `ScribeFragment::live` delegates to `over` | `peer_network::security::peer_context_refusals`: 1 passed (exit 0) | PASS |
| Protobuf drift | — | `mise run check:proto-drift`: exit 0 | PASS |
| Lints | — | `cargo fmt --check`; clippy `--all-features` on `wyrd-testing --test oracle`: 0 warnings; `git diff --check`: clean | PASS |

The Scribe fragment counter advances only after
`fragment_follower().execute(...)` returns (`wyrd-server/src/oracle/peer_service.rs`),
and the claims and digest checks return before that call, so an unchanged
counter means no plan decode, provider or read ran.

Diagnosis (first run): the journey failed with `Malformed` from
`QueryIpcDecoder`. That decoder reads the public query stream; each worker
batch frame is its own Arrow IPC stream (`oracle/live.rs` reads it with
`StreamReader`). The journey now uses Arrow's `StreamReader`.

| New item | Owners searched | Why new |
|---|---|---|
| `PeerCluster::register_table_with`, `PeerCluster::ingest_live_batch` | `register_table`, `ingest_live_rows` (now delegate) | the fixture's schema was fixed; one registration and one IPC encode now serve both |
| `ScribeFragment::over`, `ScribeFragment::request_with` | `ScribeFragment::live`, `request` (now delegate) | columns, predicates and claims were fixed |

#### Sampling evidence (spec revision 21)

Scribe picks each claim's layout from a seeded sample stratified by
`principal_id` (`scribe/claim_sample.rs` `ClaimVariantSample`, over the
fork's `VariantSampler`). Forge combines its sources' footer counts
(compaction `combined_variant_layout` → fork `VariantLayout::combine`). Both
writers open with the layout known; nothing is buffered.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-015.1 Reproducible: same sample, same layout, byte-identical objects | seed = first 8 bytes of `StagingClaimId`; position = row index across runs in claim order | `scribe::parquet_writer::tests::rerun_claim_writes_byte_identical_objects`; fork `sampling_is_seeded_and_reproducible` | PASS |
| AC-015.2 Small writers count (own stratum ≥ 30 rows; merged shared stratum) | `VariantSampler::new` merges strata under `min_stratum_rows` | `scribe::claim_sample::tests::writer_with_its_own_stratum_keeps_its_key` (40 of 20,040 rows), `writers_in_the_shared_stratum_keep_their_key` (10 of 20,030 rows); fork `small_writers_keep_their_fields` | PASS |
| AC-015.3 Cochran sizes with FPC | `VariantShreddingPolicy::stratum_sample_size`; `BIFROST_VARIANT_SHREDDING` (`z = 2.5758`, `e = 0.02`, 30, 10%) | `scribe::claim_sample::tests::bifrost_policy_follows_cochran` (4,147 / 98 / 30); fork `stratum_sample_size_follows_cochran` | PASS |
| AC-015.4 Ranking at the cap by rows covered, then name | fork `VariantSampler::layout` weights rows/kept | fork `cap_ranks_by_rows_covered`, `children_are_capped_by_frequency_then_name` | PASS |
| AC-015.5 No buffer, bounds, or memory charge | `VariantPrefix`, `PrefixStep`, `ReadyPrefix`, `RollingArtifactWriter::pending`, `open_inferred`, `reserve_variant_prefixes` deleted; `max_rows`/`max_bytes` gone from the policy | compiles without them; `variant_inference_memory_releases_on_every_terminal` deleted with the prefix it tested | PASS |
| AC-015.6 Residual metric recorded | `parquet/variant_residual.rs` (`variant_residual_share`, `VariantWriter::record`); Scribe seal; Forge `record_variant_residual_shares` | `parquet::variant_residual::tests::residual_share_measures_unshredded_bytes`; `scribe::parquet_writer::tests::final_objects_record_residual_share_and_recovery_runs_do_not`; Forge sample count asserted in the AC-013 journey | PASS |
| Recovery runs unshredded; final objects take the claim's sample | `RollingArtifactWriter::variant_layout: Option<VariantLayout>` (`None` = recovery run) | `scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_the_claim_sample_shreds` | PASS |
| AC-013 Forge outputs use the combined source layout; threshold; type conflict; values unchanged | compaction `combined_variant_layout` (footers only, 16 at a time); `VariantParquetWriterBuilder::new(inner, layout)` | `forge::managed_rewrite::rewrite_outputs_share_the_combined_source_layout` (`z` shredded by one source, 1 of 13 rows, dropped; `t` keeps the integer type); fork `combine_counts_footer_leaves`, `combine_keeps_the_type_covering_more_rows`; compaction `rolled_outputs_share_the_combined_source_layout` | PASS |
| AC-013 no prefix reservation | `reserve_variant_prefixes` and its call deleted; `RUNNER_MAX_PARALLELISM` private again | redux lib 867 passed | PASS |
| Neighbors and journeys | — | redux lib (Postgres) 867/867; redux `integration --run-ignored all` 73/73; `wyrd-testing --test scribe` `recovery::variant_staging_restores_and_publishes_once` 1/1; `wyrd-testing --test oracle -P journey --run-ignored all` 53/53 (includes `published::struct_and_variant_share_physical_pushdown`); fork iceberg lib 1797/1797; compaction core lib 153/153. After repinning: redux lib 867/867, `forge::managed_rewrite` 12/12, `check:workspace-hack` clean, one `iceberg` in `Cargo.lock` | PASS |
| Lints | — | nightly fmt (Wyrd, fork, compaction); clippy `-D warnings` on `vala-bifrost-redux --all-features --all-targets`, fork `iceberg --all-targets`, `iceberg-compaction-core --all-targets` | PASS |

Pins: iceberg-rust `087c430df865c2fd78738eec9f149cc5a6749fd6`, iceberg-compaction `5579ea3c9431a619212788ebca2059bc81da9d10` (both pushed).

Corrections recorded in `spec-revision-21-sampling.md`:

- Pass 2 re-reads `principal_id`, which gives each row its stratum without
  holding one entry per row.
- The metric is labelled by writer role, not by table:
  `architecture/bifrost-design.md` forbids table labels.
- Forge reads each output footer once to measure it. Iceberg's `DataFile`
  keeps only the whole Variant column's size.

Diagnosis (first run of the AC-013 journey):

- **Symptom:** the hot-object assertion failed.
- **Evidence:** the expected rows had `{"t":1,"a":1}` and the actual rows had
  `{"a":1,"t":1}`.
- **Cause:** the fixture used `serde_json` insertion order, while Variant
  objects read back with their keys sorted.
- **Fix site:** the fixture now writes its keys in sorted order. No
  production change.

| New item | Owners searched | Why new |
|---|---|---|
| `scribe::claim_sample::ClaimVariantSample` | `claim_merge::StagedRunMerge` (merge order, all columns), `claim_assembly::ClaimAssembler` | one owner of the claim's two projected passes; the merge reads every column in merge order and cannot give run-order positions |
| `parquet::variant_residual::{VariantWriter, variant_residual_share}` | `forge/metrics.rs`, Scribe persistence metrics, fork `parquet_to_data_file_builder` (whole-column size only) | one footer measure shared by Scribe and Forge |
| `record_variant_residual_shares` (Forge executor) | compaction `combined_variant_layout` (sources, private to the fork) | outputs' footers are not loaded anywhere else |
| fork `VariantSampler::column_names` | `VariantSampler` internals | lets Scribe project only the Variant columns the sampler reads |

Commands:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib'`;
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/claim_sample::/) | test(/variant_residual::/) | test(/parquet_writer::tests::(recovery_runs|rerun_claim|final_objects)/)'`;
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration --run-ignored all'`;
fork: `mise exec -- cargo test -p iceberg --lib`; compaction: `mise exec -- cargo test -p iceberg-compaction-core --lib`.

#### Output leaf projection evidence (spec revision 22)

One physical rule, `LeafPathPushdown` (`oracle/leaf_paths.rs`), walks the plan
top-down and records, per scan column, the literal paths the query reads. A
Variant path (`variant_get` with literal keys) decodes its `typed_value`
subtree, its path-local `value` and the top-level `metadata`. A Struct path
(`get_field` literal chain) decodes only that field. Any other use reads the
column whole. Hot reads trim through `HotParquetExec.leaf_paths`; published
reads pass the same `LeafPaths::leaf_mask` to the iceberg fork as
`ParquetFileReadNarrowing.projection`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-007 hot projection-only decodes only the path's leaves | `LeafPaths::projection` (`OracleVariantSql::trimmed` / `StructTrim`), `HotParquetExec::with_leaf_paths` | `oracle::leaf_paths::tests::trimmed_struct_reads_only_its_paths_leaves` (decoded `id`, `s.b`, `v.metadata`, `v.typed_value.a.{value,typed_value}`) | PASS |
| AC-007 published projection-only decodes only the path's leaves | `PublishedFileReadPlanner.leaf_paths` → `LeafPaths::leaf_mask`; fork `ParquetFileReadNarrowing.projection`, `RecordBatchTransformer::promote` | `oracle::leaf_paths::tests::published_reads_decode_only_struct_and_variant_paths`; fork `struct_missing_children_are_rebuilt_by_name` | PASS |
| AC-007 trimmed result equals the all-residual/whole read (Variant and Struct) | same | both tests above compare values with a whole read | PASS |
| No regression to stock DataFusion results | `Demand` (schema-aware path recognition), `keeps_columns`, `VariantGet::placement` | `oracle::leaf_paths::tests::literal_key_reads_trim_the_scan_and_match_stock_datafusion` (10 queries: filter, ORDER BY, LIMIT/TopK, GROUP BY, nested Struct, mixed Struct+Variant); `other_uses_read_the_column_whole` (9 queries: whole column, IS NOT NULL, CASE key, integer index, window) | PASS |
| Distributed peers carry the paths | codec `leaf_paths_json` (tag 6) | redux integration (see Neighbors) | PASS |
| Neighbors and journeys | — | redux lib (Postgres) 872/872; redux `integration --run-ignored all` 73/73; `wyrd-testing --test oracle -P journey --run-ignored all` 53/53; fork iceberg lib 1798/1798; compaction core lib 153/153; `check:workspace-hack` clean | PASS |
| Lints | — | nightly fmt; clippy `-D warnings` on `vala-bifrost-redux --all-features --all-targets` and fork `iceberg --all-targets`; `git diff --check` | PASS |

Pins: iceberg-rust `9ec4f03d9130bc44fc4a05a713e7b66b304c060b`, iceberg-compaction `5919997444dd8ee4626648e51083a8a4cd4fd987` (both pushed).

Limit: DataFusion does not move field lookups below a window, so a window query
reads the column whole (pinned by `other_uses_read_the_column_whole`).

Diagnoses:

- **TopK harness.** Symptom: trimmed `LIMIT 3` returned 2 rows. Evidence:
  `SortExec TopK filter=[id@1 < 3]` survived into the second run. Cause: the
  test re-executed one plan instance, whose dynamic filter keeps state. Fix
  site: the test plans freshly per run; no production change.
- **Adapter cast.** Symptom: `s.a` still decoded for `SELECT s['b']`.
  Evidence: the schema adapter casts `s` (field-id metadata differs), and the
  planner keeps every leaf the cast consumes. Cause: trimming applied after
  the cast. Fix site: `StructTrim` takes `get_field` arguments, which
  DataFusion narrows through the cast.
- **Iceberg Struct promote.** Symptom: published read failed "Incorrect number
  of arrays for StructArray fields, expected 2 got 1". Evidence: Arrow `cast`
  of a pruned Struct. Cause: the transformer cast by position. Fix site: fork
  `RecordBatchTransformer::promote` rebuilds by name.

| New item | Owners searched | Why new |
|---|---|---|
| `oracle::leaf_paths::{LeafPaths, LeafPathPushdown, Demand}` (renamed from the Variant-only rule) | DataFusion `ProjectionPushdown`, `get_field` simplification, `variant_sql` | DataFusion pushes whole columns only; one rule now serves Struct and Variant |
| `oracle::leaf_paths::StructTrim` | DataFusion `get_field`, `named_struct`, Arrow `cast` | rebuilds the full Struct type from the read children so downstream types stay unchanged; `named_struct` cannot keep unread required children |
| `VariantGet::placement` | DataFusion `get_field` placement | lets DataFusion move Variant lookups below aggregates, as it does for `get_field` |
| fork `ParquetFileReadNarrowing.projection`, `RecordBatchTransformer::promote` | fork reader pipeline, Arrow `cast` | the reader had no way to accept a leaf mask; `cast` cannot rebuild a Struct with missing children |

Commands:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib'`;
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/oracle::leaf_paths::/)'`;
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --test integration --run-ignored all && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored all'`;
fork: `mise exec -- cargo test -p iceberg --lib`; compaction: `mise exec -- cargo test -p iceberg-compaction-core`.

### Hot-reader fix: a key the file did not shred

Diagnosis:

- **Symptom:** on a real Scribe or Forge object, `doc->>'c' = 'x'` failed with
  `Error evaluating filter predicate ... Invalid VariantArray: requires
  StructArray as input`. That happened whenever every row's data was in
  shredded keys and `c` was not one of them.
- **Evidence:** a dumped Scribe object (shreds only `a`) failed on every
  unshredded key. Fixtures with other keys left in `value` passed. Arrow
  `parquet-variant-compute` 60 `variant_get.rs:286-303`: when the key is
  absent from `typed_value` and `value` is all null, the step is `Missing`.
  With no `as_type` it returns a `NullArray`.
- **Cause:** `VariantGet::invoke_with_args` (`oracle/variant_sql.rs`) passed
  no `as_type`, then `VariantArray::try_new` refused the `NullArray`.
- **Fix site:** that one call. Every Variant leaf filter and projection on
  both readers goes through it. It now requests Variant output
  (`GetOptions::with_as_type(variant_field)`), so Arrow returns an all-null
  Variant. The result is unshredded right after, as before.

Journey note: Forge's scheduler promotes a published object right away
(`forge/scheduler.rs` `promote_hinted`), so a plain journey reads it through
Iceberg and never reaches this code. Without the fix, the Scribe recovery
journey with these filters passed (`hot_files=0 iceberg_files=1`). The new
journey parks promotion the way the existing hot-reader journey does.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| A filter or projection on an unshredded key reads null, not an error, when every residual is null | `oracle/variant_sql.rs` `VariantGet::invoke_with_args` | `oracle::nested_pushdown::tests::absent_variant_key_reads_null_when_every_residual_is_null`: failed with the production error before the fix, passed after | PASS |
| The public query over a hot-only object returns exact rows for a shredded and an absent key | journey `oracle/distributed.rs` | `distributed::pg_hot_variant_filter_on_an_unshredded_key_reads_null`: without the fix `QueryExecutionFailed` (`hot_files=1`, `StructArray as input`); with it 1 passed | PASS |
| Neighbors unchanged | `register_table` delegates to `register_table_with` | `distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads`, `published::variant_sql_registry_covers_every_session`: 2 passed; `vala-bifrost-redux --lib` with Postgres: 858 passed | PASS |
| Lints | — | nightly fmt; clippy `-D warnings` on `vala-bifrost-redux --all-targets --all-features` and `wyrd-testing --test oracle --all-features`; `git diff --check` clean | PASS |

| New item | Owners searched | Why new |
|---|---|---|
| `register_table_with` (oracle journey support) | `register_table` (now delegates), `PeerCluster::register_table_with` (peer harness, different cluster type) | the journey table needs a Variant column |
| `prove_hot_variant_absent_key` and its test | `prove_selective_predicate_pruning` (its fixed table has no Variant column) | first journey that reads a shredded Variant object through the hot reader |

Commands:
`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::nested_pushdown::tests::absent_variant_key_reads_null_when_every_residual_is_null)'`;
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::pg_hot_variant_filter_on_an_unshredded_key_reads_null)"'`.

### Arrow 60 upgrade (user decision, before Scenario 1)

Arrow 59.3 `shred_variant` silently converts values that do not fit the typed
column (for an Int64 column, `true` becomes 1 and `1.5` becomes 1; for a
Boolean column, every number becomes `true`). arrow-rs #10157 fixed this in
60.0.0: values that don't fit stay in the residual `value` column. The user
chose to move the whole stack to Arrow 60.

| Repository | Branch | Tested revision | Proof |
|---|---|---|---|
| `bohmian-ai/datafusion` | `wyrd-arrow60-variant-read-plan` | `202c5e101c8edac0f391d22fb0b29993fc19dcce` | apache main `b631f2c7d9` (arrow 60, still v55.1.0) plus the fork's seven commits, cherry-picked with `-x`. common 691, expr 323, functions 386, physical-expr 1667, physical-expr-adapter 45, pruning 108, datasource-parquet 281, core `parquet_integration` 244, all passing; sqllogictest `projection_pushdown` exit 0 |
| `bohmian-ai/iceberg-rust` | `wyrd/bifrost-variant` | `e12773876e9a1974f7e1d0fc57eef1bea6489e5c` | parquet 60 page-index port from apache/iceberg-rust#3257. `iceberg` + `iceberg-datafusion` 1968 passed, 0 failed; nightly clippy `-D warnings` clean |
| `bohmian-ai/iceberg-compaction` | `wyrd/bifrost-variant` | `15949359922a6ef7d2145db0622df1ad6da1c0a5` | core 152 passed, 0 failed; clippy clean. Its docker integration tests cannot start here: Docker Hub no longer serves `minio/mc` |
| `bohmian-ai/datafusion-distributed` (new fork, user-approved) | `wyrd-arrow60` | `22a395f89e1e49c3a47c04736ea43a0df3e617ef` | upstream is still on Arrow 59. 388 passed, 0 failed; clippy `-D warnings` clean |

Wyrd: arrow, parquet and parquet-variant move to 60.0; the four pins above
replace the old revisions; the workspace-hack is regenerated; the
`object_store` single-version check now expects 0.14.* (the whole stack
resolves 0.14.2). Code changes: `Field::with_metadata` takes the map
directly; `TableProvider::scan` takes `Option<&[usize]>`; two test helpers
read offset indexes through `page_index_for_row_group`.
`verify:bifrost`: exit 0 on the Arrow 60 tree (2026-10-07), after fixing five stale tests; diagnoses in `../diagnostics/README.md`. Pushed as Wyrd `f84ca165a`.

Open follow-up (recorded at the merge, not done): JSON-row → Arrow value
conversion (`build_list`, `build_map`, timestamp parsing run by
`RowPreflight`) still lives in `wyrd-queue/src/batch_builder.rs`. Type
conversion belongs in `wyrd-types`; moving it needs the user's go-ahead.

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
`peer_network::security::unsigned_leaf_predicates_round_trip_and_execute`, and
`query::pg_tests::sensitive_variant_leaf_is_denied_before_io`. The conversion
test covers all three leaf variants, every operator, every malformed case
above, and logical-root mismatch. The peer journey serializes Struct and
Variant predicates including `IN`, digests, decodes, executes them on a worker,
and asserts exact rows; it also checks one-byte tamper, missing mTLS, version
mismatch, and zero provider/read counters. Run:
`mise exec -- cargo nextest run --locked -p wyrd-tonic --lib --features server,client -E 'test(=private_conversion::tests::leaf_predicates_round_trip_and_reject_malformed)'`,
`mise run check:proto-drift`,
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::security::unsigned_leaf_predicates_round_trip_and_execute)"'`
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
9. `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=published::struct_and_variant_share_physical_pushdown) | test(=peer_network::security::unsigned_leaf_predicates_round_trip_and_execute)"'`
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
