# Remediation review verification

Immutable candidate bfe479b9248f4f71c43a719b61db1ce35bad69d9; prior candidate dabe0e7dde58c6e26d584120ae0a48cfede79436. Source unchanged during review.

- `git diff --check dabe0e7dd bfe479b92`: PASS.
- `git merge-base --is-ancestor dabe0e7dd bfe479b92`: PASS; no rebase correspondence needed.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=bifrost::telemetry::tests::resources_render_elapsed_and_process_scope)'`: PASS 1/1 (70 skipped), run c10d99aa-18da-4d60-ab9a-7d8970f3b558, 0.023s test time. macOS linker emitted the existing compact-unwind warning; build/test succeeded.

## Available original measurement evidence

The committed R1 task records three repetitions per production side on the same harness, host and build settings, with base-file substitution and restore. Review did not repeat that substitution or change source. Existing original logs were located read-only under:

`/private/tmp/claude-501/-Users-stevenforrester-Documents-GitHub-schema-change-task-7/e8060899-5ac4-426b-8ba3-09a54d77cba8/scratchpad/measure/`

Files include `base-journeys-{1,2,3}.log`, `candidate-journeys-{1,2,3}.log`, `base-storage-{1,2,3}.log`, `candidate-storage-{1,2,3}.log`, `base-trace-{1,2,3}.log`, and `candidate-trace-{1,2,3}.log`. The fresh validator independently inspects required measurements and records the closure evidence in findings-validation.md. These original logs are implementation verification evidence, not new review reruns. No full logs, patches or source archives are copied into this review directory.

## Completed review reruns

- `mise run build:server`: PASS, 1m56s; existing macOS compact-unwind linker warning only.
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::completed_result_lookup_preserves_evidence) | test(=verification_runtime::window_lookup_preserves_population) | test(=verification_runtime::group_lookup_preserves_source_occurrences) | test(=verification_runtime::custom_query_preserves_evidence_and_limits) | test(=verification_runtime::exploration_filters_keep_exact_populations)"'`: PASS, all 5/5 (50 skipped), run 27495698-dc8d-4854-bcb9-9c39e2e9420b, 90.534s total with serial execution. Migration prerequisite passed 1/1, run 5c6ac06f-31dd-4c6a-b3b1-e2c21efb7d10. Postgres lifecycle exited successfully. The unchanged window journey completed in 33.958s; expanded exploration in 42.173s. Build/test emitted the existing linker warning; no failure.

No baseline production files were swapped during review. Original three-repetition measurement, storage and trace outputs were independently inspected by the validator; broad gates, full Oracle/distributed suites, gateway payload journey, storage/trace reruns, platform/capacity qualification and format/lint/tenant checks were not repeated. Implementation's format/lint/tenant results and unaffected prior production acceptance are carried forward. No additional run was justified after the focused tests passed.
