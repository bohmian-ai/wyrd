# TASK-001 System-Resilience Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 11
- Task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation inputs: `TASK-001-R1-close-variant-contract-gaps.md` and `TASK-001-R2-exact-integers-and-late-errors.md`
- Candidate identity was rechecked before writing this report.

## Deployed-path coverage

| Changed path | Deployed topology and ownership | Failure/recovery assessment |
|---|---|---|
| Built-in and user ingest | Authenticated client/OTLP producer -> Gate -> Scribe admission -> WAL fsync and batch fence -> active/immutable rows -> staged Parquet -> published Iceberg | Variant validation is repeated at the server trust boundary before acknowledgement. A refusal mutates neither WAL nor admitted state. Accepted Variant bytes use the ordinary Scribe durability and replay lifecycle; no new restart-only representation or side store was added. |
| Interactive query | HTTP/gRPC ingress -> Oracle authenticated provider and one registered Variant SQL session -> published/hot/live sources -> framed Arrow stream -> terminal -> shared `wyrd-client` reconstruction -> Rust/Python/TypeScript/MCP consumers | Local execution errors retain typed `BifrostError` sources, are converted once to a full `WyrdProblem`, and fail the terminal. Oracle cancels descendants, joins settlement, releases admission and active-read ownership, and emits the terminal only after cleanup. The shared client accepts a terminal only at clean EOF and returns no collected partial result after failure. |
| Analytical query | Leader Oracle -> frozen multi-pod graph -> follower worker sessions with the same Variant registry and peer-contract version -> leader merge -> public terminal | Session parity and peer version binding prevent a worker from decoding a plan with different Variant functions. Graph cancellation and settlement precede terminal emission. However, worker errors cross the distributed boundary as rendered text, and the candidate reconstructs only the four Variant error variants; other reachable catalogued worker errors lose their identity. See `SYS-001`. |
| Iceberg v3 maintenance | Scribe publication with `first_row_id` -> Forge compaction through pinned `iceberg-compaction` -> atomic snapshot commit -> v3 manifest/snapshot/orphan maintenance | Fresh and built-in tables are v3-only and physical validation rejects another version. Standard Iceberg v3 hidden lineage is copied through rewrites; the repository adds no duplicate-ID scan or optional-metrics gate. Failed rewrite work does not become a committed snapshot, and restart continues through existing Forge task/object evidence. |
| Process stop/restart | Server-owned Scribe runtime and Oracle/Forge owners | The change does not bypass the existing bounded Scribe drain, WAL/staged-run replay, Oracle cancellation, or Forge idempotent commit/reconciliation paths. Variant data is stored in the normal Arrow/Parquet/Iceberg representation, so replay does not depend on a process-local cache. |

## Failure-path evidence

- `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:450-515` converges batch errors and local failure events on `failed_terminal_on_path`, then `settle_and_finish_stream` cancels request/stream work, joins distributed work, drains child reservations, settles the analytical graph, releases active reads, and only then returns the terminal (`:656-716`). This keeps unrelated server capabilities online while failing only the request.
- `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3674-3698` turns the selected catalog error into its full RFC 9457 problem. `crates/wyrd-spec/src/vala/api.rs:809-904` requires exactly one problem on a failed terminal, while `crates/wyrd/wyrd-tonic/src/query_conversion.rs:462-492,581-589` validates the wire document and row count.
- `crates/shared/wyrd-client/src/bifrost/query.rs:779-813` waits for clean EOF before accepting either success or failure; a failed terminal becomes `FailedTerminal`, and collection does not return already buffered batches. `crates/shared/wyrd-client/src/bifrost/query.rs:127-138` and `crates/shared/wyrd-client/src/error.rs:148-155` rebuild the catalog error through the shared client owner used by every language binding.
- `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:999-1099` creates and validates v3 tables. `crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs:1450-1570` exercises repeated production rewrites, maintenance, later publication, and shutdown while comparing each observed row's standard hidden lineage tuple. The binding decision correctly excludes duplicate-ID scans and metrics gates.

## Affected capabilities

