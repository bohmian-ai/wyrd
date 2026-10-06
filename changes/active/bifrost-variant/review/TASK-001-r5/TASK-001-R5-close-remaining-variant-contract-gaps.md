---
id: TASK-001-R5
kind: remediation
status: review
spec: SPEC-bifrost-variant
spec_revision: 13
requirements: [REQ-003, REQ-004, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-019, INV-002, INV-007, AC-003, AC-005]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-14, FIND-TASK-001-16, FIND-TASK-001-18, FIND-TASK-001-21, FIND-TASK-001-22, FIND-TASK-001-23, FIND-TASK-001-24]
---

# Close the remaining Variant contract gaps

## Authority and immutable subject

- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13.
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`.
- Reviewed base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`.
- Reviewed candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`.
- Reviewed tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`.
- Validated ledger: `changes/active/bifrost-variant/review/TASK-001-r5/findings-validation.md`.

Implement this packet with `$wyrd-implement`, then submit the cumulative task
for another `$wyrd-task-review`. The choices below are complete; implementation
must not reopen the product or architecture decisions.

## Outcome

TASK-001 accepts exactly the user-visible Variant domain promised by revision
13, refuses every invalid row before ACK or durable write with deterministic
catalog identity, preserves the intended built-in Struct schemas, and leaves
the active task evidence truthful and in scope.

## Root causes and implementation units

These findings have two roots and must be implemented as two corrections, not
five local patches:

1. Raw Arrow and Variant encodings can represent more states than Wyrd's
   semantic domain. Validate once at admission; after success, only the
   narrower trusted state may flow toward WAL, storage, or rendering.
2. The Variant walkers currently select an error while traversing. Detect all
   relevant violations within the existing size bound first, then select the
   public error once in the locked precedence order.

Malformed external bytes, JSON, and Arrow arrays necessarily remain
representable at the trust boundary so Wyrd can reject them. The required
unrepresentability begins after the fallible admission function succeeds.

## Unit 1 — validated admission (`FIND-TASK-001-14`, `-22`, `-24`)

### One Arrow-to-domain gate

`scribe::execution_lanes::decode_rows` remains the one ingress transition from
untrusted `RecordBatch` to accepted, stamped rows. Every built-in semantic
check stays behind its existing `DomainTable::CANONICAL_VALIDATOR`, reached by
`enforce_builtin_source_contract`, before fingerprinting, stamping, WAL, or
ACK. Do not add a second ingress gate, a generic validation framework, or
read-side normalization.

Success from this path is the admission witness: no caller may send the
original unvalidated batch to durable work after `decode_rows` has been called.
Keep the current single call path rather than introducing a public
`ValidatedRecordBatch` abstraction; the batch is immutable and the private
decode boundary already owns the transition.

### FIND-TASK-001-14 — make partial nullable Struct values inadmissible

**Diagnosis.** Making persisted children nullable fixed absent-parent Parquet
reads, but raw Arrow can independently set parent and child validity and can
therefore encode states that `Option<DriftReport>`,
`Option<EvalWorkflowSummary>`, metric `BucketRow`, and `Option<ModelRef>` cannot
represent.

**Correction.** Add one small stateless helper beside the existing table
validators that checks a named Struct column row by row and requires every
child validity bit to equal the parent validity bit. Invoke it from the owning
Results, Metrics, and Calls validators for `drift_report`, `eval_summary`,
`positive_buckets`, `negative_buckets`, and `resolved_model`, after ordinary
schema and Variant validation. Refuse a mismatch with the existing
`BifrostError::SchemaParse`. Keep the physical children nullable because
Parquet requires that representation for an absent parent; do not weaken the
semantic domain or add a new error.

After the validator succeeds, a partial value is not admitted even though raw
Arrow can still express it at the boundary.

**Proof.** Test the helper once for complete-present, null-absent,
present-parent/null-child, and null-parent/present-child rows; test each table's
validator wiring. Drive raw Arrow through the real server for verification,
metrics, and gateway and assert the exact code, no retained row, a successful
following valid write, and unchanged hot/published reads.

