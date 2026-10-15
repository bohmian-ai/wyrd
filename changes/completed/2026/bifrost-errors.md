# Bifrost caller errors and live-table subqueries

- Change ID: `SPEC-bifrost-errors`, approved revision 1.
- Completed: 2026-10-14.
- Reviewed task candidate: `95b350d078226e811ac298230c233bf935975d42`.
- Completion target: `3d5dfe46f90665d59c122e3f4a95f9bdd2bca7f2`, after integrating main `5bcd9e4872f1376f1722922fa63b67384243be04`.
- Delivery reference: branch `split/bifrost-errors`.
- Completion authority: explicit owner approval using the passing task review, without another final change review. No final-change-review PASS is claimed.

## Intent and shipped behavior

Callers can repair invalid SQL from consistent planner messages and query live
evidence using ordinary analytical joins and subqueries. Logical planning
failures caused by unknown columns/functions, type mismatch or ambiguity return
the existing 4xx catalog error with the authorized planner detail and suggestion
across HTTP, Rust, Python, TypeScript and MCP. Internal and physical-planner
diagnostics retain their redacted error boundary.

Scalar, IN and EXISTS queries over live tables return equivalent-join rows,
including managed run_id correlation. Scalar inputs with more than one row
fail rather than silently acting as a set; equal values and NULL rows still
count. Valid correlated unique-key lookups retain support for pure expressions
of outer values, including CASE, ranges, lists, patterns and null/truth tests.

Selective execution-bound predicates reach remote live scans as resolved
literals. A compact fixed corpus covers common query shapes over mixed live
and published data, including joins, subqueries, a joined CTE and windows.

## Lasting constraints and decisions

Existing authenticated tenant identity, authorized table cuts, signed fragment
bindings, query-owned cancellation/resources and terminal-safe results remain
authoritative. Planner suggestions use only admitted providers. A failed query
never turns partial rows into a successful complete result.

DataFusion owns scalar snapshots, sound row bounds, fixed-expression proofs
and native scalar cardinality. Its registered single_value aggregate uses the
ordinary partial/final count state and codec to validate rows across workers;
there is no new Wyrd evaluator, node, protocol or query-shape guard. Native
fixed-value proof accepts an ordinary pure composition only if every child is
fixed for the inner invocation. Input-dependent, volatile and opaque scoped
forms remain unproven. Composite-key and nullable-key rules are preserved.

The native fork is pinned to `b8ec4b1d291cd8e371bb88fc76959d91e8503a1d`;
manifest, lock and Oracle codec revision agree. The distributed fork retains
its physical-scalar restriction. Two logical optimizer passes allow rewritten
equality predicates to become native hash keys. The live leaf snapshots ready
predicates at first poll, assigning planned routes to partitions before further
pruning, while the leader retains authoritative filtering.

Revision 1 remains the approved behavior. Owner-directed compact common-query
coverage supplements it; completion introduces no behavior or architecture
revision. Broad generated query-oracle machinery was removed in favor of
existing fixtures, explicit results and native engine tests.

## Acceptance evidence and limits

| Obligation | Evidence retained in the implementation |
|---|---|
| REQ-002 actionable caller errors and safe disclosure | Exact mapper unit plus first-class SDK/MCP journeys assert planner detail, suggestions and forbidden-table non-disclosure. |
| REQ-006 live subqueries and correlation | Real SDK/server and MCP journeys compare explicit expected rows with equivalent joins over live records and managed run_ids. |
| Scalar correctness and preserved valid queries | Native zero/one/multiple/equal/NULL and partial-merge controls; sound FULL JOIN/grouping/overflow bounds; fixed-expression child matrix and correlated SQL positives/negatives; remote scalar refusal and grouped CASE journeys. |
| Selective transfer and partition ownership | Exact tests inspect static/resolved literals, re-execution and disjoint routes; remote journey asserts two fragments and two footer rows. |
| INV-001 tenant isolation | Independent source tracing and scoped unauthorized-table journeys preserve existing provider, cut and fragment authority. |
| Common query coverage | Fixed mixed-storage two-table corpus asserts join, grouping, scalar, membership, EXISTS, NULL, joined CTE and window results. |
| AC-003 owning verification | Recorded implementation evidence includes focused native/Wyrd units, codecs, peer journeys and corpus, plus format, lint, codegen, dependency and tenancy checks. All five task findings were independently closed. |

The current-pin native evidence includes one subquery SQL file, expr265,
optimizer886, single_value4 and proto library17; library17 is distinct from
the earlier external proto265 run. Broader historical results are not relabeled
as current-pin executions. The latest-main integration was conflict-free;
canonical workspace-hack regeneration and its check passed.

Closeout verification was stopped at the owner's direction. Formatting and
Clippy completed, but the full Bifrost gate and a new codegen run are not
claimed as passed. No final integrated review or benchmark acceptance is
claimed. Performance evidence is structural pruning, native readiness and
bounded transfer, rather than measured latency or throughput. Runtime scalar
cardinality errors keep the existing opaque execution-failure code. A historical
untraced memory-reservation observation remains unexplained; no new leak was
established by task review.

## Current authority, owners and tests

- [Wyrd design](../../../architecture/wyrd-design.md), [Bifrost design](../../../architecture/bifrost-design.md) and [security posture](../../../architecture/wyrd-security-posture.md).
- [Public error catalog](../../../crates/wyrd-spec/src/vala/error.rs), [Oracle planning/errors](../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs), [live execution](../../../crates/vala/vala-bifrost-redux/src/oracle/live.rs), [codec fence](../../../crates/vala/vala-bifrost-redux/src/oracle/codec.rs) and [session configuration](../../../crates/vala/vala-bifrost-redux/src/resources.rs).
- [Remote scalar/join journeys](../../../crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/live_subquery.rs), [Rust query journeys](../../../sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs), [common-query corpus](../../../sdks/wyrd-sdk-rust/tests/integration/query_coverage.rs), [Python queries](../../../sdks/wyrd-sdk-python/tests/integration/test_query_bifrost.py), [TypeScript queries](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts) and [MCP queries](../../../crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs).
