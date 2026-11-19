# Focused independent verification

Prior candidate 9abeaf124585cd0d726d2554965b105fc0c0ef86.
Candidate e5af2ce5caef2d00303e1c980b12c8a9bca4af0e.
No source changes, test mutation/broken-source experiments, broad gate or load.

## Vala owner/action/check/budget/records

```bash
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::timed_out_attempt_settles_before_retry_or_result) | test(=blocks_agent::agent_check_settles_before_result) | test(=blocks_agent::interrupted_blocking_work_keeps_its_permit) | test(=blocks_agent::admitted_waiters_survive_halt_without_dispatch) | test(=blocks_agent::agent_tool_budget_spans_attempts_without_refund) | test(=blocks_agent::agent_records_survive_failed_steps_and_inconclusive_checks)'
```

PASS 6/6, run ID 096acda0-dfe3-463b-86ec-d22172196063.
Four-phase timeout test 6.025 s; check completion also passes.

## Hosted work guards and setup deadline

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tools::pg_tests::extraction_settles_before_workspace_release) | test(=verification::hosted::tools::pg_tests::code_workspace_wait_obeys_operation_deadline)'
```

PASS 2/2, run ID 10b9edab-5594-40cd-a56b-5e0e93f9f2f0.

Reported wider owner lanes, format/lints/dependencies, historical RED and test
mutation observations remain implementation evidence, not newly executed here.
Actual assertions and source adequacy are separately reviewed.

## Changed direct-call hosted fixtures and real worker consumers

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_bridge_preserves_declared_tool_boundaries) | test(=pg_hosted_artifacts::code_host_waits_obey_deadlines_and_cancellation) | test(=pg_hosted_artifacts::capacity_refusal_is_busy_at_every_hosted_consumer)'
```

PASS 3/3, run ID b1308a98-65af-49c9-ae72-ecc2ec4dd6ba.
Total independently rerun: 11 tests, all passing.
Cumulative whitespace check PASS:
`mise exec -- git diff --check 9abeaf124 e5af2ce5`.
