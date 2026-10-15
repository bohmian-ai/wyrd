# TASK-001 / R1 system resilience review

**PASS** — no material system-resilience finding.

## Subject and authority

Reviewed cumulative base `dac4c340f835676adc29f3dcc01f98e1d3d51465` to candidate
`295446005b18604030bf376de6209098a28886f7`, using the latest fix range from
`692d390dc40e43c6e1d7e25c7850d5c826d26a9e` for navigation. HEAD matched the
candidate. Inputs include approved spec r2, original TASK-001, prior verdict,
validated findings and R1 remediation/evidence. No implementation summary or
other current discovery report was used.

Applied AGENTS.md, agent rules, task-review skill, reference router,
spec-driven-development and maintainer-style guidance, telemetry-observations,
analytical-operations-reliability, Wyrd doctrine, and relevant Wyrd design
doctrine/client/authentication/trace/observation and Bifrost failure/recovery
contracts. CodeGraph was consulted before locating source. This report assesses
runtime topology, failure propagation, ownership and recovery, rather than
claiming to independently replace the language, documentation or security
specialist reports.

## Runtime topology and ownership

| Deployed path | Source and ownership evidence | System effect |
|---|---|---|
| Native Python frameworks / Python and TS Agents bridges → application OTEL provider | Python `otel.py:351-449`; TS `index.ts:2535-2648`; framework app builders and TS integration model construction | Instrumentation runs in the user's application process. Bridge maps retain only live callback spans, correlation and metadata. Shutdown terminates unfinished mapped spans with fixed ERROR and clears contexts; adapter flush/shutdown never owns the provider. |
| Managed provider → stock batch processor → stock OTLP/HTTP exporter → Wyrd HTTP edge | Python `otel.py:214-310`; TS `otel.ts:25-46,103-117`; server `http/otlp.rs:1-39,183-246` | One installed process-global provider; current-token callbacks reuse the selected shared client's auth owner. No second transport, export queue, retry system or server endpoint is introduced. The HTTP edge authenticates before bounded decoding; Gate routes and Scribe owns admission, WAL acknowledgement and visibility. |
| Explicit state flush/shutdown → managed OTEL wait → native shared writer drain | Python binding `src/state/mod.rs:262-311,508-525`; TS `index.ts:2718-2753,2958-2995`; shared `state.rs:680-706,2090-2164` | Provider errors are saved until the native drain has been attempted. Shared catalog settlement gives OTEL primary precedence and retains any native failure's stable code. Native-only errors remain unchanged. |
| Native observation emission → shared WyrdState BifrostLifecycle → Bifrost pooled producers → server | Shared `state.rs:689-705`; `observe/lifecycle.rs:29-52,122-151` | Native writer identity, admission, shutdown and ambiguous-drain retry authority remain in the existing shared owner. Successful native shutdown closes the writer; a failed drain retains the Started writer for retry. |
| Readback → existing Bifrost client → server Oracle/Scribe storage | Python integration `timeline`/`drift`; TS integration counterparts; existing test server `server.rs:1219-1248` | Journeys query actual accepted records. Server `flush_bifrost` settles server-owned work and publishes accepted rows; it cannot reach or drain the application's OTEL provider or native client queue. |

## Failure and recovery assessment

