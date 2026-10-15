# Maintainer review — TASK-001 / R2

Result: **PASS**. Proposed material finding ledger: **empty**.

## Immutable subject and authority

Reviewed the cumulative task from `dac4c340f835676adc29f3dcc01f98e1d3d51465`
through `d088519ea846aba8b2a77c24b2af2491ad2b3936`, including original TASK-001,
approved spec revision 2, R1 and R2 remediation packets and their committed
evidence. Used `1b769759d..d088519ea` for R2 navigation and
`cb29781..d088519ea` only to distinguish task work from the explicitly authorized
main merge. Neither supplemental range replaced the original subject.

Authority: AGENTS.md §§2–11 and §16; agent rules; active design's client,
OTLP/provider and observation-identity contracts; doctrine's runtime and public
surface boundaries; Bifrost ingest/durability/query boundaries; reference router;
maintainer style; spec-driven development; Python API/stubs, TypeScript and PyO3
guidance; telemetry-observations; docs CONTRIBUTING and PRODUCT. CodeGraph was
used before source navigation. No current peer discovery report was read.

## Changed-surface coverage

| Surface | Symbols, owners, callers and proof inspected | Assessment |
|---|---|---|
| Python managed setup | `PyWyrdState` fields/construction, `start_telemetry`, `__traverse__`, `__clear__`; `_start_telemetry`; setup repeat/overlap/other-state/application/lost-race and import-retry cases | The per-state gate owns initialization only. `OnceLock<Py<PyAny>>` remains the sole retained handle and GC authority. Interpreter-aware lock waiting and the under-lock recheck are local to the actual owner. Failed setup remains retryable; losing global installers release their own provider. |
| Python flush coordination | `_ManagedTelemetry.__init__/force_flush/_flush`; native `flush_telemetry`, public `flush/shutdown`; real-stock held-export and stale-settlement tests | The retained provider, current Future and operation number form one cohesive owner. The monotonic deadline is shared across waits; a later invocation requires a covering operation. The worker performs the stock provider operation and introduces no export queue, token state or replacement transport. Both lifecycle consumers use the same owner. |
| Python framework bridge | `WyrdAgentsTraceProcessor` callback methods, `_start/_end`, `_nanos`, `_span_name`, `_gen_ai`; optional imports; Run context helpers and correlation installation | Map/lock/closed state belongs to the processor. Attribute collection happens at completion; scope is captured at start. Ordinary OTel providers own sampling/export. Callback lifecycle, threading, unknown data, safe errors, zero Responses counters and withheld-content usage have focused runtime tests. |
| Shared error ownership | `TelemetryFlushFailure` constructors, `into_error`, `settle`; catalog variants/details projection; Python and TS consumers; inline Rust settlement test | Payload-free stage failure is represented once in the shared client. The value does not acquire runtime resources or duplicate the native drain. The error catalog remains authoritative. Rustdoc explains precedence, secondary native error and intent. |
| TypeScript root API | Local `AgentsSpanData/AgentsUsage/AgentsSpan/AgentsTrace`, `WyrdAgentsTraceProcessor`, timestamp/name/attribute mapping; existing Run scope; `WyrdState.startTelemetry/#flushTelemetry/#settle/flush/shutdown` | Structural callback types express the fields read without forcing optional framework types into every consumer. State owns its installed provider. The post-import recheck covers the only asynchronous same-state install interval; no parallel initialization subsystem is introduced. |
| TypeScript OTel module | Exporter factories/config, `runCorrelation`, `runCorrelationProcessor`, `startTelemetry`; application-owned provider and managed-state callers | Reuses stock exporters, existing per-export client auth and construction-time processor attachment. The stateless processor factory exposes an existing capability rather than a second correlation implementation. Refusal shuts down only the losing provider. |
| Declarations and packaging | Python state authored stub; Python extras/dev dependencies/floor and abi3 metadata; TS package peer metadata/lockfile, generated native declarations in both module formats, native `telemetry_flush_error`; package checker consumer | Native error projection and generated declaration signatures agree. Python lifecycle documentation reflects the native signature/behavior. The clean-consumer check directly guards optional dependency closure at the published root surface. Framework production dependencies remain optional; Python floor/dev-group changes are recorded as owner-directed. |
| Unit proof layout | Existing Python state test owner, TS telemetry test owner, shared client inline test | Runtime-dependent tests stay in their language. Tests assert caller-visible codes, correlation, provider identity, sampling, lifecycle and exported spans. Added coordination proof extends the relevant owner rather than creating an unrelated harness. |
| Real framework proof | Python `otel/frameworks/app.py` builders/scenarios and test module; TS `agents-trace-processor.test.ts`, `observed-service.ts` fixture and its consumers | Actual framework agents use scripted HTTP upstreams. Python process isolation earns the helper application because providers are global and ADK captures its tracer at import. Managed live flush is read back before continuation; owned-provider, negative, streamed, nested/concurrent and ambient gateway scenarios remain distinct. Shared TS fixture changes preserve the existing default fixture export. |
| Lane selection | Python integration recursion/dev installation and removed special framework group/lane; Bifrost Python/TS journey lists; native setup dependencies | Selection names real telemetry journeys and retains MCP journeys as separate behavior. No skip or zero-selection substitute was added. Dependency and runtime ownership are visible in package/mise metadata. |
| Durable and user documentation | Observe recipes/prerequisites/readback/owned provider/failure table; Connect/Understand/Quickstart edits; design provider contract | Builders, scoped execution and readback are concrete. The corrected TS install includes all static OTel entrypoint imports, including logs and metrics exporters. Lifecycle ownership, separate provider credentials, payload limits and Python refusal-reporting limitation are documented. Draft state and language tabs remain. |
| Authorized-main identity/ingest/query seams | Shared `Attribution` and runtime projection; `Caller` and Gate `AuthContext`; managed-column declarations and Scribe admitted/decode/stamping flow; client TableConfig/observation/workflow updates; server outbox/peer and test-server fixture adaptations | New SDK glue does not independently resolve publisher identity or stamp tenant/publisher columns. These remain verified server-owned values. Optional Card/Run scope stays client correlation, distinct from publisher attribution. Main's added subject/actor fields do not change callback types or Run scope storage. |

