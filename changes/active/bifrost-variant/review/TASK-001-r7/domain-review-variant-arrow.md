# Variant / Arrow domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation reviewed: `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md`

The commit and tree identities matched before and after this review. The source
was not modified.

## Boundary and authority coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| JSON text and `serde_json::Value` admission | Spec REQ-004/REQ-019 and INV-002/INV-007; R6 findings 18/25; `EncodedVariant::{from_json,from_json_text}`, `append_raw`, `scan_numbers`, `raw_number_variant`; `serde_json` 1.0.151 `RawValue`/`Deserializer::ignore_value` | FAIL: findings VARIANT-R7-001 and VARIANT-R7-002 |
| Raw Variant canonicality | `architecture/bifrost-design.md` Storage format and Variant; R6 findings 16/24/26; `EncodedVariant::{from_bytes,validate}`, `scan_encoded`, `object_field_slots`; installed `parquet-variant` 59.3 object/list shallow and full validators | FAIL: finding VARIANT-R7-003 |
| Numeric domain and failure priority | Spec REQ-004/REQ-019; Decimal4/8/16 and Float/Double arms; `VariantViolations`; JSON/raw unit and raw-IPC journeys | PASS except the JSON size accounting defect in VARIANT-R7-001 |
| Renderer entry points | `variant_bytes_to_json`, `variant_cell_to_json`, `VariantJsonEncoderFactory`, `mask_placeholders`, Oracle `decode_rows`/`to_json`; Rust/SDK/MCP/server callers | PASS: present unshredded cells reach the shared borrowed validation gate before upstream recursion |
| Arrow extension identity and nesting | `variant_field`, `is_variant`, schema conversion, `validate_declared_variants`, recursive `validate_variant_values` | PASS: the exact extension name/metadata and nested declared layout are checked before values |
| Producer and consumer seams | `batch_builder`, OTLP `signal::{attributes_variant,any_value_variant,finish_variant}`, Scribe table validation, Oracle `parse_json`/`try_parse_json`, JSON result encoders | PASS subject to the findings below |

## Review Findings

### Critical

None.

### Important

- **VARIANT-R7-001 — INCORRECT** — [`crates/shared/wyrd-queue/src/variant.rs:800`](../../../../../crates/shared/wyrd-queue/src/variant.rs) records an out-of-range JSON number and then appends a Variant null placeholder at lines 802–804, even though the revision-13 authority and R6 correction require a refused number to add no encoded bytes; enough rejected numbers, or one rejected number beside an accepted value just below the limit, can therefore manufacture an oversized encoding and return `WYRD_VALA_413_VARIANT_TOO_LARGE` instead of the required `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`.  Stop adding rejected numeric members to the temporary builder, preserving the existing violation record and member path, and add a boundary test whose accepted portion remains within 8,388,608 bytes but whose current null placeholder crosses it and incorrectly changes the selected error.

- **VARIANT-R7-002 — VIOLATION** — [`crates/shared/wyrd-queue/src/variant.rs:822`](../../../../../crates/shared/wyrd-queue/src/variant.rs) classifies numbers below depth 64 by repeatedly deserializing every remaining object/array subtree at lines 837–853, so a compact deeply nested value performs depth-times-subtree parsing and allocates one `String` path token per level; this is reachable from buffered JSON writes and Oracle `parse_json`, and a request-sized hostile value can monopolize CPU and amplify memory far beyond its bytes despite INV-007 and the R6 requirement for a bounded non-building scan.  Replace the repeated subtree deserializations with the R6 packet's single syntax-authoritative, stack-safe traversal that visits each input token once, retains final-key-wins and numeric-before-depth behavior, and prove it with the existing 10,000-level classifications plus a focused work-bound check that fails if nested suffixes are reparsed.

- **VARIANT-R7-003 — VIOLATION** — [`crates/shared/wyrd-queue/src/variant.rs:1063`](../../../../../crates/shared/wyrd-queue/src/variant.rs) sorts all object field starts and performs a binary search per field, making the canonical raw-byte gate `O(fields log fields)` rather than the iterative linear-in-byte-size validation promised by `architecture/bifrost-design.md`; a wide raw Arrow object reaches this work before ACK and again in every renderer.  Derive disjoint field ownership in one linear pass over the already size-bounded value region, without adding a second validator, and retain the shared/overlap, reverse-offset, duplicate-name, numeric, depth, and renderer tests while adding a wide-object proof whose algorithm visits offsets/value positions only a constant number of times.

### Suggestions

None.

## Open Questions

None. Each correction stays inside the approved Variant contract and the
existing `EncodedVariant` owner; no product, public API, persistence, or
cross-service decision is needed.

## Verification Notes

- Reviewed the recorded successful evidence: 68 `wyrd-queue` tests, 847
  `vala-bifrost-redux` tests under Postgres, the focused Oracle and gateway
  tests, three journeys, and format/lint/codegen/skills/docs/diff checks.
- The hostile-depth tests establish stack safety and error identity at 10,000
  levels, but do not detect repeated suffix parsing or path-allocation
  amplification.
- `json_size_outranks_numeric_and_depth` uses an accepted sibling already over
  the size ceiling; it therefore cannot detect that rejected-number null
  placeholders incorrectly contribute bytes.
- `raw_shared_field_values_are_refused` proves shared and overlapping ranges
  are refused, but neither it nor the journeys prove the authority's linear
  work bound for a wide object.
- Installed dependency behavior was checked directly: `serde_json` 1.0.151
  validates `RawValue` syntax iteratively, while `parquet-variant` 59.3 leaves
  object starts unordered and recursively validates child values; neither
  dependency supplies Wyrd's unique-region or exact numeric-domain rule.

## Overall result

**FAIL** — the exact raw numeric domain, Arrow extension identity, renderer
gate, and hostile-stack behavior are present, but stable JSON error precedence
and the approved JSON/raw work bounds remain incomplete.
