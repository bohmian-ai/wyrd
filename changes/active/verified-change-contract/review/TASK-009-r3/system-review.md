# TASK-009 Round 3 System-Resilience Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `d01307b8c37b47488115743c94b624a29d66e4be`
- Approved specification: `changes/active/verified-change-contract/spec.md`, current revision 45, preserving the task's revision-35 obligations
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediations: `review/TASK-009-r1/TASK-009-R1-restore-otel-correlation.md` and `review/TASK-009-r2/TASK-009-R2-close-proof-and-boundary-parity.md`

The repository has no `.codegraph/` directory, so navigation used repository
search and direct caller/source inspection. `HEAD` was the stated candidate
before and after review. The reviewed implementation was not modified.

## Deployed path and affected capabilities

The changed runtime path is confined to the client process until an existing
transport boundary is crossed:

1. Shared `WyrdState::run` or `run_for_card` resolves the root or hydrated Card
   locally and constructs one `Run`; immutable `Run::for_card` views retain its
   UUIDv7 `run_id` and select another exact CardRef
   (`crates/shared/wyrd-client/src/state.rs:485-511`,
   `crates/shared/wyrd-client/src/observe/mod.rs:42-100`). Rust and TypeScript
   project this same owner and do not depend on Python OpenTelemetry state.
2. Python `Run.__enter__` and `__exit__` delegate only best-effort ambient
   correlation to `wyrd.otel`; they do not contact the server, start or end a
   span, flush a provider, or touch the Bifrost lifecycle
   (`sdks/wyrd-sdk-python/src/observe/mod.rs:195-239`). R2 aligns the public
   exit parameter names/defaults but does not expand that lifecycle boundary.
3. `_enter_run` installs at most one processor on the selected provider,
   attaches the exact `(card_ref, run_id)` under one process-private key, and
   records the token plus prior value in an execution-local stack. The
   processor copies only those two values to spans started in the scope
   (`sdks/wyrd-sdk-python/python/wyrd/otel.py:186-297`).
4. Framework spans leave the process through the caller-owned stock OTLP/HTTP
   provider to authenticated `POST /v1/traces`. Explicit Drift, Eval, and
   custom records bypass that provider and continue through the state-owned
   `wyrd_client::Bifrost` facade and its existing bounded producers
   (`test_observe_journey.py:365-402,505-559`).
5. The unchanged server path authenticates the publisher, authorizes the
   asserted CardRef against signed scope, derives Card UID, and hands accepted
   rows to Scribe. Publication remains an explicit later barrier. The journey
   verifies the persisted trace/custom/Eval joins through public Bifrost query
   (`test_observe_journey.py:413-474,550-563`).

Affected capabilities are Python ambient span correlation and joins that rely
on it, plus local initial-Card selection projected by all SDKs. Unrelated SDK
operations, server routes, Gate/Scribe admission, query service, authorization,
and publication topology are unchanged.

## Failure and recovery assessment