| Credible interruption | What stops / remains available | Recovery and evidence | Result |
|---|---|---|---|
| Python stock export blocks beyond flush deadline | Only the explicit lifecycle caller waits up to 30 seconds. `_ManagedTelemetry.force_flush` holds its lock only while selecting/starting one attempt, then `Future.result` waits without owning the callback/correlation locks. Actual export remains owned by stock BatchSpanProcessor. Native drain follows timeout. | `otel.py:214-255`; pinned installed OTEL `_shared_internal/__init__.py:167-193,240-246` confirms stock flush is synchronous and ignores its bound. `test_managed_telemetry_flush_deadline` uses that real processor and held exporter, checks two 504 returns plus native NOT_STARTED, emits correlated spans during the hold and observes one managed flush worker. | PASS |
| Repeated/overlapping managed Python flush during outage | Later calls reuse the still-running Future. They do not spawn another outstanding provider flush or add a retry backlog. The worker is daemonized; state lifecycle does not join it after the deadline. | After release, existing stock draining can finish; next completed-attempt call starts a fresh flush. Deadline test releases and joins controlled work, then successful state shutdown and application provider flush export all expected spans. The in-progress attempt's outcome is shared; this is not a cancellation or delivery promise for spans concurrently emitted after its snapshot. | PASS |
| Provider returns false, times out or raises/rejects | Explicit operation fails with scrubbed 504/503; no application/provider-wide shutdown is invoked. Native drain is still attempted. | Python `flush_telemetry`, TS `#flushTelemetry` and shared settlement; Python parametrized lifecycle test and TS managed lifecycle cases inspect native secondary code and payload exclusion. Both flush and shutdown use the same ordering. | PASS |
| Native drain also fails | Lifecycle returns OTEL primary error with native code, or unchanged native error when OTEL passed. Other clients and provider remain available. | `TelemetryFlushFailure::settle`; existing BifrostLifecycle retains a writer after ambiguous shutdown failure. No replacement writer or retry engine is introduced. This preserves the existing retry-on-same-state recovery path. | PASS |
| Export endpoint/auth unavailable or refuses export | Stock exporter owns its bounded network attempts and buffering. Framework execution and optional Run enrichment stay independent of explicit lifecycle failure. Python stock SDK can log failed export yet report a successful force_flush; this pre-existing documented limitation is not redefined as delivery. | Exporters request fresh shared-client tokens per export; no stale setup token or separate renewal owner. Negative real framework journeys check no unauthorized persistence. TS explicit refusal is observed as 503; Python limitation is disclosed in task/docs. | PASS |
| Flush while framework process remains alive | Flush leaves active spans active and keeps both native writer and managed global provider. | Python managed journey pauses on stdin after public state.flush, reads model/tool spans and native drift observation before allowing second work; TS does the same in-process. Both then emit second framework work and a native observation through the same state. Held span ends afterward and reads back after shutdown. | PASS |
| State shuts down while application continues | Native writer closes, but global provider and framework instrumentation remain usable. Application-owned providers are never flushed by a state. | Python app `live:312-333` and test `:137-179`; TS managed journey `:232-256` execute another real turn after state shutdown and explicitly application-flush the same global provider. Owned-provider source plus dedicated lifecycle/correlation cases prove isolation. | PASS |
| Competing setup / failed install | Another provider is refused without replacement; losing newly created processors are shut down. Same-state TS calls overlapping the dynamic import now recheck their owner before synchronous installation. | Python `_start_telemetry:257-290`, PyWyrdState OnceLock; TS `startTelemetry:2924-2941`, `otel.ts:103-117`; setup tests assert same-provider idempotence and losing-provider cleanup. | PASS |
| Application crash or rolling replacement | Live bridge maps, pending stock exporter spans and unacknowledged native client rows are process memory and may be lost. Daemon lifecycle work does not create a new persistent handoff. A replacement process installs its own provider and client; already acknowledged Scribe records retain the existing server recovery boundary. | Native shutdown documentation explicitly discloses pending-row loss before acknowledgement. Bifrost design live-tail/recovery section retains WAL replay/staged-file reconciliation. No changed server recovery, durable queue, new health dependency or process-exit trigger appears in the cumulative runtime diff. | PASS |

The new managed wrapper earns its state: it retains the provider and one
in-flight flush identity across lifecycle calls. Existing provider/exporter
owners alone cannot enforce the pinned Python stock flush bound. This small
coordination owner does not take over synchronous span callbacks, exported
batch ownership, native writer state or server durability. A timeout is local
to an explicit lifecycle call; it neither crashes a shared server nor disables
unrelated user operations.

## Prior finding closure relevant to this review

- **FIND-TASK-001-3: closed.** Real managed live readback now precedes shutdown
  in both languages, includes native writer readback, proves second work on the
  same state and active-span survival, and checks continued global export after
  state shutdown. Server publication helpers cannot mask an application queue
  that was never exported. R1 additionally records a no-op managed-flush mutation
  failing the first Python live readback.
- **FIND-TASK-001-5: closed.** The actual stock flush runs behind a bounded
  lifecycle wait and one retained in-flight attempt; native drain/error
  settlement occurs after timeout and ordinary synchronous correlation/emission
  remains usable. Held-export recovery is covered without replacing processors
  or adding retry/export queues.
- **FIND-TASK-001-6: closed for the system ownership consequence.** The TS
  post-import recheck avoids refusing the winning setup of the same state;
  distinct-state/provider refusal and cleanup remain intact.
- Findings 1/2/4 have no additional system-topology correction. Latest source
  removes the optional TS root type edge, completes docs and reads safe Responses
  usage in the existing mapper. Their complete contract/doc acceptance belongs
  to the corresponding independent reviews.

## Proof and limits

Source inspected includes full managed owner bodies and callers, both bridge
lifecycle implementations, existing shared native state/lifecycle ownership,
stock processor/exporter boundary, real live journey control flow and readback
helpers, failure-injection tests, and the unchanged HTTP/Scribe authority seam.
R1 committed evidence records C5's nine lifecycle/deadline/owned cases, C6's
three framework journeys and C7's TS journey passing, plus owner checks. These
are inspected command claims, not fresh executions by this reviewer. The held
stock exporter supplies credible runtime-local timeout/recovery evidence as
explicitly permitted by REQ-016; successful exports remain real journeys.

No material findings or required corrections. PASS applies to system resilience
within the approved task and stated delivery boundaries.

Skipped new builds, tests, process-kill experiments and broad gate execution;
only this report was written. Residual risk is stock best-effort export and
loss of unacknowledged in-memory work on application death, already within the
approved lifecycle boundary.
