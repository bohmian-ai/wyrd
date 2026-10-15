# Concurrency domain review

Result: **FAIL** — one proposed finding for independent validation.

Subject: base `dac4c340f835676adc29f3dcc01f98e1d3d51465`, candidate
`692d390dc40e43c6e1d7e25c7850d5c826d26a9e`, approved spec r2 and original
TASK-001. Reviewed cumulative changes and their existing lifecycle/correlation
owners; this is not a last-commit review.

## Boundary and authority

This review covers Python callback synchronization, task-local Run/Card
attribution, TypeScript callback scheduling, duplicate/missing/late callbacks,
managed versus application-owned global-provider installation, and process
lifetime. It does not require implicit propagation into arbitrary threads or
detached tasks. Auth, data persistence, packaging, and broad error-policy
acceptance are separate reviewer domains.

| Authority | Applied obligation |
|---|---|
| `AGENTS.md` §§2, 3, 6–8, 10–12; `architecture/agent-rules.md` | Runtime ownership, Python lifetime boundaries, explicit context and lifecycle behavior, scoped proof |
| `architecture/references/README.md`; `languages/spec-driven-development.md`; `languages/maintainer-style.md` | Approved packet authority; inspect real owner/caller paths rather than accepting execution claims |
| `architecture/wyrd-design.md` authentication/export and Observation identity sections, especially 664–686 and 818–846; `architecture/wyrd-doctrine.mdx` | One managed provider, explicit application ownership, optional execution-local correlation, no Run-owned lifecycle |
| `architecture/references/domain/telemetry-observations.md` | Parent/context preservation, sampling, attributable observations |
| `architecture/references/languages/pyo3-boundaries.md`; `languages/typescript-guide.md` | Foreign-runtime lifetime and concurrency boundaries |
| Approved `spec.md` REQ-010, REQ-012–014, REQ-016, INV-003/004; original task scenarios 1–4 and 6 | Live duplicate idempotence, thread-safe callbacks, start-time attribution, setup idempotence, flush/drain ordering and preserved provider usability |

CodeGraph was consulted before source navigation. Installed framework sources
were inspected to verify callback dispatch behavior for the locked packages;
they supplement, rather than override, the approved Wyrd contract.

## Source and consumer coverage

| Source/consumer | Review evidence and conclusion |
|---|---|
| Python `python/wyrd/otel.py`: `_RunCorrelationProcessor`, `install_run_correlation`, `_enter_run`, `_exit_run`, `_start_telemetry`, `WyrdAgentsTraceProcessor` | Scope is a context-local immutable tuple, not a mutable stack shared by Run instances. Install marker/check/add is locked; failed add removes marker for retry. Bridge map/closed state is protected by one lock. Starts capture current scope and selected SDK parent immediately; completion mutates the retained span rather than looking up the completion-time Run. Shutdown closes admission and takes/clears remaining spans before ending them. Losing global install shuts down its own provider. |
| Python `src/observe/mod.rs::__enter__/__exit__`; `src/state/mod.rs::start_telemetry`, `flush`, `shutdown`, `flush_telemetry`, GC traversal/clear | Run entry/exit forwards identity without retaining context tokens on the shared Run object. State retains its installed provider; state flush/shutdown never calls provider shutdown. Provider failure is captured before native drain and settled afterward. No new interpreter lifetime is introduced in Rust tests. |
| TypeScript `src/index.ts::Run.scope`, `WyrdAgentsTraceProcessor`, `WyrdState` telemetry/lifecycle methods | Run uses the active OTEL context and the established shared key. Callback bodies perform their map operations synchronously with no suspension, despite promise-returning framework signatures; concurrent async callers cannot interleave halfway through a map transition. Completion does not read the active Run. `shutdown` closes admission, ends unfinished spans and clears its map without touching provider lifetime. Setup has the race below. |
| TypeScript `src/otel.ts::runCorrelationProcessor`, `runCorrelation`, `startTelemetry` | Correlation processor is stateless; construction-time application installation preserves its provider configuration. Failed global registration closes only the newly constructed provider. Managed success registers context/propagation through the standard API. |
| Shared `wyrd-client/src/state.rs::WyrdState` lifecycle and `TelemetryFlushFailure`; TS `native/src/cards.rs::flush/shutdown/telemetry_flush_error` | Existing shared writer ownership remains the native owner; language boundaries add provider flush before the existing native operation. Shared failure projection does not introduce another writer, task, queue or provider owner. |
| Python `tests/unit/state/test_observe_surface.py` | Reviewed setup repeat/other-state/application/lost-race cases, retryable registration, concurrent task scopes including same Run object, explicit captured context into a thread, completion in another Run, duplicate starts/ends, missing starts, shutdown/late callbacks, and eight-worker callback test. Distinct worker IDs test ordinary parallel callbacks; the map lock also serializes same-live-ID admissions. |
| TS `tests/unit/telemetry.test.ts` | Reviewed owned-provider async/nested scopes, sequential setup repeat/collision, callback start/end attribution, missing parent fallback, interrupted shutdown and provider-failure lifecycle cases. Same-state overlapping setup is absent. |
| Python framework `app.py` and `test_framework_telemetry.py`; TS `tests/integration/agents-trace-processor.test.ts` and `support/observed-service.ts` | Real framework execution uses one explicit processor and exercises nested/concurrent Runs and streaming completion outside the scope's lexical return. Python subprocess separation respects once-only global registration and ADK tracer binding. Journeys flush completed spans through state shutdown or application-owned provider shutdown. |
| Existing Python `tests/integration/test_local_development.py`; TS `tests/support/local-development.ts`, `tests/integration/verify-in-real-time.test.ts` | Existing local workflows repeat setup sequentially. TS real-time tests memoize their own startup promise to share process-global telemetry across tests; that helper does not serialize the public state API. |
| Installed Python Agents `tracing/processor_interface.py`, `traces.py`, `spans.py`; TS Agents Core 0.20 `dist/tracing/processor.mjs`, `traces.mjs`, `spans.mjs` | Python framework callbacks are synchronous and require thread safety. TS trace start/end awaits its processor; Wyrd's callback transitions themselves contain no awaits, so start-time active context is read before callback suspension. |

