# TASK-001 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Prior verdict: `changes/active/bifrost-variant/review/TASK-001-r1/verdict.md`
- Remediation task: `changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`

The candidate remained the checked-out `HEAD` while this report was prepared.
The cumulative base-to-candidate diff, not only the remediation commits, was
reviewed. CodeGraph was unavailable because this repository has no
`.codegraph/` index, so repository-native source search and direct caller
inspection were used.

## Producer-to-sink invariant traces

### Exact Variant values and stable failures

`EncodedVariant::from_json_text` first validates the complete input as
`serde_json::value::RawValue`, recursively retains raw child tokens, and routes
integer lexical forms through `i128` and then the one `integer_variant` rule
(`crates/shared/wyrd-queue/src/variant.rs:157-163,592-695`). It therefore does
not enable workspace-wide `serde_json/arbitrary_precision`, and integral text
does not pass through `f64`. `parse_json` calls this same owner and wraps its
typed `BifrostError` in `VariantQueryError`
(`crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:624-695`). Local
source-chain recovery and distributed tagged-serde recovery converge in
`variant_query_error` (`oracle/mod.rs:4270-4295`); malformed and unrelated text
remain generic. The exact-number unit tests and the real interactive and
analytical journey named in the remediation evidence cover the producer and
both query sinks.

### Built-in admission, WAL, and acknowledgement

Ingress resolves every built-in to its `BuiltinTableDefinition` before decode
(`scribe/ingress.rs:507-588`). `decode_rows` verifies the source fingerprint,
then invokes `DomainDefinition::validate_variants` before correlation stamping
and before the decoded batch can reach WAL admission
(`scribe/execution_lanes.rs:533-580`). The validator first checks extension
identity across all declared Variant-bearing fields, then walks rows and fields
in order, including Struct and List descendants, and delegates size, encoding,
and depth to `EncodedVariant::from_bytes`
(`tables/mod.rs:200-344`). `ScribeError::ContractViolation` retains the typed
`BifrostError`; Gate maps it without flattening, and the gRPC response carries
the full problem document. Thus malformed or mismarked built-in Variant data
is refused before ACK/WAL while genuine fingerprint mismatches remain the
existing schema refusal.

### Iceberg v3 lineage

The production path now follows the approved standard boundary: the pinned
compaction fork projects the two reserved fields by field ID, validates their
presence, `Int64` type, and non-nullness per output batch, and writes the batch
unchanged. Forge no longer requires optional `DataFile` metric entries before
publication. No rewrite-wide duplicate-ID collection remains in production.

The required proof, however, still goes beyond that boundary. Wyrd's journey
scans every live row into a map keyed by `_row_id` and fails on any duplicate
(`crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs:1158-1226`),
then expressly cites that uniqueness check as proof (`:1232-1262`). The pinned
fork's `rewrite_preserves_v3_row_lineage` does the same whole-table duplicate
scan (`/home/thorrester/Documents/GitHub/iceberg-compaction/core/src/compaction/mod.rs:3289-3350`)
and additionally requires optional `DataFile.value_counts` and
`null_value_counts` entries for both lineage fields (`:3401-3411`). These are
the duplicate-ID scan and metrics check the binding human decision rejects.
They are not necessary to prove unchanged lineage: the logical row identity
already present in each fixture can be mapped to its two observed lineage
values before and after each rewrite.

### Built-in schemas, Oracle registration, Bloom sizing, and consumers

The cumulative source and the recorded V1-V17 evidence close the remaining
task invariants: built-in producers use the declared Variant/Struct shapes;
all Oracle construction sites call the shared `OracleVariantSql` owner before
planning or execution; sensitivity remains rooted at the logical column;
typed and JSON terminals share the Variant decoder/encoder; and both writer
recipes leave Bloom NDV unset so parquet-rs derives it from row-group geometry.
The active architecture and schema guide describe the resulting v3, Variant,
SQL, and built-in layouts.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001, REQ-002, INV-003, INV-006, AC-002: v3-only tables, standard hidden-lineage projection/copy, repeated rewrites, and v3 GC | Catalog/Forge cumulative diff; pinned compaction fork `iceberg_file_task_scan.rs` and `executor/datafusion/mod.rs`; Forge publication no longer inspects optional lineage metrics | V10 and V13 are recorded PASS, but their proof still performs rejected duplicate-ID scans and V13 still asserts optional lineage metric maps | **FAIL** — `INV-R2-001` |
| REQ-003, REQ-004, REQ-019, INV-002: one Variant type, exact integer meaning, limits, and stable failures | `wyrd-queue/src/variant.rs:157-180,592-695`; fingerprint/wire/schema owners in the cumulative diff | Exact token-classification tests, Variant contract tests, `parse_json` exactness test, and query journeys recorded PASS | PASS |
| REQ-006 through REQ-011, INV-001, INV-004, INV-005, AC-001, AC-003, AC-005: built-in shapes, all producers/consumers, promotions, sensitivity, and no legacy stored forms | Built-in ledgers/projections and server/client producers in the cumulative diff; `DomainDefinition::validate_variants` reaches all built-ins before WAL | V1-V8, V14-V15, the new raw-Arrow built-in refusal journey, and OTLP journeys recorded PASS | PASS |
| REQ-017, REQ-019, INV-004, INV-006, AC-005, AC-008: one Oracle SQL surface in every session and stable local/distributed errors | `OracleVariantSql::shared` at every production session owner; `VariantQueryError` tagged-serde carrier and coordinator recovery | Oracle unit tests and `published::variant_sql_registry_covers_every_session` recorded PASS for interactive and analytical paths | PASS |
| REQ-005 and AC-009: native row-group Bloom sizing with unchanged FPP/folding | Explicit Wyrd NDV setter/constant removed; parquet-rs owns unset-NDV derivation | `bloom_capacity_uses_row_group_limit_for_scribe_and_forge` recorded PASS | PASS |
| Public Rust/Python/TypeScript/MCP/HTTP contracts, generated artifacts, and documentation remain aligned | Query terminals, TS declarations/JSDoc, generated schemas, `architecture/bifrost-design.md`, and `docs/.../schema.svx` in cumulative diff | Rust/Python/TypeScript/MCP journeys, `codegen:check`, `ts:typecheck`, `docs:check`, and `check:docs` recorded PASS | PASS |
| Prohibited scope: no second Variant model/reader, DataFusion repin, signing, migration, compatibility alias, TASK-002 implementation, or new check/setting | Dependency and source diff; approved `RawValue` local classification; native parquet Bloom behavior | Manifest/boundary checks and cumulative diff inspection | PASS except for the nonstandard lineage checks captured by `INV-R2-001` |

