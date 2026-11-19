# Focused independent verification

Subject: prior 0e51af2a2332f362ab44b7c5b88b51c3cb4f8011 to
f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18 (source candidate 039374040).
No source edits, broken-source experiments, broad gate or performance load.

## Vala dispatch/delivery/capacity

```bash
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::admitted_waiters_survive_halt_without_dispatch) | test(=blocks_agent::interrupted_blocking_work_keeps_its_permit) | test(=blocks_agent::code_bridge_shares_the_recorded_path) | test(=blocks_agent::oracle_queue_refusal_is_busy_at_every_origin)'
```

PASS: 4/4, run ID 72d297a9-159c-43f3-98ce-c8c8ce29016f.

## Hosted owner lifecycle/deadline/conversion

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tools::pg_tests::extraction_settles_before_workspace_release) | test(=verification::hosted::tools::pg_tests::code_workspace_wait_obeys_operation_deadline) | test(=verification::hosted::sandbox::tests::code_bridge_json_conversion_is_lossless_and_bounded)'
```

PASS: 3/3, run ID e2bc5429-5b59-45cc-8c43-a3e518d2a557.

## Hosted consumer behavior and queue composition

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_host_waits_obey_deadlines_and_cancellation) | test(=pg_hosted_artifacts::code_bridge_preserves_declared_tool_boundaries) | test(=pg_hosted_artifacts::capacity_refusal_is_busy_at_every_hosted_consumer) | test(=pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees)'
```

PASS: 4/4, run ID 6c5edb1d-2b1b-48a5-b172-791c330bb4e6.

Cumulative diff whitespace check: PASS:
`mise exec -- git diff --check 0e51af2a2 f8b45a720`.
Implementation-reported owner lanes/format/lints/dependency and RED runs remain
reported evidence; these review runs do not claim to reproduce them.

## Public Rust SDK failure/recovery

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::code_mode_failure_releases_session)'
```

PASS: 1/1, run ID 1992849f-a6cd-49e9-a656-f35bb57a6137, 14.16 s.
Total independently rerun: 12 tests, all passing. Passing tests establish only
their actual assertions; reviewers assess adequacy against approved boundaries.
