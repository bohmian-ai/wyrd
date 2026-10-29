# Integrated verification

Subject: `3899b0a35ec2eede7bf294db0cf66b27defa1498` →
`462ebee97c81772103054f7d090053ac7054bafa`.

## Completed aggregate

Command: `mise run -c gate`.
Raw log: `/tmp/wyrd-change-462ebee97-gate.log` (outside the reports-only directory).
The existing task graph uses one job and sequential Cargo work; its aggregate
owns full non-credentialed surface journeys and avoids duplicate component runs.

Final result: **exit 100**, 3,760.75 seconds (62 minutes 41 seconds).
Six failed tasks: `test:wyrd`, `test:bifrost:gate`,
`test:rust-sdk:journey`, `py:test:integration`, `ts:test:integration`,
`test:local-dev:journey`. All other aggregate leaves completed; no failed
assertion, timeout, gate or test was edited, skipped or waived.

At the initial checkpoint, format, workspace-hack, both Clippy commands,
dependency/tenant/storage-drift/CI-selection checks, Rust documentation,
example compilation and code generation completed without failure. Both fresh
Postgres migration checks passed. The Wyrd family selected 2,419 tests across
32 binaries. This is not a claim that the aggregate has passed.

Wyrd family final result: 2,418 passed, 1 failed, 296 skipped (225.192 seconds).
The failure is `wyrd-testing` library
`bifrost::cluster::tests::cluster_restart_rederives_same_plan_from_retained_snapshot`,
at `cluster.rs:3013`: left `322122546`, right `161061273`.
Its retained trace shows original/restarted server resource declarations of
3 effective CPUs and 3,221,225,472 managed-memory bytes; the reviewer is tracing
the budget mismatch before prescribing a correction. No source/test change or
failure waiver occurred. The aggregate's continue mode proceeds to other lanes.

Skald family: 335 tests passed, 0 skipped (0.803 seconds).

Further completed aggregate results:

| Slice | Actual result |
|---|---|
| Vala family | 1,176 passed, 23 skipped (79.460 seconds) |
| Shared family | 819 passed, 169 skipped (23.992 seconds) |
| Native gateway | 14 passed, 2 skipped |
| Vault gateway | 2 passed |
| Identity server | 31 passed |
| Identity browser | 4 passed |
| Identity Python / TypeScript | 7 passed / 7 passed |
| Docs unit tests | 102 passed; docs command/link/build/accessibility tasks completed |
| Ignored Bifrost/MCP aggregate | 266 passed, 8 failed, 118 skipped (668.939 seconds) |
| Full Rust SDK journey aggregate | 136 passed, 2 failed, 8 skipped (204.040 seconds) |
| Full Python integration | 331 passed, 1 failed, 2 skipped, 706 deselected (754.15 seconds) |
| Python harness / TensorFlow | 2 passed / 4 passed |
| TypeScript integration | 257 passed, 2 failed, 3 skipped across 31 files (264.05 seconds) |
| TypeScript unit | 81 passed across 11 files |
| Storage emulator matrix | All selected signer, handle and server e2e tests passed; local, S3 multipart, GCS single PUT and Azure block coverage |
| Installed CLI initial Rust slice | 6 passed |
| Installed CLI first Python scenario | Failed in PTY observer after child exited 130; later inner commands were not reached |

Bifrost/MCP failures: MCP staging pressure; two Scribe claim-budget concurrency
tests; Scribe telemetry freeze; stage-pressure/restart; scheduled-query audit
retention; failed-WAL-replay byte preservation; grouped-source query counts.
Rust SDK failures: custom-data retained query uses retired `outcome` payload
column; staging/refusal recovery result-count assertion. Detailed diagnoses
and correction boundaries belong to the independent source review, not this
execution summary. No test, source, timeout or assertion has been modified.

Python staging failed while declaring fixtures with a capacity-six outbox,
before its intended pressure scenario. TypeScript staging corroborated the
Rust group-input population mismatch (seven rows versus six). TypeScript's
policy-review story also received an EngineError::Busy projection; its exact
producer is unresolved in the full-run output and is being isolated before
any correction is prescribed. Installed CLI's failure is `termios` ENOTTY
when reading a slave PTY descriptor after reaping its controlling-session
leader; source diagnosis and observer probes are distinguished below.

## Focused post-aggregate diagnostics

An exact eleven-selector Rust follow-up ran sequentially against fresh
repository Postgres with debug tracing and no source changes. Result:
**10 failed, 1 passed, 331 skipped**, 196.212 seconds. It reproduced the
retired-column query, staging population, MCP setup, cache accounting,
both claim-budget, telemetry freeze, stage-pressure settlement, WAL snapshot
and remote audit observer failures. Group lookup passed when its extra
visibility-deferred input read did not occur; that confirms the full-run
global-count assumption depends on scheduling rather than weakening the
original occurrence assertions. Raw log:
`/tmp/wyrd-change-462ebee97-focused.log`.