| Failure or interruption | What stops and what remains available | Recovery and evidence | Result |
|---|---|---|---|
| Missing OpenTelemetry package | Ambient span enrichment is absent. Local Run/Card identity, user code, and explicit observations remain available; invalid aliases and explicit-write errors remain strict. | Imports and all optional operations are contained in `install_run_correlation`, `_enter_run`, and `_exit_run` (`otel.py:246-326`). The missing-package test reaches the ordinary explicit Drift boundary (`test_observe_surface.py:392-425`). | PASS |
| API-only/unsupported provider or processor-registration failure | Only processor installation fails; entry continues and explicit Bifrost observations remain independent. A failed provider is removed from bookkeeping rather than falsely treated as installed. | `otel.py:246-276`; R2's focused case selects a provider whose `add_span_processor` records and raises, then emits Drift and proves the attempt occurred (`test_observe_surface.py:431-455`). A later entry may retry registration, but there is no background loop, network call, or health-check amplification. | PASS |
| Attach or span enrichment failure | The failing entry records a paired `None` when no token was installed, so exit cannot detach another scope. Processor and active-span errors cannot escape into application code. Explicit observations remain usable. | `otel.py:222-231,279-297`; focused attach/enrichment cases reach the ordinary Drift boundary (`test_observe_surface.py:431-479`). | PASS |
| Detach raises or silently fails to reset | Exit attempts the exact token, compares the resulting Wyrd context value with the recorded prior value, and best-effort restores that prior value. An inner failure restores the outer pair; an outer failure leaves later spans without the exited pair. | `otel.py:300-326`; same-context nested and outer recovery proof at `test_observe_surface.py:482-501`. User exceptions remain unsuppressed because PyO3 exit always returns `false`. | PASS |
| Concurrent first entry, nested scopes, `await`, task concurrency, or task creation | Key creation is serialized once; values and token stacks are execution-local. Concurrent tasks cannot detach one another's entries. A task created inside a scope intentionally retains its copied correlation after the creator exits. | Double-checked key creation under the existing lock (`otel.py:201-212`); concurrent-thread, nested, await, concurrent-task, and copied-task proofs (`test_observe_surface.py:291-373,504-546`). | PASS |
| Python cancellation or user exception | Normal context-manager unwinding attempts cleanup and propagates the original exception; cleanup failures are swallowed only at the optional telemetry boundary. Abrupt interpreter termination performs no cleanup, but there is no server Run resource or shared durable context to strand. | `src/observe/mod.rs:219-239`; user-exception and public-exit parity proofs in `test_observe_surface.py:431-479` and the R2 evidence. | PASS |
| OTLP exporter or `/v1/traces` outage | Ambient trace delivery can fail or remain buffered according to the caller-owned provider. Explicit Wyrd observations and their Bifrost lifecycle are not converted into trace-export failures. The SDK adds no retry loop, second exporter, queue, or shutdown hook. | Ownership separation is explicit in the PyO3 boundary and journey. Recovery is caller/provider lifecycle: retry or flush the provider separately; context exit is not a durability acknowledgement. | PASS within the approved fail-open/no-provider-lifecycle boundary |
| Explicit Bifrost dependency outage | Explicit writes return the existing typed error and remain fail-closed; optional OTel handling neither hides nor replaces it. Other local Run/Card operations remain available. | Native `Observe` uses the state-owned writer and unchanged error surface (`crates/shared/wyrd-client/src/observe/mod.rs:102-159`); failure tests expect the ordinary `WYRD_SDK_400_BIFROST_NOT_STARTED` error rather than suppressing it. | PASS |
| Client process crash or restart | Execution-local OTel state, provider registration, the in-memory Run, and unflushed client buffers are lost. Already accepted server evidence remains under existing Bifrost durability. | No persistent Run or process-global Card scope was introduced. On restart the application rebuilds state, opens a new invocation, and installs correlation lazily. Graceful durability still requires explicit tracer flush, state shutdown, and server publication, in that order (`test_observe_journey.py:550-563`). | PASS |
| Server rejects an asserted Card outside signed scope | The trace request fails at the authenticated ingest boundary; the client does not fall back to another Card, UID, tenant, or principal. Unrelated server capabilities stay available. | The client writes only `wyrd.card_ref` and `wyrd.run_id` (`otel.py:186-187,226-229`); server-derived publisher/Card UID are verified by the persisted journey (`test_observe_journey.py:421-474`). | PASS |

No changed path treats a component error as permission to crash the Python
process or shared server. Conversely, the fail-open boundary does not weaken
Card lookup, authorization, input validation, or explicit write failures.

## Recovery and proof assessment

- Independently ran the focused Python surface file through the repository
  toolchain: **34 passed**. This directly covers registration, attach,
  enrichment, detach, concurrency, nesting, async propagation, copied task
  context, exception propagation, and exit signature parity.
- `git diff --check` passed for the cumulative base-to-candidate range.
- The immutable R2 implementation evidence records the authenticated
  Postgres-backed persisted journey, shared/Python suites, type/codegen/PyO3
  boundary checks, format, and lints as passing. This reviewer inspected the
  exercised path but did not rerun the Postgres-backed journey or broad lanes.
- Exporter outage and abrupt process death are assessed from actual ownership
  and lifecycle source. Adding exporter retries, health checks, or crash
  persistence would violate the approved non-goals rather than close a task
  requirement.

## Prior-finding closure

- `FIND-TASK-009-1`: closed. The actual raising registration branch now occurs
  during Run entry and is followed by representative explicit Drift proof.
- `FIND-TASK-009-2`: closed. The shared Run owner documents root, initially
  selected, and sibling-selected subjects.
- `FIND-TASK-009-3`: closed. Raising and swallowed detach failures restore the
  recorded prior correlation in the same execution context.
- `FIND-TASK-009-4`: closed. Concurrent first entries share one private key.
- `FIND-TASK-009-5`: closed without a topology change. Runtime and generated
  exit declarations agree on names and optional defaults.
- `FIND-TASK-009-6`: closed by documentation only; it has no runtime effect.

## Material proposed findings

None.

## Overall result

**PASS**

The cumulative candidate satisfies the approved deployed failure and recovery
boundaries. Optional Python telemetry failures remain contained to ambient
enrichment, while explicit observation integrity, lifecycle ownership, server
authorization, durability barriers, and unrelated capabilities retain their
existing behavior.
