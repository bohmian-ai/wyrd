# TASK-001-R1 remediation review scope

Wyrd root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-pruning-statistics`.
Original base: `d7de12a74c4bdbf93e727cc2383147e34298f689`.
Prior reviewed candidate: `01fd3116d2501d89317073a2abb547818a23b4a8`.
Current candidate: `6707728421b2b9a931aaf559144581a5257447c7`.
DataFusion root: `/home/thorrester/Documents/GitHub/datafusion-pruning-statistics`.
Original dependency base: `19796a13ed46877cf998013886947b28a6f774b3`.
Prior dependency candidate: `9c724e501b613ea1e3de6a7e8d9fb700eed65b25`.
Current dependency candidate: `b669163b78a7692a9a381d4d13dceeae632c5c34`.

Both worktrees were clean at start. Both prior candidates are ancestors of the
current candidates; no rebase or patch-correspondence reconstruction is needed.
The earlier no-CodeGraph navigation assessment carries forward; check for an
index before source traversal if one has appeared.

## Authority and context

- Approved spec revision 3: `changes/active/bifrost-pruning-statistics/spec.md`.
- Original task: `changes/active/bifrost-pruning-statistics/tasks/TASK-001-dependency-owned-pruning.md`.
- Prior reports, verdict, validation and remediation task:
  `changes/active/bifrost-pruning-statistics/review/TASK-001-01fd3116d-r1/`.
- Remediation: `TASK-001-R1-close-required-gates.md` in that prior directory;
  its appended Implementation evidence is the available verification/diagnosis
  record. Treat it as a claim to check against source, not an intended verdict.
- Applicable root/nested instructions, task-review skill, spec-driven workflow,
  maintainer/Rust guides, Bifrost source/Variant authority and fork contributor
  requirements continue to govern.

Use `git diff <prior-candidate> <current-candidate>` for each remediation delta;
the full original-base cumulative ranges remain available as acceptance context.
Only reports may be written; reviewed source and packets remain immutable.

## Starting map (expand and verify against source)

- Wyrd `scan_sizing.rs::BatchPruning::keep` documentation and test-module imports;
  `oracle/live.rs` test imports; `published.rs` journey helper constants/imports.
- Wyrd eleven DataFusion patches, regenerated Cargo.lock and
  `oracle/codec.rs::ORACLE_DATAFUSION_REVISION` move to the current fork revision.
- Fork `physical-expr-adapter/src/schema_rewriter.rs` formatting and workspace
  Cargo.toml ordering; Cargo.lock compatible updates; unused dependencies removed
  from functions-variant/Cargo.toml.
- Fork `core/tests/memory_limit/memory_limit_validation/utils.rs::measure_max_rss`
  now initializes an empty sysinfo System before refreshing the measured pid;
  trace all shared test callers and measurement/assertion semantics.
- Fork `core/tests/parquet/variant.rs::invalid_stored_variants_are_errors` changes
  a lookup from field k to index 0; trace corrupt fixtures and pinned arrow-rs
  path-only decoding before deciding whether proof is preserved.
- Fork `functions-variant/src/lib.rs` documentation clarifies malformed bytes
  read by a function; trace whether executable behavior changes in this delta.
- Available proof: prior accepted runtime matrix, remediation evidence including
  pinned-toolchain gate results and two failure diagnoses, focused Wyrd tests,
  journeys and owner checks. Verify changed dependencies do not invalidate carryover.

## Selection and carryover

Select one fresh remediation ponytail reviewer for closure, changed source,
gate repairs, applicable standards, test-diagnosis validity and affected paths.
Select a second fresh independent ponytail validator after that report to
validate closure and any proposals. Roles remain separate; neither is the
orchestrator. No new system/lifecycle/security specialist is selected initially:
the delta contains bounded test/gate repairs and compatible locked updates,
not a new shared authority, durable behavior or public runtime boundary. Add a
focused specialist only if actual evidence exposes a material unresolved boundary.

Carry forward the first review's source-backed acceptance of partial statistics,
safe pruning/full-match, empty dispatch, diagnostics and source/tenant authority
where executable source is unchanged. Reopen the changed fork test measurements,
Variant fixture/proof, compatible locked packages, gate delivery and pin consumers
because remediation changes them. This is the first remediation review, not a
restart of comprehensive discovery or a convergence-limit round.

Report paths for this round:
- `remediation-review.md`: fresh closure reviewer.
- `findings-validation.md`: separate independent validator.
- `verdict.md`: orchestrator after validation.

No Cargo builds/tests/lints without orchestrator coordination; commands through
mise and Cargo-backed work sequential per target directory. No commits/pushes.
