---
id: TASK-004-REV-04
kind: remediation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 26
requirements: [REQ-028, REQ-039, REQ-061, INV-001, INV-002, INV-004, AC-018, AC-022]
depends_on: []
parent_task: TASK-004
remediates: [BIFROST-BLOOM-001, BIFROST-SCHEMA-001]
---

# Task 04 — Remove redundant casts and sensitivity metadata

## Outcome and Value

An Observe query such as `service_name IN ('a','b','c','d')` uses the
Parquet Bloom filter to skip row groups that contain none of those values,
just as a three-value list does. Results remain identical. A Bloom filter is
a file index that can prove a value is absent; an absent value must not force
the reader to decode the row group.

Remove `wyrd:sensitive` from Bifrost's schemas and writers. It currently checks
that its own labels survive schema fingerprinting; it does not enforce read
permission. Keep the existing table-owned permission classifications.

The caller explicitly requested this one remediation on 2026-11-04, including
changes to Bifrost and its DataFusion fork. That direction supplements approved
specification revision 26 for this task; it authorizes removing the labels and
their fingerprint dependency. It does not authorize implementation, publishing
a fork revision, deleting data, or revising unrelated TASK-004 assignments.

## What Changes and What Must Stay True

- **BIFROST-BLOOM-001:** the table schema comes from Iceberg, while a Parquet
  field may carry extra metadata. DataFusion inserts a text-to-identical-text
  cast to reconcile them. That cast blocks Bloom extraction for longer `IN`
  lists. Remove redundant casts in the existing per-file **filter** adaptation,
  before pruning. Do not change projection adaptation, which must still return
  the requested output schema and metadata.
- **BIFROST-SCHEMA-001:** remove the `wyrd:sensitive` key, its writer, reader,
  required-marker check and fingerprint sensitivity byte. Keep `FieldClass`,
  sensitive-column lists and their authorization/projection consumers. Do not
  replace the removed marker with another metadata key or policy hash.
- The canonical physical fingerprint advances from encoding version 2 to 3.
  It continues to detect changes to ordered names, physical types, nested
  shape, nullability and remaining semantic metadata; registered field IDs
  remain excluded. Its public description field retains its existing shape,
  with the new digest. Catalog/user-schema fingerprints and WAL identity
  formats retain their existing meanings. The separate catalog fingerprint
  already excludes field metadata; do not change it to accommodate this fix.
  The canonical physical digest is recomputed by the registry/description and
  admission check; it is not a replacement for the durable WAL/fence digest.
- Existing tagged Parquet objects remain readable and benefit from the fix.
  No file rewrite, warehouse reset, SQL migration, old-fingerprint decoder or
  dual fingerprint mode is required. Do not alter acknowledged WAL bytes or
  stored fence identities. New admissions and their recovery must agree on
  the new canonical physical schema. Historical WAL may retain its original
  tagged Arrow schema as inert metadata; replay must preserve its original
  identity and rows. The WAL format and its schema-fingerprint algorithm do
  not change, so no old canonical-fingerprint decoder is introduced.
- Remove only casts directly over an ordinary scalar column where the exact
  Arrow datatype is unchanged and neither source nor target field has Arrow
  extension semantics. Cover the scalar types used by the canonical ledgers:
  booleans, signed/unsigned integers, floating point, strings, binary,
  fixed-size binary and timestamps. Timestamp unit/timezone and binary width
  must match exactly. Preserve nested, dictionary and extension/Variant casts,
  actual conversions, missing-column handling and their errors.
- Make this a generic value-preserving filter correction in the fork. The
  engine must not know the spelling `wyrd:sensitive`, delete file metadata or
  broadly treat all Arrow fields as equal. Permission and tenant checks,
  field-ID mapping, Variant identity and SQL NULL semantics stay intact.
- Fix the shared reader used by ordinary and distributed scans, including
  nested-pushdown planning. Keep the default expression adapter and existing
  distributed serialization. No custom adapter, codec, flag, dependency,
  query-specific workaround or UI change.

