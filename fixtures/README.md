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
| `register_and_hydrate` | `register_and_hydrate/support-model.yaml`, `register_and_hydrate/support-desk.yaml` (`cards_get_returns_every_kind_typed` also registers `latency_baseline/latency-baseline.yaml` for its Data); `register_and_hydrate/authored-prompts.yaml` holds the portable, native, and inline-Agent Prompt forms | `prompt_forms_round_trip_as_authored`, `agent_prompt_registration_identity`, `hydrated_agent_prompt_card_navigation`, `service_graph_registers_and_hydrates`, `latest_version_resolves_to_the_registered_card`, `registering_the_graph_again_is_idempotent`, `cards_get_returns_every_kind_typed`, `artifact_without_digest_registers`, `wrong_artifact_digest_is_refused`, `retired_card_kind_is_refused`, `reader_cannot_register_cards`, `cli_apply_and_get_round_trip_the_graph`, `refused_cli_command_raises_its_catalog_code` |
| `workflow_loading` | `workflow_loading/team/security.yaml`, `workflow_loading/team/correctness.yaml`; see the table below | `local_workflow_runs_without_credentials`, `gateway_workflow_without_credentials_is_refused_before_any_step`, `registry_refs_resolve_through_the_registry`, `registry_refs_without_read_access_are_refused`, `local_sibling_never_satisfies_a_registry_ref`, `deleted_registry_card_is_refused`, `applied_workflow_stays_pinned_to_its_registered_cards`, `loaded_workflow_runs_its_pinned_cards`, `loading_a_bad_selector_is_refused`, `text_input_needs_a_declared_input_named_input`, `yaml_workflow_loads_without_resolving_file_targets` |
| `observe_a_run` | `observe_a_run/observed-model.yaml`, `observe_a_run/observed-service.yaml`; the Service binds the Custom drift Verifier `observed-drift.yaml` on a schedule | `run_observations_read_back_by_run_id`, `custom_drift_judges_the_submitted_window_on_schedule`, `run_view_exposes_its_alias`, `card_scoped_key_cannot_write_another_cards_observations`, `unsealable_byte_budget_is_refused_at_connect` |
| `verify_in_real_time` | `latency_baseline/latency-baseline.yaml`, `verify_in_real_time/latency-model.yaml`, `verify_in_real_time/assistant.yaml`; `verify_in_real_time/unfitted-assistant.yaml` binds `tier-drift`, whose baseline never fits | `agent_answer_passes_its_verifier`, `agent_answer_fails_its_verifier`, `judged_answer_passes_the_agent_check`, `judge_call_is_attributed_to_the_run_and_card`, `judge_call_without_a_run_joins_the_callers_trace_attributed_to_the_card`, `traced_sdk_verify_joins_the_callers_trace`, `malformed_traceparent_is_ignored_by_the_judge_call`, `submitted_answer_is_judged_in_the_background_and_resumes_by_id`, `another_tenant_cannot_resume_an_execution`, `window_verifier_records_samples_and_refuses_verify`, `unfitted_baseline_is_judged_inconclusive_on_schedule`, `unbound_verifier_fails_locally`, `input_that_does_not_fit_its_slot_fails_locally`, `caller_without_the_verifier_grant_is_refused`, `exact_verifier_grant_verifies_only_that_verifier`, `another_tenant_cannot_verify_the_assistant` |
| `scheduled_drift_alerts_operator` | create http connection `on-call-hooks` on the local receiver's origin, then `latency_baseline/latency-baseline.yaml`, `scheduled_drift_alerts_operator/latency-watch.yaml` | `failed_schedule_alerts_its_operator_on_the_connection_origin`, `passing_window_alerts_no_one`, `empty_window_is_inconclusive_and_alerts_no_one`, `path_only_operator_without_connection_is_refused`; `ready_spc_baseline_charts_a_scheduled_window` registers `scheduled_drift_alerts_operator/latency-steady.yaml` instead |
| `query_bifrost` | no Cards | `parameterized_sql_returns_the_callers_rows`, `bound_sql_text_is_treated_as_data`, `unwritten_builtin_table_reads_as_empty`, `stream_yields_arrow_batches_and_a_terminal`, `caller_without_bifrost_read_is_refused` |
| `gateway_inference` | `gateway_inference/ask-agent.yaml` with its `ask-prompt.yaml`; `ask-service.yaml` composes that Agent | `openai_client_calls_the_gateway_with_an_access_token`, `caller_without_gateway_invoke_is_refused`, `cli_issues_a_card_scoped_key_and_writes_a_provider_credential` |
| `gateway_jev` | `gateway_jev/reviewer.yaml`, after the `jev/jev-latest` deployment | `native_systemone_answers_relay_intact`, `invalid_jev_questions_are_refused_before_jev`, `chat_completions_pass_typed_jev_requests_through`, `jev_refusals_relay_like_other_providers` |
| `gateway_admin` | no Cards | `cli_written_credential_reads_back_through_the_gateway`, `deployment_round_trips`, `deployment_without_capabilities_is_refused`, `credential_in_use_cannot_be_deleted`, `deleting_a_credential_twice_succeeds`, `capture_policy_round_trips`, `gateway_reader_cannot_delete_a_deployment` |
| `operator_connections` | no Cards | `admin_manages_redacted_connections`, `writer_is_refused`, `reader_cannot_disable_a_connection`, `other_tenant_sees_nothing` |
| `principal_roles` | `observe_a_run/observed-model.yaml`, `observe_a_run/observed-service.yaml`; `gateway_inference/ask-prompt.yaml` is the authored Card | `granted_editor_reaches_the_next_token_until_revoked`, `only_a_tenant_admin_assigns_roles`, `direct_and_idp_user_assignments_coexist` |
| `local_development` | `latency_baseline/latency-baseline.yaml`, `verify_in_real_time/latency-model.yaml`, `verify_in_real_time/assistant.yaml` | `admin_key_completes_the_local_workflow` |
| `verifier_building_blocks` | `verifier_building_blocks/policy-desk.yaml`, which binds `policy-review`, `document-formats`, and `tool-review`; `samples/` holds one tracked sample per extractable format | see the coverage matrix below |
| `signed_in_development` | the `local_development` Cards; alice's login comes from the test server's saved-login fixtures | `saved_login_completes_the_workflow_past_token_expiry` |
| `otel_export` | no Cards | `stock_exporter_span_reads_back_through_bifrost` |
| `saved_user_auth` | `gateway_inference/ask-prompt.yaml`; logins come from the test server's saved-login fixtures | `newest_saved_login_is_used_without_a_selector`, `saved_reader_login_is_denied_a_write`, `stale_login_refreshes_and_saves_the_renewal`, `revoked_login_is_refused`, `selector_naming_no_saved_login_is_refused`, `explicit_key_beside_a_tenant_selector_is_refused` |

