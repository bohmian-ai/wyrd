# TASK-001 r7 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Latest remediation: `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md`

This review inspected the complete cumulative base-to-candidate range, not only
the R6 commits. CodeGraph was used first, followed by the approved authorities,
the current implementations and callers, the prior task-review/remediation
history, and the recorded verification evidence. No other R7 report was read.

## Navigation map and realistic paths

| Capability | Producer / owner | Admission / execution | Observable result |
|---|---|---|---|
| JSON-authored Variant | `BatchBuilder` or Oracle `parse_json` -> `EncodedVariant::from_json_text` | canonical encode and violation selection | pre-ACK catalog refusal, SQL Variant, or catalog query error |
| Raw Arrow Variant | SDK/raw IPC or an OTLP projection -> `EncodedVariant::from_bytes` | `validate_declared_variants` at Scribe, or signal record projection | accepted row, pre-ACK refusal, or OTLP per-record partial success |
| Stored Variant rendering | `variant_bytes_to_json`, `variant_cell_to_json`, `VariantJsonEncoderFactory`, Oracle placeholder masking | shared `EncodedVariant::validate` -> upstream renderer | Rust/Python/TypeScript/HTTP/MCP/CLI native JSON or typed error |
| Nullable gateway Struct | gateway capture or capture-peer IPC -> `CallsTable::WHOLE_STRUCTS` | common pre-ACK whole-Struct validator | hot and published child projections |
| Durable/query contracts | catalog v3 creation -> Scribe -> Oracle/Forge | shared Oracle registration, late-error envelope, lineage rewrite/GC | consistent SDK queries and stable v3 row identity |

The retained behavior defect is at the shared JSON producer. Both public row
insertion and Oracle parsing reach it, and its over-depth scan is also executed
once per input value in a query batch.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/REQ-002, INV-003/INV-006, AC-002: v3-only tables, hidden lineage across repeated rewrites, and v3 GC | cumulative catalog/Forge changes and pinned Iceberg/compaction revisions retain the internal metadata-column transport and five-field handoff | recorded focused Forge journey and fork tests pass | PASS |
| REQ-003/REQ-004, INV-001/INV-002: one canonical Variant type, fingerprint, exact accepted values, unique object keys, and missing/null distinction | `wyrd-queue/src/variant.rs`, `tables/mod.rs`, schema/wire projections, shared result renderers | 68 queue tests, built-in contract tests, raw-IPC journey, and prior SDK/MCP/CLI journeys are recorded green | PASS |
| REQ-004/REQ-019 and the revision-13 raw domain: canonical raw object layout, exact decimals, finite floats, and size -> malformed -> numeric -> depth selection | `EncodedVariant::validate`, `scan_encoded`, `object_field_slots`, and every stored-value renderer route through the same borrowed gate | raw shared/overlap, duplicate-name, numeric-domain, hostile-depth renderer, and pre-ACK raw IPC proofs pass | PASS |
| REQ-004/REQ-019, INV-007: JSON syntax and numeric precedence at arbitrary depth must remain bounded by the approved depth/size limits | `append_raw` stops building at depth 65, but `scan_numbers` reparses every remaining subtree (`variant.rs:822-864`) | the 10,000-level unit, Oracle, and pre-ACK cases prove classification but do not prove bounded work; the focused 20 KB unit case took 3.835 seconds in this review | **FAIL — `BVR-R7-BEH-001`** |
| REQ-006/REQ-007/REQ-008/REQ-011, AC-001: signal Variants, promotions, and OTLP/canonical equivalence | table schemas and projections use typed Structs/Variants and the shared numeric gate | signal unit suites and existing span/log/metric journeys are green; R6's newly exposed non-finite OTLP refusal has only an in-process projector test | **FAIL — `BVR-R7-BEH-002`** |
| REQ-009/REQ-010, AC-003: verification summaries and other named payloads use the typed persisted shapes | table schemas, named producers, and query renderers use nullable Struct children or Variant as approved | server and SDK/MCP built-in journeys are recorded green | PASS |
| R6 gateway closure: partial `resolved_model` is refused and absent children remain null hot and published | `CallsTable::WHOLE_STRUCTS`, common preflight, gateway capture | capture-peer refusal plus hot/published gateway child assertions pass | PASS |
| REQ-017/REQ-019, INV-004/INV-006, AC-005/AC-008: one SQL registry, exact Struct access, semantic Variant access, stable late errors, sensitivity before IO | `OracleVariantSql::install`, `QueryCatalogError`, and shared client terminal collection remain the cumulative owners | session matrix and late-error journeys are recorded green; JSON parsing inherits `BVR-R7-BEH-001` | FAIL only for `BVR-R7-BEH-001` |
| REQ-005/AC-009: Scribe and Forge Bloom filters use row-group capacity and retain native folding/FPP | shared writer-property calculation | focused Bloom test recorded green | PASS |
| R6 lifecycle/authority closure | R5 status is `review`; Bifrost authority and validator rustdoc state the revision-12/13 rules | docs, skill sync, and diff checks recorded green | PASS |
| Scope and non-goals | cumulative diff adds no migration, compatibility alias, second Variant model/renderer, shredding implementation, DataFusion repin, or per-language validator | cumulative source/diff inspection | PASS |