Loss of Bloom pruning does not imply that every row group is read: min/max
statistics may still exclude groups. The tests must distinguish those two
mechanisms. This task proves pruning, not a new latency SLO or benchmark.

## Approach

1. Add a failing production-scan regression with a tagged file and an untagged
   table schema. Extend the existing Oracle journey matrix with the same
   longer-list case before changing production behavior.
2. Correct redundant scalar filter casts in the fork's existing per-file
   reader. Verify the fix against tagged and untagged files and preserve real
   conversion, extension and projection behavior.
3. Remove sensitivity metadata at Bifrost's canonical schema owner and remove
   its fingerprint dependency. Advance the encoding and update the affected
   goldens, schema tests, Scribe checks and rustdoc without weakening structural
   or permission checks.
4. Integrate the fork through the existing shared Cargo pins and lockfile;
   resolve one DataFusion/Arrow universe. Do not hand-edit a Cargo cache or
   leave local path overrides in the delivered change. Git publication remains
   caller-owned; do not invent a revision hash for an unpublished change.
5. Run the focused regressions and existing SDK journeys through hot files,
   Iceberg promotion and Forge rewrite. Record RED, GREEN, resolved source
   revisions, pruning counters and exact returned rows. Update the owning
   Bifrost design's schema/Oracle sections with the removed-label and
   value-preserving-filter contracts; do not leave contrary owner text.

If the corrected fork has no remotely resolvable revision, first finish the
reviewable fork diff, Bifrost diff and local verification using a temporary
dependency override. Then hand the concrete fork change to the caller for the
separately authorized commit/publication step. Final pin/lock integration and
its checks resume with that actual revision. Until then delivery is blocked
on that external action, not reported complete; do not ask for publication
before the local change is concrete and reviewable.

## Ordered Implementation Scenarios

### Scenario 1 — Longer filters skip absent row groups, including tagged files

**Behavior.** Query a file whose field carries extra ordinary metadata while
the table field does not. An absent four-value `IN` list returns no rows and
skips the row group using its Bloom filter. Exercise both a historical
`wyrd:sensitive` tag and a neutral test metadata key so deleting the Bifrost
tag cannot conceal an unfixed reader. An untagged file is the control.

| Case | Required observation |
|---|---|
| Absent lists of 1, 3, 4, 8 and 32 values | Exact empty result and Bloom exclusion; no threshold-dependent loss. |
| Four-value list containing a match | Exact matching row identities, including duplicates in the input data. |
| Nullable column, `IN` containing NULL, `NOT IN` containing NULL | Exact SQL three-valued results; no false exclusion. Do not require Bloom skipping where SQL does not permit it. |
| `IS NULL` / `IS NOT NULL` on groups with known null counts | Preserve exact results and restore applicable null-count pruning despite ordinary metadata differences. |
| Actual type conversion; extension/Variant; nested null parent | Preserve conversion results/errors, extension identity and null behavior. |
| Output projection with differing metadata | Return the requested schema metadata unchanged by the filter correction. |

