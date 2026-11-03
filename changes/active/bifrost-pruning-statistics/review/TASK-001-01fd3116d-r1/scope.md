# TASK-001 first implementation review scope

Wyrd root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-pruning-statistics`.
Base: `d7de12a74c4bdbf93e727cc2383147e34298f689`.
Candidate: `01fd3116d2501d89317073a2abb547818a23b4a8`.
DataFusion root: `/home/thorrester/Documents/GitHub/datafusion-pruning-statistics`.
Dependency cumulative base: `19796a13ed46877cf998013886947b28a6f774b3`.
Dependency candidate: `9c724e501b613ea1e3de6a7e8d9fb700eed65b25`.
Both working trees were clean at review start. No Wyrd `.codegraph/` exists.

Authority: approved revision 3 `changes/active/bifrost-pruning-statistics/spec.md`;
original `tasks/TASK-001-dependency-owned-pruning.md` under that packet.
Available verification evidence is its appended Implementation Evidence section.
Review actual committed ranges with `git diff <base>...<candidate>` in each repo.
No prior implementation acceptance is carried forward; readiness is not acceptance.

## Starting navigation map (reviewers must expand and verify)

- DataFusion `common/src/pruning.rs`: `PrunableStatistics`, shared exact min/max
  conversion, new diagnostic helper and regression matrix. Producer inputs are
  `Statistics`/`ColumnStatistics`; scalar conversion is in `common/src/scalar/mod.rs`.
- DataFusion `pruning/src/pruning_predicate.rs`: real predicate consumer and new
  empty/mixed proof; sibling file consumer in `pruning/src/file_pruner.rs`.
- Wyrd `scan_sizing.rs`: `BatchPruning::keep`, `fully_matching`, `ScanSizer`,
  restored mixed nanosecond regression and removed zero-batch guard.
- Wyrd `oracle/live.rs`: `LiveTableRoutes::selected_by`, `LiveScribeExec::resolved`,
  dispatch loop and new empty-route regression.
- Wyrd `oracle/memtable_source.rs`: batch exclusion, full-match, sizing, new
  partial-statistics proof; statistics produced by `scribe/memtable.rs`.
- Wyrd `oracle/exec.rs`: `set_time_bounds` visibility changed; persisted readers
  and existing footer tenant refusal proof remain relevant consumers.
- Wyrd `oracle/codec.rs`: pinned dependency revision/fingerprint constant.
- Wyrd Cargo.toml/Cargo.lock: eleven patched packages and resolution.
- `wyrd-testing/tests/bifrost/oracle/published.rs`: new SDK/server journey;
  fixtures in support, canonical_signals, cluster and production telemetry.
- `mise.toml`, `TESTING.md`, `.config/nextest.toml`, fork AGENTS/contributor
  instructions: proof commands, required checks and independent fork authority.

## Reviewer selection

- Fresh behavior-rev: full acceptance/invariant/caller and test evidence audit,
  including ordinary failure containment, maintainability and reuse.
- Fresh repo-rev: two repository authorities/toolchains, changed-item rustdoc,
  imports, manifests, test rules, mandatory gates and proof compliance.
- Fresh domain-rev: data correctness of generic typed statistics conversion,
  parameterized values, genuine mismatch fallback, diagnostic non-disclosure,
  and pruning/full-match consumers. No persistence or lifecycle redesign is
  expected, so separate lifecycle/system/maintainer reviewers are not selected.
- Fresh ponytail-rev after discovery: independent source/materiality validation
  and final deduplicated findings ledger, even if discoveries are clean.

Reports only in this directory. Do not edit reviewed source, packet, manifests,
lockfiles or tests. No commits/pushes. Cargo-backed work must be sequential per
target directory; ask the orchestrator before running any build/test/lint.
