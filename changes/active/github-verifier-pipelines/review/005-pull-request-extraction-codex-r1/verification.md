# Verification during this review

Immutable candidate `1f798d132c03cbc2474e1bad4bc36763e8047848`, base `b262696afcd731d94433dfd1d8d0a9b3ab0d64f4`. These are fresh executions, distinct from the task's historical Evidence claims.

| Command | Result |
|---|---|
| `mise run fmt:check` | PASS, exit 0 |
| `mise run check:deps` | PASS, exit 0; includes `security_inline_gate` 1/1, nextest d6609fa5-0802-497b-919a-18fcfe5cceea |
| `UV_CACHE_DIR=/tmp/wyrd-task005-review-uv mise run check:tenant-isolation` | PASS, exit 0. Initial default-cache attempt could not write `/home/thorrester/.cache/uv`; redirecting cache to writable temporary storage resolves the environment constraint. |
| `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(/verifier::/)'` | PASS 24/24, nextest 6d036292-a35c-4a20-ab0f-b4c510ebec4b |
| `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/extraction\|engines/)'` | PASS 2/2, nextest 5ac6a7e1-f2ac-4b95-8aa4-fd88a510e8ba; selector uses regex alternation `extraction|engines` |
| `git diff --check b262696af...1f798d132` | PASS, exit 0 |

Focused real-Postgres server integration rerun is pending; final results will be appended. Cargo execution is serialized by the orchestrator. Reviewers independently inspect source and actual assertions; they do not treat historical test names as executed proof.

`codegen:check` and `docs:check` invoke generators that rewrite subject files. They are not rerun in this immutable review; generated parity and documentation are inspected statically, with historical execution claims clearly distinguished. No full aggregate or benchmark is run by default; system reviewer assesses whether modified waits touch direct hot paths or approved queue-only acquisition.

## Connected server rerun

`WYRD_LOG=info,wyrd_server=debug mise exec -- bash scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(/pg_github_pull_request_extraction|pg_code_review_cache|pg_github_pull_requests|pg_hosted_files|registration_derives_queue_only/)'`

PASS: 55/55 (363 unselected), 46.822 s, nextest `0cf578f8-35fa-49e4-8ecd-e3652c7aebeb`, command exit 0. Repository-managed isolated Docker Postgres lifecycle completed. Includes full/incremental/fallback extraction, missing/truncated source, model limits, live held-persistence consumption, recovery after context expiry, Prompt/workspace and bystander isolation, direct refusal, actual metadata group commits/lone write/failure rollback, provider admission/access regressions, and legacy media extraction.

Focused SQL execution-owner lane started next, serialized after server lane; result pending below.

## SQL owner rerun

`WYRD_LOG=info,wyrd_sql=debug mise exec -- bash scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(/pg_verifier_executions/)'`

PASS: 40/40 (99 unselected), 5.200 s, nextest `e78fdda3-e7ab-4afd-b966-59222eaf00bf`, command exit 0. Covers baseline identity/policy/tenant eligibility, judgment-independent extraction fact, single/batched completion parity and replay, stale/rolled-back/cancelled completion, permitted PR origin, existing claims/deadlines and submission/settlement regressions.

## Shared block-engine regression rerun

`mise exec -- cargo nextest run --locked -p vala-verify --lib`

PASS: 77/77, 0.328 s, nextest `2dd8f2ce-752a-4819-9d1a-6e740f1f726f`, command exit 0. Includes existing media extraction/binding, accepted step restoration, fanout, ordinary inputs/workspace and requirement evaluation around the changed extraction source binding.

All pending runs mentioned earlier in this log have now completed successfully. Total focused behavioral tests: 198 passed (24 spec + 2 server unit + 55 server integration + 40 SQL integration + 77 block-engine); dependency check additionally runs its inline security test. No source-changing generators, broad aggregate, full Clippy or performance campaign were run. No production provider credentials were needed; connected tests use local provider/model transports with real repository-managed Postgres.