## Verifier coverage matrix

Each row is one Verifier building block or flow, the fixture that declares
it, and the Rust test that proves it; Python and TypeScript mirror the test
name.

| Row | Fixture | Rust test |
|---|---|---|
| `extract` action over a PDF, step `assert` over its pages | `verifier_building_blocks/policy-review.yaml` (`policy-text`, `policy-has-text`) | `policy_review_runs_every_action_and_check_kind` |
| `agent` action with `fs.read` over a workspace file, tool calls recorded | `policy-review.yaml` (`refund-rules`), `refund-reader.yaml` | `policy_review_runs_every_action_and_check_kind` |
| `code` action and `code` check | `policy-review.yaml` (`answer-facts`, `names-the-policy`) | `policy_review_runs_every_action_and_check_kind` |
| `trace` check over a trace slot: first span, span order, and span attribute | `policy-review.yaml` (`policy-looked-up`, `answer-drafted-after-lookup`, `lookup-read-refunds`) | `policy_review_runs_every_action_and_check_kind` |
| Tool-less `agent` check with cited verdict | `policy-review.yaml` (`follows-policy`), `policy-judge.yaml` | `policy_review_runs_every_action_and_check_kind` |
| `assert` fails on a value over the limit | `policy-review.yaml` (`refund-within-limit`) | `refund_over_the_limit_fails_the_assertion` |
| Judge verdict fails | `policy-judge.yaml` | `judge_verdict_fails_the_agent_check` |
| Uncited judge verdict never passes | `policy-judge.yaml` | `uncited_judge_verdict_cannot_pass` |
| Raising code check never passes | `policy-review.yaml` (`names-the-policy`) | `raising_code_check_cannot_pass` |
| Forbidden code access fails its step and skips dependent work | `policy-review.yaml` (`answer-facts`) | `forbidden_code_access_cannot_pass_dependent_work` |
| Trace check fails without its span | `policy-review.yaml` (`policy-looked-up`) | `trace_check_fails_a_guess_and_needs_its_trace` |
| Trace check fails on reversed span order and on another attribute value | `policy-review.yaml` (`answer-drafted-after-lookup`, `lookup-read-refunds`) | `trace_check_judges_span_order_and_attributes` |
| `agent` action provider failure, output-schema refusal, and deadline fail the step, skip its dependent step, and leave dependents inconclusive | `policy-review.yaml` (`refund-rules`, `timeout_seconds`), `refund-reader.yaml` | `failed_reader_leaves_its_dependents_inconclusive` |
| `code` action raise and deadline fail the step and leave its assertion inconclusive | `policy-review.yaml` (`answer-facts`, `timeout_seconds`) | `failed_code_step_leaves_its_assertion_inconclusive` |
| Malformed judge Verdict and invalid original-evidence citation are inconclusive | `policy-judge.yaml` | `invalid_judge_verdicts_are_inconclusive` |
| `code` check valid failed cited Verdict; malformed Verdict, foreign citation, forbidden access, and deadline inconclusive | `policy-review.yaml` (`names-the-policy`) | `code_check_fails_with_citations_and_is_inconclusive_otherwise` |
| Hosted Agent `cards.get_by_ref`, `bifrost.query`, and `code.run` need the tool grant and the domain grant; an exact tool grant covers only its tool; denials return nothing | `tool-review.yaml`, `investigator.yaml` | `hosted_tools_need_the_tool_and_domain_grants` |
| A delegate narrows the subject's tool authority and the result keeps subject and actor | `tool-review.yaml`, `investigator.yaml` | `delegated_tools_keep_the_narrower_authority` |
| Missing media input is inconclusive | `policy-review.yaml` (`policy` slot) | `missing_policy_is_inconclusive` |
| Malformed media never passes | `policy-review.yaml` (`policy` slot) | `malformed_policy_cannot_pass` |
| Oversized media is refused locally | `policy-review.yaml` (`max_bytes`) | `oversized_policy_is_refused_locally` |
| Background `submit` resumes by execution ID | `policy-review.yaml` | `submitted_review_is_judged_in_the_background` |
| Every extractable format's locked output | `document-formats.yaml`, `samples/sample.*` | `every_format_extracts_its_locked_output` |
| TypeScript positional input: a lone File fills the one slot; a multi-slot Verifier refuses a positional value; an object marked `{ positional: true }` is one value | `document-formats.yaml`, `policy-review.yaml`, `verify_in_real_time/answer-check.yaml` | TypeScript only: `document passed positionally fills the only slot`, `positional value for a multi slot verifier is refused locally`, `positional object is the one answer, not its slots` |
| Immediate `verify` with an `assert` | `verify_in_real_time/answer-check.yaml` | `agent_answer_passes_its_verifier`, `agent_answer_fails_its_verifier` |
| Immediate `verify` with an `agent` judge, attributed to Run and trace | `verify_in_real_time/answer-judged.yaml`, `judge-agent.yaml`, `judge-prompt.yaml` | `judged_answer_passes_the_agent_check`, `judge_call_is_attributed_to_the_run_and_card`, `traced_sdk_verify_joins_the_callers_trace` |
| `submit` on a manual binding queues; another tenant cannot resume | `verify_in_real_time/answer-check.yaml` | `submitted_answer_is_judged_in_the_background_and_resumes_by_id`, `another_tenant_cannot_resume_an_execution` |
| Window Verifier records samples and refuses `verify` | `verify_in_real_time/latency-drift.yaml` | `window_verifier_records_samples_and_refuses_verify` |
| Unfitted baseline is inconclusive on schedule | `verify_in_real_time/tier-drift.yaml`, `unfitted-assistant.yaml` | `unfitted_baseline_is_judged_inconclusive_on_schedule` |
| Grants: missing, exact, other tenant | `verify_in_real_time/assistant.yaml` | `caller_without_the_verifier_grant_is_refused`, `exact_verifier_grant_verifies_only_that_verifier`, `another_tenant_cannot_verify_the_assistant` |
| PSI drift on schedule fails and alerts once | `scheduled_drift_alerts_operator/latency-shift.yaml`, `latency-watch.yaml` | `failed_schedule_alerts_its_operator_on_the_connection_origin` |
| Passing and empty windows alert no one | `scheduled_drift_alerts_operator/latency-watch.yaml` | `passing_window_alerts_no_one`, `empty_window_is_inconclusive_and_alerts_no_one` |
| Ready SPC baseline charts a scheduled window with its X-bar/S evidence | `scheduled_drift_alerts_operator/latency-control.yaml`, `latency-steady.yaml` | `ready_spc_baseline_charts_a_scheduled_window` |
| Custom drift on schedule | `observe_a_run/observed-drift.yaml` | `custom_drift_judges_the_submitted_window_on_schedule` |

