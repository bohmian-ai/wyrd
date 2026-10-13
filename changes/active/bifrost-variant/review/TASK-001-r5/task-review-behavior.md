# TASK-001 r5 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Original base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Remediation range: `bb6ae8070e276c20011e675ba1a804f0356e51ea..0e37748f3a27d3bcec4713e6210e97328e045886`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 12
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The cumulative base-to-candidate diff and the r4 remediation range were
reviewed. `HEAD` and its tree matched the pinned candidate at the beginning and
end of this review. The reviewed source was not modified.

## Navigation and caller-to-result paths

| Path | Source trace | User-visible result |
|---|---|---|
| JSON row / SQL JSON parser | `BatchBuilder::push_json` -> `build_variant_column` -> `EncodedVariant::from_json_text`; and `parse_json` -> `EncodedVariant::from_json_text` | Stable Variant refusal or SQL query error |
| Raw Arrow built-in admission | Gate/Scribe decode -> table-owned `validate_predeclared` / `validate_declared_variants` -> `validate_variant_values` -> `EncodedVariant::from_bytes` | Pre-ACK stable Variant refusal, with no durable row |
| Verification summaries | verification producer -> nullable Struct children -> fixed IPC/material preflight -> hot/published Oracle `get_field` | Absent report children remain SQL null |
| Query rendering | Oracle Variant UDFs -> Arrow Variant result -> shared client/HTTP/MCP/CLI renderers | Native JSON values with exact integer and double spelling |
| V3 maintenance | catalog v3 creation -> Scribe publication -> Forge rewrite -> pinned Iceberg metadata-column owner | Hidden lineage survives repeated compaction |

