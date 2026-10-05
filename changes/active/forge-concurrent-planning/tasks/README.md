# Forge implementation tasks

The current approved authority is spec revision 12. The tasks were derived from
earlier revisions, recorded in each task's `spec_revision`. There are six
behavioral implementation steps, in order:

1. TASK-001-leader-and-promotion.md — one leader and exact Iceberg commit tracking.
2. TASK-002-pull-and-worker-results.md — compactor pull, worker planning and reports.
3. TASK-003-maintenance-and-removal.md — timer maintenance, safe cleanup and removal.
4. TASK-004-compaction-defaults-and-type.md — compaction on by default; per-table compaction type (REQ-011, REQ-012, added 2026-10-03).
5. `../revision/TASK-005-iceberg-filtering-across-tiers.md` scenarios 0–3 — field IDs and pruning across hot, promoted, and rewritten cuts.
6. TASK-005-R1-active-table-reader-cut.md — replace TASK-005 Scenario 4 with
   one tenant-scoped active cut, reader-controlled destructive cleanup, and
   terminal hot-file metadata retirement; passing it completes TASK-005.

Only the approved revision-12 spec, these tasks, and their review remediation tasks direct Forge and Oracle cut implementation. The separate Python docstring task remains an owner-directed documentation task.

Start by dropping the nine Forge concurrent-planning/review commits after
`ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`; keep the unrelated
`7b56776ff` lint/SDK fix. Preserve this approved spec and task packet across
the history rewrite. The older Forge implementation at `ce5c09ef` still has
planning-demand machinery; TASK-001 through TASK-003 simplify that baseline.
The nine commits to drop are `a86f2d6e7`, `42b64cb1c`, `f21338a66`,
`36d8634a6`, `6937c7603`, `f67b59b2c`, `be922f434`, `2cbf8b7b3`, and
`1e5fae2c7`. Save a branch ref at the old head before rewriting history.

## Existing uncommitted implementation edits

Before changing history, save a recoverable copy of the current branch and
the five dirty code/test diffs. Then discard those five revision-2
pass-cutoff edits as part of dropping concurrent planning. Do not replay them
onto the old baseline. Preserve the approved spec and new tasks separately;
the old revision-2 task documents are not implementation authority.

| Dirty path in wyrd-forge | Current uncommitted change | Disposition |
| --- | --- | --- |
| planning_scheduler.rs | Adds a Postgres pass cutoff to the concurrent claim loop. | Save diff, then discard with revision-2 code. |
| forge_tasks.rs | Adds pass-cutoff claim SQL. | Save diff, then discard with revision-2 code. |
| pg_forge_tasks.rs | Adapts claim tests to the cutoff API. | Save diff, then discard with revision-2 code. |
| production_routes.rs | Adds failed-backlog/cutoff coverage. | Save diff, then discard with revision-2 code. |
| expired_cleanup.rs | Adapts a claim fixture. | Save diff, then discard with revision-2 code. |

Dropping the branch-only commits removes the concurrent implementation and
its branch-only migration in one step. TASK-003 still removes the older
planning-demand implementation after its replacement behavior passes.

## Final comparison required for review

The RisingWave links in each task target exact lines in the local checkout at
commit e23ddf952c3e6ebc03cc254789e84d1179cfacae. Reviewers should compare
against that pinned commit, not an updated RisingWave branch. A completed
implementation report must fill every task's comparison row with: the exact
RisingWave link, the implemented Forge source link, the focused test name and
executed command/result, or an explicit approved Wyrd difference. Empty rows
block completion. The approved Wyrd differences are Scribe hot-publication
recovery, Oracle/hot-object deletion protection and Bifrost central-governor
charging with governed spill placement. Worker pull, physical selection,
result handling and Iceberg maintenance follow the cited RisingWave logic.

TASK-002 additionally requires the reproducible release-mode
bench:bifrost:forge-capacity result. Passing functional tests alone do not
establish that the leader stays out of the throughput path. Report decision
latency separately from RPC/catalog time and show 1/2/4-worker completion
rates, leader CPU, worker occupancy, and storage/SQL utilization under the
same sustained backlog. The task's numeric gates determine pass or failure.
TASK-001 and TASK-002 use only their exact focused RED/GREEN commands;
TASK-003 alone runs mise run verify:bifrost once after integration. Do not
repeat component Bifrost lanes as final verification.

Each task names the existing test and benchmark owners it must extend.
Review rejects a second replica runner, Forge completion ledger, test clock,
catalog-fault wrapper, release server, resource sampler or generic benchmark
reporter. A task report must list reused owners and justify every new helper.
