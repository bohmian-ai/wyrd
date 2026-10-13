---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-bifrost-variant
spec_revision: 10
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-017, REQ-019, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, AC-001, AC-002, AC-003, AC-005, AC-008, AC-009]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8, FIND-TASK-001-9, FIND-TASK-001-10]
---

# Close TASK-001 Variant contract gaps and remove duplicate mechanisms

## Authority and immutable review subject

- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Reviewed base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Reviewed candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Validated ledger: `changes/active/bifrost-variant/review/TASK-001-r1/findings-validation.md`

Implement this task with `$wyrd-implement`. Reassess the full original
base-to-remediated-candidate range afterward; do not review only the remediation
diff.

## Outcome

TASK-001 satisfies the approved Variant, built-in storage, Oracle, Iceberg v3,
lineage, Bloom, documentation, and repository-shape contracts without bespoke
verification mechanisms. Large JSON integers retain exact meaning, every
TASK-001 built-in enforces Variant integrity before acknowledgement, remote
errors retain stable identity without parsing prose, lineage and Bloom behavior
use their established native mechanisms, public documentation matches the
shipped contract, and the cumulative Rust/TypeScript changes meet repository
rules.

## Issue diagnoses and required corrections

### FIND-TASK-001-1 — Exact JSON integer classification

`EncodedVariant::from_json_text` parses through the default
`serde_json::Value` representation. Integral tokens beyond `u64` can therefore
become `f64` before `number_variant` classifies them. This violates REQ-004,
REQ-019, and INV-002 because a supported scale-zero Decimal16 can lose its type
or value and an unsupported integer can be acknowledged instead of returning
`WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`.

Correct the shared `EncodedVariant` owner, not its consumers. Use serde_json's
installed arbitrary-precision token preservation and classify integral lexical
forms before floating-point conversion: values fitting `i64` remain integers,
other exact integral values fitting Decimal16 become scale-zero Decimal16, and
larger values return the existing typed range error with the required path.
Keep one numeric classification path; add no parser, dependency, numeric model,
option, or downstream guard.

### FIND-TASK-001-2 — Built-in Variant validation before admission

The common built-in admission path accepts declarations whose Arrow data type
matches while `DomainTable::CANONICAL_VALIDATOR` is absent for verification,
evaluation, gateway, agent-trace, and audit tables. Extension metadata is not
part of the fingerprint, nested Variant identity is not generally checked, and
validator errors currently collapse to schema mismatch. A raw Arrow IPC caller
can therefore bypass the server-side repetition of the Variant contract or
receive the wrong stable error.

At the common decoded built-in admission boundary, reuse
`EncodedVariant::from_bytes` from one schema-driven recursive validation pass
before admission or WAL mutation. Validate the `arrow.parquet.variant`
extension identity and bytes for top-level and nested TASK-001 built-in fields.
Preserve typed `BifrostError` identity and precedence through Scribe and Gate;
leave genuine schema mismatches as `SchemaMismatch`. Do not add per-table
validators, another decoder/error model, or TASK-002 dynamic-table behavior.

### FIND-TASK-001-3 — Standard Iceberg row-lineage preservation

Forge currently refuses output when optional Iceberg `DataFile` metric maps do
not contain both hidden lineage fields, although Iceberg treats missing metric
entries as unknown and the counts cannot prove identity preservation. The
pinned compaction fork also retains every row ID, globally sorts it, and checks
uniqueness after already validating and copying the hidden fields. These are
nonstandard duplicate checks with false-refusal and rewrite-size memory costs.

Delete the publication-time optional-metric prerequisite and the fork's
rewrite-wide row-ID collection, merge, sort, and duplicate scan. Retain the
standard field-ID projection, bounded input-batch missing/type/null validation,
unchanged write of both hidden fields, and the repeated-rewrite before/after
equality journey. Add no replacement metadata, check, setting, or option.

### FIND-TASK-001-4 — Architecture and supported schema documentation

