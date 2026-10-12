# TASK-001 r6 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The candidate commit and tree matched before validation. They were reconfirmed
after this report was written and remained unchanged. This validation ran no
Cargo or `mise` command.

## Method and coverage

CodeGraph was used first. I then independently inspected the complete
base-to-candidate diff, the approved specification and task, repository and
Bifrost authorities, every r1-r5 verdict/validated ledger/root-cause/remediation
record, every current r6 report (including `followup-review.md` and
`root-cause.md`), current owners and sibling consumers, and the exact locally
resolved `serde_json` 1.0.150 and Arrow Variant 59.3 dependency source.

The decisive runtime trace was:

```text
JSON or raw Arrow
  -> EncodedVariant::{from_json_text,from_bytes}
  -> built-in table validator in Scribe decode_rows
  -> WAL/ACK/storage
  -> variant_bytes_to_json / variant_cell_to_json /
     VariantJsonEncoderFactory / mask_placeholders
  -> Rust, Python, TypeScript, HTTP, MCP, and CLI terminals
```

Report agreement was not treated as proof. In particular, the green Scribe,
Iceberg, SDK, reuse, and system reports establish their reviewed ordinary paths
but do not cover the omitted inputs retained below.

## Proposal disposition

| Current proposal | Disposition | Stable finding | Independent result |
|---|---|---|---|
| `BVR-R6-BEH-001`, `VARIANT-ARROW-R6-004` | CONFIRMED / CONSOLIDATED | `FIND-TASK-001-18` | Same JSON-owner closure remains incomplete: recursive serde work can decide before Wyrd's depth/numeric rules. |
| `BVR-R6-BEH-002` | CONFIRMED / NARROWED | `FIND-TASK-001-25` | JSON conversion returns numeric/depth before checking an already deterministically oversized encoding. The broader demand to assign canonical size to an unencodable skipped subtree is rejected. |
| `INV-R6-001`, `VARIANT-ARROW-R6-003` | CONFIRMED / CONSOLIDATED | `FIND-TASK-001-24` | Raw Decimal4/8 and non-finite Float/Double bypass the revision-13 numeric-domain invariant. |
| `ODF-R6-001`, `SEC-R6-001`, `VARIANT-ARROW-R6-002` | CONFIRMED / CONSOLIDATED | `FIND-TASK-001-26` | The same unsorted-metadata equality predicate admits duplicate resolved names. |
| `VARIANT-ARROW-R6-001` | CONFIRMED / REVISED | `FIND-TASK-001-16` | The node-count addendum closes one fixture, not the shared-range amplification root, and raw renderers bypass the bounded scanner. |
| `PERSIST-R6-001` | CONFIRMED AS PROOF-ONLY | `FIND-TASK-001-14` | Production source is correct, but the explicitly required gateway boundary/publication proof is absent. |
| `STD-R6-001` | CONFIRMED | `FIND-TASK-001-21` | The implemented r5 remediation still advertises `ready`. |
| `MNT-R6-001` | CONFIRMED / CONSOLIDATED | `FIND-TASK-001-4` | Later revision-12/13 rules were not propagated to the active authority and immediate owner rustdoc. |

The r6 Iceberg, Scribe, SDK, reuse, and system reports proposed no additional
finding. Their pass conclusions remain valid within their stated coverage.

## Product/spec decisions versus implementation findings

No specification revision is required.

- Revision 13 already decides the JSON numeric domain, raw Decimal domain,
  unique-key rule, failure precedence, depth/size limits, persisted nullable
  Struct semantics, bounded processing, and cross-terminal preservation.
- Finite `Float` and `Double` remain valid floating-point values. Only
  non-finite values are rejected because JSON/native terminals cannot preserve
  them. Decimal4/8 are rejected because revision 13 says raw input uses the
  JSON domain and explicitly sends callers needing other decimals to strings.
- For `FIND-TASK-001-25`, the retained implementation defect is the
  unambiguous case where the bytes actually produced from valid siblings
  already exceed the limit. No finding requires inventing a hypothetical
  canonical byte size for a numeric/deep subtree that the approved domain
  cannot encode. If the product later wants such hypothetical sizing, that is
  a specification decision, not part of this correction.
- Rejecting alias-amplifying raw encodings is an implementation enforcement of
  INV-007 and the existing canonical-input boundary, not a new public Variant
  model. Ordinary builder-produced values do not share child byte ownership.

## Final deduplicated ledger

### FIND-TASK-001-4 — Active Bifrost authority and validator rustdoc omit enforced persisted rules

- **Classification:** MISSING / VIOLATION; reopened incomplete authority-sync
  root.
- **Violated obligation:** `AGENTS.md` makes
  `architecture/bifrost-design.md` the active Bifrost authority and requires
  materially changed Rust documentation to state the real error contract. The
  specification also requires that authority to describe the shipped result.
