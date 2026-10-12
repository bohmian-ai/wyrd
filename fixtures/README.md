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
| `register_and_hydrate` | `register_and_hydrate/support-model.yaml`, `register_and_hydrate/support-desk.yaml` (`cards_get_returns_every_kind_typed` also registers `latency_baseline/latency-baseline.yaml` for its Data) | `service_graph_registers_and_hydrates`, `latest_version_resolves_to_the_registered_card`, `registering_the_graph_again_is_idempotent`, `cards_get_returns_every_kind_typed`, `artifact_without_digest_registers`, `wrong_artifact_digest_is_refused`, `retired_card_kind_is_refused`, `reader_cannot_register_cards`, `cli_apply_and_get_round_trip_the_graph`, `refused_cli_command_raises_its_catalog_code` |
| `workflow_loading` | `workflow_loading/team/security.yaml`, `workflow_loading/team/correctness.yaml`; see the table below | `local_workflow_runs_without_credentials`, `gateway_workflow_without_credentials_is_refused_before_any_step`, `registry_refs_resolve_through_the_registry`, `registry_refs_without_read_access_are_refused`, `local_sibling_never_satisfies_a_registry_ref`, `deleted_registry_card_is_refused`, `applied_workflow_stays_pinned_to_its_registered_cards`, `loaded_workflow_runs_its_pinned_cards`, `loading_a_bad_selector_is_refused`, `text_input_needs_a_declared_input_named_input`, `yaml_workflow_loads_without_resolving_file_targets` |
| `observe_a_run` | `observe_a_run/observed-model.yaml`, `observe_a_run/observed-service.yaml` | `run_observations_read_back_by_run_id`, `run_view_exposes_its_alias`, `card_scoped_key_cannot_write_another_cards_observations`, `unsealable_byte_budget_is_refused_at_connect` |
| `verify_in_real_time` | `latency_baseline/latency-baseline.yaml`, `verify_in_real_time/latency-model.yaml`, `verify_in_real_time/assistant.yaml`; `verify_in_real_time/unfitted-assistant.yaml` binds `tier-drift`, whose baseline never fits | `agent_answer_passes_its_verifier`, `agent_answer_fails_its_verifier`, `judged_answer_passes_the_llm_judge`, `model_latency_like_the_baseline_passes_its_verifier`, `model_latency_drift_is_judged_failed`, `verify_before_baseline_ready_is_refused`, `unbound_verifier_fails_locally`, `input_of_the_wrong_shape_fails_locally`, `caller_without_evals_run_is_refused`, `another_tenant_cannot_verify_the_assistant`, `verify_records_no_observation` |
| `scheduled_drift_alerts_operator` | create http connection `on-call-hooks` on the local receiver's origin, then `latency_baseline/latency-baseline.yaml`, `scheduled_drift_alerts_operator/latency-watch.yaml` | `failed_schedule_alerts_its_operator_on_the_connection_origin`, `path_only_operator_without_connection_is_refused` |
| `query_bifrost` | no Cards | `parameterized_sql_returns_the_callers_rows`, `bound_sql_text_is_treated_as_data`, `unwritten_builtin_table_reads_as_empty`, `stream_yields_arrow_batches_and_a_terminal`, `caller_without_bifrost_read_is_refused` |
| `gateway_inference` | `gateway_inference/ask.yaml`; `ask-external.yaml` names the registered `ask-agent`; the example Workflow is `examples/workflows/code-review` | `openai_client_calls_the_gateway_with_an_access_token`, `caller_without_gateway_invoke_is_refused`, `cli_issues_a_card_scoped_key_and_writes_a_provider_credential`, `loaded_workflow_calls_the_gateway_through_its_loading_client`, `example_workflow_runs_through_the_wyrd_gateway`, `applying_a_workflow_calls_no_model`, `registered_example_runs_through_the_gateway` |
| `gateway_admin` | no Cards | `cli_written_credential_reads_back_through_the_gateway`, `deployment_round_trips`, `deployment_without_capabilities_is_refused`, `credential_in_use_cannot_be_deleted`, `deleting_a_credential_twice_succeeds`, `capture_policy_round_trips`, `gateway_reader_cannot_delete_a_deployment` |
| `operator_connections` | no Cards | `admin_manages_redacted_connections`, `writer_is_refused`, `reader_cannot_disable_a_connection`, `other_tenant_sees_nothing` |
| `principal_roles` | `observe_a_run/observed-model.yaml`, `observe_a_run/observed-service.yaml`; `gateway_inference/ask-prompt.yaml` is the authored Card | `granted_editor_reaches_the_next_token_until_revoked`, `only_a_tenant_admin_assigns_roles`, `direct_and_idp_user_assignments_coexist` |
| `local_development` | `latency_baseline/latency-baseline.yaml`, `verify_in_real_time/latency-model.yaml`, `verify_in_real_time/assistant.yaml` | `admin_key_completes_the_local_workflow` |
| `signed_in_development` | the `local_development` Cards; alice's login comes from the test server's saved-login fixtures | `saved_login_completes_the_workflow_past_token_expiry` |
| `otel_export` | no Cards | `stock_exporter_span_reads_back_through_bifrost` |
| `saved_user_auth` | `gateway_inference/ask-prompt.yaml`; logins come from the test server's saved-login fixtures | `newest_saved_login_is_used_without_a_selector`, `saved_reader_login_is_denied_a_write`, `stale_login_refreshes_and_saves_the_renewal`, `revoked_login_is_refused`, `selector_naming_no_saved_login_is_refused`, `explicit_key_beside_a_tenant_selector_is_refused` |

