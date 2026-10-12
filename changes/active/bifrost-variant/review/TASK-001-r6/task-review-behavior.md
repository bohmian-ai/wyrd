# TASK-001 r6 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Latest remediation: `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md`

This review inspected the complete cumulative base-to-candidate range, not only
the R5 remediation commits. CodeGraph was used first. I then traced the current
caller-to-result paths and checked the R1-R5 verdicts, validated ledgers,
remediation packets, and the R5 implementation evidence. I did not read any
other R6 report.

## Navigation map and realistic paths

| Capability | Producer / owner | Admission / execution | Observable result |
|---|---|---|---|
| JSON-authored Variant | `wyrd_queue::BatchBuilder` -> `EncodedVariant::from_json_text` | prepared queue row -> Gate/Scribe | hot/published Oracle -> Rust/Python/TypeScript/MCP/CLI native JSON |
| Raw Arrow Variant | `Bifrost::write_batch` or raw IPC -> declared extension column | Scribe `canonical_validator` -> `EncodedVariant::from_bytes` | stored extension or native JSON terminal |
| Built-in nullable Structs | verification, gateway, and OTLP metrics producers | table-owned validator -> `refuse_partial_structs` | hot and published Struct field reads |
| Variant SQL | every Oracle session installs `OracleVariantSql`; `parse_json` also calls `EncodedVariant::from_json_text` | interactive, analytical, follower, and distributed execution | Arrow extension, typed rows, or structured query error |
| Durable layout | catalog v3 creation/validation -> Scribe append -> Forge rewrite/GC | pinned Iceberg/compaction behavior | stable hidden lineage after publication and repeated rewrite |

The two retained findings share the public `EncodedVariant::from_json_text`
producer. They affect both row insertion (`batch_builder.rs:376-385`) and
Oracle `parse_json`/`try_parse_json` (`oracle/variant_sql.rs:600-617`).

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/REQ-002, INV-003/INV-006, AC-002: v3-only tables, hidden lineage preserved through rewrite, v3 GC | `catalog/bifrost_catalog.rs:1047-1058,1095-1099`; Forge and pinned-fork cumulative diff | R5 record: V10, V12, V13 all exit 0 at the recorded pins | PASS |
| REQ-003/REQ-004, INV-001/INV-002: canonical Variant type, fingerprint, exact accepted values, missing/null distinction | `wyrd-queue/src/variant.rs`; `tables/mod.rs`; generated schemas and result renderers | R5 record: queue Variant suite, built-in contract test, Rust/Python/TypeScript/MCP journeys exit 0 | PASS except the REQ-019 failures below |
| REQ-004/REQ-019: valid JSON nested beyond 64 is classified as `TOO_DEEP`, with numeric range outranking depth | `from_json_text` initially parses the whole input with default `serde_json::from_str` before its own depth/numeric walker (`variant.rs:199-205`) | Existing unit and journey evidence covers depth 65 and numeric-under-depth, but not valid JSON beyond serde's 128-level parser ceiling | **FAIL — `BVR-R6-BEH-001`** |
| Locked write precedence and REQ-019: Variant byte size precedes JSON validity, numeric range, and depth | `from_bytes` checks size first, but `from_json_text` calls `found.finish()` before `builder.finish()` and `sized` (`variant.rs:203-207`) | Existing journey proves oversized+malformed precedence only for raw Arrow bytes; no JSON size-plus-numeric/depth case is recorded | **FAIL — `BVR-R6-BEH-002`** |
| REQ-006/REQ-007/REQ-008/REQ-011, AC-001: spans, logs, metrics, promotions, and canonical/OTLP equivalence | canonical signal ledgers and projections under `tables/{traces,logs,metrics,signal}` | R5 record: all three OTLP journeys plus canonical-signal Rust/Python/TypeScript reads exit 0 | PASS |
| REQ-009/REQ-010, AC-003: verification Structs and other built-in open payloads are typed and queryable | `ResultsTable::{drift_report_fields,eval_summary_fields}`, built-in Variant declarations and producers | `typed_builtin_payloads_are_queryable` plus SDK/MCP built-in journeys recorded green | PASS |
| R5 findings 14/22: nullable physical children retain whole-present/null-absent semantics and requested/resolved models remain distinct | `tables/mod.rs:273-333`; `ResultsTable::WHOLE_STRUCTS`; `CallsTable::WHOLE_STRUCTS`; separate model declarations; metrics validator | R5 record: focused table tests, gateway unit test, verification pre-ACK journey, metrics OTLP journey exit 0 | PASS |
| R5 findings 16/18/24: raw malformed/numeric/depth order, JSON numeric-before-depth, exact raw Decimal16 domain | `VariantViolations`, `scan_encoded`, `scan_numbers`, raw decimal guard in `variant.rs` | R5 record: 12 queue Variant tests, Oracle Variant SQL tests, and pre-ACK raw/JSON journey exit 0 | PASS for the tested ranges; findings above identify untested reachable boundaries |
| REQ-017/REQ-019, INV-004/INV-006, AC-005/AC-008: one Variant SQL surface in every production session, stable distributed errors, sensitivity before IO | `OracleVariantSql::install` call sites in `resources.rs`, `oracle/mod.rs`, `oracle/analytical.rs`; `QueryCatalogError` and terminal projection | R5 record: Oracle session matrix, client/SDK/MCP journeys, late-error evidence green | PASS except `parse_json` inherits both JSON findings |
| REQ-005/AC-009: Scribe and Forge Bloom capacity is the row-group maximum | shared writer properties; `writer_properties.rs:303-330` | focused Bloom test recorded green | PASS |
| Scope/non-goals: no second Variant model/reader, shredding policy, migration, compatibility alias, or unrelated R5 workflow drift | cumulative diff and R5 correction restoring `wyrd-implement`; explicitly approved task-review exception retained | `check:skills-sync`, codegen, docs, format, lints, and `git diff --check` recorded green | PASS |

