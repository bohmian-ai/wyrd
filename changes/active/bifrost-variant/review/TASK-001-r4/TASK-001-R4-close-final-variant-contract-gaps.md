---
id: TASK-001-R4
kind: remediation
status: ready
spec: SPEC-bifrost-variant
spec_revision: 11
requirements: [REQ-003, REQ-004, REQ-009, REQ-018, REQ-019, INV-002, INV-007, AC-003, AC-005]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-14, FIND-TASK-001-15, FIND-TASK-001-16, FIND-TASK-001-17, FIND-TASK-001-18, FIND-TASK-001-19, FIND-TASK-001-20, FIND-TASK-001-21]
---

# Close final TASK-001 Variant contract and evidence gaps

## Authority and immutable review subject

- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Reviewed base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Reviewed candidate: `a6429060fb011aafa4335f2f736c70adab231739`
- Reviewed tree: `64177d67141993ec18e63b43dc227dbc31d70950`
- Validated ledger: `changes/active/bifrost-variant/review/TASK-001-r4/findings-validation.md`

Implement this task with `$wyrd-implement`. Reassess the complete original
base-to-remediated-candidate range afterward; do not review only this
remediation diff.

## Outcome

TASK-001 retains its consolidated Variant, Oracle, built-in, SDK, and Iceberg
owners while closing the remaining trust-boundary and proof gaps. Raw Variant
input is bounded before dependency recursion, only the exact canonical Arrow
extension is admitted, locked failure precedence is deterministic, nullable
verification Structs cannot fabricate children, the CLI has its real journey,
and the tracked task packet describes the actual candidate. The dead rendering
wrapper is absent; the explicitly allowed review-policy edits are preserved.

## Issue diagnoses and required corrections

### FIND-TASK-001-14 — Null verification Structs fabricate child values

The verification-result producer builds absent `drift_report` and
`eval_summary` parents with null parent validity but valid placeholder children:
empty strings, encoded JSON null, and numeric zero. DataFusion 55 `get_field`
returns the child array without applying parent validity, and the Variant
placeholder guard cannot identify valid JSON-null bytes or primitive zeros.
Queries can therefore observe child values for a report that is absent.

Correct the existing verification-result producer. Under an absent parent,
emit null child slots using Arrow's native masked-null Struct semantics; use
the existing `VariantColumnBuilder` null path for `features` and nullable
primitive/string arrays for the other children. Preserve the declared public
non-null child schema, present-row values, DataFusion `get_field`, and the
shared Variant placeholder guard used by other storage inputs. Do not add a
read-normalization layer or custom Struct operator.

### FIND-TASK-001-15 — Predeclared built-ins violate locked error precedence

The predeclared table validator traverses declared Variant values before the
later fingerprint establishes the complete undeclared, missing, ordered, and
non-Variant wire schema. A request combining an earlier schema defect with a
later malformed, over-depth, or oversized Variant therefore returns the wrong
stable error. Canonical signal validation already demonstrates the required
phase ordering.

Extend the existing table-owned predeclared validator to establish the
complete declared user schema in logical-field order and return the existing
undeclared or unsupported errors before calling the shared recursive Variant
value validator. Keep the later fingerprint as the physical identity fence.
Do not add a second validator type, table-name switch, or downstream guard.

### FIND-TASK-001-16 — Raw depth is checked after recursive full validation

`EncodedVariant::from_bytes` checks size, calls upstream `Variant::try_new`,
and only then checks the Wyrd depth limit. The pinned dependency recursively
fully validates list/object children, while a very deep one-child structure
remains far below the byte ceiling. A raw built-in Arrow request can therefore
exhaust the process stack before Wyrd returns the required depth error.

At the existing `EncodedVariant::from_bytes` owner, keep the byte limit first,
then iteratively preflight depth through the installed Variant
representation's shallow accessors and stop at depth 65. Contain documented
malformed shallow-access panics with the repository's standard-library unwind
boundary and map them to the existing invalid-JSON violation. Only after the
bounded preflight passes should upstream `Variant::try_new` perform full
validity checking. Upstream remains the encoding authority; do not add a
byte-format parser, second Variant model, dependency, or configurable limit.

### FIND-TASK-001-17 — Foreign extension metadata is accepted and erased

The shared `is_variant` predicate checks only `ARROW:extension:name`.
Arrow-to-wire conversion then normalizes a correct-name field with foreign
extension metadata into canonical Variant and erases the mismatch. The
dependency intentionally ignores supplied metadata, so it does not enforce the
Bifrost wire contract.

