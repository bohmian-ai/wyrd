# Complete focused proof catalog

All original P01–P31 groups are retained below. These are immutable-input source recipes, not claims of execution or permission to resurrect deleted targets. The original bytes remain in `source/TASK-001-integrate-bifrost-variant-with-main.md`.

Planning inspection found all 133 literal Rust exact selectors in at least one pinned input. Source-only cases require import/relocation into current main targets; names/packages/features must be rediscovered on each cumulative candidate before execution. Main manifest targets were inspected for vala-bifrost-redux integration (test-support), wyrd-testing capability binaries, Rust SDK/client integration, MCP mcp and CLI cli. Test-support is explicitly stated below for the two concurrent-persistence lib cases and the Vala engine integration target. This preserves proof and makes feature intent explicit; no tests were executed during planning.

P18 and P19 intentionally retain duplicate historical recipes for lossless traceability; execute a shared exact command once per appropriate candidate and record both covered mappings. P23 language build commands and P25 installed-binary build are prerequisites. P31 check/list is static discovery; execute actual discovered parsing/dispatch/lifetime selectors before claiming S23 GREEN.

Every named new or relocated test receives its real focused command in implementation evidence; do not invent future private names. Execute only the active task/scenario selectors and affected regressions, not this entire catalog on every edit.

#### P01 — Declared schema/time conformance (source lib owners)

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::layout::tests::time_column_drives_partition_and_sort)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::layout::tests::invalid_time_column_is_refused_naming_the_field)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::execution_lanes::tests::same_iceberg_type_spellings_cast_to_the_registered_type)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::execution_lanes::tests::a_naive_timestamp_is_refused_for_a_zoned_column)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::row_group_bounds_read_the_time_column)'
```

#### P02 — Source cache reset/invalidation

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::cut_cache::tests::cut_cache_serves_only_fresh_listened_cuts)'
```

#### P03 — Source deadline projection and retention configuration

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::request_deadline_projects_unusable_budgets_to_the_default)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=config::tests::max_query_deadline_sits_between_default_deadline_and_retention)'
```

#### P04 — Source group/fence/crash recovery

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::group_commit_syncs_once)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::same_batch_resent_before_commit_lands_in_a_later_group_and_commits_once)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::group_commit_crash_between_sync_and_fence_replays)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::zeroed_block_after_the_newest_fence_ends_the_chain)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::bad_record_before_an_acknowledged_commit_fails_closed)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::bad_record_without_a_fence_authority_fails_closed)'
```

#### P06 — S snapshot retention and native rewrites (S selectors only)

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::snapshot_expiration::retryable_catalog_failure_retains_prepared_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::reader_expiry_ordering::replaced_snapshots_expire_only_after_the_retention_window)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::reader_expiry_ordering::cut_read_completes_while_snapshot_expiry_holds_the_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::standard_variant_layouts_round_trip_per_file)'
```

#### P08 — Role task stop/abort/drop

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=cluster::tests::stop_joins_the_task_once) | test(=cluster::tests::stop_aborts_a_task_past_its_deadline) | test(=cluster::tests::abort_and_drop_end_the_task)'
```

#### P09 — Real membership lifetime and audit drain

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks) | test(=query::oracle_only_pod_retains_audit_staged_before_its_drain)"'
```

#### P10 — SQL role fences and private peer trust

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-sql --test integration -E "test(=pg_oracle_membership::same_node_roles_have_independent_fences) | test(=pg_oracle_membership::stale_role_mutation_is_rejected) | test(=pg_oracle_membership::live_discovery_excludes_expired_heartbeat)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete) | test(=peer_network::security::peer_context_refusals) | test(=peer_network::transport::peer_transport_uses_immutable_fenced_destinations)"'
```

#### P11 — Shared outbox no-Oracle routing, bounded retry and refusal

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=scribe_outbox::tests::a_peer_route_needs_no_oracle) | test(=scribe_outbox::tests::a_retry_resubmits_identical_frames) | test(=scribe_outbox::tests::a_terminal_rejection_never_blocks_later_writes) | test(=scribe_outbox::tests::an_oversized_detail_reaches_scribe_in_bounded_frames_that_replay_identically) | test(=scribe_outbox::tests::an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it) | test(=scribe_outbox::tests::a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe)'
```

#### P12 — Native UID/ACK/audit projection and actual Gate adapter

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::audit::projection::tests::projects_canonical_content_with_the_decision_time) | test(=tables::eval::observations::tests::acknowledged_keys_carry_subject_and_created_at) | test(=scribe::execution_lanes::tests::card_scope_rejects_before_writer_admission) | test(=scribe::execution_lanes::tests::optional_and_scoped_card_correlations_stamp_signed_uids)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::gate_enforces_bifrost_record_write)'
```

#### P13 — Real retained audit and six-destination private validator

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_retention::retained_history_carries_both_credential_shapes) | test(=audit_retention::system_owner_security_rejections_retain_once)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=grpc::capture_peer::tests::requests_are_confined_to_tenant_capture_destinations)'
```

#### P14 — OTLP API-key and UID runtime entrances

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=trace_export::pg_tests::stock_exporter_authenticates_with_api_key_after_token_lifetime)"'

scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::unbound_writer_attributes_spans_to_registered_cards_only) | test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)"'
```

