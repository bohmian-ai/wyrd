---
id: TASK-001-R7
kind: remediation
status: ready
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
