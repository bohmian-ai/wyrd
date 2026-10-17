# Independent verification

Source subject remains immutable: candidate 5c421663b43d7deeca135134d6b07cf49a11b83f, base ef3a57b0776dd7bc451273b9176498fcca700550. No production or test edits.

## Executed

- `git diff --check ef3a57b07..5c421663b`: exit 0.
- `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier_card_tests::removed_scheduled_samples_fields_fail_at_their_path)'`: exit 0, 1 passed, 757 skipped, nextest run 763106c1-077a-43fc-981a-7ab3060edfe8.
- `UV_CACHE_DIR=/tmp/task002-review-uv mise run py:format:check`: exit 0, 172 files already formatted.
- `UV_CACHE_DIR=/tmp/task002-review-uv mise run py:lints`: exit 0, Ruff passed and authored-stub validation passed.

Initial Python invocation failed before checking because uv could not create a cache temporary file in the read-only home cache. Retried using a writable temporary cache; no configuration or source changes.

## Pending Postgres probe

`WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E "test(/^pg_verification_bindings::/)"'`

Docker reports the isolated Postgres container healthy, but the wrapper is still awaiting a host endpoint. No migration or SQL test success is claimed. Final outcome will be appended.

## Limits

No full journey suite or `mise run gate` executed, per user constraint. Implementation evidence in the task remains separately attributed claims; this review does not convert recorded counts into independently rerun results. Broad checks, Python unit suite, and served OpenAPI runtime proof have not been independently rerun.

## Final Postgres outcome

The wrapper's sanctioned single rebuild recovered the host endpoint. The command exited 0: Wyrd migration/idempotence 1 passed; Vala migration/idempotence 1 passed; `pg_verification_bindings::` 10 passed (93 skipped). Nextest binding run: 6f7d5da3-3b86-444f-af93-3d65aaa83f7e. Thus Postgres-backed commands did run in this sandbox; the initial endpoint delay was an environment recovery, not a test failure. The wrapper completed its managed lifecycle.

## Focused served OpenAPI proof

Executed:

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E "test(=pg_openapi_contract::card_contract_publishes_typed_lifecycle_and_problem_shapes) | test(=pg_openapi_contract::verification_contract_publishes_five_typed_operations_and_the_local_upload)"'
```

Exit 0, 2 selected tests passed, 268 skipped; nextest run bdbf7a15-6a81-438d-b1b4-1668978224a1. The setup also repeated its required Wyrd and Vala migration/idempotence tests, 1+1 passed. This lifecycle again needed the wrapper's existing single endpoint rebuild. These focused tests inspect the served document for Card/verification contracts; they are not the full principals suite and do not newly assert every removed scheduled field. The earlier statement that served OpenAPI had not been rerun is superseded by this result.
