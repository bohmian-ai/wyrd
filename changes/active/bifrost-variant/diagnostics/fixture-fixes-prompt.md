You are an independent read-only reviewer in the Wyrd repository at /home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003. Do not edit any file. Read AGENTS.md first.

Two test failures appeared under `mise run verify:bifrost`:

A) mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads) | test(=published::variant_sql_registry_covers_every_session)'
   Error: InvalidArgumentError("column types must match schema types, expected Timestamp(µs, \"+00:00\") but found Timestamp(µs, \"UTC\") at column index 7")
   Trace: /tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-bifrost-variant-task-003/b0f1672d-4b58-46cd-a08c-b67292902cbb/scratchpad/diag-ts.log

B) mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_grpc_ingest_smoke --test-threads=1 -E 'test(=system_writer_alone_writes_verification_results) | test(=system_result_writes_require_the_exact_signed_verifier_scope)'
   Error: Bifrost write refused: "undeclared field details in row 0" (WYRD_VALA_400_BIFROST_UNDECLARED_FIELD)
   Trace: /tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-bifrost-variant-task-003/b0f1672d-4b58-46cd-a08c-b67292902cbb/scratchpad/diag-smoke.log

(Both need scripts/postgres/with-test-postgres.sh; you need not run them.)

Proposed fix (both now pass): /tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-bifrost-variant-task-003/b0f1672d-4b58-46cd-a08c-b67292902cbb/scratchpad/fixture-diff.patch — it changes only test fixtures.

Decide independently, from production code and git history:
1. For A and B, the code-level cause (file:line).
2. Is the right fix site the test fixture, or is production code wrong? Say which and why.
3. Does the proposed diff weaken any assertion, or hide a real production defect?
Under 350 words.
