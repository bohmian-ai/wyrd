# Framework-agnostic agent telemetry

- Change ID: `SPEC-agents-trace-processor`, approved revision 2.
- Completed: 2026-10-15.
- Reviewed target: not supplied; no final-change-review PASS is claimed.
- Completion target: `a223cde378c82617bccf29827c4f711ba5e66afb` (TASK-001 independent R3 task review PASS at `1cf675c58d01cba0e714102c75cb7d89e7a678b5`, then merged with `fix/main-attribution-tests` at `96bfced52`).
- Delivery reference: not supplied.
- Completion authority: owner-approved closeout on 2026-10-15 waiving final change review.

## Intent and behavior

Users connect existing agent instrumentation to Wyrd once, run their agents
normally, and read standard OpenTelemetry back through Bifrost. Each span can
optionally carry the exact Run and Card it belongs to. Wyrd ships no
framework runtime, registry, or monkey-patching.

- **Python.** Pydantic AI and Google ADK export their native OTEL spans. OpenAI
  Agents (Python `wyrd.otel.WyrdAgentsTraceProcessor`, TypeScript
  `wyrd-ai.WyrdAgentsTraceProcessor`) maps callbacks onto ordinary OTEL spans
  through the active global provider:
  - It keeps hierarchy, timestamps, model, and token usage. Responses usage is
    read even when content is withheld.
  - It sets payload-free error status and captures Run/Card scope at span
    start.
  - Duplicate starts and ends are idempotent, and late callbacks are ignored.
  - Shutdown ends unfinished spans with ERROR and never closes the provider.
- **Managed setup.** `state.start_telemetry()` / `state.startTelemetry()`
  installs one global provider. It is idempotent per state, including
  overlapping calls; Python serializes them with a GIL-aware gate. A provider
  installed by anyone else is refused with
  `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`, and a failed setup can be retried.
- **Application-owned providers.** They compose `span_exporter(client)` /
  `spanExporter(client)` with `install_run_correlation(provider)` (Python) or
  `runCorrelationProcessor()` from `wyrd-ai/otel` (TypeScript), and keep
  their own sampling, resources, and lifecycle.
- **Flush and shutdown.** State `flush`/`shutdown` first run a bounded flush
  of the managed provider, then always attempt the native Bifrost drain.
  - A timeout fails with `WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT`; any other
    provider failure fails with `WYRD_SDK_503_TELEMETRY_FLUSH_FAILED`.
  - When both stages fail, the native failure's code is kept as
    `native_code`.
  - Active spans and the global provider survive. In Python, each call covers
    spans that ended before it.
- **Authentication.** Exporters and stock gateway clients reuse the merged
  ambient-auth client and its per-request token path.
- **Server change.** Gate's derived OTLP batch identity now hashes both the
  verified subject and the current actor. Identical content re-exported by
  a delegated actor therefore keeps its own rows and actor stamp instead of
  being suppressed as a replay.
- **Docs.** The draft Observe page has an install table per framework, full
  Pydantic AI, ADK, and OpenAI Agents (Python and TypeScript) recipes, and a
  readback procedure.

## Lasting decisions and constraints

- OTEL/OTLP is the integration boundary, and adapters are local runtime glue.
  There is no new route, table, migration, Card kind, transport, export queue,
  token cache, or Pydantic/ADK adapter.
- Provider ownership stays explicit: setup never adopts an existing provider,
  and Run scope owns no provider lifecycle.
- Framework packages are optional runtime extras and ordinary dev
  dependencies. The Python floor is 3.11 (`abi3-py311`), because Pydantic AI
  and ADK require it. The TS public declarations do not import
  `@openai/agents-core`.
- An implicit OTLP replay identity is scoped to tenant, verified
  subject/actor pair, table, logical content, and accepted correlation.
  Native explicit batch ids and Scribe fences are unchanged. Every derived id
  changed with this change, so a retry spanning the deploy may commit once
  more.
- Known limits:
  - The Python OTEL SDK reports a refused export as a successful flush.
  - TS Chat Completions spans carry no usage.
  - TS `run.scope()` does not stamp an already-active enclosing span.

## Acceptance evidence and limits

| Obligation | Recorded evidence |
|---|---|
| AC-001, AC-007, AC-010 — real framework export, fidelity, edges | Python `test_framework_telemetry.py` journeys for all three frameworks (managed, owned, edges, live flush) and TS `agents-trace-processor.test.ts`; Python/TS callback lifecycle and concurrency unit tests |
| AC-008, AC-009 — setup idempotence, owned providers, flush/drain | `test_telemetry_setup` (repeat, overlap, other_state, application, lost_race), `test_managed_telemetry_flush_deadline`, `state::tests::telemetry_flush_failure_settles_with_the_native_drain`, TS telemetry unit tests, managed live-flush journeys (C6/C7) |
| AC-011, INV-001 — sampling, content, denied export, tenancy, publisher identity | Sentinel-absence and denied/cross-tenant framework journeys; `gate::otlp_batch_id_tests::derived_identity_is_stable_per_tenant_table_and_payload`; OTLP journey `identical_exports_from_two_principals_each_keep_their_own_attribution` (direct and delegated pairs over gRPC and HTTP) |
| AC-012 — docs | `docs:generate`/`docs:check`; clean-install probe of the documented TS peer set |
| AC-013 — ambient auth | `test_ambient_auth_gateway_telemetry`, which asserts exact `WYRD_CLIENT_401_NO_CREDENTIALS` for the credential-less child |
| AC-003 — static and package checks | After the merge, at the completion target: `fmt`, `lints`, Python and TS format/lint/typecheck, the TS package check, `ts:test:unit` (71), `py:test:unit` (613), the Gate and settlement unit tests, the OTLP publisher journey, the gateway journey, and the managed framework journeys (Python 3, TS 1) all pass |

TASK-001 went through three independent task reviews. Twelve findings were
remediated in R1–R3, and the R3 review passed with an empty ledger. No
independent final change review ran.

## Current authority, owners and tests

- [Protocol authority](../../../architecture/wyrd-design.md), [telemetry observations](../../../architecture/references/domain/telemetry-observations.md) and [user docs](../../../docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md).
- [Python OTEL glue](../../../sdks/wyrd-sdk-python/python/wyrd/otel.py), [Python state binding](../../../sdks/wyrd-sdk-python/src/state/mod.rs), [shared state flush settlement](../../../crates/shared/wyrd-client/src/state.rs), [TypeScript SDK](../../../sdks/wyrd-sdk-ts/wyrd/src/index.ts), [TypeScript OTEL entrypoint](../../../sdks/wyrd-sdk-ts/wyrd/src/otel.ts) and [Gate OTLP identity](../../../crates/vala/vala-bifrost-redux/src/gate/mod.rs).
- Tests: [Python unit](../../../sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py), [Python framework journeys](../../../sdks/wyrd-sdk-python/tests/integration/otel/frameworks/test_framework_telemetry.py), [TypeScript unit](../../../sdks/wyrd-sdk-ts/wyrd/tests/unit/telemetry.test.ts), [TypeScript journey](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/agents-trace-processor.test.ts), [OTLP publisher journey](../../../crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs) and [TS package check](../../../scripts/check_ts_package.mjs).
