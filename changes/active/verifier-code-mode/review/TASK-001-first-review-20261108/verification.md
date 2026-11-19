# Independent review verification

Reviewed source remains unchanged. Commands run by the orchestrator sequentially against candidate `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`; no production or test edits. These observed results supplement, not replace, acceptance/source tracing.

| Command | Result and evidence |
| --- | --- |
| `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::verification_limits_env_applies_and_refuses) &#124; test(=verification::hosted::sandbox::tests::code_bridge_json_conversion_is_lossless_and_bounded)'` | PASS: 2 selected, 2 passed (run becdf291-692d-4c20-915a-66c731805c5d). |
| `mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::agent_records_survive_failed_steps_and_inconclusive_checks) &#124; test(=blocks_agent::agent_tool_budget_spans_attempts_without_refund) &#124; test(=blocks_agent::code_bridge_shares_the_recorded_path)'` | PASS: 3 selected, 3 passed (run 0173b772-9f21-4369-802b-2d5bf4a427fb). |
| `mise run build:server` | PASS: existing dev worker built and launched, wyrd-server 0.4.1. |

The following exact public-journey command completed through the real repository Postgres wrapper:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::code_mode_reduces_evidence_and_preserves_citations) | test(=verifier_building_blocks::hosted_tools_need_the_tool_and_domain_grants) | test(=verifier_building_blocks::code_mode_failure_releases_session)'
```

PASS: 3 selected, 3 passed; nextest run `df34de3b-b73c-48f4-bee8-ac0670bffae5`, 15.760 seconds. This confirms current assertions pass, not that those assertions satisfy the approved failure contract. No overlapping Cargo commands permitted. No broad gate or fault-injection test edits have been run in this review.


Additional focused wire check:

```sh
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=verification::execution::tests::verification_result_round_trips_documented_outcome)'
```

PASS: 1 selected, 1 passed; run `83e74491-3044-46b2-b0f4-66cba61f4135`.


Focused Busy fixture regression:

```sh
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::busy_tool_ends_the_agent_step_before_a_pending_sibling)'
```

PASS: 1 selected, 1 passed; run `2d815474-0929-4e90-a227-59c75c00c32b`. The recorded diagnosis still requires source validation; passing the present assertion alone does not prove original failure history.

`mise exec -- git diff --check 5234961d51798581a026834cfffd5efaadc226b4...0e51af2a2332f362ab44b7c5b88b51c3cb4f8011` also passed.
