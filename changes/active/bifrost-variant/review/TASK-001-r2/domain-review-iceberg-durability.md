# Iceberg v3 durability domain review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Prior remediation: `changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`
- Reviewed boundary: format-v3 creation and validation, native append row-ID assignment, hidden-lineage projection and rewrite, publication failure atomicity, repeated rewrites, v3 manifest/snapshot garbage collection, and native Parquet Bloom geometry.

The candidate remained `99c5871ec5ca664b9f54baa379b437ee09d66e95` throughout this review. The human decision for `FIND-TASK-001-3` is controlling: lineage relies on standard Iceberg v3 field-ID projection, per-batch presence/type/null validation, and unchanged copying. A rewrite-wide duplicate-ID scan or a `DataFile` metrics gate is prohibited and is not required below.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Fresh v3-only physical tables | Specification REQ-001/AC-002; `architecture/bifrost-design.md` “Storage format and Variant”; `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:996-1100`; catalog registration callers | PASS: built-in and user registration share `create_physical_table`, which creates `FormatVersion::V3`, while reconciliation rejects any existing non-v3 table. No migration or compatibility mode was added. |
| Append row-ID allocation | Apache Iceberg v3 row-lineage and first-row-ID rules; `crates/vala/vala-bifrost-redux/src/forge/scribe_promotion.rs:1012-1070`; pinned `iceberg-rust` `transaction/snapshot.rs` and manifest-list inheritance | PASS: Scribe promotion uses the fork's standard fast-append transaction. The v3 snapshot/manifest-list writer assigns row-ID ranges from table `next-row-id`; Wyrd adds no alternate allocator or persisted setting. |
| Lineage projection and unchanged rewrite | Specification REQ-002; binding human decision; pinned `iceberg-compaction` `core/src/executor/datafusion/iceberg_file_task_scan.rs:160-214,764-773` and `core/src/executor/datafusion/mod.rs:251-306,414-438`; pinned `iceberg-rust` row-lineage reader | PASS: the internal scan resolves the two reserved field IDs although they are absent from the logical schema. Every v3 output batch must contain non-null `Int64` values, and that same batch is passed unchanged to the Iceberg writer. There is no production duplicate-ID collection/sort and no separate lineage model. |
| Publication boundary and failed rewrite | `iceberg-compaction` managed boundary/executor and output ledger; `crates/vala/vala-bifrost-redux/src/forge/managed/handoff.rs:13-111`; `crates/vala/vala-bifrost-redux/src/forge/publication.rs:362-458`; `crates/vala/vala-bifrost-redux/src/forge/worker.rs` rewrite/publication caller | PASS: the compaction fork owns no catalog commit permission. A batch validation failure drains writers, reports tracked objects as failed-attempt output, and returns no successful handoff. Forge can derive and submit a catalog transaction only from a successful five-field handoff. Publication checks content, partition, nonempty measurements, and live-set identity, but no longer requires optional lineage metrics. |
| Repeated rewrites and row identity | `crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs:1098-1580`; pinned `iceberg-compaction` `compaction::tests::rewrite_preserves_v3_row_lineage` | PASS: the Wyrd journey observes a user table and built-in through two physical rewrites and compares both hidden values after every step. The fork test also mixes inherited append lineage with physically stored rewrite lineage and confirms the hidden fields remain outside the logical schema. |
| v3 garbage collection | `crates/vala/vala-bifrost-redux/src/forge/gc.rs:223-341`; pinned `iceberg-rust` v3 `RewriteManifestsAction`, manifest inheritance, and snapshot expiry; Wyrd repeated-rewrite journey | PASS: Forge no longer skips v3. Native manifest rewrite carries existing data-file `first_row_id`, and the journey proves manifest merge, snapshot expiry, preserved existing lineage, and fresh append allocation after GC. No Wyrd row-ID repair path was introduced. |
| Bloom geometry | Specification REQ-005/AC-009; prior `FIND-TASK-001-8`; `crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs:101-150,302-333`; Scribe and Forge writer callers; parquet-rs writer properties | PASS: Wyrd sets only Bloom enablement and the existing FPP. parquet-rs resolves unset NDV from `max_row_group_row_count` and folds after insertion. The removed Wyrd NDV constant/setter has no replacement option, helper, or check. |
| Dependency identity | Root `Cargo.toml:234-247`, `Cargo.lock`; checked-out fork revisions and relevant source/tests | PASS: every Iceberg crate is pinned to `e999331f280b698bcd026550812b5047e8789df6`; compaction is pinned to `fb3a594b0a93d9f99b62a77084e94be96fb7fba7`; both checked-out repositories matched those revisions and were clean. |

The primary Iceberg v3 specification requires inherited row IDs for new rows and requires existing non-null `_row_id` and unmodified-row `_last_updated_sequence_number` values to be copied into replacement files. It does not make optional `DataFile` metric maps the proof of preservation. parquet-rs documents unset Bloom NDV as resolving from the maximum row-group row count and folding to the populated data. The candidate now uses those standard mechanisms directly.

## Verification assessment

Independently run against the immutable candidate and pinned forks:

- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=parquet::writer_properties::tests::bloom_capacity_uses_row_group_limit_for_scribe_and_forge)'` — PASS, 1 test.
- `mise exec -- cargo nextest run --locked --manifest-path /home/thorrester/Documents/GitHub/iceberg-compaction/Cargo.toml -p iceberg-compaction-core --lib -E 'test(=executor::datafusion::tests::row_lineage_is_complete) | test(=compaction::tests::rewrite_preserves_v3_row_lineage)'` — PASS, 2 tests at the pinned revision.
- `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..99c5871ec5ca664b9f54baa379b437ee09d66e95` — PASS.

The candidate's immutable task evidence records PASS for the repository-managed PostgreSQL journey `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite`, which covers v3 creation, two rewrites, manifest rewrite, snapshot expiry, and post-GC append allocation. I did not rerun that shared-environment journey during this parallel review. Missing/type/null refusal is proved at the exact per-batch validator and by the managed core's no-commit boundary; no prohibited duplicate-row scan or metrics-based substitute was added.

## Findings

No material findings.

`FIND-TASK-001-3` is closed: the optional-metrics publication prerequisite and rewrite-wide row-ID collection/sort were deleted; standard field-ID projection, per-batch presence/type/null validation, unchanged write, and repeated-rewrite equality proof remain. `FIND-TASK-001-8` is closed: native parquet-rs geometry now owns Bloom NDV and folding without a Wyrd duplicate setting.

## Overall result

**PASS**

The candidate satisfies the Iceberg v3 persistent-data and Forge durability obligations in this domain. Its mechanisms are the established Iceberg/parquet-rs mechanisms required by the task; no unsupported Wyrd-specific row-lineage gate, scan, setting, or option remains.
