# Independent focused verification

Immutable candidate: 9abeaf124585cd0d726d2554965b105fc0c0ef86.
Prior candidate: f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18.
No source edits, broken-source experiments, new tests, full suite or broad gate.

## Changed lifecycle and V2 Vala regressions

```bash
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::timed_out_attempt_settles_before_retry_or_result) | test(=blocks_agent::interrupted_blocking_work_keeps_its_permit) | test(=blocks_agent::admitted_waiters_survive_halt_without_dispatch) | test(=blocks_agent::agent_tool_budget_spans_attempts_without_refund) | test(=blocks_agent::agent_records_survive_failed_steps_and_inconclusive_checks)'
```

PASS: 5/5, run ID 664b6075-5cf7-4738-aec9-c5ccb27e005d.
New timeout test takes 2.015 s for retry/no-retry cases.

Implementation-reported RED, wider owner lanes, formatting/lints/dependency
checks remain reported evidence, not newly claimed review executions. Passing
tests prove actual assertions; source review determines acceptance adequacy.

## Unchanged hosted producer/guard regressions

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tools::pg_tests::extraction_settles_before_workspace_release) | test(=verification::hosted::tools::pg_tests::code_workspace_wait_obeys_operation_deadline)'
```

Cumulative whitespace check passed:
`mise exec -- git diff --check f8b45a720 9abeaf124`.

Hosted owner result: PASS 2/2, run ID 3c24b621-af44-474d-934a-ee535372a900.
Total independently rerun: 7 tests, all passing.
