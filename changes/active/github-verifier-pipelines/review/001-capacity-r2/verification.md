# Fresh verification for remediation review

Immutable candidate: ea4460c3c97adb55395acd50f062658bc8d0d4cd.

`git diff --check 34fb45bb3 ea4460c3c`: exit 0.

Fresh exact four-unit selector U running (hosted memory/defaults, config env, config file/env, runtime defaults). No overlapping Cargo work permitted.

Planned fresh checks: G exact gateway caller pair, O exact orchestration pair, J exact relocated real-client isolation journey, all within repository-managed Postgres. These are focused proof, not full capability suites. Results will be recorded here, separate from implementation-reported evidence.

The remediation packet retains implementation-reported fmt/lints/check:deps/docs:check and exact assertion evidence. Reviewer source checks must assess those claims; this review does not rerun broad aggregates or performance/platform benchmarks. Historical production-before-test exceptions and after-the-fact mutation RED remain disclosed; no source mutation is permitted during review.

## U result

`mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tests::lanes_split_the_memory_ceilings) | test(=config::tests::verification_limits_env_applies_and_refuses) | test(=config::tests::verification_capacity_file_values_resolve_under_env) | test(=verification::tests::production_defaults_are_the_specified_ceilings)'`: exit 0, 4/4 pass (588 skipped), nextest run 6fd388d3-ec54-42eb-9a85-05ab8479d7d1.

Managed Postgres G/O/J command started sequentially in one lifecycle; diagnostic output is temporarily redirected outside the repository to /tmp/wyrd-capacity-r2-runtime.log to preserve any failing trace without overflowing review output. Only summaries/diagnostic evidence belong in this report.

## G/O/J results

The single repository-managed Postgres lifecycle completed with exit 0. It ran `mise run db:migrate:all:inner` then the following exact selectors sequentially:

- G: `cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_invocation_tests::lane_wait_spends_the_agent_call_budget) | test(=components::gateway::pg_invocation_tests::gateway_agent_calls_start_only_on_a_compatible_adapter)'`: 2/2 pass, 590 skipped; lane budget test 3.818 s, compatible adapter 1.579 s. Run cb92d275-3464-44f2-aab8-3ed8d54f19de.
- O: `cargo nextest run --locked -p wyrd-ai -P journey --run-ignored=all -E 'test(=verifier_orchestration::speculative_recovery_cannot_resurrect_content) | test(=verifier_orchestration::speculative_disposition_controls_consumer_admission)'`: 2/2 pass, 169 skipped; recovery 11.923 s, disposition 14.193 s. Run 9a061168-c9e8-46f5-8b7e-cb16db45581e.
- J: `cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::isolated_capacity::direct_verification_runs_while_queued_capacity_is_held)'`: 1/1 pass in 20.290 s, 57 skipped. Run 64a60ec5-85fd-4e52-b131-41571fdeb60a.

Wrapper: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc '<migrations followed by the three commands above with &&>'`. Both repository migration checks pass. All nine freshly selected closure tests pass; no selected test was missing or skipped by selector. Candidate HEAD rechecked unchanged, tracked working tree clean. These results do not prove proposed untested source paths absent; reviewers/validator must distinguish tested acceptance from a separate material failure.

## Focused deadline representation witness

At the independent follow-up's request, ran this read-only expression in the repository-managed Postgres lifecycle (no Cargo or application mutation):

```sql
SELECT statement_timestamp() + (9223372036854775807::bigint * INTERVAL '1 millisecond');
```

`psql -X -v ON_ERROR_STOP=1 -v VERBOSITY=verbose` returned exit 1 as expected: SQLSTATE `22008`, `interval out of range`, at `interval_mul`, `timestamp.c:3412`. This reproduces the failed SQL expression for the accepted signed-max millisecond value. It does not by itself prove startup/runner reachability; the separate source trace in `followup-config-timeout.md` must establish that.

Cumulative `git diff --check 395274d58 ea4460c3c` also exits 0. Full suites, platform/hardware measurement, and mutation testing were not rerun.
