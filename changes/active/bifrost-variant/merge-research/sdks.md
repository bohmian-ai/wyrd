# Merge research: sdks/ (Python, TypeScript, Rust SDKs)

OURS = `HEAD` cfe21dc90, THEIRS = `origin/main` 8a8813a74, base 5ab92b003.
Evidence comes from `git merge-tree --write-tree HEAD origin/main` (tree `574f4069d`),
`git diff`, `git log`, and `git grep` on both sides. Nothing was built or run.

## Summary

19 conflicting files: 11 content conflicts and 8 modify/delete conflicts (main deleted, ours modified).

| Resolution kind | Count | Files |
|---|---|---|
| Combine (hand-merge the source) | 6 | py `bifrost/__init__.py`, py `stubs/bifrost.pyi`, py `src/bifrost/mod.rs`, ts `native/src/lib.rs`, ts `src/index.ts`, ts `tests/unit/bifrost-query.test.ts` |
| Regenerate (never hand-merge) | 3 | py `bifrost/__init__.pyi` (from `stubs/bifrost.pyi`), ts `index.d.ts` + `index.d.cts` (from `native/src/lib.rs`) |
| Take main, then port our assertions into main's stories | 2 | py `tests/integration/bifrost/test_bifrost_e2e.py`, ts `tests/integration/otel-export.test.ts` |
| Accept main's deletion, then port our assertions | 8 | py `state/test_observe_journey.py`, `test_bifrost_query.py`, `test_drift_journey.py`; rust `tests/drift_verification.rs`, `tests/observe_run.rs`; ts `drift-verification.test.ts`, `observe-run.test.ts`, `oracle-query.test.ts` |

Risk: high 5 (the three py/ts source files, py `mod.rs`, ts `lib.rs`); medium 7 (the four journey ports, the unit test, two regenerations); low 7 (deletions whose only payload is already covered or moot).

### Top 5 hazards
1. **The terminal error shape changes underneath code that merges cleanly.** Main added `PyQueryTerminal.error_code`/`error_detail` (`origin/main:sdks/wyrd-sdk-python/src/bifrost/mod.rs:418-434`). They read `QueryTerminalError{code: QueryTerminalErrorCode, detail: QueryErrorDetail}`. OURS 70142ba09 replaced that with `error: Option<Box<WyrdProblem>>` (`HEAD:crates/wyrd-spec/src/vala/api.rs:857`) and deleted `QueryErrorDetail`/`QueryTerminalErrorCode`. The merged file auto-merges and still references `wyrd_spec::vala::api::QueryErrorDetail::as_str` (merged mod.rs:522), so it will not compile.
2. **Python `TableConfig.from_json_schema` drops `time_column` and breaks at runtime after a clean auto-merge.** The merged `__init__.py:329` calls `_layout_json(partition_granularity, sort_keys, bloom_columns)` with 3 args, but ours made `_layout_json` take 4 (`time_column`). The result is a TypeError on every `TableConfig(...)` once `__init__` routes through main's new `from_json_schema`.
3. **The positional signatures our new journeys use changed on main.** These tests (Py and TS, every new Variant, timestamp and time-column story) use main's old call shapes and will fail or silently mis-bind after merge:
   - Python `TableConfig.from_arrow(schema, table)` is now `(table, schema)`.
   - `Bifrost.sql(q, Model)` is now `sql(q, params=None, *, model=Model)`. Passing a model positionally lands it in `params` and raises a params validation error.
   - TS `TableConfig.fromJsonSchema/fromArrow(table, schema, layout, target, type)` is now `(table, schema, {layout, compactionTargetFileSizeBytes, compactionType})`.
   - TS `bifrost.sql(q, rows)` is now `sql(q, params, rows)`.
   - Constructor transport args (`server_url`/`credential`, `serverUrl`/`credential`/`grpcUrl`) were removed in 1ca7a4316 / 753916bd0.
