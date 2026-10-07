# TASK-001 r7 focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation range inspected: `b4ea01848..6147cc617`, especially
  `679502cc0`, `0db3afad5`, `8121e0405`, `fec6d5cf4`, `db65d41cc`,
  `df74821aa`, and `6147cc617`

The candidate and tree matched before and after this review. I read every r7
discovery report, the r1-r6 verdicts, validated ledgers, root-cause ledgers and
remediation tasks, the cumulative diff, revision-13 authority, current source
and callers, and the pinned dependency/fork evidence. CodeGraph was used first.
No source or test was changed.

## Uncertainty resolution

| Uncertainty | Resolution | Source evidence and smallest correction boundary |
|---|---|---|
| `BVR-R7-BEH-001`, `INV-R7-001`, `ORACLE-R7-001`, `SYS-R7-001`, and `VARIANT-R7-002`: repeated subtree parsing | **CONFIRMED / CONSOLIDATED.** This is one reachable incomplete closure of prior `FIND-TASK-001-18`. | `from_json_text` performs one iterative syntax check, then `append_raw` reparses at each accepted level and, after the depth-65 refusal, `scan_numbers` (`variant.rs:822-864`) reparses every remaining nested suffix to preserve numeric-before-depth. The accepted portion is capped at 64 levels, but the refused suffix has no depth cap and adds no encoded bytes, so the 8 MiB encoded limit does not bound this work. Buffered JSON writes and synchronous Oracle `parse_json`/`try_parse_json` both reach it. Correct the existing `EncodedVariant` JSON traversal once so syntax, exact numeric tokens, last-key-wins normalization, and violation collection consume input proportionally rather than reparsing suffixes; add no downstream timeout/guard or second parser owner. |
| `VARIANT-R7-001`: refused numeric placeholders affect size | **CONFIRMED.** This is an incomplete implementation of prior `FIND-TASK-001-25`, not a hypothetical-size request. | `append_raw` records `raw_number_variant`'s error and still calls `builder.append_value(Variant::Null)` (`variant.rs:800-805`); the builder therefore includes bytes for a member revision-13 authority says contributes none, and `from_json_text` checks those bytes before returning the recorded numeric error. Stop appending the rejected member while retaining its violation/path, and prove an accepted portion below 8,388,608 bytes stays numeric even when the current placeholder would cross the limit. |
| `VARIANT-R7-003` and `STD-R7-001`: `object_field_slots` complexity | **REVISED TO AUTHORITY DRIFT; code correction rejected.** | `object_field_slots` collects `f + 1` offsets, sorts `f` starts, and performs `f` binary searches (`variant.rs:1050-1074`), so it is `O(f log f)`, not the linear algorithm claimed by `bifrost-design.md:169`. It is nevertheless iterative, capped by the 8 MiB encoded value, and prevents shared/overlapping-region amplification; revision-13 `INV-007` requires bounded processing, not this particular algorithm. The smallest correction is to replace the false linearity claim in the active authority and R6 implementation evidence with the behavior actually approved and delivered: iterative, size-bounded, and non-amplifying. Do not add interval machinery solely to preserve an accidental prose claim. |
| `REUSE-R7-001`: duplicate `Variant::try_new` | **CONFIRMED.** This is a remediation-induced reuse defect adjacent to prior findings 16 and 20, but neither earlier correction removes it. | `EncodedVariant::validate` runs Wyrd's scan and then full upstream `Variant::try_new` (`variant.rs:269-284`); `variant_bytes_to_json` immediately runs the same full upstream validation again (`variant.rs:347-350`). Keep one private borrowed validation owner that returns the already validated dependency `Variant` on success; constructors and `mask_placeholders` may discard it, while the byte renderer renders it directly. The panic containment around Wyrd's raw scan remains at this owner; no public validated-borrow type or second renderer is needed. |
| `BVR-R7-BEH-002` versus `SDK-R7-001`: OTLP non-finite contract and proof | **Behavior conflict rejected; proof gap confirmed.** | Revision 13 says canonical Arrow input uses the same numeric domain as JSON, the r6 validated ledger expressly resolved finite Float/Double as accepted and non-finite values as refused, the approved R6 packet requires non-finite refusal, and active Bifrost authority states it. `SDK-R7-001` reads “IEEE meaning” in isolation and does not establish an unresolved product decision. The new record-local OTLP refusal is user observable, however, and only `logs::tests::non_finite_log_body_rejects_only_its_record` proves it in-process. Extend the existing `log_variant_body_attributes_and_promotions_are_queryable` OTLP journey with one non-finite record and one valid sibling, assert the exact partial-success reason/count, publish, and prove only the sibling persists; no new harness or span/metric copies are required because the shared attribute owner is already covered independently. |
| `PERSIST-R7-001`: reconciliation ignores Variant identity | **CONFIRMED.** This is a new physical-reconciliation predicate gap related to, but independent of, prior admission finding 17. | `validate_physical_table` converts the current Iceberg schema and calls `schema_shape_matches` (`bifrost_catalog.rs:1099-1108`); `field_shape_matches` compares field name/nullability and delegates only `DataType`, while `arrow_type_shape_matches` recursively ignores field metadata (`tables/mod.rs:833-878`). A physical Iceberg Struct with the Variant storage children can therefore match an expected atomic Iceberg Variant after Arrow conversion. Make the existing field-level physical comparator recursively require canonical Variant-extension parity with `wyrd_queue::variant::is_variant`, while preserving the existing approved type aliases and ignoring field IDs; prove top-level and nested Variant-versus-Struct refusal plus the real Iceberg Variant round trip. |
| `PERSIST-R7-002`: missing failed-lineage/no-commit journey | **CONFIRMED AS PROOF-ONLY.** | TASK-001 lines 126-139 explicitly require the named journey to inject missing lineage and assert no commit. `v3_row_lineage_survives_repeated_rewrite` (`managed_rewrite.rs:1450-1562`) exercises only successful repeated rewrites and GC; the fork unit test proves `validate_row_lineage` rejects bad batches but cannot prove validation remains before output/handoff/commit. Extend the existing managed-rewrite seam to deliver a valid batch followed by missing/null lineage, then prove failure, unchanged snapshot/rows, no commit-capable handoff, and tracked/reclaimable possible outputs. |
| `STD-R7-002`: exact focused command evidence | **CONFIRMED.** | R6 evidence names new or modified Rust tests in its acceptance and diagnosis sections, but records only the whole queue library lane for six Variant tests, no exact command for the modified maximal-log test, and a regex rather than exact selector for the gateway unit. Run and record each named test using the repository-mandated exact `test(=...)` form, retaining the broader green lanes. This is evidence repair, not production work. |
| `STD-R7-003`: cumulative whitespace gate | **CONFIRMED.** | `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..6147cc617d81f2c03464043be698ab2565e9d745` reports `TASK-001-r6/standards-review.md:88: new blank line at EOF`; the R6 record checked only `b4ea01848..HEAD`. Remove that one extra terminal blank line and rerun the cumulative command. |

