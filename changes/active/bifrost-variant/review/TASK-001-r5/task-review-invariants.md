# TASK-001 invariant review — round 5

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Remediation range: `bb6ae8070e276c20011e675ba1a804f0356e51ea..0e37748f3a27d3bcec4713e6210e97328e045886`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 12
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

I reviewed the cumulative base-to-candidate change, the four prior verdicts and
remediation tasks, and the round-4 implementation evidence. I traced Variant
JSON/raw-byte values from the shared encoder through built-in admission and
Oracle, nullable Struct state from producers through Scribe and Parquet field
projection, and recorded proof back to the current owners. CodeGraph was used
first for the current symbol/caller map.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001, REQ-002, INV-003, INV-006, AC-002: v3-only creation, hidden-lineage preservation, repeated rewrite, v3 GC, and unchanged logical handoff | Catalog/Forge cumulative source and final pins `iceberg-rust@e999331f...`, `iceberg-compaction@2b65fa18...` | Task V10/V12/V13 evidence and prior independent fork review; no r5 source change on this path | PASS |
| REQ-003: one canonical Variant extension, logical fingerprint tag `0x0d`, exact empty extension metadata, and SDK/wire projection | `wyrd-queue/src/variant.rs:403-442`; schema/table conversions and generated contracts | `variant_extension_requires_empty_metadata` and `variant_contract_and_builtin_schemas_are_stable` rerun in r5 | PASS |
| REQ-004, REQ-019, INV-002, INV-007: exact JSON numeric meaning, fixed size/depth limits, and numeric-range-before-depth precedence | Raw-token conversion is owned by `EncodedVariant::from_json_text`, but `append_raw` skips an entire container as soon as entering it would exceed depth 64 (`variant.rs:619-634`) | Existing direct/Oracle/write proofs put the numeric and deep branches in sibling keys; they do not place the out-of-range token inside the skipped over-depth subtree | **FAIL — `INV-R5-001`** |
| Raw encoded Variant validation remains bounded and cannot lose the server process | `EncodedVariant::from_bytes` checks size, catches shallow-access panics, stops its depth walk at 65 frames, then delegates full validity to upstream (`variant.rs:167-197,752-790`) | `raw_depth_is_bounded_before_full_validation` rerun in r5; round-4 raw-IPC journey records post-refusal availability | PASS |
| REQ-006–REQ-011, INV-001, INV-005, AC-001, AC-003: built-in logical layouts, Variant replacements, promotions, and producer closure | Built-in table ledgers and producers emit the revision-12 shapes; absent verification summaries now carry null parents and null child slots | Schema test rerun; task V2–V8/V14–V15 and round-4 focused evidence cover normal owner-produced rows | PASS for declared layouts and owner-produced rows; trust-boundary completeness fails below |
| Revision-12 persisted Struct invariant: an absent `drift_report`/`eval_summary` has null children, while a present Struct has every child | The producer satisfies both halves (`wyrd-server/src/verification/results.rs:691-746`), but the public table declares every child nullable (`tables/verification/results.rs:38-67`) and its default validator checks only schema shape plus nested Variant bytes (`tables/mod.rs:272-291,348-395`) | Existing tests cover absent and complete present rows, not an authorized raw batch with a valid parent and a missing child | **FAIL — `INV-R5-002`** |
| FIND-TASK-001-15 closure: schema/extension refusal precedes Variant value traversal | `validate_predeclared` refuses undeclared columns, gates the value walk on declared shape, and `validate_declared_variants` checks Variant wire identity before bytes (`tables/mod.rs:217-314`) | Round-4 raw-IPC journey covers undeclared and non-Variant type changes competing with invalid/deep/oversized Variant values | PASS |
| REQ-017, INV-004, AC-005, AC-008: one Oracle registration owner, Struct `get_field`, semantic `variant_get`, sensitivity before provider/source IO, and stable local/distributed failures | `oracle/variant_sql.rs`; general `QueryCatalogError` carrier in `oracle/mod.rs`; shared client reconstruction | Prior focused Oracle/tonic/client and multi-pod evidence; no r5 source change on the carrier | PASS |
| REQ-005, AC-009: row-group Bloom capacity with native parquet-rs NDV folding | Shared writer properties retain the native row-group-derived behavior | Task V11 and prior independent focused proof | PASS |
| Rust/Python/TypeScript/MCP/HTTP/gRPC/CLI Variant rendering and no partial query results | Shared byte/Arrow renderers and terminal problem projection; compiled CLI journey now queries Variant `3.0` and `u64::MAX` | Round-4 CLI and cross-language journey evidence | PASS |
| Prior finding closure: FIND-14 through FIND-20 | Null-child projection, predeclared ordering, bounded raw depth, extension metadata, sibling-order numeric precedence, CLI coverage, and dead wrapper removal are present | Focused round-4 evidence plus r5 unit reruns | PASS except the broader precedence and present-Struct cases in `INV-R5-001`/`002` |
| FIND-TASK-001-21 and completion evidence: the task record describes the actual candidate and proof owners | The task now names revision 12 and the final fork pins, but its Oracle evidence still cites deleted `remote_variant_error` instead of the current general `QueryCatalogError` owner (`TASK-001-variant-storage-and-query.md:255`) | Repository search finds no `remote_variant_error` symbol; current carrier is `oracle/mod.rs:4258-4306` | **FAIL — `INV-R5-003`** |
| Non-goals and ownership boundaries | No shredding, TASK-003 DataFusion repin, migration, compatibility alias, second reader/model, public lineage API, or new configuration entered the remediation | Cumulative and remediation diffs inspected | PASS |

