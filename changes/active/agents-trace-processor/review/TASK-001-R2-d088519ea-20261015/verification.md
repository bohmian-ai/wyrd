# Fresh verification — R2 re-review

Candidate `d088519ea846aba8b2a77c24b2af2491ad2b3936`; original base
`dac4c340f835676adc29f3dcc01f98e1d3d51465`. All execution below is by the
orchestrator against current existing development artifacts, separate from
committed original/R1/R2 evidence. No build/install/source edit was needed.

## Completed focused proof

1. `git diff --check dac4c340f835676adc29f3dcc01f98e1d3d51465 d088519ea846aba8b2a77c24b2af2491ad2b3936`: PASS.
2. Python R2's exact combined runtime selectors: **16 passed in 50.86s**, no skipped tests:

```sh
WYRD_LOG=info mise exec -- uv run --directory sdks/wyrd-sdk-python --no-sync python -m pytest -q tests/unit/state/test_observe_surface.py::test_managed_telemetry_flush_deadline tests/unit/state/test_observe_surface.py::test_managed_telemetry_lifecycle tests/unit/state/test_observe_surface.py::test_owned_provider_correlation tests/unit/state/test_observe_surface.py::test_telemetry_setup tests/unit/state/test_observe_surface.py::test_telemetry_setup_names_the_otel_extra_without_the_sdk tests/unit/state/test_observe_surface.py::test_application_provider_is_never_flushed_by_state
```

3. TS telemetry unit file: **5 passed** (584ms test / 1.07s total):

```sh
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts
```

4. Existing clean packed package runtime/strict declaration consumer: **PASS**.

```sh
mise exec -- node scripts/check_ts_package.mjs
```

It packed `wyrd-ai` with current host `wyrd-ai-linux-x64-gnu`, installed/imported
the compiled facade and checked the clean consumer without optional peers.
Its existing temporary-project cleanup completed. No manifest/checker changed.

5. Transient documentation dependency closure: **PASS**. Executed with
`mise exec -- node --input-type=module` and a heredoc script, no permanent script.
It parsed the current Observe TS `npm install` row, collected all seven direct
external imports from the current emitted `dist/otel.js`, required every one
in that command, copied that unchanged emitted module to a temporary ESM
project and linked exactly those seven direct packages from the existing SDK
installation. Import succeeded with exports `logExporter`, `metricExporter`,
`runCorrelationProcessor`, `spanExporter`, `startTelemetry`. The temporary
project was removed in `finally`. This verifies current direct import closure
using installed versions, not a fresh registry install or a new network/dependency
compatibility matrix. The committed R2 clean-install probe supplies that
historical additional evidence. It does not replace generic no-optional-peer
package proof above.

## Environment-backed proof in progress

The original task's exact framework journey files are running with traced,
repository-managed isolated Postgres. Results will be appended; no success is
presumed here.

```sh
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python --no-sync python -m pytest -q -m integration tests/integration/otel/frameworks/test_framework_telemetry.py && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/agents-trace-processor.test.ts'
```

Migration selected exactly
`pg_migration::pg_tests::migrations_apply_and_are_idempotent`: 1 passed,
105 unselected. No concurrent Cargo execution was started. Framework results
cover actual supported frameworks through real server/readback, including the
merged-main publisher/attribution surface; they are the capability's two named
files, not the repository-wide journey aggregates.

## Limits

Broad format/lint/type/codegen/dependency/docs and whole-language unit aggregate
results are committed implementation evidence, inspected against source and
selectors; they have not been rerun by the review orchestrator. The task does
not own final integrated change-review aggregate selection. No live model
provider, production credential, synthetic host load, source/index edit,
staging, commit, merge, push or deploy was performed during review.

## Environment-backed results

The preceding combined command completed with exit 0. Python framework file:
**13 passed in 65.33s**, covering three cases each for managed, owned, execution
edges and sampling/privacy/authorization, plus one ambient-auth gateway case.
No skips. TS framework file: **5 passed**, all five named capability cases,
8.57s tests / 9.00s total. Actual Pydantic AI/ADK/OpenAI Python and OpenAI TS
builders used local scripted models and real Wyrd server/query boundaries.
Server logs show normal shutdown; the repository Postgres wrapper completed
its managed environment lifecycle. No trace failure needed diagnosis.
The earlier “in progress” heading is retained as execution provenance; these
final results supersede its pending status.

## Existing OTLP attribution journey — FAIL

Follow-up located the existing publisher-separated replay journey. Root executed:

```sh
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)"'
```

Migration: 1 passed. The exact journey selected 1 and **failed** in 2.310s
(15 unselected), exit 100. Cargo rebuilt affected dependencies/test binary in
1m30s; this supersedes the earlier statement that review needed no build.
No explicit setup/install or source changes occurred. Failure after normal
server startup and accepted exports is the query
`SELECT trace_state, principal_id, run_id FROM vala.traces.spans WHERE scope_name = 'wyrd.tests.otlp.negative.trace'`.
The authenticated public query returns a schema error: **No field named
principal_id**, suggesting `sub_principal_id`. Source consumer is
`tests/bifrost/otlp/negative.rs:433,440`; the panic is in shared
`support.rs:1046` query-unwrapping. Failure teardown logs report abort after
unwinding, not an ingest-availability cause. The obsolete readback is independently
identified in followup-review.md; no baseline checkout or blame comparison was
used. This red existing test blocks completion even though the Python/TS
framework files passed. It does not dynamically reproduce the distinct-actor
collision: source/follow-up establish that separate proposed path.

A second execution with `WYRD_LOG=info,vala_bifrost_redux=debug` is in progress
before final diagnosis; its result will be appended. No correction is applied
inside this immutable review.

Debug rerun completed with exit 100: migration 1 passed, exact journey 1 failed
in 1.834s (15 unselected). Same command with
`WYRD_LOG=info,vala_bifrost_redux=debug`; full local output temporarily retained
at `/tmp/wyrd-r2-otlp-attribution-review.log`. Traces show canonical gRPC dispatch,
Scribe group fence/write/commit and spans publication proceeding; Oracle then
refuses SQL planning for the obsolete `principal_id` column. This confirms a
readback consumer/schema mismatch rather than export timeout or service outage.
No second actor scenario is exercised by this old direct-publisher test.
Source/index/HEAD remained unchanged after both runs. No Cargo session remains.

## Final packet integrity

A transient artifact check confirmed all 16 required packet files are nonempty,
final verdict/task relative links resolve, all four retained stable IDs occur
in both final artifacts, validator and verdict cover all 21 original obligation
IDs, and prospective shell blocks pass `bash -n`. Final HEAD is still
`d088519ea846aba8b2a77c24b2af2491ad2b3936`; tracked working-tree and index diffs
are empty. Only this current review directory is untracked. No implementation,
staging, commit, merge, push or deploy occurred. Independent validator's final
report is complete and all material proposals are disposed; no agent/test
session remains active for this review.
