# R1 candidate verification

Candidate `d3db737e5fccacbe02421ea135fd5327b6335d79`; workspace/evidence commit `fa990be1989959de963d062ad8a47336b40c62da` differs only in seven prior review documents. Production, tests, public docs, spec/task, and build configuration match candidate; user instruction edits are preserved. Root serializes all Cargo runs; reviewers do not run Cargo.

## Focused execution

The command uses the repository-managed isolated Postgres lifecycle:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc '<commands below in order, joined with &&>'
```

Migration setup: `mise run db:migrate:all:inner`.

```sh
WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-server --features test-support --lib --test-threads=1 -E 'test(=components::admin::routes::pg_tests::replacing_issuer_secret_keeps_its_bindings_and_loses_to_deletion) | test(=components::admin::identity::pg_tests::renewing_a_connection_secret_keeps_its_revision_and_login)'

WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_openapi_contract::operator_connection_contract_publishes_six_typed_operations)'

WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=compatible::managed_secret_opens_through_vault_transit_only_while_preparing)'
```

Then, under the existing Keycloak/Dex lifecycle wrapper, with `WYRD_KEYCLOAK_ISSUER=http://localhost:8180/realms/wyrd-test` and `WYRD_DEX_ISSUER=http://localhost:5556`:

```sh
scripts/identity/with-identity-providers.sh -- bash -lc '<build and test below, joined with &&>'
cargo build --locked -p wyrd-cli --bins --tests
WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-server --features test-support --test integration --run-ignored=all --test-threads=1 -E 'test(=identity_e2e::renewing_and_replacing_auth_secrets_keeps_clients_working)'
```

Execution started with a cold compilation cache. Managed Wyrd/Vala migration setup passed. The two exact server-library tests passed: nextest `8f0f2834-bffa-494a-a9b4-6e25d31f197e`, 2/2 PASS, 8.899 seconds, 668 excluded. The first server-library compilation took 6m11s. macOS emitted the existing large `__eh_frame` linker warning; compilation and tests succeeded.

- Operator six-operation served contract: nextest `4a13ee3b-b65f-4633-be49-43adb0aded0c`, 1/1 PASS, 1.595 seconds, 487 excluded.
- Gateway post-renewal provider dispatch: nextest `49166cba-0a92-42fb-bf23-a4af8cae7234`, journey profile, 1/1 PASS, 2.193 seconds, 21 excluded.
- Keycloak and Dex containers both reached healthy status before the identity wrapper proceeded.
- Identity HTTP/session/real-CLI journey: nextest `9b26c16d-79fe-4565-85ab-77d4336bd4b6`, 1/1 PASS, 4.379 seconds, 487 excluded. The preceding CLI binary/test build passed. The Keycloak/Dex and Postgres lifecycle wrappers completed cleanup; overall command exit 0.

Fresh focused total: **5/5 PASS**. These passing tests close the original five findings; they do not cover or contradict the separate source-validated new error-code declaration defect. Controlled owner/Gateway Transit endpoints are protocol mocks; Keycloak/Dex are real local providers. No additional real Vault or cloud KMS execution is claimed.

## Other evidence and limits

`git diff --check` passed. The implemented R1 task contains prior exact assertions/results including identity nextest `2f0237eb-ebe3-4f4d-849e-28497b3837c3` and Operator contract `f5a4733e-c5de-46b3-adee-9924ac5d2f83`, plus reported checks. This review distinguishes those records from new reruns.

No broad aggregate or benchmarks are assigned here; no runtime hot-path change is identified. New checks target the five prior findings; unaffected original evidence carries forward subject to independent closure review. Prior process history remains in the original record and is not an acceptance blocker.
