# Merge plan: integration (TASK-001 + TASK-002) into TASK-003

- Source: `wyrd/bifrost-variant/integration` @ `66859d22f` (spec revision 17).
- Target: `wyrd/bifrost-variant/TASK-003` (spec revision 11; working tree uncommitted).
- Merge base: `535367c94`.
- Status: **plan only — decisions D1–D4 made; awaiting approval to execute.** Nothing below has been executed.

## Evidence

- Trial merge with `git merge-tree` on a snapshot commit of the working tree. It wrote no files and touched no index or stash.
  - 10 files have textual conflicts.
  - 28 files changed on both sides; the other 18 auto-merged.
  - `spec.md` auto-merges to exactly revision 17.
- Eight read-only audits were run: three file-scoped overlap audits, plus deleted/moved code, dead code, semantic duplicates, cross-side test breaks, and spec/docs drift.
- Every deletion and move comes from TASK-003. The integration branch only edits files.
- Taking the integration `write_batch` (verbatim, no client describe) removes the cause of the earlier `test:bifrost` publication failures. Those were the verifier publisher's HTTP describe, covering 18 tests across server, MCP, drift and SDK journeys.

## Decisions (all made by the user)

| # | Decision | Recommendation | Reason |
|---|---|---|---|
| D1 | Owner of the unsupported-type refusal | Restore `check_supported` in `wyrd-types` as the single decider, used by the SDK declaration and server registration. The catalog `iceberg_form` failure becomes an internal invariant, never a type refusal. | REQ-016 requires refusal before any request. Client-tier crates cannot depend on iceberg. TASK-003 deleted `check_supported` (base code from `44fec94cb`), so SDK refusal is gone in the merged tree. |
| D2 | A zoned timestamp of any label (e.g. `America/New_York`) | **Decided (TASK-003, implemented):** any zoned Arrow timestamp declares `TIMESTAMP_LTZ` (`TimestampKind::of`, `wyrd-types/src/timestamp.rs:104`) and is stored and read as `+00:00` (`UTC_TIME_ZONE`, `wyrd-spec/src/vala/api.rs:336-343`). A naive timestamp declares `TIMESTAMP_NTZ`; the `{utc, local}` struct declares `TIMESTAMP_TZ`. Bifrost converts the user's datetimes to the column's type. Revise REQ-016 in revision 18 to remove "a timestamp with a non-UTC zone". Drop the zone clause from the restored `check_supported`. | The user's decision for TASK-003; the code already does this. |
| D3 | TS typed-read Int64 | **Decided:** `bigint`, what Apache Arrow returns. Delete the TASK-003 bigint→number line in `nativeValue`. Variant integers keep the existing native safe-integer rule. | No new code; one field has one type. |
| D4 | Where the TASK-003 additions are specified (three timestamp types, `wyrd-types`, server cast, Map, Iceberg as schema truth) | **Decided:** spec revision 18+ adds the requirements; the existing TASK-003 task file gains a section for this scope. No new task file. | The code is on this branch and is reviewed in the same diff. |

## Steps

### 0. Commit current TASK-003 work
- Commit on `wyrd/bifrost-variant/TASK-003`. No rebase.

### 1. Merge
- Run `git merge wyrd/bifrost-variant/integration`.

### 2. Resolve the 10 textual conflicts

