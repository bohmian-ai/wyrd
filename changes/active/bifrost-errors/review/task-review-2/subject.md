# Immutable review subject

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`
- Candidate: `7d6a1ff95b3dc09315e3af236ab9906b305e11d0`
- Original task: none; approved `changes/active/bifrost-errors/spec.md` plus owner directions in this conversation.
- Prior verdict/ledger: `changes/active/bifrost-errors/review/task-review-1/{verdict,findings-validation}.md`.
- Remediation and verification table: `changes/active/bifrost-errors/review/task-review-1/bifrost-errors-R1-materialize-live-subquery-predicates.md`.
- DataFusion: `5697cfa35192f175bbcd54f59a5fd88527c2bbf5`, cached at `/home/thorrester/.cargo/git/checkouts/datafusion-44e2ebb5250b7f6f/5697cfa`.
- Distributed fork: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`, cached at `/home/thorrester/.cargo/git/checkouts/datafusion-distributed-615cde49e12a7772/8fac2a9`.

## Starting navigation map (expand independently)

- `oracle/mod.rs`: logical and physical error mapping, authorization, session selection; callers SDK/HTTP/MCP.
- `oracle/live.rs`: `LiveScribeExec` pushdown, `resolved(partition)`, fragment serialization, lazy `LiveFragmentRead::into_stream`, static/runtime route pruning; consumers `oracle/exec.rs`, codec and dispatcher.
- `resources.rs`: `OracleSessionShape::apply` shared session configuration, optimizer pass count; leader/follower/stage consumers.
- `oracle/analytical.rs`: `planning_session`; distributed flag and stage boundaries.
- DataFusion scalar snapshot, logical scalar-to-join rewrite, equijoin extraction, hash join dynamic filter producer/readiness, snapshot serialization and reset.
- `wyrd-server/src/state.rs`, `oracle/peer_service.rs`, Bifrost dispatcher: test-support fragment observations and exact footer rows; `wyrd-testing` peer cluster consumers.
- `peer_network/live_subquery.rs`: remote selective scalar and representative remote join.
- Rust `query_coverage.rs`: common query corpus over mixed published/live data; `query_bifrost.rs` and Python/TS/MCP focused journeys.
- `Cargo.toml`, lock, codec revision, `mise.toml`: dependency revision and test lane changes.

## Authority and audit boundaries

Read `AGENTS.md`, `architecture/agent-rules.md`, applicable design documents and canonical reference router. Apply the full `wyrd-task-review` skill. Audit cumulative base-to-candidate; use original candidate `f1e1be815069b23fa9920a61c1198524ecaad514` to locate remediation changes. Private prescribed mechanics are subordinate to behavior and repository authority. Owner added compact common-query coverage and CTE/table join coverage; no benchmarks or synthetic host load. Do not edit candidate source, dependencies, task evidence or earlier reports. Write only your assigned report in this directory. Other agents are reviewing concurrently.

Available verification is the committed remediation evidence table. Review its credibility and limits independently; do not assume an implementer's claim establishes acceptance. No broad test reruns or resource-heavy lanes during this review without orchestrator coordination.
