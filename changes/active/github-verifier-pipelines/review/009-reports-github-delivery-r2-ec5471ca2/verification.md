# Focused verification

Candidate HEAD: ec5471ca2421e9ba0049658546554f1b3c6dac8a. No source edits made.

Independent exact-name unit rerun:

```text
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::github::presentation::tests::reports_render_without_an_extraction) | test(=components::github::presentation::tests::evidence_in_base_or_context_source_is_valid) | test(=components::github::presentation::tests::every_rendered_report_has_a_review) | test(=components::github::presentation::tests::anchors_outside_the_retained_source_are_problems) | test(=components::github::app::tests::cooldowns_last_as_long_as_github_asks) | test(=components::github::delivery::tests::outcome_identity_is_per_event_execution_and_revision)'
```

Result: 6 selected tests passed; 641 skipped by selector. Nextest run 489c3129-3f2a-43ff-894c-3807a0791ea6.

git diff --check 4d001e6d9 ec5471ca2 passed.

Focused Postgres command:

```text
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner >/dev/null && cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_github_delivery::a_waiting_delivery_never_updates_an_existing_check) | test(=pg_github_delivery::cancelling_a_waiting_execution_completes_its_check_cancelled) | test(=pg_github_delivery::a_code_review_report_becomes_one_check_and_one_comment_review) | test(=pg_github_delivery::each_execution_selects_its_report_from_its_accepted_final_outputs)'"
```

Result: 4 selected journeys passed; 446 skipped by selector. Nextest run 02171f4f-2040-410e-bef6-69dd3f53f83d. Both required migration setup tests also passed (Wyrd and Vala idempotent migrations). The disposable Postgres wrapper completed successfully.

Implementation evidence in the committed remediation task records presentation units 9/9, GitHub units 16/16, pg_github_delivery 10/10, pg_github_pull_request_extraction 21/21, CLI github_journey 5/5, report units 4/4, lints, docs:check and diff check. Those are recorded implementation results, independently assessed through source/assertions rather than claimed as fresh reruns here.

Round-1 source-backed acceptance and its verification.md carry unchanged evidence. No broad journey suite, live GitHub, full benchmark, codegen, lint or docs rerun is claimed for this round. No test was changed during review. The no-separate-RED process exception is explicitly supplied by the user; it does not waive behavior or documentation gates.
