# TASK-002 R4 candidate-bound verification

All commands ran against candidate `2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`
with only the new R4 review directory added to the worktree. Repository-managed
Postgres wrappers provisioned and removed their own local environments. No live
provider credential was used.

| Command / evidence | Result |
|---|---|
| `git rev-parse HEAD` | PASS — exact candidate |
| `git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c 2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c` | PASS |
| `mise run check:workspace-hack` | PASS |
| `mise run fmt:check` | PASS |
| `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=workflow::tests::from_path_uses_existing_loader)'` | PASS — 1 selected / 1 passed |
| `mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=tests::load_explicit_workflow_bundle)'` | PASS — 1 / 1 |
| managed Postgres `pg_workflow_registration::registers_only_valid_explicit_workflow_graphs` | PASS — 1 / 1 |
| managed Postgres `pg_workflow_registration::refuses_stale_preflight_after_dependency_replacement` | PASS — 1 / 1 |
| managed Postgres `pg_workflow_registration::fetches_and_executes_locked_workflow_graph` | PASS — 1 / 1 |
| managed Postgres `pg_cards_register::relationship_recheck_blocks_target_lifecycle_race` | PASS — 1 / 1 |
| managed Postgres Rust SDK `workflow_loading_journey` with `--run-ignored all` | PASS — 1 / 1 |
| managed Postgres Python integration `test_workflow_loading_journey` after `mise run py:setup` | PASS — 1 / 1 (13 deselected) |
| managed Postgres TypeScript `^Workflow loading workflow loading journey$` after native/testing builds | PASS — 1 / 1 |
| Python `test_registry_selector_errors_use_request_validation` | PASS — 36 / 36 |
| Python `test_user_metadata_rejects_invalid_reserved_and_secret_values` | PASS — 1 / 1 |
| `mise run codegen:check` | PASS |
| `mise run check:client-tier` | PASS |
| `mise run check:sdk-client-tier` | PASS |
| `mise run check:pyo3-scope` | PASS |
| `mise run check:registry-tx-coupling` | PASS |
| `mise run py:typecheck` | PASS |
| `mise run ts:typecheck` | PASS |
| `mise run ts:napi:check` | PASS |

These focused runs directly cover the R2/R3 remediation seams and final
generated/boundary parity. They do not substitute for source review and do not
independently rerun every broader aggregate claimed in the task evidence.