### FIND-TASK-001-22 — represent the two gateway contracts separately

**Diagnosis.** `CallsTable::model_ref_type` erases a real domain distinction by
giving nullable children to both required `requested_model` and optional
`resolved_model`.

**Correction.** Delete the shared helper and declare the two small Struct
shapes explicitly in `CallsTable::arrow_fields`: `requested_model.provider`
and `.model` are non-null; `resolved_model.provider` and `.model` are nullable.
The schema now represents the required model correctly, while finding 14's
validator enforces the conditional invariant that Arrow cannot express for the
optional model. Add no bool parameter, wrapper, trait, generator,
compatibility schema, or migration; Bifrost is unreleased.

**Proof.** Extend the existing gateway schema contract test to assert both
nested child layouts, retain
`unresolved_call_nulls_resolved_model_children`, and prove raw admission
refuses a partial present `resolved_model`.

### FIND-TASK-001-24 — make unsupported numbers unconstructible internally

**Diagnosis.** Raw Variant bytes can contain Decimal16 values outside Wyrd's
cross-language exact-number domain, while a successfully constructed
`EncodedVariant` currently does not prove otherwise.

**Correction.** Make the existing private-field `EncodedVariant` invariant
literal: every fallible constructor must return it only for fully valid,
bounded, canonical bytes in revision 13's numeric domain. In the raw-byte
validator owned by `EncodedVariant::from_bytes`, accept integer primitives
representable as `i64` and accept Decimal16 only at scale zero with coefficient
`i64::MAX + 1..=u64::MAX`; refuse every other Decimal16 before ACK as
`WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE` with
`numeric_kind: "decimal"`. All renderers and storage paths continue to accept
only `EncodedVariant`, so an acknowledged unsupported decimal is
unconstructible. Add no public decimal type or arbitrary-precision terminal.

**Proof.** Prove fractional Decimal16, scale-zero Decimal16 above `u64::MAX`,
and scale-zero Decimal16 within the signed range are refused with exact details;
prove canonical `u64::MAX` Decimal16 round-trips with all digits intact through
the raw-IPC server journey, with no ACK/no row and continued availability on
every refusal.

## Unit 2 — complete detection, then one error decision (`FIND-TASK-001-16`, `-18`)

### One locked selection rule

Neither the raw-byte walker nor JSON walker may return a public catalog error
merely because it encountered one condition first. Each performs a stack-safe
scan within the already-enforced size bound until every higher-priority class
is ruled out; only a discovered malformed encoding may stop early because
nothing outranks it. The walkers record violation facts without building
rejected descendants and call one local selector after detection. Add one
private `VariantViolations` accumulator beside the existing
singular `VariantViolation`; it stores at most the first malformed, numeric,
and depth violation and its sole finishing method selects malformed first,
numeric range second, and depth third. Both walkers use that finishing method,
so traversal order cannot become public error priority again. This is internal
bookkeeping, not another public error envelope. Do not add another parser and
do not unify the two input walkers. Size remains the earlier outer check.

### FIND-TASK-001-16 — raw bytes become valid only after the complete scan

**Diagnosis.** `EncodedVariant::from_bytes` records depth before complete
encoding validation, so malformed content below depth 64 can receive the wrong
error and an invalid `EncodedVariant` constructor contract remains possible.

**Correction.** Replace the recursive early-return depth walk with one
explicit-stack traversal over the pinned Variant representation's shallow
accessors. Visit the whole size-bounded value; validate metadata, node shape,
offsets, and object-name ordering; record malformed, numeric, and first-depth
facts; then run the one selector. Call upstream recursive `Variant::try_new`
only when no depth violation was recorded, and contain shallow-accessor panics
at `from_bytes` as invalid JSON. Once `from_bytes` succeeds, malformed,
over-depth, or numerically unsupported bytes must be unconstructible as
`EncodedVariant`.

**Proof.** Put malformed content below a depth-65 wrapper and in a later sibling
in both orders; each must select invalid JSON. Retain the 20,000-level
hostile-depth availability proof and all finding-24 numeric cases. Extend the
raw-IPC journey with the compound failures, exact catalog problem, no ACK/no
row, and a successful following write.

