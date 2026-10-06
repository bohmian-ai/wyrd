---
id: TASK-001-R3
kind: remediation
status: ready
spec: SPEC-bifrost-variant
spec_revision: 11
parent_task: TASK-001
remediates: [FIND-TASK-001-4, FIND-TASK-001-10, FIND-TASK-001-12]
---

# Close TASK-001 authority, Rust-style, and distributed-error gaps

## Authority and immutable review subject

- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Review evidence: `changes/active/bifrost-variant/review/TASK-001-r3/findings-validation.md`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Reviewed candidate: `555308ba14058ddc56102d2f925298ef43858175`

## Outcome

TASK-001's active authority states the shipped revision-11 contracts, its
cumulative Rust declarations satisfy the repository import rules, and every
catalogued Analytical worker failure reaches the existing late terminal as its
complete derive-backed problem without family-specific decoding or prose
classification.

## Issue diagnoses and required corrections

### FIND-TASK-001-4 — Active authority contradicts revision 11

`architecture/bifrost-design.md:144-152` still accepts any integral token that
fits a Variant decimal, while revision 11 and
`EncodedVariant::from_json_text` accept signed `i64`, then unsigned `u64` as a
scale-zero decimal, and refuse every other integral token. Its Variant SQL and
terminal sections at `:439-465` and `:698-715` also stop short of the revision-11
rule that every catalogued late failure carries the same complete problem over
Interactive and Analytical HTTP/gRPC, every SDK raises it unchanged, and
collection returns no partial result.

The active Bifrost design is the winning subsystem authority. Leaving it stale
makes the superseded wider-number and family-specific late-error contracts
appear valid even though executable source implements narrower behavior.

Update only those existing paragraphs. State the exact `i64`/`u64` boundary,
the out-of-range refusal, the complete derive-backed late problem for every
catalogued failure, unchanged SDK projection, no partial collected result, and
the generic fallback only for uncatalogued failures. Reuse the existing
authority and guide vocabulary. Add no document, generator, checker, setting,
option, compatibility path, or alternate error mechanism.

### FIND-TASK-001-10 — Prior import-rule remediation is incomplete

The cumulative changed Rust surface still contains ordinary function-local
`DataFusionError` imports in
`crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:1654,1678,1714`.
It also spells `serde::Serialize`, `serde_json::Value`,
`wyrd_spec::error::WyrdProblem`, and `std::fmt::Display` as qualified names in
changed fields, signatures, return types, iterator items, and bounds in:

- `crates/shared/wyrd-client/src/observe/eval.rs:188`;
- `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:2701-2713`;
- `crates/vala/vala-bifrost-redux/src/tables/signal.rs:1145`;
- `crates/wyrd/wyrd-server/src/query/service.rs:372`;
- `crates/wyrd/wyrd-server/src/verification/results.rs:593-595,758-761,947`;
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:1302-1307`.

These declarations are reachable production or journey surfaces, and R1's
stable finding explicitly required the cumulative diff to contain no such
site. Move the ordinary imports into each existing owning module or test-module
import block and use bare names in the listed declarations. Preserve the
documented local `Trait as _` exception. Change no behavior and add no lint,
checker, wrapper, script, or allow attribute.

### FIND-TASK-001-12 — Distributed identity is still Variant-specific and prose-dependent

The public terminal, protobuf conversion, and shared client can carry a full
`WyrdProblem`, but the Analytical worker-to-coordinator boundary produces the
wrong input for them. `oracle/variant_sql.rs:635-695` wraps and decodes exactly
four Variant errors through serialized `Display` text.
`oracle/mod.rs:4240-4294` applies that decoder generally and recognizes sibling
catalog failures through message fragments. The pinned distributed transport
reduces `DataFusionError::External` to a string, so worker-side catalog errors
outside the four Variant variants have no general structured reconstruction.
The candidate's focused test even requires a serialized `QueryForbidden` to
become generic. A reachable worker footer refusal regains
`QueryTenantInvariant` only through the phrase `tenant invariant`.

Delete the Variant-only carrier/decoder and the catalog message heuristics.
Reuse the existing tagged `BifrostError` serde representation and one general
catalog-error envelope at the existing `DataFusionError::External`
worker boundary. Reconstruct that envelope before the existing
`map_datafusion_error` and `failed_terminal_on_path` flow. This keeps the
installed transport, terminal, catalog owner, and SDK projections; uncatalogued
external failures alone map to `QueryExecutionFailed`.

Do not repin or modify the dependency protocol, add a protobuf field or side
channel, maintain a closed error-code list, add a Variant branch, parse human
wording, or create a new public API, dependency, setting, compatibility path,
or test harness.

## Constraints and preserved behavior

- Preserve standard Iceberg v3 lineage only. Add no duplicate-ID scan or
  optional-metrics gate in production or tests.
- Keep `serde_json/arbitrary_precision` disabled and refuse integers outside
  `i64`/`u64`.
- Preserve Variant limits, fingerprints, admission ordering, built-in shapes,
  promotion semantics, sensitivity, tenant tripwires, audit hash inputs, and
  fixed trace identifiers.
- Preserve one shared Oracle session registration owner, Struct `get_field`,
  semantic Variant `variant_get`, and the existing terminal/protobuf/client
  full-problem path.
- Preserve the accepted Python/TypeScript Interactive-only journey limit;
  distributed proof remains in the Rust multi-pod journey through the shared
  client.
- Keep `wyrd-spec` IO-free and PyO3-free and keep DataFusion, Parquet, and
  Iceberg out of client-tier crates.
- Add no shredding, user-table authoring, migration, compatibility alias,
  second Variant model/reader, unrelated refactor, repository check, setting,
  option, or replacement lineage proof.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-4` | The active Bifrost authority states the exact `i64`/`u64` integer policy and the universal full-problem late-terminal/no-partial-result contract without a new artifact or mechanism. |
