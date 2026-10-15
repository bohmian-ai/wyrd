# Maintainer review

Result: **FAIL**

Artifact: complete cumulative `dac4c340f835676adc29f3dcc01f98e1d3d51465..692d390dc40e43c6e1d7e25c7850d5c826d26a9e`, SPEC-agents-trace-processor r2 and TASK-001.

The runtime additions mostly extend the existing owners; the material maintenance costs are a mandatory framework type dependency leaking into the ordinary TypeScript client and incomplete public framework recipes.

## Authorities and changed-surface coverage

Read the immutable subject, approved specification, original task and its execution evidence, AGENTS.md, agent rules, the reference router, spec-driven-development and maintainer-style guides. Applicable slices inspected: architecture patterns, Rust core, PyO3 boundaries, Python API/stubs, TypeScript guide, errors, testing workflows and telemetry observations; Wyrd design doctrine/client model, runtime authentication, trace context, observation identity and Bifrost facade; public doctrine; docs CONTRIBUTING and PRODUCT. CodeGraph was the first code-navigation tool, followed by source/caller searches.

| Changed surface | Changed symbols and connected source/tests inspected | Assessment |
|---|---|---|
| Python OTEL module | `install_run_correlation`, `_INSTALL_LOCK`, `_start_telemetry`, `WyrdAgentsTraceProcessor` constructor and every callback/private method, `_INTERRUPTED`, `_nanos`, `_span_name`, `_gen_ai`; surrounding `_RunCorrelationProcessor`, scope entry/exit, stock exporters/session and endpoint | Retains the existing context/export path. New instance state is earned by live framework span identity; lock protects shared callback state. No parallel exporter, authentication owner or Pydantic/ADK adapter. |
| Python state boundary and declarations | Full changed `start_telemetry`, `flush`, `shutdown`, `flush_telemetry`; owning `PyWyrdState`, retained `telemetry`, constructor, existing Rust state/native drain and boundary conversion; authored `state/__init__.pyi` and public package exports | Native API and authored flush/shutdown declarations agree. Managed-provider lifetime remains in its SDK owner; shared Bifrost drain remains in Rust. Changed fallible methods document their failure cases. |
| Shared errors | `TelemetryFlushFailure` fields, constructors, `into_error`, `settle`, new test; `WyrdError::SdkTelemetryFlushTimeout`/`SdkTelemetryFlushFailed`, metadata fields and `message_and_details` arms | Two real runtime consumers justify common catalog projection; value owns failure invariants without a new service or trait. Constructors, conversion, fields and test carry substantive rustdoc. |
| TS native projection | `telemetry_flush_error`, existing provider-exists projection, `NativeWyrdState` client/flush/shutdown and `NativeLifecycleResult`; generated `index.d.ts` and `index.d.cts` | Generated signature matches native source and stable error projection. New function has workflow rustdoc. No durable lifecycle copied into binding. |
| TS bridge and state | Full `WyrdAgentsTraceProcessor`, every hook/private method, `telemetryFlushFailure`, `AGENTS_INTERRUPTED`, `agentsTime`, `agentsSpanName`, `agentsGenAi`, `WyrdState.#flushTelemetry/#settle/flush/shutdown`; surrounding `otel()`, Run scope, state startTelemetry and stock exporter; emitted public declaration | Callback and provider owners remain cohesive. **MNT-001:** optional framework's public types escape into the root declaration. |
| TS OTEL facade | Full `runCorrelation`, `runCorrelationProcessor`, `startTelemetry`, exporter config/factories and callers | Exposes the existing stateless processor; no second correlation mechanism. Cleanup extends existing refused setup. |
| Python supporting proof | Full new/changed tests and helpers in `tests/unit/state/test_observe_surface.py`: optional imports, setup/refusal, registration retry, owned providers, lifecycle failures, token freshness, framework callback data and threaded starts/ends; existing scope tests and fixtures | Tests exercise public runtime owners and ordinary error boundaries. Helpers have concrete test consumers; no new general-purpose harness. |
| Python framework journeys | Full `otel/frameworks/app.py` and `test_framework_telemetry.py`, empty package initializer; modified MCP-framework helper signatures/call sites | Framework builders, local model, managed/owned process setup, failures, streaming, nested/concurrent scopes, readback, privacy and auth assertions inspected. Process separation is earned by once-only OTEL globals and ADK import-time tracer binding. |
| TS supporting proof and journeys | Full telemetry unit suite and agents integration suite, all helper functions; `observed-service.ts` factory and retained `observedServiceTest`, existing `serverTest`, HTTP receiver and existing observation-suite callers | Factory adds a real second setup consumer while preserving existing callers. Uses existing server/CLI registration/hydration fixture workflow. No unnecessary independent server harness. |
| Package, locks and collection | Python Cargo/pyproject/uv.lock changes, TS package and pnpm-lock changes, mise SDK/Bifrost and Python collection changes, entire package smoke-check context | Extras, Python floor/ABI and locks inspected; task records owner's Python-floor direction. Ordinary typecheck has framework dev dependencies, and clean package smoke proves runtime imports only, leaving MNT-001 uncovered. No new dependency is moved into a foundational crate. |
| Docs and architecture | Changed design export-helper/lifecycle paragraph; complete Observe page, Connect stock-client procedure, quickstart prerequisites and Understand cross-link; docs frontmatter/links/readback | Draft status is preserved; lifecycle prose and error codes match their boundaries. **MNT-002:** framework examples cannot be executed from the documented prerequisites. |
| Tracked change packet | Specification and task including owner instructions, reuse map, selectors, verification claims and deviations | Used as authority/evidence claims, not as proof that source or public recipes work. |

