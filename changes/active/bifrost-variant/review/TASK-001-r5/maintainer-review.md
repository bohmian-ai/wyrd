# TASK-001 R5 Maintainer Review

## Review Findings

### Critical

None.

### Important

- **MNT-R5-001 — The shared gateway `ModelRef` Arrow type weakens the required request contract while fixing the optional result.**
  - Changed location: `crates/vala/vala-bifrost-redux/src/tables/gateway/calls.rs:33-45,111-112`.
  - Governing principle: maintainer style requires the persisted declaration, owning typed contract, and documentation to describe the same operation; a shared helper must not erase a meaningful contract distinction.
  - Evidence: `GatewayCallPayloadV1` declares `requested_model: ModelRef` and `resolved_model: Option<ModelRef>` (`crates/wyrd-spec/src/gateway/record.rs:542-545`), while `ModelRef` itself requires both `provider` and `model` (`crates/wyrd-spec/src/gateway/mod.rs:96-111`). The candidate changed `CallsTable::model_ref_type` so both children are nullable to prevent padded values only when the nullable `resolved_model` parent is absent, but the same helper is also used for non-null `requested_model`. The table contract test checks only top-level nullability and therefore does not expose that unrelated weakening.
  - Concrete maintenance cost: the physical schema now permits a partial requested model that the durable Rust contract cannot represent, and a maintainer cannot infer from `model_ref_type` which of its two consumers owns the nullable-child workaround. Future validation or ingestion changes can accept invalid `requested_model` rows while still appearing schema-correct.
  - Smallest testable correction: keep `requested_model.provider` and `requested_model.model` non-null, make only the children under nullable `resolved_model` nullable, and extend the existing gateway table contract test to pin both nested layouts. Reuse the current table owner; add no new type, trait, or validation layer.

- **MNT-R5-002 — The active R4 remediation packet contradicts its revision-12 implementation and evidence.**
  - Changed location: `changes/active/bifrost-variant/review/TASK-001-r4/TASK-001-R4-close-final-variant-contract-gaps.md:6,17,49-55,290-319`.
  - Governing principle: spec-driven artifacts must identify their governing authority and remain decision-complete; maintainer documentation must not give mutually exclusive contract instructions.
  - Evidence: the packet front matter and authority section still bind specification revision 11, and the required correction still says to preserve non-null verification Struct children. The implementation-evidence section added by this candidate says a human approved revision 12, that TASK-001 now binds revision 12, and that those children are nullable; the original task and specification also now bind revision 12.
  - Concrete maintenance cost: a maintainer or later remediation agent reading the task from the top is instructed to restore the exact schema that the accepted resolution and candidate deliberately replaced. The immutable subject is also ambiguous because the packet names two different approved revisions.
  - Smallest testable correction: update the packet's front matter, authority line, outcome/correction wording, and preserved-behavior text to revision 12 and the nullable-child decision, while retaining the revision-11 diagnosis as historical evidence clearly marked as superseded. A text comparison against `spec.md` and the original TASK-001 front matter is sufficient proof.

### Suggestions

None.

## Immutable Subject

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Remediation range: `bb6ae8070..0e37748f3a27d3bcec4713e6210e97328e045886`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 12
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The candidate and tree matched the supplied identities before this report was written.

## Changed-Surface Coverage

