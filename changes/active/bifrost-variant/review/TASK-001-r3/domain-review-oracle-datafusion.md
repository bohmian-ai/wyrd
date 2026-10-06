# Oracle/DataFusion Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation inputs: `TASK-001-R1-close-variant-contract-gaps.md` and `TASK-001-R2-exact-integers-and-late-errors.md`
- Candidate remained at the stated commit during this review.

## Reviewed boundary and coverage

| Boundary | Source and caller coverage | Result |
|---|---|---|
| Variant SQL semantics | Read `oracle/variant_sql.rs` end to end around registration, operator lowering, literal and dynamic lookup, canonical result storage, `variant_as_text`, `parse_json`, `try_parse_json`, `to_json`, and its tests. Traced construction into leader planning (`oracle/mod.rs`), admitted and follower sessions (`resources.rs`, `admission.rs`, `follower.rs`), Analytical planning/leader/worker sessions (`analytical.rs`, `analytical_scan.rs`), and physical codec fingerprints (`codec.rs`, `peer.rs`). | PASS |
| Exact JSON integers | Traced `parse_json` to `wyrd_queue::variant::EncodedVariant::from_json_text`, raw-token recursion, and `raw_number_variant`; checked manifests/features and confirmed `serde_json` `arbitrary_precision` is not enabled. Values inside `i64`, above `i64` through `u64::MAX`, and the two first out-of-range directions are directly tested. | PASS |
| Local and distributed execution errors | Traced DataFusion errors from UDF/live/scan producers through `map_datafusion_error`, first-batch and late-stream consumers, `failed_terminal_on_path`, DataFusion Distributed's pinned gRPC error conversion, and the real Interactive/Analytical late-failure journey. | **FAIL** — ODF-001 |
| Public late terminal | Traced `QueryTerminalFrame.error: Option<Box<WyrdProblem>>`, terminal validation, `wyrd.v1.proto` `error_problem_json`, tonic conversion, HTTP/gRPC route forwarding, scheduled and MCP collection, shared `wyrd-client` reconstruction, and typed result collection. The public terminal itself carries and reconstructs a full problem. | PASS once Oracle has the correct problem; producer gap ODF-001 remains upstream. |
| Rust/Python/TypeScript clients | Traced shared `wyrd-client` failed-terminal reconstruction and the Python/TypeScript Interactive journeys. The stated single-pod Interactive-only language limit is accepted: Rust V9 supplies the distributed execution proof through the same shared client reconstruction. | PASS subject to ODF-001 |
| Shared nested-field and distributed scan seam | Inspected the current Task-001 full-root `variant_get` behavior and the current `exec`/assignment/peer seams. The revised leaf predicate wire, shared per-file read-planning facade, assignment-authority v8, and physical nested-field pruning belong to TASK-003 under the approved task split (INV-008/AC-007), so their absence is not a TASK-001 finding. | N/A for this task |
| HTTP/gRPC/MCP/scheduled terminal consumers | `wyrd-server` query routes and gRPC forward the protobuf terminal; MCP and scheduled queries call the same `query::service::terminal_error`; all rebuild via `wyrd_client::error::from_problem`. No separate Variant-only terminal branch exists at these consumers. | PASS |

## Material finding

### ODF-001 — VIOLATION — distributed catalog identity is still recovered from Variant-only `Display` text

- **Violated obligation:** Revision 11 REQ-019 and the binding human decision require the late terminal to carry the full catalog problem for *every* failure in Interactive and distributed execution, with failures lacking catalog identity remaining generic. The binding direction expressly disallows a Variant-only/prose branch.
- **Exact locations:**
  - `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:650-695`
  - `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4232-4279`
  - `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:1669-1704`
