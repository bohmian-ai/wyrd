# TASK-004 remediation review verification

Prior candidate: `a020500e8ea0c39311d508d6b5bfa85ebe90368c`. Cumulative candidate: `c9d72729b85187f2399725e0ed9496c4c4a39298`. Approved revision-8 spec SHA-256 remains `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`. All commands ran sequentially against unchanged source; review edits reports only.

| Independently rerun check | Result |
|---|---|
| Bounded histogram/precision plus timing module tests | 3 passed |
| Actual client trace/header module | 6 passed |
| Server metrics owner | 9 passed |
| Native Gate verified-write activation | 1 passed |
| Six exact new/changed stop/cohort/evidence/scan regressions | 6 passed |
| Affected tooling lib/binary module lanes under Postgres | 34 passed |
| Actual HTTP bearer receiving-activation test under Postgres | 1 passed |
| Repository-managed migration prerequisites | Both SQL owners passed |
| Prior-to-candidate whitespace check | Passed |
| Scoped formatting check | Passed |
| Scoped all-target/test-support clippy, warnings denied | Passed |
| Dependency gate and worktree whitespace check | Passed |

Exact runtime commands:

```bash
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(=tests::benchmark_histograms_preserve_bounded_intervals) | test(/^timing::tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-client --features test-support --lib -E 'test(/^transport::trace::tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^app::metrics::tests::/)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::only_verified_writes_activate_benchmark_labels)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --lib --bin capacity -E 'test(=capacity::tests::the_measurement_freezes_at_the_stop_before_work_drains) | test(=query::run::tests::completions_after_the_stop_are_late_and_kept) | test(=query::competing::tests::cohorts_follow_admission_not_completion) | test(=verification::run::tests::recovery_grace_arrivals_join_overload) | test(=experiment::tests::kept_timing_evidence_rebuilds_judged_readings) | test(=capacity::tests::snapshot_windows_keep_classes_and_scans_apart)' && cargo nextest run --locked -p wyrd-testing --lib --bin capacity -E 'test(/^capacity::tests::/) | test(/^experiment::tests::/) | test(/^query::run::tests::/) | test(/^query::competing::tests::/) | test(/^gateway::run::tests::/) | test(/^gateway::capacity::tests::/) | test(/^verification::run::tests::/)' && cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::middleware::authenticate::pg_tests::only_authenticated_requests_activate_benchmark_labels)'"
mise exec -- git diff --check a020500e8 c9d72729b
mise exec -- cargo fmt -p wyrd-telemetry -p wyrd-client -p wyrd-tonic -p wyrd-server -p wyrd-testing -p vala-bifrost-redux -- --check
mise exec -- cargo clippy --locked -p wyrd-telemetry -p wyrd-client -p wyrd-tonic -p wyrd-server -p wyrd-testing -p vala-bifrost-redux --features wyrd-telemetry/test-support,wyrd-client/test-support --all-targets -- -D warnings
mise run check:deps
mise exec -- git diff --check
```

New exact selectors are exercised individually by identity in the six-test OR command and again as part of the 34-test owning module lane. No listed selected test was skipped. Other tests outside these bounded selectors were not selected; no filter excludes a failing selected test or a Postgres dependency.

Implementation-reported RED causes are preserved in the candidate remediation evidence; review did not modify immutable source to reproduce RED. No release smoke, full benchmark, complete language journey suite or broad aggregate ran for this remediation. Prior smoke proved unchanged boundary availability at the old candidate, not changed producer metadata or post-drain counter placement at this candidate. Source-backed closure assessment remains necessary alongside green focused checks.