| `FIND-TASK-001-10` | Every listed cumulative changed declaration uses module-scope imports and bare names; the only retained local imports match the documented `Trait as _` exception. |
| `FIND-TASK-001-12` | A Variant catalog error and a non-Variant worker `BifrostError` round-trip through one general distributed envelope into identical pre-stream and late catalog problems; malformed or uncatalogued external text stays generic; no message parser or family-specific branch remains. |
| `FIND-TASK-001-12` | The existing Rust multi-pod journey produces at least one valid worker batch before a worker-side `QueryTenantInvariant`, receives the same complete problem as the pre-stream form, returns no partial collected result, and settles graph ownership; the unrelated late cast remains generic. |

## Focused proof and broader verification

Use the existing test owners and harnesses only:

1. Replace the Variant-only forwarded-text assertion with a general structured
   round-trip covering one Variant error, one non-Variant `BifrostError`,
   malformed or uncatalogued text, and nested DataFusion context. Run its exact
   `mise exec -- cargo nextest` selector.
2. Extend the existing Rust multi-pod late-failure journey with its established
   foreign- or missing-footer fixture. Prove a valid batch precedes
   `QueryTenantInvariant`, compare the late problem with the pre-stream problem,
   assert no partial result, and retain the generic cast case. Run its exact
   repository-managed Postgres selector.
3. Reinspect the complete cumulative Rust diff for ordinary function-local
   imports and qualified declaration types.
4. Run `mise run fmt`, `mise run lints`, `mise run docs:check`,
   `mise run check:docs`, the task's existing focused Oracle/tonic/client
   late-terminal tests, `mise run codegen:check`, and `git diff --check`.
5. Rerun the narrowest existing Bifrost journey lanes that cover the changed
   Oracle distributed path and shared client. Do not add or require a separate
   Python/TypeScript Analytical harness.

Route this task directly to `$wyrd-implement`.

## Implementation Evidence — 2026-10-06

