# TASK-001 r3 focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Inputs: the original TASK-001, the r1/r2 remediation tasks, and every discovery report in `TASK-001-r3`
- Candidate identity was rechecked before this report and remained unchanged.

CodeGraph was unavailable because this repository has no `.codegraph/` index. I used the cumulative diff, direct source/caller tracing, and the pinned `datafusion-distributed` checkout. This pass investigated only the three assigned conflicts.

## 1. Distributed late catalog identity

### Inspected path

- Producer: `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:635-695`
- Worker transport: `/home/thorrester/.cargo/git/checkouts/datafusion-distributed-1b9be9e248e2e537/4cfa166/src/protocol/grpc/errors/datafusion_error.rs:86-183,185-310`
- Coordinator mapping: `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4208-4294`
- Late terminal: `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:450-515` and `oracle/mod.rs:3664-3699`
- Sibling catalog producers: `oracle/exec.rs:1092-1263,1500-1529,1660-1678,3030-3147`; `oracle/bindings.rs:120-147`; `oracle/live.rs:520-646`
- Worker construction and lazy leaf resolution: `oracle/analytical.rs:400-543,1573-1586`; `oracle/codec.rs:487-606,820-845`; `oracle/analytical_scan.rs:116-190`; `oracle/follower.rs:400-552`
- Proof: `oracle/query_stream.rs:1646-1725`, `oracle/mod.rs:4514-4574`, and `wyrd-testing/tests/bifrost/oracle/published.rs:1287-1371`

### Resolution

`SYS-001` and `ODF-001` are correct about the carrier, while the behavior review's claim that every executable REQ-019 path is covered is not supported.

`parse_json` wraps only Variant failures in `VariantQueryError`. Its `Display` is tagged JSON, and `decode` accepts exactly four Variant variants. The pinned distributed transport converts every `DataFusionError::External` source to `err.to_string()` at the worker and reconstructs only an external string at the coordinator. `catalog_query_error` then runs the Variant-only decoder against every source string. The candidate therefore still has the Variant-only structured branch and string inspection that r2 FIND-TASK-001-12 explicitly required to remove.

The focused unit proof does not close the universal contract. `late_catalog_error_keeps_its_identity` synthesizes the same Variant JSON text that the special decoder accepts. `variant_errors_keep_their_catalog_identity_locally_and_remotely` expressly asserts that serialized `QueryForbidden` becomes `QueryExecutionFailed`. V9 proves a worker-side `parse_json` error and an uncatalogued cast error; it does not exercise a worker-side non-Variant catalog error. Green terminal, protobuf, and shared-client tests begin after Oracle has selected a `BifrostError`, so they cannot prove the missing worker-to-coordinator identity.

The system report's specific `QueryTimeout` example is not established by the inspected worker path. `OracleExecutionGrant::ensure_live` and the live-source timeout are leader-bound owners; the Analytical worker session uses its query runtime plus `AnalyticalScanExec`, not the leader's execution bindings. That example should not be retained as source-proven consequence. A worker footer refusal is reachable as `QueryTenantInvariant`, but the coordinator currently recovers it through the separate `"tenant invariant"` message heuristic at `oracle/mod.rs:4282-4294`, which is itself the prohibited prose mechanism. The defect is thus a source-proven contract violation even though the timeout example is narrowed out.

### Proposed finding FUP-001

- **Classification:** VIOLATION / DRIFT
- **Violated obligation:** revision 11 REQ-019 and r2 FIND-TASK-001-12 require the existing full catalog identity for every late failure and explicitly prohibit prose parsing or a Variant-only branch.
- **Exact locations:** `oracle/variant_sql.rs:650-695`; `oracle/mod.rs:4240-4294`; the pinned distributed conversion at `datafusion_error.rs:143-150,185-310`.
- **Evidence:** the only structured remote decoder is closed to four Variant errors; sibling remote classification still uses message fragments; the tests prove only that special case and even require another serialized catalog error to become generic.
- **Observable consequence:** distributed identity is defined by error family and display text rather than the catalog. A reachable worker tenant refusal depends on wording to regain its code, and any worker catalog error outside the four Variant variants has no general reconstruction path.
- **Smallest correction boundary:** remove the Variant-only carrier/decoder and the catalog message heuristics. Reuse one general structured catalog carrier over the already-existing distributed external-error boundary so every worker-originated `BifrostError` is reconstructed before the existing `failed_terminal_on_path`; uncatalogued failures alone remain `QueryExecutionFailed`. Do not repin DataFusion, add a side channel, add a closed error list, or add another public error API. Extend the existing Rust multi-pod late-failure journey with one reachable non-Variant worker catalog error and retain the uncatalogued generic case.

## 2. Active Bifrost authority

### Inspected path

