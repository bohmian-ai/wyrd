# Review verification

Candidate: 37b3c2f96b1bd425bf1f0923a1950d0363c40907. Orchestrator runs Cargo sequentially through mise. No source mutation or new test harness is used.

| Exact command | Observed result |
|---|---|
| `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::core_primitives_match_canonical_contracts) \| test(=authoring::tests::offline_edits_preserve_snapshots) \| test(=authoring::tests::replacements_keep_only_current_dependencies) \| test(=authoring::tests::declarations_match_file_authored_tree) \| test(=authoring::tests::field_edits_defer_validation_and_preserve_snapshots) \| test(=authoring::tests::yaml_and_code_share_diagnostics)'` | 6 passed; c1cf2595-105c-4ebb-9c8d-af8dc2da8be8 |
| `mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -E 'test(=authoring::public_paths_author_every_native_family)'` | 1 passed; 290da148-4538-4ca0-89a9-e38c3aa8936f |
| `mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::independent_actions_overlap_within_the_concurrency_bound) \| test(=blocks::engine::tests::supplied_null_follows_the_slot_schema_not_requiredness) \| test(=blocks::engine::tests::missing_and_optional_evidence_resolve_honestly)'` | 3 passed; 93d07a8b-f783-4acc-9db0-002c13dd0b73 |

| `mise run build:server` | passed, wyrd-server 0.4.1 |
| `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::multi_component_service_verifies_each_subject) \| test(=authoring::local_artifacts_reuse_and_keep_versions) \| test(=authoring::declarations_match_files_and_keep_refusals) \| test(=authoring::dependent_steps_consume_prior_outputs) \| test(=declare_and_pull::declares_ready_verification_state)' && cargo nextest run --locked -p wyrd-rust-examples --test finance_release -P journey --run-ignored=all -E 'test(=finance_release_passes_and_fails)'"` | Both migration tests passed. Five SDK tests passed (86c675eb-9b4b-4678-a33f-83942aa00932); finance binary journey passed (55fcdddd-2cb0-40ab-b6e2-315cc6017b09). |
| `mise exec -- cargo doc --locked -p wyrd-client -p wyrd-loader -p wyrd-ai --no-deps` | passed without warnings |
| `mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-loader -p wyrd-ai -p wyrd-rust-examples --all-targets -- -D warnings` | passed |
| `mise run fmt:check` | passed |
| `mise exec -- git diff --check a387f2a47 37b3c2f96` | clean |

Sixteen focused behavioral tests passed, plus both migration tests. SDK/server/example linking emitted the previously observed __eh_frame compact-unwind-size warning; builds and selected tests passed. Task-appended broader execution/mutation history is historical evidence, distinct from these fresh observations. No fresh full workspace check, dependency gate, cross-platform run, live-provider quality test or fault injection was performed. Green baseline assertions do not settle proposed untested paths or mandatory private-item documentation.
