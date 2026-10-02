---
id: TASK-009
kind: implementation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 35
requirements: [REQ-123, REQ-151, INV-007, INV-012, AC-032]
depends_on: [TASK-002]
---

## Outcome and Value

Developers can select the initial Card when opening a Run, and Python users can
wrap arbitrary agent-framework execution in `with state.run(card="...")` so
framework-created OpenTelemetry spans reach Bifrost with the exact CardRef and
invocation ID. The integration remains optional and fail-open: applications,
Runs, and explicit Wyrd observations work unchanged when Python OpenTelemetry
is absent or broken.

One production-shaped Python journey proves the complete user outcome: create
a Service, export framework spans to Wyrd's authenticated `/v1/traces`
endpoint, emit custom data and Eval evidence in the same Run scope, and query
persisted Bifrost rows that join those signals by their correlation identity.

## Owners, Scope, Consumers, and Prohibited Changes

Shared `wyrd-client` owns Run identity, local hydrated-Card selection, and the
language-neutral initial-Card behavior. Rust, Python, and TypeScript project
that behavior idiomatically. The Python SDK owns its PyO3 context-manager
surface and Python-runtime OpenTelemetry integration. Vala retains its existing
per-record extraction, signed Card-scope authorization, and server-derived
managed identity.

Do not add a server Run resource, another Bifrost client or queue, a mandatory
OpenTelemetry dependency, a wrapper span, process-global Card scope, implicit
flush/shutdown, ambient log or metric enrichment, or client-authored tenant,
principal, Card UID, or request identity. Do not reopen TASK-002 or rewrite its
evidence.

The approved public API is:

```text
Python
  WyrdState.run(*, card: str | None = None) -> Run
  Run.for_card(alias: str) -> Run
  Run.__enter__() -> Run
  Run.__exit__(exc_type, exc_value, traceback) -> False
  wyrd.otel.install_run_correlation(provider=None) -> bool

TypeScript
  WyrdState.run(card?: string): Run
  Run.forCard(alias: string): Run

Rust
  WyrdState::run(&self) -> Run
  WyrdState::run_for_card(&self, alias: &str) -> Result<Run, WyrdError>
  Run::for_card(&self, alias: &str) -> Result<Run, WyrdError>
```

`for_card` remains the multi-Card operation for immutable views that share one
`run_id`. Python `with` changes ambient span correlation only and is not
required for explicit `run.observe` calls.

## Approach

1. Extend the shared Run surface with initial hydrated-Card selection and
   project it through all three first-class SDKs without changing root or
   immutable sibling-view behavior.
2. Add the Python synchronous context-manager behavior defined by REQ-151,
   keeping Run identity in shared Rust and optional Python OpenTelemetry work at
   the foreign-runtime boundary.
3. Apply `wyrd.card_ref` and `wyrd.run_id` to an active span and spans created
   within the scope, with correct nesting, asyncio propagation, provider
   idempotency, and restoration.
4. Contain every optional OpenTelemetry absence or failure without weakening
   Card lookup, authorization, validation, or explicit observation errors.
5. Extend the existing Python scoped-observation journey to export through
   `/v1/traces` and prove persisted trace/custom/Eval joins.

## Ordered Implementation Scenarios

### Scenario 1 — Initial Card selection is local and immutable

**Behavior.** Python `run(card=...)`, TypeScript `run(card?)`, and Rust
`run_for_card(...)` select an exact Card from the hydrated graph while minting
one UUIDv7 invocation ID. Root construction remains the default; later sibling
views share the same ID; unknown aliases fail locally without network IO.

**RED.** Add focused cases in the existing shared Run and language SDK Run test
homes for initial component selection, root default, sibling identity, and
unknown alias. They fail because only root construction and later `for_card`
selection exist.

**GREEN.** Add the minimum shared behavior and thin SDK projections needed for
those public calls while preserving every existing Run test.

**REFACTOR.** Reuse the hydrated graph and immutable view semantics; remove any
duplicated language-boundary Card resolution exposed by the passing cases.

