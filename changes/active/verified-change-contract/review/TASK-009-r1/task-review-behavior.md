# TASK-009 Behavior Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `e4f30547906b8046bdccbfd38dff0eb0ee475d06`
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Approved authority: `SPEC-verified-change-contract` revision 35, especially REQ-123, REQ-151, INV-007, INV-012, and AC-032; `changes/active/verified-change-contract/architecture/logic/run_api.md`
- Review scope: complete cumulative base-to-candidate diff

## Proposed findings

No material behavior findings.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-123: Rust, Python, and TypeScript expose the approved initial-Card Run selection while retaining root-default and immutable same-invocation sibling views | `crates/shared/wyrd-client/src/state.rs:489-511` resolves an alias before constructing `Run`; `crates/shared/wyrd-client/src/observe/mod.rs:44-91` keeps subject and `run_id` on immutable views; Python `src/state/mod.rs:154-172` and TypeScript native/public projections delegate to that shared owner | Shared unit cases cover root/default, selected Card, sibling identity, fresh invocation identity, and unknown aliases; Rust, Python, and TypeScript journey code exercises the public projections | PASS |
| REQ-123: unknown or out-of-graph aliases fail locally without opening a Run or performing network IO | `WyrdState::run_for_card` calls the hydrated graph's `card_ref(alias)?` before `Run::new`; all language projections preserve the structured error | Shared `run_for_card_refuses_an_unknown_alias_without_network_io`; Python `test_run_card_refuses_an_unknown_alias`; TypeScript and Rust journey assertions | PASS |
| REQ-151: Python `Run` is a synchronous context manager that returns itself and never suppresses a user exception | `sdks/wyrd-sdk-python/src/observe/mod.rs:133-176` delegates entry/exit to `wyrd.otel`, returns the same bound object, swallows only optional telemetry errors, and returns `false` | `test_entering_a_run_returns_it_and_correlates_active_and_child_spans`; `test_enrichment_and_detach_failures_never_escape` checks unchanged user-exception propagation and `False` | PASS |
| REQ-151: entry installs exact `wyrd.card_ref` and `wyrd.run_id` correlation on the active recording span and spans started in scope, overriding conflicting initial values | `sdks/wyrd-sdk-python/python/wyrd/otel.py:186-238,274-291` attaches the exact pair, stamps the active span, and has the registered processor copy the parent-context pair on start | `test_entering_a_run_returns_it_and_correlates_active_and_child_spans` covers active, child, grandchild, and a conflicting initial CardRef | PASS |
| REQ-151 / AC-032: nested scopes restore the outer Card; correlation survives `await`, concurrent tasks, and a task created in scope that runs after exit | Execution-local OTel context plus `_scope_tokens: ContextVar[...]` in `otel.py:189-207,274-307`; no attach token is stored on `PyRun` | `test_nested_card_scopes_share_the_run_and_restore_the_outer_card`; `test_scope_survives_await_and_isolates_concurrent_tasks` | PASS |
| REQ-151: global and explicitly supplied private providers receive at most one processor and unsupported providers fail open | `install_run_correlation` in `otel.py:241-271` uses a lock and weak per-provider registration set; `_enter_run` invokes it for the global provider | `test_global_and_private_providers_receive_one_processor_each`; `test_unsupported_providers_are_refused_without_raising` | PASS |
| REQ-151 / AC-032: missing packages, provider-registration failure, processor/enrichment failure, and detach failure do not escape or weaken Card lookup, validation, or explicit observation errors | Optional imports and each OTel operation are contained in `otel.py:217-226,241-307`; PyO3 entry/exit also discard helper-call errors; Card resolution remains in shared Rust before context entry and observation calls retain their existing errors | Focused missing-package, unsupported/failing-provider, enrichment/detach, unknown-alias, and explicit-observation error cases | PASS |
| AC-032: a real Python SDK-to-server journey exports stock OTLP/HTTP spans to authenticated `/v1/traces` and proves persisted trace/custom/Eval joins | `test_observe_journey.py:353-474` exchanges the credential, configures `OTLPSpanExporter`, enters `state.run(card="agent")`, emits framework spans plus one custom row and Eval, then queries Bifrost after explicit barriers | `test_scoped_run_emits_drift_eval_and_generic_rows` asserts exact Run/Card correlation, authenticated publisher, server-resolved Card UID, custom join by `run_id`, and Eval join by exact trace/span IDs | PASS |
| Context exit is restoration only, not a telemetry/Bifrost durability barrier | `PyRun::__exit__` only calls `_exit_run`; `_exit_run` only detaches; journey separately calls provider flush, state shutdown, and server publication before queries | Unit test observes post-exit spans as uncorrelated; production-shaped journey orders explicit flush/shutdown/publication after exit | PASS |
| INV-007: tenant, principal, Card UID, and request identity remain server-derived and asserted CardRef stays authorization-bound | Client processor injects only the two approved strings; the candidate does not change server authorization or identity derivation | Journey reads persisted `principal_id` and server-resolved `card_uid` and checks them alongside asserted CardRef; existing signed-scope path is reused | PASS |
| INV-012: reuse existing observation/Bifrost contracts and record semantics | Shared Run correlation and existing `/v1/traces`, Eval, custom-record, Bifrost queue, and query paths are reused; no Vala schema or ingest behavior changes | Existing multi-language observation journeys remain in place and the Python journey extends the existing Service fixture | PASS |
| Non-goals: no server Run resource, second pipeline/queue, wrapper span, mandatory OTel dependency, implicit flush/shutdown, process-global Card scope, or ambient log/metric promise | Diff adds only shared local Run selection, thin language projections, Python execution-local span integration, generated declarations, and tests; the OTLP/HTTP exporter is dev-only | Dependency diff leaves production dependencies unchanged; code and tests contain no extra span, transport, queue, lifecycle action, log, or metric enrichment | PASS |
| Scope control / Ponytail: no speculative abstraction or duplicated durable behavior | Initial selection reuses `WyrdState::card_ref` and `Run`; Python-only OTel behavior stays at the foreign-runtime boundary; TypeScript and Python wrappers remain thin | Complete diff inspection found no unrelated product behavior or compatibility path | PASS |

