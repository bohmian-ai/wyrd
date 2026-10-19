# Independent verification

Only the commands below were executed in this review. Implementer and prior-review results remain historical evidence.

- `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier_card_tests::removed_scheduled_samples_fields_fail_at_their_path)'`: PASS, 1 passed / 757 skipped, nextest db57447e-6cef-46f1-a47c-260307136586.
- `WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_card_registration_route::referenced_binding_refusals_leave_no_writes)"'`: PASS. Repository-managed Postgres runnable in this sandbox. Wyrd and Vala migration/idempotence tests each passed (1 each); focused HTTP test 1 passed / 269 skipped, nextest 108fac5a-d227-44ba-a520-50c3493a358f.
- `mise run fmt:check`: PASS, exit 0; no source writes.
- `UV_CACHE_DIR=/tmp/wyrd-task002-review-uv mise run docs:check`: environment failure, exit 1 at docs:install (`pnpm install --frozen-lockfile`: `[ERROR] unable to open database file`). No generation, docs tests, link/build/a11y stages ran. Tracked source remained unchanged. Do not claim docs gate green in this review. Historical implementation evidence records a green docs:check after remediation commit.
- `git diff --check ef3a57b0776dd7bc451273b9176498fcca700550...df148bd287d3e9fcae861470fe9c27bd576f2eeb`: exit 2, solely reports a new blank line at EOF of the prior review's domain-review-persistent-data.md:53. This is report whitespace, with no runtime/public contract consequence; no source correction made.

No full journey suites or mise run gate executed. No historical RED was reproduced by modifying the immutable source. Broad lint/codegen/SDK checks and runtime recovery qualification were not independently rerun. No production/test/task files modified; review reports only in the new directory.

## Additional focused served OpenAPI proof

Executed successfully:

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_openapi_contract::card_contract_publishes_typed_lifecycle_and_problem_shapes) | test(=pg_openapi_contract::verification_contract_publishes_five_typed_operations_and_the_local_upload)"'
```

Exit 0; 2 selected tests passed, 268 skipped; nextest 7e54e817-d9d5-4e51-9ab6-e89a3840806b. Wyrd and Vala migration/idempotence setup again each passed, 1+1. Postgres was runnable without a sandbox failure. These tests exercise the served Card/verification OpenAPI contract; they do not newly assert every removed field or replace the full principals suite. Any discovery report's earlier statement that no current served OpenAPI evidence exists is superseded by this entry. No source mutations or full journey suites.

## Documentation gate recovery — final PASS

The sanctioned lane succeeded with writable pnpm/npm/uv caches, without modifying repository configuration or tracked source:

```bash
PNPM_HOME=/tmp/wyrd-task002-review-pnpm npm_config_cache=/tmp/wyrd-task002-review-npm-cache npm_config_store_dir=/tmp/wyrd-task002-review-pnpm-store UV_CACHE_DIR=/tmp/wyrd-task002-review-uv mise run docs:check
```

Exit 0. Install succeeded using pinned pnpm 11.4.0; API/llms generation and tracked/untracked generated drift checks passed; docs tests 11 files / 87 tests passed; command and link checks passed; production static build/search passed; a11y passed, 32 pages and AA contrast in light/dark. `git diff --exit-code` afterwards passed: generated tracked files remain byte-identical to candidate. No source/test/config correction was made. This supersedes earlier unresolved-docs statements in discovery reports; the initial invocation remains accurately recorded as failed.

Two intermediate cache probes before the successful command were not verification passes: changing XDG data roots caused mise to attempt reinstalling tools (read-only rustup failure; interrupted, exit 130), and preserving MISE_DATA_DIR with relocated XDG roots failed protoc setup on the read-only mise installation (exit 1). Both stopped before docs execution. The final command changes only pnpm/npm/uv cache paths and retains default tool discovery. No gate was bypassed or weakened.
