# Behavior review — round 4

## Subject and result

**Overall: FAIL.** Resumed by explicit owner direction after the unrelated main merge. Current Wyrd candidate `17697dc8c2740213565e5f913cde1edfb0cb8ce5`; integration baseline `945a1548c47897dc94583169a6ba04163a73a749`; original task base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`. Cumulative task diff integration-baseline→candidate retains the full Bifrost change. Fork base `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`, candidate `b2d37bc182b9d3e576650a414b364f3d9b5aefe3` unchanged. Proposed BEH-1 retains the open FIND-3 root invariant; review is no longer blocked merely by the authorized merge.

### Merge seam revalidation

Read actual pre-merge→candidate diffs at shared AppState, WyrdTestServer, security posture, lock and mise. State/test-server changes consolidate gateway secret ownership and add provider configuration; they do not replace Oracle/Scribe owners, query credentials, tenant resolution or footer counters. The security-authority delta describes gateway managed-secret sealing only. Cargo.lock adds test-helper rand while the Bifrost fork revisions remain unchanged. Query corpus remains included in the observed mise journey lane. No merge delta changed vala-bifrost-redux, the Bifrost query fixture/writer, peer Oracle journeys, scalar/refusal SDK query tests or common-query corpus. Re-read current Filter::is_scalar and verified cached fork HEAD exactly b2d37bc. Revalidated current Wyrd HEAD as17697dc8. Existing source evidence and obligation matrix remain applicable; no new current-tree runtime results are claimed.
## Scope and source coverage

Read specification, R1/R2/R3 acceptance and evidence, repository rules and reference router, spec-driven-development and maintainer style, DataFusion/OLAP/error references and relevant Wyrd architecture anchors. CodeGraph was used before locating Wyrd production symbols. Reviewed cumulative Wyrd diff and fork diff, with current R3 delta locating proof repair. Traced Oracle logical-error mapping and physical-error mapping, analytical planning session and flag configuration, shared session passes, LiveScribeExec Pre/Post pushdown/resolved/execute, LiveFragmentRead first-poll fragment opening, test-support footer and encoded-plan recording, SDK fixture/run correlation, Rust/Python/TS/MCP additions, compact common-query corpus and remote scalar/join/refusal journeys. In fork, read entire max_rows closed match, Filter::is_scalar, unique-key creation/propagation in aggregate functional dependencies, both shared scalar rewrite call sites and build_join, correlated scalar invariant validator, ScalarSubqueryExpr snapshot, entire single_value aggregate/state/merge, SQL refusal cases and bound tests. Public tests were assessed from actual assertions, not completion summary. No benchmarks, builds or heavy reruns.

## Acceptance matrix

`PASS` below describes source and assertion evidence on the resumed immutable subject; the overall task fails the retained cardinality invariant.

| Obligation | Implementation evidence | Verification evidence assessed | Result |
|---|---|---|---|
| REQ-002 caller SQL planner message/suggestion through typed 4xx | oracle/mod.rs::plan_logical uses map_sql_planning_error; caller_planning_fault unwraps only selected typed wrappers; physical mapper remains redacted | Exact mapper test asserts wrappers, unknown function/schema, internal/resource cases; SDK/MCP journeys assert suggestion/type/ambiguity | PASS |
| REQ-002 internal errors redacted; unauthorized schema cannot supply suggestion | Logical planning only registers authorized providers; untouched preplanning authorization; physical mapper fixed action and execution mapper opaque | All first-class journey additions assert unauthorized-table forbidden without column spelling | PASS |
| REQ-006 scalar/IN/EXISTS/correlated run_id return proper live rows | Native DataFusion paths retained, scalar snapshot added; fixtures stamp actual managed RunIds | Rust/Python/TS/MCP subquery-vs-join assertions with explicit expected IDs | PASS except scalar cardinality below |
| INV-001 tenant boundaries unchanged | Fragment identity and binding remain existing owners; no new tenant filter/SQL authority | Existing redaction coverage retained; native fragment encoding remains fenced | PASS |
| AC-001 journey coverage for shipped surfaces | SDKs call public client/server, MCP tool path retained | Journey sources and committed evidence selectors/counts align; no result-only unit substituted | PASS |
| AC-002 relevant negative unauthorized-table behavior | Existing permission owner precedes SQL suggestions | Per-surface forbidden/redaction tests | PASS; unrelated inherited Jev/traceparent clauses outside split scope |
| AC-003 focused gates | Pins/lock/revision updated together; no feature or protocol addition | Committed R3 exact selectors/counts and scoped format/lint/codegen/deps/tenant results; cumulative diff-check executed read-only for assigned candidate and clean | PASS subject to reported evidence; merged runtime lanes not rerun by this reviewer |
| R1-1 snapshot semantics ready/pending/metadata/null/reset | ScalarSubqueryExpr::snapshot returns literal or error, leaves reusable producer intact | Fork snapshot tests and Wyrd two-execution scalar unit | PASS |
| R1-2/3 bounded remote output and static pruning/partition ownership | Live leaf accepts Post dynamic filter, snapshots first-poll, deals immutable planned list before narrowing | Remote test checks two fragments and two footer rows, static and resolved literal; unit verifies disjoint route ownership/reset | PASS |
| R1-4 retained surfaces | Native scalar/IN/EXISTS and typed errors remain | Per-surface source assertions | PASS |
| R1-5/6 workload removed, compact common-query coverage | No support_workload module or production DataFusion testing dependency; fixed corpus explicit expected rows including joined CTE, windows, memberships | query_coverage corpus and mise inclusion | PASS |
| R1-7 representative remote join | Existing remote live leaf and native hash join | remote_live_join_returns_its_rows exact rows | PASS |
| R1-8 focused checks | No new public schema | Evidence records checks | PASS within resumed candidate source |
| R2-1/2 multi-row scalar Filter/Projection globally validate including duplicates | Shared build_join uses single_value for unknown/>1 bound; partial state counts rows; final merge checks total | SQL negative tests and published/live duplicate journey; aggregate merge tests; codec roundtrip | FAIL: Filter singleton proof can still wrongly bypass aggregate |
| R2-3 native singleton/empty/typed NULL | single_value initialized with typed NULL and rows0; no-row update ignored; singleton preserved | Aggregate and SQL positive/empty/null controls; corpus | PASS |
| R2-4 negative journey/native global codec | Existing AggregateUDF path and coalesce/final network path reused | Peer negative assertions and proto test actually refuse | PASS for covered shapes |
| R2-5 no regression to R1/no flag reversal/new protocol | Distributed flag unchanged; hash build readiness used | Remote literal/footer proof retained | PASS |
| R2-6 FIND-4 rustdoc/cumulative whitespace | Changed Wyrd tests have Panics sections | Diff/source documentation and clean candidate diff-check | PASS |
| R3-1 full join/grouping conservative; all singleton bypass proofs sound | Full join/nonempty and grouping-set cases now None; however Filter::is_scalar remains unsound | New bound test proves specified cases; does not cover row-dependent unique-key comparison | FAIL overall sound-proof invariant |
| R3-2 empty/overflow conservative | Correct empty-side outer join branches; checked_mul/checked_add | Bound test includes empty and overflow; other closed branches inspected | PASS except Filter proof |
| R3-3 native Filter/Projection refuse full/grouping cases | Native build_join now receives None and inserts validator | Four SQL negative cases | PASS |
| R3-4 authorized analytical peer refusal | Existing outer events table/public credential; added two expanding-input projections | Journey asserts opaque execution refusal rather than rows | PASS |
| R3-5 preserved controls/no scope extension | Aggregate/codec unchanged in latest delta, singleton MAX/LIMIT bound valid | Existing focused regression evidence and source assertions | PASS except retained same-cause Filter bypass |

## Proposed finding BEH-1 — retain FIND-bifrost-errors-3

- Classification: **INCORRECT**.
- Violated obligation: R3 sound singleton proof owner/complete sibling audit; R2 evaluated uncorrelated scalar must refuse multiple rows in Filter and Projection; REQ-006 correctness.
- Exact producer: pinned fork `datafusion/expr/src/logical_plan/plan.rs:2909-2964`, especially `Filter::is_scalar` at 2943-2952, consumed by `LogicalPlan::max_rows` at 1453-1458.
- Shared consumers: `datafusion/optimizer/src/scalar_subquery_to_join.rs:383-405` through both Filter and Projection call sites (124/194); correlated validator `datafusion/expr/src/logical_plan/invariants.rs:188` is another consumer of the same bound. Projection/alias preserve false bound.
- Source evidence: is_scalar collects the unique-key column for any equality whose other operand is not a bare Column. A BinaryExpr containing another input column qualifies. It then returns true if the collected key covers a non-null Single dependency. Ordinary GROUP BY creates exactly such a dependency (`datafusion/common/src/functional_dependencies.rs:481-507`), so no DDL/primary-key metadata is necessary to reach this state.
- Concrete input: two grouped rows `(x, y) = (1,0),(2,1)` satisfy `x = y + 1`. The group key x is unique but the equality constrains it to a different value in each row, so there are still two rows. Filter reports Some(1); scalar rewrite omits single_value; native join may emit two outer results rather than refusing.

```sql
SELECT id,
       (SELECT x
        FROM (SELECT x, MAX(y) AS y
              FROM (VALUES (1,0),(2,1)) t(x,y)
              GROUP BY x) s
        WHERE x = y + 1) AS picked
