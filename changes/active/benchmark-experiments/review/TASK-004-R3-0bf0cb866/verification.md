# Independent verification

Source HEAD `0bf0cb8662a221233a963ed147d57ef0cbb6720f`. No source mutation.
The uncommitted spec and TASK-005 edits are preserved, identities in scope.md.
Cargo commands run sequentially; reviewers perform source inspection separately.

## Focused affected lane — PASS

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --lib --bin capacity --test-threads=1 -E 'test(/^capacity::tests::/) | test(/^experiment::tests::/) | test(/^query::run::tests::/) | test(/^query::competing::tests::/) | test(/^gateway::run::tests::/) | test(/^gateway::capacity::tests::/) | test(/^verification::run::tests::/) | test(/^client_queue::/) | test(/^tests::/)'"
```

Both migration/idempotence tests pass; 46 selected tests pass across library
and capacity binaries, 58 skipped by explicit scope. This includes current
smoke expectations; a green test does not establish its expected behavior
matches authority.

## Exact closure/calibration/smoke selectors — PASS

```sh
mise exec -- cargo nextest run --locked -p wyrd-testing --lib --bin capacity --test-threads=1 -E 'test(=capacity::tests::the_measurement_freezes_at_the_stop_before_work_drains) | test(=capacity::tests::known_delay_overhead_and_cpu_calibrate_the_measurement_path) | test(=experiment::tests::smoke_runs_the_selected_workflow_and_stays_out_of_history) | test(=query::run::tests::otlp_exports_carry_their_scoped_labels) | test(=query::competing::tests::cohorts_follow_admission_not_completion) | test(=query::competing::tests::arrivals_waking_after_the_end_are_missed) | test(=verification::run::tests::arrivals_waking_after_the_end_are_missed) | test(=gateway::capacity::tests::the_last_arrival_waking_after_the_end_is_missed)'
```

8/8 pass. These exact tests need no external services.

## Static checks — PASS

```sh
mise exec -- cargo fmt -p wyrd-testing -p wyrd-telemetry -- --check
mise exec -- cargo clippy --locked -p wyrd-testing -p wyrd-telemetry --features wyrd-telemetry/test-support --all-targets -- -D warnings
git diff --check c9d72729b 0bf0cb866
git diff --check
mise run check:deps
```

Dependency boundary check passes, including its production-feature negative test.

## Existing release evidence and limits

Located implementation's clean-worktree artifacts at:
`/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-benchmark-experiments/b20fe86c-f621-4d14-bf8c-ff976ee65dc5/scratchpad/clean/target/benchmarks/smoke/`.
The worktree is now at merge `9a540b7b8`; the eight successful default-plan
records identify `2f43ca9bd` and mode smoke, not the reviewed merged candidate.
The additional writes record `20261025T214449Z` is INVALID, and another
Verification smoke is the full-run prepass. Records and timing files exist;
these are historical smoke proof, not a new execution or final-candidate proof.
The checkout's own target contains earlier dirty-source INVALID records.

Readback executed using the preserved clean-worktree release executable:

```sh
mise exec -- /tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-benchmark-experiments/b20fe86c-f621-4d14-bf8c-ff976ee65dc5/scratchpad/clean/target/profiling/release/capacity report /tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-benchmark-experiments/b20fe86c-f621-4d14-bf8c-ff976ee65dc5/scratchpad/clean/target/benchmarks/smoke/gateway/overhead/20261025T215225Z-2f43ca9bd.json
```

Exit 0, regenerates historical Gateway report with mode smoke and PASS, including
capture shedding (20,631 items in chat-capture.c64) and inapplicable criteria.
This checks persisted readback, not current-source compilation/execution.

No release smoke or full benchmark rerun during this review. No full language
journey suites or broad gate: those belong to integrated review, not this bounded
remediation. Imported PR215 is checked at changed benchmark/production seams,
not re-reviewed as an entire schema change.
