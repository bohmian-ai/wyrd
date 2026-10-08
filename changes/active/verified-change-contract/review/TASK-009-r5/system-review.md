# TASK-009 Round 5 System-Resilience Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `11eb8ed2b64b70903f171335723dcc549b463352`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 46
- Task chain: TASK-009 plus TASK-009-R1, TASK-009-R2, TASK-009-R3, and the human-approved TASK-009-R4E that supersedes TASK-009-R4

The repository has no `.codegraph/` directory, so navigation used repository search and direct source/caller inspection. The candidate remained at the stated commit before this report was written. The reviewed implementation was not modified.

## Review Findings

### Critical

None.

### Important

- **SYS-R5-001 — [`sdks/wyrd-sdk-python/python/wyrd/otel.py:202-267`](../../../../../sdks/wyrd-sdk-python/python/wyrd/otel.py) — registration outcomes are keyed by provider equality rather than provider identity.** `weakref.WeakKeyDictionary` follows its keys' `__hash__` and `__eq__`; it does not distinguish two live objects that compare equal. The public escape hatch accepts any weak-referenceable duck-typed private provider, so two distinct provider instances may legitimately compare equal. After the first is installed, `install_run_correlation(second)` reads the first provider's cached `True` at line 258 and returns without calling the second provider's `add_span_processor`. The second provider then emits uncorrelated framework spans: explicit Drift/Eval/custom observations remain available, but the persisted trace-to-observation join required by REQ-151 and AC-032 fails for that provider. A failed first provider similarly causes a healthy equal second provider to inherit cached `False`. This is directly reachable without an outage or race; a minimal standard-library check with two distinct weak-referenceable objects implementing equal hashes/equality returns the first object's `WeakKeyDictionary` value for the second and reports one entry. Replace the outcome cache with weak, **identity-keyed** bookkeeping while preserving the revision-46 pre-mark, at-most-once, thread-safe, and no-strong-retention semantics. Add one focused test with two distinct equal providers and assert that each receives exactly one processor and enriches its own span; include the inverse case where a failed equal provider does not suppress registration on a healthy peer.

### Suggestions

None.

## Open Questions

None.

## Deployed-path evidence

| Runtime boundary | Source and behavior | System effect |
|---|---|---|
| Local Run selection | [`crates/shared/wyrd-client/src/state.rs:489-510`](../../../../../crates/shared/wyrd-client/src/state.rs) resolves an initial hydrated Card alias locally; [`crates/shared/wyrd-client/src/observe/mod.rs:47-113`](../../../../../crates/shared/wyrd-client/src/observe/mod.rs) owns one UUIDv7 Run ID and immutable Card-scoped sibling views. | Rust, Python, and TypeScript share the same local owner. Run creation adds no network dependency, server Run, or durable process state. Unknown aliases fail before Python context entry. |
| Python context boundary | [`sdks/wyrd-sdk-python/src/observe/mod.rs:195-259`](../../../../../sdks/wyrd-sdk-python/src/observe/mod.rs) passes the view's exact CardRef and Run ID to `_enter_run` and `_exit_run`; exit always returns `false`. | Entry/exit changes only optional ambient correlation. It starts no span, performs no network IO, and owns neither provider nor Bifrost lifecycle. User exceptions and cancellation delivered through normal unwinding are not suppressed. |
| Execution-local scope | [`sdks/wyrd-sdk-python/python/wyrd/otel.py:195-198,272-313`](../../../../../sdks/wyrd-sdk-python/python/wyrd/otel.py) keeps the complete tuple stack under one import-time OpenTelemetry context key. Entry and matching exit each attach a replacement value; no token, detach call, Run mutation, or process-global Card selection remains. | Nested scopes restore their outer pair; asyncio tasks receive context copies; simultaneous tasks using the same immutable Run do not remove one another's values. A failed or mismatched exit leaves the current context unchanged, as revision 46 specifies. |
| Provider registration and span start | [`sdks/wyrd-sdk-python/python/wyrd/otel.py:200-269`](../../../../../sdks/wyrd-sdk-python/python/wyrd/otel.py) weakly caches a terminal outcome before the foreign registration call and uses one stateless processor. `on_start` reads the parent context and contains all lookup/span failures. | Healthy ordinary providers receive one processor and provider failure cannot escape. The equality-keyed cache, however, can make one distinct provider inherit another's outcome and silently lose correlation (SYS-R5-001). Provider/exporter startup, flush, and shutdown otherwise remain caller-owned. |
| Explicit observations | [`sdks/wyrd-sdk-python/src/observe/mod.rs:125-153,308-350`](../../../../../sdks/wyrd-sdk-python/src/observe/mod.rs) reads an active Python span only as a best-effort Eval default, then delegates to the existing Rust observation path; the shared Run retains the state-owned Bifrost facade. | Missing/broken OpenTelemetry does not hide Card lookup, input validation, authorization, queue pressure, or writer errors. Explicit observations bypass the span processor and remain available when ambient enrichment fails. |
| Authenticated trace persistence | [`sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:365-474`](../../../../../sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py) registers a private stock provider, exports through authenticated OTLP/HTTP, and queries durable trace/custom/Eval joins. | Gate still authorizes asserted CardRef against signed scope; tenant, publisher, and Card UID remain server-derived. No server, Scribe, Oracle, or schema source changed for TASK-009. The healthy stock provider is covered, but equality-colliding private providers are not. |
| Graceful completion | [`sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:550-563`](../../../../../sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py) explicitly force-flushes the provider, shuts down state, waits for server publication, queries, then shuts down the provider. | Run exit is correctly not represented as an exporter flush, Bifrost drain, Scribe publication, or query-visibility barrier. |

