# Review-time verification

Candidate `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`; base `15f939fc917b22efa5e5e4d12fd1f97be807a0a7`.

The following exact-selector run completed successfully on fresh repository-managed Postgres with tracing enabled:

```sh
WYRD_LOG=info,wyrd_server=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_verification_routes::file_upload_uses_shared_completion) | test(=pg_verification_routes::file_reuse_requires_completed_file_inventory) | test(=pg_verification_routes::file_upload_authority_and_validation_are_preserved)'"
```

Wyrd migration/idempotency and Vala migration/idempotency each passed (one exact setup test each). All three selected File tests passed, 305 tests skipped by the explicit selection. Nextest run `97228ebd-c750-47f5-b838-803699102f01`: three passed in 7.385 seconds. A linker compact-unwind size warning did not prevent execution.

`git diff --check 15f939fc9...5ef2981e6` passed. No source was modified. Broad suites, cloud credential lanes and the implementation's historical RED runs were not repeated. Runtime tests above establish only their exercised scenarios; independently source-proven lifecycle gaps require their own closure proof.