## Proposed findings

### INV-R2-001 — Lineage verification retains the rejected duplicate-ID and metrics checks

- **Classification:** DRIFT
- **Prior finding:** `FIND-TASK-001-3` is not fully closed.
- **Violated obligation:** The binding human decision requires Iceberg row
  lineage to rely solely on standard Iceberg v3 field-ID projection,
  per-batch presence/type/null validation, and unchanged copy, with no
  duplicate-ID scan or metrics gate. The standing direction requires any
  mechanism or check absent the established standard and comparable projects
  to be classified as drift and forbids reproducing it in remediation.
- **Exact locations:**
  - `crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs:1158-1226`
    scans the live table keyed by `_row_id` and asserts no duplicate.
  - The same file at `:1232-1262` names that uniqueness scan as part of the
    proof.
  - Pinned fork revision `fb3a594b0a93d9f99b62a77084e94be96fb7fba7`,
    `/home/thorrester/Documents/GitHub/iceberg-compaction/core/src/compaction/mod.rs:3289-3350`,
    repeats the whole-table duplicate-ID scan.
  - The pinned fork test at `:3401-3411` requires optional lineage value/null
    metric entries, the same nonstandard metric prerequisite removed from
    Forge publication.
- **Evidence:** Production no longer needs either check: field-ID projection
  supplies the reserved columns, `validate_row_lineage` enforces presence,
  type, and nullness per batch, and the writer copies the validated batches.
  The fixtures already carry stable logical row keys, so before/after equality
  of each row's `_row_id` and `_last_updated_sequence_number` can be observed
  without proving global `_row_id` uniqueness or inspecting optional file
  metrics.
- **Observable consequence:** A standards-valid implementation can fail the
  task's required V10/V13 proof solely because an optional metrics map is
  absent or because the test imposes a global duplicate invariant outside the
  approved Iceberg v3 handling. The recorded PASS therefore does not close the
  human-approved remediation boundary.
- **Required testable correction:** Delete the duplicate-ID assertions and
  optional lineage-metrics assertions from the Wyrd journey and pinned fork
  test, then repin the fork. Retain the existing per-batch production
  validation and prove repeated-rewrite preservation by comparing each
  fixture row's stable logical identity to its `_row_id` and
  `_last_updated_sequence_number` before and after rewrites. Add no replacement
  scan, metric requirement, setting, option, or repository check. Rerun V10
  and V13.

## Prior-finding closure

| Prior finding | Invariant disposition |
|---|---|
| `FIND-TASK-001-1` | Closed by the explicitly approved local `RawValue` plus `i128` lexical classification. Workspace-wide `arbitrary_precision` remains absent. |
| `FIND-TASK-001-2` | Closed: every built-in definition reaches recursive Variant validation before WAL/ACK and typed failures survive Gate/gRPC. |
| `FIND-TASK-001-3` | **Not closed:** production gates were removed, but the required Wyrd and pinned-fork proofs retain the rejected duplicate-ID scan, and the fork proof retains the optional metrics check. |
| `FIND-TASK-001-4` | Closed by the updated active architecture and schema guide. |
| `FIND-TASK-001-5` | Closed by tagged-serde transport through the existing string carrier. |
| `FIND-TASK-001-6` | Closed: construction is validated and the dead emptiness API is absent. |
| `FIND-TASK-001-7` | Closed: `QueryResult` owns its JSDoc. |
| `FIND-TASK-001-8` | Closed: native parquet-rs NDV derivation is reused. |
| `FIND-TASK-001-9` | Closed by cumulative changed-item documentation and recorded lint evidence. |
| `FIND-TASK-001-10` | Closed by import/signature cleanup and recorded lint evidence. |

## Verification assessment

The remediation record reports the exact original V1-V17 proofs plus format,
lint, documentation, codegen, typecheck, and boundary lanes as passing. That
evidence is credible for the passing rows above. It cannot convert the
explicitly rejected assertions inside V10/V13 into acceptable proof; those
assertions are the remaining defect, not a verification limit.

## Result

**FAIL**

One bounded drift finding remains: `INV-R2-001`.