FROM vala.bifrost.<authorized_events>;
```

This query has a real authorized outer table and uses only standard relational constructs already accepted by Wyrd. An equivalent filter-position scalar shares build_join. It is the previously open false-singleton-proof cause, not a new independent finding and not unrelated bound debt: R3 explicitly audited Filter::is_scalar and required every remaining proof to be sound. The implementation evidence claims that unique-key information makes this branch sound, but uniqueness alone does not imply a unique-key equality against a row-dependent expression selects at most one row.

- Observable consequence: a successful wrong scalar answer remains reachable on flag-off analytical configuration. Runtime count is correct when present; it is skipped on a false static proof.
- Required smallest correction: repair the existing Filter singleton proof at its producer. Treat a unique-key equality as fixing that key only when the comparison value is provably constant for the complete input; otherwise preserve input bound/unknown and let existing single_value validate. Prefer conservative literal equality, including ordinary native constant folding, over inventing a second expression classifier. Do not accept merely “not a bare column” or merely “contains no input Column” as proof, because volatile/per-row expressions are not fixed values either. Keep genuine key-to-literal proofs, global MAX and LIMIT1 optimizations; retain existing aggregate/global codec/live pushdown owners.
- Focused proof: direct Filter/max_rows regression over grouped non-null key and row-dependent arithmetic; a literal-equality singleton control; flag-off SQL Filter/Projection second-row refusals for the concrete input, with a native physical-scalar control; extend existing analytical public negative journey only if necessary to prove boundary. No new harness or benchmark.
- Reproduction status: source-established proposal sent to orchestration for independent focused execution/validation; this reviewer did not execute it or claim an observed runtime result.

## Prior finding closure and limits

FIND-1 materialized equality pushdown and FIND-2 compact coverage have direct candidate source and assertion evidence. FIND-4 required Wyrd test rustdoc is present. FIND-3 closes the specifically reported FULL JOIN/GROUPING cases, but the same bound-to-validator-bypass invariant remains unsatisfied at Filter. Exact registered SQL test runs and all scoped lane results in R3 evidence are credible for their assertions, not evidence that every bound constructor is sound. Historical unexplained memory-reservation lane failure stays a verification limit, not a newly inferred defect. No heavy tests or benchmarks run by this reviewer. Owner direction authorizes the resumed merged subject; source and relevant seams were revalidated. The task remains FAIL on proposed BEH-1, pending independent validation; no blocker is asserted solely for the main merge.
