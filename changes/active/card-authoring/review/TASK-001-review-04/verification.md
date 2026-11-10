# Focused R4 review verification

Candidate: 9be515a10607ba7d930594b1c314757ee05a0424. Orchestrator runs Cargo commands sequentially through mise; no source mutation, deliberate reverted fix or new harness during review.

| Exact command | Observed result |
|---|---|
| `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::offline_edits_preserve_snapshots) \| test(=authoring::tests::replacements_keep_only_current_dependencies) \| test(=authoring::tests::field_edits_defer_validation_and_preserve_snapshots) \| test(=authoring::tests::yaml_and_code_share_diagnostics) \| test(=authoring::tests::core_primitives_match_canonical_contracts) \| test(=authoring::tests::declarations_match_file_authored_tree)'` | 6 passed; e0213a12-c720-4ea9-90c8-e241d85ce42d |
| `mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -E 'test(=authoring::public_paths_author_every_native_family)'` | 1 passed; 8882df40-eb7e-42b7-8179-837f03c9d1b9 |
| `mise run build:server` | passed, wyrd-server 0.4.1 |

| `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::local_artifacts_reuse_and_keep_versions)'"` | Both migration tests passed; expanded Artifact journey passed, c21db0f1-5457-4b93-9dca-bc01bdccd201 |
| `mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-loader -p wyrd-ai -p wyrd-rust-examples --all-targets -- -D warnings` | passed |
| `mise exec -- cargo doc --locked -p wyrd-client -p wyrd-loader -p wyrd-ai --no-deps` | passed, no warnings |
| `mise run fmt:check` | passed |
| `mise exec -- git diff --check 37b3c2f96 9be515a10` | clean |

Eight focused behavioral tests passed, plus both migration tests. Cached server/SDK linking reports the previously observed __eh_frame compact-unwind-size warning without build/test failure. Prior review-03/verification.md supplies accepted unrelated executor/component/security/finance/path-state proof. These unchanged domains are carried forward under subject.md; no full journey suite/broad gate, cross-platform execution or new fault injection is repeated. Task RED/mutation history is reported history, not an independent rerun of historical TDD. Broader implementation-reported gates remain historical evidence, distinct from the fresh checks above.
