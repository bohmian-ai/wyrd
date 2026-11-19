# Review verification

Immutable subject is in scope.md. No source edits, staging, commits or pushes.

- Ancestry `git merge-base --is-ancestor efda8cb2c fb8da5ad7`: pass.
- `git diff --check efda8cb2c fb8da5ad7`: pass.
- `mise run fmt:check`, `mise run lints`: pass (workspace all-feature/all-target Clippy plus shipped server binary).
- `mise run check:deps`: pass, including inline-source security gate 1/1 (`633c9e4d-d468-4f0f-9e9e-7ca94ac30838`).
- `mise run check:tenant-isolation`, `mise run py:lints` (Ruff and authored stubs), `mise run ts:lints`: pass.
- Managed Postgres visible migrations: wyrd-sql 1/1 and vala-sql 1/1 pass on both completed lifecycles.

First focused server command:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(/pg_github_pull_requests/) | test(/pg_github_connection/) | test(/pg_verification_routes/) | test(/pg_durable_verifier_work/)" && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(/pull_request_/)"'
```

Server run `ab6733c7-635c-457c-8b15-f77eb4cc3034`: 66/67 pass, one failure `pg_durable_verifier_work::cancellation_writer_race_preserves_terminal_delivery` (11.261s); all PR/connection tests passed. SQL did not execute because the server command failed. The tool output truncated the full failure assertion; retained trace showed Scribe terminal rejection and writer changes refused during cancellation, insufficient to establish cause. Do not claim a diagnosis or erase this result.

Exact isolated rerun under a fresh managed Postgres lifecycle:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_durable_verifier_work::cancellation_writer_race_preserves_terminal_delivery)" && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(/pull_request_/)"'
```

Isolated test run `6cf8f8c6-4383-4b8e-a4d4-c3d806754276`: 1/1 pass, 6.075s. SQL run `818bab76-b323-4f9b-a2d8-e067e4e7360c`: 2/2 pass. Log captured outside review directory at /tmp/wyrd-task006-r2-focused.log. The same-selector server rerun under a third fresh managed lifecycle passed 67/67 in 44.370s, run `d0d2d93c-9f27-499d-b9c2-9e67b6fdbc7e`. Both visible migration proofs passed. Output is captured outside reports at /tmp/wyrd-task006-r2-server-rerun.log. The initial isolated failure remains unexplained; no timeout/test/assertion/source was changed between runs. This is a verification limitation, not evidence of a diagnosed fix.

No production source mutation or reproduction patch is permitted in review. Implementation evidence for 602 unit tests, docs/codegen/dependency checks and original five journeys remains attributed to task/prior reports, not newly executed here. No full journeys, broad gate or source-writing generator rerun. Initial reporting helper invocation using `mise exec -- python` failed because that executable is absent; `python3` succeeded, no source affected.