- **Evidence:** `VariantQueryError::fmt` serializes a `BifrostError` to JSON text, while `VariantQueryError::decode` accepts only the four Variant variants. `catalog_query_error` then calls that decoder against every error-chain member's `to_string()`. The pinned `datafusion-distributed` transport reduces `DataFusionError::External` to its string in `protocol/grpc/errors/datafusion_error.rs:143-145` and reconstructs a generic external error, so an ordinary catalog error raised on a worker no longer has a typed `BifrostError` source at the coordinator. The special parser restores only Variant errors; worker-originated catalog errors such as `QueryTimeout` are not restored and fall through to `QueryExecutionFailed` unless a separate prose heuristic happens to recognize them. The adjacent `is_tenant_refusal` and `map_datafusion_error` branches also classify tenant/reconciliation/audit failures by message substrings, confirming that distributed identity is not one general structured contract.
- **Reachability:** Analytical expressions execute on distributed workers. The V9 journey at `wyrd-testing/tests/bifrost/oracle/published.rs:1287-1371` proves a worker-side late `parse_json` failure only because that error takes this custom Variant JSON-text path. Local `QueryTimeout` is tested only with its typed source still present (`query_stream.rs:1706-1725`); there is no equivalent remote round trip. The focused forwarded test itself constructs `DataFusionError::External(serde_json::to_string(&variant_error).into())`, explicitly proving the prose mechanism rather than a general catalog transport.
- **Observable consequence:** a late distributed failure can expose a different code/details from the same failure before streaming or on Interactive. In particular, a worker-originated timeout or another non-Variant catalog error can arrive as `WYRD_VALA_500_QUERY_EXECUTION_FAILED`, so the otherwise-correct public `WyrdProblem` terminal contains the wrong problem. This also leaves the R2 claim “no prose parsing, side channel or Variant-only branch” untrue.
- **Required testable correction:** remove `VariantQueryError`'s serialized-`Display` recovery and the catalog/message parsing in `catalog_query_error`. Carry the canonical full `WyrdProblem` through the distributed failure boundary for every catalogued `BifrostError`, using the repository's existing standard structured gRPC problem/status mechanism rather than a new Variant carrier or closed error list; only an error with no valid catalog problem maps to `QueryExecutionFailed`. Keep `failed_terminal_on_path`, tonic terminal conversion, and shared client reconstruction as the single downstream path. Extend the real Rust multi-pod late-failure journey with one non-Variant catalog error raised after at least one worker batch and assert exact code/details match its pre-stream form; retain a malformed/uncatalogued distributed failure proving the generic fallback. Delete the synthetic forwarded-text test rather than blessing that mechanism.

## Verification evidence and limits

Focused commands run on the immutable candidate:

- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=oracle::variant_sql::tests::parse_json_keeps_exact_integers_and_refuses_the_rest) | test(=oracle::variant_sql::tests::variant_operators_and_functions_follow_the_contract) | test(=oracle::query_stream::tests::late_catalog_error_keeps_its_identity) | test(=oracle::query_stream::tests::late_query_deadline_is_typed_terminal)'` — 4 passed.
- `mise exec -- cargo nextest run --locked -p wyrd-tonic --lib -E 'test(=query_conversion::tests::failed_terminal_problem_round_trips)'` — passed.
- `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::failed_terminal_problem_rebuilds_its_catalog_error)'` — passed.

The remediation records report the full V1-V17 and repository lanes green, including the Rust multi-pod V9 journey. I did not rerun the environment-owning multi-pod, Python, TypeScript, MCP, or full repository lanes in this independent review. Their recorded success does not close ODF-001 because the distributed case exercises the prohibited Variant-only text carrier and no non-Variant catalog error crosses the worker boundary.

## Overall result

**FAIL**

Exact integer handling, Variant SQL registration/semantics, the public full-problem terminal, transport conversion, client reconstruction, and the accepted Python/TypeScript harness limit satisfy this domain. ODF-001 remains a reachable violation of the binding all-errors distributed contract and of the explicit ban on Variant-only/prose recovery.