- `SYS-001` affects only late failures originating on an Analytical follower. Successful distributed queries, local Interactive failures, the specifically wrapped Variant failures, and unrelated server capabilities are not taken offline.
- Iceberg publication, repeated Forge rewrite, v3 garbage collection, and post-maintenance publication use their existing atomic/recoverable owners. No material availability or recovery regression was found in those paths.
- Python and TypeScript proving late failure only through the single-pod Interactive harness is the accepted product limit. Multi-pod Analytical behavior is a server/shared-client contract and is appropriately exercised in Rust; that accepted limit is not a finding or a verification deficiency.

## Material finding

### SYS-001 — DRIFT / INCORRECT: distributed late failures preserve only Variant catalog identities

- **Violated obligation:** Specification revision 11 REQ-019 and the binding decision require every late failure with a catalog identity to carry the same full catalog problem in Interactive and distributed execution. R2 also explicitly excludes prose parsing and a Variant-only branch.
- **Exact location:** `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:648-681`; `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4259-4281`.
- **Evidence:** `VariantQueryError::Display` serializes a `BifrostError` into the worker error's display string. `VariantQueryError::decode` accepts only `VariantInvalidJson`, `VariantNumericOutOfRange`, `VariantTooDeep`, and `VariantTooLarge`. `catalog_query_error` reparses every error-chain string through that Variant-specific decoder. Its own test at `oracle/mod.rs:4555-4574` requires another serialized catalog error to collapse to `QueryExecutionFailed`. Meanwhile, follower-executed leaves can raise other typed catalog errors: `oracle/bindings.rs:125-146` returns `QueryTimeout` before row IO and `oracle/exec.rs:1674` returns `QueryTenantInvariant`. Once rendered by the distributed transport, the timeout is not recognized by `catalog_query_error` or the later message heuristics and reaches the public failed terminal as `WYRD_VALA_500_QUERY_EXECUTION_FAILED` instead of `WYRD_VALA_504_QUERY_TIMEOUT`.
- **Observable system consequence:** the same late failure changes code, HTTP status, remediation, and retry meaning solely because the physical plan placed the failing leaf on a follower. Clients can treat a retryable timeout as an internal execution fault. Cleanup remains request-scoped and safe, but the recovery instruction exposed to the caller is wrong.
- **Why this is DRIFT:** the Variant-only display-string carrier is a bespoke mechanism absent from the approved contract and from the standard RFC 9457 problem already used on Wyrd's HTTP, gRPC, and terminal boundaries. It exists only to patch one error family through a generic distributed failure channel and is expressly excluded by the remediation direction.
- **Smallest testable correction:** delete the Variant-only carrier/decoder. Reuse the existing catalog/problem owner for one generic structured catalog-error carrier at the distributed execution boundary, so any reachable `BifrostError` retains its exact problem and catalog-less failures alone map to `QueryExecutionFailed`. Prove a follower-originated non-Variant catalog error (at minimum `QueryTimeout`) after at least one batch yields the same problem as its pre-stream/local form, settles the graph, releases ownership, and returns no partial result. Do not add a second public error API, side channel, compatibility field, or error-specific parser.

## Recovery and proof assessment

The supplied implementation evidence reports V1-V17 green on the final candidate, including the real multi-pod V9 journey, repeated-rewrite V10 journey, pinned-fork lineage checks, language journeys, code generation, and formatting/lint lanes. I also ran the exact focused unit selection for `oracle::query_stream::tests::late_catalog_error_keeps_its_identity`, `oracle::query_stream::tests::late_query_deadline_is_typed_terminal`, and `oracle::tests::late_failure_terminal_is_closed_and_non_success`; all three passed. Those tests prove local timeout handling and local/forwarded Variant reconstruction, but they do not exercise a follower-originated non-Variant catalog failure. V9 deliberately checks distributed late invalid JSON plus an unrelated generic cast error, so it also does not close `SYS-001`.

## Overall result

**FAIL**

The storage, restart, Forge, cancellation, settlement, and client partial-result boundaries are credible, but the deployed multi-pod error path violates the required catalog identity for reachable non-Variant follower failures. The defect is bounded to the existing distributed error carrier and has a decision-complete correction within the approved contract.