- **Exact source:** `architecture/bifrost-design.md:144-188` and
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:197-216`.
- **Reachability:** The omitted rules are live in
  `EncodedVariant::from_bytes`, `validate_declared_variants`,
  `refuse_partial_structs`, `ResultsTable`, `CallsTable`, and the metric table
  validator. Maintainers are directed to the stale architecture before editing
  any of those paths.
- **Consequence:** The authority permits a future implementation to omit raw
  numeric-range selection or restore required Parquet leaves/partial optional
  Structs while still appearing compliant. The immediate validator rustdoc
  also falsely limits its catalog errors to size, encoding, and depth.
- **Smallest correction:** Amend the existing Variant paragraph with the raw
  numeric domain and `invalid -> numeric -> depth` selection; amend the
  existing built-in bullets with physically nullable children plus
  whole-present/null-absent admission for verification results, metric buckets,
  and gateway `resolved_model`; add numeric-range to
  `validate_declared_variants`'s existing `# Errors`. Add no new document,
  checker, or abstraction.
- **Focused proof:** Review the amended authority against revision 13 and the
  current owner, then run `mise run docs:check` and `git diff --check` during
  implementation verification.

### FIND-TASK-001-14 — Gateway nullable-Struct correction lacks its required boundary and publication proof

- **Classification:** MISSING EVIDENCE; production source currently passes.
- **Violated obligation:** TASK-001-R5 explicitly requires gateway raw/server
  refusal with exact problem, no ACK/no row, continued availability, and
  unchanged hot/published reads, including partial-present `resolved_model`.