All commands ran on the final candidate with
`CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd-bifrost-variant/target`.
The V1–V17 commands are exactly those in TASK-001 Verification.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-4: the active authority states the `i64`/`u64` integer policy and the universal full-problem late terminal with no partial result | `architecture/bifrost-design.md` (79f60eec3); no new artifact or mechanism | `mise run docs:check`, `mise run check:docs` | PASS |
| FIND-10: changed declarations use module-scope imports and bare names; only `Trait as _` local imports remain | Cumulative Rust diff from `80b33286e` reinspected. b0acb1901 names the built-in validator types (`CanonicalBatchValidator`, `RecordBatch`, `BifrostError`) through module imports. | The function-local-import scan over every changed line reports nothing. The qualified-type scan reports no qualified declaration type. Its remaining hits are expression paths (constants, enum values, constructors) and `fmt::Result`. `mise run lints` | PASS |
| FIND-12: Variant and non-Variant worker `BifrostError`s round-trip through one general envelope into identical pre-stream and late problems; malformed or uncatalogued text stays generic; no message parser or family branch | `oracle/mod.rs` catalog envelope at the `DataFusionError::External` boundary (929ce7771). Deleted: `VariantQueryError`, `catalog_query_error`, `is_tenant_refusal` | `oracle::tests::catalog_errors_keep_their_identity_locally_and_remotely`; `oracle::query_stream::tests::late_catalog_error_keeps_its_identity`; `oracle::tests::late_failure_terminal_is_closed_and_non_success`; `query_conversion::tests::failed_terminal_problem_round_trips`; `bifrost::query::tests::failed_terminal_problem_rebuilds_its_catalog_error` | PASS |
| FIND-12: the multi-pod journey's worker-side `QueryTenantInvariant` keeps the complete pre-stream problem, returns no partial result, and settles graph ownership; the late cast stays generic | `published.rs` `prove_worker_tenant_refusal` with `peer_cluster.rs` `seed_foreign_hot_row` (eaa8a5dbe) | V9 `published::variant_sql_registry_covers_every_session` | PASS (either surface, see below) |
| Repository lanes | — | `mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`, `mise run ts:typecheck`, V16 `mise run codegen:check`, `mise run docs:check`, `mise run check:docs`, V17 `git diff --check`, V1–V15 | PASS |

Stated limit (lead decision): the worker tenant refusal is accepted on either
the pre-stream or the late surface. The footer tenant check fails on first
poll, so deterministic late ordering would need a new hook, which is
forbidden. The late-surface identity of a worker `BifrostError` is pinned by
the remote arm of `late_catalog_error_keeps_its_identity`. The Variant late
path is proven end to end by `prove_late_failures` in the same journey.

### Duplication remediation (round-4 addendum)

`scratchpad/dedup-gate.sh` exits 0. Production code since 79f60eec3, tests
excluded, is net negative.

| Finding | Owner kept | Code deleted | Test |
|---|---|---|---|
| D0 (929ce7771) | `map_datafusion_error` plus the general catalog envelope | `VariantQueryError`, `catalog_query_error`, `is_tenant_refusal` | `oracle::tests::catalog_errors_keep_their_identity_locally_and_remotely` |
| D1 (126d214cd) | `wyrd_queue::schema` (`field_to_spec`, `spec_to_field`, `is_extension_key`) | Variant arms and key filters in `catalog/wire.rs` and `wyrd-server/src/bifrost/convert.rs`, plus redux `data_type_to_arrow`/`time_unit_*` | `schema::tests::arrow_schema_refuses_unrepresentable_types`; `fieldspec_to_arrow_*_round_trip` |
| D2 (a59101b34) | `wyrd_queue::variant::variant_field` via `VariantType`; one `is_variant` | hand-written `EXTENSION_TYPE_*` keys; `fields::variant_storage`, `fields::variant`, `mark_variant`, redux `is_variant` | V1 `tables::tests::variant_contract_and_builtin_schemas_are_stable` |
| D3 (53fe413ec) | upstream `VariantArray` | `binary_cell`, hand decoding in `variant_cell_to_json` | `variant::tests::json_writer_renders_variants_as_values` |
| D4 (95d070055) | `EncodedVariant::from_json_text` token walker | `append_json`, `append_items`, `number_variant` | `variant::tests::json_converts_under_the_variant_contract`; `json_text_classifies_integers_from_their_tokens` |
| D5 (0545aa97c) | `VariantColumnBuilder::encode` and `FromIterator` | five hand loops (batch builder, `results.rs` `variants`, audit projection, gateway capture, signal) | `variant::tests::column_encoding_names_the_refused_row`; audit projection and V2 journeys |
| D6 (4a9d52f8c) | `wyrd_tonic::error::wyrd_error_to_status` | the Gate's hand-built `WYRD_ERROR_HEADER` insert | `gate::error::tests::every_ingest_error_has_one_transport_projection` |
| D7 (dab737003, b0acb1901) | the `CanonicalBatchValidator` seam, now returning `BifrostError` | `validate_variants`, `variant_identity_matches`, the signal extension recheck | `scribe::execution_lanes::tests::canonical_validator_refusal_keeps_its_catalogued_code` |
| D8 (9a0dff4b0) | serialized `BifrostError` details; every producer sends them | `bifrost_error_from_code` and its two message parsers (about 300 lines) | `error::tests::bifrost_problems_reconstruct_their_exact_variant`; `grpc_convergence::*` built from the real producers |
| D9 (eae32051b) | `impl From<&WyrdQueueError> for WyrdError` | `queue_catalog_error` | `wyrd-client` and `wyrd-queue` lib tests |
| D10 (5a30d46f5) | `vala_sql::queries::audit_staging::entry_hash` (made `pub`) | `recomputed_entry_hash`, `push_text`, `push_optional`, `RetainedAuditRow` | V2 `verification_runtime::typed_builtin_payloads_are_queryable` |
| D11 (e92db1cb6) | Arrow JSON writer plus `VariantJsonEncoderFactory`; Oracle `to_json` | `cell_json`, `variant_texts` | V2 and all `eval_verification::*` journeys |
| D12 (fork 2b65fa1, repin 9a65d5469) | `iceberg::metadata_columns::get_metadata_field_id` | `row_lineage_field_id` | V13 `compaction::tests::rewrite_preserves_v3_row_lineage`; V10 |
| D13 | `VariantFailure::reason` | signal.rs `refusal`, deleted by D7 | covered by D7 |
| P1 (4fa671a65) | `mask_placeholders` validates each present cell with `Variant::try_new` | `root.value(row)` panic path | `variant::tests::malformed_stored_variant_is_an_error_not_a_panic`; `oracle::variant_sql::tests::malformed_stored_variant_fails_every_function_without_panicking` |

