# DataFusion domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8`
- Candidate: `f1e1be815069b23fa9920a61c1198524ecaad514`
- Approved specification: `changes/active/bifrost-errors/spec.md` revision 1
- DataFusion fork: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`
- `datafusion-distributed` fork: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`

The candidate remained at the stated commit while this review was performed.

## Reviewed boundary and authority

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Live-table filter planning and fragment construction | `architecture/bifrost-design.md` Query/Oracle and Live-tail sections; `oracle/live.rs`; `oracle/exec.rs` | FAIL: scalar-subquery predicates are deliberately removed from Scribe fragments even though the architecture says the fragment enforces its predicate. |
| DataFusion scalar-subquery execution state | DataFusion `physical-expr/src/scalar_subquery.rs`; `physical-plan/src/scalar_subquery.rs`; proto decode context and scalar-subquery round-trip tests | The expression reads runtime state populated once by `ScalarSubqueryExec`; a bare serialized expression cannot be decoded because it has lost that owner. |
| Runtime physical-expression transport contract | DataFusion `physical-expr-common/src/physical_expr.rs` (`PhysicalExpr::snapshot` and `snapshot_physical_expr`) | The existing generic contract explicitly covers simplifying stateful expressions before remote serialization. `ScalarSubqueryExpr` does not implement it. |
| Distributed stage splitting | `datafusion-distributed/src/metrics/task_metrics_collector.rs`; distributed planner/codec source | The fork already records this same owner/expression split as an ignored end-to-end test. The stage planner can put a boundary between `ScalarSubqueryExec` and its dependent filter. |
| Scalar, `IN`, `EXISTS`, and correlated SQL shapes | Wyrd Rust/HTTP, Python, TypeScript, MCP journeys; the seeded analytical workload and remote-Scribe journey | Result-equivalence coverage is broad. `IN` and `EXISTS` forms are decorrelated to join forms when DataFusion supports the shape; the candidate-specific workaround applies only to uncorrelated physical `ScalarSubqueryExpr`. |
| Performance proof | Live-leaf unit tests, SDK journeys, analytical correctness journeys, remote-Scribe journey | FAIL: tests prove rows and that some fragments ran, but never prove the scalar predicate reached Scribe or bound remote rows/bytes and leader work. |

Applicable authority requires Oracle/Scribe to execute the one DataFusion plan and states that Scribe fragments enforce projection and predicate while streaming bounded Arrow results. The human review instruction additionally requires subquery support without a performance regression or a support-specific workaround.

## Execution-path assessment

For an uncorrelated scalar predicate, DataFusion builds a `ScalarSubqueryExec` around the main plan. It executes the subquery once and writes the result into a shared `ScalarSubqueryResults`; the main filter's `ScalarSubqueryExpr` reads the value by index. The ordinary physical filter-pushdown pass splits conjunctions before they reach `LiveScribeExec`, so independent static conjuncts still push to Scribe.

The candidate detects a `ScalarSubqueryExpr` anywhere in each offered conjunct and omits that conjunct from `LiveScribeExec.predicate` (`oracle/live.rs:401-421`, `509-523`). The leader retains its `FilterExec`, so results are correct. At execution the Scribe therefore scans and sends every live row surviving the other conjuncts; the leader applies the scalar predicate after transport. Cost grows with live rows in network bytes, Arrow decode, and leader filter work. The change is safe for correctness but is not performance-neutral source pushdown.

The failure is not that scalar subqueries are intrinsically unshippable. The value is unavailable during physical optimization, but it is available when `LiveScribeExec` opens its fragment: `ScalarSubqueryExec` waits for all scalar subqueries before executing the main input. DataFusion's `PhysicalExpr::snapshot` contract explicitly tells remote executors to snapshot stateful expressions before serialization (`physical-expr-common/src/physical_expr.rs:405-432`). `ScalarSubqueryExpr` has the needed result handle and already errors when evaluated before population (`physical-expr/src/scalar_subquery.rs:134-146`), but it supplies no snapshot that replaces a ready result with a serializable literal. That missing behavior belongs in the DataFusion fork. Wyrd owns the runtime fragment boundary and should invoke the generic snapshot immediately before fragment encoding.

The `datafusion-distributed` fork has a related planning-time issue: its ignored `test_metrics_collection_e2e_3` states that a stage boundary may separate `ScalarSubqueryExpr` from `ScalarSubqueryExec`, producing a fragment that cannot decode (`task_metrics_collector.rs:221-228`). Runtime literal snapshotting fixes Wyrd's execution-time live fragment, but it cannot by itself fix a distributed fragment encoded before the scalar subquery executes. The distributed planner must keep this runtime-state owner and its dependent expression in one stage, or explicitly model the scalar result as a predecessor-stage input. That fix belongs in `datafusion-distributed`, not in Wyrd operator allowlists.

`IN`, `EXISTS`, and correlated predicates are a different physical case. DataFusion rewrites supported forms to semi/anti/mark joins or correlated join plans; no scalar-expression transport exception should be added for them. The journeys cover representative `IN`, correlated `EXISTS`, correlated scalar aggregate, null, join, live, mixed, and remote-live cases. They do not establish support for a correlated shape DataFusion itself cannot decorrelate, nor does the specification require Wyrd to implement a second SQL planner.

## Acceptance matrix