### FIND-TASK-001-18 — JSON scanning cannot hide a numeric violation

**Diagnosis.** `append_raw` records depth at the first depth-65 container and
returns from that subtree, so it can select depth without discovering a
higher-priority out-of-range integer below it.

**Correction.** When the normal JSON builder reaches the depth boundary,
switch that `RawValue` subtree to an explicit-stack, non-building token scan.
Reuse `raw_number_variant` and the existing path owner to record the first
numeric violation while retaining the first depth violation. Finish the
size-bounded scan and invoke the same local precedence selector; never build
the rejected subtree and never recurse through hostile depth.

**Proof.** Put an out-of-range integer inside 65 nested arrays and prove the
numeric-range code and exact path through direct conversion, Oracle
`parse_json`, and the pre-ACK JSON-row journey. Assert no retained row,
successful following work, unchanged isolated-depth behavior, and stack safety
at the existing hostile-depth ceiling.

## Artifact corrections (`FIND-TASK-001-21`, `-23`)

### FIND-TASK-001-21 — make the active records factual

**Diagnosis.** The reviewed TASK-001 and implemented R4 packet disagree with
the operative specification, lifecycle, nullable-child decision, and current
catalog-error owner.

**Correction.** Keep TASK-001 in `review`, bound throughout to revision 13,
and cite `QueryCatalogError` as the one local/distributed catalog-error owner.
Mark the R4 remediation `complete`, bind its operative authority and outcome to
revision 12, and label its earlier revision-11 non-null-child diagnosis as
superseded historical evidence. Do not erase history or add a checker or a new
evidence file; R5 alone owns revision 13's raw-decimal decision.

**Proof.** Resolve every active source citation, check both front matters and
operative sections against their named revisions, and run `git diff --check`.

### FIND-TASK-001-23 — remove unrelated workflow-policy drift

**Diagnosis.** Commit `c8da11067` changed both mirrored
`wyrd-implement/SKILL.md` files without TASK-001 authority; the user's earlier
exception covered only the task-review skill changes.

**Correction.** Revert only `c8da11067`'s changes in
`.agents/skills/wyrd-implement/SKILL.md` and its `.claude` mirror. Preserve the
approved task-review skill edits and all unrelated repository state.

**Proof.** Compare those two files to the pre-`c8da11067` content, confirm the
task-review files are unchanged, and run `mise run check:skills-sync`.

## Shared constraints and non-goals

- Preserve exact i64/u64 JSON meaning, IEEE double meaning including `3.0`,
  last-key-wins behavior, missing/null distinction, stable limits, catalog
  codes, and logical fingerprints.
- Preserve nullable physical children where an absent parent requires them and
  preserve every valid built-in producer.
- Reuse the installed Arrow 59.3 Variant crates, the existing `EncodedVariant`
  owner, and existing table validators; create no parallel validation or
  rendering path.
- Add no dependency, public API, error, configuration, migration,
  compatibility schema, arbitrary-precision number surface, or shredding.
- Do not change query-error transport, authorization, tenancy, Iceberg
  lineage, Forge behavior, SDK collection semantics, or TASK-002/TASK-003
  ownership.

## Acceptance criteria

| Finding | Done when |
|---|---|
| `FIND-TASK-001-14` | The one pre-ACK table-validation seam admits only complete-present or null-absent affected Structs; no partial value reaches durable work. |
| `FIND-TASK-001-16` | `EncodedVariant::from_bytes` succeeds only after a complete iterative bounded scan, and malformed encoding outranks numeric and depth without stack risk. |
| `FIND-TASK-001-18` | JSON walkers finish bounded violation detection before selecting an error, so an out-of-range number below depth 64 wins in every admission path. |
| `FIND-TASK-001-21` | TASK-001, R4, revision ownership, lifecycle, and source citations agree. |
| `FIND-TASK-001-22` | Required and optional gateway model fields have separate declarations with their distinct nested nullability. |
| `FIND-TASK-001-23` | Only the unauthorized `wyrd-implement` edits are gone and mirrored skills are synchronized. |
| `FIND-TASK-001-24` | Every `EncodedVariant` constructor enforces revision 13's exact numeric domain, so storage and renderers cannot receive an unsupported Decimal16. |

