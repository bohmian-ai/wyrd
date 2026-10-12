# Scribe ingest, admission, and Arrow IPC domain review

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None. No optional refactor is needed for this boundary.

## Reviewed boundary

Immutable subject:

- base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- remediation range inspected: `bb6ae8070..0e37748f3a27d3bcec4713e6210e97328e045886`

The candidate and tree matched those identities before and after this review.
CodeGraph was used first to locate the Scribe admission owners, every built-in
validator path, and their callers. I then inspected the cumulative diff, the
remediation diff, the applicable authorities, prior r1-r4 findings and
remediation evidence, the current source, and the pinned Arrow 59.3 validation
behavior on which the candidate relies.

## Authority and source coverage

| Concern | Authority | Source and result |
|---|---|---|
| Pre-ACK trust boundary and durable transition | `architecture/bifrost-design.md`; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/analytical-operations-reliability.md` | `scribe/ingress.rs:408-450,552-627` plans and admits before decode, then decodes/validates before `AdmittedAppend`, shard dispatch, WAL completion, or ACK. Decode, contract, memory, and queue refusals drop the local owners and return without a durable write. **PASS** |
| Hostile native IPC preflight | `architecture/references/domain/arrow-analytical-interop.md`; INV-007 | `scribe/material_plan.rs:156-195,353-486,850-930` bounds nesting and node counts, validates framing, lengths, offsets, buffer ranges, validity counts, and projected material before decode allocation. Required child nulls are admitted by preflight only when the schema has an enclosing nullable Struct with no intervening List; exact bitmap containment remains enforced by the ordinary Arrow decoder. **PASS** |
| Native decode validation | Arrow trust-boundary rules; no validation bypass | `scribe/preprocess.rs:145-191,306-350` constructs `StreamDecoder::new()` and never enables `with_skip_validation`. Arrow 59.3 therefore performs its normal full `ArrayData` validation, including required-child null containment, before the batch reaches table validation. **PASS** |
| WAL/current-slice IPC encoding | Arrow schema/nullability preservation | `scribe/fixed_ipc.rs:193-244,270-360,374-423,1252-1301` uses the same ancestor-mask rule while counting and encoding. `has_unmasked_null` rejects a required child null whenever no enclosing Struct null masks that row, resets the mask across List item position spaces, and recomputes the facts before writing. **PASS** |
| All built-in validation routes | Spec REQ-006-REQ-011, REQ-019; prior `FIND-TASK-001-2` | `tables/mod.rs:400-436,1038-1069` registers one validator for all eleven built-ins. Spans, logs, and metrics override with the canonical signal validator; the other eight use `validate_predeclared`. `scribe/execution_lanes.rs:533-577,580-635` runs the resolved definition's validator for every built-in before the storage fingerprint, stamping, dispatch, WAL, or ACK and preserves `BifrostError` through `ScribeError::ContractViolation`. **PASS** |
| Predeclared schema/value precedence | Locked order in spec lines 221-226; `FIND-TASK-001-15` | `tables/mod.rs:256-314` refuses the first undeclared supplied field, establishes complete count/order/name/nullability/storage shape, and only traverses Variant values when that phase succeeds. Missing, reordered, nullability-drifted, or non-Variant type-drifted blocks reach the later fingerprint refusal without reading hostile Variant bytes. **PASS** |
| Canonical signal schema/value precedence | Same locked order; field-name mapping rule | `tables/signal.rs:880-952` refuses undeclared fields, then checks count, each declared field by name, storage type, and nullability in logical declaration order, then validates Variant extension identity and values. The returned batch is rebuilt in canonical order. **PASS** |
| Recursive Variant wire and value validation | REQ-003, REQ-019; `FIND-TASK-001-16` and `-17` | `tables/mod.rs:197-253,316-383` compares top-level and nested Variant declarations through the shared Arrow/wire conversion and validates present cells row-first/field-second. `wyrd-queue/src/variant.rs:167-197,428-442,752-790` enforces size, bounded depth preflight, canonical extension name plus empty/absent metadata, then upstream full validity. The hostile depth path cannot enter unbounded upstream recursion first. **PASS** |
| JSON-row exact numeric admission | REQ-004, REQ-019; `FIND-TASK-001-18` | `wyrd-queue/src/batch_builder.rs:106-139,205-263,265-366` retains each top-level field as `RawValue`; only Variant fields go directly to `EncodedVariant::from_json_text`, preserving integer tokens beyond `u64`. Other typed columns retain their prior `serde_json::Value` conversions. `variant.rs:139-165,601-718` retains depth as the lower-priority error while continuing the bounded sibling traversal needed to find numeric range first. **PASS** |
| Error identity and no-write behavior | Stable catalog errors; AC-005 | `execution_lanes.rs:601-635`, `gate/error.rs:318-326`, and the raw-IPC journey preserve typed undeclared/unsupported/Variant errors. `ingress.rs:574-619` cannot create and dispatch `AdmittedAppend` until decode and table validation succeed, so all reviewed hostile inputs are pre-WAL and pre-ACK refusals. **PASS** |
| Copied TASK-002 implementation | R4 human decision and single-owner constraint | Candidate `fixed_ipc.rs` and `material_plan.rs` are byte-identical to commit `535367c94` for those files. The copied code is treated as candidate production code here, not accepted merely because of provenance; its callers, Arrow dependency behavior, and tests were independently traced above. **PASS** |

## Prior-finding closure

- `FIND-TASK-001-2`: closed. Every one of the eleven built-ins now reaches a
  table-owned recursive Variant validator before fingerprinting, WAL, and ACK;
  typed catalog identity is retained.
- `FIND-TASK-001-15`: closed. `validate_predeclared` completes undeclared and
  full schema-shape checks before any Variant byte walk, and the real-server
  journey combines each earlier schema defect with invalid, over-depth, and
  oversized Variant values.
- `FIND-TASK-001-16`: closed. Raw encoded depth is capped before upstream full
  recursive validation, with panic containment for malformed shallow access;
  the 20,000-level fixture returns the depth-65 error and the journey then
  accepts a valid batch on the same server.
- `FIND-TASK-001-17`: closed. The shared `is_variant` predicate requires the
  canonical extension name and empty/absent extension metadata, including at
  nested built-in positions.
- `FIND-TASK-001-18`: closed for the approved compound case. Direct conversion,
  Oracle `parse_json`, and the public JSON-row write path cover both sibling key
  orders and return the same numeric-range details before depth.
- R4 masked-Struct diagnosis: closed. The native preflight and fixed IPC writer
  now share Arrow's enclosing-Struct mask rule, while revision 12 makes the
  affected persisted verification children nullable so published Parquet reads
  retain SQL nulls without a read-normalization layer.

## Open Questions

None.

## Verification Notes

- Independently run during this review:
  `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=scribe::fixed_ipc::tests::masked_required_struct_child_null_roundtrips) | test(=scribe::material_plan::tests::native_preflight_rejects_nonnullable_nulls) | test(=scribe::execution_lanes::tests::canonical_validator_refusal_keeps_its_catalogued_code)'` — **3 passed**.
- `git diff --check` passed for both the cumulative base-to-candidate range and
  `bb6ae8070..candidate`.
- Reviewed, but did not independently rerun, the recorded successful real-server
  evidence for `verification_runtime::builtin_variant_columns_are_refused_before_ack`,
  `verification_runtime::typed_builtin_payloads_are_queryable`, and the metrics
  publication journey. Those tests cover exact catalog codes, no ACK/no durable
  row, post-refusal server availability, and hot/published null behavior.
- Residual verification limit: the focused unit selection does not itself stand
  up Postgres or exercise the network ACK boundary; that proof is supplied by
  the recorded repository-managed journeys above.

## Overall result

**PASS** — no material Scribe ingest, admission-order, nullability, hostile IPC,
error-identity, or ACK/durability defect remains in the reviewed candidate.
