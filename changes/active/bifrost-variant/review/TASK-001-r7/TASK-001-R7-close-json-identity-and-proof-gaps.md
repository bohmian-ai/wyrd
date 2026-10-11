---
id: TASK-001-R7
kind: remediation
status: review
spec: SPEC-bifrost-variant
spec_revision: 13
requirements: [REQ-003, REQ-004, REQ-006, REQ-011, REQ-019, INV-002, INV-007, AC-001, AC-003, AC-005, AC-008]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-18, FIND-TASK-001-24, FIND-TASK-001-25, FIND-TASK-001-27, FIND-TASK-001-28]
---

# Close JSON traversal, Variant identity, and proof gaps

## Authority and immutable subject

- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13.
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`.
- Reviewed base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`.
- Reviewed candidate: `6147cc617d81f2c03464043be698ab2565e9d745`.
- Reviewed tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`.
- Validated ledger: `changes/active/bifrost-variant/review/TASK-001-r7/findings-validation.md`.
- Root-cause ledger: `changes/active/bifrost-variant/review/TASK-001-r7/root-cause.md`.

Implement this packet with `$wyrd-implement`, then submit the complete
base-to-new-candidate task for `$wyrd-task-review`. Every product and ownership
decision is resolved; no specification revision is required.

## Outcome

JSON admission classifies hostile depth in work proportional to the supplied
text, sizes only accepted members, and preserves the locked error order; raw
byte rendering reuses one validated value; existing-table reconciliation keeps
Variant distinct from its storage Struct; the two required failure journeys
cross their real state boundaries; and the active authority and cumulative
evidence state only what the candidate proves.

## Correction root 1 — one bounded JSON traversal (`FIND-TASK-001-18`)

### Diagnosis

`EncodedVariant::from_json_text` delegates accepted containers to `append_raw`.
Once nesting passes 64, `scan_numbers` deserializes every pending container's
complete remaining `RawValue`; a one-child chain therefore scans successively
shorter copies of the same suffix and performs depth-times-size work. Buffered
writes and Oracle `parse_json`/`try_parse_json` share this synchronous owner, so
a compact valid tenant value can hold an ingest or DataFusion executor thread
for quadratic work, and a query deadline cannot preempt that computation.

This is one incomplete closure of prior finding 18, not separate Oracle and
ingest defects. Per-caller timeouts or guards would duplicate responsibility
and leave sibling consumers exposed.

### Decision-complete recommendation

Keep `EncodedVariant` as the sole JSON conversion owner and `serde_json` as the
sole syntax and number authority. Replace the repeated nested-suffix
deserialization with one private iterative traversal of the syntax-validated
text that consumes each token/span once, records exact numeric tokens and JSON
pointers, and builds only members within the accepted depth. Preserve
last-key-wins object normalization, exact i64/u64 handling, finite-double
meaning, malformed/numeric/depth selection, encoded-size precedence, and
`try_parse_json` mapping only invalid JSON to null.

Add no downstream timeout, second semantic model, public parser, public type,
configuration, or dependency solely for this correction.

### Focused closure proof

- Retain exact classification at depths 64, 65, 128, 129, and 10,000 through
  direct conversion and the existing Oracle test.
- Add deterministic test-only token or byte visitation evidence demonstrating
  proportional work for 1,000- and 10,000-level compact inputs; do not use
  wall-clock thresholds.
- Retain malformed deep input, numeric-below-depth, `try_parse_json`, exact
  integer, duplicate-key, and `3.0` behavior.

## Correction root 2 — actual-built JSON size (`FIND-TASK-001-25`)

### Diagnosis

When `append_raw` encounters a refused number it records the numeric violation
but appends a Variant null, and an object also retains that rejected member's
field metadata. `from_json_text` sizes this temporary encoding before choosing
the recorded violation. A valid accepted portion just below 8,388,608 bytes can
therefore become `WYRD_VALA_413_VARIANT_TOO_LARGE` only because the refused
number manufactured bytes, contradicting the approved rule that rejected
numeric and skipped over-depth content contribute none.

### Decision-complete recommendation

At the existing JSON builder owner, record a refused number without appending a
value or object member. Do not change accepted JSON null handling, member
ordering, traversal order, the builder abstraction, or the fixed size ->
invalid -> numeric -> depth selection. This is the remaining root of prior
finding 25; no renderer or caller guard is needed.

### Focused closure proof

- Add a boundary case whose accepted members remain within the limit but whose
  current rejected-number placeholder crosses it; assert the exact numeric
  error and pointer.
- Retain the case whose accepted bytes alone exceed the limit and assert that
  size still wins over numeric or depth.

## Correction root 3 — reuse one raw validated value (`FIND-TASK-001-27`)

### Diagnosis

`EncodedVariant::validate` performs Wyrd's raw scan and a complete upstream
`Variant::try_new`, then `variant_bytes_to_json` immediately repeats the same
upstream construction before rendering. Every byte-result adapter therefore
pays duplicate dependency validation, and the new shared gate discards the
validated value that its renderer needs.

### Decision-complete recommendation

Make the existing private borrowed validation gate return the validated
dependency `Variant` it already constructs. Parse `VariantMetadata` once, pass
it through the existing Wyrd scan and upstream validation, let byte rendering
consume the returned value, and let constructors or non-rendering guards
discard it. Preserve size-first behavior, depth rejection before upstream
recursion, panic containment, and the single public `EncodedVariant`; add no
public validated-borrow wrapper, second renderer, or second validation owner.

### Focused closure proof

Render one accepted nested object through `variant_bytes_to_json` and retain
the existing malformed, hostile-depth, overlap, duplicate-name, decimal, and
non-finite refusal cases through the same gate.

## Correction root 4 — preserve physical Variant identity (`FIND-TASK-001-28`)

### Diagnosis

Existing-table reconciliation converts the current Iceberg schema and compares
field name, nullability, and Arrow `DataType`, including recursive storage
children, but never compares Variant extension identity. A canonical Variant
and an ordinary `Struct<metadata: Binary, value: Binary>` therefore compare
equal even though only one is the logical Iceberg Variant required by REQ-003.
Incoming batch validation is a separate owner and does not protect this catalog
reconciliation path.

### Decision-complete recommendation

Extend the existing field-level recursive physical comparator to require
canonical Variant parity through `wyrd_queue::variant::is_variant` at every
field before applying its existing binary/string/list/timezone normalizations.
Preserve field-ID exclusion, nested name and nullability comparison, approved
aliases, v3-only refusal, and the no-migration decision. Add no second schema
comparator, compatibility type, or migration path.

### Focused closure proof

- Reject Variant versus its plain storage Struct in both directions at the top
  level and nested under Struct/List.
- Retain the accepted timezone/width normalizations and pinned Iceberg Variant
  round trip.

## Correction root 5 — required boundary proofs (`FIND-TASK-001-24`, `-3`)

### OTLP record-local numeric refusal

The shared projector now refuses a log, span, or metric record containing a
non-finite Variant number while retaining valid siblings, but the new proof is
only an in-process log projector test. Extend the existing log Variant OTLP
journey—without production changes or a new harness—with one non-finite body
and one valid sibling. Assert the real collector partial-success result carries
`WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`, publish the batch, and query the
existing scope to prove exactly the valid sibling persisted. One log journey
is sufficient because span and metric attributes use the same shared numeric
owner and retain their narrow projector coverage.

### Failed lineage and no commit

The managed rewrite journey proves repeated successful v3 lineage preservation
and garbage collection, while the fork unit proves only the lineage predicate;
the original task also requires a missing/null lineage batch to fail before a
commit-capable handoff. Extend the existing managed-rewrite/fork test seam to
deliver at least one valid batch followed by one missing or null lineage batch,
then exercise the existing managed-attempt observer and publication boundary.
Assert failure, unchanged snapshot and logical rows, no successful or
commit-capable handoff, and complete accounting of any opened output as a
reclaimable possible output. Add no public injection hook, alternate handoff,
or production guard unless this proof falsifies the traced implementation.

## Correction root 6 — truthful authority and complete evidence (`FIND-TASK-001-4`)

### Complexity authority

Keep `object_field_slots` unchanged: sorting offsets is the smallest safe
implementation for unordered input under the fixed 8 MiB limit. Replace the
linearity claims in `architecture/bifrost-design.md`, the immediate owner
rustdoc, and the R6 implementation evidence with the actual approved property:
validation is iterative, fixed-size-bounded, and rejects shared or overlapping
regions so rendering work cannot be amplified. Add no bitmap, interval tree,
alternate scanner, or performance configuration merely to preserve accidental
prose.

### Exact focused evidence

Run and append explicit package, target, features, exact
`test(=fully::qualified::name)` selectors, selected counts, and exits for every
named new or modified R6 Rust test:

- `raw_shared_field_values_are_refused`
- `renderers_refuse_hostile_stored_variants`
- `json_depth_is_decided_by_wyrd_at_any_depth`
- `raw_numbers_outside_the_json_domain_are_refused`
- `json_size_outranks_numeric_and_depth`
- `raw_repeated_field_names_are_refused`
- `maximal_log_projection_preserves_body_context_and_presence`
- `unresolved_call_nulls_resolved_model_children`

Use the repository Postgres wrapper only where the selected test requires it.
Retain the broader library and journey evidence; add no script or checker.

### Cumulative whitespace gate

Delete only the extra terminal blank line at
`changes/active/bifrost-variant/review/TASK-001-r6/standards-review.md:88`,
then run and record:

```bash
git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..HEAD
```

## Preserved behavior and non-goals

- Preserve one `EncodedVariant` owner, one raw gate, one table-validation seam,
  one renderer contract, and the derive-backed error catalog.
- Preserve exact i64/u64 values, accepted finite IEEE values including `3.0`,
  JSON last-key-wins normalization, missing/null distinction, raw object
  canonicality, extension identity, and size/depth limits.
- Preserve Scribe WAL/ACK ordering, OTLP record-local partial success, Iceberg
  v3 and lineage/GC behavior, Bloom sizing, Forge authority, Oracle session and
  distributed terminal behavior, SDK collection, RBAC, tenancy, and sensitive
  payload rules.
- Add no public API, configuration, migration, compatibility schema, new JSON
  syntax authority, per-language validator, second renderer, timeout guard,
  public test hook, TASK-002/TASK-003 ownership change, or finding-13 revival.
- Non-finite raw/OTLP refusal is approved behavior; this task proves it and does
  not reopen the product decision.

## Acceptance map

| Finding | Done when |
|---|---|
| `FIND-TASK-001-18` | Deep valid JSON is classified by one proportional traversal shared by writes and Oracle, with deterministic work evidence. |
| `FIND-TASK-001-25` | Refused numeric members contribute no encoded bytes, while accepted oversized bytes still make size win. |
| `FIND-TASK-001-4` | Authority, owner rustdoc, and R6 evidence state the implemented bounded/non-amplifying property without claiming linearity. |
| `FIND-TASK-001-27` | Byte rendering consumes the dependency value returned by the one raw validation gate and performs no second full validation. |
| `FIND-TASK-001-24` | The real log OTLP journey proves numeric refusal, sibling survival, publication, and query. |
| `FIND-TASK-001-28` | Physical reconciliation rejects top-level and nested Variant/storage-Struct mismatches in either direction. |
| `FIND-TASK-001-3` | A missing/null lineage rewrite cannot publish or expose a commit-capable handoff, and possible outputs remain reclaimable. |
| Completion cleanup | Every named new or modified R6 Rust test has a recorded exact selector, count, and zero exit. |
| Completion cleanup | The cumulative base-to-candidate `git diff --check` exits zero. |

## Verification

Run each new or modified Rust test with its exact final name through
`mise exec -- cargo nextest run --locked`, including the owning package, target,
features, and repository-managed environment wrapper when required. Run the
narrowest existing Bifrost lanes covering `wyrd-queue`, Oracle Variant SQL,
OTLP logs, physical catalog reconciliation, and managed Forge rewrite. Then
run:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:skills-sync
mise run docs:check
git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..HEAD
```

