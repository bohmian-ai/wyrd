# TASK-009 Round 6 System-Resilience Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 46
- Task chain: TASK-009, TASK-009-R1, TASK-009-R2, TASK-009-R3,
  TASK-009-R4E (which supersedes TASK-009-R4), and TASK-009-R5

The repository has no `.codegraph/` directory, so navigation used repository
search and direct source/caller inspection. The candidate remained at the
stated commit through review. Only this report was written.

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None.

## Open Questions

None.

## Deployed-path evidence

| Runtime boundary | Source and behavior | System effect |
|---|---|---|
| Local Run creation and Card selection | [`crates/shared/wyrd-client/src/state.rs:489`](../../../../../crates/shared/wyrd-client/src/state.rs) resolves the root or an initial hydrated Card locally; [`crates/shared/wyrd-client/src/observe/mod.rs:42`](../../../../../crates/shared/wyrd-client/src/observe/mod.rs) owns one UUIDv7 Run ID and immutable Card-scoped views. | Rust, Python, and TypeScript share the same local owner. No network request, server Run, lease, or durable client state is created. Unknown aliases fail before Python scope entry. |
| Python context-manager boundary | [`sdks/wyrd-sdk-python/src/observe/mod.rs:195`](../../../../../sdks/wyrd-sdk-python/src/observe/mod.rs) passes the view's exact CardRef and Run ID to `wyrd.otel`; `__exit__` always returns `false`. | Entry and exit alter only optional ambient correlation. They start no span, perform no network IO, and own neither provider nor Bifrost lifecycle. User exceptions and normal asyncio cancellation are not suppressed. |
| Execution-local correlation | [`sdks/wyrd-sdk-python/python/wyrd/otel.py:192`](../../../../../sdks/wyrd-sdk-python/python/wyrd/otel.py) keeps a tuple stack under one import-time OTel context key; entry and top-matching exit attach replacement values without tokens or Run mutation. | Nested views restore the outer pair, and copied asyncio contexts isolate concurrent uses of the same immutable Run. A failed or mismatched exit leaves the context unchanged as revision 46 specifies. |
| Provider registration | [`sdks/wyrd-sdk-python/python/wyrd/otel.py:200`](../../../../../sdks/wyrd-sdk-python/python/wyrd/otel.py) weakly records terminal outcomes by referent identity under one lock. Each attempt creates one inactive processor, activates it only after `add_span_processor` returns normally, and never retries that provider object. | Equal-but-distinct providers cannot inherit one another's outcome. A provider that retains the processor and raises cannot enrich later spans. No strong provider retention, retry loop, background task, or lifecycle takeover was added. |
| Span-start callback | [`sdks/wyrd-sdk-python/python/wyrd/otel.py:208`](../../../../../sdks/wyrd-sdk-python/python/wyrd/otel.py) synchronously reads the parent context and writes only `wyrd.card_ref` and `wyrd.run_id`; the complete active callback path contains optional lookup and mutation failures. | A processor failure is confined to ambient enrichment in the caller's Python process. It cannot crash the Wyrd server, stop explicit observations, or invent tenant, principal, request, or Card UID identity. |
| Explicit observation path | [`sdks/wyrd-sdk-python/src/observe/mod.rs:251`](../../../../../sdks/wyrd-sdk-python/src/observe/mod.rs) delegates Drift, Eval, and generic records to the shared Run and state-owned Bifrost facade. | Card lookup, input validation, authorization, queue pressure, and writer failures keep their existing strict behavior. Optional OTel failure is not converted into success or a synthetic observation. |
| Authenticated trace persistence | [`sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:365`](../../../../../sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py) installs the processor on a private stock provider, exports through authenticated OTLP/HTTP, and queries persisted trace/custom/Eval joins. | Gate retains signed Card-scope authorization and derives tenant, publisher, request identity, and Card UID. No server, Scribe, schema, or deployment topology changed. |
| Graceful completion | [`sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:550`](../../../../../sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py) force-flushes the provider, shuts down state, waits for server publication, queries, and finally shuts down the provider. | Run exit is correctly not represented as exporter flush, Bifrost drain, publication, or query visibility. |

Affected runtime capabilities are the local initial-Card Run surface in all
three SDKs and optional Python span correlation. Server request serving,
authorization, storage recovery, and explicit observation delivery do not gain
an OpenTelemetry dependency.

## Failure-path and recovery assessment