The implementation creates only Iceberg v3 tables, preserves hidden lineage,
stores open built-in values as Variant, changes persisted Struct layouts, and
adds Variant SQL functions. `architecture/bifrost-design.md` does not yet state
those contracts, while `docs/src/content/docs/bifrost/schema.svx` still lists
removed Binary/Utf8/List<Binary>/`details` shapes and omits Variant.

Update those two existing documents in place from the owning declarations.
Cover Iceberg v3 and hidden lineage, Variant representation and final-duplicate
key behavior, the shipped `->`/`->>` and JSON function surface, the actual
promoted built-in shapes, and removal of `details`. Do not add a generator,
documentation checker, file, setting, or speculative example; discovery found
no concrete stale example.

### FIND-TASK-001-5 — Structured identity over the existing distributed carrier

Remote Oracle execution reduces external errors to a string. The candidate
recovers Variant error identity by parsing human `Display` fragments, creating
a second undocumented grammar that breaks when wording or punctuation changes.
Simply deleting recovery would also violate the exact stable-error contract.

At the Variant external-error producer, serialize the existing tagged
`BifrostError` into the dependency's existing string carrier while retaining
the typed local source. At the coordinator, recognize and deserialize only
that structured representation; unrelated or malformed strings remain generic.
Delete the prose parser. Do not repin or modify the dependency protocol, add a
protobuf/error enum, or introduce another public format, option, or setting.

### FIND-TASK-001-6 — `EncodedVariant` invariant surface

Public `EncodedVariant::sized` validates only byte count and can construct
empty or malformed data even though the type promises a storable Variant.
Public `is_empty` advertises that impossible state. Neither has an external
caller; validated construction already goes through `from_json`,
`from_json_text`, or `from_bytes`.

Make the size-only construction step private and delete `is_empty`. Keep the
three validated public constructors. Add no replacement abstraction, flag,
alias, or compatibility surface.

### FIND-TASK-001-7 — `QueryResult` public documentation

The existing TypeScript `QueryResult` JSDoc now attaches to the inserted
`VARIANT_EXTENSION` constant, leaving the exported class undocumented.

Move the existing block immediately above `QueryResult`. Do not hand-edit
generated declarations or add a new documentation mechanism.

### FIND-TASK-001-8 — Native Parquet Bloom NDV derivation

Wyrd copies `DEFAULT_MAX_ROW_GROUP_ROW_COUNT` into `BLOOM_NDV` and explicitly
sets each Bloom column's maximum NDV. parquet-rs 59.3 already derives an unset
Bloom NDV from the writer's maximum row-group row count, so the duplicate can
drift when geometry changes.

Delete the local constant/import and explicit NDV setters. Retain Bloom
enablement, the false-positive probability, row-group geometry, and the
focused resolved-properties assertion. Add no wrapper or setting.

### FIND-TASK-001-9 — Mandatory Rust documentation

The cumulative Rust diff contains added and materially changed items without
the substantive rustdoc required by `AGENTS.md` and `architecture/agent-rules.md`.
Confirmed examples include new `ScalarUDFImpl` methods in
`oracle/variant_sql.rs` and a panicking client error test, but the obligation
applies to every changed Rust item rather than only those examples.

Audit the cumulative base-to-remediated-candidate Rust diff item by item and
add concise owner-local rustdoc describing intent, workflow role, invariants,
and side effects. Add `# Errors`, `# Panics`, and async cancellation or partial
progress sections wherever the rules require them. Do not add a checker, lint
exception, generated file, or allow attribute.

### FIND-TASK-001-10 — Module imports and bare signature types

The cumulative Rust diff includes ordinary function-local imports and fully
qualified type/bound names in signatures. This violates the mandatory module
style and scatters the dependency surface.

Move ordinary imports to the relevant module top, including the enclosing test
module where applicable, and import signature types and bounds under bare
names. Preserve only the documented `Trait as _` exception. Do not add an
import checker or allow rule.

