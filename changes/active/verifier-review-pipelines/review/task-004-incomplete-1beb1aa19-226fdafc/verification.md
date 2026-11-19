# Independent verification of the unfinished candidate

Subject: HEAD `1beb1aa1980cb03495dfb0778a353604670ba935` plus captured WIP tree
`226fdafc56bacc707b9167d1887be9b72f5e6d53`, base `dfd19aa12`.

| Check | Actual result |
| --- | --- |
| `mise exec -- cargo check --locked -p wyrd-server --features test-support` | PASS, 1m18s after initially waiting for an existing Cargo lock. The reported `failure.summary()` compile failure did not reproduce in this captured candidate/configuration. |
| Hash each of the 92 changed source paths against its captured Git blob | PASS; no source changed after capture. Untracked source is checked by blob hash, not mistaken for deletion by a normal index-based diff. |
| `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=github_app_connection::install_wyrd_and_connect_selected_repositories)"'` | PASS, 1/1 in 6.013s after 2m20s test build; both migration prerequisites also passed. This is the existing Rust server-flow test, not a completed production browser UI journey. |

Implementation-recorded tests remain claims unless explicitly appended below as
rerun results. No code was fixed during review. No live provider/GHES exercise
or broad aggregate was run. This build check does not prove unfinished runtime
behavior or public user journeys.
