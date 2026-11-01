# Review-time verification

Candidate remains `826fcf9c742a36bcae5e8f68307e76685d88244e`.

- `git diff --check 3899b0a35...826fcf9c7`: exit 0.
- `mise tasks ls --hidden`: checked owning task map before execution.
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::inventory_ack_is_isolated)'`: 1 passed, 574 skipped.
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_card_declaration_route::completed_objects_use_one_inventory) | test(=pg_card_declaration_route::deleted_objects_survive_completion_replay) | test(=pg_card_declaration_route::inventory_reads_are_scoped_and_retryable)'"`: fresh repository-managed Postgres; Wyrd and Vala migration tests each passed; all 3 inventory journeys passed, 296 skipped.

The linker warned that `__eh_frame` exceeds the compact unwind table offset limit; tests completed successfully. No implementation changes were made. Broader format/lint/drift/dependency/codegen and producer regression runs are available as recorded task evidence and are not rerun absent a new reason.

These passing tests establish their asserted scenarios, not acceptance for unexercised paths. Source audit remains independent.
