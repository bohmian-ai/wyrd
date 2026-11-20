# Candidate verification

Candidate: `c93287b54bf944c8c8cc9323f81c12ba5174369c`; root orchestrated all Cargo commands sequentially. Reviewed production/spec/task files were unchanged from this commit before execution. Current uncommitted contributor-instruction edits are disclosed in `subject.md`.

Every Postgres invocation below was executed through `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc '<commands>'`. Each began with `mise run db:migrate:all:inner &&`; both Wyrd and Vala migration/idempotence setup tests passed. Test commands used `WYRD_LOG=info,wyrd_server=debug`.

## Focused owner tests

```sh
cargo nextest run --locked -p wyrd-server --features test-support --lib --test-threads=1 -E 'test(/renewing_a_connection_secret_keeps_its_revision_and_login$/) | test(/renewing_issuer_encryption_keeps_the_secret_and_respects_deletion$/) | test(/issuer_renewal_survives_outage_and_loses_to_deletion$/) | test(/gateway_managed_secret_renewal_keeps_the_key_and_loses_races$/)'
```

Run `de0d966a-abd7-44d2-89a8-5ad99cf27f20`: **4/4 PASS**, 13.280 seconds; 665 excluded. Exact selected tests:

- `components::admin::identity::pg_tests::renewing_a_connection_secret_keeps_its_revision_and_login`.
- `components::admin::routes::pg_tests::renewing_issuer_encryption_keeps_the_secret_and_respects_deletion`.
- `components::admin::routes::pg_tests::issuer_renewal_survives_outage_and_loses_to_deletion`.
- `components::gateway::pg_administration_tests::gateway_managed_secret_renewal_keeps_the_key_and_loses_races`.

```sh
cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_operator_delivery::renewed_encryption_keeps_the_credential_and_delivers_it) | test(=pg_operator_connection_routes::renewal_keeps_revocation_and_refuses_without_authority) | test(=platform_admin_e2e::renewing_platform_connection_encryption_keeps_its_sign_in)'
```

Run `3a58c668-d210-4c09-99e5-bed5a08dde93`: **3/3 PASS**, 41.091 seconds; 484 excluded. Overall wrapper exit 0.

These owner tests use real repository Postgres and the controlled HTTP `TransitFixture`, not a real Vault daemon. Prior TASK-001 verification records real dev Vault provider execution (`f1932e09-4d54-467e-988c-3f3cf7fd7b72`); no new live-provider run is claimed here.

## Served OpenAPI contract

```sh
cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 --no-fail-fast -E 'test(=pg_openapi_contract::operator_connection_contract_publishes_five_typed_operations) | test(=pg_openapi_contract::unauthenticated_requests_match_each_operation_declared_security) | test(=pg_openapi_contract::gateway_operations_publish_problem_json_refusals)'
```

Run `cef3e312-e11e-4e0f-989a-2e89bd758fe2`: **2 PASS, 1 FAIL**, 4.840 seconds; 484 excluded; wrapper exit 100.

The Operator test failed at `crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:2128`. Its exact-operation-set assertion received six operations, including `POST /v1/operator-connections/{connection_id}/renew-encryption`, against five expected operations that omit renewal. The trace shows successful server boot and `GET /openapi.json` 200 before this assertion; this is a contract-test expectation failure, not setup failure. The gateway problem-response and unauthenticated-security tests passed.

## CLI consumers

The managed Postgres wrapper ran migration setup, then `cargo build --locked -p wyrd-cli -p wyrd-server --bins`, followed by:

```sh
WYRD_CLI_E2E=1 WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all --test-threads=1 --no-fail-fast -E 'test(=card_lifecycle::pg_tests::cli_manages_redacted_operator_connections) | test(=gateway_server_journey::gateway_commands_manage_redacted_tenant_configuration) | test(=secret_sources::trusted_issuer_renew_encryption_sends_no_secret)'
```

Run `bbc711d5-cb78-437e-9dd4-aa27732d6f39`: **3/3 PASS**, 5.555 seconds; 59 excluded; wrapper exit 0. Operator and Gateway tests use the real server; the trusted-issuer test uses wiremock and establishes CLI request/output behavior only.

## Limits and other checks

`git diff --check` passed. Compilation emitted a macOS linker warning about the large `__eh_frame` unwind table; compilation and execution succeeded. Full lint/codegen/docs/dependency checks are recorded as passing in the implementation task; this review does not claim to have rerun them. No benchmark or broad aggregate was run: reviewers found no changed runtime hot path, and the current task does not authorize benchmarks.

Historical test-first evidence remains **VIOLATED**, as explicitly recorded in TASK-003. These GREEN runs do not repair historical RED chronology.