## Proposed findings

### INV-R5-001 — INCORRECT: numeric-range precedence still fails when the number is inside the over-depth subtree

- **Classification:** INCORRECT; prior `FIND-TASK-001-18` is not fully closed.
- **Violated obligation:** Spec revision 12's locked write order places numeric range before depth, and REQ-004/REQ-019 require the same stable result independently of JSON layout. The round-4 remediation required retaining depth while continuing the bounded inspection needed to discover a higher-priority numeric violation.
- **Exact location:** `crates/shared/wyrd-queue/src/variant.rs:619-634`; reachable consumers at `variant.rs:154-165`, `crates/shared/wyrd-queue/src/batch_builder.rs:376-385`, and `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:535-614`.
- **Evidence:** `append_raw` calls `enter_container`; on the first container at depth 65 it stores `TooDeep` and immediately returns `Ok(())`. It never deserializes or inspects that raw subtree. Therefore a compact value such as 65 nested arrays whose innermost value is `18446744073709551616` returns `TooDeep`, while placing the same out-of-range token in an in-limit sibling returns `NumericOutOfRange`. Both are valid JSON, remain far below the request and encoded-size ceilings, and reach this owner through `parse_json` and JSON-row admission. The existing tests only swap two sibling object keys, so they stay green while this path remains.
- **Observable consequence:** Semantically equivalent compound-invalid inputs can still select different public catalog codes and details based on whether the numeric token is inside or beside the first over-depth container.
- **Required testable correction:** At the existing `EncodedVariant::from_json_text` raw-token owner, retain the first depth violation but continue a bounded numeric-token inspection of the skipped raw subtree before selecting the final error. Reuse the existing serde/raw-token mechanism and numeric classifier; add no second Variant model or configurable limit. Prove direct conversion, Oracle `parse_json`, and one pre-ACK write where the out-of-range integer is inside the depth-65 branch, plus the existing sibling-order cases.

### INV-R5-002 — INCORRECT: nullable persisted children allow incomplete present verification Structs through the trust boundary

