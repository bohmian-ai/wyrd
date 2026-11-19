# Independent review verification

Root ran these sequentially against candidate 28f07d99c; no source changes or generators. All passed with nonzero exact selection.

1. `mise exec -- git diff --check 75c9bfa9e 28f07d99c`: exit 0, clean.
2. `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::state::tests::a_call_keeps_its_original_publication)'`: 1 passed / 588 skipped; Nextest 28ba5a47-947c-4a3a-96bb-8a91e891c7aa.
3. Repository Postgres wrapper/migration + the receiving audit and three affected state/recovery cases, exact command:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --lib --run-ignored=all --test-threads=1 -E "test(=verification::hosted::tools::pg_tests::file_publications_decide_their_file_permission)" && cargo nextest run --locked -p wyrd-server --features test-support --test integration --run-ignored=all --test-threads=1 -E "test(=pg_durable_verifier_work::retained_publications_keep_their_busy_attempt) | test(=pg_durable_verifier_work::agent_publications_reach_a_recovered_consumer) | test(=pg_durable_verifier_work::dropped_agent_work_makes_no_later_call)"'
```

Both repository migration prerequisites passed. File permission lib case: 1 passed / 588 skipped, Nextest d5891398-5dd1-4484-b04d-c1061ce30b95. Integration: 3 passed / 350 skipped, Nextest 609ba1d1-9dfc-41d5-b6b8-8ebd8b7f7908. All command exits 0. Postgres wrapper used its managed lifecycle.

Supplied evidence in the remediation file records focused SDK/MCP/HTTP/schema/CLI runs, formats/clippy/deps/tenant and Python/TS format/lints/types. Reviewers must inspect actual assertions; these remain supplied execution results unless explicitly rerun. No broad suite, benchmark, codegen, docs aggregate or synthetic host load was run in this review. Generation inputs are unchanged in the remediation. Historical TDD admission remains preserved; current fixes record their genuine RED/GREEN separately.