## Obligation matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001, REQ-002, INV-003, INV-006, AC-002: v3-only tables, hidden lineage preservation, repeated Forge rewrites, and v3 GC | Cumulative catalog/Forge changes retain the standard metadata-field projection and the final `iceberg-compaction` pin `2b65fa189f2d05002acc6e59515a071a63777970`; no public lineage column or changed five-field handoff is present. | Original V10, final-pin V12/V13, and prior independent Iceberg reviews record passing proof. No contradictory source path was found. | PASS |
| REQ-003: one canonical Variant type, extension, fingerprint tag, and wire/SDK projection | `wyrd_queue::variant::{variant_field,is_variant}`, canonical schema/fingerprint changes, `DataTypeSpec::Variant`, generated schemas, and Rust/Python/TypeScript declarations use `arrow.parquet.variant`; exact empty/absent metadata is enforced at `variant.rs:439-442`. | `variant_contract_and_builtin_schemas_are_stable`, `variant_extension_requires_empty_metadata`, codegen evidence, and raw top-level/nested admission cases are present. | PASS |
| REQ-004, REQ-019, INV-002, INV-007: exact JSON numeric meaning, bounded depth/size, and locked stable failure precedence | Exact `i64`/`u64` token classification and bounded raw-byte depth preflight exist in `variant.rs`. Compound failures still violate the locked precedence in both JSON and raw-byte paths. | Existing tests cover sibling numeric/depth defects and isolated malformed/deep values, but not a numeric defect below the first over-depth container or malformed bytes after an over-depth branch. | **FAIL — `BVR-R5-BEH-001`, `BVR-R5-BEH-002`** |
| REQ-005, AC-009: Bloom capacity follows row-group geometry with native folding/FPP | Wyrd's duplicate NDV owner is absent; Scribe and Forge recipes use the Parquet row-group geometry. | Focused Bloom test and prior review evidence pass. | PASS |
| REQ-006–REQ-008, REQ-011, INV-001, INV-005, AC-001: signal Variants, entity Structs, promotions, and OTLP/canonical equivalence | Signal schemas and projections use Variant for open values, typed Structs for fixed values, and the declared promoted columns. Metrics bucket child nullability now prevents published padded-child exposure without altering the signal Variant contract. | Rust OTLP journeys cover spans/logs/metrics and the metrics published bucket regression; Rust canonical Arrow and Python/TypeScript OTLP-to-query journeys are recorded passing. | PASS |
| REQ-009, AC-003: verification result Structs have exact nullable-child semantics | `ResultsTable::{drift_report_fields,eval_summary_fields}` declares nullable children; the server producer writes null child slots for absent reports; fixed IPC accepts only Arrow-valid parent-masked nulls. | `typed_builtin_payloads_are_queryable` checks all children hot and published; producer and fixed-IPC focused tests cover absent and present rows. | PASS |
| REQ-010, INV-001: every listed JSON-text payload is Variant with producer/consumer closure | The cumulative table ledgers and named producers use Variant for eval, gateway, agent trace, and audit payloads; gateway `resolved_model` nullable children also avoid a separate published Struct leak. | Rust/Python/TypeScript/MCP built-in journeys and gateway/audit focused tests are recorded passing. | PASS |
| REQ-017: one Oracle registration owner; `variant_get`, `->>`, parse/try/to JSON; Struct remains `get_field` | `OracleVariantSql` owns the five functions and planner; production constructors install it; literal chaining, per-row paths, canonical storage, string-vs-JSON text, and Struct separation are implemented in `oracle/variant_sql.rs`. | `variant_operators_and_functions_follow_the_contract` and `variant_sql_registry_covers_every_session` cover local/analytical sessions and plan shape. The `parse_json` compound-precedence gap is captured by `BVR-R5-BEH-001`. | **FAIL — `BVR-R5-BEH-001`** |
| REQ-019 late failures: identical full catalog problem before/after rows, interactive/distributed, no partial collection | The terminal carries `WyrdProblem`; tonic, server, shared client, and the general distributed `QueryCatalogError` envelope reuse the catalog instead of parsing prose. | Focused terminal/conversion/client tests and Rust multi-pod late-failure journey are recorded passing; Python/TypeScript exercise the shared interactive projection. | PASS |
| INV-004, AC-008: sensitive logical columns stay gated through Variant/Struct expressions | Authorization is resolved on logical source columns before provider/source IO; expression lowering does not introduce an alternate leaf permission path. | Existing sensitive whole-column/operator/Struct journey and prior security validation found no reachable bypass. | PASS |
| AC-005: stable refusals and no admission on refused writes | Undeclared/schema/extension/isolated Variant refusals occur before ACK/WAL. The two compound Variant cases return the wrong stable code, so the acceptance obligation is not fully met. | `builtin_variant_columns_are_refused_before_ack` covers the repaired ordinary cases and post-refusal availability, but omits the two compound states below. | **FAIL — `BVR-R5-BEH-001`, `BVR-R5-BEH-002`** |
| CLI/MCP/HTTP and first-class SDK rendering parity | Shared rendering uses native Variant JSON; the compiled CLI journey asserts the physical Struct is hidden, `u64::MAX` remains exact, and `3.0` remains a double. | `query_server_journey::query_command_reads_seeded_table` plus Rust/Python/TypeScript/MCP journeys are present. | PASS |
| Non-goals and task boundary | No shredding, DataFusion repin, user-model inference, migration, compatibility alias, second reader/model, signing, public configuration, or new wire side channel entered TASK-001. | Cumulative diff and remediation diff inspection. | PASS |

## Proposed findings

### BVR-R5-BEH-001 — INCORRECT — JSON numeric precedence stops at the depth boundary

- **Violated obligation:** Spec revision 12 locked contract at
  `changes/active/bifrost-variant/spec.md:221` requires numeric range to be
  selected before depth; REQ-004, REQ-017, REQ-019, INV-002, and AC-005 require
  the same stable outcome through writes and `parse_json`.
- **Location:** `crates/shared/wyrd-queue/src/variant.rs:628-634`.
- **Evidence:** `append_raw` records `TooDeep` and returns immediately when it
  encounters the container at depth 65. It therefore never visits a numeric
  token inside that rejected container. For example, a Variant whose only
  branch is 65 nested arrays containing `18446744073709551616` returns
  `VariantTooDeep`, even though the locked ordering requires
  `VariantNumericOutOfRange`. The r4 regression at `variant.rs:921` places the
  numeric defect in a separate shallow sibling, so it cannot exercise this
  path. Both the public JSON-row builder (`batch_builder.rs:378`) and Oracle
  `parse_json` (`oracle/variant_sql.rs:590`) call this owner, making the path
  reachable before ACK and during queries.
