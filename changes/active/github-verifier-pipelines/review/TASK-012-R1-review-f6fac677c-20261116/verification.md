# Review execution evidence

Subject: 9016393004dc0eb319075245c5f6a1d7a6c2cd46 to f6fac677cd71208871fa5c21e4035085026a99c4. These are fresh review executions, distinct from committed implementation evidence. Cargo commands ran sequentially. Each successful named selector selected at least one test.

## Focused tests

Every row below passed using `mise exec -- cargo nextest run --locked -p <package> --lib -E 'test(=<selector>)'`, except the integration and PostgreSQL commands recorded separately.

| Package | Exact selector |
|---|---|
| vala-verify | blocks::engine::tests::refused_attempts_keep_their_tool_spend |
| wyrd-runtime | outbox::tests::admitted_items_charge_process_memory_until_written |
| wyrd-server | verification::state::tests::held_changes_coalesce_into_the_next_revision |
| wyrd-server | verification::hosted::workspace::tests::writes_charge_process_scratch |
| vala-verify | blocks::engine::tests::execution_deadline_waits_for_started_host_work |
| vala-verify | blocks::engine::tests::discarded_speculation_records_after_its_host_work_settles |
| wyrd-runtime | outbox::tests::charged_items_are_taken_over_without_a_second_charge |
| wyrd-server | verification::state::tests::live_state_stays_charged_through_acknowledged_revisions |
| wyrd-server | verification::state::tests::publication_reads_hold_their_copy_charge |
| wyrd-server | verification::hosted::workspace::tests::extraction_charges_process_memory_and_scratch |
| wyrd-server | verification::hosted::workspace::tests::surviving_workspace_keeps_its_scratch_charge |
| wyrd-server | verification::hosted::workspace::tests::abandoned_cleanup_keeps_its_scratch_charge |

The following exact integration commands passed:

```bash
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::bound_media_stays_held_until_the_agent_call_ends)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_verification_runtime::deadline_terminal_waits_for_started_host_work)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::workspace::pg_tests::cancelled_open_returns_its_charges) | test(=verification::hosted::workspace::pg_tests::workspace_cleans_up_on_drop)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=sandbox_worker::sandbox_contains_runaway_code)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E 'test(=pg_tests::init_keeps_the_gateway_managed_secret_key)'
```

Total: 18 focused tests passed. The two workspace cleanup selectors initially used `workspace::tests` instead of `workspace::pg_tests`, selected zero tests and exited 4. Those attempts are not proof; the corrected exact selectors above passed under the required PostgreSQL wrapper.

## Checks and limits

`mise run fmt:check` and `git diff --check 901639300 f6fac677c` passed. Ancestry check confirms the prior candidate is an ancestor; HEAD remained the candidate.

The first scoped all-feature/all-target clippy attempt could not create temporary metadata directories because the workspace filesystem had no space (OS error 28). Only regenerable `target/debug/incremental/wyrd_testing-*` build caches were removed, returning about 13 GiB; no source or user files were removed. The initial `mise exec -- python` cleanup invocation failed because only python3 is available; the python3 invocation succeeded. The retried scoped check and the separate binary check both passed:

```bash
mise exec -- cargo clippy --locked -p wyrd-runtime -p vala-verify -p wyrd-server -p wyrd-testing --all-features --all-targets -- -D warnings
mise exec -- cargo clippy --locked -p wyrd-server --bin wyrd-server -- -D warnings
```

No source mutation, new runtime repro test, benchmark, synthetic host load, gate or full journey suite was performed. The committed R1 Evidence section records the broader implementation test and lint results; those counts are not claimed as fresh review executions. Passing selected tests proves their assertions, not closure of paths they do not exercise. Allocation ordering is inspected statically. G2's host-load-only RED remains unavailable by explicit exclusion.