## Existing-owner comparison

`BifrostLifecycle` and the Bifrost facade already own writer startup, drain and
terminal closure. They cannot replace `_ManagedTelemetry`: they neither hold
the foreign-runtime tracer provider nor perform its stock flush, and their
writer closure semantics intentionally differ from the retained global
provider. The candidate calls those existing native owners after the runtime
provider step instead of reproducing their queues or state machine.

The Python setup cell already owned successful provider publication. Extending
its initialization boundary with `MutexExt::lock_py_attached` preserves that
ownership and the installed native mechanism; replacing the cell or moving GC
through a Python-attached accessor is unnecessary. The Future/thread boundary
is narrowly justified by the stock synchronous processor ignoring its nominal
timeout. The new operation number prevents obsolete-result success; it is not
a span tracker or a second exporter owner.

Existing Run/context correlation, shared client credentials and stock exporter
factories remain the mechanisms reused by native frameworks and the explicit
OpenAI bridge. No new framework discovery, framework runtime, shared registry,
token cache, retry machinery or server contract was found in the task-owned
changes. Imported-main attribution also uses its established server owner;
the task does not fork it in either SDK.

## Remediation closure and findings

The maintainer-relevant original causes remain addressed in source: root TS
types no longer import optional Agents types; docs construct agents and state
their full dependencies; real live-flush proof remains; Python Responses
mapping retains safe usage fallback and zero; bounded Python flush and fresh
covering operation share one owner; TS overlap is handled at the await boundary;
Python overlap is handled across fallible native initialization with retry and
GC preservation. No additional material maintenance defect or unearned owner
requiring deletion was established. No style-only blocker or optional redesign
is proposed.

## Verification limits and source guard

This was a read-only static review except for this report. No builds, tests,
installs, staging or source edits were performed. Committed execution evidence
is an input claim; the root's fresh verification is separate. The authorized
main merge was inspected for telemetry/identity/ingest/query/lifecycle and test
interactions, not presented as a new independent audit of every unrelated main
feature. No missing task-owned surface was identified in this maintainer pass.

HEAD was `d088519ea846aba8b2a77c24b2af2491ad2b3936` at entry and after source
inspection. Only the current review directory was untracked; other reviewers'
files were preserved.