- **Observable consequence:** The same two invalid properties produce different
  stable catalog codes depending on whether the out-of-range token is a sibling
  of, or nested inside, the over-depth branch. Callers branching on the
  catalogued first failure see `WYRD_VALA_400_VARIANT_TOO_DEEP` where the
  approved contract promises `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`.
- **Relationship to prior findings:** This is an uncovered boundary of
  `FIND-TASK-001-18`, not a new product decision. The r4 correction made sibling
  ordering deterministic but explicitly skips everything below the depth
  boundary.
- **Required testable correction:** Keep `EncodedVariant::from_json_text` as the
  sole owner, but continue a bounded, non-building inspection of the rejected
  subtree sufficient to detect a higher-priority numeric violation before
  returning the retained depth error. Add focused direct, `parse_json`, and
  pre-ACK JSON-row cases with the out-of-range integer *inside* the depth-65
  branch, and assert numeric range plus unchanged server availability/no row.

### BVR-R5-BEH-002 — INCORRECT — raw depth preflight can outrank malformed encoding

- **Violated obligation:** Spec revision 12 at
  `changes/active/bifrost-variant/spec.md:221-225` and the active Bifrost
  authority at `architecture/bifrost-design.md:158-164` require size, encoding
  validity, then depth for each declared built-in Variant value.
- **Location:** `crates/shared/wyrd-queue/src/variant.rs:184-195` and
  `crates/shared/wyrd-queue/src/variant.rs:766-789`.
- **Evidence:** `EncodedVariant::from_bytes` runs `check_depth` before
  `Variant::try_new`. `check_depth` returns immediately at the first depth-65
  container, so full validation never runs. A raw Variant with an over-depth
  first branch and malformed bytes in a later branch is reported as
  `TooDeep`; moving the malformed branch before the deep branch can instead
  panic inside the shallow accessor and be mapped to `InvalidJson`. The r4
  test at `variant.rs:941` covers a valid 20,000-level value and a separate
  malformed value, not their compound ordering. Every predeclared built-in
  reaches this owner through `tables/mod.rs:348-372` before ACK/WAL.
- **Observable consequence:** A raw Arrow writer can make the public failure
  depend on physical child order and receive
  `WYRD_VALA_400_VARIANT_TOO_DEEP` for bytes that should first be classified as
  `WYRD_VALA_400_VARIANT_INVALID_JSON`, contrary to the stable trust-boundary
  contract.
- **Relationship to prior findings:** This is the compound-error boundary left
  open by `FIND-TASK-001-16`. The bounded preflight prevents stack exhaustion,
  but it currently becomes the final answer before the higher-priority validity
  classification is established.
- **Required testable correction:** Preserve the bounded preflight and upstream
  `Variant::try_new` as the full validity authority, but make the preflight
  establish/retain malformed evidence over the safely inspected structure
  before selecting depth; it must not recurse beyond the Wyrd bound or add a
  second Variant parser. Add a focused raw-byte test and the existing raw-IPC
  admission journey with malformed/deep sibling order reversed, asserting
  `VariantInvalidJson`, no ACK/durable row, and subsequent valid admission.

## Verification notes

- Source inspection used CodeGraph first, then the cumulative Git diff,
  remediation diff, owning modules, callers, and existing tests.
- `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..0e37748f3a27d3bcec4713e6210e97328e045886`
  passed.
- The r4 implementation record reports the focused unit/journey commands,
  V1–V17 affected proofs, format, lints, codegen, skill sync, and diff check as
  passing. This reviewer did not rerun the full environment-backed suite.
- The recorded green tests do not exercise either retained compound state;
  both findings are source-confirmed at shared owners reached by public write
  and query paths.

## Overall result

**FAIL**

TASK-001 still violates the approved stable error ordering for two reachable
compound Variant inputs. The corrections are bounded within the existing
`EncodedVariant` owner and require no specification revision.