| File | Resolution |
|---|---|
| `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs` | One pass, `conform_to_registered`, replaces `in_declared_order` + `cast_to_registered`. It loops over the registered fields: it projects by name, keeps unregistered columns in relative order after the registered ones, and casts a column whose Iceberg type matches but whose Arrow spelling differs. It runs before the fingerprint check. Keep `columns_are_put_in_declared_order_by_name` and the cast test. |
| `crates/vala/vala-bifrost-redux/src/tables/mod.rs` | Delete `variant_identity_matches`, the `wire` closure, `field_layout_matches`, `arrow_type_shape_matches`, and the unused `VariantType` import. `validate_declared_variants` keeps only value checks (`EncodedVariant::from_bytes`, row then field order); rewrite its rustdoc. |
| `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs` | Take TASK-003: `schema_shape_matches`, `field_shape_matches` and `schema_shape_tests` go. Port `schema_shape_keeps_variant_identity_at_any_depth` and `schema_shape_rejects_reordered_columns` to the exact fingerprint. Drop the two UTC-alias tests. |
| `crates/shared/wyrd-queue/src/variant.rs` | Delete the duplicate `variant_storage_type`/`variant_storage_fields`/`variant_field`/`is_variant` and their imports; `wyrd_types::variant` owns them. Port the strict `is_variant` (extension parameters `None \| Some("")`) into `wyrd-types`. Read-side `mask_placeholders` keeps TASK-003's shredded-cell skip but validates through `EncodedVariant::validate`. Keep `VariantMetadata`/`VariantObject` imports. |
| `crates/shared/wyrd-queue/src/batch_builder.rs` | Imports from `wyrd_types` only; drop the unused top-level `variant_field`. |
| `crates/shared/wyrd-client/src/bifrost/facade.rs` | Take integration: delete `mod tests` (`variant_batch_describes_before_admission` and its helpers). |
| `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs` | Keep both `#[path]` module lists. Take integration for the Variant journey (delete TASK-003's inline copy). Use integration's message with `wyrd_types::schema::is_extension_key`. |
| `mise.toml` | Take TASK-003's directory form for the Python Bifrost journey lane; it already includes `test_variant_tables.py`. |
| `sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py` | Combine both docstrings (timestamp types + Struct/List/Variant mapping). |
| `sdks/wyrd-sdk-ts/wyrd/src/index.ts` | Keep the TASK-003 timestamp types, `timestampKinds` read wiring and Map branch. Int64 follows D3. Docs combine the timestamp text with integration's verbatim-write text. |

### 3. Silent breaks in auto-merged files
- `crates/shared/wyrd-types/src/schema.rs` tests import `check_supported`, `RowPreflight` and `WyrdQueueError`.
  - Restore `check_supported`/`check_supported_type` and their tests (D1), returning `BifrostError`.
  - Move the `RowPreflight` half of `array_becomes_list` into wyrd-queue `batch_builder` tests.
- `crates/shared/wyrd-client/tests/pg_bifrost_e2e/variant_tables.rs:21,441`: repoint to `wyrd_types::variant`.
- `wyrd-queue/src/variant.rs` tests: rename `VariantJsonEncoderFactory` to `WyrdJsonEncoderFactory`.
- Stale references:
  - fix the broken intra-doc link at `wyrd-queue/src/variant.rs:640`;
  - fix the stale `select_row_groups_for_predicates` comment at `wyrd-testing/tests/bifrost/oracle/distributed.rs:275`.
- Use one UTC constant: `tables/fields.rs` uses `iceberg::arrow::UTC_TIME_ZONE`, the server uses `wyrd_spec::vala::api::UTC_TIME_ZONE`. Pick one per tier.
- `SchemaFingerprint::from_arrow_schema_exact` must hash Variant identity (`is_variant`) so that registration reconciliation tells a Variant from a Struct. `from_arrow_schema` is persisted and stays unchanged.
- Re-add the masked-required-struct-child-null regression test lost with `scribe/fixed_ipc.rs` (`masked_required_struct_child_null_roundtrips`).
- Confirm the lockfile resolves once: `iceberg-compaction-core` 171bf39 against the DataFusion 55.1 fork patch.

### 4. Remove remaining duplicates (one owner per behavior)

| Behavior | Single owner | Removed |
|---|---|---|
| Write-path type equality | Iceberg round trip (`same_iceberg_type`). It keeps field metadata (strips only field ids) so Variant ≠ Struct, adds the `is_variant` parameters check, and ignores top-level nullability. | Hand-written UTC aliases, `arrow_type_shape_matches`, `variant_identity_matches` |
| Unsupported types | `check_supported` (D1) | Catalog `iceberg_form` → `UnsupportedType` mapping becomes an internal invariant. `field_to_spec` keeps only "no wire form". |
| Variant helpers | `wyrd_types::variant` | wyrd-queue copies |
| Column matching | `conform_to_registered` | Two separate passes |
| TIMESTAMP_TZ rebuild and format table | Rust `TimestampKind`/`TimestampTz` | Python `from_stored` arithmetic and `_FORMAT`/`_NAME` (call native, as TS does) |
| JS safe-integer rule | Native Variant rule only | `nativeValue` bigint→number line (if D3 = `bigint`) |

### 5. Tests
- Integration's `variant_tables` unsupported-type tests (Rust `:334`, `:406`; Python `test_variant_tables.py:158`) stand under D1, except their `Timestamp(us, America/New_York)` case. Under D2 it declares `TIMESTAMP_LTZ`, so change it to assert that registration succeeds and the column is `+00:00`.
- Remove the duplication between integration's raw-HTTP refusal and TASK-003's `a_type_with_no_iceberg_column_is_refused_at_registration`: keep one per surface.
- In `verification_runtime.rs` `large_model`, Utf8→LargeUtf8 is now a valid same-Iceberg-type cast. Change it to a real type change (e.g. Int64) so it still proves 409.
- Under D3, change `register-a-table-from-a-model.test.ts` and `every-iceberg-column-type.test.ts` to bigint.
- Add `every-iceberg-column-type`, `register-a-table-from-a-model` and `three-timestamp-types` to `test:bifrost:journey:typescript`.
- Trim the overlapping assertions between `variant_tables` and `register_a_table_from_a_model` (Rust/Python/TS).
- Re-check the earlier diagnosed failures after the merge; integration may already fix some:
  - gRPC smoke `details` column;
  - Oracle `support.rs` `"UTC"` fixture;
  - `REWRITE_ROW_GROUP_BYTES`;
  - `observe_run` Variant-as-object.

### 6. Spec, task files, docs
- Spec revisions 18+, newest first, above 17:
  - three timestamp types (storage, JSON Schema formats, Pydantic/Zod mapping, row-terminal reads);
  - `wyrd-types` owns the declared-type mapping;
  - the registered Iceberg schema is the table's source of truth;
  - REQ-014 server-side cast of same-Iceberg-type spellings;
  - REQ-016: SDK and server refusal stays (D1); remove "a timestamp with a non-UTC zone", because any zone declares `TIMESTAMP_LTZ` (D2);
  - Map in REQ-012;
  - fix the line-89 path.
- Task files: the TASK-003 section "Added Scope and Progress" is written (D4). After the merge, update its test names if any changed; update `spec_revision`; update the TASK-002 evidence row that cites `fixed_ipc.rs`.
- Docs:
  - `docs/.../bifrost/writing-data.svx`: wrong `400 ..._SCHEMA_FINGERPRINT_MISMATCH`; add by-name matching, Variant extension and casts;
  - `docs/.../bifrost/schema.svx`: UTC spelling;
  - `architecture/bifrost-design.md`: timestamp model and ingest cast;
  - `wyrd-client/src/bifrost/table.rs:73,83-86`;
  - the Python `__init__.py`/`.pyi`/`src/bifrost/mod.rs` "non-UTC timestamp" sentences;
  - the REQ-016 supported-types docs page.

### 7. Verify
- `mise run fmt`, `mise run lints`, `mise run codegen:check`.
- `mise run test:bifrost:integration:redux`.
- `mise run test:bifrost:journey:{server,mcp,sdk,python,typescript}`.
- Not `verify:bifrost`.

### 8. Commit
- Two commits, the merge and then the dedup, each after user approval.
- Git identity Thorrester, no AI co-author trailers.