## Constraints and preserved behavior

- Preserve the approved revision-10 Variant limits, fingerprint tag, error
  codes/details/precedence, persisted Struct layouts, promotion semantics,
  sensitivity, and final-duplicate OTel key rule.
- Preserve one shared Variant model and decoder based on the installed Arrow
  59.3 Variant crates.
- Preserve Struct `get_field`, semantic Variant `variant_get`, full-root and
  residual correctness, and registration in every production Oracle session.
- Preserve permission evaluation before provider IO, tenant tripwires, audit
  hash inputs, fixed trace identifiers, and language-native result decoding.
- Preserve Iceberg v3-only creation, exact hidden-lineage copy, the five-field
  Forge handoff, existing attempt/recovery identity, and v3 GC.
- Preserve row-group geometry, Bloom enablement/FPP/folding, and both writer
  recipes.
- Keep `wyrd-spec` IO-free and PyO3-free; keep DataFusion, Parquet, and Iceberg
  out of client-tier crates.
- Do not weaken validation, errors, tenancy, security, durability, or any
  existing task-local journey to reduce code.

## Non-goals

- No shredding policy, user-model inference, second reader, second Variant
  model, DataFusion repin, `datafusion-variant`, signing, migration, or
  compatibility alias.
- No TASK-002 dynamic-table, Python, or TypeScript Variant authoring work.
- No new public API, wire field, error catalog, protocol, dependency, feature,
  configuration setting, validation framework, test harness, repository check,
  documentation generator, or example.