Behaviour changes accepted with D7:
- A canonical-table refusal keeps its catalog code instead of collapsing to
  `FingerprintMismatch`.
- An undeclared column is `UndeclaredField`.
- A signal storage-type mismatch is `UnsupportedType`.
- A non-catalogued signal or metric-kind reason is `SchemaParse`.

Defect found during D11 (6c518085b):
- **Symptom:** the journey's `drift_report` score `3.0` read back as `3`
  through Arrow's JSON writer.
- **Evidence:** `VariantJsonEncoderFactory` and the Oracle's
  `to_json`/`->>` rendered through upstream `to_json_string`, which writes
  doubles with `Display`. `variant_bytes_to_json`, used by Python and
  TypeScript, renders through `to_json_value`.
- **Cause:** two renderings, so a double became an integer on the Rust,
  MCP, CLI and SQL surfaces. This violates REQ-004.
- **Fix site:** all three sites render `to_json_value()?.to_string()`.
  `json_writer_renders_variants_as_values` now asserts `"score": 3.0`.

The eval journey's `context LIKE` lookup was refused at planning because
`context` is a Variant. It now reads `context ->> 'marker'`.

### Final verification on 44840ebb3

Every lane exited 0:
- **Formatting and lint lanes:** `mise run fmt`, `mise run lints`,
  `mise run py:format`, `mise run py:lints` and `mise run ts:typecheck`.
- **Task lanes:** V1–V15, V16 `mise run codegen:check`, `mise run docs:check`,
  `mise run check:docs`, V17 `git diff --check`, and `git diff --check 79f60eec3`.
- **Focused late-terminal tests:** the three Oracle tests, the tonic test and
  the client test named above, each run through an exact `cargo nextest`
  selector.
- **Touched-crate lanes:**
  - libraries: `wyrd-queue` lib; `vala-sql` audit_staging; `vala-bifrost-redux` lib (Postgres);
  - clients and server: `wyrd-client` lib; `wyrd-tonic` query_conversion; `wyrd-server` query, verification, gRPC and MCP lib (Postgres); `wyrd-server` gateway (Postgres);
  - journeys: all `eval_verification` and `verification_runtime` journeys;
  - forks: the fork V12/V13.

On the first pass, v16, docs:check, check:docs and V17 failed with
`No space left on device`. The `wyrd-server` lib lane also failed: it ran
without the Postgres wrapper, so `PgFixture` panicked. All five were rerun
after freeing space, with the wrapper, and passed.
