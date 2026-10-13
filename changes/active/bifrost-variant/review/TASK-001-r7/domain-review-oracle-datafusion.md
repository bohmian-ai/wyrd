# Oracle/DataFusion domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Repeat-review input: TASK-001 R6 verdict, remediation, and implementation evidence

The candidate commit and tree matched these identities before and after this review.

## Reviewed boundary and authority

This review traced Oracle SQL registration and execution from logical planning through local and distributed physical execution, the Variant UDFs and stored-value readers, catalog-error reconstruction, terminal framing over HTTP and gRPC, and shared-client terminal acceptance. The governing obligations were specification revision 13 `REQ-017`, `REQ-018`, `REQ-019`, `INV-003`, `INV-004`, `INV-006`, and `INV-007`, the TASK-001 Oracle/session and terminal scenarios, `AGENTS.md`, `architecture/agent-rules.md`, and the active Bifrost design.

## Coverage

| Boundary | Source and proof inspected | Result |
|---|---|---|
| One Variant SQL owner in every production session | `oracle/variant_sql.rs:77-122`; planning session at `oracle/mod.rs:3094-3109`; distributed worker at `oracle/analytical.rs:500-543`; analytical leader at `oracle/analytical.rs:7675-7693`; physical-plan and stage digests in `oracle/codec.rs` and `oracle/peer.rs` | PASS |
| Operator and function semantics | `VariantOperatorPlanner`, `VariantGet`, `VariantAsText`, `ParseJson`, and `ToJson`; Struct remains DataFusion `get_field`, literal Variant chains lower to one `variant_get`, results retain the extension, and lenient parsing suppresses invalid JSON only | PASS |
| Hot/published stored Variant reads | `mask_placeholders` is reached before Variant decoding by `variant_get`, `variant_as_text`, and `to_json`; R6 raw validation is shared rather than reimplemented in Oracle | PASS |
| Exact local and distributed catalog failures | `QueryCatalogError::external/find` and `map_datafusion_error` preserve the complete `BifrostError`; an unclassified DataFusion failure becomes `WYRD_VALA_500_QUERY_EXECUTION_FAILED`; `query_stream::tests::late_catalog_error_keeps_its_identity` covers local and forwarded envelopes | PASS |
| HTTP/gRPC terminal parity | HTTP and gRPC adapters project the same domain frames; `http_and_grpc_query_frame_parity_covers_degraded_and_late_failed` covers a failed terminal carrying the complete Variant problem | PASS |
| Interactive/distributed caller behavior | The Rust multi-pod journey covers Interactive and Analytical late Variant failures plus a generic late failure; the shared collector returns only after a validated terminal and drops accumulated batches on error; Python and TypeScript exercise the same shared client projection | PASS |
| Deep JSON resource bound | `EncodedVariant::from_json_text` validates syntax once but then reparses every nested subtree in `append_raw` and `scan_numbers`; synchronous `ParseJson::invoke_with_args` performs that work inside a DataFusion executor poll | **FAIL — ORACLE-R7-001** |

## Material finding

### ORACLE-R7-001 — INCORRECT — deeply nested JSON causes quadratic synchronous work inside Oracle

- **Violated obligation:** Revision 13 `INV-007` requires Variant conversion to be bounded by the value depth/size contract, and the R6 correction requires Wyrd to classify hostile deep input without letting dependency work outrun that bounded decision.
- **Exact location:** `crates/shared/wyrd-queue/src/variant.rs:218-226`, `:731-807`, and especially `:819-864`; the reachable Oracle consumer is `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:600-620`.
- **Evidence:** At every nested array/object, `scan_numbers` calls `serde_json::from_str` on the complete remaining subtree, so a depth-`d`, size-`n` value takes `O(d*n)` work (quadratic for compact nesting); the source itself records this at lines 819-821. The fresh exact test `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::json_depth_is_decided_by_wyrd_at_any_depth)'` passed but took 3.979 seconds of test time and about 4.74 seconds wall time while exercising only the current 10,000-level fixtures. Oracle accepts SQL up to 64 KiB and `parse_json` can also consume stored `Utf8` values, so this path is reachable by an admitted tenant query, not only by a unit fixture.
- **Observable consequence:** One compact hostile value can monopolize a DataFusion executor thread for seconds, and larger size-bounded column values scale far worse; because the scalar UDF performs this work synchronously during `poll_next`, the query deadline cannot preempt the CPU loop and concurrent queries lose executor capacity.
- **Required testable correction:** Keep `EncodedVariant` and `serde_json` as the only owners, but replace subtree reparsing with one syntax-authoritative traversal that records numeric/depth violations and builds only the accepted portion in work linear in input bytes; use the already-resolved stack-growth mechanism only if that traversal recurses, and add no parser, public API, option, or downstream Oracle guard. Prove the same malformed/size/numeric/depth precedence and duplicate-key behavior, then add a deterministic work-bound assertion (for example, a test-only visit/byte counter) showing that 1,000- and 10,000-level inputs are scanned proportionally to their encoded length rather than timing the host.

## Verification limits

- I inspected the complete cumulative Oracle/DataFusion paths and the R6 implementation delta; I did not rerun the expensive multi-pod, Python, TypeScript, HTTP, or gRPC journeys whose passing results are recorded in the task and prior remediation evidence.
- I freshly ran the exact `wyrd-queue` hostile-depth test above to confirm reachability and cost shape; it passed functionally.
- TASK-003's future shredded physical planning was not treated as TASK-001 behavior; only the current semantic UDF and unshredded/hot/published contracts were assessed.

## Overall result

**FAIL**

Oracle session registration, query semantics, distributed error identity, terminal framing, and shared-client refusal behavior satisfy their contracts, but the R6 deep-JSON correction leaves a reachable quadratic CPU path in the synchronous DataFusion UDF and therefore does not close the bounded-processing obligation.