| Failure or interruption | Boundary, availability, and recovery | Assessment |
|---|---|---|
| OpenTelemetry absent at module import | The private key is absent and installation returns `False`; Run construction, entry/exit, user code, and explicit observations continue. A later package installation takes effect after module/process restart. | PASS. Optional telemetry remains optional. |
| API-only, unsupported, or non-weak-referenceable provider | The exact provider identity receives a terminal `False`; no processor is handed over. Other providers and explicit observations remain available. | PASS. Failure stays at one provider component. |
| Provider raises before retaining its processor | Its pre-recorded outcome remains `False` and is not retried. | PASS. No retry amplification or duplicate handoff occurs. |
| Provider retains the processor and then raises | The retained per-attempt processor remains inactive forever, while later calls return the cached `False`. | PASS. This closes `FIND-TASK-009-10`; the focused regression invokes the retained callback inside a live Run scope and observes no attributes. |
| Two distinct providers compare equal | Identity lookup gives each live object an independent attempt and outcome; a failed peer cannot suppress a healthy one. | PASS. This closes `FIND-TASK-009-9`; the focused regression proves healthy-peer enrichment and one processor. |
| Provider object is released | Only a weak reference remains in the outcome list; dead entries are pruned on the next lookup. A new provider receives its own attempt even if Python later reuses an address. | PASS. Registration bookkeeping does not extend provider lifetime. |
| Context lookup, attach, active-span mutation, or processor mutation fails | The optional operation is contained. A failed exit changes nothing; user exceptions still propagate; explicit observations retain their ordinary result. | PASS. Focused tests cover entry/context update, enrichment, mismatched exit, and explicit-observation independence. |
| Nested, awaited, copied-task, and concurrent identical-Run scopes | Each execution context owns its tuple value. A task created in a scope intentionally retains its copied value after the creator exits. | PASS. Deterministic asyncio coverage exercises same-Run overlap and independent exit. Raw threads remain outside the approved propagation promise. |
| OTLP endpoint or exporter is unavailable | Caller-owned OTel buffering/export policy determines loss and retry. Explicit Wyrd observations continue through the separate Bifrost writer; Wyrd adds no retry, health probe, exporter, or shutdown hook. | PASS within the approved fail-open and provider-lifecycle boundary. |
| Task cancellation, client crash, or process restart | Normal unwinding invokes `__exit__`; abrupt termination discards in-process context, the weak registry, Run identity, and unflushed provider/Bifrost buffers. Already accepted server evidence remains durable. Restart creates a fresh invocation and registration registry. | PASS within the documented lifecycle ceiling; there is deliberately no durable server Run to recover. |
| Server rejects asserted CardRef or an explicit write | Authorization and validation remain fail-closed at the server or writer boundary; ambient correlation never substitutes another identity. | PASS. Optional telemetry containment does not weaken Wyrd trust boundaries. |

The provider registration call is synchronous and intentionally has no timeout;
a provider implementation that never returns can delay the calling Run entry.
Revision 46 and TASK-009-R5 explicitly reject adding timeout or provider
lifecycle machinery, and the stock OTel provider performs only local processor
registration. No broader availability claim was inferred.

## Recovery and proof assessment

- The cumulative source closes the prior sibling-provider blast-radius defect:
  terminal outcomes are weakly identity-keyed, not equality-keyed.
- It also closes the failed-provider contamination path: activation belongs to
  the per-attempt processor and occurs only after normal registration return.
- The correction adds no server dependency, retry, queue, exporter, health
  check, process-global Card scope, or durable recovery state.
- The production-shaped journey remains the relevant healthy-path system proof:
  authenticated stock OTLP/HTTP export, explicit Bifrost drain, publication,
  and persisted trace/custom/Eval joins with server-derived identity.
- Focused proof directly covers the two R5 recovery boundaries rather than
  inferring them from the journey's ordinary provider.

## Prior-finding closure

- `FIND-TASK-009-1` through `FIND-TASK-009-8` remain closed on the cumulative
  source and focused tests: explicit-observation independence, Run subject
  documentation, revision-46 context restoration, import-time key ownership,
  exit signature parity, Rust proof documentation, same-Run async isolation,
  and terminal no-retry registration behavior are preserved.
- `FIND-TASK-009-9` is closed by weak identity lookup plus the distinct-equal
  provider regression.
- `FIND-TASK-009-10` is closed by per-attempt inactive processors plus direct
  invocation of the processor retained by a failed provider.
- `FIND-TASK-009-11` through `FIND-TASK-009-13` are artifact/proof corrections
  with no adverse runtime or recovery effect; the authority now names revision
  46, R4 is superseded, and exact Rust commands are recorded.

## Verification Notes

- Reviewed the complete base-to-candidate diff, applicable architecture, the
  original task, and remediation chain R1/R2/R3/R4E/R5.
- Ran the four focused provider tests: **4 passed**.
- An initial whole-file invocation without the task's required setup failed
  only because the subprocess could not import the uninstalled local `wyrd`
  package. After `mise run py:setup`, the repository-prescribed focused file
  passed: **39 passed**. The setup/build completed successfully.
- The Postgres-backed journey and broader Rust/TypeScript/generation/lint lanes
  were inspected through the immutable implementation evidence rather than
  rerun in this system-resilience pass.
- Candidate identity was checked before writing and remained
  `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`.

## Overall result

**PASS** — no material system-resilience finding remains. Provider failure is
contained to the exact provider identity, a failed retained processor stays
inert, explicit observations and unrelated services remain available, and
recovery/lifecycle ownership stays with the existing provider, Bifrost client,
and server boundaries.