- No unrelated refactor of Oracle, Scribe, Forge, SDK, or documentation owners.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-1` | Large positive and negative integral JSON tokens round-trip exactly as integer/Decimal16 when supported and return the exact range error when unsupported; no integral token passes through `f64`. |
| `FIND-TASK-001-2` | Every TASK-001 built-in rejects missing/wrong Variant extension metadata and malformed, too-deep, or too-large top-level/nested Variant data before ACK/WAL; typed Variant errors retain exact code/details and genuine schema mismatches remain distinct. |
| `FIND-TASK-001-3` | Repeated Forge rewrites preserve both hidden lineage values and publish successfully when optional lineage metric entries are absent, without a rewrite-wide row-ID collection/sort. |
| `FIND-TASK-001-4` | The active Bifrost authority and existing schema guide match the shipped v3, Variant, query, duplicate-key, built-in Struct/promotion, and removed-field behavior. |
| `FIND-TASK-001-5` | Interactive and distributed Variant failures return identical stable code/details independent of human `Display`; unrelated/malformed remote strings stay generic. |
| `FIND-TASK-001-6` | External callers can construct `EncodedVariant` only through validated constructors; the dead emptiness surface is absent. |
| `FIND-TASK-001-7` | Public/editor documentation attaches the existing `QueryResult` description to the exported class. |
| `FIND-TASK-001-8` | Both writer recipes resolve Bloom NDV from native parquet-rs row-group geometry and retain the approved FPP/folding behavior without a Wyrd NDV duplicate. |
| `FIND-TASK-001-9` | Every added or materially changed Rust item in the cumulative diff satisfies the repository rustdoc contract. |
| `FIND-TASK-001-10` | The cumulative Rust diff contains no disallowed function-local import or qualified signature/bound name. |

## Focused proof

Use the repository-pinned toolchain and keep new behavior tests in existing
owners. Required proof is:

1. Extend the existing shared Variant contract test to cover `u64::MAX + 1`,
   a negative integer below `i64::MIN`, the supported Decimal16 boundary, and
   the first out-of-range integer; run its exact `mise exec -- cargo nextest`
   selector.
2. Add one existing-target server journey that sends raw Arrow IPC to a
   non-signal built-in and a nested signal Variant, proving wrong/missing
   extension metadata, invalid bytes, depth, size, error precedence, no ACK,
   and no persisted row. Record and run its exact selector through the
   repository Postgres wrapper.
3. Run the existing Oracle Variant SQL contract test and
   `published::variant_sql_registry_covers_every_session`, including exact
   local/remote structured error identity plus an unrelated malformed remote
   error case.
4. Run
   `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite` and the
   pinned compaction lineage test after adding the standards-valid absent-metric
   case.
5. Run
   `parquet::writer_properties::tests::bloom_capacity_uses_row_group_limit_for_scribe_and_forge`.
6. Run the owning shared-client and TypeScript checks after API/JSDoc cleanup.
7. Run `mise run docs:check` and `mise run check:docs`.
8. Reinspect the cumulative Rust diff for documentation/import compliance;
   use the existing format and lint lanes, not a new checker.

Every specifically named new test must be recorded with its exact target and
`mise exec -- cargo nextest run --locked ... -E 'test(=...)'` selector after
the implementer places it in an existing owner.

## Broader verification

Rerun the original task's V1–V17 task-local proofs on the remediated cumulative
candidate, including the exact Rust, Python, TypeScript, MCP, Oracle, Forge,
fork, codegen, and diff-check commands. Also run:

- `mise run fmt`
- `mise run lints`
- `mise run py:format`
- `mise run py:lints`
- `mise run ts:typecheck`
- `mise run docs:check`
- `mise run check:docs`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- `mise run check:unwrap-audit`

Do not run `mise run verify:bifrost` or `mise run gate`; the original task and
these remediation-specific lanes are the complete scoped proof.

## Implementation evidence

Command prefix for every cargo/mise command: `CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd-bifrost-variant/target`. PG journeys use `scripts/postgres/with-test-postgres.sh` with the migrate step from the original V-commands.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-001-1` | 45b61e80c: `wyrd-queue/src/variant.rs` `EncodedVariant::from_json_text` validates once as `RawValue` and classifies number tokens lexically (`raw_number_variant` parses integral tokens as `i128`); `integer_variant` is the one integer rule for both JSON paths (i64 narrowed, else Decimal16 scale 0, else `NumericOutOfRange{numeric_kind:"integer"}`); a fraction or exponent token is a finite `f64`; objects split into a `BTreeMap`, so the last duplicate key wins. Declared the already-unified `raw_value` feature on wyrd-queue's serde_json, following the wyrd-client precedent | `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::json_text_classifies_integers_from_their_tokens) \| test(=variant::tests::json_converts_under_the_variant_contract)'`; `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=oracle::variant_sql::tests::parse_json_keeps_exact_integers_and_refuses_the_rest)'` | PASS |
| `FIND-TASK-001-2` | 347747ee3: `DomainDefinition::validate_variants` runs in Scribe `decode_rows` before ACK and WAL for every built-in, top-level and nested; `ScribeError`/`IngestError::ContractViolation` carry the typed `BifrostError`. 87b736e2e: `IngestError::into_status` attaches the `wyrd-error-bin` problem document so gRPC clients rebuild the exact error | `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::builtin_variant_columns_are_refused_before_ack)"'`; `gate::error::tests::*` | PASS |
| `FIND-TASK-001-3` | fb2415572 removes the publication optional-metrics prerequisite. Fork `iceberg-compaction` fb3a594 (pinned in eccf249ad) removes the rewrite-wide `_row_id` collect/sort/duplicate scan and keeps per-batch missing/type/null validation plus the unchanged copy. Per the human direction relayed by the lead, 83954156c removes the journey's duplicate-copy refusal step because it asserted the deleted mechanism, and the design doc no longer claims duplicate detection | V10 `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite`; V13 `compaction::tests::rewrite_preserves_v3_row_lineage` and `executor::datafusion::tests::row_lineage_is_complete` (fork) | PASS |
| `FIND-TASK-001-4` | 9e39241ac, 83954156c: `architecture/bifrost-design.md` gains "Storage format and Variant", "Variant SQL", and Forge lineage text, and drops the "losslessly" duplicate-key wording. `docs/.../bifrost/schema.svx` gains Storage format and Variant sections, and its spans, metrics, logs, agent_traces, eval, and verification tables are regenerated from the ledgers (`details` removed) | `mise run docs:check`; `mise run check:docs` | PASS |
| `FIND-TASK-001-5` | 0181ff52a: `VariantQueryError` carries the tagged serde `BifrostError`; the coordinator decodes only that form | `oracle::tests::variant_errors_keep_their_catalog_identity_locally_and_remotely`; `error::tests::bifrost_problem_details_reconstruct_exact_variant` (wyrd-client) | PASS |
| `FIND-TASK-001-6` | 963782b7d: `sized` private, `encoded_bytes()` replaces `len`/`is_empty` | `wyrd-queue` `variant::tests::*`; workspace `mise run lints` | PASS |
| `FIND-TASK-001-7` | 963782b7d: TS `QueryResult` JSDoc on the exported class | `mise run ts:typecheck` | PASS |
| `FIND-TASK-001-8` | 86f570328: Wyrd NDV constant removed; parquet-rs derives NDV from row-group geometry | V11 `parquet::writer_properties::tests::bloom_capacity_uses_row_group_limit_for_scribe_and_forge` | PASS |
| `FIND-TASK-001-9` | 0d562a891, 111e7bdce: rustdoc, `# Errors` and `# Panics` on every added or changed item, by item-by-item audit of the base..HEAD diff | audit of the cumulative diff; `mise run lints` | PASS |
| `FIND-TASK-001-10` | 1612506e2: imports hoisted to module or test-module tops; bare signature names | `cargo check --all-targets` on touched crates; `mise run lints` | PASS |

