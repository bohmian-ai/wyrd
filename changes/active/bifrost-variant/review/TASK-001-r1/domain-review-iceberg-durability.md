# Iceberg durability domain review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Reviewed domain: Iceberg v3 creation and validation, append row-ID assignment, hidden row-lineage transport, repeated Forge rewrite and pre-commit refusal, five-field handoff and recovery identity, v3 manifest/snapshot garbage collection, and Scribe/Forge Bloom geometry and folding.

The candidate commit remained `3cf911fce699bbfe197f8b95e72b13e2f551f766` throughout this review.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Fresh-v3-only catalog contract | `spec.md` REQ-001/AC-002; `architecture/bifrost-design.md`; `catalog/bifrost_catalog.rs:916-1150` | PASS: both built-in and user registration converge on `create_physical_table`, which creates `FormatVersion::V3`; reconciliation rejects any existing non-v3 physical table. |
| Append row-ID assignment | Apache Iceberg v3 row-lineage and first-row-ID inheritance rules; `forge/scribe_promotion.rs:999-1055`; pinned `iceberg-rust` fast-append implementation and tests | PASS: Scribe promotion uses the native Iceberg fast-append transaction, leaving first-row-ID allocation to the commit mechanism required by the specification. |
| Hidden lineage through rewrite | `spec.md` REQ-002 and “Safe pruning and hidden row lineage”; pinned `iceberg-compaction` `datafusion_processor.rs:867-955`, `executor/datafusion/mod.rs:246-472`; `forge/managed/executor.rs:258-336` | PASS: v3 scans add the two reserved metadata fields only to the internal physical schemas; batches require non-null `Int64` values and row IDs are checked for uniqueness before a handoff can be returned. The public logical schema is unchanged. |
| Publication and no-partial-commit refusal | `forge/publication.rs:353-465,1456-1492`; `forge/worker.rs` rewrite publication callers | PASS: output `DataFile` metrics must prove both reserved fields have one non-null value per record while the commit request is still a pure derivation. Refusal occurs before the catalog transaction is assembled, so it cannot partially publish. |
| Five-field handoff and recovery identity | `forge/managed/handoff.rs:13-108`; `forge/managed/executor.rs:285-336`; `forge/publication.rs`; `forge/worker.rs` reconciliation paths | PASS: `RewriteHandoff` remains exactly the existing five fields. Lineage is carried in the output files and their `DataFile` evidence, not as a new public field or recovery identity. |
| Repeated rewrite and v3 GC | `forge/gc.rs:223-320`; pinned `iceberg-rust` v3 manifest-rewrite implementation; `tests/integration/forge/managed_rewrite.rs:1120-1695` | PASS: manifest rewrite now uses the native v3-capable Iceberg transaction instead of skipping v3. The journey covers user and built-in tables, two rewrites, invalid-lineage refusal without a commit, recovery under the same scheduling identity, manifest rewrite, snapshot expiry, and fresh post-GC row-ID assignment. |
| Bloom sizing and folding | `spec.md` REQ-005/AC-009; `parquet/writer_properties.rs:31-151,295-325`; Scribe and Forge callers; parquet-rs 59.3 writer source and official API documentation | FAIL: the output behavior is correct, but Wyrd duplicates parquet-rs's native row-group NDV default with its own constant and per-column setter. See `ICE-DUR-001`. |
| Dependency pins | root `Cargo.toml`, `Cargo.lock`, `vala-bifrost-redux/Cargo.toml`; checked-out fork revisions | PASS: all Iceberg crates share `e999331f280b698bcd026550812b5047e8789df6`; compaction is pinned to `94db7b94f72c48c75c937d36a59e80c237e8ca72`; the checked-out repositories matched those exact revisions. |

Apache Iceberg's v3 specification requires unique `_row_id`, inherited `_last_updated_sequence_number`, and copying both non-null values when an existing row moves to another data file. It also specifies first-row-ID inheritance and preservation during manifest rewriting: [Apache Iceberg specification, Row Lineage](https://iceberg.apache.org/spec/#row-lineage).

For Bloom filters, parquet-rs already defines the standard behavior the task needs: when NDV is unset on an enabled filter it resolves to `max_row_group_row_count`, and after insertion the writer folds the filter to the smallest size that retains the configured FPP: [parquet-rs `BloomFilterProperties`](https://arrow.apache.org/rust/parquet/file/properties/struct.BloomFilterProperties.html).

## Verification assessment

Independently rerun on the immutable candidate:

- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=parquet::writer_properties::tests::bloom_capacity_uses_row_group_limit_for_scribe_and_forge)'` — PASS, 1 test.
- `mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-rust/Cargo.toml -p iceberg --lib -E 'test(=arrow::schema::tests::variant_round_trips_unshredded)'` at `e999331f280b698bcd026550812b5047e8789df6` — PASS, 1 test.
- `mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-compaction/Cargo.toml -p iceberg-compaction-core --lib -E 'test(=compaction::tests::rewrite_preserves_v3_row_lineage)'` at `94db7b94f72c48c75c937d36a59e80c237e8ca72` — PASS, 1 test.
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite)'` — PASS, 1 test in 8.773s.

The task records successful task-local evidence for the same proofs plus `git diff --check`. I did not rerun unrelated Variant/Oracle/SDK lanes because they do not strengthen this domain conclusion. The focused Bloom test proves resolved capacity and FPP for both recipes; folding itself is native parquet-rs behavior documented and tested by that dependency. Existing Bifrost pruning journeys cover physical Bloom presence and Bloom-negative row-group pruning, but TASK-001 did not list them as a task-local command.

## Material finding

### ICE-DUR-001 — DRIFT: Wyrd duplicates parquet-rs's native Bloom NDV resolution

- **Classification:** DRIFT
- **Violated obligation:** Human standing direction requires Wyrd to use the established mechanism and flags a setting or check that comparable widely used projects do not require. Ponytail requires using the native platform behavior before adding local policy.
- **Location:** `crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs:34-41` and `:146-151`.
- **Evidence:** The candidate introduces `BLOOM_NDV = DEFAULT_MAX_ROW_GROUP_ROW_COUNT` and explicitly calls `set_column_bloom_filter_max_ndv` for every Bloom column. parquet-rs 59.3 already resolves an enabled filter with no explicit NDV to `WriterProperties::max_row_group_row_count`, then folds the populated filter to the target FPP. The candidate keeps the default row-group row maximum, so the local constant and setter produce exactly the native default and add no behavior.
- **Observable consequence:** Today this is behaviorally redundant. It creates a second geometry authority that can silently diverge if Bifrost later changes `max_row_group_row_count`: parquet-rs would naturally follow the changed writer property, while `BLOOM_NDV` would remain pinned to the library default. It also makes Wyrd own a setting parquet-rs deliberately owns.
- **Required testable correction:** Delete `BLOOM_NDV` and the per-column `set_column_bloom_filter_max_ndv` call. Keep enabling each selected column and setting the existing `BLOOM_FPP`; allow parquet-rs to resolve NDV from the writer's row-group maximum and perform folding. Keep the existing focused test asserting that both Scribe and Forge resolve `bloom.ndv()` to `max_row_group_row_count()` and preserve the FPP. No new option, helper, check, or dependency is required.

## Overall result

**FAIL**

Iceberg v3 durability, lineage preservation/refusal, unchanged handoff/recovery identity, and v3 GC satisfy the approved task and the Apache Iceberg model. One bounded DRIFT finding remains in Bloom configuration: the candidate re-implements a parquet-rs native default instead of relying on the standard mechanism.
