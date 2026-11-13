# Round 3 observed verification

Immutable source candidate: `4d55d3654ab7361b97aeadbb72b8813707515d49`. Commands executed by the orchestrator, sequential Cargo invocations, with no source mutation.

1. `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(/pg_approvals::/) | test(=pg_verification_routes::enqueue_is_authorized_by_rbac_alone) | test(=pg_verification_routes::direct_execution_is_authorized_by_rbac_alone) | test(=pg_verification_runtime::accepted_authority_survives_account_and_context_changes)"'`
   PASS: 13/13 tests, run `ee99047e-eb54-4e52-8042-318f98d0c157`. Covers every approval integration test, including unavailable target, cold one-slot pool, authored expiry, delegated actor, receipt/replay, ACK unavailability, withdrawal, restart and supersession; plus changed direct/enqueue RBAC and accepted-authority journeys. Both SQL migration tests passed first.
2. `mise run fmt:check`: PASS.
3. `git diff --check cac4a4ce3 4d55d3654`: PASS.
4. `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::direct_execution_is_authorized_by_rbac_alone)"'`
   PASS: 1/1 test, run `d40d6843-92eb-47a5-a07e-06fbcbef45c4`. Both SQL migration tests passed first.

No failed test, timeout adjustment, ignored test, source patch, synthetic load or alternative harness was introduced. Temporary command logs are outside the review directory. Implementation-recorded lint/codegen/docs evidence remains historical, not a fresh run claimed here. Broader SDK/CLI suites, full lints/codegen/docs checks, aggregate gates and L1 benchmarks were not rerun; full integrated qualification remains change-review work under the carried-forward R1/R2 bounded-review agreement. Current source was checked against the immutable HEAD before validation; requested report-only commit follows verdict construction.
