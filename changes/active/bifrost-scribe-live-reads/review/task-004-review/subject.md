# TASK-004 review subject

- Repository root: /home/thorrester/Documents/GitHub/wyrd-verfication-t006-merge (branch vcc/task-004).
- Base: origin/main a56ab7569aa702dddec0b36d097d38abe8918e8c.
- Immutable candidate: 990803fc06811c87e1d9312fc14e80f42496ee1b. Review `git diff a56ab7569..990803fc0`.
  Later commit 78bd1049b (and anything touching changes/active/opitimization-and-benchmarks) is a separate
  draft spec and is excluded.
- Approved spec: changes/active/bifrost-scribe-live-reads/spec.md, revision 24.
- Task: changes/active/bifrost-scribe-live-reads/tasks/TASK-004-integrate-eval-server-and-unify-bifrost-memory.md
  (original body plus Revision 13 and appended evidence D1-D19).
- Related contract: changes/active/verified-change-contract/spec.md (task says carry revision 44 from vcc/task-006 0e9c6e98c).

## Scope notes
- The range also carries commits recorded under sibling tasks of the same change (TASK-005 telemetry,
  TASK-006 real-server benchmark / process-harness removal, TASK-007 one Parquet scan, TASK-008 tenant-proven
  per file) which have their own reviews in this review/ directory. Do not raise DRIFT merely because those
  sibling-task changes appear; DO raise a finding when they break a TASK-004 obligation.
- Review artifacts under changes/active/*/review/ are out of scope.
- Performance optimization is deferred to change `opitimization-and-benchmarks`; "could be faster" is not a finding.
- The bifrost capacity benchmark must NOT be re-run (owner order). Use the benchmark evidence recorded in the task
  (Final benchmark evidence section; reports under target/bifrost-query-capacity/{standard,heavy}/ if present).

## Verification
- Orchestrator runs `mise run gate` once on the candidate tree (worktree HEAD 78bd1049b = candidate + one draft spec
  file). Results recorded in verification.md.
- Discovery reviewers are static; they may run read-only commands (git, grep, rg, cargo nextest list) but must NOT
  run Postgres lanes, mise gate/test lanes, benchmarks, builds that write to target/, or edit source.
  If a read-only cargo command is needed, set CARGO_TARGET_DIR=/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-verfication-t006-merge/33062735-d5bc-4324-9cb3-ab699f58e14e/scratchpad/target-review.
- Diffs (convenience, regenerate if in doubt): scratchpad/task004-code.diff (code, excluding changes/),
  scratchpad/task004-packet.diff (spec/task diff). Scratchpad =
  /tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-verfication-t006-merge/33062735-d5bc-4324-9cb3-ab699f58e14e/scratchpad
- No .codegraph index exists; use rg/git.
