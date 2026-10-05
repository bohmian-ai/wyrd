# Packet-wide Review Navigation Map

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Branch: `worktree-agent-ac58f45cb5b747685`
- Base (excluded): `c1508b375`
- Candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Range: `c1508b375..7ac45dec9`
- Approved authority: `changes/active/forge-concurrent-planning/spec.md`, revision 11
- Task index: `changes/active/forge-concurrent-planning/tasks/README.md`
- RisingWave reference: `/home/thorrester/Documents/GitHub/risingwave` at `e23ddf952c3e6ebc03cc254789e84d1179cfacae`

## Task authorities

- `tasks/TASK-001-leader-and-promotion.md`
- `tasks/TASK-002-pull-and-worker-results.md`
- `tasks/TASK-003-maintenance-and-removal.md`
- `tasks/TASK-004-compaction-defaults-and-type.md`
- `revision/TASK-005-iceberg-filtering-across-tiers.md` (Scenario 4 superseded by R1)
- `tasks/TASK-005-R1-active-table-reader-cut.md`
- `revision/TASK-006-python-api-docstrings.md`

Revision 11 wins over older task language. Read the spec revision history, especially revisions 6, 8, 10, and 11.

## Governing repository authority

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/references/README.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/maintainer-style.md`
- applicable routed domain/language/testing references

## Changed owners and likely seams

- Forge leadership and scheduling: `vala-bifrost-redux/src/forge/{leadership,leader,scheduler,worker,settings}.rs`; SQL in `vala-sql/src/queries/{forge_leader,forge_tasks}.rs`; server peer path in `wyrd-server/src/grpc/forge_peer.rs`.
- Promotion, maintenance, and deletion: `forge/{scribe_promotion,expire,gc,table_authority,expiry_policy}.rs`; `vala-sql/src/queries/forge_operations.rs`; Forge integration and production journey tests.
- Oracle active-table reads: migration `20260910000025_oracle_reader_authority.sql`; `vala-sql/src/queries/oracle_reader_authority.rs`; `vala-bifrost-redux/src/oracle/{planner,mod,exec}.rs`; Oracle/Forge production journeys.
- Scribe and Iceberg publication/filtering: `scribe/{staging_runtime,execution_lanes,persistence,hot_stage,claim_publication,ingress}.rs`; catalog, Parquet promotion, table schemas/field IDs, Iceberg fork evidence, and hot/promoted/rewritten pruning tests.
- Contracts and defaults: `wyrd-spec/src/vala/*`, generated schemas, Rust/Python/TypeScript SDK projections, server config and Bifrost services.
- Python API documentation: `sdks/wyrd-sdk-python/python/wyrd/**`, generated `.pyi` projections, PyO3 aggregation and public tests. Generated files must be source-generated, not accepted merely because present.
- Capacity proof and regression harnesses: `wyrd-testing/src/bin/bifrost_forge_capacity/*`, `tests/bifrost/{forge,oracle,scribe,server}/**`, redux integration tests, SQL tests, Python and TypeScript tests.

## Required falsification paths

1. Active-read claim acquisition, use, deadline, drop/release, descendant task lifetimes, and every destructive Forge entry point.
2. One leader term, exactly-once promotion settlement, pull capacity and oldest-due selection, worker current-head decision, stale/late results, failover, and removal of planning-demand state.
3. TenantConn/RLS coverage, catalog-pointer definer scope, grants, and absence of `iceberg_catalog` access for `wyrd_app`.
4. PostgreSQL-owned coordination time; search production and tests for host-clock deadlines, eligibility, and brackets.
5. Test-only edits in commits `05cceaf35`, `484c3b4f6`, and `3a51a24d5`; require recorded diagnosis and root-cause placement without weakened assertions.
6. Deletion closure: no reader epoch, ancestry frontier, IO gate, reader-cut field, compatibility alias, retention query cap, planning-demand state, or leftover callers; only protobuf `reserved "reader_cut"` is allowed.
7. Rustdoc, struct-centered ownership, earned async, `#[allow]`/unwrap constraints, and top-level Python tests.
8. Pinned RisingWave comparison must be non-empty and deviations limited to Scribe hot-publication recovery, Oracle/hot-object deletion protection, and central-governor charging with governed spill placement.

## Available verification claims to independently assess

- `mise run verify:bifrost`, `mise run test:principals:integration`, `mise run fmt`, `mise run lints`, and `git diff --check` were reported green.
- R1 records focused S1/S2 commands and an S2 RED with drop release disabled.
- TASK-002 records release-mode capacity evidence at `69d2efe8c`.

These are inputs, not acceptance proof. Re-run focused checks where needed to falsify a claim.