#### P15 — Evaluation/Task/trace/result layout real journeys

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::eval_runs_follow_the_writing_owner) | test(=eval_verification::integrated_enqueue_outage_preserves_ack_and_recovers) | test(=eval_verification::sealed_replay_on_a_later_day_activates_once) | test(=eval_verification::continuous_eval_runs_the_terminal_matrix) | test(=eval_verification::direct_task_judgment_records_one_correlated_result) | test(=eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence) | test(=eval_verification::continuous_eval_refuses_a_trace_over_the_span_ceiling) | test(=verification_runtime::result_layout_partitions_blooms_and_prunes_by_result)"'
```

#### P16 — Verification queue, schedule, lease and judging dependency proof

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::observation_outbox_retains_through_an_outage_and_flushes_at_shutdown) | test(=pg_verification_runtime::schedulers_create_one_run_per_occurrence_across_ticks_and_restart) | test(=pg_verification_runtime::renewal_keeps_a_long_run_and_a_taken_token_cancels_it) | test(=pg_verification_routes::direct_execution_calls_the_judge_provider)"'

scripts/postgres/with-test-postgres.sh -- bash -lc 'WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E "test(=pg_verifier_runs::observation_runs_follow_the_writer) | test(=pg_verifier_runs::concurrent_schedulers_create_one_run_per_occurrence) | test(=pg_verifier_runs::scheduler_skips_inactive_unready_and_missed_occurrences) | test(=pg_verifier_runs::leases_renew_once_a_third_has_passed_and_never_revive) | test(=pg_verifier_runs::reclaimed_lease_fences_the_stale_token)"'
```

#### P17 — Result authoring and schema-by-name

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::results::tests::direct_task_writes_one_summary_row) | test(=verification::results::tests::eval_result_writes_ran_and_skipped_items_then_summary) | test(=verification::results::tests::sampled_out_eval_writes_zero_count_summary_only) | test(=verification::results::tests::reordered_table_fields_keep_values_bound_to_names)'
```

#### P18 — SQL bind forwarding and protected gateway payloads

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::gateway_payload_columns_require_payload_read_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E "test(=components::gateway::pg_invocation_tests::gateway_payload_objects_are_authorized_convergent_and_stable_when_expired)"'
```

#### P19 — Focused first-class Rust query/observation/verification stories

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'
```

#### P20 — Accepted Workflow owner accounting

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=components::workflow::runs::tests::drain_waits_for_a_reservation_before_its_task_exists) | test(=components::workflow::runs::tests::drain_waits_for_blocking_work_its_caller_abandoned)'
```

#### P21 — Real Workflow shutdown and forwarded query settlement

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -P journey -E "test(=pg_workflow_runs::lifecycle_races_retention_and_shutdown)" && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=workflow::workflow_forwarded_query_settles_before_the_run_ends)"'
```

#### P22 — Source logical Variant reads and recovered publication

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::variant_sql_registry_covers_every_session)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::struct_and_variant_share_physical_pushdown)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::pg_hot_variant_filter_on_an_unshredded_key_reads_null)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=recovery::variant_staging_restores_and_publishes_once)'"

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_the_claim_sample_shreds)'
```

#### P23 — Focused Python/TypeScript public stories and imported source capability files

```bash
# Runtime prerequisites once, before the focused tests below.
mise run py:setup
mise run ts:build
mise run ts:build:testing

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_query_bifrost.py::test_bound_sql_text_is_treated_as_data tests/integration/test_query_bifrost.py::test_stream_yields_arrow_batches_and_a_terminal tests/integration/test_observe_a_run.py::test_run_observations_read_back_by_run_id tests/integration/test_observe_a_run.py::test_card_scoped_key_cannot_write_another_cards_observations tests/integration/test_scheduled_drift_alerts_operator.py::test_failed_schedule_alerts_its_operator_on_the_connection_origin tests/integration/test_verify_in_real_time.py::test_exact_verifier_grant_verifies_only_that_verifier'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/query-bifrost.test.ts tests/integration/observe-a-run.test.ts tests/integration/scheduled-drift-alerts-operator.test.ts'

# Source files become available after import; retain their inspected names.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/bifrost/test_variant_tables.py tests/integration/bifrost/test_every_iceberg_column_type.py tests/integration/bifrost/test_register_a_table_from_a_model.py tests/integration/bifrost/test_three_timestamp_types.py tests/integration/bifrost/test_table_time_column.py'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/variant-tables.test.ts tests/integration/every-iceberg-column-type.test.ts tests/integration/register-a-table-from-a-model.test.ts tests/integration/three-timestamp-types.test.ts tests/integration/table-time-column.test.ts'

# Rehomed Rust client cases: discover actual final symbols in existing target.
mise exec -- cargo nextest list --locked -p wyrd-client --features test-support --test integration
```

