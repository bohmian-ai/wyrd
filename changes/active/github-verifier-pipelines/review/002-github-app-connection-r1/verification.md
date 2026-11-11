# Independent verification

Candidate `3dd1555a0f40117ee512ecaaff1f5b51845ef3b6`. Checks run in the review worktree against committed source. PostgreSQL and identity providers used dedicated local Compose projects.

- `mise exec -- git diff --check 395274d58...3dd1555a0`, `mise run fmt:check`, and `mise run check:tenant-isolation` passed.
- `mise run db:migrate:all:inner` passed in the repository PostgreSQL lifecycle. The same lifecycle passed the following focused commands.

```sh
cargo nextest run --locked -p wyrd-sql --test integration -E 'test(/^pg_github_installations::/)'
cargo nextest run --locked -p wyrd-server --lib -E 'test(/config::tests::github_/)'
cargo nextest run --locked -p wyrd-server --test integration -E 'test(/^pg_github_connection::/)'
```

Results were 5 SQL, 4 configuration and 1 server test passed. These existing tests do not by themselves prove every required failure interleaving.

- The focused real-provider OIDC regression run passed 3 tests. It used the repository PostgreSQL lifecycle, `mise run db:migrate:inner`, and `scripts/identity/with-identity-providers.sh` with the repository Keycloak and Dex issuer settings.

```sh
cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --test-threads=1 -E 'test(=identity_e2e::tenant_human_login_journey) | test(=identity_e2e::tenant_callback_refusal_journey) | test(=identity_e2e::tenant_connection_test_sign_in_journey)'
```

This covers the tenant login, callback refusals and candidate connection test sign-in. The complete identity lane also covers UI and other clients and was not rerun.

- The Rust SDK, CLI and MCP journeys and served OpenAPI tests passed using the repository PostgreSQL lifecycle and `mise run db:migrate:all:inner`. The runs passed 4, 1, 7 and 24 tests respectively.

```sh
cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(/^github_connection::/)'
cargo nextest run --locked -p wyrd-cli --test cli -E 'test(/^github_journey::/)'
cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(/^github::/) | test(/^discovery::/) | test(/^connectivity::/)'
cargo nextest run --locked -p wyrd-server --test integration -E 'test(/^pg_openapi_contract::/)'
```

The TypeScript format, lint and type gates passed independently through `mise run ts:format:check`, `mise run ts:lints`, and `mise run ts:typecheck`. The last task built the native bindings and type-checked the SDK, support-desk app and docs examples. The tracked worktree still matched the candidate afterwards.

The two tests cited as examples of missing exact selectors in the standards discovery also passed independently. The OpenAPI check used the same PostgreSQL lifecycle and migration as the other OpenAPI run.

```sh
cargo nextest run --locked -p wyrd-crypt --lib -E 'test(=tests::hmac_sha256_matches_rfc_4231)'
cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::github_connection_operations_publish_their_contract)'
```

The implementation’s evidence supplies the Python and TypeScript journey results and the remaining broader gates and lints. The reviewer did not use live GitHub App credentials.