Exact executed command:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_testing=debug,wyrd_server=debug,vala_bifrost_redux=debug cargo nextest run --locked -p wyrd-testing --lib --test scribe --test server -p wyrd-mcp --test mcp -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E 'test(=bifrost::cluster::tests::cluster_restart_rederives_same_plan_from_retained_snapshot) | test(=verification::pg_tests::staging_pressure_is_retryable) | test(=lifecycle::scribe_publishes_due_claims_concurrently_within_the_claim_budget) | test(=lifecycle::concurrent_flushes_share_the_claim_budget_and_publish_each_claim_once) | test(=telemetry::scribe_hot_path_telemetry_reconciles) | test(=write_read::acknowledged_rows_survive_stage_pressure_and_restart) | test(=query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup) | test(=owner_inspection::failed_wal_replay_is_role_local) | test(=verification_runtime::group_lookup_preserves_source_occurrences) | test(=verifier_building_blocks::custom_verdict_data_survives_readback) | test(=verify_in_real_time::verification_staging_refuses_and_recovers)'" > /tmp/wyrd-change-462ebee97-focused.log 2>&1
```

PTY observer probes (separate from CLI behavior proof) ran read-only on Darwin
arm64 and a Linux Python container. After the controlling child exited 130,
the master retained readable ECHO state; the Darwin slave returned ENOTTY.
Negative and positive controls on both platforms proved that the master reports
ECHO false when the child leaves it off and true when explicitly restored.
Thus the existing master is a viable observer without ignoring errors,
dropping the echo contract or introducing a supervisor. The implementor still
must prove actual CLI signal behavior through the original focused journey.

## Feature closure from actual aggregate selection

Although the family command passes no explicit test-support flag, its shared
testing dependencies unify the feature. Actual aggregate output includes the
hosted Artifact metadata/reuse tests, grouped selectors, bounded readiness and
fitter shutdown/release scenarios as passing. Check actual selected results
for complete closure rather than infer omissions from command-line flags.
Do not rerun a separate feature slice for scenarios already selected and passed.

### TypeScript isolation

Command:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && WYRD_LOG=info,wyrd_server=debug,vala_verify=debug,wyrd_testing=debug pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts tests/integration/verify-in-real-time.test.ts --no-file-parallelism --testNamePattern '^(policy review runs every action and check kind|verification_staging_refuses_and_recovers)$'"
```

Result: policy story passed (4.334 seconds); staging story failed with the
same exact seven-versus-six population (31.047 seconds); 52 tests excluded by
the exact filter. Exit 1. Raw log:
`/tmp/wyrd-change-462ebee97-ts-focused.log`.
The isolated success does not erase the full-suite Busy refusal. Its producer
was not established, so no speculative runtime change, retry or limit increase
is prescribed. It remains an explicit verification limit for final closure.

### Python pressure isolation

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && WYRD_LOG=info,wyrd_server=debug,wyrd_testing=debug uv run python -m pytest -q tests/integration/test_verify_in_real_time.py::test_verification_staging_refuses_and_recovers"
```

Result: failed in 7.61 seconds at fixture declaration, raising the same
registration-no-longer-pending error after outbox shedding. It did not reach
the later population assertion. Exit 1. Raw log:
`/tmp/wyrd-change-462ebee97-py-focused.log`.

### Installed CLI observer isolation

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && WYRD_LOG=info,wyrd_server=debug,wyrd_testing=debug uv run python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_guided_setup"
```

Result: failed in 1.13 seconds at the same slave-PTY `tcgetattr` with ENOTTY,
after the CLI's exit-130 check succeeded. Exit 1. Raw log:
`/tmp/wyrd-change-462ebee97-cli-focused.log`. The separate master-descriptor
positive/negative probes above validate the smaller observation correction;
they do not substitute for the implementor rerunning the actual CLI journey.
General terminal API reference: [tcgetattr manual](https://man7.org/linux/man-pages/man3/tcgetattr.3.html).

## Limits

No benchmark run/extension/comparison is required under the maintainer's
current instruction. Timing remains unmeasured. Real-GCS multipart was not
available in recorded task evidence; the full emulator matrix passed, while
live-cloud storage was not run (no configured live test storage target and
missing GCS credentials). Later commands inside installed-CLI journey did not
run after its first Python failure; those scenarios are not reported passed.
No source edits, commits, branch/PR updates, push or deployment occurred.
