# Fresh review verification

Immutable range: f6fac677cd71208871fa5c21e4035085026a99c4 to 0ae00f4293f3d2dcf636bcbd88a419fe6245c6dd. These executions belong to this review, not the candidate's broader recorded runs. Source was not changed; no fix-reverted mutation or runtime counterexample was added by the review.

## Exact focused tests

All following selectors passed, each selecting one test, using `mise exec -- cargo nextest run --locked -p <package> --lib -E 'test(=<selector>)'`:

| Package | Exact selector |
|---|---|
| vala-verify | blocks::tools::tests::an_awaited_settlement_stays_owed_when_its_waiter_is_dropped |
| vala-verify | blocks::engine::tests::execution_deadline_waits_for_an_attempt_already_settling |
| vala-verify | blocks::engine::tests::discard_during_settlement_waits_for_its_host_work |
| vala-verify | blocks::engine::tests::execution_deadline_during_a_disposition_waits_for_its_host_work |
| vala-verify | blocks::engine::tests::concurrent_dispositions_wait_for_all_held_host_work |
| wyrd-server | verification::state::tests::terminal_decisions_hold_their_copy_charge |
| wyrd-server | components::gateway::snapshots::tests::miss_loads_retire_with_their_last_caller |
| vala-verify | blocks::engine::tests::refused_attempts_keep_their_tool_spend |
| vala-verify | blocks::engine::tests::execution_deadline_waits_for_started_host_work |
| vala-verify | blocks::engine::tests::discarded_speculation_records_after_its_host_work_settles |
| wyrd-runtime | outbox::tests::admitted_items_charge_process_memory_until_written |
| wyrd-runtime | outbox::tests::charged_items_are_taken_over_without_a_second_charge |
| wyrd-server | verification::state::tests::held_changes_coalesce_into_the_next_revision |
| wyrd-server | verification::state::tests::live_state_stays_charged_through_acknowledged_revisions |
| wyrd-server | verification::hosted::workspace::tests::writes_charge_process_scratch |
| wyrd-server | components::gateway::snapshots::tests::snapshots_serve_only_while_listening_and_reload_in_background |
| wyrd-server | components::gateway::snapshots::tests::reloads_that_do_not_install_stay_owed |
| wyrd-server | components::gateway::snapshots::tests::reloads_the_full_queue_cannot_hold_stop_serving |
| wyrd-server | components::gateway::snapshots::tests::snapshots_retain_a_bounded_number_of_tenants |

These exact integration/CLI commands passed, each selecting one test:

```bash
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::bound_media_charges_its_resident_copies)'
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::bound_media_stays_held_until_the_agent_call_ends)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E 'test(=pg_tests::init_keeps_the_gateway_managed_secret_key)'
```

The combined exact production-reader/queued selector below passed both selected tests. Total distinct focused tests in this review: 24 passed. An earlier wrapper session reached test startup but its final output could not be retrieved (Unknown process id); that incomplete observation is not counted as passing proof and justified repeating only these two tests.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_files::evidence_binds_native_media_only) | test(=pg_verification_runtime::deadline_terminal_waits_for_started_host_work)'
```

## Checks, historical discipline and exclusions

`mise run fmt:check`, `git diff --check f6fac677c 0ae00f429`, and both scoped clippy commands passed:

```bash
mise exec -- cargo clippy --locked -p wyrd-runtime -p vala-verify -p wyrd-server -p wyrd-testing --all-features --all-targets -- -D warnings
mise exec -- cargo clippy --locked -p wyrd-server --bin wyrd-server -- -D warnings
```

HEAD remains at the candidate and the prior candidate is its ancestor. Pre-existing local packet changes remain untouched; their tracked binary diff was compared with the initial snapshot.

Candidate-recorded fix-reverted RED results demonstrate mutation sensitivity; the review did not observe pre-production execution and cannot certify historical TDD chronology. No new test/code remedy can recreate that history. Preserve the distinction between behavioral checks and unproven test-first order.

No benchmark, synthetic host load, gate, full journey suite, SDK rebuild, migration replay, broad repository rediscovery or production source edit was performed. Candidate-recorded broader counts remain separate evidence. Allocation ordering, lifetime and media peak bounds are inspected statically; passing tests do not cover every representation bound or certify accounting by their names.
