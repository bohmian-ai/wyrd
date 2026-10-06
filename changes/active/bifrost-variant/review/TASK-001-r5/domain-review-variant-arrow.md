# Variant / Arrow Domain Review

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 12
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Latest remediation range: `bb6ae8070..0e37748f3a27d3bcec4713e6210e97328e045886`

The candidate commit and tree matched the supplied immutable subject before
and after review.

## Reviewed Boundary

This review traced the TASK-001 logical Variant path from raw JSON or Arrow
Variant bytes through `EncodedVariant`, JSON-row batching, built-in Scribe
admission, Arrow extension identity, Oracle parsing/operators, and native/JSON
rendering. It specifically checked malformed bytes and panic containment,
depth and numeric failure precedence, exact i64/u64 and floating-point
semantics, top-level and nested extension metadata, SQL null versus Variant
null, and the reachable callers and sibling consumers of the shared helpers.

The review used CodeGraph first, then verified the resulting map against the
complete base-to-candidate diff, the remediation diff, current source, tests,
and the pinned dependency source.

## Authority and Source Coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Variant representation, limits, and failure order | Spec revision 12 locked Variant contract, REQ-003/004/019 and INV-007; `architecture/bifrost-design.md` Storage format and Variant; `AGENTS.md`; Arrow/OLAP references | **FAIL** — `VARIANT-ARROW-R5-001` |
| JSON token encoding | `wyrd-queue/src/variant.rs::from_json_text`, `append_raw`, `raw_number_variant`; `batch_builder.rs::collect_raw` and `build_variant_column`; direct and Oracle tests | Exact i64/u64 integers, `3.0`, duplicate-key last-wins, and object null-versus-missing behavior pass; one compound precedence case remains (`VARIANT-ARROW-R5-001`) |
| Raw encoded Variant validation | `EncodedVariant::from_bytes`, `check_depth`; built-in `validate_declared_variants` / `validate_variant_values`; pinned `parquet-variant` 59.3 constructors and recursive full validation | Hostile depth is bounded before dependency recursion and shallow panics are contained; full validity can still be masked by depth (`VARIANT-ARROW-R5-001`) |
| Arrow extension identity at every nesting level | `is_variant`, recursive `field_to_spec`, `holds_variant`, normalized schema comparison, raw-IPC journey cases | **PASS** — canonical name plus absent/empty metadata only; foreign metadata is refused at top-level and nested positions |
| Logical null behavior | `VariantColumnBuilder`, `mask_placeholders`, `VariantJsonEncoderFactory`, nullable verification Struct children, `variant_get`, `variant_as_text`, `to_json` | **PASS** — SQL null, Variant null, missing key, and null-parent placeholders retain their distinct required behavior |
| Oracle query semantics | `oracle/variant_sql.rs` planner, `variant_get`, dynamic/literal paths, `parse_json`, `try_parse_json`, `variant_as_text`, `to_json`; DataFusion 55.0 API behavior | **PASS** except the shared conversion precedence inherited from `EncodedVariant` |
| Result rendering and exact values | shared byte/cell renderers, Arrow JSON encoder, CLI journey, Rust/Python/TypeScript/MCP consumers | **PASS** — native Variant JSON, exact `u64::MAX`, and raw `3.0` spelling use the shared upstream renderer |
| Prior r1-r4 domain findings | prior Variant/Arrow reports, verdicts, validation, root-cause report, and R1-R4 remediation tasks, checked against current source | **PASS** for the previously identified crash-risk ordering, foreign extension metadata, sibling-branch numeric/depth ordering, CLI rendering proof, and dead wrapper removal; the finding below is a narrower still-reachable compound case |

## Material Proposed Findings

### VARIANT-ARROW-R5-001 — INCORRECT — the depth preflights hide higher-priority failures inside the first over-depth container

**Violated obligation.** Spec revision 12 fixes per-Variant failure precedence
as byte limit, JSON/extension validity, numeric range, then depth. Bifrost's
active design likewise requires raw values to be checked in size, encoding,
then depth order. The R4 remediation requires numeric range to outrank depth
independently of JSON traversal order while retaining one shared conversion
owner.

**Exact locations.** `crates/shared/wyrd-queue/src/variant.rs:154-164`,
`:619-684`, and `:766-790`; Oracle inherits the JSON behavior at
`crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:600-620`, and built-in
Arrow admission inherits the byte behavior at
`crates/vala/vala-bifrost-redux/src/tables/mod.rs:348-372`.

