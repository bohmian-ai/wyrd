# TASK-001 r7 persistent-schema / Iceberg / Forge domain review

## Review Findings

### Critical

No critical findings.

### Important

- **PERSIST-R7-001 — physical reconciliation does not compare Variant identity.** [`crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1101`](../../../../../crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs) converts the current Iceberg schema to Arrow and accepts it through `schema_shape_matches`, but `field_shape_matches` at lines 1524-1528 compares only name, nullability, and `DataType`; its recursive owner `arrow_type_shape_matches` at `tables/mod.rs:833-867` likewise ignores field metadata, so an Iceberg `Struct<metadata: Binary, value: Binary>` is considered the same physical type as the required atomic Iceberg Variant even though only the latter round-trips with `arrow.parquet.variant`. This violates REQ-003 and the Arrow authority's requirement to preserve logical type and metadata, and it means reconciliation can accept a stale or externally altered physical table whose catalog row claims Variant while its Iceberg schema is Struct, after which readers and writers no longer share one logical schema. Make the existing physical-schema comparison recursively require canonical Variant-extension parity through `wyrd_queue::variant::is_variant` while retaining only the approved binary/string/list/timezone normalization and ignoring field IDs; add a focused catalog/schema-shape test that rejects top-level and nested Variant-versus-storage-Struct mismatches and still accepts the pinned Iceberg Variant round trip.

- **PERSIST-R7-002 — the mandated failed-lineage/no-commit journey is absent.** TASK-001 lines 126-139 require `v3_row_lineage_survives_repeated_rewrite` to inject missing lineage and prove that the failed rewrite commits nothing, but the integration test at `managed_rewrite.rs:1450-1562` covers only successful repeated rewrites and GC; the fork's `row_lineage_is_complete` test at `iceberg-compaction/core/src/executor/datafusion/mod.rs:533-565` calls the predicate directly and therefore cannot detect moving that check after a write or accidentally returning a commit-capable handoff. The production loop currently validates each batch before writing it (`mod.rs:303-313`), but a later-batch failure may follow already opened output objects, making the observer/error/handoff boundary the durable behavior that needs proof. Extend the existing managed-rewrite integration seam, without a new harness or public hook, to force a rewrite stream with missing/null lineage after at least one valid batch and assert failure, unchanged snapshot/table rows, no commit-capable handoff, and tracked/reclaimable possible outputs.

### Suggestions

None.

## Open Questions

None.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- R6 remediation and evidence: `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md`
- Pinned Iceberg fork: `e999331f280b698bcd026550812b5047e8789df6` (tree `fa0ed3eb80c707b57a2081b9f376eedce8624f89`)
- Pinned compaction fork: `2b65fa189f2d05002acc6e59515a071a63777970` (tree `a6447b0f0754b1c6040047643ed51d53cb75d8e2`)

The supplied candidate and tree matched before and after review. CodeGraph was used before direct source inspection. I reviewed the cumulative base-to-candidate implementation, specification revision 13, TASK-001, the R6 remediation evidence, applicable repository and Arrow analytical authority, both pinned forks, and the current production and journey tests. No production source was edited.

## Boundary coverage

| Boundary | Evidence | Result |
|---|---|---|
| Built-in Variant and Struct declarations | `tables/{fields,verification/results,gateway/calls,metrics/points}.rs`; the locked Variant columns, Result Struct order/nullability, required `requested_model`, optional whole-or-absent `resolved_model`, and nullable metric bucket children match revision 13. | PASS |
| Whole-present / null-absent admission | `tables/mod.rs:256-337`; Results and Calls `WHOLE_STRUCTS`; metrics validator; peer refusal proof in `tests/gateway/peer.rs`. Partial values are refused before durable work. | PASS |
| Producers and hot/published parity | Verification and gateway producers null every child under an absent nullable parent; `typed_builtin_payloads_are_queryable` now proves resolved and unresolved gateway children hot and after flush/publication. | PASS |
| Variant Arrow/Iceberg/Parquet representation | `wyrd-queue::variant_field`, `tables::iceberg_schema_for`, pinned Iceberg `variant_round_trips_unshredded`, and promoted-object handling preserve ordinary canonical unshredded Variant bytes and the extension. | PASS for creation/round trip; FAIL for mismatch reconciliation (`PERSIST-R7-001`) |
| Logical and physical fingerprints | Variant uses the fixed `0x0d` logical tag with no storage children; nested nullability participates in schema identity; registered Iceberg IDs and hidden lineage stay excluded. | PASS |
| Iceberg v3-only creation and no migration | `bifrost_catalog.rs:999-1107` creates v3 and refuses non-v3; the product is unreleased, so the intentional nested-schema changes need no migration or compatibility reader. | PASS |
| Forge row lineage | The pinned compaction fork projects the two reserved fields only into internal schemas, resolves them through Iceberg metadata-column IDs, checks non-null `Int64`, and the Wyrd journey proves both values survive two rewrites for user and built-in tables. | PASS for success; FAIL for required failure proof (`PERSIST-R7-002`) |
| V3 maintenance and GC | `forge/gc.rs` rewrites v3 manifests while preserving explicit `first_row_id`; the integration journey rewrites manifests, expires replaced snapshots, rechecks surviving lineage, and accepts later appends. | PASS |
| Bloom row-group capacity | One writer recipe is used by Scribe and Forge; parquet-rs derives unset NDV from the maximum row-group row count and folds filters to actual values while retaining the 0.01 FPP. | PASS; hot `IN` probing remains explicitly owned by TASK-003 |
| Dependency identity | Root manifest and lockfile pin Iceberg `e999331f...` and compaction `2b65fa18...`; local checkouts matched those revisions with no path patch. | PASS |

## Verification Notes

Recorded evidence inspected: 68 `wyrd-queue` tests, 847 Postgres-backed `vala-bifrost-redux` tests, the Oracle and gateway focused tests, the two server journeys and gateway peer journey, format, lints, codegen, skills sync, docs, the pinned Iceberg Variant round-trip, pinned compaction lineage unit/rewrite tests, the Wyrd repeated-rewrite/GC journey, and Bloom-capacity unit proof.

I ran no Cargo, mise, Postgres, object-store, or fork test job in the shared checkout. A cumulative `git diff --check base..candidate` reports the pre-existing extra blank line at EOF in `TASK-001-r6/standards-review.md`; the R6 implementation record's narrower `b4ea01848..HEAD` diff-check does not cover that older artifact. This is outside the persistent-schema domain but means the cumulative diff-check is not currently green.

## Overall result

**FAIL**

The runtime schemas, producers, gateway publication proof, v3 success path, GC, fork pins, and Bloom sizing are otherwise coherent, but physical reconciliation can erase the Variant-versus-Struct distinction and the task's explicit failed-lineage/no-commit scenario remains unproven.
