# Round 2 executed verification

Subject: `798b6b0e83595356c0d2328fa541119267e84fb2..cac4a4ce3ba4b6c41d23e81e927f6afbe1de284a`.

- `mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::a_reached_approval_is_recorded_while_independent_work_runs) | test(=blocks::engine::tests::approvals_hold_only_their_dependents_until_decided)'`: PASS 2/2, run `7dfb7987-73e2-4c40-862b-cb2d2808b2c4`.
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner >/dev/null 2>&1; WYRD_LOG=info,wyrd_server=debug mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(/pg_approvals::/) | test(=pg_github_pull_request_extraction::an_approval_beside_running_work_is_decided_before_it_parks)"'`: PASS 8/8, run `52ef6593-bf22-4bbc-a184-ffe5918be3c1`.
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::verification::service::approvals::tests::only_the_deciding_actor_replays_a_decision) | test(=verification::approvals::tests::an_unavailable_read_hides_its_cause) | test(=verification::state::tests::each_approval_occurrence_has_its_own_stable_id)'`: PASS 3/3, run `0f45450c-21cc-4ab5-802c-125c717c94f5`.
- `mise run fmt:check`: PASS.
- `git diff --check`: PASS.

Build/test Cargo commands were sequential. Process caveat: `fmt:check` (cargo fmt) was launched while the Postgres wrapper was still active; that wrapper's test finished before its next poll, so exact overlap with cargo test was not established. No further overlap was introduced. This does not comply with the stricter instruction to keep all Cargo invocations sequential; disclosed rather than claiming strict serialization.

No production source changes, RED mutations, new harness, broad SDK/CLI/MCP suites, full Clippy, codegen/docs build, SQL-wide gate or L1 benchmark was run for this bounded review. Historical remediation evidence is separate from these observed runs. Full integrated qualification remains change-review work under the R1 remediation's explicit scope. The delegated-actor unit test is not an executed real-token-exchange journey.