Record every command, selected test count, and exit status in this file before
returning the task to review.

## Implementation evidence

Commits: `d66f3e741`, `11fc2d00d`, `9cb5ca4d3`, `ce2ec6e00`, `b5aacc424`,
`dbec11312`, `4abfd3ccd`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-001-18` one proportional traversal | `crates/shared/wyrd-queue/src/variant.rs`: serde_json `RawValue` stays the syntax authority; `JsonTree::split` indexes spans in one byte scan, and `append_raw`/`scan_numbers` walk it iteratively; shared by writes and Oracle `parse_json` through `EncodedVariant::from_json_text` | `variant::tests::json_walk_reads_each_byte_once` (each byte is read at most once at depth 1,000), `json_depth_is_decided_by_wyrd_at_any_depth` | PASS |
| `FIND-TASK-001-25` refused numbers add no bytes | A refused number only records the violation; upstream `ObjectFieldBuilder` inserts nothing without an append, and size is taken from the actual `try_finish` output | `variant::tests::refused_numbers_add_no_size`, `json_size_outranks_numeric_and_depth` | PASS |
| `FIND-TASK-001-4` truthful bound | `architecture/bifrost-design.md`, the `EncodedVariant::validate` rustdoc and the R6 evidence state iterative, fixed-size-bounded, non-amplifying validation; linearity is no longer claimed; `object_field_slots` is unchanged | `mise run docs:check` | PASS |
| `FIND-TASK-001-27` one raw validation | `EncodedVariant::validate` returns the `Variant` from its single `Variant::try_new_with_metadata`; `variant_bytes_to_json` renders that value; `sized` is removed | `variant::tests::renderers_refuse_hostile_stored_variants`, `raw_shared_field_values_are_refused` | PASS |
| `FIND-TASK-001-24` OTLP journey | `crates/wyrd/wyrd-testing/tests/bifrost/otlp/logs_export.rs`: the existing log Variant journey sends a NaN body and a valid sibling; the collector's partial success carries `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`; after publish the query returns only the sibling | `logs_export::pg_tests::log_variant_body_attributes_and_promotions_are_queryable`, `test:bifrost:journey:otlp` | PASS |
| `FIND-TASK-001-28` physical Variant identity | `tables::field_layout_matches` (Variant parity, nullability, recursive shape) is the one comparator; `bifrost_catalog::field_shape_matches` reuses it | `catalog::bifrost_catalog::schema_shape_tests::schema_shape_keeps_variant_identity_at_any_depth` (top level, struct child, list element, both directions) | PASS |
| `FIND-TASK-001-3` missing lineage, no commit | The existing rewrite seam gains `RewriteOutputBreak::UnassignRowIds`, which serves manifest lists with no `first_row_id`, so the real reader yields null lineage; `refuse_rewrite_without_lineage` runs the managed attempt | `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite`: fails on the lineage check, no output opened, no possible outputs, zero catalog commits, live set and every row's lineage unchanged | PASS (see limits) |
| Exact R6 selectors | — | Commands table below | PASS |
| Cumulative whitespace | `TASK-001-r6/standards-review.md` trailing blank line removed | `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..HEAD` exit 0 | PASS |

### Commands

`CARGO_TARGET_DIR=<repo>/target` is set for every command. `PG` means
`scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && …"`
(`db:migrate:inner` for `wyrd-testing`).

