# Independent behavior review

## Subject

- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`
- Candidate: `7d6a1ff95b3dc09315e3af236ab9906b305e11d0`
- Spec: `changes/active/bifrost-errors/spec.md` revision 1.
- Original task: absent; specification and owner-directed common-query and performance coverage govern.
- Remediation: `review/task-review-1/bifrost-errors-R1-materialize-live-subquery-predicates.md`.
- DataFusion: `5697cfa35192f175bbcd54f59a5fd88527c2bbf5`; distributed fork `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

I inspected the cumulative diff and current source, using CodeGraph before locating Wyrd symbols. Prior findings were hypotheses, not acceptance evidence. Applicable authority is AGENTS.md, agent-rules.md, Wyrd design/doctrine, Bifrost design, reference router, spec-driven development, maintainer style, DataFusion/OLAP, errors and testing references. No candidate source or other report was edited.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 actionable SQL planning 4xx, suggestions retained across joins/subqueries | `oracle/mod.rs:3018-3033,3735-3797`: logical SQL/optimization errors use narrow wrapper traversal and planner detail; physical and execution failures stay redacted | Mapper unit, Rust query_bifrost, corresponding Python/TS cases, MCP query journey | PASS |
| Internal failures do not disclose internals | Context/diagnostic/shared/collection wrappers are allowed but External/Execution/Internal leaves are not; resource errors take precedence; general execution mapper remains generic | Mapper covers Internal/Execution and resource refusal; physical mapper fixed repair detail | PASS |
| Authorization precedes diagnostics, including subquery tables | `build_physical_root` receives authorized pinned cuts; providers register only admitted tables; SQL lowering receives that session | Unauthorized-table journeys in all SDKs and MCP; prior tenancy closure remains source-supported | PASS |
| REQ-006 scalar, IN, EXISTS and managed run_id correlation over live rows match joins | Existing server execution plus runtime snapshot at Scribe boundary; source fixture stamps distinct/shared typed RunIds | Focused Rust/Python/TS/MCP comparisons assert exact expected rows as well as equivalent joins | PASS |
| R1 AC1 ready/pending/metadata/null/recursive snapshot and reset | Fork `physical-expr/src/scalar_subquery.rs:170-186`; tests at 364,392,411,422; snapshot copies populated value and metadata without altering reusable expression | Recorded exact fork test command: 10 scalar tests; live scalar unit clears and repopulates results | PASS |
| R1 AC2 restrictive scalar reaches remote live Scribe on analytical-configured path, bounded output | `resources.rs:4122` permits equijoin extraction on second optimizer pass; existing hash join publishes build bound; `live.rs:447-476,298-317,673-680` retains runtime filter and snapshots at first poll | Remote journey asserts exactly 2 fragments and 2 total footer rows from 200 event rows plus one scalar row; fragment contains static and resolved instant | PASS |
| R1 AC3 independent planning pruning and no route duplication/omission | `live.rs:463-474` preserves independently snapshotable static conjunct; `resolved` deals each planned index to exactly one partition then only removes routes | Live unit asserts H0 excluded before scalar ready and union over partitions equals H2/H3 then H3 after value reset; remote test excludes extra fragments | PASS |
| R1 AC4 retain focused public-surface behavior | No new public wire or SDK transport semantics; same shared client facade and errors | Supplied Oracle lane 57/57 includes MCP; exact Rust nine query journeys; earlier language proofs remain relevant but no new independent rerun here | PASS |
| R1 AC5 delete workload machinery and dependency promotion | No support_workload or analytical_correctness files/registrations; wyrd-testing Cargo.toml datafusion at dev-dependencies:137 | Source/manifest inspection; check:deps recorded | PASS |
| R1 AC6 compact common query corpus, mixed published/live, CTE outer join | `query_coverage.rs`: six orders and four customers split by flush; fixed expected rows; projection/filter/order/limit, aggregates/HAVING, inner/left join, scalars including empty/null, correlated scalar, IN/NOT IN nulls, EXISTS/NOT EXISTS, CTE joining a second table, rank | Exact common_queries_answer_over_published_and_live_rows command recorded PASS | PASS |
| R1 AC7 representative remote join | `peer_network/live_subquery.rs`: two live remote tables and explicit equijoin; public HTTP client query | Exact remote_live_join_returns_its_rows command recorded PASS | PASS |
| Format/lints/codegen/dependency/tenant checks | No generated contract changes; dependency revision and codec fence agree | Recorded commands exit 0; independent cumulative diff-check finds one blank EOF in old verdict, no behavior consequence | PASS |
| Non-goals: no Wyrd evaluator/planner/public framework/dependency addition; no benchmark load | Native expression snapshot and native hash-join runtime filter reused; Wyrd only serializes its private live fragment | Actual diff and native producer source | PASS |

