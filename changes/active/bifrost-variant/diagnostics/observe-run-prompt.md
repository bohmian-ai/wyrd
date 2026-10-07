You are a read-only diagnostician in the Wyrd repository at /home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003. Do not edit any file. Read AGENTS.md first.

Failing command (run through the repo's Postgres wrapper):
  WYRD_LOG=info,vala_bifrost_redux=debug,wyrd_client=debug scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test observe_run -P journey --run-ignored=all --no-capture -E 'test(=scoped_run_emits_drift_eval_and_generic_rows)'"

Failure: sdks/wyrd-sdk-rust/tests/observe_run.rs:308 panics with
  eval rows read back: RowDeserialization("invalid type: map, expected a string at line 1 column 12")

Full trace: /tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-bifrost-variant-task-003/b0f1672d-4b58-46cd-a08c-b67292902cbb/scratchpad/diag-observe.txt
Uncommitted diff on top of HEAD: /tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-bifrost-variant-task-003/b0f1672d-4b58-46cd-a08c-b67292902cbb/scratchpad/observe-diff.patch (mostly an Arrow 59->60 dependency bump; HEAD is the last commit).

Find the root cause from code and the trace. Return exactly:
1. Cause: the code-level reason (file:line), not "flaky".
2. Fix site: the single shared owner where the fix belongs.
3. Affected callers: other code paths that hit the same owner.
Keep it under 300 words.