#### P24 — Changed boot, credentials, signing boundary and platform initialization

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=boot::auth::tests::build_auth_handles_mints_tokens_the_request_verifier_accepts) | test(=boot::auth::tests::build_auth_handles_rejects_an_invalid_signing_key)'
mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=dsn::tests::explicit_serving_dsns_stay_separate_and_derive_the_catalog) | test(=dsn::tests::platform_url_falls_back_to_database_url)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::the_signing_key_file_requires_a_restrictive_regular_bounded_file)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=platform_admin_e2e::an_operator_initializes_the_deployment_through_the_shipped_binary) | test(=platform_admin_e2e::operator_provisions_a_usable_tenant_without_touching_the_database) | test(=platform_admin_e2e::revoking_a_platform_credential_ends_its_live_sessions)"'
```

#### P25 — Installed CLI bootstrap (unmigrated fixture)

```bash
mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready) | test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state) | test(=pg_tests::server_dev_selected_mcp_host_reads)'
```

#### P26 — Installed local callers in owning language runtimes

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[rust]'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[python]'
mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[typescript]'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[interrupt]' 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[exit]'
```

#### P27 — S forward-owner uniqueness (no migration-history test; REQ-024 struck)

```bash
mise exec -- cargo nextest run --locked -p vala-sql --lib -E 'test(=tests::forge_and_oracle_migrations_have_unique_forward_owners)'
```

#### P28 — Actual SQL replay/readiness/RLS/main upgrade owners

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-sql --test integration --test-threads=1 -E 'test(=pg_migration::pg_tests::vala_migrations_apply_and_are_idempotent)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E 'test(=pg_migration::pg_tests::migrations_apply_and_are_idempotent) | test(=pg_migration::pg_tests::single_url_boot_and_separate_url_regression) | test(=pg_migration::pg_tests::tenant_policy_on_another_column_fails_readiness)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-sql --test integration --test-threads=1 -E 'test(=pg_schema_usage::pg_tests::tenant_login_registers_bifrost_table) | test(=pg_schema_usage::pg_tests::fresh_schema_is_tenant_only_and_keys_are_tenant_qualified)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E 'test(=pg_migration::pg_tests::human_connection_upgrade_preflight) | test(=pg_admin_principals::pg_tests::system_principal_upgrade_backfills_existing_tenants_idempotently) | test(=pg_admin_principals::pg_tests::tenant_principal_queries_are_confined_by_row_level_security)'
```

#### P29 — Mounted MCP, compiled CLI and late HTTP terminal

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::agent_investigates_from_multiple_evidence_pivots) | test(=query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution) | test(=query::pg_tests::delegated_agent_query_is_authorized_as_the_subject)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -P journey -E "test(=query_server_journey::query_command_reads_seeded_table) | test(=query_server_journey::query_command_denial_preserves_problem_without_read_decision)"'

mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::routes::tests::http_query_stream_preserves_late_failed_terminal)'
```