### Scenario 2 — Python scope correlates active, child, nested, and async spans

**Behavior.** Entering a Run returns that Run, applies its exact CardRef and
run ID to an already-active recording span and spans created inside the scope,
and restores the previous correlation on exit. Nested Card scopes share the Run
ID and restore the outer Card. Correlation survives `await` and remains isolated
across concurrent tasks using the same immutable Run.

**RED.** Extend the existing Python Run surface tests with real Python
OpenTelemetry spans covering entry, child creation, nesting, `await`, concurrent
tasks, and a task created inside the scope. The spans lack Wyrd correlation and
Run lacks context-manager behavior before implementation. Run the focused file:

```bash
mise run py:setup
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py)
```

**GREEN.** Implement the approved context behavior so the new cases and prior
Run/observation cases pass without adding an async-only context-manager API.

**REFACTOR.** Keep scope state execution-local and preserve one immutable Run
model across synchronous and asynchronous use.

### Scenario 3 — Optional OpenTelemetry fails open

**Behavior.** Missing OpenTelemetry packages, an API-only or unsupported
provider, registration failure, span enrichment failure, and exit
context-update failure do not escape or block explicit Wyrd observations. A
user exception propagates unchanged. Unknown Card aliases still fail. Run
entry marks the global provider, and `install_run_correlation` marks an
explicitly supplied private provider, so repeated entry registers once; a
duplicate from a concurrent first entry is harmless.

**RED.** Add failure-injection cases beside the Python Run surface cases. They
fail until optional integration failures are contained and repeated entry
skips a marked provider. Use the same focused Python command from Scenario
2.

**GREEN.** Contain only the optional telemetry integration failures while
leaving existing Card and observation failures visible.

**REFACTOR.** Delete warnings, retries, fallback spans, or configuration
objects that the behavioral cases do not require.

### Scenario 4 — Real OTLP traces join custom and Eval evidence

**Behavior.** The existing Python scoped-observation journey creates and
registers its Service graph, configures stock Python OpenTelemetry export to
the Card-bound service's authenticated `POST /v1/traces` endpoint, and enters
`state.run(card="agent")`. Framework-style code creates spans without Wyrd
attributes, writes one caller-owned custom row, and emits one Eval observation
without explicit trace/span IDs. After scope exit, explicit tracer flush,
`WyrdState.shutdown()`, and server publication, persisted queries prove:

- trace rows carry the exact Run ID, asserted CardRef, authenticated publisher,
  and server-resolved Agent Card UID;
- the custom row joins those traces by `run_id`; and
- the Eval row joins its active span by exact `trace_id` and `span_id`, with the
  same Run ID and Card UID.

Context exit restores correlation but is not treated as a durability barrier.

**RED.** Extend the existing
`test_scoped_run_emits_drift_eval_and_generic_rows` journey with stock OTLP/HTTP
export and the three persisted assertions. It initially fails because Run is
not a context manager and framework spans do not receive Wyrd correlation. Run
the exact journey:

```bash
mise run py:setup:testing
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/state/test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows'
```

**GREEN.** Complete the SDK correlation and test integration needed for the
existing real Service, `/v1/traces`, Bifrost publication, and public query path
to satisfy those assertions without changing Gate or Scribe semantics.

**REFACTOR.** Reuse the journey's Service graph, credential, custom table, Eval
emission, query client, and lifecycle barriers; do not create a parallel
fixture or an in-memory substitute for persisted proof.

## Acceptance Criteria

- `state.run(card=...)` is the concise single-Card form; immutable
  `for_card`/`forCard` remains the same-run multi-Card form.
- Python framework spans inside a Run scope carry record-level
  `wyrd.card_ref` and `wyrd.run_id`; Bifrost resolves their Card UID under the
  existing signed scope.
- The real Python journey exports through authenticated `/v1/traces` and proves
  persisted trace-to-custom-data and trace-to-Eval joins.