All focused negative journeys must assert the exact catalog code and details,
no ACK/no retained row, and continued availability after refusal.

## Verification

Run the exact new or modified tests by their final names with
`mise exec -- cargo nextest run --locked`, including the owning package and
exact expression.
At minimum retain and run these existing focused proofs where affected:

- `variant::tests::raw_depth_is_bounded_before_full_validation`;
- `variant::tests::numeric_range_outranks_depth_in_any_key_order`;
- `oracle::variant_sql::tests::parse_json_numeric_range_outranks_depth_in_any_key_order`;
- `tables::tests::variant_contract_and_builtin_schemas_are_stable`;
- `components::gateway::capture::tests::unresolved_call_nulls_resolved_model_children`;
- `verification_runtime::typed_builtin_payloads_are_queryable`;
- `verification_runtime::builtin_variant_columns_are_refused_before_ack`;
- `metrics_export::pg_tests::metric_variant_fields_and_promotions_are_queryable`.

Then run the narrowest owning Bifrost journey lanes for raw Arrow admission,
JSON admission, verification, metrics, gateway, Oracle `parse_json`, and the
Rust/Python/TypeScript/MCP Variant contract, followed by:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:skills-sync
git diff --check
```

Record every command and exit status in this file's implementation evidence.

## Implementation Evidence — 2026-10-06

Every command ran on the final working tree with
`CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd-bifrost-variant/target`
and `WYRD_LOG=info`. Spec binding is revision 13.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-001-14` | `tables/mod.rs` `refuse_partial_structs`, called from `validate_predeclared` for each table's `DomainTable::WHOLE_STRUCTS` (Results: `drift_report`, `eval_summary`; Calls: `resolved_model`) and from `metrics/projection.rs` `validate_metric_points` for both bucket sets. Refusal is `BifrostError::SchemaParse` (`WYRD_VALA_400_SCHEMA_PARSE`) with the row and child named | `tables::tests::partial_nullable_structs_are_refused` (all four parent/child states; Results and Calls validators); `tables::tests::partial_metric_buckets_are_refused`; journey `verification_runtime::typed_builtin_payloads_are_queryable` (`refuse_partial_summary` before the Eval write) and `builtin_variant_columns_are_refused_before_ack` (partial points frame refused with the exact detail, no row, accepted points frame afterwards). Gateway calls stay unit-level: only the in-process `GATEWAY_CAPTURE_PRINCIPAL` may write that table, so no client can submit a raw partial `resolved_model` | PASS |
| `FIND-TASK-001-16` | `wyrd-queue/src/variant.rs` `from_bytes`: size, then `scan_encoded` (iterative explicit stack, `VariantMetadata::try_new`, name-order check, depth, Decimal16 domain) inside `catch_unwind`, then `Variant::try_new` only when no depth violation was found; `VariantViolations::finish` selects malformed, then numeric, then depth. `check_depth` deleted | `variant::tests::raw_depth_is_bounded_before_full_validation`; `variant::tests::raw_malformed_outranks_depth_in_any_position`; journey cases malformed below the limit, malformed before and after an over-deep sibling | PASS |
| `FIND-TASK-001-18` | `append_raw` records instead of returning; an over-depth container is still scanned by `scan_numbers` (iterative, last duplicate key wins) so a numeric violation anywhere is found before the error is chosen | `variant::tests::numeric_range_outranks_depth_in_any_key_order` (incl. nested path `/a` + `/0`×65); `oracle::variant_sql::tests::parse_json_numeric_range_outranks_depth_in_any_key_order`; journey `refuse_numeric_before_depth` (three cases) | PASS |
| `FIND-TASK-001-21` | TASK-001 front matter and body bind revision 13 and cite `QueryCatalogError`; R4 packet `status: complete`, `spec_revision: 12`, authority line names revision 12, revision-11 child diagnosis labelled superseded history | `git grep remote_variant_error` finds only r1 review history; `git diff --check` | PASS |
| `FIND-TASK-001-22` | `gateway/calls.rs` declares `requested_model` (required children) and `RESOLVED_MODEL` (nullable children) separately; `model_ref_type` deleted | `gateway::calls::tests::calls_table_is_the_payload_contract_plus_the_managed_envelope`; `components::gateway::capture::tests::unresolved_call_nulls_resolved_model_children` | PASS |
| `FIND-TASK-001-23` | both `wyrd-implement/SKILL.md` mirrors restored to `c8da11067~1` | `mise run check:skills-sync` | PASS |
| `FIND-TASK-001-24` | `scan_encoded` refuses a Decimal16 with nonzero scale or a coefficient outside `i64::MAX+1..=u64::MAX` with `numeric_kind: "decimal"`; `EncodedVariant` doc states the domain; `docs/src/content/docs/bifrost/schema.svx` states it | `variant::tests::raw_decimals_outside_the_exact_domain_are_refused` (`u64::MAX` accepted and rendered exactly); journey cases fractional, above `u64::MAX`, within `i64`, refused decimal after an over-deep sibling (`/1`), and accepted `decimal16(u64::MAX, 0)` read back as `[[u64::MAX]]` | PASS |