Make the existing shared predicate require both `VariantType::NAME` and exact
empty extension metadata. Continue using this one owner from Arrow-to-wire
conversion and recursive admission at every nesting level. Add no sibling
validator.

### FIND-TASK-001-18 — Numeric/depth compound failures depend on key order

The sole raw-token walker returns immediately when it reaches either a depth
or numeric violation, and objects are traversed in sorted-key order. A JSON
object containing an out-of-range integer and a depth-65 branch can therefore
return either public error based only on key names, although numeric range is
locked before depth.

Keep `EncodedVariant::from_json_text` and its raw-token walker as the only
conversion owner. Retain a depth violation while continuing the bounded
traversal necessary to find a higher-priority numeric violation, then select
numeric before depth independently of object order. Do not add another parser,
Variant type, or limit. The rejected oversize-plus-depth claim is not part of
this remediation.

### FIND-TASK-001-19 — CLI Variant rendering lacks a journey

The compiled CLI now installs `VariantJsonEncoderFactory`, but its existing
server journey selects only primitive columns. SDK and MCP journeys cannot
prove CLI metadata propagation, binary wiring, or JSONL spelling, so a CLI-only
regression could emit the physical storage Struct or collapse `3.0` to `3`.

Extend the existing compiled-binary `query_server_journey` using its current
server and credential helpers. Query a Variant object containing `3.0` and
`u64::MAX`; assert native object output, exact integer digits, and raw `3.0`
spelling. Add no harness, fixture framework, or CLI-specific encoder.

### FIND-TASK-001-20 — Dead public rendering wrapper

`EncodedVariant::to_json` delegates directly to the existing
`variant_bytes_to_json` owner and has only two same-module unit-test callers.
Production consumers already use the byte renderer because they hold metadata
and value buffers separately. The public method adds a second owner and API
surface without a capability.

Delete `EncodedVariant::to_json` and update its two unit-test assertions to
call `variant_bytes_to_json(encoded.metadata(), encoded.value())`. Add no
replacement method, trait, or new test.

### FIND-TASK-001-21 — Final task evidence is factually stale

The tracked task front matter binds revision 11 while its authority link says
revision 10; its final evidence records compaction revision `94db7b94...` while
the candidate pins `2b65fa189f2d05002acc6e59515a071a63777970`; and its
placeholder diagnosis names nonexistent `wyrd_queue::variant::is_placeholder`
instead of the actual logic in `mask_placeholders`. Independent r4 proof makes
this an evidence-integrity defect rather than an unresolved lineage defect.

Update the existing task record once: name revision 11, the actual
`mask_placeholders` owner and behavior, and the tested/pinned final compaction
SHA. Attribute the final-pin fork proof accurately and do not imply the broader
Postgres V10 journey was rerun at the final pin if it was not. Do not create a
second evidence artifact.

## Constraints and preserved behavior

- Preserve exact i64/u64 integer meaning, doubles including `3.0`, duplicate-key
  last-wins behavior, null-versus-missing semantics, limits, catalog codes, and
  the logical fingerprint tag `0x0d`.
- Preserve one Variant model based on the installed Arrow 59.3 Variant crates;
  upstream remains the full encoding/validity authority.
- Preserve table-owned built-in validation before ACK/WAL and the existing
  catalog error identities; only phase ordering and exact extension identity
  change.
- Preserve the public verification Struct schemas, present values, sensitive
  classification, audit hash inputs, promoted values, and canonical producers.
- Preserve Struct `get_field`, semantic `variant_get`, all Oracle session
  registration, late interactive/distributed error identity, and SDK
  no-partial-result behavior.
- Preserve Iceberg v3 creation, row lineage, final fork pins, v3 GC, the
  five-field Forge handoff, and native Parquet Bloom behavior.
- Keep `wyrd-spec` IO-free and PyO3-free; keep DataFusion, Parquet, and Iceberg
  out of client-tier crates.
- Make no security/planning change for the rejected `SEC-R4-001` proposal.

## Non-goals

- No shredding, leaf projection/pruning, TASK-002 authoring behavior,
  DataFusion repin, migration, compatibility alias, signing, new public wire
  field, error code, configuration, dependency, feature, or validation
  framework.
- No second Variant parser/model, read-normalization layer, Struct SQL
  operator, CLI encoder, test harness, evidence artifact, or repository check.