**RED.** Add the planned [production-scan regression](#v1--production-scan-regression)
and [fork regression](#v2--fork-regression). Use absent probes inside the
row group's min/max bounds, and verify the actual Bloom says each probe is
absent. Require zero statistics-pruned groups and a Bloom-pruned group, rather
than accepting empty results alone. Today the tagged four-value case must fail
the Bloom assertion. Add the four-value case to the existing [SDK journeys](#v3--sdk-pruning-journeys)
and record its baseline failure too, before the reader fix. A compile/setup
failure is not RED. Keep the tagged fixture after the fix.

**GREEN.** Remove redundant scalar casts from adapted filter expressions in
the existing per-file planner, under the datatype/extension restrictions
above. Preserve the default schema adapter for all real conversions and
output projections. Cover nested-pushdown's use of the same planner. Rerun
V1, V2 and V3 against the corrected fork; test cast preservation and exact
results as well as pruning metrics. The Bifrost label still exists at this
stage, proving the reader itself was fixed.

**REFACTOR.** Reuse the reader's existing expression traversal and test
facilities. Keep the correction with per-file predicate adaptation, without a
second optimizer, metadata transport scheme or Wyrd-specific engine rule.

### Scenario 2 — New canonical schemas have no sensitivity metadata

**Behavior.** Spans, records and points, their managed fields and every nested
child contain no `wyrd:sensitive` entry. They still ingest, describe and replay
with one canonical physical identity. Permission classes and metadata-only
projections select exactly the same columns as before.

Historical recovery is a separate regression obligation: replay an acknowledged
batch encoded by the pre-change writer, with its tagged schema and original
fence. Require identical row values, field IDs, batch identity and fence/digest
validation. Use a baseline-produced fixture; encoding it with the new writer
would not prove historical recovery. Existing accepted data is not restamped
merely to make the new canonical fingerprint match.

**RED.** Add the planned [schema regression](#v4--schema-and-fingerprint-regression).
Assert recursively that the registered canonical schema has no sensitivity
key and that the version-3 physical fingerprint can be computed without that
key. The current schema must fail the no-key assertion. Extend the golden
encoding expectation to the selected format before changing the encoder; its
failure must identify the old sensitivity-dependent encoding. Retain tests
for actual structural drift, field IDs, Variant identity and permission lists.

**GREEN.** Stop generating the tag at the canonical field owner. Remove its
constant, reader and sensitivity-byte encoding; advance to encoding version
3 and update the exact bytes/digest golden. Keep the canonical physical
identity check at Scribe admission. Keep the ledger's permission classes and
remaining metadata checks. Replace obsolete label assertions with no-label
assertions; preserve the intent of drift tests using remaining semantic
metadata where appropriate. Update inaccurate rustdoc and affected tests at
all consumers. Rerun Scenario 1 plus [identity/recovery regressions](#v5--existing-identity-and-recovery-regressions)
and the [description/admission journey](#v6--description-and-admission-journey).

**REFACTOR.** Delete now-unused sensitivity-metadata machinery. Do not replace
it with a ledger lookup during fingerprinting, a compatibility mode or a
second physical-schema identity.

## Acceptance Criteria

| Obligation | Owner and proof | Failure |
|---|---|---|
| BIFROST-BLOOM-001; REQ-039/061 | Fork per-file reader + Oracle: V1/V2 show tagged and untagged files retain Bloom skipping for long lists; V3 proves the actual SDK path before and after compaction. | Only results are correct, or min/max pruning conceals missing Bloom pruning. |
| BIFROST-SCHEMA-001; INV-004 | Canonical schema/fingerprint owner: V4 proves no tag recursively, a version-3 golden and unchanged permission classes. | Another marker or policy hash replaces the removed key, or a label is still emitted/required. |
| REQ-028; AC-018; INV-001/002 | Reader + Scribe: V1/V2/V5 retain NULL, type-conversion, field-ID, nested and Variant behavior; V6 describes and admits the new schema. | A necessary conversion, identity check, permission check or error is bypassed. |
| REQ-039; AC-022 | V3 returns the same row identities across Hot, Promoted and Rewritten cuts; existing tagged files work without mutation. | A rewrite/reset is needed to read old files, or a cut loses rows/pruning. |
| Existing dependency boundary | V7 resolves all patched crates to one fork revision; no path overrides or new adapter codec; V3 exercises distributed execution. | Mixed DataFusion sources, a cache edit or a local-only fix is delivered. |

This remediation contributes to the mapped Observe outcomes; it does not claim
to complete their UI, visual acceptance or latency obligations.

## Implementation Reference

Paths identify inspected owners, not an implementation allowlist.

- `tables/fields.rs`: `CanonicalField::to_arrow` emits the tag;
  `FieldClass::is_sensitive` still serves permission classification. Reuse the
  canonical declarations and delete metadata stamping rather than restating
  schemas in Oracle. `managed_columns.rs` and signal table tests contain
  affected recursive/tag assertions.
- `tables/mod.rs`: `DomainTable::schema`, `CanonicalPhysicalFingerprint`,
  `encode_field` and `ResolvedSchemaIdentity` own physical identity. Existing
  goldens and drift tests cover the encoder. `schema/fingerprint.rs` owns the
  separate metadata-independent catalog/buffer identities; preserve them.
- `scribe/execution_lanes.rs`: `enforce_canonical_physical_identity` checks the
  stamped batch. Its callers, canonical stamping and `scribe/replay.rs` must
  continue to agree. Do not remove the check to make tag removal pass.
- `catalog/bifrost_catalog.rs::describe_metadata` exposes the recomputed
  fingerprint through `wyrd-spec/src/vala/api.rs::BifrostTableDescription`.
  Keep that wire shape. No SDK export or generated contract change is needed.
- `architecture/bifrost-design.md`, declared-schema and per-file Oracle source
  sections: record the physical-schema metadata removal and predicate-only
  correction at the existing authority owner, without revising permission
  policy or the retained Iceberg schema's ownership.
- `oracle/exec.rs::hot_parquet_scan`, `fixture_hot_scan`, `pushed_filter` and
  `parquet_pruning` provide production execution and counters. Hot and published
  inputs share the scan owner. `oracle/nested_pushdown.rs::FileReadPlan::new`
  also uses `PerFileParquetReadPlanner`; inspect this caller when fixing it.
- `oracle/mod.rs::authorize_payload_columns` uses table/column permission
  policies, not this metadata. Preserve this boundary.
- `wyrd-testing/tests/bifrost/oracle/distributed.rs`: existing filtering
  journeys and `FilteringFixture::run_signal_matrix` cover Hot, Promoted and
  Rewritten cuts. Current built-in Bloom assertions are equality-based;
  the policy-list coverage has only three values. Extend these owners.
- Fork `bohmian-ai/datafusion`, pinned baseline
  `b669163b78a7692a9a381d4d13dceeae632c5c34`:
  `datafusion/datasource-parquet/src/opener/mod.rs::PerFileParquetReadPlanner`
  adapts predicate and projection separately. This is the existing correction
  point. `physical-expr-adapter/src/schema_rewriter.rs` inserts field-target
  casts; its projection metadata contract must remain intact.
  `physical-expr/src/expressions/cast.rs` has metadata-sensitive Variant
  conversions, so equal datatype alone is not a sufficient removal rule.
- Root `Cargo.toml` patches all DataFusion owners to the same fork;
  `Cargo.lock` closes the dependency change. Use an editable fork checkout
  based on the pinned revision, preserving unrelated local work. The adjacent
  checkout observed during planning is at `ab03f683...`, not the pin, and has
  a modified `datafusion-testing` submodule; do not reset or use it blindly.

## Verification and Evidence

Commands run from the Wyrd root, sequentially. Planned test names below are
not existing tests: after adding each, confirm its actual selector with
nextest listing before RED. Record any equivalent selector correction. Fork
commands use `DATAFUSION_FORK`, an absolute path to the editable fork checkout;
set it for the implementation environment. Never build from Cargo's cache.

### V1 — Production-scan regression

Planned inline test in the existing `vala-bifrost-redux` library target:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::metadata_only_schema_differences_preserve_pruning)'
```

Expected: a behavioral RED at four values; GREEN includes counters, exact rows,
NULL cases, null-count pruning and tagged/untagged controls.

### V2 — Fork regression

Planned inline test in the existing `datafusion-datasource-parquet` library:

```bash
mise exec -- cargo nextest run --manifest-path "$DATAFUSION_FORK/Cargo.toml" --locked -p datafusion-datasource-parquet --lib -E 'test(=opener::tests::metadata_only_filter_casts_preserve_pruning)'
```

Expected: cast-free ordinary scalar filters, restored pruning and unchanged
projection metadata, actual conversions and extension/Variant behavior. Use
the fork's existing fixtures; no new test framework is required.

### V3 — SDK pruning journeys

Existing target, profile and Postgres setup, with new assertions:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::hot_filtering_mechanisms_cover_all_table_kinds) | test(=distributed::iceberg_filtering_mechanisms_cover_all_table_kinds)'"
```

Expected: exact rows and positive `oracle_query_row_groups_pruned_bloom_total`
for the longer absent list at each cut, using probes whose absence is proven
by each relevant Bloom. Other pruning must not stand in for that assertion.

### V4 — Schema and fingerprint regression

Planned no-label test plus the existing exact golden:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::tests::canonical_schemas_do_not_carry_sensitivity_metadata) | test(=tables::tests::canonical_physical_fingerprint_bytes_are_stable)'
```

Expected: recursive no-label RED, then untagged schemas and stable version-3
bytes/digest. Update all affected label tests, not just these selectors.

### V5 — Existing identity and recovery regressions

Include the planned historical-WAL regression, produced from the pre-change
writer, alongside the existing tests:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::tests::resolved_identity_rejects_normalized_physical_drift) | test(=tables::tests::stamped_batches_carry_the_registered_field_ids) | test(=scribe::replay::tests::nested_accepted_subset_replays_one_fence_and_digest) | test(=scribe::replay::tests::tagged_wal_replays_without_identity_changes) | test(=oracle::exec::tests::struct_fields_of_null_parents_are_null_with_and_without_pruning)'
```

Expected: retain each invariant while removing only obsolete label checks;
drift tests must still detect real shape/remaining semantic metadata changes.
Historical replay must retain rows and original acknowledged identity without
rewriting its WAL or accepting a mismatched fence. This is preservation proof,
not new recovery behavior requiring an artificial RED.

### V6 — Description and admission journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=describe::described_canonical_and_dynamic_schemas_reach_exact_physical_schema)'"
```

Expected: SDK description, canonical ingestion and exact readback agree on the
new physical schema/fingerprint; dynamic-table behavior remains unchanged.

### V7 — Affected owners and static closure

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^tables::/) | test(/^scribe::execution_lanes::tests::/) | test(/^schema::fingerprint::tests::/) | test(/^oracle::exec::tests::/)'
mise exec -- cargo nextest run --manifest-path "$DATAFUSION_FORK/Cargo.toml" --locked -p datafusion-datasource-parquet --lib
mise exec -- cargo fmt --manifest-path "$DATAFUSION_FORK/Cargo.toml" --all -- --check
mise exec -- cargo clippy --manifest-path "$DATAFUSION_FORK/Cargo.toml" -p datafusion-datasource-parquet --all-targets --all-features -- -D warnings
mise run fmt:check
mise run lints
mise run check:deps
mise run codegen:check
mise exec -- git diff --check
```

Static/documentation edits and dependency pin alignment do not require a
manufactured RED. Inspect the resolved dependency graph and both diffs; record
the fork revision and show one shared DataFusion/Arrow stack. No generated
wire change is expected. Follow the fork's additional repository-required
checks before a separately authorized commit or publication.

## Material Stop Conditions

- Removing the key requires changing catalog/user-schema or WAL/fence identity
  formats, mutating acknowledged data, a migration or a reset. Do not silently
  turn the fingerprint cleanup into persistent-data repair.
- A proposed cast removal changes values, errors, extension identity or output
  metadata, or requires broad nested-type normalization/custom codecs. Keep
  required conversions; report the specific conflicting case.
- The fork cannot be integrated as one reproducible revision without changing
  another dependency contract. Local verification may use a temporary override,
  but it cannot be the delivered solution or justify an invented pin.

## Authority Links

- [Approved specification revision 26](../../spec.md), supplemented only by the
  caller's explicit removal/cast-fix instruction recorded above.
- [Repository rules](../../../../../AGENTS.md).
- [Bifrost schema ownership](../../../../../architecture/bifrost-design.md#declared-types-and-the-registered-schema)
  and [per-file source authority](../../../../../architecture/bifrost-design.md#immutable-planning-and-source-authority).
- [Spec-driven development](../../../../spec-driven-development.md),
  [implementation execution](../../../../implementation-execution.md) and
  [testing](../../../../../TESTING.md).