| Command | Selected | Exit |
|---|---|---|
| `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::raw_shared_field_values_are_refused) \| test(=variant::tests::renderers_refuse_hostile_stored_variants) \| test(=variant::tests::json_depth_is_decided_by_wyrd_at_any_depth) \| test(=variant::tests::raw_numbers_outside_the_json_domain_are_refused) \| test(=variant::tests::json_size_outranks_numeric_and_depth) \| test(=variant::tests::raw_repeated_field_names_are_refused) \| test(=variant::tests::refused_numbers_add_no_size) \| test(=variant::tests::json_walk_reads_each_byte_once)'` | 8 passed | 0 |
| `mise exec -- cargo nextest run --locked -p wyrd-queue --lib` | 70 passed | 0 |
| `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::logs::tests::maximal_log_projection_preserves_body_context_and_presence)'` | 1 passed | 0 |
| `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::bifrost_catalog::schema_shape_tests::schema_shape_keeps_variant_identity_at_any_depth)'` | 1 passed | 0 |
| `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::capture::tests::unresolved_call_nulls_resolved_model_children)'` | 1 passed | 0 |
| `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::analytical::tests::follower_graph_release_waits_for_children_and_retains_cleanup_failure)'` | 1 passed (failed before `4abfd3ccd`) | 0 |
| `PG mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib` | 848 passed | 0 |
| `PG mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(/^forge::managed_rewrite::/)'` | 10 passed, including `v3_row_lineage_survives_repeated_rewrite` | 0 |
| `PG mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E 'test(=logs_export::pg_tests::log_variant_body_attributes_and_promotions_are_queryable)'` | 1 passed | 0 |
| `mise run test:bifrost:journey:otlp` | 14 passed | 0 |
| `PG mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::iceberg_filtering_mechanisms_cover_all_table_kinds)'` | 1 passed | 0 |
| `mise run test:bifrost:journey:oracle` | 50 passed | 0 |
| `mise run fmt` | — | 0 |
| `mise run lints` | — | 0 |
| `mise run codegen:check` | — | 0 |
| `mise run check:skills-sync` | — | 0 |
| `mise run docs:check` | — | 0 |
| `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..HEAD` | — | 0 |

