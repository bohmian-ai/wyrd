# Maintainer review — TASK-001 / R3

Result: **PASS**. Proposed material findings: **empty**.

## Subject and scope

Reviewed cumulative task changes from `dac4c340f835676adc29f3dcc01f98e1d3d51465`
to `1cf675c58d01cba0e714102c75cb7d89e7a678b5`, including approved spec r2,
original TASK-001, R1/R2/R3 remediation requirements, historical findings and
the task-facing seams of the authorized main merge. Used `d088519ea..1cf675c58`
to locate R3 changes and `cb29781..1cf675c58` to distinguish task-owned changes;
neither supplemental comparison replaces the original cumulative subject.

Applied AGENTS.md, agent rules, maintainer style, spec-driven development,
reference router, active Wyrd design/doctrine and Bifrost ownership rules,
Python API/stubs, PyO3 boundaries, TypeScript, Rust ownership and telemetry
references, and docsite CONTRIBUTING/PRODUCT conventions. CodeGraph was used
before locating source. No current peer discovery report was read.

## Changed-surface coverage

| Surface | Owners, callers and proof inspected | Maintainer assessment |
|---|---|---|
| Python setup and lifetime | `PyWyrdState` construction, `telemetry`, `telemetry_setup`, `start_telemetry`, `__traverse__`, `__clear__`; `otel._start_telemetry`; setup repeat/overlap/other-state/application/lost-race and import retry tests | The native state retains one successful Python handle; the lock covers initialization rather than replacing the publication or GC owner. Failed attempts remain retryable and losing provider installations release their own resources. |
| Python flush coordination | `_ManagedTelemetry.__init__/force_flush/_flush`; native `flush_telemetry`, `flush`, `shutdown`; held stock export and obsolete-attempt tests | Provider, current Future and operation counter form one cohesive runtime owner. A later call requires a covering flush, and both waits share its deadline. Both native lifecycle consumers reuse the same settlement path. |
| Python framework bridge and context | `WyrdAgentsTraceProcessor` callbacks and `_start/_end`; timestamp/name/GenAI helpers; `_RunCorrelationProcessor`, installation marker/lock and Run entry/exit; callback, thread, Responses usage and optional import tests | State is local to the processor; final metadata is read at completion while correlation is captured at start. Pure mappings remain small functions. Existing OTEL context and provider mechanisms own sampling, ancestry and export. |
| Shared error projection | `TelemetryFlushFailure` constructors, `into_error`, `settle`; catalog variants; Python use; TS native `telemetry_flush_error`; inline settlement regression | One shared payload-free value determines primary/secondary error semantics. Native bindings remain projections and retain the existing writer drain. R3 adds the missing substantive Panics contract without changing assertions. |
| TypeScript root surface | Structural `AgentsSpanData/AgentsUsage/AgentsSpan/AgentsTrace`; bridge and mapping helpers; existing `Run.scope`; `WyrdState.startTelemetry/#flushTelemetry/#settle/flush/shutdown` | Callback declarations describe the fields read without importing the optional Agents SDK into root declarations. The post-import setup recheck addresses the asynchronous boundary locally. Provider state remains on the existing state handle. |
| TypeScript OTEL surface | Stock exporter factories and config; `runCorrelation`, `runCorrelationProcessor`, `startTelemetry`; owned/managed callers | The public factory exposes the existing stateless processor and uses supported construction-time attachment. Setup cleanup affects only the losing provider. Existing per-export credential retrieval is retained. |
| Typing, exports and packaging | Authored Python state stub and package exports; native TS error function and both generated declaration formats; TS root emitted declarations/optional peers; clean consumer package checker; Python metadata/abi3 floor/dev dependencies and lock changes | Source and declaration shapes agree. Root consumer checking extends the existing package smoke workflow rather than creating a parallel packaging subsystem. Optional framework imports and runtime-dependent tests stay at their language boundary. The Python floor change is recorded as owner-directed in task evidence. |
| Unit tests | Existing Python `test_observe_surface.py`, TS `telemetry.test.ts`, shared Rust inline test | Tests are placed with the owning runtime and assert caller outcomes: exact codes, correlation, provider preservation, timeout, retry and safe metadata. Coordination fixtures use the actual provider/processor boundary where its semantics matter. |
| Real framework journeys | Python `otel/frameworks/app.py` and parent module; TS `agents-trace-processor.test.ts`; `observed-service.ts` and retained default fixture consumer | Real frameworks use scripted upstreams and real server readback. Python process isolation is justified by process-global providers and ADK import binding. Live readback precedes continuation. R3 transmits the public exception code through the existing JSON report and retains nonzero exit, gateway assertions and isolated ambient credentials. |
| Lane/config ownership | Python integration recursion and moved development framework group; removed special MCP lane; Bifrost Python/TS leaf lists; package/native setup dependencies | Existing SDK and capability lanes explicitly collect the framework journeys. MCP scenarios remain separate proof, and runtime/dependency requirements are visible in existing manifests and tasks. |
| User and durable docs | Observe install/builders/readback/owned-provider/failure recipes; Connect/Understand/Quickstart cross-links; lasting provider lifecycle authority | Recipes name prerequisites, build actual agents and show query readback. Provider ownership, add-versus-replace semantics, sampling, native residual content and Python refusal reporting limits are stated. Existing draft flags and language tabs are retained. |
| Gate identity and R3 regression | `otlp_batch_id`, complete `dispatch_canonical`, all three `ingest_decoded_resource_*` callers; derived-ID regression; OTLP negative journey and seven-file sibling sweep | The publisher pair is added once at the shared implicit identity producer. The helper is a pure calculation and earns no new state owner. Rustdoc identifies excluded request/time fields and pair semantics. The readback uses current subject/actor columns and groups by pair/run, preserving per-marker duplicate checks. |
| Authorized-main seams | `Attribution`, `AuthContext::attribution/from_verified`; Scribe ingress propagation/stamping and logical managed-column exclusion; native explicit-ID dispatch; outbox frame/identity ownership; query projections and test-server delegation/token helpers | SDK enrichment still cannot choose publisher or tenant. Gate reuses the verified attribution owner used by Scribe, while explicit native IDs and internal outbox identities retain their separate existing contracts. No replacement identity resolver or fence owner was introduced. |

