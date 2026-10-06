# Variant / Arrow Data-Integrity Domain Review

Immutable subject: base `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`, candidate `a6429060fb011aafa4335f2f736c70adab231739`, tree `64177d67141993ec18e63b43dc227dbc31d70950`, approved spec revision 11, original `TASK-001` cumulative diff.

## Review Findings

### Critical

- **VARIANT-ARROW-R4-001 — [`crates/shared/wyrd-queue/src/variant.rs:172`](../../../../../crates/shared/wyrd-queue/src/variant.rs#L172) violates the depth bound before it is enforced.**
  - **Violated obligation:** spec revision 11 lines 202–205 and INV-007 require Variant conversion to be bounded at 64 containers; `architecture/bifrost-design.md:155-164` requires Scribe to reject over-depth raw Arrow values before acknowledgement.
  - **Evidence:** `EncodedVariant::from_bytes` checks byte size, then calls `Variant::try_new`, and only afterward calls Wyrd's `check_depth` (`variant.rs:172-178`). In pinned `parquet-variant-59.3.0`, `Variant::try_new_with_metadata` invokes recursive `with_full_validation` (`variant.rs:402-406`); list validation recursively calls `Variant::try_new_with_metadata` for each child (`variant/list.rs:212-234`). A single-element nested-list encoding grows by only ten bytes per level (the repository's own fixture builder at `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:1608-1616` demonstrates that encoding), so a value can be tens of thousands of levels deep while remaining far below 8,388,608 bytes. The built-in Scribe trust boundary reaches this path through `enforce_builtin_source_contract` → `canonical_validator` → `validate_declared_variants` → `EncodedVariant::from_bytes` (`scribe/execution_lanes.rs:601-621`, `tables/mod.rs:247-250,315-328`).
  - **Observable consequence:** a raw Arrow writer can force dependency recursion long before Wyrd observes depth 65. This defeats the promised processing bound and risks worker-stack exhaustion/process termination instead of the stable `WYRD_VALA_400_VARIANT_TOO_DEEP` refusal, affecting other tenants sharing the server process.
  - **Required correction:** enforce the Wyrd depth ceiling through a bounded, fallible validation path before entering upstream recursive full validation, while retaining upstream `parquet-variant` as the encoding/validity authority. Add a subprocess or otherwise process-safe trust-boundary test with a compact, very deeply nested raw Variant that proves a stable `VARIANT_TOO_DEEP` refusal and proves the server remains available; retain the existing malformed-byte and depth-65 cases.

### Important