- Optional Python OpenTelemetry failures never escape or block explicit Wyrd
  observations, while invalid Card identity and authorization remain strict.
- Nested and asyncio scopes restore and isolate correlation correctly.
- Context exit is not a flush, shutdown, or durability acknowledgement.
- No required dependency, second telemetry pipeline, wrapper span, or
  log/metric promise is added.

## Expected Write Set and Consumer Closure

Likely owners are the shared `wyrd-client` Run/state surface; Rust, Python, and
TypeScript SDK Run projections and generated public typing; the Python SDK's
existing OpenTelemetry integration; shared and language Run tests; and the
existing Python scoped-observation journey. Vala extraction changes only if
needed to consume the same exact correlation attribute contract; its schema,
authorization, and server-derived identity remain unchanged. The Python
development/test dependency set and lockfile may add the stock OTLP/HTTP
exporter needed by the journey; production dependencies remain optional and
unchanged.

## Verification and Evidence

Run the focused commands in Scenarios 2–4 and each new Rust test by its exact
final name. Then run:

```bash
mise run test:shared
mise run test:wyrd-sdk
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run ts:test:unit
mise run ts:test:integration
mise run ts:typecheck
mise run ts:napi:check
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run fmt
mise run py:format
mise run lints
mise run py:lints
git diff --check
```

## Material Stop Conditions

Stop if correct enrichment requires mandatory OpenTelemetry installation, a
new server or Bifrost contract, client-trusted Card UID/tenant/principal data,
provider lifecycle ownership, a second exporter/queue, or mutable process-
global Card scope. Stop if the authenticated `/v1/traces` journey cannot join
the existing trace, custom, and Eval schemas using their approved identities;
do not add a new correlation field without specification authority. Stop if a
target framework hides a private provider that cannot use the explicit
installation hook; do not monkey-patch it.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/architecture/logic/run_api.md`
- `architecture/wyrd-design.md`
- `architecture/bifrost-design.md`
- `architecture/references/domain/telemetry-observations.md`
- `architecture/references/languages/testing-workflows.md`
- `AGENTS.md`

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `state.run(card=...)` is the single-Card form; `for_card`/`forCard` stays the same-run multi-Card form | `crates/shared/wyrd-client/src/state.rs` (`WyrdState::run_for_card`), `observe/mod.rs` (`Run::new(state, subject)`); Python `src/state/mod.rs` `run(*, card=None)`; TS `native/src/cards.rs` + `wyrd/src/index.ts` `run(card?)`; Rust SDK re-exports the shared state | `observe::tests::run_for_card_selects_the_initial_view_and_shares_its_invocation`, `observe::tests::run_for_card_refuses_an_unknown_alias_without_network_io`; Python `test_run_card_*`; TS `observe-run.test.ts`; Rust SDK `observe_run::scoped_run_emits_drift_eval_and_generic_rows` (`assert_initial_card_selection`) | PASS |
| Python framework spans in a Run scope carry `wyrd.card_ref`/`wyrd.run_id`; Bifrost resolves Card UID under the signed scope | `sdks/wyrd-sdk-python/src/observe/mod.rs` `__enter__`/`__exit__` delegate to `python/wyrd/otel.py` (`_enter_run`, `_exit_run`, `_RunCorrelationProcessor`, `install_run_correlation`) | `test_entering_a_run_returns_it_and_correlates_active_and_child_spans`; journey `assert_scope_joins` (persisted `card_uid`, asserted CardRef from the lossless `attributes` payload) | PASS |
| Real Python journey exports via authenticated `/v1/traces` and proves trace-to-custom and trace-to-Eval joins | `tests/integration/state/test_observe_journey.py` (`otlp_provider`, `emit_framework_scope`, `assert_scope_joins`); dev group adds `opentelemetry-exporter-otlp-proto-http` | exact journey command (Scenario 4) exit 0; `py:test:integration` 72 passed | PASS |
| Optional OTel failures never escape or block explicit observations; Card identity stays strict | `otel.py` contains every optional failure; Rust boundary swallows only the `wyrd.otel` call result | `test_missing_opentelemetry_is_a_no_op`, `test_unsupported_providers_are_refused_without_raising`, `test_enrichment_and_detach_failures_never_escape`, `test_run_card_refuses_an_unknown_alias` | PASS |
| Nested and asyncio scopes restore and isolate correlation | execution-local `ContextVar` token stack in `otel.py`; nothing stored on `Run` | `test_nested_card_scopes_share_the_run_and_restore_the_outer_card`, `test_scope_survives_await_and_isolates_concurrent_tasks` | PASS |
| Context exit is not a flush, shutdown, or durability acknowledgement | `__exit__` only detaches; journey flushes the provider, calls `state.shutdown()`, then `server.flush_bifrost()` before reading | journey ordering; stub docs | PASS |
| No required dependency, second pipeline, wrapper span, or log/metric promise | production `dependencies` unchanged (`otel` extra still optional); processor duck-typed, registered on the caller's provider | `py:test:unit`, `check:client-tier`, `check:pyo3-scope` | PASS |

Verification (all exit 0): focused Rust `cargo nextest run --locked -p wyrd-client --lib -E 'test(/observe::tests::run_for_card/)'`;
`uv run python -m pytest -q tests/unit/state/test_observe_surface.py` (30 passed); the Scenario 4 journey command;
`cargo nextest run --locked -p wyrd-sdk-rust --test observe_run -P journey --run-ignored=all -E 'test(=scoped_run_emits_drift_eval_and_generic_rows)'` under the Postgres wrapper;
`mise run test:shared`, `test:wyrd-sdk`, `py:test:unit`, `py:test:integration`, `py:typecheck`, `ts:test:unit`, `ts:test:integration`, `ts:typecheck`, `ts:napi:check`, `codegen:check`, `check:client-tier`, `check:pyo3-scope`, `fmt`, `py:format`, `lints`, `py:lints`, `git diff --check`.

Exact named Rust proof (TASK-009-R5, all exit 0):

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=observe::tests::run_for_card_selects_the_initial_view_and_shares_its_invocation)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=observe::tests::run_for_card_refuses_an_unknown_alias_without_network_io)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test observe_run \
  -P journey --run-ignored=all -E 'test(=scoped_run_emits_drift_eval_and_generic_rows)'"
```

