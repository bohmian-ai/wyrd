# TASK-009 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `017a54d5390eb488e820f890bbb953a5f1ca3a53`
- Approved specification: `changes/active/verified-change-contract/spec.md`
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Prior remediation tasks: `TASK-009-R1-restore-otel-correlation.md`, `TASK-009-R2-close-proof-and-boundary-parity.md`, and `TASK-009-R3-prove-same-run-async-isolation.md`
- Applicable interface authority: `changes/active/verified-change-contract/architecture/logic/run_api.md`

The candidate remained `017a54d5390eb488e820f890bbb953a5f1ca3a53` throughout this review. The cumulative range was reviewed; prior findings were treated as hypotheses and checked against the final source and proof rather than accepted from their summaries.

## Navigation and caller-to-result paths

| Path | Producer / owner | Consumers and observable result |
|---|---|---|
| Initial Card selection | `crates/shared/wyrd-client/src/state.rs:493-511` resolves the root or alias before `Run::new`; `crates/shared/wyrd-client/src/observe/mod.rs:43-89` owns the invocation and immutable view | Python `src/state/mod.rs:177-194`, TypeScript native `native/src/cards.rs:298-313` and public `wyrd/src/index.ts:1854-1866`, and the Rust SDK all project the same shared owner. Every later `for_card` retains the minted `run_id`; unknown aliases return `WYRD_SDK_404_UNKNOWN_ALIAS` before a Run is opened. |
| Python scope entry/exit | `sdks/wyrd-sdk-python/src/observe/mod.rs:195-240` delegates only the foreign-runtime work | `python/wyrd/otel.py:279-326` attaches/restores execution-local correlation; ordinary `Run.observe` remains the shared Rust path. |
| Child span enrichment | `python/wyrd/otel.py:201-243` owns one private context key and a stateless processor | A global provider is installed on Run entry; `install_run_correlation` at `:246-276` supports a private provider. Started spans receive the exact scoped CardRef and Run ID. |
| Persistent proof | `tests/integration/state/test_observe_journey.py:365-474` configures stock OTLP/HTTP export and asserts stored rows | The existing journey at `:477-548` uses one registered Service graph, a Card-bound credential, explicit tracer flush, state shutdown, publication, and public Bifrost queries to prove trace/custom/Eval joins. |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-123: Rust, Python, and TypeScript expose the approved root and initial-Card Run forms without duplicating Run ownership | Shared `WyrdState::run` / `run_for_card` in `state.rs:493-511`; Python `run(*, card=None)` in `src/state/mod.rs:177-194`; TypeScript `run(card?)` in `native/src/cards.rs:298-313` and `wyrd/src/index.ts:1854-1866`; Rust SDK consumes the shared type | Focused Rust selection tests passed during this review: 2/2. Python focused surface passed 35/35. Candidate evidence records the Rust SDK and TypeScript journey assertions for root, selected alias, shared sibling identity, and unknown alias. | PASS |
| REQ-123: a Run mints one UUIDv7 ID, defaults to the root, resolves initial and later aliases locally, and keeps sibling views immutable | `Run::new` mints once and `Run::for_card` clones the same ID in `observe/mod.rs:55-89`; `run_for_card` resolves before construction in `state.rs:508-511` | `observe/tests.rs:648-690`; Python `test_run_card_selects_the_initial_view_and_shares_its_invocation` and `test_run_card_refuses_an_unknown_alias`; Rust/TS journey assertions | PASS |
| REQ-151: Python `Run` is a synchronous context manager; entry returns the same Run and never starts a wrapper span | PyO3 `__enter__` returns `slf` after delegating at `src/observe/mod.rs:195-217`; `_enter_run` only installs, attaches, and stamps at `otel.py:279-297` | `test_entering_a_run_returns_it_and_correlates_active_and_child_spans` passed in the 35-test focused file; exported names contain only caller-created spans | PASS |
| REQ-151 / AC-032: the already-active recording span and every span started in scope carry exact `wyrd.card_ref` and `wyrd.run_id`, replacing conflicting values | `_enter_run` stamps the active span at `otel.py:291-294`; `_RunCorrelationProcessor.on_start` stamps child spans at `:222-231` | `test_entering_a_run_returns_it_and_correlates_active_and_child_spans`; persisted journey asserts both framework spans' exact asserted CardRef and run ID at `test_observe_journey.py:424-437` | PASS |
| REQ-151 / AC-032: nested scopes, `await`, task creation, sibling concurrent tasks, and concurrent tasks entering the identical immutable Run restore and isolate correlation | Execution-local `_scope_tokens` stack at `otel.py:189-198`, paired entry/exit at `:279-326`; no token is stored on `PyRun` | `test_nested_card_scopes_share_the_run_and_restore_the_outer_card`, `test_scope_survives_await_and_isolates_concurrent_tasks`, and `test_concurrent_tasks_entering_the_same_run_exit_independently` all passed. The last deterministically checks one task after exit while the other remains entered and both after exit. | PASS |
| REQ-151: global-provider setup is automatic; explicit private-provider installation is thread-safe and idempotent per provider | `_enter_run` calls `install_run_correlation`; weak provider registry and lock at `otel.py:197-198,246-276` | `test_global_and_private_providers_receive_one_processor_each` and `test_concurrent_first_entries_share_one_scope_key` passed | PASS |
| REQ-151 / AC-032: missing API, API-only provider, registration failure, attach failure, enrichment failure, and detach failure fail open and do not block explicit observations | Optional calls are contained in `otel.py:201-326`; PyO3 import/call errors are discarded at `src/observe/mod.rs:205-216,225-239`; explicit observations remain shared Rust calls | Focused passing cases: `test_missing_opentelemetry_is_a_no_op`, `test_registration_and_attach_failures_never_block_observations`, `test_enrichment_failure_never_blocks_observations`, and `test_detach_failure_restores_the_prior_correlation`. Each required failure class reaches the ordinary explicit Drift error; registration attempt and post-detach behavior are asserted. | PASS |
| REQ-151 / AC-032: user exceptions propagate; unknown aliases and ordinary Wyrd validation/write failures remain strict | `__exit__` always returns false at `src/observe/mod.rs:225-239`; Card resolution remains outside telemetry in `src/state/mod.rs:187-193` | Failure tests assert unchanged `ValueError` propagation and `WYRD_SDK_404_UNKNOWN_ALIAS`; generated stub and runtime keyword/default parity are exercised by `test_run_exit_accepts_conventional_keywords_and_omitted_arguments` | PASS |
| REQ-151: failed detach restores the preceding pair rather than leaking the exited scope | Entry records `(token, prior)` and exit checks/re-attaches the prior value at `otel.py:289-290,307-326` | `test_detach_failure_restores_the_prior_correlation` proves swallowed inner detach restores the outer Card and raising outer detach leaves later spans uncorrelated; explicit observations still reach their normal boundary | PASS |
| AC-032: a real Python SDK-to-server journey exports stock OTLP/HTTP spans to authenticated `/v1/traces` and persists the exact managed identities | `otlp_provider` uses `OTLPSpanExporter` and a Card-bound exchanged token at `test_observe_journey.py:353-381`; `emit_framework_scope` creates caller spans without Wyrd attributes at `:384-402` | Candidate-recorded exact journey passed. Persisted queries at `:413-474` assert two spans, exact asserted CardRef/run ID, one authenticated publisher, server-resolved Agent UID, custom-row join by `run_id`, and Eval join by exact trace/span IDs plus matching run/Card identity. | PASS |
| AC-032: context exit is not a durability barrier | `__exit__` only delegates `_exit_run`; it performs no flush, shutdown, transport, or server operation | Journey explicitly calls `provider.force_flush()`, then `state.shutdown()`, then `server.flush_bifrost()` before queries | PASS |
| INV-007: signed Card scope and server-derived publisher/Card UID remain authoritative | Client injects only `_CARD_REF` and `_RUN_ID`; no tenant, principal, Card UID, or request identity was added | Persisted journey authenticates with the registered Service's Card-bound credential and asserts non-null publisher plus the server-resolved Agent UID | PASS |
| INV-012: existing observation, Eval active-span capture, Bifrost ingest, and server resolution are reused rather than replaced | No Vala/Gate/Scribe or observation record semantics changed; Python scope delegates to the existing shared Run/Observe and existing `/v1/traces` path | Existing Python scoped-observation journey was extended in place; its pre-existing Drift/Eval/generic behavior and negative flows remain in the same test | PASS |
| Non-goals: no server Run, second pipeline/queue/exporter owner, wrapper span, mandatory OTel dependency, global Card scope, implicit lifecycle action, or log/metric promise | Production dependency remains `pyarrow`; OTel API remains in the optional `otel` extra, with the OTLP/HTTP exporter added only to dev dependencies. The implementation uses caller/global providers and execution-local context. No server or Vala production file changed. | Diff inspection; focused tests assert no post-scope correlation and only caller-created spans. Candidate evidence records client-tier, PyO3-scope, codegen, typecheck, format, lint, and language journey lanes passing. | PASS |
| Prior findings 1-7 are closed in the cumulative candidate | 1: complete failure/observation proof; 2: corrected `Run::subject` invariant; 3: prior restoration after detach failure; 4: locked one-time key creation; 5: runtime/stub `__exit__` parity; 6: Rust test panic docs; 7: identical-Run concurrent-task proof | The focused Python file passed 35/35 and the exact shared Rust selection expression passed 2/2 during this review; the source and named tests directly cover every prior diagnosis | PASS |

## Proposed findings

None. I found no reachable missing, incorrect, drifting, violating, or regressing behavior in the cumulative candidate.

## Verification assessment

Commands run during this review:

```text
mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py
# 35 passed

mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(/observe::tests::run_for_card/)'
# 2 passed
```

The candidate's immutable implementation records additionally report the exact authenticated Python journey, Rust SDK journey, TypeScript integration, Python integration/unit/typecheck, TypeScript unit/integration/typecheck/N-API, shared tests, codegen, boundary checks, format, lint, and diff checks passing. No verification limitation changes the result.

## Overall result

**PASS**

The cumulative candidate satisfies the original TASK-009 behavior, closes all seven prior findings, preserves the explicit non-goals, and has direct proof for the realistic initial-selection, OpenTelemetry correlation, failure, concurrency, and persisted-join paths.