### Failure diagnoses

**Oracle filtering journey**
- Symptom: `distributed::iceberg_filtering_mechanisms_cover_all_table_kinds` failed with "the row group holding key 50000 must carry several key pages, saw 1". It failed on every run.
- Evidence: the trace shows the Rewritten cut's row groups at about 17,000 rows. The Hot and Promoted cuts held 45,000-row groups.
- Cause: v3 rewrites now carry a unique `_row_id` as well as `key_id` and `score`. Each unique column counts its uncompressed dictionary toward the size estimate, so the 512 KiB `REWRITE_ROW_GROUP_BYTES` closed each group before it reached the 20,000-row page limit. 1 MiB also failed, producing a single 135,000-row group: once a dictionary reaches the 256 KiB page limit it falls back to plain encoding, and the estimate turns compressed.
- Fix site: the fixture constant only; the shared writer recipe is correct. At 768 KiB the groups close near 26,000 rows. Fresh diagnostician report recorded; its 1 MiB suggestion was falsified by the run above.

**Oracle held-cut journey**
- Symptom: `distributed::held_cut_owns_active_reads_until_all_descendants_settle` failed intermittently, in one full-lane run only, with `attempts_active: 1.0` left after drop.
- Evidence: the WARN "follower could not settle a closed graph … still owns a live attempt", and the ingress refusing SetPlan with `reason=ownership`.
- Cause: `GraphLease::settle` copied its live attempts without calling `supervisor.begin_draining`, so a SetPlan that had already resolved the lease could still pass `spawn_attempt`'s Active check. That attempt was never joined.
- Fix site: `GraphLease::settle`, which now drains first, as the leader (`analytical.rs` leader settle) and shutdown paths already do. Every follower settlement route goes through it. Fresh diagnostician report recorded.
- Deterministic regression: a lease resolved before a failed settle must refuse admission.

