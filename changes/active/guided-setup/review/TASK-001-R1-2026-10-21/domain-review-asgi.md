# ASGI concurrency and resource lifecycle domain review

Result: **PASS**. No material proposed findings in this domain.

## Immutable subject and reviewed boundary

Base `ebd75dc224854413d17cb06b70da8181b78cde6b`; candidate `796377a1d02f5a0a80fd905cc18fbbbb2d49bb4a`; remediation start `00b6148919b9ccd5f958f3b621ab6a89c6372001`. Reviewed the cumulative diff for ASGI integration, execution-local OTel correlation, its exports, examples, dependencies and tests, then the latest fix diff to locate prior-finding closure. HEAD remained the candidate. No current discovery report was read. This is a domain verdict, not acceptance of the unrelated CLI, host, Manual, schema or rustdoc surfaces.

The boundary is one application worker's loaded WyrdState, its writer/provider lifecycle, one Run per HTTP/WebSocket invocation, and the projection of lifecycle failures. No server data-plane ownership or durable semantics move to Python. The reviewed flow is public `wyrd.integrations.asgi.WyrdMiddleware` → state lifecycle/native Run → existing telemetry and shared Bifrost lifecycle owners.

## Authority and source coverage

Read the task-review skill, root/Python rules, change workflow, agent-doc index, maintainer/error guides, testing map, approved spec revision 6 (REQ-021–026, AC-012–014, INV-005/009), original task scenarios 4–7 and evidence, prior verdict/final ledger, and remediation diagnoses/evidence. Revision 6 changes guided CLI scope; it leaves this domain's approved semantics unchanged.

