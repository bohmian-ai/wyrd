# R3 review verification

Pinned candidate: `1cf675c58d01cba0e714102c75cb7d89e7a678b5`.
Original base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
Fresh execution is root-owned and separate from committed original/R1/R2/R3
implementation evidence. Reports may inspect this file; pending status is not
success. No source/index/HEAD modification, install or explicit build was done.
Tests use existing development artifacts; Cargo may rebuild its affected targets.

## Running / completed commands

Cumulative `git diff --check` completed PASS.

The following commands are running sequentially (no concurrent Cargo):

```sh
WYRD_LOG=info mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::otlp_batch_id_tests::derived_identity_is_stable_per_tenant_table_and_payload)' && WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=state::tests::telemetry_flush_failure_settles_with_the_native_drain)'
```

First exact unit selector: selected 1, passed 1 (834 unselected), 0.009s test.
Second result pending.

Independent existing TS unit/package and Python runtime selectors are running:

```sh
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts
mise exec -- node scripts/check_ts_package.mjs
WYRD_LOG=info mise exec -- uv run --directory sdks/wyrd-sdk-python --no-sync python -m pytest -q tests/unit/state/test_observe_surface.py::test_managed_telemetry_flush_deadline tests/unit/state/test_observe_surface.py::test_telemetry_setup tests/unit/state/test_observe_surface.py::test_telemetry_setup_names_the_otel_extra_without_the_sdk tests/unit/state/test_observe_surface.py::test_agents_callback_lifecycle
```

Environment-backed attribution and Python gateway journeys will run next once
Cargo unit work completes. Broad static/format/lint/type/generated/docs and
whole-language/framework aggregates remain historical committed evidence; they
are not claimed as fresh review executions. Final integrated selection remains
change-review-owned. No live model/OIDC/cloud provider or production credential
is used; no synthetic host load or source modification is permitted by review.

## Focused results

Both Rust exact unit selectors PASS: derived identity selected1/passed1 (834
unselected), settlement selected1/passed1 (262 unselected, 0.015s). Sequential
Cargo execution finished. TS telemetry file: 5 passed (373ms / 602ms total).
Existing packed clean runtime/strict consumer: PASS without optional peers,
using the current linux-x64 native addon and existing script cleanup. Python
exact runtime selectors: 8 passed in 3.28s, including setup overlap/retry,
real-stock bounded/stale flush regression and safe actual Responses callback.

## Environment-backed command now running

```sh
WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)" && mise exec -- uv run --directory sdks/wyrd-sdk-python --no-sync python -m pytest -q -m integration tests/integration/otel/frameworks/test_framework_telemetry.py::test_managed_framework_telemetry tests/integration/otel/frameworks/test_framework_telemetry.py::test_ambient_auth_gateway_telemetry && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/agents-trace-processor.test.ts -t "^managed framework telemetry$"'
```

This selects the existing extended direct/delegated pair/replay Rust journey,
the three actual managed Python frameworks and their real ambient gateway
negative-code scenario, and the TS managed OpenAI live-flush journey. They use
repository Postgres/server/local scripted models, not a live provider. Results
pending until appended. All Cargo work remains sequential.

## Environment-backed results — PASS

Combined managed Postgres command exited 0. Migration selected1/passed1 (105
unselected). Existing Rust direct/delegated publisher attribution journey
selected1/passed1 (15 unselected), 1.821s; Cargo rebuilt only its test target
in 5.76s. This is dynamic pair/readback/replay proof under both gRPC and HTTP,
not only source reasoning. The journey retains five exact pair/Run row sets
with no duplicate marker per set and real authorized delegation.

Python selected managed three-framework cases plus exact gateway/credential-code
case: **4 passed in 19.38s**, no skips. TS selected managed OpenAI case:
**1 passed**, four unselected (runner labels them skipped), 4.571s selected
case / 5.19s total. Normal server shutdown completed; the Postgres wrapper
completed environment cleanup. These prove actual framework instrumentation,
live flush/readback/continuation and post-shutdown provider export, plus the
Python real-process missing-credential code and gateway credential separation.
All sessions are complete, no Cargo session remains and no failure needed
reproduction/diagnosis. Large TS debug output was tool-truncated; process
status and test summary were present and normal shutdown was observed.

The previously red exact Rust attribution journey now passes on the pinned
candidate. Broad aggregates/static lanes remain historical evidence; all
fresh passes are explicitly scoped above. Cross-version replay behavior is
source-review/contract assessment, not a rolling multi-version deployment test.

## Final packet integrity

All 14 required artifacts exist and are nonempty, including nine independent
discovery reports and fresh validation. A transient check confirmed final
verdict links resolve, validator/verdict each cover all 21 original obligation
IDs and all 12 prior stable finding closures, both return PASS, and no R4
remediation artifact is present. Final HEAD remains
`1cf675c58d01cba0e714102c75cb7d89e7a678b5`; tracked working-tree and index
diffs are empty. Only this new review directory is untracked. No source,
specification, existing task, staging or commit changed. No tool/agent execution
is pending for this review. Final verdict is based on independently validated
empty material ledger; the disclosed mixed-version retry risk remains explicit.
