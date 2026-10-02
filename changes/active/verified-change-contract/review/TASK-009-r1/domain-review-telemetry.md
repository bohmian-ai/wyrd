# Telemetry and Correlation Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `e4f30547906b8046bdccbfd38dff0eb0ee475d06`
- Task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Approved specification: `changes/active/verified-change-contract/spec.md` (`REQ-151`, `AC-032`)
- Locked Run contract: `changes/active/verified-change-contract/architecture/logic/run_api.md`

## Reviewed boundary and authority coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Python Run scope producer | `REQ-151`; `run_api.md` Python OpenTelemetry run scope; `architecture/wyrd-design.md` observation identity; `sdks/wyrd-sdk-python/src/observe/mod.rs` `PyRun::__enter__` / `__exit__`; `python/wyrd/otel.py` `_enter_run`, `_exit_run`, `_key` | FAIL: first-use key publication is not thread-safe. |
| Span processor and provider registration | `REQ-151`, `AC-032`; pinned OpenTelemetry API/SDK `1.42.1`; `otel.py` `_RunCorrelationProcessor`, `install_run_correlation`; unit provider/failure tests | PASS apart from the shared-key race below. Registration itself is locked and idempotent for supported providers. |
| Execution-context isolation and fail-open behavior | `REQ-151`; `run_api.md`; `test_observe_surface.py` nested, await, concurrent-task, spawned-task, missing-package, provider, enrichment, detach, and user-exception cases | PASS for the covered sequential and asyncio paths; thread-safe first initialization is missing. |
| Eval active-span identity | `run_api.md`; `sdks/wyrd-sdk-python/src/observe/mod.rs` `active_span_ids` and `PyObserveHandle::eval`; unit active/explicit ID cases | PASS. Valid Python OTel trace/span IDs are formatted to their exact fixed-width lowercase hex forms; explicit IDs remain authoritative. |
| OTLP export through persistence | `AC-032`; `architecture/references/domain/telemetry-observations.md`; `architecture/wyrd-design.md` lines 600-654; `architecture/bifrost-design.md` lines 39-61; `test_observe_journey.py` `otlp_provider`, `emit_framework_scope`, and `assert_scope_joins` | PASS by source inspection and recorded journey evidence. The journey uses the stock OTLP/HTTP exporter and persisted Bifrost queries rather than an in-memory substitute. |
| Server extraction and managed identity | `crates/vala/vala-bifrost-redux/src/tables/signal.rs` `RecordCorrelation::extract`; `tables/traces/projection.rs` `SpanColumns::push`; `scribe/execution_lanes.rs` Card-scope validation and UID resolution | PASS. Record-level final attributes are parsed, checked against signed Card scope, retained losslessly, and converted to server-stamped `card_uid`; tenant and publisher remain server-derived. |
| Trace/custom/Eval join spine | `test_observe_journey.py` persisted queries; trace projection; shared Run correlation | PASS when enrichment occurs: traces and custom rows join on `run_id`, and Eval joins on exact trace/span identity with the same Run and Card UID. |

## Material proposed findings

### D-TEL-001 — INCORRECT: concurrent first Run entries can use different OTel context keys

- **Violated obligation:** `REQ-151` requires the processor to copy the selected CardRef and invocation ID to every span started inside the scope, with execution-local concurrent use. `architecture/wyrd-design.md` lines 621-628 requires scoped correlation to reach spans created inside the scope.
- **Exact location:** `sdks/wyrd-sdk-python/python/wyrd/otel.py:195-207`, consumed by `_enter_run` at lines 274-291 and `_RunCorrelationProcessor.on_start` at lines 217-225.
- **Evidence:** `_scope_key` is lazily initialized by an unlocked check-then-create. `opentelemetry.context.create_key` returns a fresh UUID-backed key on every call. Two threads making the first Run entry can both observe `None`, attach correlation under different keys, and leave the later assignment in `_scope_key`. The processor subsequently reads only that last global key. A deterministic probe that forced both `create_key` calls to overlap produced one correlated span and one span with no `wyrd.card_ref` or `wyrd.run_id`. The existing concurrency test (`test_observe_surface.py:311-343`) starts asyncio tasks only after a sequential first entry has initialized the key, so it cannot detect this path.
- **Producer-to-sink trace:** `_key` produces distinct key A/key B -> each `_enter_run` stores its own `(card_ref, run_id)` under its returned key -> the global `_scope_key` retains only one key -> `_RunCorrelationProcessor.on_start` looks up only that key -> the other in-scope span exports without correlation -> `RecordCorrelation::extract` validly projects null correlation -> persisted `card_uid`/`run_id` are null and the trace cannot join the scope's custom or Eval evidence.
- **Observable consequence:** On concurrent first use in a multi-threaded Python process, a span created inside a valid Run scope can silently lose both required correlation attributes. The request still succeeds because generic telemetry permits null correlation, making the loss visible only as missing Card/Run joins.
- **Testable correction:** Serialize the one-time `_scope_key` creation (reusing the existing module lock is sufficient) so every caller receives the same key, while keeping OpenTelemetry import optional. Add one focused unit test that forces two first `_key` calls to overlap and proves spans from both independently entered thread-local scopes receive their own exact CardRef and shared/selected Run ID. No server or new abstraction is needed.

## Verification limits

- Independently ran `mise exec -- uv run --project sdks/wyrd-sdk-python python -m pytest -q sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py`: **30 passed**.
- Independently reproduced D-TEL-001 with the pinned OpenTelemetry `1.42.1` runtime by forcing concurrent first calls to `create_key`; one finished span had exact Wyrd attributes and the sibling had none.
- Did not rerun the Postgres-backed OTLP journey or any Cargo command in this sub-review. The task records the exact journey and broader Python integration lanes as passing; source inspection confirms those assertions exercise persisted rows and the authenticated endpoint.
- Raw-thread context propagation is explicitly outside the contract. The finding does not require propagation into a newly created thread: each affected thread directly enters its own Run scope, so it is within the context-manager contract.

## Overall result

**FAIL** — the sequential, asyncio, fail-open, authenticated ingest, managed identity, and persisted join paths are otherwise supported, but D-TEL-001 violates the required in-scope span-correlation guarantee on a reachable concurrent first-use path.