| Surface | Owners, callers, tests, and declarations inspected | Result |
|---|---|---|
| Shared Variant encoding and JSON-row ingestion | `wyrd-queue::variant::{EncodedVariant, append_raw, check_depth, is_variant, VariantJsonEncoderFactory}`; `batch_builder::{BuiltRow, collect, collect_raw, build_variant_column}`; Oracle and client callers; focused exactness, depth, metadata, and rendering tests | PASS. The invariant-bearing type remains the single encoding owner; raw tokens are preserved only to the existing encoder, helper names expose their roles, and the deleted rendering wrapper has no replacement abstraction. |
| Scribe native planning and fixed IPC | `NativeScan::{visit_schema,push_field,visit_batch}`, `visit_nodes`, `has_unmasked_null`, Arrow decode/WAL call sequence, and fixed-IPC/material-plan tests | PASS for layout and readability. The shared mask propagation is documented at both raw-plan and decoded-array boundaries and uses Arrow's `NullBuffer`; no parallel encoder was introduced. |
| Built-in schema refusal and Variant precedence | `validate_predeclared`, `refuse_undeclared`, `validate_declared_variants`, canonical signal validation, fingerprint callers, and the raw-ingest server journey | PASS. The ordering dependency on the later fingerprint is explicit, and the undeclared-field check now has one owner shared by predeclared and canonical validators. |
| Verification Struct declarations and producer | `ResultsTable::{drift_report_fields,eval_summary_fields}`, server `drift_report`/`eval_summary`, hot and published query paths, schema fixture, unit tests, and real-server journey | PASS. Revision-12 nullable children, producer null slots, rustdoc, and query assertions agree. |
| Metrics nullable bucket Structs | `POSITIVE_BUCKET_FIELDS`, `NEGATIVE_BUCKET_FIELDS`, `PointColumns`, `push_buckets`, `bucket_struct`, table consumers, schema guide, and published OTLP journey | PASS. The projection's `Option` types mirror the declared nullable children and the journey asserts published child projections, not only parent validity. |
| Gateway nullable model Struct | `CallsTable::model_ref_type`, `GatewayCallPayloadV1`, `ModelRef`, `CallFacts`, `CallCapture`, table contract test, and unresolved-call producer test | FAIL — MNT-R5-001. The helper applies the optional resolved-model workaround to required requested-model children and the declaration test omits nested parity. |
| Oracle SQL | `OracleVariantSql` parse/render path and the added compound-precedence test, plus cumulative session registration and distributed error owners from prior rounds | PASS. The new proof stays with the existing SQL owner; no second parser, registry, or error carrier was added. |
| CLI journey | Compiled `query_server_journey`, seeded table helper, `VariantJsonEncoderFactory` wiring, JSONL assertion, exact `u64::MAX`, and raw `3.0` spelling | PASS. The behavior is exercised through the existing binary/server journey without a new harness. |
| Python RBAC assertion | `test_negative_empty_permissions_denied_rbac_on_write`, Python `WyrdError.code`, queue/client problem propagation, and sibling assertions | PASS. The test now branches on the typed error code rather than display text and adds no Python-side error logic. |
| Cumulative errors, Iceberg v3/Forge, Bloom sizing, and SDK terminals | Prior r1-r4 maintainer reports and validated findings; current owners for catalog errors, terminal problem reconstruction, v3 lineage/GC, writer properties, Rust/Python/TypeScript/MCP result projection, manifests, lockfile, proto/schema/declaration artifacts | PASS. The remediation does not introduce a second mechanism or declaration surface in these retained areas; recorded fork pins and public declaration parity match the candidate. |
| Task/spec/docs/evidence | Revision-12 spec, original TASK-001, R1-R4 verdicts/findings/remediations, Bifrost authority, schema guide, and R4 implementation evidence | FAIL — MNT-R5-002. The original task and source docs are current, but the active remediation packet remains internally contradictory. |
| Rust maintainability and tests | Materially changed symbols, module imports, bare signature types, owner/method placement, rustdoc, test names, and helper placement across the remediation range | PASS apart from the two findings above. No new single-implementation trait, zero-state utility owner, dependency, feature, compatibility path, or repository check was added. |

## Open Questions

None.

## Verification Notes

- Used CodeGraph first, then inspected the full cumulative base-to-candidate diff, the complete `bb6ae8070..candidate` remediation diff, materially changed owners, callers, tests, and the prior r1-r4 artifacts.
- `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..0e37748f3a27d3bcec4713e6210e97328e045886` passed.
- Reviewed the recorded final verification: focused queue/Oracle/Scribe/server tests; V2, V4-V15 and CLI journeys; Rust/Python/TypeScript/MCP consumers; pinned Iceberg/compaction proofs; format, lints, codegen, skill sync, and diff check. This static maintainer pass did not rerun environment-backed suites.
- MNT-R5-001 needs the existing gateway schema/unit tests extended to assert nested nullability; MNT-R5-002 needs no new runtime harness.

## Overall Result

**FAIL**

The cumulative implementation remains cohesive at its Variant, Scribe, Oracle, table, producer, and SDK owners, but the gateway physical contract now weakens required request fields and the active remediation packet gives contradictory authority and schema instructions. Both have bounded corrections in existing owners.
