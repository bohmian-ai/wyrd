# Review-time focused verification

Immutable subjects are recorded in scope.md. No source changes were made.

1. Wyrd focused nextest command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --no-tests=fail -E 'test(=scan_sizing::tests::nanosecond_pruning_preserves_unknown_container_bounds) | test(=oracle::live::tests::empty_live_route_selections_do_not_warn) | test(=oracle::memtable_source::tests::partial_statistics_preserve_conservative_filtering) | test(=oracle::memtable_source::tests::point_lookup_prunes_batches_outside_their_bounds) | test(=oracle::memtable_source::tests::range_filter_skips_evaluation_only_on_proven_batches) | test(=scan_sizing::tests::scan_sizer_counts_covered_columns_of_kept_batches) | test(=oracle::exec::tests::hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row)'
```

Result: PASS, all seven selected tests, 895 skipped; nextest run
`13be3621-addc-4826-8672-ebd90a3ff384`.

2. DataFusion original statistics-owner module, in the recorded fork worktree:

```bash
mise exec -- cargo test --locked -p datafusion-common --lib pruning::tests::
```

Result: PASS, 13 selected tests, 604 filtered out. Includes all D1/D2/D4 names.
Compiler emitted an unused-import warning in unchanged `common/src/config.rs:4058`
for this feature selection. This focused run does not claim workspace lint,
all-feature or extended-suite proof.

3. DataFusion real predicate regression in the recorded fork worktree:

```bash
mise exec -- cargo test --locked -p datafusion-pruning --lib pruning_predicate::tests::empty_containers_prune_without_warnings -- --exact
```

Result: PASS, one selected test, 110 filtered out. Build emitted deprecation
warnings in unchanged execution/physical-plan code. Neither focused fork run
passed an explicit toolchain override; they are not evidence of the separately
claimed pinned-toolchain checks.

The task's
implementation-evidence tables remain the available historical RED/GREEN,
journey and broader-check claims; review-time runs do not replace missing
mandatory dependency checks.