The scheduled journeys follow one sequence: `submit` returns `Recorded`, no
Operator fires, the binding comes due, the failed result reads back by its
execution ID, and the Operator delivers exactly once.

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

Refusals load one deliberately broken Card from `invalid/`:

| File | Refused with |
|---|---|
| `invalid/wrong-artifact-digest/support-model.yaml` | `WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH` |
| `invalid/operator-path-without-connection.yaml` | `WYRD_SPEC_400_INVALID_OPERATOR` |
| `invalid/retired-drift-kind.yaml` | `WYRD_LOADER_400_INVALID_ENVELOPE` (registration and `wyrd apply` alike) |
| `invalid/prompt-with-removed-fields.yaml` | `WYRD_LOADER_400_INVALID_ENVELOPE` |

Names are fixed, so registering a fixture again is idempotent. Add a story
only when a journey needs Cards none of these provide.

Offline inputs that only Python unit tests load live with them in
`sdks/wyrd-sdk-python/tests/fixtures/`: `bundles/` holds downloaded Service
bundles that `WyrdState` hydrates without a server (`builtin-model/` holds only
the files that differ from `complete/`), `authoring/` holds Cards loaded
through `from_path`, and `invalid/` holds one broken input per local refusal.

## MCP investigation

The MCP evidence journey reuses `register_and_hydrate` Model, Agent, Prompt,
and assertion Verifier Cards. Its additional fixtures live in `cards/mcp_investigation`:

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
