# Shared SDK fixtures

Checked-in Cards that the Rust, Python, and TypeScript journeys all load
(`TESTING.md`, client-facing checklist). Register a file through the public
SDK or CLI; never write or edit YAML in a test. A Card with an artifact is
registered alone, before the graph that references it.

A story uses the same file and test names in every SDK: `<story>.rs`,
`test_<story>.py`, `<story-with-dashes>.test.ts`; test `outcome_name` is
`test_outcome_name` in Python and `it("outcome name")` in TypeScript.

| Story | Register, in order | Tests |
|---|---|---|
| `register_and_hydrate` | `register_and_hydrate/support-model.yaml`, `register_and_hydrate/support-desk.yaml` (`cards_get_returns_every_kind_typed` also registers `latency_baseline/latency-baseline.yaml` for its Data) | `service_graph_registers_and_hydrates`, `registering_the_graph_again_is_idempotent`, `cards_get_returns_every_kind_typed`, `artifact_without_digest_registers`, `wrong_artifact_digest_is_refused`, `retired_card_kind_is_refused`, `reader_cannot_register_cards`, `cli_apply_and_get_round_trip_the_graph`, `refused_cli_command_raises_its_catalog_code` |
| `observe_a_run` | `observe_a_run/observed-model.yaml`, `observe_a_run/observed-service.yaml` | `run_observations_read_back_by_run_id`, `run_view_exposes_its_alias`, `card_scoped_key_cannot_write_another_cards_observations` |
| `verify_in_real_time` | `latency_baseline/latency-baseline.yaml`, `verify_in_real_time/latency-model.yaml`, `verify_in_real_time/assistant.yaml`; `verify_in_real_time/unfitted-assistant.yaml` binds `tier-drift`, whose baseline never fits | `agent_answer_passes_its_verifier`, `agent_answer_fails_its_verifier`, `judged_answer_passes_the_llm_judge`, `model_latency_drift_is_judged_failed`, `verify_before_baseline_ready_is_refused`, `unbound_verifier_fails_locally`, `caller_without_evals_run_is_refused`, `verify_records_no_observation` |
| `scheduled_drift_alerts_operator` | create http connection `on-call-hooks` on the local receiver's origin, then `latency_baseline/latency-baseline.yaml`, `scheduled_drift_alerts_operator/latency-watch.yaml` | `failed_schedule_alerts_its_operator_on_the_connection_origin`, `path_only_operator_without_connection_is_refused` |
| `query_bifrost` | no Cards | `parameterized_sql_returns_the_callers_rows`, `bound_sql_text_is_treated_as_data`, `unwritten_builtin_table_reads_as_empty` |
| `gateway_inference` | `gateway_inference/ask.yaml`; `ask-external.yaml` names the registered `ask-agent` | `openai_client_calls_the_gateway_with_an_access_token`, `cli_issues_a_card_scoped_key_and_writes_a_provider_credential`, `loaded_workflow_calls_the_gateway_through_its_loading_client` |
| `operator_connections` | no Cards | `admin_manages_redacted_connections`, `writer_is_refused`, `other_tenant_sees_nothing` |
| `otel_export` | no Cards | `stock_exporter_span_reads_back_through_bifrost` |

Refusals load one deliberately broken Card from `invalid/`:

| File | Refused with |
|---|---|
| `invalid/wrong-artifact-digest/support-model.yaml` | `WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH` |
| `invalid/operator-path-without-connection.yaml` | `WYRD_SPEC_400_INVALID_OPERATOR` |
| `invalid/retired-drift-kind.yaml` | `WYRD_LOADER_400_INVALID_ENVELOPE` |

Names are fixed, so registering a fixture again is idempotent. Add a story
only when a journey needs Cards none of these provide.