#### P30 — Service before-activation preflight and served OpenAPI

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -P journey -E "test(=pg_card_registration_route::service_declared_tables_are_ensured_before_activation)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -P journey -E "test(=pg_openapi_contract::bifrost_operations_publish_typed_problem_refusals) | test(=pg_openapi_contract::every_problem_response_declares_its_media_type_and_stable_code) | test(=pg_openapi_contract::no_operation_documents_an_audit_caused_refusal)"'
```

#### P31 — Consolidated benchmark target and new focused mode proof discovery

```bash
# Static target compilation, not behavioral RED or a benchmark result.
mise exec -- cargo check --locked -p wyrd-testing --bin capacity
# Add focused parsing/dispatch/lifetime cases in this existing owner, then
# discover their actual symbols. Record and run each exact selector before GREEN.
mise exec -- cargo nextest list --locked -p wyrd-testing --bin capacity
# Recorded command shape after discovery (substitute an actual discovered name):
# mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=<actual-full-name>)'
```

## Source selector inspection evidence (static, not execution)

| Original exact selector | Inspected immutable source location(s) |
| --- | --- |
| `catalog::layout::tests::time_column_drives_partition_and_sort` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/catalog/layout.rs:1588:    fn time_column_drives_partition_and_sort() {` |
| `catalog::layout::tests::invalid_time_column_is_refused_naming_the_field` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/shared/wyrd-client/tests/bifrost/table_time_column.rs:170:async fn invalid_time_column_is_refused_naming_the_field() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/catalog/layout.rs:1636:    fn invalid_time_column_is_refused_naming_the_field() {` |
| `scribe::execution_lanes::tests::same_iceberg_type_spellings_cast_to_the_registered_type` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:2582:    fn same_iceberg_type_spellings_cast_to_the_registered_type() {` |
| `scribe::execution_lanes::tests::a_naive_timestamp_is_refused_for_a_zoned_column` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:2643:    fn a_naive_timestamp_is_refused_for_a_zoned_column() {` |
| `scribe::parquet_writer::tests::row_group_bounds_read_the_time_column` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs:2458:    fn row_group_bounds_read_the_time_column() {` |
| `catalog::cut_cache::tests::cut_cache_serves_only_fresh_listened_cuts` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/catalog/cut_cache.rs:251:    fn cut_cache_serves_only_fresh_listened_cuts() {` |
| `oracle::tests::request_deadline_projects_unusable_budgets_to_the_default` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4559:    fn request_deadline_projects_unusable_budgets_to_the_default() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3904:    fn request_deadline_projects_unusable_budgets_to_the_default() {` |
| `config::tests::max_query_deadline_sits_between_default_deadline_and_retention` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/src/config.rs:4170:    fn max_query_deadline_sits_between_default_deadline_and_retention() {` |
| `scribe::shards::tests::group_commit_syncs_once` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/scribe/shards.rs:6910:    async fn group_commit_syncs_once() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/scribe/shards.rs:6990:    async fn group_commit_syncs_once() {` |
| `scribe::shards::tests::same_batch_resent_before_commit_lands_in_a_later_group_and_commits_once` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/scribe/shards.rs:6950:    async fn same_batch_resent_before_commit_lands_in_a_later_group_and_commits_once() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/scribe/shards.rs:7030:    async fn same_batch_resent_before_commit_lands_in_a_later_group_and_commits_once() {` |
| `scribe::shards::tests::group_commit_crash_between_sync_and_fence_replays` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/scribe/shards.rs:7024:    async fn group_commit_crash_between_sync_and_fence_replays() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/scribe/shards.rs:7105:    async fn group_commit_crash_between_sync_and_fence_replays() {` |
| `scribe::pg_scribe_crash_injection::zeroed_block_after_the_newest_fence_ends_the_chain` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/scribe/tests/pg_scribe_crash_injection.rs:363:fn zeroed_block_after_the_newest_fence_ends_the_chain() {` |
| `scribe::pg_scribe_crash_injection::bad_record_before_an_acknowledged_commit_fails_closed` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/scribe/tests/pg_scribe_crash_injection.rs:403:fn bad_record_before_an_acknowledged_commit_fails_closed() {` |
| `scribe::pg_scribe_crash_injection::bad_record_without_a_fence_authority_fails_closed` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/scribe/tests/pg_scribe_crash_injection.rs:422:fn bad_record_without_a_fence_authority_fails_closed() {` |
| `forge::snapshot_expiration::retryable_catalog_failure_retains_prepared_authority` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/tests/integration/forge/snapshot_expiration.rs:500:async fn retryable_catalog_failure_retains_prepared_authority() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/tests/integration/forge/snapshot_expiration.rs:497:async fn retryable_catalog_failure_retains_prepared_authority() {` |
| `forge::reader_expiry_ordering::replaced_snapshots_expire_only_after_the_retention_window` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/tests/integration/forge/reader_expiry_ordering.rs:44:async fn replaced_snapshots_expire_only_after_the_retention_window() {` |
| `forge::reader_expiry_ordering::cut_read_completes_while_snapshot_expiry_holds_the_authority` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/tests/integration/forge/reader_expiry_ordering.rs:128:async fn cut_read_completes_while_snapshot_expiry_holds_the_authority() {` |
| `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs:1473:async fn v3_row_lineage_survives_repeated_rewrite() {` |
| `forge::managed_rewrite::standard_variant_layouts_round_trip_per_file` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs:1950:async fn standard_variant_layouts_round_trip_per_file() {` |
| `cluster::tests::stop_joins_the_task_once` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/cluster/mod.rs:833:    async fn stop_joins_the_task_once() {` |
| `cluster::tests::stop_aborts_a_task_past_its_deadline` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/cluster/mod.rs:859:    async fn stop_aborts_a_task_past_its_deadline() {` |
| `cluster::tests::abort_and_drop_end_the_task` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/cluster/mod.rs:871:    async fn abort_and_drop_end_the_task() {` |
| `owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/owner_inspection.rs:527:async fn unready_roles_heartbeat_until_teardown_ends_membership_tasks()` |
| `query::oracle_only_pod_retains_audit_staged_before_its_drain` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs:764:async fn oracle_only_pod_retains_audit_staged_before_its_drain() -> Result<(), ServerJourneyError> {` |
| `pg_oracle_membership::same_node_roles_have_independent_fences` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-sql/tests/integration/pg_oracle_membership.rs:198:async fn same_node_roles_have_independent_fences() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-sql/tests/pg_oracle_membership.rs:198:async fn same_node_roles_have_independent_fences() {` |
| `pg_oracle_membership::stale_role_mutation_is_rejected` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-sql/tests/integration/pg_oracle_membership.rs:254:async fn stale_role_mutation_is_rejected() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-sql/tests/pg_oracle_membership.rs:254:async fn stale_role_mutation_is_rejected() {` |
| `pg_oracle_membership::live_discovery_excludes_expired_heartbeat` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-sql/tests/integration/pg_oracle_membership.rs:292:async fn live_discovery_excludes_expired_heartbeat() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-sql/tests/pg_oracle_membership.rs:292:async fn live_discovery_excludes_expired_heartbeat() {` |
| `peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/listener.rs:36:async fn peer_listener_is_isolated_mtls_and_role_complete() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/listener.rs:36:async fn peer_listener_is_isolated_mtls_and_role_complete() {` |
| `peer_network::security::peer_context_refusals` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/security.rs:41:async fn peer_context_refusals() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/security.rs:46:async fn peer_context_refusals() {` |
| `peer_network::transport::peer_transport_uses_immutable_fenced_destinations` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/transport.rs:23:async fn peer_transport_uses_immutable_fenced_destinations() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/transport.rs:23:async fn peer_transport_uses_immutable_fenced_destinations() {` |
| `scribe_outbox::tests::a_peer_route_needs_no_oracle` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/scribe_outbox.rs:1204:    async fn a_peer_route_needs_no_oracle() {` |
| `scribe_outbox::tests::a_retry_resubmits_identical_frames` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/scribe_outbox.rs:1126:    async fn a_retry_resubmits_identical_frames() {` |
| `scribe_outbox::tests::a_terminal_rejection_never_blocks_later_writes` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/scribe_outbox.rs:1155:    async fn a_terminal_rejection_never_blocks_later_writes() {` |
| `scribe_outbox::tests::an_oversized_detail_reaches_scribe_in_bounded_frames_that_replay_identically` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/scribe_outbox.rs:1449:    async fn an_oversized_detail_reaches_scribe_in_bounded_frames_that_replay_identically() {` |
| `scribe_outbox::tests::an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/scribe_outbox.rs:1486:    async fn an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it() {` |
| `scribe_outbox::tests::a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/scribe_outbox.rs:1610:    async fn a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe() {` |
| `tables::audit::projection::tests::projects_canonical_content_with_the_decision_time` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs:85:    fn projects_canonical_content_with_the_decision_time() {` |
| `scribe::execution_lanes::tests::card_scope_rejects_before_writer_admission` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:2760:    fn card_scope_rejects_before_writer_admission() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:3277:    fn card_scope_rejects_before_writer_admission() {` |
| `scribe::execution_lanes::tests::optional_and_scoped_card_correlations_stamp_signed_uids` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:4076:    fn optional_and_scoped_card_correlations_stamp_signed_uids() {` |
| `gate::tests::gate_enforces_bifrost_record_write` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/gate/mod.rs:1882:    async fn gate_enforces_bifrost_record_write() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/gate/mod.rs:1848:    async fn gate_enforces_bifrost_record_write() {` |
| `audit_retention::retained_history_carries_both_credential_shapes` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/audit_retention.rs:75:async fn retained_history_carries_both_credential_shapes() -> Result<(), ServerJourneyError> {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:801:async fn retained_history_carries_both_credential_shapes() -> Result<(), ServerJourneyError> {` |
| `audit_retention::system_owner_security_rejections_retain_once` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/audit_retention.rs:145:async fn system_owner_security_rejections_retain_once() -> Result<(), ServerJourneyError> {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:701:async fn system_owner_security_rejections_retain_once() -> Result<(), ServerJourneyError> {` |
| `grpc::capture_peer::tests::requests_are_confined_to_tenant_capture_destinations` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/grpc/capture_peer.rs:177:    fn requests_are_confined_to_tenant_capture_destinations() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/src/grpc/capture_peer.rs:139:    fn requests_are_confined_to_tenant_capture_destinations() {` |
| `trace_export::pg_tests::stock_exporter_authenticates_with_api_key_after_token_lifetime` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/otlp/trace_export.rs:623:    async fn stock_exporter_authenticates_with_api_key_after_token_lifetime() {` |
| `negative::pg_tests::unbound_writer_attributes_spans_to_registered_cards_only` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:498:    async fn unbound_writer_attributes_spans_to_registered_cards_only() {` |
| `negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:406:    async fn identical_exports_from_two_principals_each_keep_their_own_attribution() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:389:    async fn identical_exports_from_two_principals_each_keep_their_own_attribution() {` |
| `eval_verification::eval_runs_follow_the_writing_owner` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:2426:async fn eval_runs_follow_the_writing_owner() -> Result<(), ServerJourneyError> {` |
| `eval_verification::integrated_enqueue_outage_preserves_ack_and_recovers` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:1245:async fn integrated_enqueue_outage_preserves_ack_and_recovers() -> Result<(), ServerJourneyError> {` |
| `eval_verification::sealed_replay_on_a_later_day_activates_once` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:1075:async fn sealed_replay_on_a_later_day_activates_once() -> Result<(), ServerJourneyError> {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:1105:async fn sealed_replay_on_a_later_day_activates_once() -> Result<(), ServerJourneyError> {` |
| `eval_verification::continuous_eval_runs_the_terminal_matrix` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:660:async fn continuous_eval_runs_the_terminal_matrix() -> Result<(), ServerJourneyError> {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:612:async fn continuous_eval_runs_the_terminal_matrix() -> Result<(), ServerJourneyError> {` |
| `eval_verification::direct_task_judgment_records_one_correlated_result` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:2517:async fn direct_task_judgment_records_one_correlated_result() -> Result<(), ServerJourneyError> {` |
| `eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:1662:async fn continuous_eval_reads_ordered_bounded_trace_evidence() -> Result<(), ServerJourneyError> {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:1694:async fn continuous_eval_reads_ordered_bounded_trace_evidence() -> Result<(), ServerJourneyError> {` |
| `eval_verification::continuous_eval_refuses_a_trace_over_the_span_ceiling` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:1799:async fn continuous_eval_refuses_a_trace_over_the_span_ceiling() -> Result<(), ServerJourneyError> {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:1815:async fn continuous_eval_refuses_a_trace_over_the_span_ceiling() -> Result<(), ServerJourneyError> {` |
| `verification_runtime::result_layout_partitions_blooms_and_prunes_by_result` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:538:async fn result_layout_partitions_blooms_and_prunes_by_result() -> Result<(), ServerJourneyError> {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:577:async fn result_layout_partitions_blooms_and_prunes_by_result() -> Result<(), ServerJourneyError> {` |
| `pg_verification_runtime::observation_outbox_retains_through_an_outage_and_flushes_at_shutdown` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs:2406:async fn observation_outbox_retains_through_an_outage_and_flushes_at_shutdown() {` |
| `pg_verification_runtime::schedulers_create_one_run_per_occurrence_across_ticks_and_restart` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs:1637:async fn schedulers_create_one_run_per_occurrence_across_ticks_and_restart() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1635:async fn schedulers_create_one_run_per_occurrence_across_ticks_and_restart() {` |
| `pg_verification_runtime::renewal_keeps_a_long_run_and_a_taken_token_cancels_it` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs:2597:async fn renewal_keeps_a_long_run_and_a_taken_token_cancels_it() {` |
| `pg_verification_routes::direct_execution_calls_the_judge_provider` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs:1356:async fn direct_execution_calls_the_judge_provider() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/tests/pg_verification_routes.rs:1455:async fn direct_execution_calls_the_judge_provider() {` |
| `pg_verifier_runs::observation_runs_follow_the_writer` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/tests/integration/pg_verifier_runs.rs:1948:async fn observation_runs_follow_the_writer() {` |
| `pg_verifier_runs::concurrent_schedulers_create_one_run_per_occurrence` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/tests/integration/pg_verifier_runs.rs:943:async fn concurrent_schedulers_create_one_run_per_occurrence() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-sql/tests/pg_verifier_runs.rs:930:async fn concurrent_schedulers_create_one_run_per_occurrence() {` |
| `pg_verifier_runs::scheduler_skips_inactive_unready_and_missed_occurrences` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/tests/integration/pg_verifier_runs.rs:1050:async fn scheduler_skips_inactive_unready_and_missed_occurrences() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-sql/tests/pg_verifier_runs.rs:1036:async fn scheduler_skips_inactive_unready_and_missed_occurrences() {` |
| `pg_verifier_runs::leases_renew_once_a_third_has_passed_and_never_revive` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/tests/integration/pg_verifier_runs.rs:1128:async fn leases_renew_once_a_third_has_passed_and_never_revive() {` |
| `pg_verifier_runs::reclaimed_lease_fences_the_stale_token` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/tests/integration/pg_verifier_runs.rs:1199:async fn reclaimed_lease_fences_the_stale_token() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-sql/tests/pg_verifier_runs.rs:1097:async fn reclaimed_lease_fences_the_stale_token() {` |
| `verification::results::tests::direct_task_writes_one_summary_row` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/verification/results.rs:1062:    fn direct_task_writes_one_summary_row() {` |
| `verification::results::tests::eval_result_writes_ran_and_skipped_items_then_summary` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/verification/results.rs:970:    fn eval_result_writes_ran_and_skipped_items_then_summary() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/src/verification/results.rs:1044:    fn eval_result_writes_ran_and_skipped_items_then_summary() {` |
| `verification::results::tests::sampled_out_eval_writes_zero_count_summary_only` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/verification/results.rs:1039:    fn sampled_out_eval_writes_zero_count_summary_only() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/src/verification/results.rs:1122:    fn sampled_out_eval_writes_zero_count_summary_only() {` |
| `verification::results::tests::reordered_table_fields_keep_values_bound_to_names` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/verification/results.rs:1165:    fn reordered_table_fields_keep_values_bound_to_names() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/src/verification/results.rs:1181:    fn reordered_table_fields_keep_values_bound_to_names() {` |
| `query_bifrost::parameterized_sql_returns_the_callers_rows` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:84:async fn parameterized_sql_returns_the_callers_rows() {` |
| `query_bifrost::bound_sql_text_is_treated_as_data` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:118:async fn bound_sql_text_is_treated_as_data() {` |
| `query_bifrost::stream_yields_arrow_batches_and_a_terminal` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:161:async fn stream_yields_arrow_batches_and_a_terminal() {` |
| `query_bifrost::caller_without_bifrost_read_is_refused` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:197:async fn caller_without_bifrost_read_is_refused() {` |
| `observe_a_run::run_observations_read_back_by_run_id` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:sdks/wyrd-sdk-rust/tests/integration/observe_a_run.rs:90:async fn run_observations_read_back_by_run_id() {` |
| `observe_a_run::card_scoped_key_cannot_write_another_cards_observations` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:sdks/wyrd-sdk-rust/tests/integration/observe_a_run.rs:181:async fn card_scoped_key_cannot_write_another_cards_observations() {` |
| `observe_a_run::unsealable_byte_budget_is_refused_at_connect` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:sdks/wyrd-sdk-rust/tests/integration/observe_a_run.rs:211:async fn unsealable_byte_budget_is_refused_at_connect() {` |
| `scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:sdks/wyrd-sdk-rust/tests/integration/scheduled_drift_alerts_operator.rs:52:async fn failed_schedule_alerts_its_operator_on_the_connection_origin() {` |
| `verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:500:async fn exact_verifier_grant_verifies_only_that_verifier() {` |
| `verify_in_real_time::another_tenant_cannot_verify_the_assistant` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:531:async fn another_tenant_cannot_verify_the_assistant() {` |
| `bifrost::query::tests::sql_forwards_bind_values_without_interpolation` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/shared/wyrd-client/src/bifrost/query.rs:1912:    async fn sql_forwards_bind_values_without_interpolation() {` |
| `oracle::tests::gateway_payload_columns_require_payload_read_authority` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4680:    async fn gateway_payload_columns_require_payload_read_authority() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4050:    async fn gateway_payload_columns_require_payload_read_authority() {` |
| `components::gateway::pg_invocation_tests::gateway_payload_objects_are_authorized_convergent_and_stable_when_expired` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs:3316:async fn gateway_payload_objects_are_authorized_convergent_and_stable_when_expired() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs:3213:async fn gateway_payload_objects_are_authorized_convergent_and_stable_when_expired() {` |
| `components::workflow::runs::tests::drain_waits_for_a_reservation_before_its_task_exists` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/components/workflow/runs.rs:755:    async fn drain_waits_for_a_reservation_before_its_task_exists() {` |
| `components::workflow::runs::tests::drain_waits_for_blocking_work_its_caller_abandoned` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/components/workflow/runs.rs:804:    async fn drain_waits_for_blocking_work_its_caller_abandoned() {` |
| `pg_workflow_runs::lifecycle_races_retention_and_shutdown` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/pg_workflow_runs.rs:2534:async fn lifecycle_races_retention_and_shutdown() {` |
| `workflow::workflow_forwarded_query_settles_before_the_run_ends` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:101:async fn workflow_forwarded_query_settles_before_the_run_ends() {` |
| `published::variant_sql_registry_covers_every_session` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:839:async fn variant_sql_registry_covers_every_session() {` |
| `published::struct_and_variant_share_physical_pushdown` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:1286:async fn struct_and_variant_share_physical_pushdown() -> Result<(), JourneyError> {` |
| `distributed::pg_hot_variant_filter_on_an_unshredded_key_reads_null` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:133:async fn pg_hot_variant_filter_on_an_unshredded_key_reads_null() {` |
| `recovery::variant_staging_restores_and_publishes_once` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-testing/tests/bifrost/scribe/recovery.rs:145:async fn variant_staging_restores_and_publishes_once() {` |
| `scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_the_claim_sample_shreds` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs:2369:    fn recovery_runs_are_unshredded_and_the_claim_sample_shreds() {` |
| `boot::auth::tests::build_auth_handles_mints_tokens_the_request_verifier_accepts` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/boot/auth.rs:165:    async fn build_auth_handles_mints_tokens_the_request_verifier_accepts() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/src/boot/auth.rs:157:    async fn build_auth_handles_mints_tokens_the_request_verifier_accepts() {` |
| `boot::auth::tests::build_auth_handles_rejects_an_invalid_signing_key` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/boot/auth.rs:207:    async fn build_auth_handles_rejects_an_invalid_signing_key() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/src/boot/auth.rs:199:    async fn build_auth_handles_rejects_an_invalid_signing_key() {` |
| `dsn::tests::explicit_serving_dsns_stay_separate_and_derive_the_catalog` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/src/dsn.rs:157:    fn explicit_serving_dsns_stay_separate_and_derive_the_catalog() {` |
| `dsn::tests::platform_url_falls_back_to_database_url` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/src/dsn.rs:199:    fn platform_url_falls_back_to_database_url() {` |
| `config::tests::the_signing_key_file_requires_a_restrictive_regular_bounded_file` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/config.rs:6123:    fn the_signing_key_file_requires_a_restrictive_regular_bounded_file() {` |
| `platform_admin_e2e::an_operator_initializes_the_deployment_through_the_shipped_binary` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/platform_admin_e2e.rs:244:async fn an_operator_initializes_the_deployment_through_the_shipped_binary() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:230:async fn an_operator_initializes_the_deployment_through_the_shipped_binary() {` |
| `platform_admin_e2e::operator_provisions_a_usable_tenant_without_touching_the_database` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/platform_admin_e2e.rs:131:async fn operator_provisions_a_usable_tenant_without_touching_the_database() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:114:async fn operator_provisions_a_usable_tenant_without_touching_the_database() {` |
| `platform_admin_e2e::revoking_a_platform_credential_ends_its_live_sessions` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/platform_admin_e2e.rs:576:async fn revoking_a_platform_credential_ends_its_live_sessions() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:566:async fn revoking_a_platform_credential_ends_its_live_sessions() {` |
| `pg_tests::server_dev_first_run_is_client_ready` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-cli/tests/pg_tests.rs:281:async fn server_dev_first_run_is_client_ready() {` |
| `pg_tests::server_dev_repeat_and_partial_failure_preserve_state` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-cli/tests/pg_tests.rs:407:async fn server_dev_repeat_and_partial_failure_preserve_state() {` |
| `pg_tests::server_dev_selected_mcp_host_reads` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-cli/tests/pg_tests.rs:582:async fn server_dev_selected_mcp_host_reads() {` |
| `tests::forge_and_oracle_migrations_have_unique_forward_owners` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-sql/src/lib.rs:191:    fn forge_and_oracle_migrations_have_unique_forward_owners() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-sql/src/lib.rs:201:    fn forge_and_oracle_migrations_have_unique_forward_owners() {` |
| `pg_migration::pg_tests::vala_migrations_apply_and_are_idempotent` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-sql/tests/integration/pg_migration.rs:22:    async fn vala_migrations_apply_and_are_idempotent() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-sql/tests/pg_migration.rs:19:    async fn vala_migrations_apply_and_are_idempotent() {` |
| `pg_migration::pg_tests::migrations_apply_and_are_idempotent` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:816:    async fn migrations_apply_and_are_idempotent() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-sql/tests/pg_migration.rs:290:    async fn migrations_apply_and_are_idempotent() {` |
| `pg_migration::pg_tests::single_url_boot_and_separate_url_regression` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:184:    async fn single_url_boot_and_separate_url_regression() {` |
| `pg_migration::pg_tests::tenant_policy_on_another_column_fails_readiness` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:124:    async fn tenant_policy_on_another_column_fails_readiness() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-sql/tests/pg_migration.rs:119:    async fn tenant_policy_on_another_column_fails_readiness() {` |
| `pg_schema_usage::pg_tests::tenant_login_registers_bifrost_table` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-sql/tests/integration/pg_schema_usage.rs:31:    async fn tenant_login_registers_bifrost_table() {` |
| `pg_schema_usage::pg_tests::fresh_schema_is_tenant_only_and_keys_are_tenant_qualified` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/vala/vala-sql/tests/integration/pg_schema_usage.rs:64:    async fn fresh_schema_is_tenant_only_and_keys_are_tenant_qualified() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/vala/vala-sql/tests/pg_schema_usage.rs:69:    async fn fresh_schema_is_tenant_only_and_keys_are_tenant_qualified() {` |
| `pg_migration::pg_tests::human_connection_upgrade_preflight` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:589:    async fn human_connection_upgrade_preflight() {` |
| `pg_admin_principals::pg_tests::system_principal_upgrade_backfills_existing_tenants_idempotently` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/tests/integration/pg_admin_principals.rs:964:    async fn system_principal_upgrade_backfills_existing_tenants_idempotently() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs:964:    async fn system_principal_upgrade_backfills_existing_tenants_idempotently() {` |
| `pg_admin_principals::pg_tests::tenant_principal_queries_are_confined_by_row_level_security` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-sql/tests/integration/pg_admin_principals.rs:659:    async fn tenant_principal_queries_are_confined_by_row_level_security() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs:659:    async fn tenant_principal_queries_are_confined_by_row_level_security() {` |
| `query::pg_tests::agent_investigates_from_multiple_evidence_pivots` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:38:    async fn agent_investigates_from_multiple_evidence_pivots() -> Result<(), McpJourneyError> {` |
| `query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:209:    async fn bifrost_query_errors_are_actionable_before_row_execution()`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:192:    async fn bifrost_query_errors_are_actionable_before_row_execution()` |
| `query::pg_tests::delegated_agent_query_is_authorized_as_the_subject` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:84:    async fn delegated_agent_query_is_authorized_as_the_subject() -> Result<(), McpJourneyError> {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:67:    async fn delegated_agent_query_is_authorized_as_the_subject() -> Result<(), McpJourneyError> {` |
| `query_server_journey::query_command_reads_seeded_table` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-cli/tests/query_server_journey.rs:73:async fn query_command_reads_seeded_table() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-cli/tests/query_server_journey.rs:78:async fn query_command_reads_seeded_table() {` |
| `query_server_journey::query_command_denial_preserves_problem_without_read_decision` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-cli/tests/query_server_journey.rs:133:async fn query_command_denial_preserves_problem_without_read_decision() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-cli/tests/query_server_journey.rs:172:async fn query_command_denial_preserves_problem_without_read_decision() {` |
| `query::routes::tests::http_query_stream_preserves_late_failed_terminal` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/src/query/routes.rs:804:    async fn http_query_stream_preserves_late_failed_terminal() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/src/query/routes.rs:809:    async fn http_query_stream_preserves_late_failed_terminal() {` |
| `pg_card_registration_route::service_declared_tables_are_ensured_before_activation` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs:3935:async fn service_declared_tables_are_ensured_before_activation() {` |
| `pg_openapi_contract::bifrost_operations_publish_typed_problem_refusals` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:706:async fn bifrost_operations_publish_typed_problem_refusals() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:428:async fn bifrost_operations_publish_typed_problem_refusals() {` |
| `pg_openapi_contract::every_problem_response_declares_its_media_type_and_stable_code` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:494:async fn every_problem_response_declares_its_media_type_and_stable_code() {`; `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05:crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:266:async fn every_problem_response_declares_its_media_type_and_stable_code() {` |
| `pg_openapi_contract::no_operation_documents_an_audit_caused_refusal` | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6:crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:671:async fn no_operation_documents_an_audit_caused_refusal() {` |
