# Packet-wide re-review navigation map

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Original implementation base: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`
- Prior reviewed candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Remediation base and prior-review commit: `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`
- Re-review candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`
- Remediation delta: `e8d3cca13..7fcb45fc1`
- Cumulative implementation range: `c1508b375..7fcb45fc1`
- Checked-out branch has advanced beyond the candidate. Inspect candidate source with
  `git show 7fcb45fc1:<path>`, `git diff <base>..7fcb45fc1`, and commit-scoped
  commands; do not treat the current worktree file contents as the immutable subject.

## Authority and packet

- Approved authority: `changes/active/forge-concurrent-planning/spec.md`, revision 11.
- Task index and comparison obligations:
  `changes/active/forge-concurrent-planning/tasks/README.md`.
- Original tasks: TASK-001, TASK-002, TASK-003, TASK-004, TASK-005 surviving
  scenarios 0-3, TASK-005-R1, and TASK-006 in the packet's `tasks/` and
  `revision/` directories.
- Prior verdict and validated ledger:
  `changes/active/forge-concurrent-planning/review/packet-wide-7ac45dec9/{verdict,findings-validation}.md`.
- Remediation tasks in that same directory: TASK-001-R1, TASK-002-R1,
  TASK-003-R1, TASK-004-R1, TASK-005-R2, TASK-006-R1, and TASK-PACKET-R1.
- Repository authority: `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
  `architecture/bifrost-design.md`, and routed references.
- Pinned RisingWave comparison: `e23ddf952c3e6ebc03cc254789e84d1179cfacae`.
- Current narrowed iceberg-compaction fork pin is declared in workspace
  manifests and lockfile; validate its recorded comparison against the exact pin.

## Prior findings to close

- TASK-001: `FIND-TASK-001-1` revocable leader lifetime.
- TASK-002: `FIND-TASK-002-1` dispatch shutdown ownership;
  `FIND-TASK-002-2` exact shipped-fork comparison.
- TASK-003: `FIND-TASK-003-1` retained prepared cleanup settlement.
- TASK-004: `FIND-TASK-004-1` canonical compaction values/default prose;
  `FIND-TASK-004-2` Rust SDK projection.
- TASK-005-R1: `FIND-TASK-005-R1-1` leader-stream analytical ownership;
  `-2` immutable deadline; `-3` continuous destructive authority; `-4`
  sibling-maintenance test ban; `-5` SQL ownership.
- TASK-006: `FIND-TASK-006-1` runtime/stub parity; `-2` after-hook behavior.
- Packet: `FIND-PACKET-1` rustdoc; `-2` import/signature shape; `-3`
  diff check; `-4` broad final gate.

## Changed owners, callers, and consumers

- Leader and worker lifecycle: `forge/{leadership,scheduler,leader,worker,gc,
  scribe_promotion,metrics}.rs`; SQL election/task owners; private Forge peer;
  server boot/supervision; leader, worker, promotion, and production journeys.
- Cleanup and authority: `forge/{expire,orphan_gc,table_authority,worker}.rs`;
  `vala-sql` Oracle reader, Forge operation/task, catalog and file-list queries;
  expiry/orphan/reader-ordering/closeout tests.
- Analytical lifetime: `oracle/{analytical,analytical_supervisor,query_stream,
  planner,exec,follower,mod}.rs`, participant grants and resource envelope;
  distributed and held-cut journeys.
- Compaction contract/fork: workspace pin, `forge/settings.rs`, Wyrd spec wire
  types and schemas, shared client, Rust/Python/TypeScript SDK projections,
  docs, and fork-review evidence.
- Python runtime parity: `skald-agent/{callbacks,loop_runtime,python}.rs`,
  Python source stubs, generated public stubs, top-level runtime parity tests.
- Standards sweep: the broad Rust/doc/import changes, `scripts/checks/
  mocks-scope.sh`, and recorded final-gate evidence.
- Added adjacent behavior to regression-check: server supervision now restarts
  failed Forge worker/scheduler components; leader/worker/cleanup metrics were
  added. These are not prior finding closures and must not weaken shared-process
  integrity or broaden public contracts.
- Scope seam: commit `406464a59` adds
  `changes/active/bifrost-variant/spec.md` inside the immutable remediation
  range even though the implementer labels it another change packet. Determine
  its effect under the task-review rule that PASS requires no unrelated change
  in the reviewed diff. Commit `5ab92b003` is after the candidate and excluded.

## Verification claims to falsify

- Recorded focused tests in each remediation task.
- `mise run verify:bifrost`, `mise run test:principals:integration`, boundary
  checks, format, lints, and `git diff --check` are recorded green.
- Release benchmark records 16/16 checks and the final `mise run gate` is
  recorded green after three test-only wiremock allowlist entries.
- Review the deleted Python test and the replacement parity coverage; review
  `capacity_refused` classification for the first refused cleanup preparation;
  neither is automatically a defect without a violated approved obligation.
