# Independent verification

Candidate: `23eafa368bca19208faf8311eb7b5421e3660b38`, 2026-09-30. No Postgres wrappers, full mise lanes, benchmark, source edits or commits. All commands ran from this worktree root.

## Focused proof

```sh
CARGO_TARGET_DIR=$PWD/target-review MISE_STATE_DIR=$PWD/.mise-review-state mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=parquet::footer::tests::footer_tenant_proof_refuses_missing_and_foreign_tenants) | test(=oracle::exec::tests::hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row) | test(=scribe::parquet_writer::tests::parquet_footer_preserves_complete_claim_evidence) | test(=scribe::parquet_writer::tests::generation_encoding_preserves_sort_tenant_and_artifact_identity) | test(=schema::managed_columns::tests::with_managed_columns_appends_the_envelope_without_a_tenant_column) | test(=oracle::follower::tests::scribe_live_sources_keep_the_session_partition_count) | test(=oracle::live::tests::native_completion_reconciles_the_delivered_output) | test(=oracle::follower::tests::scribe_staged_scan_prunes_non_matching_row_groups) | test(=oracle::follower::tests::a_dropped_staged_scan_releases_its_lease_immediately) | test(=oracle::follower::tests::scribe_provider_projects_and_filters_a_nonzero_ordinal) | test(=scribe::tail_rpc::tests::an_open_live_read_keeps_staged_runs_across_publication) | test(=resources::tests::scribe_follower_execution_shape_contract) | test(=oracle::live::tests::live_frames_release_batches_incrementally_and_validate_the_footer)'
```

PASS, exit 0: 13 selected, 13 passed, 795 skipped. Nextest ID `bd1a5edc-3f95-42c9-b644-58612817b77d`; build 1m34s, tests 2.310s. [Captured output](focused.log). Every exact expression matched its named test.

## Static checks

```sh
CARGO_TARGET_DIR=$PWD/target-review MISE_STATE_DIR=$PWD/.mise-review-state mise exec -- cargo fmt -p vala-bifrost-redux -- --check
CARGO_TARGET_DIR=$PWD/target-review MISE_STATE_DIR=$PWD/.mise-review-state mise exec -- cargo clippy --locked -p vala-bifrost-redux --features test-support --all-targets -- -D warnings
git diff --check HEAD~1 HEAD
git diff --check a7582db587c6170a290760f1741673125612b797 HEAD
git diff --check
git diff --exit-code
git rev-parse HEAD
```

Scoped format PASS, exit 0. Scoped Clippy PASS, exit 0, completed in 2m16s; captured output [clippy.log](clippy.log). This is not the repository-wide all-features lane.

Both committed-range whitespace checks FAIL, exit 2: ten trailing spaces in `task-008-review/final-named.log:1-10` and an extra blank line at EOF in `task-008-review/task-review-invariants.md:55`. [R1 output](diff-check.log). Working-tree whitespace and tracked diff checks PASS, exit 0; these do not inspect whitespace already committed. HEAD remains the immutable candidate.

`git diff --check HEAD~1 HEAD -- crates architecture docs changes/active/bifrost-scribe-live-reads/tasks` passes, exit 0 ([empty scoped output](scoped-diff-check.log)), isolating the committed whitespace diagnostics to the added prior-review artifacts.

## Evidence limits

The supplied `task-008-review/final-named.log` records 13 redux units, 3 server units, 9 environment-backed journeys plus migration proof, each selecting one test and passing. The TASK-008 integrated-tree section supplies package/features/target/environment recipes with concrete names and attribution. Those supplied server/journey/gate results were inspected, not independently rerun. Codegen/docs/full lanes/capacity remain outside this review's permitted execution. Claims in historical evidence are distinct from independently executed checks here.