## Reachability and consumer trace

```text
JSON text / serde_json::Value
  -> EncodedVariant::from_json_text
  -> append_raw -> scan_numbers after depth refusal
  -> batch_builder or Oracle parse_json/try_parse_json

raw Arrow Variant / stored Variant
  -> EncodedVariant::validate -> scan_encoded -> Variant::try_new
  -> Scribe admission, placeholder masking, or variant_bytes_to_json
  -> SDK/HTTP/MCP/CLI/Oracle result

registered physical table
  -> validate_physical_table
  -> schema_shape_matches -> field_shape_matches -> arrow_type_shape_matches
  -> reconciliation acceptance or refusal

Forge rewrite stream
  -> per-batch validate_row_lineage -> writer close -> RewriteHandoff
  -> publication commit adapter
```

The deep-JSON issue affects both write preparation and Oracle execution. The
placeholder issue affects JSON error selection only. The duplicate dependency
validation affects the byte-rendering path, not Scribe's durable ordering. The
physical reconciliation issue is reachable whenever an existing table is
accepted; logical catalog fingerprints do not replace validation of the
physical Iceberg type. The lineage source is currently ordered correctly, but
the task-mandated failure transition remains unproven.

## New proposals

No new finding was added beyond the r7 discovery union. The discovery claims
resolve into the independently testable roots above; duplicate deep-JSON
claims are one finding, the raw-object algorithm claim is documentation-only,
and the alleged OTLP specification conflict is rejected while its real journey
gap remains.

## Result

**RESOLVED.** All explicit conflicts and unexplored paths were resolved from
approved authority and current source; no specification revision is required.
