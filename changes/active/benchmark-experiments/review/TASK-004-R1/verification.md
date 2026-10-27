# TASK-004 review verification

Subject: base `0b500d677531ba501924b1c6d412fe14ff9aa5cd`, candidate `a020500e8ea0c39311d508d6b5bfa85ebe90368c`. All runtime commands ran sequentially, against unchanged committed source. Review artifacts and pre-existing benchmark records are untracked; no source edits or `--allow-dirty` were used. Spec approval persists for revision 8 SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`.

## Independently rerun results

| Check | Result |
|---|---|
| V1 bounded HDR interval/precision test | 1 passed |
| V2, V3, actual V2a server owners and metrics owner tests | 16 passed |
| Gateway existing owner wait test | 1 passed |
| Bifrost Gate lifecycle and durable append tests | 2 passed |
| V4/V5/V6 exact selectors under repository Postgres | 7 passed |
| Entire `wyrd-testing --lib --bins` under same Postgres lifecycle | 91 passed, 0 skipped |
| SQL migration prerequisites | Both owners passed |
| `mise run bench:smoke` after all tests completed | Exit 0, 85.63 seconds, all dispatched paths passed |
| Workspace formatting check (`--check`, no edits) | Passed |
| `check:deps` | Passed, including its security-inline gate |
| Base-to-candidate whitespace check | Passed |

Exact test commands:

```bash
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(=tests::benchmark_histograms_preserve_bounded_intervals)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=app::metrics::tests::benchmark_timings_preserve_server_boundaries) | test(=app::metrics::tests::benchmark_snapshots_are_local_read_only_and_opt_in) | test(=verification::telemetry::tests::union_counts_overlap_once) | test(=verification::telemetry::tests::overhead_is_the_execution_less_its_blocks) | test(=verification::telemetry::tests::abandoned_direct_execution_finishes_cancelled) | test(=verification::hosted::blocks::tests::histogram_overhead_excludes_real_overlapping_blocks) | test(=components::gateway::telemetry::tests::histograms_preserve_head_and_final_overhead) | test(=query::routes::tests::histograms_cover_arrival_through_terminal_handoff) | test(=grpc::otlp::tests::histograms_cover_service_call_through_acknowledgement) | test(/^app::metrics::tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=engine::tests::execution_timing_separates_provider_wait_from_adapter_work)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::query_request_lifecycle_reconciles_exact_terminal_outcomes) | test(=gate::tests::histogram_write_boundary_includes_durable_append)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::histogram_cohorts_preserve_measurement_and_drain_windows) | test(=experiment::tests::histogram_records_preserve_method_and_legacy_reports) | test(=experiment::tests::metrics_preserve_process_scope_and_units) | test(=experiment::tests::repetitions_exclude_warmup_and_bound_execution) | test(=experiment::tests::reports_regenerate_from_persisted_records) | test(=capacity::tests::shared_measurements_preserve_workload_deltas) | test(=capacity::tests::smoke_names_every_missing_piece)' && cargo nextest run --locked -p wyrd-testing --lib --bins"
mise run bench:smoke
mise exec -- cargo fmt --all -- --check
mise run check:deps
mise exec -- git diff --check 0b500d677...a020500e8
```

Actual hosted-block selector is recorded above; no nonexistent selector was used. Passing tests are evidence of their exercised behavior, not a substitute for checking production call paths and original acceptance obligations.

## Release smoke observations

The task builds through its existing `bench:build` dependency (cached build completed), then dispatches fresh repository Postgres lifecycles. Eight smoke invocations passed: three Bifrost L1 experiments plus Verification/Gateway L1 and all three L2 engines. Output reports each Bifrost L1 invocation under the same `smoke l1 bifrost` label; the table records invocation order rather than inventing a per-output experiment label.

| Path | Boundary observations | Server CPU / cores | Work evidence |
|---|---|---|---|
| Bifrost L1 invocation 1 | 2 traces, 2 logs, 2 native writes, 1 query | 0.060 s / 32 | 15,333,191 memtable bytes |
| Bifrost L1 invocation 2 | 2 traces, 2 logs, 2 native writes, 1 query | 0.050 s / 32 | 15,325,951 memtable bytes |
| Bifrost L1 invocation 3 | 2 traces, 2 logs, 2 native writes, 1 query | 0.060 s / 32 | 15,333,455 memtable bytes |
| Verification L1 | 4,280 direct | 1.270 s / 32 | 4,280 expected verdicts, 37,670 runtime component observations |
| Gateway L1 | 1,920 buffered, 1,244 stream head, 1,244 stream end | 1.880 s / 32 | 3,164 validated calls/captures, 364 outbox writes |
| Bifrost L2 | 2 traces, 2 logs, 2 native writes, 1 query | 0.050 s / 32 | 15,326,519 memtable bytes |
| Verification L2 | 4,361 direct | 1.270 s / 32 | 4,361 expected verdicts, 38,499 runtime component observations |
| Gateway L2 | 5 buffered, 1 stream head, 1 stream end | 0.050 s / 32 | 6 validated calls/captures, 17 outbox writes |

The driver enables the existing opt-in snapshot with the server's `1` flag; task evidence and source show operator trace export remains absent by default. Smoke validates available real boundaries/resources. It does not claim long-run cohort accounting, interleaved class attribution, persisted interval-artifact completeness or baseline qualification.

## Limits

Implementation evidence reports fmt, workspace lints, dependency check and expected RED/diagnosis history. Review does not mutate source to recreate RED. No full L1/L2 capacity runs, baseline promotion, profiling or platform matrix were run. Source findings and independent validation determine whether green smoke/tests establish the complete task. No measurement overlapped Cargo/build/test work.