Deviation record (FIND-1): the remediation text says to use serde_json's
"installed arbitrary-precision token preservation". That feature is not
installed; only `raw_value` is. Enabling `arbitrary_precision` would unify
across the workspace and break `f64` and `i128` deserialization through
`#[serde(flatten)]`, untagged, and internally tagged enums: 141 such
attributes, plus iceberg serde. This is a known serde_json limitation. The lead
chose the local `RawValue` plus `i128` lexical classification instead.

Broader verification on the remediated candidate: V1
`tables::tests::variant_contract_and_builtin_schemas_are_stable`; V2
`typed_builtin_payloads_are_queryable` plus
`result_layout_partitions_blooms_and_prunes_by_result`; V4 all three OTLP
`pg_tests` Variant journeys; V5 wyrd-client, V6/V14 Python, V7/V15
TypeScript, and V8 MCP journeys; V9
`published::variant_sql_registry_covers_every_session`; V10; V11; V12
`arrow::schema::tests::variant_round_trips_unshredded` (iceberg-rust e999331f2);
V13; V16 `mise run codegen:check`; V17 `git diff --check`. All PASS. Also PASS:
`mise run fmt`, `lints`, `py:format`, `py:lints`, `ts:typecheck`,
`docs:check`, `check:docs`, `check:client-tier`, `check:pyo3-scope`, and
`check:unwrap-audit`. Not run, as instructed: `verify:bifrost` and `gate`.

Unexpected failure diagnosis (V10):

- **Symptom:** `refuse_unencodable_lineage` asserted that the poisoned
  partition was unchanged, but the rewrite committed.
- **Evidence:** `managed_rewrite.rs:1526`. The live set changed from
  {output, output.duplicate} to one new output.
- **Cause:** the step injected a byte-copy duplicate of `_row_id` and relied on
  the rewrite-wide duplicate scan, which FIND-3 deletes by direction.
- **Fix site:** that journey step and its two helpers, removed in 83954156c.
- **Other callers checked:** none outside `managed_rewrite.rs`.

The lead forbade sub-agents, so no diagnostician was spawned.

Non-goals stayed excluded: no new dependency (the `raw_value` feature was
already unified through workspace-hack), no new option or check, no shredding
or dynamic-table work.