## Proposed findings

### BVR-R7-BEH-001 — over-depth JSON classification has quadratic CPU amplification

- **Classification:** INCORRECT
- **Violated obligation:** INV-007 requires Variant conversion to be bounded by
  the approved value depth and size, while REQ-019 requires an over-depth value
  to be completely inspected for the higher-priority numeric error; the R6
  outcome likewise requires bounded JSON admission.
- **Exact location:** `crates/shared/wyrd-queue/src/variant.rs:742-744` and
  `crates/shared/wyrd-queue/src/variant.rs:819-864`, reached from JSON row
  preparation and Oracle `parse_json`/`try_parse_json`.
- **Evidence:** after depth 65, `scan_numbers` puts each nested `RawValue` on an
  explicit stack but calls `serde_json::from_str` on that node's complete
  remaining text before visiting its child. A single-item array chain therefore
  scans lengths `n + (n-2) + ...`, making work quadratic in input length. The
  code and R6 evidence acknowledge `depth x subtree size`; the claimed 8 MiB
  Variant limit does not bound this path because an over-depth subtree is not
  built and contributes zero encoded bytes. The exact focused 10,000-level
  test, whose JSON text is only about 20 KB, took 3.835 seconds after compilation
  in this review.
- **Observable consequence:** a tenant can submit a compact deeply nested JSON
  row, or evaluate `parse_json` over such text values, and consume disproportionate
  CPU before receiving the expected depth refusal; larger request-valid inputs
  can monopolize an ingest or query worker despite the fixed depth contract.
- **Required testable correction:** keep `EncodedVariant` and `serde_json` as
  the sole owners, but replace repeated subtree deserialization below the depth
  boundary with one stack-safe traversal of the already syntax-validated input
  that records the same first numeric violation and pointer under the existing
  last-key-wins/object-order rules. Retain size/invalid/numeric/depth precedence
  and add a focused hostile-depth work-bound regression plus the existing exact
  classification cases; do not add another public parser or per-caller guard.

### BVR-R7-BEH-002 — the new OTLP non-finite refusal is not proven through the user boundary

- **Classification:** MISSING
- **Violated obligation:** AGENTS.md section 11 requires a user-journey test for
  new user-observable behavior, especially a negative flow with cross-boundary
  state; REQ-004, REQ-011, INV-002, and AC-001 require signal Variant input to
  follow the same numeric contract without dropping valid sibling records.
- **Exact location:** the only new proof is
  `crates/vala/vala-bifrost-redux/src/tables/logs/mod.rs:170-203`; the existing
  real OTLP journey at
  `crates/wyrd/wyrd-testing/tests/bifrost/otlp/logs_export.rs:489-531` proves
  partial success for oversize but not for NaN/infinity.
- **Evidence:** R6 newly makes an OTLP span, log, or metric containing NaN or
  infinity reject exactly its own record. The added test calls
  `project_resource_logs` directly and checks only its batch/outcome; it does
  not cross the gRPC/HTTP collector response, Scribe acknowledgement,
  publication, or Oracle readback. The R6 command record ran that unit test but
  no OTLP journey after this behavior changed.
- **Observable consequence:** the evidence cannot establish that an exporter
  receives the promised partial-success count/code, that the invalid record is
  absent durably, or that its valid sibling still lands through the real
  collector-to-query path.
- **Required testable correction:** extend the existing log Variant OTLP journey
  (no new harness) with one NaN-bodied record and one valid sibling, assert one
  rejected record with `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`, then publish
  and query to prove only the sibling persisted; the projector unit test may
  remain as the narrow rule proof.

## Verification notes

- Ran
  `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::json_depth_is_decided_by_wyrd_at_any_depth)'`:
  1 passed, 67 skipped, test time 3.835 seconds.
- The R6 implementation record reports 68 `wyrd-queue` tests, 847
  `vala-bifrost-redux` tests with Postgres, the focused Oracle/log/gateway unit
  tests, two server journeys, the gateway peer journey, and format, lints,
  codegen, skill sync, docs, and diff checks all exiting zero.
- I did not rerun the environment-backed journeys or the full cumulative task
  suite. The recorded evidence is credible for the passing matrix rows; it
  lacks the OTLP boundary proof above and its deep-JSON tests demonstrate
  classification rather than a safe work bound.

## Overall result

**FAIL**

The R6 corrections close their stated canonicality, precedence, gateway,
lifecycle, and authority gaps, but the shared over-depth JSON scan still has a
reachable quadratic resource-amplification path and the newly exposed OTLP
non-finite refusal lacks the required real user-journey proof.