## Proposed findings

### BVR-R6-BEH-001 — valid deeply nested JSON is reported as invalid JSON

- **Classification:** INCORRECT
- **Violated obligation:** The revision-13 locked contract and REQ-019 require
  any valid Variant nested beyond depth 64 to fail as
  `WYRD_VALA_400_VARIANT_TOO_DEEP`; numeric range outranks depth after complete
  bounded detection, and `try_parse_json` maps only invalid JSON to SQL null.
- **Exact location:** `crates/shared/wyrd-queue/src/variant.rs:199-205`, reached
  from `crates/shared/wyrd-queue/src/batch_builder.rs:376-385` and
  `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:600-617`.
- **Evidence:** `from_json_text` first calls ordinary
  `serde_json::from_str::<&RawValue>`. The pinned `serde_json` is 1.0.150
  (`Cargo.lock:8361-8364`); the workspace/queue manifests enable `raw_value`
  but not `unbounded_depth`. That deserializer starts with a 128-level
  recursion budget and returns `RecursionLimitExceeded` before the candidate's
  own 64-level walker runs. Consequently a syntactically valid JSON array or
  object nested sufficiently beyond that parser ceiling becomes
  `VariantInvalidJson { path: "" }`, not `VariantTooDeep`; a higher-priority
  out-of-range integer below that point is also never discovered.
- **Observable consequence:** JSON row insertion and `parse_json` expose the
  wrong stable catalog code. More seriously, `try_parse_json` treats this valid
  but over-deep JSON as malformed and returns SQL null, while an equivalent
  canonical Arrow Variant is refused as too deep.
- **Required testable correction:** Make the existing JSON Variant owner
  validate the complete size-bounded input without serde's lower recursion
  ceiling masking Bifrost's rules, while retaining the current one-owner
  precedence and non-recursive scan below depth 64. Add focused direct,
  pre-ACK JSON-row, `parse_json`, and `try_parse_json` cases beyond serde's
  default ceiling; assert `TOO_DEEP`, numeric-before-depth when applicable, no
  retained row, and continued service availability.

### BVR-R6-BEH-002 — JSON conversion selects numeric/depth before encoded size

- **Classification:** INCORRECT
- **Violated obligation:** The locked write order is request-envelope size,
  row/field order, Variant encoded-byte limit, JSON/extension validity, numeric
  range, then depth. REQ-019 requires oversized Variants to use the stable 413
  code rather than whichever lower-priority condition also occurs.
- **Exact location:** `crates/shared/wyrd-queue/src/variant.rs:203-207`, reached
  by `crates/shared/wyrd-queue/src/batch_builder.rs:376-385`.
- **Evidence:** `append_raw` records numeric/depth facts, then
  `found.finish()?` returns them before the builder is finished and
  `Self::sized` computes encoded bytes. A shallow object containing a string
  that makes the Variant exceed 8,388,608 encoded bytes plus an out-of-range
  integer therefore returns `VARIANT_NUMERIC_OUT_OF_RANGE`; replacing the
  number with an over-deep branch returns `VARIANT_TOO_DEEP`. The default
  Bifrost ingest request ceiling is 16 MiB, so a single >8 MiB Variant remains
  reachable after the request-envelope check.
- **Observable consequence:** Equivalent oversized input has different public
  errors depending on whether it arrives as JSON or canonical Arrow, and the
  JSON write contradicts the specified failure order.
- **Required testable correction:** At the existing `EncodedVariant` JSON
  boundary, determine and select the encoded-size violation before returning
  recorded numeric/depth violations, without admitting or retaining any
  partial value and without adding another Variant model. Add one direct and
  one pre-ACK JSON-row case combining oversize with numeric range, plus
  oversize with depth; assert exact `WYRD_VALA_413_VARIANT_TOO_LARGE` details,
  no ACK/no row, and successful following work.

## Verification limits

No Cargo, mise, codegen, or journey commands were run during this review,
because the shared checkout was reserved from parallel Cargo work. The matrix
uses the command/results recorded in the R5 remediation file. Those results
credibly cover the named happy, refusal, cross-language, Oracle, Forge, and
gate paths, but they contain neither of the compound JSON cases above.

## Overall result

**FAIL**

The R5 corrections close their seven validated findings, but the cumulative
candidate still fails two reachable revision-13 JSON Variant error contracts.
