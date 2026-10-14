# Immutable review subject, round 3

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`.
- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Candidate: `dc353ef5ecf39da13f3dcac5b9b5a31558b6a35e`.
- Previous candidate: `7d6a1ff95b3dc09315e3af236ab9906b305e11d0`.
- Spec: `changes/active/bifrost-errors/spec.md`, approved revision 1.
- Original task: absent; spec and owner directions in the conversation govern.
- Prior verdicts/ledgers: `review/task-review-1` and `review/task-review-2` under this change.
- Remediation: `review/task-review-2/bifrost-errors-R2-enforce-analytical-scalar-cardinality.md` (includes committed verification evidence).
- First remediation: `review/task-review-1/bifrost-errors-R1-materialize-live-subquery-predicates.md`.
- DataFusion candidate: `f059acf6c58b2c5fd4bc36b3358e8a56ac283476`.
- DataFusion original: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`; previous `5697cfa35192f175bbcd54f59a5fd88527c2bbf5`.
- Fork source: `/home/thorrester/.cargo/git/checkouts/datafusion-44e2ebb5250b7f6f/f059acf`.
- Distributed fork unchanged: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`, `/home/thorrester/.cargo/git/checkouts/datafusion-distributed-615cde49e12a7772/8fac2a9`.

## Shared starting map (expand independently)

- Fork `functions-aggregate/src/single_value.rs`: AggregateUDFImpl, accumulator update/merge/state/evaluate, return metadata/type and fixed tests; registration in `lib.rs`.
- Fork `optimizer/src/scalar_subquery_to_join.rs`: rewrite/build_join shared Filter/Projection producer, max_rows shortcut, function-registry fallback, correlated siblings.
- Fork scalar execution original semantics in `physical-plan/src/scalar_subquery.rs`; partial/final aggregation, coalescing/distribution, native AggregateUDF codec/registry and proto roundtrip.
- Fork subquery SQL tests/config/docs and information-schema changes.
- Wyrd `oracle/analytical.rs`: planning_session and distributed flag; `resources.rs`: shared two-pass session; `oracle/live.rs`: Pre/Post predicate consumers, first-poll snapshot, planned route ownership, remote fragment serialization.
- Wyrd error mapping `oracle/mod.rs`, authorized table discovery/cut, codec revision/pins/lock.
- Peer remote journeys `wyrd-testing/tests/bifrost/oracle/peer_network/live_subquery.rs`: existing literal/footer/join proof plus analytical two-row Filter/Projection rejection; existing PeerCluster/client/auth helper reuse.
- Rust `query_coverage.rs`: fixed mixed-state corpus plus refusal cases; existing Rust/Python/TS/MCP focused query journeys and Panics documentation.
- Test-support fragment observation fields and dispatcher/peer service callers from R1 are still in cumulative scope.

## Review boundaries

Use the full wyrd-task-review skill and AGENTS/agent-rules/reference router with applicable full authorities. Review cumulative Wyrd base-to-candidate AND pinned fork original-to-candidate; use latest diffs for navigation/closure only. No original implementation task existed. Private prescribed mechanics are subordinate to behavior and existing/native reuse. Owner required compact common-query/CTE join coverage and no benchmarks; do not demand new range-filter optimization rejected by prior validation.

Do not treat the chat implementation summary or another discovery review as evidence or an intended verdict. Committed task evidence is available verification to reconcile independently with source/assertions. Check prior FIND-1 through FIND-4 closure as hypotheses. A public standard aggregate may be appropriate native reuse; investigate actual semantics and cost rather than insist on a previously prescribed private node.

Read-only candidate: write only your assigned new report. Other reviewers share the workspace; do not modify their files, prior reports, tasks or source. No benchmarks, synthetic load, heavy Cargo lanes or external-environment tests during discovery without orchestration. Candidate/available checks were clean at start; cumulative diff-check passed. Final integrated capability verification is a later change-review gate.