**Evidence.** Both bounded walks stop before inspecting the contents of the
first container at depth 65:

- `append_raw` records `TooDeep` and returns `Ok(())` immediately at lines
  628-634. It continues sibling traversal, which fixes the R4 two-branch/key-
  order case, but it never sees an out-of-range integer nested inside that
  over-depth container. For example, 65 list wrappers around
  `18446744073709551616` are valid JSON and remain far below the request and
  Variant byte ceilings, yet the shared constructor returns `TooDeep` instead
  of the higher-priority `NumericOutOfRange`. `parse_json` and JSON-row writes
  call the same constructor, so they expose the same wrong catalog identity.
- `check_depth` returns as soon as `enter_container` sees depth 65. The `?` in
  `from_bytes` therefore returns `TooDeep` before the subsequent
  `Variant::try_new` full-validity check runs. Wrapping an invalid primitive
  byte in the existing compact-list fixture shape 65 times consequently masks
  malformed encoding as `TooDeep`, although the design orders encoding before
  depth. Pinned `parquet-variant` 59.3 confirms that `Variant::new` performs
  only shallow validation while `Variant::try_new` performs the recursive full
  validation; the skipped call is the only complete encoding check.

The current focused tests do not cover either case. The numeric/depth test puts
the two defects in sibling object branches, and the raw-depth test uses a valid
null leaf plus a separate shallow malformed fixture.

**Observable consequence.** Equivalent invalid Variant inputs receive a
different stable public error depending on whether the higher-priority defect
is above or below the depth boundary. Callers branching on the documented
catalog code see `WYRD_VALA_400_VARIANT_TOO_DEEP` where
`WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE` or
`WYRD_VALA_400_VARIANT_INVALID_JSON` is required. Writes are still refused and
the hostile raw-byte path no longer risks recursive stack exhaustion, so this
is an error-contract defect rather than a durability or availability failure.

**Required testable correction.** Keep `EncodedVariant` as the single owner and
retain the pre-recursion safety boundary, but complete a bounded, fallible scan
of the admitted value before selecting depth: JSON conversion must still detect
out-of-range numeric tokens below the first over-depth container without
building that subtree, and raw-byte validation must establish encoding
validity through the pinned Variant shallow/fallible APIs before returning a
recorded depth violation. Do not reintroduce recursive upstream validation
before the depth guard or add a second Variant model. Prove closure with:

1. 65 nested JSON lists around an out-of-range integer through direct
   conversion and Oracle `parse_json`, expecting identical numeric-range code
   and path;
2. the same compound value through one bounded pre-ACK JSON-row write,
   expecting no ACK or durable row; and
3. a compact depth-65 raw Variant whose deeper leaf is malformed, expecting
   `VARIANT_INVALID_JSON`, followed by a valid write proving server
   availability.

## Verification Evidence and Limits

- Independently ran
  `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::raw_depth_is_bounded_before_full_validation) | test(=variant::tests::numeric_range_outranks_depth_in_any_key_order) | test(=variant::tests::variant_extension_requires_empty_metadata) | test(=variant::tests::malformed_stored_variant_is_an_error_not_a_panic)'`:
  **4 passed, 56 skipped**.
- Inspected the task's recorded real-server raw-IPC journey, Oracle test,
  compiled CLI journey, and cross-language consumer evidence. Those tests
  credibly prove the remediated cases they name, but none places the numeric or
  malformed-byte defect below the first over-depth container.
- Verified pinned dependency behavior from `parquet-variant` 59.3.0,
  `parquet-variant-compute` 59.3.0, `parquet-variant-json` 59.3.0,
  `serde_json` 1.0.150, Arrow 59.3.0, and DataFusion 55.0.0 source selected by
  `Cargo.lock`.
- I did not rerun the Postgres-backed server journey, CLI binary journey, or
  full cross-language lanes in this domain pass. Their recorded green results
  do not exercise the retained compound precedence gap.
- TASK-003's shredded physical layout and pruning work remains outside this
  TASK-001 logical Variant/Arrow review.

## Overall Result

**FAIL** — `VARIANT-ARROW-R5-001` leaves one bounded but user-visible stable
failure-precedence defect in the shared Variant owner.