The spec contains split-change references to Jev/traceparent and unrelated REQ numbers. They are not Bifrost obligations for this task and were not silently expanded into its review scope.

## Caller and producer trace

Logical lowering uses the authorized session before DataFusion produces diagnostic suggestions. The narrow mapping intentionally excludes dependency External error roots; broad find_root reuse would be unsafe at this disclosure boundary. Unknown table errors stay the existing actionable catalog 404 before provider registration, as previously validated.

Interactive scalar execution fills ScalarSubqueryResults before opening the main input. Its scalar expression now snapshots via DataFusion's standard operation to a built-in Literal. Analytical planning preserves the distributed planner's scalar-disable invariant; no global flag reversal ships process-local scalar state. With two optimizer passes the equality produced by scalar-to-join rewriting can become an equijoin key. HashJoinExec Post-phase pushdown constructs a runtime filter on its probe child, and its consumer search recognizes LiveScribeExec through apply_expressions (`live.rs:413-420`). Reporting unsupported retains authoritative upstream filtering/join behavior while still making the leaf a consumer.

Pinned DataFusion hash_join/exec.rs:1960-2017 registers the producer only when a consumer holds the expression; stream.rs:738-779 finishes build before polling probe. First-poll snapshotting therefore uses the completed scalar bound for this tested shape. Each partition obtains a stable static predicate before fragment construction. A conservative snapshot from progressively narrowing filters such as TopK can lose later pruning opportunities but cannot discard rows needed by the authoritative leader operator. Large hash-table lookup expressions retain their dependency's safe wire fallback, rather than introducing a Wyrd evaluator.

Route partitioning intentionally differs from the private task's prescribed order: assign planned indices first, then narrow. Because separate output streams may snapshot at different moments, this preserves disjoint ownership regardless of those snapshots and prevents duplicate/omitted ownership. The task's intended behavior is satisfied; mechanically pruning independently before dealing could violate it. Source and the partition-union assertion support this choice.

The corpus is test-only and ordinary fixed expected outputs replace both the public support model and the second DataFusion oracle. It covers the owner's exact nonrecursive CTE-to-second-table shape and includes null-sensitive edge cases.

## Prior finding closure

- `FIND-bifrost-errors-1`: CLOSED for the bounded requested selective scalar scenario on both stock scalar and analytical-configured hash-join paths. The fragment predicate is retained, resolved, serialized, applied on Scribe and observed through exact output counters. No concrete scalar exclusion remains.
- `FIND-bifrost-errors-2`: CLOSED. General generated framework/public API and normal DataFusion dependency promotion are absent, with intentionally compact common-query coverage replacing their useful goal.

## Limits and non-blocking observations

No tests or benchmarks were executed in this review. Available verification is the committed evidence table, reconciled against exact source test names and actual assertions; it does not provide independent execution logs for every command. A passing deterministic footer assertion proves the selected transfer/pruning property, not a measured planning-latency claim or a promise that all scalar inequalities optimize equally.

Analytical scalar-to-join rewriting's pre-existing multi-row cardinality limitation is not repaired by this candidate. It must not be described as full SQL scalar-cardinality qualification; the added empty and typed-null cases do not prove multi-row refusal. This is a dependency limitation already present under the unchanged distributed scalar-disable flag, not a new bounded finding against the tested scalar scenarios.

DataFusion's reset_plan_states documentation explicitly excludes dynamic-filter plan reuse (`physical-plan/src/execution_plan.rs:1997-2001`); its HashJoin builder reset drops producer state while children can retain old filters. Wyrd builds a fresh physical plan per ordinary request, and no required public request path was established that resets this reused dynamic join. Therefore I do not turn an unsupported dependency reuse case into remediation scope. The direct scalar reset assertion proves new scalar snapshots, not general dynamic-join reset compatibility.

The recorded earlier MCP memory-reservation failure has no established diagnosis and a later complete lane passes. No assertion/timeout/skip was weakened in the diff to hide it. Treat it as an unresolved verification risk, not proof that this predicate change caused a leak.

## Proposed findings and result

Proposed finding ledger: empty. Overall result: **PASS**, subject to independent validation and the orchestrator's verification reconciliation.