| Requirement or constraint | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scalar live-table subqueries return join-equivalent rows | Leader retains the original `FilterExec`; only its scalar conjunct is withheld from the live fragment. | Rust/Python/TypeScript/MCP journeys plus analytical workload cases compare results with joins or plain DataFusion. | PASS for represented result semantics. |
| `IN` and `EXISTS`, including `run_id` correlation, return join-equivalent rows | DataFusion decorrelation and normal join execution remain unchanged. | `query_bifrost.rs:246-316` and sibling language/MCP journeys cover `IN`, correlated `EXISTS`, and managed `run_id`. | PASS for represented shapes. |
| Tenant isolation remains unchanged | Fragment binding, assignment, and authorization paths are unchanged; the candidate changes only which physical filter is serialized. | Existing journeys use public authenticated clients; no new tenant authority path exists. | PASS. |
| Scribe fragment enforces the query predicate without avoidable transport/leader regression | `oracle/live.rs:405-406` deletes scalar-subquery conjuncts from the fragment predicate. | No test asserts decoded literal pushdown, remote input/output rows, scanned bytes, or leader filtering bounds. | FAIL (`DF-001`). |
| Fix resides at the owner of DataFusion runtime state and remains generic | Wyrd downcasts to the concrete `ScalarSubqueryExpr`; DataFusion's existing snapshot contract is unused and unimplemented by this expression. | The dependency's proto decoder requires a surrounding `ScalarSubqueryExec`; the distributed fork has the same known ignored failure. | FAIL (`DF-001`). |
| Performance is credibly bounded | Independent conjunctions still push because DataFusion splits conjunctions, limiting the regression to scalar-dependent conjuncts. | Fixtures contain only a few or hundreds of rows and assert answers/fragment count, not selectivity or bytes. | FAIL (`DF-001`). |

## Material finding

### DF-001 — INCORRECT: scalar support bypasses remote predicate pushdown instead of snapshotting the ready scalar value

- **Violated obligation:** Live Scribe fragments must enforce the query predicate, and the requested scalar-subquery support must not trade the decode failure for row-volume-dependent network and leader work. The implementation should use the owning engine's generic runtime-expression mechanism rather than a Wyrd-specific concrete-type exclusion.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/live.rs:401-421` and `509-523`; DataFusion fork `datafusion/physical-expr/src/scalar_subquery.rs:134-155`; `datafusion/physical-expr-common/src/physical_expr.rs:405-432`.
- **Evidence:** `reads_scalar_subquery` downcasts to one concrete DataFusion expression and removes that entire conjunct before `LiveScribeExec.predicate` is built. `fragment()` can only serialize predicates retained in that field. DataFusion's proto decoder expressly rejects a scalar expression outside its `ScalarSubqueryExec` scope (`datafusion/proto/src/physical_plan/from_proto.rs:354-361`), while the generic `snapshot` contract exists specifically to convert stateful expressions for remote serialization. `ScalarSubqueryExpr` implements evaluation but no snapshot. The distributed fork independently documents the same split-state failure in an ignored end-to-end test.
- **Observable consequence:** A selective query such as `WHERE id = (SELECT max(id) ...)` transfers all live rows from each selected Scribe route and filters them on the leader. At large live-tail volume this increases peer bytes, Arrow decoding, leader CPU, and time-to-first/last result in proportion to source rows. Current small result-equivalence tests cannot detect that regression. More generally, a distributed stage boundary can still serialize a scalar-dependent filter without its result owner.
- **Required testable correction:**
  1. In the DataFusion fork, make `ScalarSubqueryExpr` participate in the existing stateful-expression snapshot contract by yielding a normal literal expression after its result slot is populated and returning a typed error while it is pending. Preserve Arrow type/metadata semantics needed by the enclosing expression. Add focused pending/ready and recursive-expression snapshot tests.
  2. In Wyrd's live-fragment owner, retain scalar-dependent static conjuncts and call DataFusion's recursive physical-expression snapshot immediately before encoding the fragment, after `ScalarSubqueryExec` has populated the result. Keep the leader `FilterExec` as the correctness check, as it is today. Do not add another expression allowlist or a Wyrd-owned scalar evaluator.
  3. In `datafusion-distributed`, prevent a planning-time stage boundary from separating `ScalarSubqueryExec` from expressions that read its result, or model the scalar result as an explicit predecessor-stage dependency. Re-enable its existing ignored scalar-subquery end-to-end test. This is distinct from Wyrd's runtime fragment snapshot because distributed stages are encoded before execution.
  4. Prove the Wyrd path with a focused remote-live test whose scalar predicate is highly selective: decode/inspect the dispatched filter to show it contains the materialized literal and no `ScalarSubqueryExpr`, and assert Scribe output rows or transport bytes are bounded by the selected rows. Also retain one independent conjunct to prove ordinary pushdown remains intact. The existing cross-surface result journeys remain the semantic proof.

## Verification limits

No verification results were supplied. This domain review did not execute the Postgres-backed or multi-pod ignored journeys. It inspected the complete candidate diff, production call path, dependency revisions, dependency tests, and the candidate's test assertions. The absence of executed journey evidence is a general acceptance limit; independently, the tests as written do not measure the performance property in DF-001.

## Overall result

**FAIL**

The candidate fixes returned rows for the exercised live-table subqueries, and ordinary independent conjuncts continue to push down. It does so by suppressing the scalar-dependent source predicate, leaving a material and unbounded remote-scan regression. The smallest complete ownership-aligned fix uses DataFusion's existing runtime-expression snapshot contract at Wyrd's fragment boundary and fixes the separate stage-scope invariant in `datafusion-distributed`.
