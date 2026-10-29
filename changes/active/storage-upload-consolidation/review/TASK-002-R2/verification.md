# Review-time verification

Candidate `a80afd68e47d992e35f20931984d96524b2e7dbf`; prior candidate `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`.

The following run completed with exit 0 on fresh repository-managed Postgres. Cargo commands ran sequentially, with tracing enabled and nonzero exact selectors:

```sh
WYRD_LOG=info,wyrd_server=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_verification_routes::file_sessions_are_independent) | test(=pg_verification_routes::file_upload_uses_shared_completion) | test(=pg_verification_routes::file_reuse_requires_completed_file_inventory) | test(=pg_verification_routes::file_upload_authority_and_validation_are_preserved) | test(=pg_hosted_files::evidence_refuses_tampered_body)' && cargo nextest run --locked -p wyrd-storage --test integration --test-threads=1 -E 'test(=pg_sweeper::pg_tests::tick_reclaims_file_sessions_without_starving_card_uploads) | test(=pg_sweeper::pg_tests::tick_sweeps_expired_uploads_and_reaps_idempotency_rows) | test(=pg_sweeper::pg_tests::sweeper_skips_reclamation_when_upload_already_completed)' && cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E 'test(=pg_migration::pg_tests::storage_admin_queries_find_and_abort_expired_uploads) | test(=pg_migration::pg_tests::storage_queries_round_trip_under_tenant_conn)' && cargo nextest run --locked -p wyrd-storage --lib -E 'test(=tenant_path::tests::validate_evidence_accepts_only_own_tenant_digest_paths)'"
```

| Selected owner | Result |
|---|---|
| Wyrd migration/idempotency setup | 1 passed |
| Vala migration/idempotency setup | 1 passed |
| Five server File overlap/lifecycle/reuse/authority/tamper selectors | 5 passed; run `04fd44cc-81d6-496a-98b2-969047fd8cbe`, 10.320 seconds |
| Three storage sweeper selectors | 3 passed; run `295b93ad-57f1-448c-bdba-6e43dd740015`, 2.663 seconds |
| Two SQL storage projection/round-trip selectors | 2 passed; run `f5c9e239-9c46-445f-9f69-beaa8d9076a1`, 0.300 seconds |
| Strict File path unit selector | 1 passed; run `5f94d49b-984a-40d7-9e55-6077d9a6e473`, 0.009 seconds |

`git diff --check 5ef2981e6 a80afd68e` passed. The linker compact-unwind size warning did not prevent execution. No implementation source was modified.

Broad format/lint/language/emulator/cloud suites and historical REDs were not rerun. Remediation evidence records its own format/lint/boundary/docs, client/unit and backend transfer results. Provider-emulator sweeper abort and real-GCS multipart remain disclosed proof limits; source validation must distinguish unchanged provider behavior from newly changed owner/path dispatch.