- **Exact source:** Requirement at
  `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md:78-93,109-112,245-255`;
  current unit-only evidence at
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:2538-2661` and
  `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:1426-1447`;
  current journey reads only payloads at
  `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:1196`.
- **Reachability:** `CallsTable::WHOLE_STRUCTS` does reach the common Scribe
  validator, and `CallCapture::calls_batch` emits correct absent children. The
  gateway capture peer is nevertheless a distinct real server ingress and the
  published child projection is a distinct storage/read seam; neither is
  exercised by the cited unit tests.
- **Consequence:** A gateway-only wiring or persisted-null regression can pass
  the current suite. This is not evidence of a present production bug, and no
  production edit is authorized unless the missing proof falsifies the source
  trace.
- **Smallest correction:** Extend the existing server-owned gateway capture
  peer test to submit a calls batch with partial-present `resolved_model` and
  assert the existing `WYRD_VALA_400_SCHEMA_PARSE`, no retained row, and a
  successful following capture. Extend the existing typed built-in journey to
  flush an unresolved capture and query both `resolved_model` children as SQL
  null. Reuse `CallsTable`, `CallCapture::calls_batch`, and the current peer;
  add no public writer or harness.
- **Focused proof:** Run only the exact extended gateway peer test and typed
  built-in journey through their repository-managed `mise` environment, then
  the nearest gateway/server capability task.

### FIND-TASK-001-16 — Shared raw ranges still amplify work and renderers bypass bounded validation

- **Classification:** INCORRECT / RESOURCE SAFETY; incomplete prior closure.
- **Violated obligation:** INV-007 requires Variant processing to be bounded by
  value size/depth; REQ-004 and REQ-019 require accepted/stored values to render
  consistently and malformed stored values to fail rather than crash or
  consume unbounded resources.
- **Exact source:** `crates/shared/wyrd-queue/src/variant.rs:230-246,287-327,330-385,405-431,896-958`;
  dependency behavior at
  `parquet-variant-59.3.0/src/variant/object.rs:256-296`,
  `parquet-variant-compute-59.3.0/src/variant_array.rs:419`, and
  `parquet-variant-json-59.3.0/src/to_json.rs:168-259`.
- **Reachability:** `scan_encoded` rejects only after node visits exceed
  `value.len()`. A shallow object can point many fields at one large child and
  keep node count below that threshold while expanding JSON quadratically.
  `variant_bytes_to_json`, `variant_cell_to_json`,
  `VariantJsonEncoderFactory`, and `mask_placeholders` accept raw slices/Arrow
  arrays and call dependency recursion without `scan_encoded`; they feed every
  Rust/Python/TypeScript/MCP/HTTP/CLI terminal and Oracle placeholder path.
- **Consequence:** An admitted alias-amplifying value or a hostile/corrupt
  stored cell can consume superlinear output memory/CPU or process stack before
  returning a typed failure. The final r5 addendum proves only its exact
  exponential-node fixture.
- **Smallest correction:** In the existing borrowing `scan_encoded` owner,
  replace the node-count symptom fence with the canonical child-byte ownership
  invariant: each object/list child owns one non-overlapping encoded region,
  and shared/overlapping regions are malformed. Extract only the private
  borrowing validation step needed so `from_bytes` and all four raw renderer
  entry paths invoke that same scanner before dependency recursion; keep
  `EncodedVariant` as the sole public validated value and add no parser, model,
  renderer, option, or downstream SDK guard.
- **Focused proof:** Keep the existing shared-child fixture; add one shallow
  many-fields/one-large-child fixture whose node count is below input bytes and
  one compact hostile-depth stored cell. Exercise the shared byte renderer and
  one Arrow/JSON-encoder path (which covers the SDK/HTTP/MCP consumers), assert
  prompt typed failure, then render an ordinary value successfully. Retain the
  raw-IPC no-ACK/no-row journey for admission.

### FIND-TASK-001-18 — Recursive serde work still outruns Wyrd's JSON depth decision

- **Classification:** INCORRECT; incomplete JSON-owner closure.
- **Violated obligation:** Revision 13 requires valid JSON beyond 64 containers
  to be `VARIANT_TOO_DEEP`, numeric range to outrank depth, malformed JSON to
  remain invalid, and `try_parse_json` to suppress only invalid JSON. INV-007
  requires stack-safe bounded conversion.
- **Exact source:** `crates/shared/wyrd-queue/src/variant.rs:180-207,677-806`;
  `serde_json-1.0.150/src/de.rs:34,63,1375-1384`.
- **Reachability:** `from_json_text` fully deserializes `&RawValue` with
  serde_json's 128-level recursion guard before Wyrd's 64-level walker.
  `from_json` recursively serializes `Value` before calling that owner. Both
  paths reach buffered writes and Oracle `parse_json`/`try_parse_json`.
- **Consequence:** Valid deeper JSON becomes root `INVALID_JSON`; an
  out-of-range number below that point is never found; `try_parse_json`
  incorrectly returns SQL null; a sufficiently deep programmatic `Value` can
  consume stack before Wyrd returns `TOO_DEEP`.
- **Smallest correction:** Keep `EncodedVariant` as the one owner. For text,
  use serde_json's unbounded-depth deserializer behind bounded stack growth
  (the standard `serde_stacker` mechanism) so syntax is still fully decided,
  then retain the current non-building 64-level Wyrd walk and selector. For a
  programmatic `Value`, perform an explicit-stack 64-level preflight before
  calling recursive serialization. Do not write another JSON parser or merely
  disable serde's guard on the process stack.
- **Focused proof:** Directly exercise both constructors at depths 64, 65, 128,
  and a much deeper compact value; add malformed deep and numeric-under-deep
  cases. Extend Oracle strict/lenient and the existing pre-ACK JSON journey to
  assert exact codes, no retained row, and a successful following operation.

### FIND-TASK-001-21 — Completed r5 remediation is still marked `ready`

- **Classification:** VIOLATION; incomplete active-packet lifecycle closure.
- **Violated obligation:**
  `architecture/references/languages/spec-driven-development.md:170-179`
  defines `review` as the phase after implementation and before approval.
- **Exact source:**
  `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md:1-10,288-373`.
- **Reachability:** The remediation contains completed implementation and final
  verification evidence, the parent TASK-001 says `review`, and r6 is actively
  reviewing that implementation, while the remediation front matter still
  routes it as `ready`.
- **Consequence:** Humans or automation can treat completed work as available
  for duplicate implementation and the active packet is not authoritative.
- **Smallest correction:** Change only the R5 front matter from
  `status: ready` to `status: review`. Do not add a checker or rewrite history.
- **Focused proof:** Confirm parent and remediation both identify the current
  review phase; run `git diff --check`.

### FIND-TASK-001-24 — Raw numeric-domain validation still admits unsupported encodings

- **Classification:** INCORRECT; incomplete numeric-domain closure.
- **Violated obligation:** Revision 13 applies the exact JSON numeric domain to
  raw canonical Arrow: integer primitives fit `i64`, the sole decimal form is
  scale-zero Decimal16 for `i64::MAX + 1..=u64::MAX`, floating values must
  survive every JSON/native terminal, and all other decimals are refused
  before ACK.
- **Exact source:** `crates/shared/wyrd-queue/src/variant.rs:829-840,896-958`.
- **Reachability:** `scan_encoded` special-cases only disallowed Decimal16;
  every Decimal4, Decimal8, Float, and Double falls through. Raw built-in IPC
  reaches this function through `validate_variant_values`. Dependency full
  validation accepts these structurally. Non-finite Float/Double then fail in
  `VariantToJson`; Decimal4/8 are noncanonical decimal inputs prohibited by
  revision 13.
- **Consequence:** A write can be acknowledged with a value outside the public
  numeric domain and fail or change representation only at a later terminal.
- **Smallest correction:** Extend the existing `scan_encoded` numeric match:
  reject all Decimal4/Decimal8 as `numeric_kind: "decimal"`; retain only the
  already approved Decimal16 interval; reject non-finite Float/Double as
  `numeric_kind: "double"`; retain finite Float/Double. Record through the
  existing `VariantViolations` selector so malformed still wins. Add no
  renderer guard or new numeric type.
- **Focused proof:** Focused constructor cases for Decimal4, Decimal8,
  canonical/refused Decimal16, finite Float/Double, and NaN/infinities; extend
  the existing raw-IPC journey with exact numeric-range details, no ACK/no row,
  and a following valid write.

### FIND-TASK-001-25 — JSON conversion returns numeric/depth before a deterministically oversized encoding

- **Classification:** INCORRECT.
- **Violated obligation:** The locked write precedence is Variant encoded-byte
  limit before JSON validity, numeric range, and depth; REQ-019 requires the
  stable 413 error for an oversized value.
- **Exact source:** `crates/shared/wyrd-queue/src/variant.rs:199-207`.
- **Reachability:** `append_raw` can build a valid string sibling whose encoded
  bytes alone exceed 8,388,608 while recording a numeric or depth violation in
  another sibling. `found.finish()` returns that lower-priority violation
  before `builder.finish()` and `Self::sized`. The default 16-MiB request
  envelope leaves the case reachable through prepared JSON rows and Oracle
  parsing.
- **Consequence:** Equivalent oversized JSON and raw Arrow inputs expose
  different stable errors and the JSON path violates its locked precedence.
- **Smallest correction:** Finish the existing builder and call
  `Self::sized(metadata, value)` before `found.finish()`. This is an ordering
  correction in the existing owner, not a new sizing pass. Do not invent a
  byte size for rejected subtrees that have no canonical encoding.
- **Focused proof:** One direct and one pre-ACK JSON-row case with an oversized
  valid sibling plus (a) an out-of-range integer and (b) an over-depth sibling;
  assert exact `WYRD_VALA_413_VARIANT_TOO_LARGE`, no row, and successful
  following work. Retain isolated numeric/depth cases.

### FIND-TASK-001-26 — Raw objects with duplicate resolved names pass admission

- **Classification:** INCORRECT / DATA INTEGRITY.
- **Violated obligation:** REQ-004 requires unique stored object keys and
  INV-002 forbids silent field loss. Last-occurrence-wins applies to JSON/OTel
  normalization before encoding, not to already encoded raw Arrow objects.
- **Exact source:** `crates/shared/wyrd-queue/src/variant.rs:921-937` and pinned
  `parquet-variant-59.3.0/src/variant/object.rs:256-277`.
- **Reachability:** For unsorted metadata, both Wyrd and the dependency reject
  only `name < previous`; equal adjacent resolved names pass. A raw IPC writer
  can use two field IDs that resolve to the same string, and every built-in
  reaches this validator before ACK.
- **Consequence:** Oracle lookup/predicates and JSON map rendering can select or
  overwrite different occurrences, so an acknowledged evidence value loses
  one field and varies by terminal. Authorization is not bypassed, but the
  authorized stored value is ambiguous.
- **Smallest correction:** In the existing `scan_encoded` name check, require
  `name > previous` for both metadata modes. Preserve JSON/OTel
  final-occurrence normalization before encoding. Add no second validator.
- **Focused proof:** One focused raw constructor and one existing real-server
  raw-IPC case with valid unsorted metadata and duplicate resolved names;
  assert `WYRD_VALA_400_VARIANT_INVALID_JSON`, no ACK/no row, and a successful
  following valid write.

## Prior-finding closure summary

- Findings `1-3`, `5-13`, `15`, `17`, `19`, `20`, `22`, and `23` remain
  closed or rejected exactly as recorded by r5. No current evidence reopens
  Iceberg lineage, Bloom sizing, distributed catalog transport, SDK collection,
  Scribe WAL/ACK ordering, RBAC, tenancy, or the removed workflow-policy drift.
- Findings `4`, `14`, `16`, `18`, `21`, and `24` retain their stable IDs
  because r6 proves incomplete closure at the same authority, evidence,
  validation, JSON-owner, lifecycle, and numeric-domain roots.
- New IDs begin after 24: `25` is JSON encoded-size precedence and `26` is raw
  duplicate-name admission.

## Recommendation

**FIX_REQUIRED**

The ledger contains seven bounded implementation/artifact corrections and one
explicit proof-only closure, all decided by the approved revision 13 contract.
No product decision is missing, so `SPEC_REVISION_REQUIRED` is not warranted.
The immutable subject remained available, so `BLOCKED` is not warranted.
