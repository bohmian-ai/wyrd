# Telemetry and observation domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `017a54d5390eb488e820f890bbb953a5f1ca3a53`
- Candidate rechecked before writing: `HEAD` remained the candidate commit.
- Scope: the cumulative TASK-009 implementation and R1/R2/R3 remediations,
  reviewed for the telemetry/observation boundary only.

## Boundary and authority coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Run identity and immutable Card views | `changes/active/verified-change-contract/spec.md` REQ-123/REQ-151/AC-032; `changes/active/verified-change-contract/architecture/logic/run_api.md`; `crates/shared/wyrd-client/src/state.rs`; `crates/shared/wyrd-client/src/observe/mod.rs`; Rust/Python/TypeScript projections and Run tests | PASS — one UUIDv7 `RunId` is minted by shared Rust, initial and later Card selection resolve against the hydrated graph, and immutable views carry their own exact `CardRef` while sharing the invocation ID. Unknown aliases fail locally before Python scope entry. |
| Python execution-local OTel scope | REQ-151; run API authority; `sdks/wyrd-sdk-python/src/observe/mod.rs`; `sdks/wyrd-sdk-python/python/wyrd/otel.py`; public stubs | PASS — `__enter__` delegates only optional telemetry work, returns the same Run, and `__exit__` restores scope and returns `False`. Tokens and prior values live in a `ContextVar` stack rather than on `PyRun`; the same Run object can therefore be entered independently by concurrent asyncio tasks. |
| Active and child span enrichment | REQ-151/AC-032; `otel.py::_enter_run`; `_RunCorrelationProcessor::on_start`; pinned `opentelemetry-api`/`opentelemetry-sdk` 1.42.1 source in the installed environment | PASS — entry stamps an already-active recording span, and the registered processor reads the OTel parent context supplied to `on_start` and overwrites the exact record-level keys `wyrd.card_ref` and `wyrd.run_id` on newly started spans. The duck-typed processor implements the 1.42.1 lifecycle hooks used by the SDK, including `_on_ending`, `on_end`, `shutdown`, and `force_flush`. |
| Provider registration and lifecycle ownership | REQ-151; `install_run_correlation`; provider tests; integration provider setup | PASS — registration is lock-serialized, weakly tracked, idempotent per provider, and retries after a failed registration. The SDK does not flush or shut down caller-owned providers; the journey performs those lifecycle barriers explicitly. Global and explicitly supplied private providers are both covered. |
| Failure containment without weakening explicit observation | REQ-151/AC-032; PyO3 entry/exit boundary; `otel.py`; failure-focused unit tests | PASS — missing imports, unsupported/API-only providers, registration failure, attach failure, span lookup/enrichment failure, and raising or silently swallowed detach failure remain contained inside the optional telemetry path. Card selection, validation, authorization, and ordinary Drift/Eval/record errors continue through their existing strict paths. User exceptions are not suppressed. |
| Async, nesting, and restoration | REQ-151/AC-032; `ContextVar` token stack; nested, copied-task, distinct-view, same-Run, concurrent-first-use, and detach-recovery tests | PASS — `await` and task copying use Python execution context, nested Card scopes restore their predecessor, concurrent tasks entering the identical immutable Run exit independently, and scope-key creation is serialized. No raw-thread propagation promise was added. |
| OTLP record correlation and signed scope resolution | `architecture/wyrd-design.md` observation identity; `architecture/references/domain/telemetry-observations.md`; `architecture/bifrost-design.md`; existing Gate/Scribe correlation owners; integration journey | PASS — only record-level `wyrd.card_ref` and `wyrd.run_id` are client-injected. The authenticated endpoint derives tenant, principal, request identity, and authoritative Card UID; the journey uses a registered Service credential whose signed scope includes the selected Agent and proves persisted spans resolve to that Agent UID. No client tenant/principal/Card UID field or alternate correlation path was introduced. |
| Persisted Bifrost joins | AC-032; `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py` (`emit_framework_scope`, `assert_scope_joins`) | PASS — stock OTLP/HTTP export reaches authenticated `POST /v1/traces`; persisted span rows retain the asserted CardRef in the lossless attribute payload and the exact run ID plus server-resolved Card UID. The caller-owned dataset joins spans by run ID, and the Eval row joins the active tool span by exact trace/span IDs while retaining the same run and Card identity. Scope exit is not used as a durability acknowledgement. |
| Non-goals and adjacent signals | TASK-009 prohibited changes; run API authority | PASS — no server Run, wrapper span, second exporter/queue, mandatory OTel runtime dependency, process-global Card scope, implicit flush/shutdown, log/metric enrichment promise, or provider lifecycle takeover entered the cumulative source diff. |

## End-to-end trace

1. `WyrdState::run_for_card` resolves the hydrated alias before constructing
   `Run`; `Run::new` mints the invocation `RunId`, and later `for_card` views
   clone that ID while selecting another exact subject.
2. Python `PyRun.__enter__` passes only the native view's serialized CardRef and
   Run ID to `wyrd.otel._enter_run`. That function installs the processor on
   the current global provider when supported, attaches the pair to the OTel
   execution context, records the exact token/prior value, and stamps an
   already-active recording span.
3. OTel 1.42.1 calls the processor synchronously at span start with the parent
   context. `_RunCorrelationProcessor.on_start` reads the scoped pair from that
   context and sets the two Bifrost-recognized record attributes. All lookup or
   span mutation errors are swallowed at this optional boundary.
4. The stock OTLP/HTTP exporter sends those span attributes to the authenticated
   traces endpoint. Existing table projection uses the final record-level
   values; signed Card scope authorizes the asserted CardRef and supplies the
   authoritative `card_uid`, while authenticated authority supplies tenant and
   publisher and `run_id` passes through opaquely.
5. The journey reads persisted Bifrost rows and proves span correlation, the
   custom-row run join, and the Eval trace/span join. Explicit provider flush,
   state shutdown, and server publication occur before readback.

## Verification executed

- `mise run py:setup`
- `cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py` — **35 passed**
- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/state/test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows'` — migration check **1 passed**; journey **1 passed**

The installed dependency source was sufficient for the version-sensitive OTel
API check: `uv.lock` pins `opentelemetry-api`, `opentelemetry-sdk`, and the
OTLP/HTTP exporter to 1.42.1, and the inspected installed `SpanProcessor`,
`context.attach`, and `context.detach` definitions match the implementation's
hooks and its recovery for the public `detach` function logging and swallowing
runtime reset failures.

## Verification limits

- This domain pass did not rerun the full repository gate or every language SDK
  lane. Those are broader orchestration evidence, not needed to resolve a
  telemetry-domain uncertainty after the focused Python suite and the real
  authenticated persisted journey passed.
- The journey deliberately uses a private provider because the OTel global
  provider is process-set-once; the focused suite separately proves automatic
  global-provider registration and the same attribute behavior.
- Raw-thread context propagation and ambient log/metric enrichment are explicit
  non-goals. They are not treated as missing proof.

## Material findings

None.

## Overall result

**PASS**

The candidate satisfies the approved telemetry/observation boundary: exact
Run/Card correlation reaches active and child spans, survives and isolates the
required execution contexts, fails open only at the optional OTel seam, remains
strict at explicit observation and authorization seams, resolves through signed
scope, and is proven by persisted Bifrost joins.