- Revision 11 REQ-004: `spec.md:620-644`
- Revision 11 REQ-019: `spec.md:830-847`
- Completion authority obligation: `spec.md:1038-1054`
- Prior documentation remediation: r1 FIND-TASK-001-4 and its acceptance criterion
- Current authority: `architecture/bifrost-design.md:125-162,439-465,698-721`
- Current public guide and encoder: `docs/src/content/docs/bifrost/schema.svx:31-42`; `crates/shared/wyrd-queue/src/variant.rs:650-680`

### Resolution

`INV-R3-001`, `MAINT-001`, and `REPO-R3-1` are confirmed; the behavior review's documentation PASS is incorrect.

The active authority says any integer fitting a Variant decimal is accepted and that "any other number" becomes a double. Revision 11 and the encoder instead accept only `i64`, then `u64` as scale-zero decimal, and refuse every other integral token. This is a direct contradiction, not an omitted detail.

The query authority also stops at Variant failures retaining stable code/details and a list of typed terminal outcomes. It omits revision 11's material public rule that every late catalog failure carries the same complete problem across Interactive/Analytical HTTP/gRPC, every SDK raises it unchanged, and collected partial rows are refused. Because `architecture/bifrost-design.md` is the governing Bifrost authority and r1 FIND-TASK-001-4 required it to match shipped query behavior, this is incomplete closure rather than optional documentation polish.

### Proposed finding FUP-002

- **Classification:** INCORRECT
- **Violated obligation:** active Bifrost authority must describe the approved durable Variant and query-terminal contracts; prior FIND-TASK-001-4 required parity.
- **Exact location:** `architecture/bifrost-design.md:149-152,439-465,698-715`.
- **Evidence:** the numeric sentence contradicts REQ-004 and executable source; the Variant SQL/read-terminal sections omit the all-errors full-problem rule from REQ-019.
- **Observable consequence:** the repository's winning authority directs a future producer toward reaccepting oversized decimal integers and permits a code-specific or prose late-error protocol that revision 11 rejected.
- **Smallest correction boundary:** update those existing paragraphs only: state the `i64`/`u64` classification and refusal, and state the full existing catalog problem/unchanged SDK/no-partial-result rule for every late failure. Add no document, checker, setting, option, or compatibility mechanism.

## 3. Import and qualified-signature closure

### Inspected path and rule

`architecture/agent-rules.md:7-10` requires ordinary imports at module scope and imported bare names in fields, signatures, return types, and bounds. The test-module exception permits a module import block; it does not permit imports inside individual tests. The sole local trait-enablement exception applies to forms such as `use Write as _`. R1 FIND-TASK-001-10 explicitly required the cumulative candidate to contain neither disallowed form.

The following cumulative changed symbols still violate that rule:

- Function-local ordinary imports: `oracle/query_stream.rs:1654,1678,1714`. Two existed at the base but their test bodies were materially changed, one is new, and r1's cumulative acceptance criterion explicitly required all three to move to the enclosing test-module block.
- `crates/shared/wyrd-client/src/observe/eval.rs:188`: `json_value<T: serde::Serialize>`.
- `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:2701-2713`: four `serde_json::Value` fields on the new `VariantSpanRow`.
- `crates/vala/vala-bifrost-redux/src/tables/signal.rs:1145`: qualified `serde_json::Value` return type.
- `crates/wyrd/wyrd-server/src/query/service.rs:372`: qualified `WyrdProblem` parameter.
- `crates/wyrd/wyrd-server/src/verification/results.rs:593-595,758-761,947`: qualified `Value` fields, iterator item, and return type.
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:1302-1307`: qualified `Display` bound and `WyrdProblem` parameter.

The maintainer report correctly identified the defect but did not enumerate the new `VariantSpanRow` fields or `signal.rs::json_of`; the finding should include them. The local `use std::fmt::Write as _` in `tables/mod.rs` is the documented trait-enablement exception and is not part of the finding.

### Proposed finding FUP-003

- **Classification:** VIOLATION
- **Violated obligation:** mandatory import/signature rules and prior FIND-TASK-001-10 closure.
- **Observable consequence:** the cumulative candidate claims closure while changed modules still hide dependencies inside functions and spell the same domain types inconsistently outside their module import manifests.
- **Smallest correction boundary:** move `DataFusionError` to the existing query-stream test-module import block; import `Serialize`, `Value`, `WyrdProblem`, and `Display` at each owning module/test-module top and use their bare names at the listed declarations. Preserve `Trait as _`; add no lint, checker, allow, or script.

## Follow-up result

**RESOLVED**

All three conflicts resolve from approved authority and current source. Proposed findings: `FUP-001`, `FUP-002`, and `FUP-003`. No new product, public API, architecture, security, compatibility, concurrency, or persistent-data decision is needed. No runtime lane was rerun: these conflicts are established by the immutable source, pinned transport conversion, current tests' exact scope, and the explicit repository rules.