### Commands

| Command | Exit |
|---|---|
| `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(/^variant::/)'` (11 tests) | 0 |
| `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(/^tables::/)'` (44 tests, before the test split) | 0 |
| `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=tables::tests::partial_nullable_structs_are_refused) \| test(=tables::tests::partial_metric_buckets_are_refused)'` (after the split) | 0 |
| `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::capture::tests::unresolved_call_nulls_resolved_model_children)'` | 0 |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(/^oracle::variant_sql::/)'"` (4 tests) | 0 |
| `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::typed_builtin_payloads_are_queryable) \| test(=verification_runtime::builtin_variant_columns_are_refused_before_ack)"'` | 0 |
| `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=metrics_export::pg_tests::metric_variant_fields_and_promotions_are_queryable)"'` | 0 |
| V5 `-p wyrd-client --test pg_bifrost_e2e -E "test(=pg_tests::builtin_variant_and_struct_payloads_are_queryable)"` (Postgres wrapper, `db:migrate:inner`) | 0 |
| V8 `-p wyrd-mcp --test mcp -E "test(=query::pg_tests::builtin_variant_and_struct_payloads_are_queryable)"` (Postgres wrapper) | 0 |
| V9 `-p wyrd-testing --test oracle -E "test(=published::variant_sql_registry_covers_every_session)"` (Postgres wrapper) | 0 |
| V6/V14 Python `pytest -m integration tests/integration/test_bifrost_query.py::test_builtin_variant_and_struct_payloads_are_queryable tests/integration/test_bifrost_query.py::test_canonical_signal_arrow_write_and_sql_read_round_trip` (Postgres wrapper, `py:setup`) | 0 |
| V7/V15 TypeScript `vitest run tests/integration/oracle-query.test.ts -t "builtin Variant and Struct payloads are queryable\|canonical signal Arrow write and SQL read round-trip"` (Postgres wrapper, `ts:build`, `ts:build:testing`) | 0 |
| `mise run fmt` | 0 |
| `mise run lints` (first run) | 101 |
| `mise run lints` (after splitting the metrics case out of `partial_nullable_structs_are_refused`) | 0 |
| `mise run codegen:check` | 0 |
| `mise run check:skills-sync` | 0 |
| `mise run docs:check` | 0 |
| `git diff --check` | 0 |

The first `lints` run failed on `clippy::too_many_lines` (103/100) in
`tables::tests::partial_nullable_structs_are_refused`; the metric bucket case
became its own test, `partial_metric_buckets_are_refused`. No production code changed for that fix.

### Non-goals