- No unrelated Oracle, Scribe, Forge, SDK, documentation, or workflow-policy
  refactor.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-14` | Every child of absent `drift_report` and `eval_summary` reads as SQL null on hot and published rows; present summaries remain unchanged. |
| `FIND-TASK-001-15` | A predeclared built-in request with competing schema and Variant defects returns the locked earlier schema code, receives no ACK, and writes no durable row; isolated Variant defects retain their codes. |
| `FIND-TASK-001-16` | A compact hostile raw Variant deeper than 64 returns the exact depth error without process loss, after which the same server accepts a valid batch; malformed bytes remain typed errors. |
| `FIND-TASK-001-17` | Correct-name Variant fields with non-empty/foreign metadata are refused as `UnsupportedType` before ACK at top-level and nested built-in positions. |
| `FIND-TASK-001-18` | Numeric-range wins over depth with identical code/details in both object-key orders through direct conversion, Oracle `parse_json`, and one bounded pre-ACK write. |
| `FIND-TASK-001-19` | The existing compiled CLI journey proves native Variant JSON, exact `u64::MAX`, and raw `3.0` spelling. |
| `FIND-TASK-001-20` | `EncodedVariant::to_json` is absent; existing Variant rendering tests pass through the shared byte owner. |
| `FIND-TASK-001-21` | The task evidence names revision 11, an existing placeholder owner, and the exact compaction SHA present in both manifest and lock; the final-pin V13 proof passes. |

## Focused proof and broader verification

Run every new or changed exact test through `mise exec --` with its explicit
package, target, features, and exact expression. At minimum prove:

1. the existing verification real-server journey extended for absent/present
   Struct child projection;
2. the existing raw-IPC server journey extended for competing-error order,
   hostile depth with post-refusal server availability, and foreign extension
   metadata;
3. focused `wyrd-queue` direct compound-precedence and rendering tests;
4. focused Oracle `parse_json` compound-precedence tests;
5. the existing compiled CLI server journey through its repository-managed
   environment;
6. the exact pinned compaction V13 command at
   `2b65fa189f2d05002acc6e59515a071a63777970`;
7. `mise run fmt`, `mise run lints`, `mise run codegen:check`,
   `mise run check:skills-sync`, and `git diff --check`;
8. the original TASK-001 V1–V17 proofs whose owned source or behavior changed,
   plus the affected Rust/Python/TypeScript/MCP consumer journeys.

Do not replace exact focused selectors with a positional filter that can select
zero tests. Do not weaken, ignore, allow, or delete an existing gate to clear
this remediation.

## Implementation Evidence — 2026-10-06

### Diagnosis — V2 Gate write refused after nulling absent Struct children

- **Symptom:** `verification_runtime::typed_builtin_payloads_are_queryable`
  failed with `OtlpRequestMalformed { table: "unknown", detail: "ingest frame
  validation failed" }` once absent `drift_report`/`eval_summary` children
  became null slots.
- **Evidence:** `scribe/material_plan.rs` `NativeScan::visit_batch` refused
  every non-nullable node with `null_count != 0`; `gate/error.rs` maps that
  `ScribeError::InvalidFrame` to the OTLP-labelled decode error before table
  resolution. Arrow 59.3 `ArrayData::validate_nulls` /
  `validate_non_nullable` (`arrow-data-59.3.0/src/data.rs:1416-1479`) admit a
  non-nullable Struct child null exactly where the parent is null.
- **Cause:** the native preflight was stricter than Arrow and the client
  encoder (`scribe/fixed_ipc.rs` `visit_nodes`); this is the same rule the
  original placeholder workaround existed to satisfy.
- **Fix site:** `NativeScan` (every native write passes it). A field under
  an enclosing nullable Struct (no List between) may carry nulls; exact
  bitmap containment stays with Arrow decode validation, which always runs (`preprocess.rs` `StreamDecoder::new()`, no
  `skip_validation`). Top-level columns and list items stay strict. Other
  callers checked: `validate_buffer_layouts` (count/bitmap agreement only),
  `tables/mod.rs` `validate_variant_values` (skips null cells),
  `execution_lanes.rs` (top-level `card_ref` only).
- **Diagnostician (`diag-r4-frame`, read-only):** same cause and fix site;
  Arrow decode validation already enforces exact containment; the
  top-level `native_preflight_rejects_nonnullable_nulls` stays red.

### Diagnosis — published Struct children of an absent summary read non-null

- **Symptom:** after the Gate fix, V2 passes "hot summary Struct children" but
  the published read (after `flush_bifrost`) returns `method: ""`,
  `verdict: ""`, numeric children, and an empty Variant for absent
  `drift_report`/`eval_summary`; its JSON rendering is invalid
  (`"...[features]":,`).
- **Evidence:** traced `rows` output (scratchpad `r4/v2c.log`): the scored
  Drift row's published `eval_summary` reads `1, 1, 0, 1.0, 7`, which are the
  Eval row's values. `parquet-59.3.0/src/arrow/record_reader/mod.rs:261-276`
  (`consume_bitmap`) drops the null mask of a REQUIRED leaf by design;
  `record_reader/buffer.rs:66-84` (`pad_nulls`) leaves vacated slots stale;
  `array_reader/struct_array.rs:121` rebuilds nulls only for the nullable
  parent; `datafusion-functions-55.1.0/src/core/getfield.rs:237-242` returns
  the child without the parent's nulls. Hot reads come from Scribe's Arrow IPC
  and keep the child nulls.
- **Cause:** a non-null Arrow child is a REQUIRED Parquet leaf, so the pinned
  reader cannot return its parent-masked nulls, and `get_field` exposes the
  padded buffer. The placeholders this remediation removed never reached
  Parquet either (the writer stores no leaf value under a null parent), so the
  published leak predates this candidate. Separately,
  `VariantJsonEncoderFactory` rendered from the masked storage but passed the
  unmasked `array.logical_nulls()` to `NullableEncoder`.
- **Fix site:** the encoder is fixed at its owner (`storage.logical_nulls()`),
  proven red-then-green by `variant::tests::json_writer_renders_variants_as_values`;
  this repairs the CLI, MCP, Rust SDK, and test renderers. The child-null leak
  has no in-contract fix site: spec revision 11 (lines 230-251) fixes these
  children as non-null, and this remediation forbids a read-normalization
  layer or custom Struct operator. The only remaining options are nullable
  children in the declared schema (a persisted fingerprint/Iceberg schema
  change), a read layer that pushes parent nulls into children on the hot
  and Iceberg paths, or an upstream parquet/DataFusion change.
- **Diagnostician (`diag-r4-published`, read-only):** same cause and fix
  sites; flags auditing other built-ins with a nullable Struct over non-null
  children (List<Struct> elements are unaffected).

**Resolution (human decision, 2026-10-06):** spec revision 12 makes every
child of the nullable `drift_report` and `eval_summary` Structs nullable
(`ResultsTable::drift_report_fields` / `eval_summary_fields`), so the leaves
are OPTIONAL Parquet columns whose nulls survive the read and `get_field`
returns SQL null with no read layer. TASK-001 now binds revision 12. The
Scribe masked-null allowance adopts TASK-002 commit `535367c94`'s
`fixed_ipc.rs` and `material_plan.rs` verbatim (same base blobs), so one
implementation exists across the stack. Out of this spec's scope and left
for its owner (`bifrost-canonical-otel-signals`): `vala.gateway.calls.resolved_model`
and `vala.metrics.points.positive_buckets`/`negative_buckets` are nullable
Structs over non-null children with the same published-read leak.

### Acceptance

All commands ran with
`CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd-bifrost-variant/target`
on the final working tree and exited 0. Spec binding is revision 12.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-14: every child of absent `drift_report`/`eval_summary` reads SQL null hot and published; present summaries unchanged | `tables/verification/results.rs` nullable children (spec rev 12); `wyrd-server/src/verification/results.rs` null child slots via `VariantColumnBuilder` null path and Option arrays; Scribe masked-null allowance from TASK-002 `535367c94`; `VariantJsonEncoderFactory` uses masked nulls | V2 `verification_runtime::typed_builtin_payloads_are_queryable` ("hot/published summary Struct children"); `verification::results::tests::unscored_drift_writes_only_the_summary`; `variant::tests::json_writer_renders_variants_as_values` (red without the encoder fix); `scribe::fixed_ipc::tests::masked_required_struct_child_null_roundtrips`; V1 layout test | PASS |
| FIND-15: competing schema + Variant defects return the earlier schema code, no ACK, no row; isolated Variant defects keep codes | `tables/mod.rs` `validate_predeclared` + `refuse_undeclared`; `tables/signal.rs` shares it | `verification_runtime::builtin_variant_columns_are_refused_before_ack` (undeclared → `UndeclaredField`, schema drift → 409 `FINGERPRINT_MISMATCH` against invalid/over-deep/oversized Variants; accepted batch afterwards) | PASS |
| FIND-16: compact hostile depth > 64 returns the depth error without process loss; server then accepts; malformed bytes stay typed | `wyrd-queue/src/variant.rs` `from_bytes`: size, bounded shallow `check_depth` inside `catch_unwind`, then `Variant::try_new` | `variant::tests::raw_depth_is_bounded_before_full_validation` (20,000 levels; upstream `try_new` alone aborts); journey case "a compact hostile depth" then accepted batch | PASS |
| FIND-17: correct-name Variant with foreign metadata refused `UnsupportedType` top-level and nested | `is_variant` requires `None` or `""` metadata (Arrow writes `""`, the Iceberg fork reads `None`) | `variant::tests::variant_extension_requires_empty_metadata`; journey cases "foreign extension metadata", "nested foreign extension metadata" | PASS |
| FIND-18: numeric range outranks depth in either key order via conversion, `parse_json`, and a pre-ACK write | `append_raw` defers the depth violation and keeps walking; `from_json_text` returns it last. `batch_builder.rs` keeps each field's raw token so SDK JSON rows reach that owner | `variant::tests::numeric_range_outranks_depth_in_any_key_order`; `oracle::variant_sql::tests::parse_json_numeric_range_outranks_depth_in_any_key_order`; journey `refuse_numeric_before_depth` | PASS |
| FIND-19: compiled CLI proves native Variant JSON, exact `u64::MAX`, raw `3.0` | `wyrd-cli/tests/query_server_journey.rs` | `query_server_journey::query_command_reads_seeded_table` | PASS |
| FIND-20: `EncodedVariant::to_json` absent | deleted; tests use `variant_bytes_to_json` | `wyrd-queue` lib (60/60) | PASS |
| FIND-21: task evidence names the bound revision, the real placeholder owner, the pinned compaction SHA; final-pin V13 passes | `tasks/TASK-001-variant-storage-and-query.md` | V13 at `2b65fa189f2d05002acc6e59515a071a63777970`; V12 at iceberg-rust `e999331f`; V10 rerun on this tree | PASS |

Commands:
- Unit: `mise exec -- cargo nextest run --locked -p wyrd-queue --lib`;
  `-p vala-bifrost-redux --lib --features test-support -E 'test(/^tables::/) | test(/^scribe::fixed_ipc::/) | test(/^scribe::material_plan::/)'`
  (58); Scribe lib under the Postgres wrapper (348); `-p wyrd-server --lib -E 'test(/verification::results::/)'` (7);
  Oracle `parse_json_numeric_range_outranks_depth_in_any_key_order` and
  `variant_operators_and_functions_follow_the_contract` under the wrapper.
- Journeys: V2 + `builtin_variant_columns_are_refused_before_ack`; V4 (3);
  V5; V6 + V14 + `test_drift_journey.py` (5); V7 + V15; V8; V9; V10; V11;
  V12; V13; CLI `query_server_journey::query_command_reads_seeded_table`;
  Rust `wyrd-sdk-rust --test drift_verification` (4); TypeScript
  `drift-verification.test.ts`.
- Gates: `mise run fmt`, `mise run lints`, `mise run codegen:check`,
  `mise run check:skills-sync`, `git diff --check`.

Non-goals stayed excluded: no shredding, read-normalization layer, Struct
operator, second Variant parser, CLI encoder, dependency, error code, or
configuration. `batch_builder.rs` changed because FIND-18's pre-ACK write
goes through it; it now hands raw tokens to the existing conversion owner
instead of reparsing through `serde_json::Value`, which also closes a silent
integer-to-double coercion of values above `u64::MAX`. The depth preflight
reuses the existing recursive `check_depth`, bounded at 65 frames, rather
than a new iterative walker.

| New item | Owners searched | Why new |
|---|---|---|
| `batch_builder::collect_raw` | `collect` (same module), `EncodedVariant::from_json_text` | `collect` now delegates to it; the Variant column needs each field's raw token, which `collect`'s `&Value` signature discards |
| `tables::refuse_undeclared` | `validate_predeclared`, `signal.rs` `validate_canonical_user_batch` inline check | the one undeclared-field check, now shared by both validators instead of inline in one |
| `fixed_ipc::has_unmasked_null` | `visit_nodes`, Arrow `NullBuffer` API | adopted verbatim from TASK-002 `535367c94` so the stack has one implementation |
| `verification_runtime` `EXTENSION_METADATA_KEY`, `refuse_numeric_before_depth` | file's `EXTENSION_NAME_KEY`; `arrow` facade (does not re-export `arrow_schema::extension`) | follows the file's existing constant; the method drives the SDK JSON-row write path the raw-IPC helpers cannot |
| tests `numeric_range_outranks_depth_in_any_key_order`, `raw_depth_is_bounded_before_full_validation`, `variant_extension_requires_empty_metadata`, `parse_json_numeric_range_outranks_depth_in_any_key_order`, `masked_required_struct_child_null_roundtrips` | existing module tests | one per new behavior |