### New items

| New item | Owners searched | Why new |
|---|---|---|
| `JsonTree`, `JsonNode` (private) | `EncodedVariant`, serde_json `RawValue`/`Value`, the parquet-variant builders | serde_json has no span tree; `Value` re-materialises and loses exact numbers. One private index is what lets the build make a single pass |
| `WALK_READ_BYTES` (test-only thread-local) | the existing variant tests | Gives deterministic work evidence without timing |
| `tables::field_layout_matches` | `arrow_type_shape_matches`, `equals_datatype`, `is_variant` | The one field comparator; the catalog reuses it instead of keeping its own |
| `assert_variant_reason`, `VARIANT_NUMERIC_OUT_OF_RANGE`, `VARIANT_NON_FINITE_LOG_SCOPE` | the OTLP `support.rs` assertions | Shared by the logs, metrics and trace journeys instead of three copies |
| `RewriteOutputBreak::UnassignRowIds`, `is_manifest_list`, `without_row_id_assignment` | the existing rewrite seam `RewriteOutputBreak`, iceberg `ManifestListWriter` | Extends the existing seam; manifest lists are rewritten with iceberg's own v3 writer |
| `refuse_rewrite_without_lineage` | `v3_row_lineage_survives_repeated_rewrite` and its fixtures | The failure phase of the existing journey, reusing `PromotedRewriteFixture`, `CountingObjectStore` and `LineageTable` |

### Non-goals confirmed

- No public API, configuration, migration, compatibility schema, new JSON syntax authority, per-language validator, second renderer, timeout guard, public test hook, TASK-002 or TASK-003 change, or finding-13 revival.
- `object_field_slots` is unchanged.
- The two Oracle fixes stay inside their owners: a test constant, and one fence in `GraphLease::settle`.

### Limits

- `FIND-TASK-001-3` requested a valid batch followed by a null-lineage batch. Iceberg's v3 `ManifestListWriter` assigns a `first_row_id` to every unassigned data manifest, so a v3 snapshot never mixes files with and without lineage. A stripped manifest list nulls `_row_id` and `_last_updated_sequence_number` for every file, because `first_row_id` gates even lineage that is physically present.
- The proof therefore drives an all-null attempt through the real reader and the fork's per-batch check. It fails before any output opens, so complete accounting of opened outputs is shown as zero opened and zero possible outputs. Settled-output reclaim stays proven by the existing `managed_rewrite_failure_preserves_attempt_global_possible_outputs`.