Nothing in the diff adds a dependency, public API, error, configuration,
migration, compatibility schema, arbitrary-precision number surface, or
shredding. Query-error transport, authorization, tenancy, Iceberg lineage,
Forge, SDK collection semantics, and TASK-002/TASK-003 ownership are
unchanged. The task-review skills are unchanged.

### Addendum — shared object field values (human direction, 2026-10-06)

- **Problem:** object field offsets may point at the same bytes, so about 640
  bytes describe 2^64 nodes. Both `scan_encoded` and upstream
  `Variant::try_new` walk every field: a throwaway probe measured 22 levels
  (221 bytes) at 5.5 s and 2.3 s respectively, doubling per level. Upstream
  arrow-rs `main` (release 60.0.0) still validates each offset with no overlap
  check, and no issue or PR tracks it.
- **Fix:** `scan_encoded` counts visited nodes; every honest node owns at
  least one byte, so past `value.len()` nodes the value is recorded malformed
  (`WYRD_VALA_400_VARIANT_INVALID_JSON`) and the walk stops. `from_bytes` no
  longer runs `Variant::try_new` once a malformed value is found.
- **Proof:** `variant::tests::raw_shared_field_values_are_refused` (64-level
  shared chain refused in milliseconds); journey case "objects whose fields
  share one child at every level" in
  `verification_runtime::builtin_variant_columns_are_refused_before_ack`.

| Command | Exit |
|---|---|
| `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(/^variant::/)'` (12 tests) | 0 |
| V2 journeys `typed_builtin_payloads_are_queryable`, `builtin_variant_columns_are_refused_before_ack` (Postgres wrapper) | 0 |
| Oracle `test(/^oracle::variant_sql::/)` (Postgres wrapper, 4 tests) | 0 |
| `mise run fmt`; `mise run lints`; `git diff --check` | 0 |

New items: `shared_objects` and `AB_METADATA` (journey fixtures; `nested_lists`
and `EMPTY_METADATA` cannot express a keyed object) and the unit test above.

### New items

| New item | Owners searched | Why new |
|---|---|---|
| `VariantViolations` (private) | `wyrd-queue/src/variant.rs` `VariantViolation`, `parquet-variant` validation API | `VariantViolation` is the error; the walk needed to hold the first violation of each class until the locked order picks one |
| `scan_numbers` (private) | `append_raw`, `serde_json` `RawValue` | `append_raw` builds a Variant; past depth 64 nothing is built, but numbers must still be checked, and recursion would hit serde's 128 limit |
| `scan_encoded` (private, replaces `check_depth`) | `Variant::try_new`, `VariantMetadata::try_new`, old `check_depth` | upstream validation is recursive and has no Decimal16 domain; the old `check_depth` stopped at the first depth hit |
| `JsonPointer::escape`, `JsonPointer::truncate` | `JsonPointer` | extend the existing owner; `scan_numbers` restores paths without rebuilding them |
| `refuse_partial_structs`, `DomainTable::WHOLE_STRUCTS` | `validate_predeclared`, `CANONICAL_VALIDATOR`, Arrow `validate_nulls` | one helper at the shared table-validator seam; Arrow allows partial nulls, so nothing upstream refuses them |
| `RESOLVED_MODEL` | `CallsTable` constants | names the column that `WHOLE_STRUCTS` and the test use |
| `partial_nullable_structs_are_refused`, `partial_metric_buckets_are_refused`, `raw_malformed_outranks_depth_in_any_position`, `raw_decimals_outside_the_exact_domain_are_refused`; test helpers `EMPTY_METADATA`, `list`, `nest` | existing `variant` and `tables` tests | new scenarios; `nest` replaces the inline nesting that `raw_depth_is_bounded_before_full_validation` used before |
| journey helpers `variant_list`, `decimal16`, `points_frame`, `refuse_partial_summary`; constants `RESULTS`, `POINTS`, `VARIANT_DECIMAL16`, `VARIANT_MALFORMED` | `verification_runtime.rs` `nested_lists` and fixtures | `nested_lists` now folds over `variant_list`; the rest encode cases no existing fixture did |