- **VARIANT-ARROW-R4-002 — [`crates/shared/wyrd-queue/src/variant.rs:430`](../../../../../crates/shared/wyrd-queue/src/variant.rs#L430) accepts non-canonical extension metadata and the admission normalizer erases the evidence.**
  - **Violated obligation:** `architecture/bifrost-design.md:134-142` requires the canonical `arrow.parquet.variant` extension at every nesting level and explicitly makes missing or foreign extension metadata `WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE`.
  - **Evidence:** `is_variant` checks only `ARROW:extension:name` (`variant.rs:425-432`). `field_to_spec` then classifies the field as `DataTypeSpec::Variant` and discards both extension metadata keys (`schema.rs:261-276`), while `validate_declared_variants` compares only that normalized reconstruction and the boolean `is_variant` result (`tables/mod.rs:222-243`). Therefore a field with the correct extension name but a non-empty/foreign `ARROW:extension:metadata` value is normalized into the canonical empty-metadata field and accepted. The pinned `VariantType` documents empty metadata (`parquet-variant-compute-59.3.0/src/variant_array.rs:93-105`) but its deserializer ignores the supplied metadata (`:107-108`), so the dependency does not close this check. The journey at `verification_runtime.rs:1222-1255` covers a missing marker and a foreign extension *name*, not foreign metadata under the correct name.
  - **Observable consequence:** the server accepts a wire schema the published contract says it must reject, including at nested Variant positions. Because normalization removes the offending metadata before comparison/stamping, downstream schema evidence cannot reveal that the trust-boundary check was bypassed.
  - **Required correction:** make the shared Variant field predicate/validator require the exact canonical name and exact canonical empty metadata, and use that same owner for top-level and nested validation. Add raw Arrow admission coverage for the correct name with non-canonical metadata at both a top-level built-in Variant and a nested built-in Variant; require `WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE` before acknowledgement.

- **VARIANT-ARROW-R4-003 — [`crates/shared/wyrd-queue/src/variant.rs:154`](../../../../../crates/shared/wyrd-queue/src/variant.rs#L154) returns JSON conversion failures in traversal order rather than the locked precedence.**
  - **Violated obligation:** spec revision 11 lines 221–226 fixes the order after field/type checks as Variant byte limit, JSON validity, numeric range, then depth, returning the first failure under that precedence.
  - **Evidence:** `from_json_text` parses JSON, recursively calls `append_raw`, and checks encoded size only after the builder finishes (`variant.rs:154-160`). During that walk, `enter_container` returns `TooDeep` immediately (`:580-588`) and `raw_number_variant` returns `NumericOutOfRange` immediately (`:671-690`); object children are visited in `BTreeMap` key order (`:616-630`). Thus an over-depth value that would also exceed the encoded-byte limit reports `VARIANT_TOO_DEEP`, not `VARIANT_TOO_LARGE`, and a compound numeric/depth failure depends on traversal/key order rather than the mandated numeric-before-depth precedence. This shared constructor feeds row builders, gateway/audit/result producers, and SQL `parse_json`, so the drift is not isolated to one surface.
  - **Observable consequence:** equivalent invalid inputs can expose the wrong stable catalog code and details across writes and SQL, breaking the public error contract and callers that branch on the documented code.
  - **Required correction:** keep one shared JSON conversion owner, but make it determine byte-limit, numeric, and depth violations and select the catalogued failure in the locked order before returning. Add focused compound-failure cases (oversize + over-depth, numeric-range + over-depth with the failures in both object-key orders) and assert identical codes/details through direct conversion and `parse_json`; add one pre-ack write proof for the oversize + over-depth case.

### Suggestions

None.

## Open Questions

None. The approved spec and Bifrost authority already decide the required depth bound, exact extension metadata, and error precedence.

## Reviewed Boundary and Source Coverage

| Boundary | Authority | Source and caller coverage | Result |
|---|---|---|---|
| Public type and stable limits/errors | Spec rev. 11 `Variant representation, limits, and failures`, REQ-003/004/019; `AGENTS.md` public-error rules | `wyrd-spec/src/vala/api.rs`, `wyrd-spec/src/vala/error.rs`, generated schema snapshots | Pass except failure ordering in VARIANT-ARROW-R4-003 |
| Canonical type and logical fingerprint | Spec REQ-003; Bifrost design `Storage format and Variant` | `tables/fields.rs`, `tables/mod.rs` canonical fingerprint encoder and golden contract test | Pass: `CanonicalType::Variant`, tag `0x0d`, zero children, extension keys excluded |
| Arrow ↔ wire mapping | Bifrost design lines 134–142; Arrow interop reference | `wyrd-queue/src/schema.rs`, server registration conversion, `DataTypeSpec::Variant` | Fail: VARIANT-ARROW-R4-002 |
| JSON/value encoding and exactness | Spec REQ-004, duplicate-key rule, fixed size/depth | `wyrd-queue/src/variant.rs`; pinned `parquet-variant`, `parquet-variant-compute`, and `parquet-variant-json` 59.3 sources | Exact signed/u64 integer handling, final duplicate key, null-vs-missing, and `3.0` JSON rendering are implemented; fail on bounded raw validation and compound error precedence |
| Raw Arrow and malformed stored bytes | Bifrost design lines 155–164; INV-007 | `tables/mod.rs::validate_declared_variants`, nested Struct/List walk, `scribe/execution_lanes.rs` admission, `mask_placeholders`, Oracle Variant UDF decode | Ordinary malformed bytes are converted to errors; fail on pre-cap recursive dependency validation |
| Built-in schemas and producers | REQ-006–REQ-011; Bifrost design lines 166–188 | spans/events/links, logs, metrics/exemplars, verification results, eval observations/items, gateway calls, agent traces, audit projection; `wyrd-client::observe::eval` and `wyrd-server::verification::results` producers | No additional material defect found; all named built-in Variant and Struct shapes route through the shared builder or Scribe validator |
| JSON/query consumers | REQ-017–REQ-019 | `oracle/variant_sql.rs`, client row JSON encoder, CLI, MCP, Python and TypeScript Variant terminals, verification readers | No additional material defect found; `to_json`, `->>`, row JSON, nested placeholders, exact u64, and `3.0` use `to_json_value`-based rendering |
| Upstream Variant API behavior | Spec locked dependencies | Cargo-locked 59.3 sources for `Variant::try_new`, recursive full validation, `VariantArray::try_value`, `VariantType`, and `VariantToJson` | Directly establishes VARIANT-ARROW-R4-001 and confirms metadata is not rejected upstream |

## Verification Notes

- Reviewed the complete base-to-candidate diff for this domain and source at the immutable candidate, not the later working-tree skill-only commit.
- Inspected the focused tests for schema/fingerprint identity, integer boundaries, duplicate keys, null-vs-missing, malformed bytes, nested Variant admission, size/depth failures, built-in layouts, JSON rendering, SQL functions, and the Rust/Python/TypeScript/MCP journey call sites.
- The candidate records successful task-local verification, but this review did not treat that record as independent proof. A focused `wyrd-queue` nextest run was attempted and canceled after it remained blocked on the shared Cargo build-directory lock; no independent executable result is claimed.
- Existing proof does not cover: hostile raw nesting far beyond 65, correct-name/foreign extension metadata, or compound JSON failures that distinguish the locked error precedence. Those gaps correspond directly to the findings above.
- Candidate tree identity was rechecked as `64177d67141993ec18e63b43dc227dbc31d70950`.

## Overall Result

**FAIL** — one process-safety/boundedness defect and two bounded public-contract defects remain in the shared Variant/Arrow owner.