## Proposed finding

### CONC-001 — overlapping setup on the same TS state rejects its own provider

- **Classification:** INCORRECT.
- **Location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:2880`, especially the
  check at 2881 and suspension/install at 2886–2891; related existing installer
  `src/otel.ts:96–107`.
- **Obligation:** REQ-012 / AC-008: repeated managed setup on the same state is
  idempotent, while a provider owned by another state/application is refused.
- **Reachable producer-to-result path:** an application shares one state with
  two startup consumers and executes
  `await Promise.all([state.startTelemetry(), state.startTelemetry()])`.
  Both calls see `#telemetry === undefined` before suspending on the dynamic
  import. The first resumed call installs its provider and assigns it to the
  state's field. The second resumes without rechecking that field, constructs
  another provider, loses the process-global registration, closes the loser,
  and throws `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`. This is guaranteed by the
  method's suspension/check ordering; it does not require worker threads,
  arbitrary context propagation or timing sleeps. A cached dynamic import is
  still awaited and therefore still permits both initial checks.
- **Observable consequence:** successful setup of the very same state's
  provider causes the other ordinary initialization caller (and thus the
  aggregate startup) to reject with a conflict intended for another owner.
  The installed provider remains intact, so this finding does not claim
  credential mixing, duplicate exports, or global replacement.
- **Existing owner and smallest correction:** keep setup in the existing
  `WyrdState.startTelemetry`; recheck its installed-provider field after the
  existing import await and before the synchronous installation sequence.
  There is no need for a registry, new lock, public API, extra exporter,
  alternate provider owner or process-wide startup manager. Preserve the
  existing other-state/application collision refusal and loser cleanup.
- **Focused closure proof:** extend existing TS setup unit coverage to invoke
  two same-state starts before awaiting either; both must resolve, global
  provider identity must remain stable, and another state's attempt must
  still reject. This tests a local async transition and requires no server.
- **Current evidence gap:** existing `telemetry setup` awaits the first call
  before issuing the second, so its green result cannot cover this path.

## Verification limits and result

No builds, test lanes, provider registrations or runtime experiments were run
by this reviewer, as directed by the orchestrator. Source/static evidence and
test bodies were inspected; the packet's reported green results were treated
as claims, not independently reproduced results. The finding above is a
source-derived counterexample awaiting independent validation, not a claimed
observed test failure.

No additional material concurrency defects were established. In particular,
this review does not strengthen shutdown into a barrier against application
callbacks that are still executing, invent callback delivery ordering beyond
the framework, or demand propagation into arbitrary threads. Those would add
semantics not required by the approved task.

At completion HEAD remains the pinned candidate; `git diff --name-only` is
empty and `git diff --check` passes. Only review artifacts are untracked.

**FAIL**, pending independent validation of CONC-001.

Skipped: runtime reproduction and build/test lanes; no source changes made.
Risk: the same-state startup race is source-proven but has no executed regression test in this review.