No source change introduces SQL, storage schema, provider execution runtime, client transport, token cache, queue, single-implementation trait or generic extensibility layer. Unchanged server data-plane implementation was not re-audited as a maintainer change.

## Material findings

### MNT-001 — REGRESSION — ordinary TypeScript consumers require the optional Agents SDK

- **Location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1`, `:2517`, `:2524`, `:2534`; manifest `sdks/wyrd-sdk-ts/wyrd/package.json:57` marks this peer optional.
- **Exact source:** `import type { Span as AgentsSpan, SpanData, Trace, TracingProcessor } from "@openai/agents-core";`, `export class WyrdAgentsTraceProcessor implements TracingProcessor`, and public callback parameters `Trace` / `AgentsSpan<SpanData>`.
- **Rule:** REQ-010 keeps framework dependencies optional; TASK-001 scenario 1 keeps generic imports usable without frameworks. Maintainer style “Python and TypeScript: document the typed contract” requires public declarations to describe a usable typed API; AGENTS.md first-class client and public typing obligations apply to ordinary clients as well as bridge users.
- **Producer → consumers:** the root facade exports the processor using external nominal declarations, so declaration emission retains an unconditional `@openai/agents-core` import. `package.json` routes every root consumer to `dist/index.d.ts`, including one importing only `WyrdClient`. The workspace's framework dev dependency hides this from `ts:typecheck`; `scripts/check_ts_package.mjs:146` only executes JavaScript after install, where type imports are erased.
- **Observed consequence:** an ordinary strict TypeScript project with the SDK, required `apache-arrow` dependency and Node types fails compilation without the optional Agents peer. An isolated no-emit consumer importing only `WyrdClient` produced the sole diagnostic `node_modules/wyrd-ai/dist/index.d.ts(1,76): error TS2307: Cannot find module '@openai/agents-core' or its corresponding type declarations.` Exit 2. No framework callback is called.
- **Smallest correction:** remove the optional framework dependency from the root emitted declaration while retaining the required root processor export. Use precise structural callback inputs for the fields the bridge actually consumes, and keep framework assignability checked in the existing framework-enabled tests. Do not make Agents a required dependency, change the approved import surface or turn off declaration checking. Extend the existing clean-package smoke proof with a tiny no-emit generic consumer without optional peers; retain an Agents-enabled typecheck proving the processor is accepted by `setTraceProcessors`.
- **Severity:** must-fix; this breaks a first-class public client for users who do not use the new integration.

### MNT-002 — MISSING — public framework recipes omit the objects needed to execute them

- **Location:** `docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md:158` through `:216`, especially Python `agent.run(question)` at `:175`, ADK `runner.run_async(...)` at `:186`, Python `Runner.run(agent, question)` at `:203`, and TS `run(agent, question)` at `:216`.
- **Exact source:** the Pydantic example imports `Agent` and enables instrumentation but never constructs `agent`; the ADK example supplies no runner, agent, session or message construction; both OpenAI examples invoke an undefined agent. The prerequisite paragraph installs `wyrd-ai[otel,agents]` and OTEL JS packages, but no Pydantic AI/ADK package or JS Agents package. The stock-client link builds an OpenAI client, not these framework objects.
- **Rule:** REQ-015 explicitly says “Show required framework construction or link a complete prerequisite; undefined agent/provider placeholders cannot be the only usable procedure.” Docs PRODUCT requires a complete executable developer task with a visible success path. Maintainer style requires docs to explain the public operation accurately enough to use and maintain it.
- **Concrete cost:** all four new framework procedures require the reader to reconstruct framework installation and object/model/session setup from outside the documented path. The snippets cannot establish the claimed integration, and a maintainer cannot reproduce a recipe from its stated prerequisites even though the test application already contains working construction.
- **Smallest correction:** complete each recipe's required framework install and agent/model construction, plus ADK runner/session/message setup, or link a complete explicit prerequisite that defines those exact variables. Reuse the public constructions already exercised by `tests/integration/otel/frameworks/app.py::{pydantic_ai,google_adk,openai_agents}` and TS `tests/integration/agents-trace-processor.test.ts::agent`; no new runtime adapter or harness is needed. Keep the ambient Wyrd client/export setup, content controls, model credential distinction, draft flag and existing query confirmation. Statically follow each procedure from installation to its existing readback, and run the owning docs check after the bounded correction.
- **Severity:** must-fix; this is an explicit public-procedure requirement, not a prose preference.

## What's actually simple here

- The bridge translates only framework callbacks into ordinary OTEL spans; native instrumentations retain their own owner and no proprietary normalized event protocol is introduced.
- Managed lifecycle sequencing extends the state wrappers and existing Rust drain; shared error construction is reused by two language boundaries rather than copied.

## Verification and limits

The original task records focused Python/TS runtime checks, integration checks and package/codegen/docs gates. Those records were inspected as claims and connected to source; this maintainer pass did not rerun those lanes, Cargo, builds, package installation or live-server journeys.

The orchestrator explicitly authorized one isolated read-only no-emit TypeScript proof. Via `mise exec -- /usr/bin/python3`, a temporary directory received copies of the current package metadata/declarations and `dist`, with only `apache-arrow` and `@types/node` linked from installed dependencies. `consumer.ts` contained `import { WyrdClient } from "wyrd-ai"; const client = WyrdClient.connect(); void client;`. The installed TypeScript 5.9.3 command was:

```text
node <repo>/sdks/wyrd-sdk-ts/node_modules/typescript/bin/tsc --noEmit --strict --target ES2023 --module NodeNext --moduleResolution NodeNext --types node consumer.ts
```

It returned the TS2307 diagnostic above and exit 2. Temporary files were automatically removed. An initial invocation using the unavailable `python` shim failed before executing the proof; the authorized retry used `/usr/bin/python3`. No repository source or package installation changed.

Final immutability check: HEAD remains `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`; `git diff --exit-code` is clean. Only review artifacts are untracked. This report is the reviewer's only written repository artifact.

Skipped broad verification and independent runtime re-execution; public declaration and recipe defects remain the identified risk.
