# Review-time verification — candidate 5e7dec697

Checked owning commands with `mise tasks ls --hidden`; Cargo runs serialized. No implementation edits.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_card_declaration_route::hydration_requires_recorded_envelope) | test(=pg_card_declaration_route::compensation_records_inventory_deletion) | test(=pg_card_declaration_route::completed_objects_use_one_inventory) | test(=pg_card_declaration_route::deleted_objects_survive_completion_replay) | test(=pg_card_declaration_route::inventory_reads_are_scoped_and_retryable) | test(=pg_card_declaration_route::card_reconciler_recovers_after_storage_retry) | test(=pg_card_declaration_route::delete_storage_failure_preserves_cleanup_state)' && cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_migration::pg_tests::fresh_install_has_no_postgres_object_inventory)'"
```

Result: both Wyrd/Vala migration setup tests passed; all seven selected Card tests passed (24.386 seconds); SQL absence proof passed (0.146 seconds). Fresh repository-managed Postgres was used. Linker warning about oversized `__eh_frame` compact-unwind offsets was emitted; tests completed successfully.

`git diff --check 826fcf9c7 5e7dec697`: exit 0.

```bash
mise exec -- env -u WYRD_DATABASE_URL -u WYRD_PLATFORM_DATABASE_URL cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_migration::pg_tests::fresh_install_has_no_postgres_object_inventory)'
```

Expected negative result reproduced: nextest exit 100, exactly one test failed at the setup expectation with `the fresh-install proof requires database DSNs`. This establishes the formerly silent skip now fails explicitly; it is not an unexpected failing gate.
Authority diff against the prior candidate is empty for AGENTS, agent_docs, architecture, spec-driven-development and TESTING. `826fcf9c7..e6a34a010` adds only prior review artifacts; there are no source changes outside the stated remediation range.

Broader format/lints/codegen and the other five reported Card regressions remain recorded remediation evidence; this review does not repeat unaffected broad lanes. Original accepted outbox/tenant/catalog/validation evidence carries forward as scoped in scope.md. Controlled RED history is recorded implementation evidence, not independently reproduced here.