Each test asserts the same outcome, from the same input, in every SDK; a
refusal asserts the same single catalog code. The Rust
`otel_export::stock_exporter_span_reads_back_through_bifrost` is the one test
about ambient configuration, so it alone runs in a child process. A test that
acts as a principal other than the deployment's passes that principal's
`WyrdClient` as `client`.

A journey that exercises a language's own surface (the installed `wyrd`
executable, Python framework integrations and async iterators, OTel log and
metric signals, Arrow conversions) lives outside these story files and is
not mirrored.

`workflow_loading/` Cards live in the `workflow-loading` space and are
derived from `examples/workflows/code-review`. Their Prompts send the
example's Chat messages to the built-in `mock` provider, which answers with
the rendered user message, so a run's outputs show which Prompt body ran.

| Directory | What it is | What the journeys prove with it |
|---|---|---|
| `team/` | `security-reviewer` and `correctness-reviewer` Agents with their Prompts (`registered ... review`), registered first | The registered Cards that other fixtures reference |
| `team-v2/` | `security-reviewer@2.0.0`, whose Prompt answers `v2 security review` | A newer version never floats into a pinned Workflow |
| `mixed/` | Workflow `code-review`: security and correctness are registry refs, the final reviewer is a local file | Authored refs resolve through the registry; this file is applied, reloaded, and run |
| `shadowed/workflow.yaml` | Workflow `shadowed-review`: a local `security-reviewer@1.0.0` plus a step referencing the registered `security-reviewer@1.0.0` | A local sibling never satisfies a registry ref with the same identity |
| `shadowed/local-workflow.yaml` | Workflow `local-review`: the same directory's three local Agents only | A wholly local Workflow loads and runs with no server |
| `retired/` | Workflow `retired-review`, whose local security Agent references Prompt `retired-prompt@1.0.0`, which the journey registers and then deletes | A deleted registry Card is refused |

Refusals load one deliberately broken Card from `invalid/`:

| File | Refused with |
|---|---|
| `invalid/wrong-artifact-digest/support-model.yaml` | `WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH` |
| `invalid/operator-path-without-connection.yaml` | `WYRD_SPEC_400_INVALID_OPERATOR` |
| `invalid/retired-drift-kind.yaml` | `WYRD_LOADER_400_INVALID_ENVELOPE` (registration and `wyrd apply` alike) |

Names are fixed, so registering a fixture again is idempotent. Add a story
only when a journey needs Cards none of these provide.

Offline inputs that only Python unit tests load live with them in
`sdks/wyrd-sdk-python/tests/fixtures/`: `bundles/` holds downloaded Service
bundles that `WyrdState` hydrates without a server (`builtin-model/` holds only
the files that differ from `complete/`), `authoring/` holds Cards loaded
through `from_path`, and `invalid/` holds one broken input per local refusal.

## MCP investigation

The MCP evidence journey reuses `register_and_hydrate` Model, Agent, Prompt,
and Eval Verifier Cards. Its additional fixtures live in `cards/mcp_investigation`:

| File | Purpose |
| --- | --- |
| `service.yaml` | Exact shared component references and the declared `investigation_events` dataset |
| `drift-verifier.yaml` | Agent-readable Custom Drift Verifier matching the seeded feature report |
| `drift-report.json` | Native retained Drift report with a failed latency feature |
| `eval-assertion.json` | Native failed refund-promise assertion item |

The journey registers Cards through the shared client, writes custom rows
through Bifrost, and publishes fixture reports through the existing native
result mapper and Scribe outbox. It reads the evidence through the real MCP
endpoint; it does not execute a live model.