- **Classification:** INCORRECT / REGRESSION introduced while closing `FIND-TASK-001-14`.
- **Violated obligation:** Revision 12's locked persisted-Struct contract states that children are nullable to preserve absent-parent reads, but producers write every child of a present `drift_report` or `eval_summary`. INV-002 forbids silently storing an incomplete/retyped built-in value, and TASK-001 requires server-side built-in contract enforcement before ACK/WAL.
- **Exact location:** `crates/vala/vala-bifrost-redux/src/tables/verification/results.rs:38-67`; default admission at `crates/vala/vala-bifrost-redux/src/tables/mod.rs:272-291,348-395`; correct owner producer at `crates/wyrd/wyrd-server/src/verification/results.rs:691-746`.
- **Evidence:** Revision 12 necessarily makes each child nullable so nulls survive Parquet. `ResultsTable` still uses the generic predeclared validator. That validator accepts a schema matching those nullable children, and `validate_variant_values` explicitly returns `Ok` for a null child. An authorized raw Arrow batch can therefore set `drift_report` valid while leaving `method`, `features`, or `verdict` null (or set a partial `eval_summary`), pass the fingerprint and Variant validation, and reach WAL. The Rust producer and current tests only generate either a null parent with all-null children or a present parent with all children, so they do not exercise the invalid third state.
- **Observable consequence:** Durable `vala.verification.results` rows can claim a present typed report while omitting contract members; field queries and SDK decoders then observe a partial report no `DriftReport` or `EvalWorkflowSummary` producer can represent.
- **Required testable correction:** Keep revision-12 child nullability, but enforce the all-or-none invariant in the existing table-owned built-in validation boundary: when either Struct parent is valid, every declared child must be valid; when the parent is null, child slots remain null. Reuse the existing `SchemaParse` contract for an invalid built-in row and preserve Variant validation/error precedence. Add a raw-IPC trust-boundary test for each Struct that proves a partial present value is refused before ACK/WAL, followed by a valid write; retain the hot/published absent-parent journey.

### INV-R5-003 — VIOLATION: the final task evidence still names a deleted distributed-error owner

- **Classification:** VIOLATION; prior `FIND-TASK-001-21` is only partially closed.
- **Violated obligation:** The remediation required the tracked task's final evidence to describe the actual immutable candidate. Repository completion requires credible, factual verification evidence.
- **Exact location:** `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md:255`; actual owner `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4258-4306`.
- **Evidence:** The evidence row claims `oracle/mod.rs` uses `remote_variant_error` to preserve catalog identity. That symbol was deleted by the round-3 generalization; the candidate uses `QueryCatalogError`, which carries every catalogued `BifrostError`, not a Variant-only decoder. Repository search finds the old name only in this evidence row.
- **Observable consequence:** A later reviewer or maintainer following the completion record cannot locate the claimed mechanism and is told the distributed contract is Variant-specific when the actual invariant is general.
- **Required testable correction:** Update the existing evidence row in place to name `QueryCatalogError` and the general structured local/remote catalog-error reconstruction it proves. Add no new evidence artifact or checker; verify the cited symbol exists and `git diff --check` passes.

## Prior-finding closure

| Stable finding | Result |
|---|---|
| `FIND-TASK-001-1` through `-17` | Closed for their recorded symptoms, except the new present-Struct state made reachable by revision 12 is `INV-R5-002`. |
| `FIND-TASK-001-18` | **Not fully closed:** sibling key order is fixed, but an out-of-range token inside the first skipped over-depth subtree is never inspected (`INV-R5-001`). |
| `FIND-TASK-001-19`, `-20` | Closed. |
| `FIND-TASK-001-21` | **Partially closed:** revision, placeholder owner, and fork pin were corrected; the same final evidence still cites deleted `remote_variant_error` (`INV-R5-003`). |

## Verification notes

Rerun on the candidate through the repository toolchain:

```text
wyrd-queue variant::tests::variant_extension_requires_empty_metadata                 PASS
wyrd-queue variant::tests::numeric_range_outranks_depth_in_any_key_order             PASS
wyrd-queue variant::tests::raw_depth_is_bounded_before_full_validation               PASS
vala-bifrost-redux tables::tests::variant_contract_and_builtin_schemas_are_stable    PASS
```

These tests prove the cases they name. They do not cover a numeric token inside
the skipped depth-65 subtree, a present verification Struct with one nullable
child omitted at the raw Arrow trust boundary, or factual symbol parity of the
task evidence. The environment-backed V1–V17 and round-4 remediation commands
are recorded as passing; I did not rerun the full Postgres/language suite in
this discovery review.

The candidate and tree were re-resolved after review and remained
`0e37748f3a27d3bcec4713e6210e97328e045886` /
`f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`.

## Overall result

**FAIL**

Two reachable data-contract gaps and one factual completion-evidence defect
remain. Each has a bounded correction at an existing owner and requires no
specification revision.