4. **`wyrd_queue::json_schema_to_arrow` is gone on OURS.** It moved to `wyrd_types::json_schema_to_arrow` (`HEAD:crates/shared/wyrd-types/src/schema.rs:192`, re-exported by `wyrd_client::bifrost`). Main's new TS `telemetry_schema` (merged `native/src/lib.rs:1370`) auto-merges with the old path, and the py `record` conflict offers it again.
5. **Main's replacement stories assert pre-Variant column types.** `origin/main:sdks/wyrd-sdk-python/tests/integration/observe/test_observation_surface.py:59-66,129-151` and `origin/main:sdks/wyrd-sdk-ts/wyrd/tests/integration/observation-surface.test.ts:57-71` read `vala.eval.observations.context`/`media` as JSON **text**. On OURS these are Variant columns that the typed terminal decodes to `dict`/`list` (our deleted `test_observe_journey.py` and `observe_run.rs` show the conversion). Both files merge cleanly and will fail at runtime.

## Per-file resolution table

| file | main intent (commits) | ours intent (commits) | resolution | risk | proof |
|---|---|---|---|---|---|
| `sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py` | Several changes: `card_ref`→`card_uid` correlation (dadaa3ab9); transport args removed, `client=` only (1ca7a4316, 753916bd0); ordered bind `params` and keyword-only `model` (82972f9f9); typed `QueryTerminal`; `to_ipc`→`to_bytes`; new `TableConfig.from_json_schema` staticmethod that `__init__` delegates to; `from_arrow(table, schema)` order swap (1d42a1e1b) | `time_column` layout arg; Pydantic `_pydantic.json_schema` hook for timestamp types; `_validated_rows` decodes Variant cells and the three timestamp types natively; `managed_candidates` removed from `TableDescription`; `PhysicalLayout.time_column` (2afc97da2, 3d673d597, f805d1b2b, cfe21dc90, 66859d22f) | **Combine.** Imports: union (`sys`, `datetime`, `Mapping`, `Sequence`), plus ours `from ..types import ...`. Take main's structure, where `__init__` delegates to `from_json_schema`. `__init__` computes `schema` with our `sys.modules`/`_pydantic` hook and calls `TableConfig.from_json_schema(table, schema, partition_granularity, sort_keys, bloom_columns, time_column, compaction_target_file_size_bytes, compaction_type)._native`. Add `time_column: str \| None = None` to `from_json_schema` after `bloom_columns`, and pass it into `_layout_json` (fixes merged line 329). `from_arrow` keeps main's `(table, schema)` order with our `time_column`. Docstrings: ours, with `card_ref`→`card_uid`. Drop `managed_candidates` from `TableDescription` (ours; the field no longer exists in `HEAD:crates/wyrd-spec/src/vala/api.rs`). Keep our `_validated_rows`, `_wyrd_timestamp`, `_is_variant`, `_holds_variant`, `_native_value` unchanged (they use `result.to_arrow()`, which survives main's rename). | high | `mise run py:test:unit` (`tests/unit/bifrost/test_timestamp_types.py`, `test_reserved_column.py`, `test_bifrost.py`); `mise run py:typecheck` (`tests/typecheck/bifrost_contracts.py`); py integration lane for `tests/integration/bifrost/*` |
| `sdks/wyrd-sdk-python/python/wyrd/stubs/bifrost.pyi` (hand-authored stub source) | Same surface changes as `__init__.py`; adds `QueryTerminal`, `QueryParam`, `from_json_schema`; full `Args:` blocks (required by `scripts/assemble_stubs.py::validate_source_stub`) | `time_column` arg and docs, `PhysicalLayout.time_column`, no `managed_candidates` | **Combine by hand** (this file is the stub source). Take main's text. Add `time_column` to `__init__`, `from_json_schema` and `from_arrow` signatures **and an `Args:` entry in each**, or `codegen:stubs` exits. Add `PhysicalLayout.time_column`; remove `managed_candidates`; use `card_uid` wording. Update the `QueryTerminal.error_code` doc: it now returns the catalog code (`WYRD_…`), not a snake_case enum name. | high | `mise run codegen:check` |
| `sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.pyi` | Generated | Generated | **Regenerate** with `mise run codegen:stubs` (`codegen:regen`) after the stub source is resolved; do not hand-merge | medium | `mise run codegen:check` |
| `sdks/wyrd-sdk-python/src/bifrost/mod.rs` | `CardUid` correlation; `QueryParam` binding (`query_params`); typed `PyQueryTerminal`; `to_bytes`; `client`-only constructors via `PyWyrdClient::resolve`; `describe_table(ns, name)`; `record` schema errors as catalog code via `client_error(error.into())` (dadaa3ab9, 82972f9f9, 753916bd0, 515f16c97) | `timestamp_kinds`, `timestamp_tz_texts`, `variant_to_python` on `PyQueryResult`; `timestamp_format` pyfunction registered; `wyrd_types::json_schema_to_arrow` (2afc97da2, 3d673d597, f805d1b2b) | **Combine.** Take main everywhere; keep ours' three `PyQueryResult` methods and `timestamp_format` plus its registration (both auto-merge). For the one textual conflict (`record`), use `wyrd_types::json_schema_to_arrow(&schema_value).map_err(\|error\| client_error(error.into()))?`. `BifrostError`→`BifrostClientError` `From` exists at `HEAD:crates/shared/wyrd-client/src/bifrost/query.rs:84`. **Silent fix:** rewrite `PyQueryTerminal::error_code` to `Ok(self.0.error.as_ref().map(\|p\| p.code.clone()))` and `error_detail` to `self.0.error.as_ref().map(\|p\| p.detail.as_str())`, because ours' `WyrdProblem` replaces `QueryTerminalError`. | high | `mise run lints` (clippy all-features); `mise run check:pyo3-scope`; py integration `test_query_bifrost.py`, our late-failure story |
| `sdks/wyrd-sdk-python/tests/integration/bifrost/test_bifrost_e2e.py` | Rewritten into short stories on session fixtures; OTel export tests moved out to `test_otel_export.py`/`otel/test_otel_signals.py` (753916bd0, d9af2ef2e, dadaa3ab9) | RBAC assertion via `.code`; OTel attribute/body/events read natively as Variant instead of protobuf `KeyValueList` (f805d1b2b, 0e37748f3) | **Take main.** Ours' OTel hunks are moot because main removed those tests. Port the Variant-native assertions into `test_otel_export.py`: add `attributes`, `resource_attributes`, `events` to `SpanRow` as `dict[str, Any]`/`list[dict]` and assert `attributes["gen_ai.request.model"]`. Ours' RBAC hunk is already the style main uses (`raised.value.code`). | medium | py integration lane: `test_otel_export.py`, `test_bifrost_e2e.py` |
| `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py` (main deleted) | Split into `test_observe_a_run.py` + `observe/test_observation_surface.py` (88b522cf8) | Eval `context` read as `dict`, `media` as `list[dict]`; span Card via `attributes ->> 'wyrd.card_ref'` (f805d1b2b, e8a90cb53, 8fc9c9694) | **Accept deletion.** Port into `observe/test_observation_surface.py`: `EvalRow.context: dict`, `media: list[dict] \| None`, `MEDIA_TEXT`→a list value, `by_context` keyed by `row.context["answer"]`. Do **not** port the `wyrd.card_ref` attribute assertion. Main renamed the stamped attribute to `wyrd.card_uid` (`origin/main:crates/vala/vala-bifrost-redux/src/gate/mod.rs:1811`), and main's story already asserts the `card_uid` column. | medium (silent) | py integration `observe/test_observation_surface.py` |
| `sdks/wyrd-sdk-python/tests/integration/test_bifrost_query.py` (main deleted) | Replaced by `test_query_bifrost.py` (bound-parameter story, ef6b5d6c5) | Canonical-signal round trip rebuilt on stock OTLP exporters; new `test_builtin_variant_and_struct_payloads_are_queryable` (Variant extension kept, native decode, `u64::MAX`, `parse_json` refusal, late failure after one 8192-row batch keeps `WYRD_VALA_500_QUERY_EXECUTION_FAILED` + details) (7f6508545, 029da306f, 4559cff37, 7dd34bdcf, 8a3e63168, 67fd6b0b8) | **Accept deletion; port the Variant story.** Move `test_builtin_variant_and_struct_payloads_are_queryable` (HEAD lines 456-602) into a new story file such as `tests/integration/test_query_variant_payloads.py`, using main's fixtures: `bifrost`, `otlp_environment` instead of hand-built headers, `Bifrost()` with no transport args, `sql(..., model=Row)`. It is the only Python proof of the late-failure terminal (FIND-003-3) and of the Variant SQL surface. The canonical-signal round trip duplicates main's `test_otel_export.py` + `otel/test_otel_signals.py`; fold its Variant column assertions into those stories instead of keeping a copy. | medium | py integration lane on the new file |
| `sdks/wyrd-sdk-python/tests/integration/test_drift_journey.py` (main deleted) | Replaced by `test_scheduled_drift_alerts_operator.py` / `test_verify_in_real_time.py` (ef86c6b38) | `details`→`to_json(drift_report['features'])` (f805d1b2b) | **Accept deletion.** Main's replacement stories never select `vala.verification.results.details` (`git grep` empty), so nothing to port | low | — |
| `sdks/wyrd-sdk-rust/tests/drift_verification.rs` (main deleted) | Moved to `tests/integration/*` single binary (515f16c97) | Eval context as `serde_json::Value` (e8a90cb53, f805d1b2b) | **Accept deletion.** Main's `tests/integration/*.rs` select no Variant-changed columns | low | `mise exec -- cargo nextest list -p wyrd-sdk-rust` (compile) |
| `sdks/wyrd-sdk-rust/tests/observe_run.rs` (main deleted) | Moved to `tests/integration/observe_a_run.rs`; also carried be6ab6b5d (client-stamped `wyrd_event_time`) | Context/media as `serde_json::Value` (f84ca165a) | **Accept deletion.** `observe_a_run.rs:118` selects only drift series columns | low | Rust journey lane `-p wyrd-sdk-rust -P journey` |
| `sdks/wyrd-sdk-ts/native/src/lib.rs` | `card_uid`; `QueryParam`; `table_config_from_json_schema`/**`table_config_from_arrow_ipc`** returning `NativeTableConfigResult` with every refusal as catalog metadata (`declare`, `validation_error`); `record`/`dropped` telemetry; `describe_table`; removed `connect_bifrost`/`describe_table_config` free fns; workspace now allows `needless_pass_by_value` and warns on `allow_attributes_without_reason` (`origin/main:Cargo.toml:272,288`) | Independently added the **same** `table_config_from_arrow_ipc` and result-returning `table_config_from_json_schema`; plus `variant_to_value`, `timestamp_format`, `timestamp_text`, `stored_timestamp_text`, `timestamp_kinds`; `wyrd_types::TimestampKind` (2afc97da2, 9545fc591, feccc925b, f805d1b2b) | **Combine.** Take main's versions of both `table_config_*` functions; drop ours' duplicates (main's is a superset: it also catalogs layout and compaction refusals). Append ours' five helpers after main's `table_config_from_arrow_ipc` and **remove their `#[allow(clippy::needless_pass_by_value)]` + justification comments** (the lint is allowed workspace-wide on main, and a reasonless `#[allow]` warns). Keep `use std::collections::HashMap`, `Uint8Array`, `wyrd_types::TimestampKind`. **Silent fix:** `telemetry_schema` must call `wyrd_types::json_schema_to_arrow`. | high | `mise run lints`; `mise run ts:napi:check`; `mise run ts:test:unit`; `ts:test:integration` |
| `sdks/wyrd-sdk-ts/wyrd/index.d.ts`, `index.d.cts` | Generated by napi | Generated | **Regenerate** with `mise run ts:build` (napi build, then `cp index.d.ts index.d.cts`) after `native/src/lib.rs` is resolved; do not hand-merge | medium | `mise run ts:napi:check` |
| `sdks/wyrd-sdk-ts/wyrd/src/index.ts` | `card_uid` correlation; `TableConfigOptions` object arg; `jsonSchemaText` helper; `fromArrow` via native; ambient `ClientOptions{client}`; `sql(query, params, rows)`; otel/telemetry `record`; principals, workflow, cli; biome formatting (dadaa3ab9, a2e20233a, 753916bd0, 51baa7f21, 16532e3a2) | `timeColumn` layout; `PhysicalLayout.time_column`; no `managed_candidates`; Zod `override` for `TIMESTAMP_NTZ`; `TimestampNTZ/LTZ/TZ` branded types + factory; `nativeValue`/`timestampValue` row projection; `BifrostQueryStream.timestampKinds`; `Map` in `DataTypeSpec` (2afc97da2, d4a0cba97, 20efc8fee, cfe21dc90, 9545fc591) | **Combine.** Imports: ours' `DataType`/`Field` plus main's single-line form. Native destructure: union of both lists (no `describeTableConfig`). `timestampKinds` getter: keep ours, then main's `[Symbol.asyncIterator]` doc. `fromJsonSchema`/`fromArrow`: take main's `options: TableConfigOptions` signatures and bodies. Put ours' Zod hook inside main's `jsonSchemaText`, as `source.toJSONSchema({ override: zodTimestampFormat })`, so `record` gets it too. Keep ours' widened `JsonSchemaSource`. Docs: ours' timestamp paragraph + main's `@param` tags; `@throws` names catalog codes. `writeBatch` doc: ours' by-name text + main's `@param/@throws`. `sql` rows map: ours' `nativeValue`/`timestampValue` loop. `TableLayout.timeColumn`, `layoutJson` `time_column`, and no `managed_candidates` auto-merge correctly. Run `mise run ts:format` (biome). | high | `mise run ts:typecheck`, `ts:lints`, `ts:test:unit` (`timestamp-types.test.ts`), `ts:test:integration` |
| `sdks/wyrd-sdk-ts/wyrd/tests/integration/drift-verification.test.ts` (main deleted) | → `scheduled-drift-alerts-operator.test.ts`, `verify-in-real-time.test.ts` (0c6bcacbb) | Variant context decode (e8a90cb53) | **Accept deletion.** Nothing to port | low | — |
| `sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts` (main deleted) | → `observe-a-run.test.ts`, `observation-surface.test.ts` | `writeBatch` by name (ec40d262c) | **Accept deletion.** Port the Variant context/media expectations into `observation-surface.test.ts:65-71`: `context: { answer: "yes" }`, `media: [{…}]`, and widen the Zod `SpannedEvalRow` to accept object/array | medium (silent) | `ts:test:integration` |
| `sdks/wyrd-sdk-ts/wyrd/tests/integration/oracle-query.test.ts` (main deleted) | → `query-bifrost.test.ts`, `query-results.test.ts` (4aa3f0923) | "Variant query journey" (u64::MAX, late failure, native decode) (7f6508545, 7dd34bdcf, 8a3e63168) | **Accept deletion; port the `Variant query journey` describe block** into a new story file such as `query-variant-payloads.test.ts` on `serverTest()`/`Bifrost.connect()` with `sql(q, [], rows)`. It is the only TS proof of the late-failure terminal. | medium | `ts:test:integration` |
| `sdks/wyrd-sdk-ts/wyrd/tests/integration/otel-export.test.ts` | Rewritten as a `serverTest` story (16f8def64, 753916bd0) | `readDecoded` reads attributes as native Variant (f805d1b2b) | **Take main.** Add `attributes`/`resource_attributes` (`z.record(z.string(), z.unknown())`) to main's `SpanRow` and assert a key, replacing ours' `readDecoded` (which uses removed `serverUrl/credential/grpcUrl` and the old `sql(q, rows)`) | medium | `ts:test:integration` |
| `sdks/wyrd-sdk-ts/wyrd/tests/unit/bifrost-query.test.ts` | Rewritten on a `fakeStream` helper with `test(...)`; type fixtures moved to `tests/types/*.test-d.ts` (0e7396a00, 16532e3a2) | Failed-terminal fixture uses the `WyrdProblem` terminal error; typing fixture adds `time_column` and drops `managed_candidates` (70142ba09, cfe21dc90) | **Take main's structure.** Change main's "failed terminal keeps its step details" fixture (`origin/main` line 80) to ours' `WyrdProblem` error object (`type,title,status,detail,code,details,remediation`). Drop ours' duplicate decode-failure test. Move the `time_column` typing check into `tests/types/table-description.test-d.ts` (one `expectTypeOf<…["physical_layout"]["time_column"]>().toEqualTypeOf<string>()`). | medium | `mise run ts:test:unit`, `ts:typecheck` |

## Silent conflicts (clean merges that break)

1. **`QueryTerminalFrame.error` type.** Main's SDK reads `error.code` (`QueryTerminalErrorCode`) and `error.detail` (`QueryErrorDetail`) at `origin/main:sdks/wyrd-sdk-python/src/bifrost/mod.rs:413-434`. Ours made it `Option<Box<WyrdProblem>>` at `HEAD:crates/wyrd-spec/src/vala/api.rs:857` and deleted both types (70142ba09). The fix is in the table above. The TS unit fixture at `origin/main:sdks/wyrd-sdk-ts/wyrd/tests/unit/bifrost-query.test.ts:80` uses the old shape.
2. **`_layout_json` arity.** The merged Python `__init__.py:329` (main's `from_json_schema`) calls `_layout_json` with 3 args, but ours' definition (merged line 152) requires 4.
3. **`wyrd_queue::json_schema_to_arrow`.** It exists at `origin/main:crates/shared/wyrd-queue/src/schema.rs:110`; on HEAD it lives only at `crates/shared/wyrd-types/src/schema.rs:192`. Main's callers are the merged ts `native/src/lib.rs:1370` and py `mod.rs` `record`. Outside the sdks group, `origin/main:crates/shared/wyrd-client/src/bifrost/table.rs:122` is the same issue for the shared group.
4. **Python `TableConfig.from_arrow` argument order** is `(table, schema)` on main. Ours' new tests call `(schema, table)`:
   - `HEAD:sdks/wyrd-sdk-python/tests/integration/bifrost/test_every_iceberg_column_type.py:36,47,99`
   - `test_table_time_column.py:55`
   - `test_three_timestamp_types.py:194`
   - `test_variant_tables.py:161`
5. **Python `Bifrost.sql(q, Model)` positional model** binds to `params` on main. Affected:
   - `HEAD:…/test_three_timestamp_types.py:108,145`
   - `test_variant_tables.py:82,101`
   - `test_register_a_table_from_a_model.py:87-124` (verify)
   - our `test_bifrost_query.py` Variant story.

   Use `model=`.
6. **Removed transport args, Python** (`server_url=`, `credential=`), at:
   - `HEAD:…/test_variant_tables.py:44-45,91`
   - our `test_bifrost_query.py` and `test_bifrost_e2e.py` hunks.

   Use the ambient env set by `wyrd_server`, or `client=`.
7. **Removed transport args, TS** (`serverUrl`, `credential`, `grpcUrl`), at:
   - `HEAD:sdks/wyrd-sdk-ts/wyrd/tests/integration/variant-tables.test.ts:38-39,91-92`
   - our `otel-export.test.ts` hunk.
8. **TS `fromJsonSchema`/`fromArrow` options object.** `HEAD:…/table-time-column.test.ts:51` passes the layout as the 3rd arg; main expects `{ layout: { partitionGranularity, timeColumn } }`. TS `sql(q, rows)` → `sql(q, [], rows)` in:
   - `three-timestamp-types.test.ts:92,120,157`
   - `variant-tables.test.ts:79,103`
   - `register-a-table-from-a-model.test.ts:86-120` (verify)
   - `every-iceberg-column-type.test.ts:96,110` (verify)
9. **TS test support.** Our `HEAD:sdks/wyrd-sdk-ts/wyrd/tests/support/test-server.ts` (`useTestServer`) duplicates main's `tests/support/server.ts::serverTest`. Main's `deploy` also isolates `WYRD_CONFIG_HOME` and clears `WYRD_ACCESS_TOKEN`/`WYRD_WORKLOAD_TOKEN`/`WYRD_TENANT`, which matters now that saved logins rank in the chain (e73f0163e). Port ours' five stories to `serverTest()`, move `columnValues` into the story or `server.ts`, and delete `test-server.ts`.
10. **Variant column types in main's replacement stories:**
    - `origin/main:…/python/tests/integration/observe/test_observation_surface.py:59-66,129-151`
    - `origin/main:…/ts/tests/integration/observation-surface.test.ts:57-71`

    Both use `context`/`media` as text; see the table.
11. **Span attribute rename.** `wyrd.card_ref` → `wyrd.card_uid` (`origin/main:crates/vala/vala-bifrost-redux/src/gate/mod.rs:1811` vs `HEAD:…:1777`). Do not carry `attributes ->> 'wyrd.card_ref'` from ours' deleted observe journey.
12. **`managed_candidates`.** Removed in `HEAD:crates/wyrd-spec/src/vala/api.rs`; still present in main's py stubs/`__init__.py` (conflict regions) and in `origin/main:crates/wyrd-spec/schemas/bifrost_table_description.json:9`, which is regenerated by the shared-spec group. TS `index.ts` auto-merged it out correctly. `tests/typecheck/bifrost_contracts.py` (renamed from `tests/bifrost/test_public_typing.py`) auto-merged correctly with `time_column` and `card_uid` (checked in merged tree).
13. **`#[allow(clippy::needless_pass_by_value)]`** on ours' TS helpers. Main removed these allows when it made the lint workspace `allow` (`origin/main:Cargo.toml:272`) and set `allow_attributes_without_reason = "warn"` (`:288`). Keeping them risks a clippy warning-as-error. Remove them.
14. **wyrd-client APIs the SDK relies on** (owned by the shared group, listed because the SDK breaks if they are not reconciled): main's SDKs call
    - `QueryResult::to_bytes` (HEAD has `to_ipc`, `HEAD:crates/shared/wyrd-client/src/bifrost/facade.rs:938`)
    - `Bifrost::sql(query, &params)` (HEAD `sql(&self, query)`, `:611`)
    - `PyWyrdClient::resolve`
    - `Correlation { card_uid }` (HEAD still `card_ref`).

    Take main for all of these in wyrd-client.
15. **Client-stamped `wyrd_event_time`** (main be6ab6b5d, `origin/main:crates/shared/wyrd-client/src/bifrost/facade.rs:530`, `handle.rs:205`) contradicts approved spec REQ-036 (`changes/active/bifrost-variant/spec.md:1111-1118`, which removes `wyrd_event_time`). No SDK story on main asserts it (`git grep` hits only docstrings, which the combine replaces). Ours wins per the spec. This needs action in the shared/redux groups (wyrd-queue `batch_builder.rs`, `producer.rs`).
16. **`wyrd.types` public module** (ours) is not listed in main's new `tests/unit/test_public_exports.py::PUBLIC_MODULES`. It is not a failure, but add `"wyrd.types"` so its `__all__` is checked.
17. **Python `record` refusal code.** Main's `test_recorded_telemetry_with_an_unmappable_schema_is_refused` expects `WYRD_VALA_400_SCHEMA_PARSE` for `{"type": "object"}`. Ours' mapper treats open objects as Variant in some positions. Confirm that `wyrd_types::json_schema_to_arrow` still refuses a property-less root with that code (unknown; prove with that test).

## Verification after the merge (sdks scope)

- `mise run codegen:check` (py stubs + TS error codes)
- `mise run ts:napi:check` (index.d.ts current)
- `mise run lints`
- `mise run fmt`
- `mise run py:format`
- `mise run py:lints`
- `mise run py:typecheck`
- `mise run ts:format:check`
- `mise run ts:lints`
- `mise run ts:typecheck`
- `mise run check:pyo3-scope`
- `mise run py:test:unit`
- `mise run ts:test:unit`
- Integration journeys:
  - `py:test:integration`
  - `ts:test:integration`
  - the `wyrd-sdk-rust` journey profile
  - in particular the ported Variant late-failure stories (Py + TS) and `observe/test_observation_surface.py` / `observation-surface.test.ts`.

## Open decisions needing the human

1. **`QueryTerminal.error_code` semantics (Python).** Main exposed snake_case closed-enum names such as `"query_execution_failed"`. Ours' wire terminal now carries the full catalog problem, so the natural value is the catalog code (`"WYRD_VALA_500_QUERY_EXECUTION_FAILED"`). Recommendation: return the catalog code and add nothing else. It matches `WyrdError.code` everywhere, and no main test asserts the old spelling. Confirm before shipping, because it is a public Python property.

There are no other genuine product or architecture choices. `wyrd_event_time` is settled by approved REQ-036, and `card_uid` by main's design.