## Existing-owner comparison

The native state/Bifrost facade already owns buffered writer drain and terminal
closure. It does not retain a Python or Node tracer provider, and its shutdown
semantics differ from the required retained global provider. The foreign-runtime
flush step therefore belongs beside the existing runtime state handle. It calls
the native lifecycle afterward; it does not recreate its queue or state machine.

The Python wrapper's worker is justified by the installed synchronous stock
processor's failure to enforce its nominal force-flush timeout. Its one active
attempt and covering-operation counter do not constitute another exporter,
span queue, auth cache or retry policy. Existing OTEL processors/exporters still
perform the actual work. TS uses its provider's existing configured bound.

Existing Run context and correlation processors serve both native-framework
instrumentation and the explicit OpenAI bridge. The bridge is needed because
OpenAI's callbacks are not OTEL spans; no Pydantic/ADK adapter, framework
discovery or common proprietary event schema was added. Stock export factories
continue asking the selected shared client for current credentials.

Gate already owns implicit OTLP batch derivation. Extending that producer with
`auth.attribution()` is sufficient for all three signals and both transport
paths. Scribe owns stamping and replay fences; teaching consumers to detect
actor collisions would duplicate that authority. The existing outbox also hashes
the pair, but includes request and encoded internal frame semantics, so reusing
its complete identity workflow for OTLP would change the wrong boundary.
R3 correctly extends Gate rather than creating a shared generic identity manager.

## Prior closure and finding assessment

Source retains the earlier closures: optional TS framework declarations,
complete install/agent procedures, real live-flush and continuation readback,
Python Responses usage fallback including zero, bounded flush with a current-call
covering operation, and same-state Python/TS setup coordination. R3's pair-aware
Gate producer, current-schema readback, structured Python credential assertion
and Rust Panics section address the latest maintainer-relevant gaps at their
existing owners. No material maintenance-cost defect or unearned machinery
requiring deletion was established. No preference-only blocker is proposed.

## Verification limits and source guard

This is a static discovery review, not an execution verdict. No builds, tests,
installs, source/index edits, staging or commits were performed; fresh execution
belongs to the root reviewer. Committed proof records were treated as claims,
with test structure and assertions inspected independently. Authorized main was
examined for task-facing identity/ingest/query/lifecycle seams, not certified
wholesale for unrelated functionality.

HEAD remained `1cf675c58d01cba0e714102c75cb7d89e7a678b5` at entry and after
inspection. Tracked worktree and index diffs were empty. Only this assigned
report was written, preserving other review artifacts.