## Caller-to-result traces

1. `WyrdState::run_for_card` resolves the hydrated alias, constructs one `Run` with a fresh UUIDv7, and `Run::for_card` clones that identity into an independently selected sibling. Python and TypeScript do not resolve aliases separately.
2. `PyRun.__enter__` calls `_enter_run`, which installs the processor, attaches the exact pair to OTel context, and stamps an active recording span. The processor reads the supplied parent context for every new span. `_exit_run` pops and detaches only the current execution context's innermost token.
3. The Python journey exports the resulting spans through authenticated OTLP/HTTP. Gate/Scribe resolve the asserted CardRef to managed identity; Bifrost queries then join the trace rows to the custom row by `run_id` and to the Eval row by exact trace/span identity.

## Regression and negative-flow assessment

- Root-default `run()` behavior remains unchanged in all languages.
- Unknown aliases remain structured local failures; optional telemetry containment does not wrap Card lookup or observation errors.
- Existing explicit observations do not depend on entering a context manager.
- Context exit performs no writer or provider lifecycle operation.
- The cumulative diff contains no unrelated server, schema, auth, storage, or verifier-execution change.

## Verification assessment

The task records successful focused and broader Rust, Python, TypeScript, codegen, boundary, format, lint, and integration lanes. I treated those entries as evidence claims rather than independently established facts. I independently inspected the named tests and their assertions and ran only `git diff --check` on the immutable range; it passed. No overlapping Cargo-backed command was run during this review.

The focused Python and multi-language journey coverage directly exercise the material behavior. The candidate's real OTLP journey is stronger than an in-memory-only proof because it verifies persisted authorization-derived identity and both requested joins.

## Overall result

**PASS**
