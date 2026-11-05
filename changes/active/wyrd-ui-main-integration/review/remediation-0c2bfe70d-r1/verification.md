# Independent verification record

Subject: ed82f63bf35ca124c0a2980c160f0d64535eebd1 -> 0c2bfe70d5186dff4dc224c7e402b57c24f354d9. Initial working tree clean, prior candidate is merge base; source unchanged through checks. New reports only.

## Independently run by orchestrator

1. `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts src/lib/ui-library.test.ts`: PASS, 22 tests, 2 files. Shared package rebuilt from current source.
2. `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::scribe_persistence_path::a_stop_after_durable_series_rows_recovers_them_and_admits_the_retry) | test(=scribe::scribe_persistence_path::a_stop_after_durable_unanswered_points_recovers_every_point_indexed) | test(=scribe::scribe_persistence_path::a_series_has_one_row_per_accepted_context_in_an_hour)'`: PASS, all 3 selected cases; 915 skipped by selection. Test execution does not alone establish crash-boundary/lifecycle validity, which is separately source-reviewed.
3. `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-004: Metric windows count every recorded value$"'`: PASS, exact math integration selected/running, 1 passed / 3 skipped; fresh repository Postgres migration setup passed. This supersedes the caller's earlier rerun limitation for this one test.
4. `git diff --check ed82f63bf 0c2bfe70d`: reports a new blank line at EOF in newly committed prior review artifact `review/range-9692bfd06-ed82f63bf-20261104/system-review.md:78`. The caller's clean-working-tree whitespace check is distinct from the immutable range check. No file was edited to remove it. Repository reviewer assesses applicability/materiality.
5. The filtered authenticated metrics browser journey is running; append result when complete.

## Supplied evidence and retained limits

The committed remediation task appends exact implementation commands/results: 22 UI unit tests, 17 scoped Rust tests, math integration, one filtered Metrics journey, UI check/build, fmt/clippy and whitespace check. Implementation evidence names e214d8505 as its pre-final-evidence endpoint; current candidate includes later committed evidence. Source/evidence correspondence is reviewed independently.

Caller reports one restart per embedded recovery test; fixture has no long-term storage and replay retires restored WAL without staging for a second restart. Assess the explicit interruption/recovery obligation against actual fixture lifecycle, without assuming one restart proves repeated production recovery or inventing an unrelated fault matrix.

No codegen check was supplied because generated contract inputs did not change; unrelated Postgres-backed pg_tests was not run. Prior latency misses, AC040 elapsed-time uncertainty and missing human light/dark visual acceptance remain disclosed; no new synthetic host load or performance experiment is run. Prior source-backed unchanged acceptance is carried forward as specified in scope.md, not blindly re-executed.

## Filtered Metrics journey result

`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated metrics investigation$"'`: PASS, 1 selected browser journey passed / 5 skipped. Current UI package and production BFF build completed during the test, and fresh Postgres migration setup passed. This is automated journey proof, not human visual signoff. It exercises the written journey's interactions, not every possible control order.

Commit correspondence verified: incremental range contains seven commits, no rebase/ancestry ambiguity; `e214d8505..0c2bfe70d` changes only the appended remediation evidence (38 lines), so supplied implementation evidence's production-source endpoint corresponds to the reviewed candidate. Final root check still matches 0c2bfe70d and `git diff --exit-code` reports no tracked working-tree changes.
