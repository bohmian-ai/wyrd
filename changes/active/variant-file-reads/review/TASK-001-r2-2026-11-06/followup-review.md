# Focused follow-up: FIND-TASK-001-2

**RESOLVED.** The output-snapshot correction is correct at its existing owner. The prior finding's public SQL regression is not established: its concrete SELECT-list example is rewritten to a join by the configured server planner. P2 proves the internal serialization invariant; P4 supplies public correctness coverage, not fix-dependent RED/GREEN proof. No material finding or further correction is proposed.

## Subject and authority

Reviewed `9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994` to `96fea41e147c799354f797469260391d2fc237ff`, original base `5234961d51798581a026834cfffd5efaadc226b4`, original task, approved spec revision 2, prior ledger and remediation diagnosis. HEAD equals the candidate. Scope is solely this scalar-output uncertainty; no other current reviewer conclusions were inspected.

Applied repository instructions, task-review skill, `changes/spec-driven-development.md` materiality/no-op-proof rules, maintainer style, spec REQ-001/002 and no-new-planner constraint, and Bifrost authority's Variant SQL/nested-field reads. No `.codegraph/` or nested AGENTS.md exists under the inspected code trees.

## Source trace

1. `oracle/mod.rs:2863-2952` builds the local planning session from `OracleSessionTemplate`, selects the analytical planning session when a handle exists, then invokes that state's `create_physical_plan(&logical)`. `oracle/session.rs:25,67` installs default features but does not fix scalar execution strategy.
2. `oracle/analytical.rs:6750-6785::planning_session` clones local state/config, installs `LiveUnionBoundary`, then calls `with_distributed_planner()` before building state. Pinned `datafusion-distributed` revision `bbbd1d9` (`Cargo.toml:265`), `src/distributed_planner/session_state_builder_ext.rs:24-29`, explicitly sets `optimizer.enable_physical_uncorrelated_scalar_subquery = false`. This decisive configuration was absent from the prior finding's trace.
3. Pinned DataFusion `5f4b171` (`Cargo.toml:277`), `core/src/execution/session_state.rs:828-835`, optimizes the logical plan before invoking the query planner. `optimizer/src/optimizer.rs:299` installs `ScalarSubqueryToJoin`. Its `scalar_subquery_to_join.rs:161-239` rewrites Projection scalars, including nested computed expressions, to join columns when that option is false (`:266-279`). `build_join` (`:358-429`) uses a Left join and wraps uncertain uncorrelated row counts with registered `single_value`; a provably one-row aggregate needs no wrapper. This explains the recorded max/single_value join observations.
4. Conversely, `core/src/physical_planner.rs:355-402` only collects runtime scalars and installs their `ScalarSubqueryExec` coordinator when the option is true. The prior finding traced this branch without establishing that production configuration selects it.
5. `oracle/live.rs:809-900::LiveUnionBoundary::optimize` moves a Projection over a live-holding Union onto its inputs and applies installed ProjectionPushdown to the live side. `LiveScribeExec::try_swapping_with_projection` (`:595`) absorbs its expressions. This can absorb an explicitly supplied `ScalarSubqueryExpr`; it does not turn the join column from step 3 back into such an expression.
6. `LiveFragmentRead::into_stream` (`live.rs:940-960`) calls `resolved` before fragment construction/open. `resolved` (`:448-474`) now clones the leaf and recursively snapshots all absorbed outputs before the no-predicate return, then snapshots/prunes the predicate. `fragment` (`:492-523`) encodes those requirements. The original plan retains its runtime expressions for another execution; pending state errors before dispatch.
7. Installed `physical-expr-common/src/physical_expr.rs:956-989::snapshot_physical_expr` recurses; `physical-expr/src/scalar_subquery.rs:174-183` replaces a populated scalar with a literal preserving field metadata and refuses pending state. `physical-expr/src/projection.rs:300-314::try_map_exprs` preserves names/order. The internal mismatch behind the prior concern is real: scalar serialization (`scalar_subquery.rs:186-212`) omits runtime results; standalone parsing (`proto/src/physical_plan/from_proto.rs:354-362`) requires a surrounding scalar coordinator that follower `signed_requirements` does not provide.

## Claim and proof disposition

| Claim | Disposition |
|---|---|
| Previously absorbed runtime scalar outputs could ship indices instead of values | Supported for this internal input: old `resolved` snapshotted only predicates, and the standalone follower lacks runtime results. |
| The prior nonempty SELECT-list max-subquery example necessarily reaches that state on the actual server | Unsupported. The configured analytical planner disables physical scalars and logical optimization replaces them with joins first. A default SessionState experiment does not establish production reachability. |
| P2 directly proves owner-level closure | Supported. `live.rs:1621-1708::absorbed_scalar_outputs_snapshot_per_execution` builds a scalar nested in addition, absorbs it without a predicate, requires pending refusal, reuses the same plan with 41 and 97, and checks encoded names/current literals/absence of scalar nodes. Recorded RED fails pending refusal without the correction; source explains that failure. This does not prove automatic server planning. |
| P4 proves a public failure caused by omitted snapshotting | Unsupported. `distributed.rs:1226-1348` verifies exact bare/nested/Variant-adjacent answers after two writes, both topologies, and fragment dispatch. It neither requires the runtime-scalar shape nor inspects its outputs. Its passing without the correction follows the configured join rewrite. No assertion was weakened to fabricate RED. |

This does not establish that every SQL shape decorrelates, or that no other Oracle configuration could build runtime scalars. A failed rewrite or a session without the analytical planner alone does not prove a supported successful public query reaches this state. No such concrete failing query is source-proven in this scoped record.

## Closure and correction burden

Narrow FIND-TASK-001-2: the internal snapshot invariant is closed; its prior public-regression rationale must not carry forward as proven. The five-line correction reuses the existing recursive mechanism at the sole execution-time output-shipping owner, preserves aliases/schema and re-execution, and introduces no planner, codec or scalar context. It completes the newly absorbed output path with bounded maintenance cost and is task-bounded. No fake public RED or new SQL/planner capability is required to rescue the earlier claim.

Skipped: builds, runtime reruns, broad rediscovery and other reviewer conclusions. Recorded P2/P4 results were source-checked; P4 cannot establish fix-dependent production reachability.