Architecture owners inspected: `architecture/wyrd-design.md` Client/framework adapter boundary (around 330–347); `architecture/bifrost-design.md` System boundary; security posture secret-handling rules. The ASGI adapter may adapt protocol and execution context, but must reuse the existing writer/provider/Run owners. The [ASGI lifespan protocol](https://asgi.readthedocs.io/en/stable/specs/lifespan.html), fetched during this review, permits exception-based unsupported-lifespan compatibility and distinguishes explicit startup/shutdown failure messages. It supplies no marker that universally distinguishes unsupported lifespan from a pre-receive app error.

CodeGraph preceded code navigation. Expanded source coverage:

- `_middleware.py` in full, all diagnostic call sites, normal and exceptional lifespan paths, public exports/stub and request-scope dispatch.
- Native Python `state/mod.rs` startup, managed-provider installation, flush and shutdown; native `observe/mod.rs` Run enter/exit. Shared `observe/lifecycle.rs` phase/claim/shutdown owner in full. Its successful shutdown closes the state; ambiguous drain failure retains the same writer for its existing retry semantics.
- `wyrd/otel.py` correlation stack and new ContextVar mirror, processor installation, optional-package handling, managed provider installation/flush and application-provider conflict behavior. All native Run enter/exit calls route through `_enter_run`/`_exit_run`; the middleware adds no second Run registry.
- Full ASGI integration journey module, worker app, FastAPI example and published integration guide. Relevant existing observation-unit tests cover nested/concurrent Runs, same Run entered by separate tasks, captured contexts crossing threads, optional packages and provider ownership. C8's two-worker process command has no preload flag; worker import loads the state after fork, and lifespan starts its writer.

## Requirement and failure-path assessment

| Obligation | Source and proof | Assessment |
| --- | --- | --- |
| Pure public adapter; no startup registration | Public exports/stub; `_middleware.py:58–102`; example uses `WyrdState.from_path` | PASS |
| Writer then optional telemetry; startup failure and partial cleanup | `_start:104–116` and `_lifespan:129–134`; native state methods retain lifecycle/provider ownership; inspected C5 writer refusal/provider conflict/missing-extra cases | PASS |
| Startup/shutdown blocking work stays off loop | `asyncio.to_thread` at 121/131; telemetry-failure cleanup is inside the same startup worker thread; fresh heartbeat probe below | PASS |
| App shutdown before drain; preserve failures; drain once | `app_send:148–156` and abnormal exception branch 158–170 share guarded local `drain:138–143`; successful/failure terminal replies are not followed by another drain | PASS |
| Controlled cancellation of established lifespan attempts cleanup then propagates | `except BaseException` routes CancelledError through shared drain and bare re-raise; actual task cancellation independently probed | PASS |
| Safe Wyrd-created lifecycle diagnostics | `_diagnostic:26–37` projects canonical Wyrd code/message, exact known missing-extra remedy, otherwise type-only classification; all generated responses and cleanup/drain logs use it | PASS |
| Per-invocation Run, isolation and propagation | `__call__:97–100` constructs Run before app execution, wraps entire await in native context manager, passes request Run in copied scope; native exit returns False; inspected C6 overlapping/streaming/WebSocket/error/cancel/unknown-alias paths | PASS |
| Instrumentation correlation without HTTP spans/provider replacement | ContextVar mirror survives instrumentation's extracted empty OTel context; processor falls back to it; no adapter span creation; C7 checks real stored span attributes and application-owned provider | PASS |
| Worker-local startup and graceful drain, disclosed limits | worker app and C8 actual two-worker command; documented enabled lifespan, timeout/failed-drain/hard-kill limits | PASS with runtime limits below |

The native state remains the one writer/provider-flush owner. `telemetry=False` skips provider setup; state shutdown flushes only telemetry the state itself installed, leaving an application-owned provider to the application. Run exit restores correlation and cannot substitute for writer drain. The local drain guard is protocol-attempt bookkeeping, not a new writer phase machine; its one-attempt behavior preserves the bounded cleanup/no-delivery-guarantee contract.

The mirrored Run stack uses execution-local immutable tuples. Enter/exit update only the current context and pop only the matching top; nested and concurrent contexts retain their own identities. When OTel's request instrumentation attaches an extracted context without the private Wyrd key, the mirror still supplies correlation for spans started inside the Run. Both paths remain optional and fail open; no framework instrumentation or provider is installed by the middleware itself beyond calling the existing state method.

## Prior findings and sibling paths

**FIND-TASK-001-1: CLOSED.** The former immediate re-raise after a replied application lifecycle is replaced by `await drain(); raise` at 160–165. A raw established application can queue final evidence and raise before a terminal reply; it now reaches the same off-loop shutdown owner as normal completion and explicit failure messages. Secondary ordinary shutdown errors are logged by `_drain`, whose diagnostic return cannot replace the primary app error. Explicit startup failure, explicit shutdown failure, successful shutdown, post-receive startup failure, unsupported-lifespan fallback, application exception and controlled established-task cancellation were traced as siblings. The guard prevents another attempt if app/framework code raises after sending a terminal response.

Submitted C5's cancellation case explicitly raises `asyncio.CancelledError()`; that is not a real `Task.cancel()` experiment. Independently importing the actual public middleware, I created an established app waiting on its receive queue, called its task's `cancel()`, and awaited termination. The observed order was `start → app-finally → drain`, one drain, original CancelledError propagation, startup.complete as the only reply, both start/drain on a different thread from the event loop, and 57 heartbeat ticks while the blocking fixture methods slept. This is real task-cancellation/cleanup-attempt proof with a small state double, not proof of native persistence. C5 separately supplies the submitted real-state/server final-evidence and state-closed proof for abnormal exits.

No requirement for repeated cancellation, cancellation during the in-flight startup thread, shielding forever, retrying failed drains, or delivery after timeout/hard kill is inferred. Those were explicitly excluded from remediation. The pre-first-receive ordinary exception compatibility branch remains supported; the prior rejected proposal is not reopened. Application-owned explicit failed-message text remains unchanged, as approved.

**FIND-TASK-001-2: CLOSED.** All Wyrd-created diagnostic consumers share the corrected `_diagnostic` producer: startup refusal, telemetry-startup cleanup warning, shutdown log/failed response and raw post-receive app startup failure. Arbitrary exception bodies/tracebacks are absent from that projection. C5 synthetic DSN/token/newline cases exercise the unexpected-error projection and known-extra remedy; canonical Wyrd error cases retain code. Forwarding an explicit application failure is a separate ownership boundary; preserving it is required, not a diagnostic leak introduced by the adapter. No new blacklist, error catalog, provider policy or application-message rewriting is needed.

## Verification and limits

Fresh review execution: public middleware actual-task-cancel/off-loop heartbeat probe passed. Existing observation unit module was rerun using `mise exec -- uv run --no-sync --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py`: **68 passed in 57.11 s**. A final HEAD check still returned the immutable candidate.

C5/C7 GREEN and earlier C6/C8 real-server evidence were inspected in the supplied task/remediation records and test source; those Postgres/worker journeys were not rerun here. C8 disables telemetry, so it directly proves worker-local writers/drain; C7 supplies managed/application provider proof in one process. Runtime proof is Linux x86_64 only. macOS arm64/Linux arm64 are unexercised. The adapter uses portable Python/asyncio mechanisms and introduces no target-OS switch, cross-process queue or supervisor.

No Cargo, formatter, source edit, commit or new harness was run. Only this report was written. Integrated journey/docs/SDK aggregates and cross-task deployment identity remain at change review. No delivery or timeout hardening beyond the approved authority is proposed.

Material proposed findings: **none**.
