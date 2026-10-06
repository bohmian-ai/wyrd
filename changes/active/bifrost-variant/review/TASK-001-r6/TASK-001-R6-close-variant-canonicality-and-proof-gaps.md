---
id: TASK-001-R6
kind: remediation
status: ready
spec: SPEC-bifrost-variant
spec_revision: 13
requirements: [REQ-003, REQ-004, REQ-006, REQ-008, REQ-009, REQ-019, INV-002, INV-007, AC-003, AC-005]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-4, FIND-TASK-001-14, FIND-TASK-001-16, FIND-TASK-001-18, FIND-TASK-001-21, FIND-TASK-001-24, FIND-TASK-001-25, FIND-TASK-001-26]
---

# Close Variant canonicality and proof gaps

## Authority and immutable subject

- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13.
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`.
- Reviewed base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`.
- Reviewed candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`.
- Reviewed tree: `e3c59991994125dac41187f17de12935e2b45424`.
- Validated ledger: `changes/active/bifrost-variant/review/TASK-001-r6/findings-validation.md`.
- Root-cause ledger: `changes/active/bifrost-variant/review/TASK-001-r6/root-cause.md`.

Implement this packet with `$wyrd-implement`, then submit the complete
base-to-new-candidate task for `$wyrd-task-review`. The approved behavior and
correction boundaries below are complete; no specification decision remains.

## Outcome

Every successfully constructed or admitted Variant is canonical, exact, and
bounded for every renderer; JSON and raw Arrow select the same locked failures;
the nullable gateway shape has its required boundary/publication proof; and the
active task packet and architecture describe the shipped behavior accurately.

## Correction group 1 — JSON admission (`FIND-TASK-001-18`, `-25`)

### Diagnosis

`EncodedVariant::from_json_text` first asks recursive `serde_json` to parse the
whole document under serde's unrelated recursion ceiling, so valid JSON can be
reported as invalid before Wyrd applies its 64-container and numeric rules;
`from_json` likewise serializes a programmatic `Value` recursively before the
Wyrd boundary. Separately, after building accepted siblings, the same owner
selects recorded numeric/depth failure before checking whether the actual
encoded bytes already exceed 8,388,608 bytes.

These paths feed buffered writes and Oracle `parse_json`/`try_parse_json`.
They can therefore return the wrong stable code, make `try_parse_json` suppress
a valid-but-too-deep value, miss a numeric error below serde's ceiling, or let
recursive library work outrun Wyrd's bounded decision.

### Decision-complete recommendation

Keep `EncodedVariant` as the sole JSON conversion owner and keep serde_json as
the syntax authority. Parse text using serde_json's unbounded-depth mode under
the already-resolved `stacker` package so parsing has bounded stack growth;
then apply the existing non-building Wyrd depth/numeric scan and
`VariantViolations` selection. Add the direct workspace/crate declaration and
serde_json feature needed to use that already-resolved mechanism, but add no
new parser, public API, configuration, or package version.

For `from_json(Value)`, perform Wyrd's existing explicit-stack depth preflight
before recursive serialization, then delegate to the same text owner; do not
create a second encoder or semantic model.

After building the accepted portion of valid JSON, evaluate the actual
metadata/value bytes with the existing encoded-size owner before selecting a
recorded numeric or depth violation. This correction covers only bytes that
were deterministically built; do not invent a hypothetical encoded size for a
rejected numeric or skipped over-depth subtree.

The required order remains: encoded size, invalid JSON, numeric range, then
depth. Preserve last-key-wins normalization, exact i64/u64 handling, finite
IEEE floating meaning, and `try_parse_json` null only for invalid JSON.

### Acceptance and focused proof

- Directly exercise text and programmatic constructors at depths 64, 65, 128,
  and a substantially deeper compact value; malformed deep input remains
  invalid, valid deep input is too-deep, and numeric-under-deep is numeric.
- Exercise Oracle `parse_json` and `try_parse_json` plus the existing pre-ACK
  JSON journey with the same classes, exact codes/details, no retained row,
  and a successful following operation.
- Combine one already-oversized valid sibling with an out-of-range integer and
  with an over-depth sibling through direct conversion and the pre-ACK journey;
  both return `WYRD_VALA_413_VARIANT_TOO_LARGE`.
- Retain isolated size, invalid, numeric, depth, duplicate-key, and `3.0`
  behavior.

## Correction group 2 — raw Variant canonicality and bounded rendering (`FIND-TASK-001-16`, `-24`, `-26`)

### Diagnosis

The existing borrowing scan remains incomplete in three independent ways:

- its visited-node count rejects the demonstrated exponential shared-child
  fixture but permits shallow objects whose many fields repeatedly reference a
  large shared/overlapping child region, producing superlinear render work;
- Decimal4, Decimal8, and non-finite Float/Double bypass revision 13's exact
  numeric domain; and
- equal resolved object names pass in unsorted metadata because equality is
  accepted, allowing lookup and JSON rendering to select or overwrite
  different occurrences.

Additionally, `variant_bytes_to_json`, `variant_cell_to_json`,
`VariantJsonEncoderFactory`, and `mask_placeholders` accept raw bytes or Arrow
arrays and enter upstream recursive validation/rendering without first applying
Wyrd's bounded scanner. The defect is therefore at the shared raw Variant
owner, not in SDKs, Oracle, or individual renderers.

### Decision-complete recommendation

Extend the existing borrowing raw-Variant scanner as the one canonicality and
resource-safety gate:

- require every object/list child to own one non-overlapping encoded value
  region; any shared or overlapping child region is malformed;
- reject Decimal4 and Decimal8 as numeric kind `decimal`;
- retain only the approved scale-zero Decimal16 interval
  `i64::MAX + 1..=u64::MAX`;
- retain finite Float/Double and reject NaN or infinities as numeric kind
  `double`; and
- require resolved object names to be strictly increasing in both metadata
  modes, while preserving JSON/OTel last-occurrence normalization before raw
  encoding.

Malformed remains higher priority than numeric, and numeric remains higher
than depth through the existing `VariantViolations` selector. Replace the
node-count symptom fence with the non-overlap invariant rather than retaining
two competing resource guards.

All four raw renderer/query entry paths named above must invoke this same
borrowing scanner before dependency recursion. Reuse `EncodedVariant` as the
only owned validated value and the existing Arrow/Variant dependencies; add no
second parser, Variant model, renderer, downstream SDK guard, public type,
option, or error.

### Acceptance and focused proof

- Retain the existing exponential shared-child case and add a shallow
  many-fields/one-large-child case whose visited-node count is below the input
  byte length; both fail promptly as invalid JSON.
- Add a compact hostile-depth stored cell and exercise the shared byte renderer
  plus one Arrow JSON-encoder path, then render an ordinary value successfully;
  the shared owner must cover Rust, Python, TypeScript, HTTP, MCP, CLI, and
  Oracle consumers without per-consumer tests.
- Cover Decimal4, Decimal8, approved/refused Decimal16, finite Float/Double,
  and NaN/infinities in the constructor and existing raw-IPC journey. Refused
  values carry exact numeric kind, produce no ACK/no row, and do not prevent a
  following valid write.
- Cover valid unsorted metadata with two field IDs resolving to the same name
  in the constructor and raw-IPC journey; it returns
  `WYRD_VALA_400_VARIANT_INVALID_JSON`, retains no row, and leaves the server
  usable.
- Retain ordinary unique objects, exact `u64::MAX`, `3.0`, malformed-before-
  numeric-before-depth selection, and the 20,000-level hostile-depth proof.

## Correction group 3 — gateway boundary and publication proof (`FIND-TASK-001-14`)

### Diagnosis

`CallsTable::WHOLE_STRUCTS` reaches the common pre-ACK validator and
`CallCapture::calls_batch` produces correct absent children, so no current
production defect is established. The R5 task nevertheless required proof at
the distinct real server gateway boundary plus published child projection;
the candidate supplies only direct validator and producer unit tests.

### Decision-complete recommendation

Keep production unchanged unless the missing proof falsifies the traced source.
Extend the existing server-owned gateway capture peer test—without adding a
public raw gateway writer or new harness—to submit a calls batch with a partial
present `resolved_model`, assert the existing
`WYRD_VALA_400_SCHEMA_PARSE`, no retained row, and a successful following
capture. Extend the existing typed built-in journey to flush an unresolved
capture and query both `resolved_model` children as SQL null through the hot
and published path already owned by that journey.

### Acceptance and focused proof

- The gateway peer refuses partial `resolved_model` before ACK with the exact
  problem, retains no row, and accepts the following valid capture.
- An unresolved valid capture reads both nested children as SQL null after
  publication.
- Existing requested-model required-child schema and capture tests stay green.

## Correction group 4 — active task lifecycle (`FIND-TASK-001-21`)

### Diagnosis and recommendation

The R5 remediation contains completed implementation and final evidence and is
now under review, but its front matter still says `ready`. Change only its
status to `review`; leave its history and evidence intact and add no checker.

### Acceptance and focused proof

TASK-001 and TASK-001-R5 both identify the current review phase, and
`git diff --check` passes.

## Correction group 5 — active authority and owner documentation (`FIND-TASK-001-4`)

### Diagnosis

`architecture/bifrost-design.md` remains the active authority but omits the
revision-13 raw numeric domain and full malformed/numeric/depth selection, and
omits revision-12 physical-nullability plus whole-present/null-absent semantics
for verification summaries, metric buckets, and gateway `resolved_model`.
`validate_declared_variants` rustdoc also omits numeric-range failures.

### Decision-complete recommendation

Amend the existing Variant paragraph and built-in table bullets with those
already-approved rules, and add numeric-range to the existing validator error
documentation. Add no new document, history narrative, checker, or abstraction.

### Acceptance and focused proof

The active authority agrees with spec revision 13 and the final implementation;
the immediate validator rustdoc names every catalog error it can return;
`mise run docs:check` and `git diff --check` pass.

## Preserved behavior and non-goals

- Preserve one `EncodedVariant` owner, one table-validation seam, one Variant
  renderer contract, and the existing derive-backed error catalog.
- Preserve exact i64/u64 values, finite IEEE Float/Double meaning, `3.0`,
  last-key-wins normalization before encoding, missing/null distinction,
  extension identity, size/depth limits, logical fingerprints, and nullable
  physical leaves required by Parquet.
- Preserve Scribe WAL/ACK ordering, Iceberg v3/lineage/GC, Bloom sizing, Forge,
  Oracle session registration, distributed error transport, SDK collection,
  authorization, tenancy, and sensitivity behavior.
- Add no public API, configuration, migration, compatibility schema,
  arbitrary-precision terminal, shredding, new JSON parser, new renderer, or
  per-language validation.
- Do not move TASK-002/TASK-003 ownership or revive rejected finding 13.

## Cross-task overlap audit

The TASK-002 worktree was checked at `b02f8262dacdaceac74ba54957d915cee540fbe9`
and the TASK-003 worktree at `d4a0cba9744dd5237a4b8791c8c2eb37cb9b543f`,
including TASK-003's current uncommitted changes. Neither worktree implements or
proves any retained R6 finding, so no correction or acceptance proof in this
packet is removed.

- TASK-002 adds dynamic-table admission by routing values through
  `validate_declared_variants` and the TASK-001 `EncodedVariant` owner; it does
  not add a raw scanner, JSON parser, renderer guard, numeric-domain rule, or
  gateway proof. Rebase it after this remediation and preserve that reuse; do
  not duplicate these corrections in its prepared-input or SDK paths.
- TASK-003 changes `mask_placeholders` only to accommodate projected/shredded
  rows and otherwise owns shredding, physical projection, and pruning. Rebase
  it after this remediation and reconcile that edit with the canonical scanner:
  present unshredded metadata/value bytes still pass the TASK-001 gate, while a
  projected shredded row keeps TASK-003's typed-value handling. Do not restore
  the pre-R6 direct `Variant::try_new` path or add a second validator.
- TASK-003's uncommitted Scribe IPC/material-planning rewrite and Map field-id
  stamping do not cover R6 canonicality, error precedence, gateway publication,
  lifecycle, or authority documentation.

## Acceptance map

| Finding | Done when |
|---|---|
| `FIND-TASK-001-4` | Active Bifrost authority and validator rustdoc state the revision-12/13 rules actually enforced. |
| `FIND-TASK-001-14` | Existing gateway peer and published-query paths prove partial refusal, no row, continuation, and null children without production changes unless proof fails. |
| `FIND-TASK-001-16` | Shared/overlapping child ranges are inadmissible and every raw renderer applies the same bounded validation before recursive dependency work. |
| `FIND-TASK-001-18` | Valid deep text and programmatic JSON reach Wyrd's stack-safe depth/numeric decision before recursive serde work can decide or exhaust stack. |
| `FIND-TASK-001-21` | Implemented R5 remediation is marked `review`. |
| `FIND-TASK-001-24` | Raw input accepts only revision 13's exact decimal and finite floating domain before ACK. |
| `FIND-TASK-001-25` | Deterministically built oversized JSON returns size before recorded numeric/depth errors. |
| `FIND-TASK-001-26` | Raw objects require unique strictly ordered resolved names in either metadata mode. |

## Verification

Run every new or modified Rust test with its exact final name through
`mise exec -- cargo nextest run --locked`, with the owning package, target,
features, and repository-managed Postgres wrapper where required. Retain the
existing focused R5 tests and run the narrowest owning Bifrost journeys for raw
IPC admission, JSON admission, gateway capture/publication, Oracle strict and
lenient parsing, and affected Variant rendering.

Then run:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:skills-sync
mise run docs:check
git diff --check
```

Do not rerun unaffected SDK/MCP journeys solely for the server-owned raw
validation changes; the shared renderer and raw-IPC/Oracle proofs own those
paths. Record every command, selected test count, and exit status in this file's
implementation evidence.