Notes: `vala.traces.spans` persists no `card_ref` column; Scribe resolves the asserted ref to `card_uid`, so the journey reads the asserted CardRef back from the span's lossless `attributes` payload. The journey uses a private provider with `install_run_correlation(provider)` because OpenTelemetry's global provider is set-once per process; unit tests cover the global-provider path. Non-goals (server Run resource, second exporter/queue, wrapper span, global Card scope, log/metric enrichment, client-authored identity) remain excluded.

Revision 47 simplification (2026-10-02, human-approved, review waived): `otel.py` registration is now a private `_wyrd_run_correlation` marker on the provider object plus one stateless `_RunCorrelationProcessor`; the outcomes registry, weakrefs, lock, and inert processors are removed, and TASK-009-R6 is superseded. Tests for dropped guarantees (accept-then-raise attempt counts, inert retained processor, equal-provider identity) were deleted; `test_repeated_entry_registers_once_on_a_marked_provider`, `test_unsupported_providers_are_refused_without_raising` (adds an unmarkable provider), and `test_registration_and_attach_failures_never_block_observations` (each failing provider paired with an explicit Drift observation and a propagated user exception) cover the three guarantees. All exit 0: `uv run python -m pytest -q tests/unit/state/test_observe_surface.py` (36 passed); the Scenario 4 journey command after `mise run py:setup`; `py:test:unit` (515 passed); `py:test:integration` (72 passed); `py:typecheck`; `codegen:check`; `check:pyo3-scope`; `fmt`; `lints`; `py:format`; `py:lints`; `git diff --check`.
