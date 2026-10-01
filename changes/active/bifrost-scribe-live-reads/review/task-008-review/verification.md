# Independent verification

Candidate 6e7add054e33701ca5ecb52a5c859948b15161a3, 2026-09-30. Each command ran from this worktree root. No Postgres wrapper, full mise lane, benchmark, source edit or commit.

```sh
CARGO_TARGET_DIR=$PWD/target-review MISE_STATE_DIR=$PWD/.mise-review-state WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=parquet::footer::tests::footer_tenant_proof_refuses_missing_and_foreign_tenants) | test(=oracle::exec::tests::hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row) | test(=oracle::follower::tests::scribe_live_sources_keep_the_session_partition_count) | test(=oracle::live::tests::live_frames_release_batches_incrementally_and_validate_the_footer)'
```

PASS: 4 selected, 4 passed, 804 skipped. First rebuild completed in 1m30s; tests completed in 0.067s. Nextest run 7f144779-495e-4a97-9e7e-1c03226ce934.

```sh
CARGO_TARGET_DIR=$PWD/target-review MISE_STATE_DIR=$PWD/.mise-review-state WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=oracle::live::tests::native_completion_reconciles_the_delivered_output)'
```

PASS: 1 selected, 1 passed, 807 skipped. Nextest run 7a229e87-e2b0-4424-bfb6-f43c6e2e8267.

```sh
git diff --check
git rev-parse HEAD
git status --short
```

Whitespace PASS. HEAD remained candidate; no tracked source changes. Only review artifacts and build/state directories are untracked.

Candidate task evidence claims module, journey, lint, format, codegen and docs passes. Those historical results were not reproduced in this constrained review and do not replace source audit. In particular server/SDK journeys, Postgres-dependent integration and full gates remain outside permitted execution.
