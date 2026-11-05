# Independent verification

Subject: e0354f9bd...5c560663e (full identities in navigation.md). All commands through mise; review source unchanged. These are actual reruns, not implementer completion claims. No secret values or captures recorded.

## Completed

Managed Postgres prefix for server/Rust/MCP runs:
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc '<migration> && <test>'`
`<migration>` is `mise run db:migrate:all:inner` (SQL and Vala migration idempotence passed).

| Proof | Exact test command after migration | Result |
|---|---|---|
| V1 | `cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::local_signup_login_and_password_lifecycle)"` | 1 passed |
| V2 | `cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::local_signup_is_tenant_bound_and_rate_limited)"` | 1 passed |
| V3 | `cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::user_keys_preserve_owner_scope_and_credential_lifecycle)"` | 1 passed |
| V6 | `cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=principal_roles::users_manage_personal_keys)"` | 1 passed |
| V9 | `cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=principals::pg_tests::users_manage_personal_key_metadata)"` | 1 passed |
| V7 | `uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_principal_roles.py::test_users_manage_personal_keys` | 1 passed |
| V8 | `pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/principal-roles.test.ts -t "^users manage personal keys$"` | 1 passed |
| Existing exchange regression | `cargo nextest run --locked -p wyrd-auth --lib -E "test(=exchange_api_key::pg_tests::api_key_exchange_issues_no_refresh_token_or_row)"` | FAIL: 1 selected, 0 passed; panic exchange_api_key.rs:1102, SQLSTATE 23514 auth_api_keys_one_owner during fixture insertion |

UI browser prefix: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "<selector>"'`.

| Proof | Exact selector | Result |
|---|---|---|
| V10 | `^observe boards match approved canvas$` | 1 passed; no human acceptance supplied; test uses recovery sign-in |
| V4 | `^local signup and personal settings$` | 1 passed |
| V11 | `^authenticated (traces and spans|logs investigation|metrics investigation)$` | 3 passed |
| V5 | `^development environment lifecycle$` | 1 passed, 59.06 seconds; small workload and append, cleanup; default-500 deferred by task to change review |

`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check`: 0 errors, 0 warnings.
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build`: exit 0; third-party Rollup annotation/circular-dependency warnings present.
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts src/lib/ui-library.test.ts`: 2 files, 16 tests passed.
`mise run py:format:check`: 172 files already formatted. Read-only equivalent selected instead of modifying formatter.
`mise run py:lints`, `mise run py:typecheck`, `mise run check:tenant-isolation`: passed.

## Additional completed checks

V12 `mise run test:identity:ui`: PASS, 5 Vitest production BFF/provider tests including rendered SSO account settings; outer exact nextest identity_ui_e2e::production_ui_bff_journey 1 passed. Real Keycloak/Dex and replicas.

`mise run fmt:check`: PASS (read-only equivalent of fmt).
`mise run lints`: PASS workspace all features/all targets and shipped server binary, -D warnings.
`mise run check:deps`: PASS, including inline-source security test.
`mise run codegen:check`: PASS; generated wire/errors/problems and palette/catalog current, five theme generator tests passed. No tracked source diff after regeneration.
`mise run ts:build` followed sequentially by `mise run ts:build:testing`: PASS; generated public/harness declarations unchanged.
`mise run --no-deps ts:typecheck`: PASS; emitted build prerequisites also completed in order. Public SDK, support-desk app, and docs examples checked.
`mise run --no-deps ts:napi:check`: PASS; public generated declaration matches Git.

Pure password tests, each exact command run independently:
- `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=passwords::tests::usernames_canonicalize_or_refuse)'`: 1 passed.
- `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=passwords::tests::new_passwords_follow_the_policy)'`: 1 passed.
- `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=passwords::tests::verifiers_match_only_their_password)'`: 1 passed.

Served OpenAPI regression owner: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(pg_openapi_contract::)"'`: 23 selected, 23 passed. This existing module does not exercise every newly invalid password body; green schema checks do not waive extractor findings.

All selected required V1–V12 and listed check reruns completed. Existing exchange regression remains red as documented. Candidate still 5c560663e77ae07c33d198862ba6d795b656097a; tracked working tree unchanged. Final verdict identity check: branch reference and HEAD both remain that candidate; git diff --exit-code is clean, with only this report directory untracked.

## Limits
No review assertion claims separate human visual approval. No default500 change-review replay. No implementation changes, skipped/weakened tests, synthetic host load or synchronization sleeps added by review.