Affected capabilities are local initial-Card Run selection in all first-class SDKs and optional Python span correlation. The defect affects Python providers reached through the public explicit installation hook; unrelated SDK methods, explicit observations, server routes, and storage recovery do not acquire an OpenTelemetry dependency.

## Failure-path and recovery assessment

| Failure or interruption | Boundary, availability, and recovery | Assessment |
|---|---|---|
| `opentelemetry-api` absent at process import | `_SCOPE_KEY` remains `None`; install returns `False`; Run entry/exit and explicit observations continue. Installing the package later requires process/module restart because bindings are intentionally import-time. | PASS. The optional dependency remains optional, and process restart recreates the module and provider bookkeeping. |
| API-only, unsupported, or non-weak-referenceable provider | Registration returns `False`; no span processor is installed, while user code and explicit observations remain available. | PASS. This is a component-level enrichment loss, not a process or service failure. |
| Provider raises before or after retaining the processor | The outcome is pre-marked `False` and never retried, avoiding duplicate installation after an unknowable side effect. Later direct installs and Run entries return the cached result. | PASS for one provider object and closes prior FIND-TASK-009-8. No retry or health loop amplifies the fault. |
| Two distinct providers compare equal | The second provider inherits the first provider's cached outcome and may never receive a processor. Restart clears the cache, but using both providers in one process has no recovery short of replacing provider objects/equality behavior or the process. Explicit observations stay available while ambient trace correlation and durable joins silently degrade. | **FAIL: SYS-R5-001.** The failure boundary should be one provider component; equality aliasing incorrectly extends it to a sibling provider in the same process. |
| Context lookup, attach, active-span mutation, or processor mutation fails | The optional operation is caught. A failed exit changes nothing; user exceptions propagate; explicit observation paths retain their ordinary result. | PASS. Focused tests exercise entry attach, enrichment, exit update, mismatched exit, and explicit observation independence. |
| Nested, awaited, copied-task, and concurrent same-Run scopes | Each Python execution context owns its attached tuple value; entry/exit match by exact pair. A task created inside a scope intentionally retains its copied correlation after the creator exits. | PASS. Event-driven same-Run async coverage avoids timing-based proof. Raw threads remain outside the approved propagation promise. |
| OTLP endpoint/exporter outage | Trace export may buffer, fail, or drop according to the caller-owned provider. Explicit Wyrd observations and the rest of the client remain separate; Wyrd adds no exporter, retry loop, readiness check, or shutdown hook. | PASS within the approved fail-open/no-provider-lifecycle boundary. Recovery is the provider's retry/flush policy, not Run exit. |
| Client cancellation, crash, or restart | Normal unwinding calls exit and returns `false`; abrupt termination loses execution-local context, provider cache, in-memory Run identity, and any unflushed client buffers. Already accepted server evidence keeps existing Bifrost durability. | PASS within the documented lifecycle ceiling. Restart creates a fresh invocation and fresh registration cache; there is deliberately no durable server Run to recover. |
| Server rejects an asserted CardRef or explicit write | Authorization/validation remains fail-closed at its owning boundary; correlation never substitutes another Card, tenant, principal, or UID. | PASS. Optional telemetry containment does not weaken explicit Wyrd errors or server-derived identity. |

## Recovery and proof assessment

- The cumulative source and diff show that revision 46 removed token/detach bookkeeping, passes the exact view pair to exit, preserves execution-local async propagation, and keeps provider/exporter/Bifrost/server lifecycle owners separate.
- Focused unit cases cover missing OpenTelemetry, unsupported and raising providers, accept-then-raise terminal caching, healthy global/private idempotency, entry/enrichment/exit failures, mismatched exit, nested scopes, concurrent distinct views, concurrent use of the identical Run object, task creation, and user-exception propagation.
- The production-shaped journey covers one ordinary private `TracerProvider`, authenticated OTLP/HTTP ingest, server-derived identity, explicit writer shutdown, publication, and durable trace/custom/Eval joins. It cannot detect SYS-R5-001 because its providers use identity equality and it installs only one private provider.
- No Cargo, pytest, or mise command was run during parallel discovery, per orchestration instruction. Source, task-recorded verification, `git diff --check`, and a minimal `python3` standard-library `WeakKeyDictionary` identity/equality check were inspected. The minimal check produced `False True True 1`: distinct objects, equal comparison, lookup through the second returned the first value, one cache entry.

## Prior-finding closure

- Prior findings 1-7 remain closed on the inspected cumulative source: explicit-observation failure proof, Run subject documentation, context restoration/key creation superseded by revision 46, exit signature parity, Rust test documentation, and same-Run async isolation all retain their required outcomes.
- `FIND-TASK-009-8` is closed for the diagnosed accept-then-raise duplicate-registration path: the pre-marked terminal cache asks one provider object at most once. SYS-R5-001 is a new sibling-provider isolation defect in that cache, not a reopening of retry behavior.

## Verification Notes

- Reviewed the complete base-to-candidate diff and the recorded TASK-009/R1/R2/R3/R4E evidence.
- Did not independently rerun the focused Python or authenticated Postgres-backed journey during parallel discovery.
- Candidate identity was checked before writing this report and remained `11eb8ed2b64b70903f171335723dcc549b463352`.

## Overall result

**FAIL** — SYS-R5-001 allows one private provider's registration state to suppress correlation on a distinct sibling provider, so the component failure boundary and required persisted joins are not reliable for every provider accepted by the public hook. All other inspected process, exporter, explicit-observation, server, and lifecycle boundaries remain correctly isolated.
