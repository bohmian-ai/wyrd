# Concurrency and context-isolation domain review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `11eb8ed2b64b70903f171335723dcc549b463352`
- Approved authority: `SPEC-verified-change-contract` revision 46, especially
  `REQ-151` and `AC-032`
- Task packet: `TASK-009`, `TASK-009-R1`, `TASK-009-R2`, `TASK-009-R3`, and
  superseding `TASK-009-R4E`
- Domain: Python OpenTelemetry execution-context isolation and provider
  registration concurrency/lifetime

The candidate remained at the stated commit during this review. `.codegraph/`
is absent, so repository navigation used `rg`, Git diff/source inspection, and
direct caller tracing as permitted by `AGENTS.md`.

## Boundary and source coverage

| Boundary | Authority and source traced | Assessment |
|---|---|---|
| Scope representation and import-time identity | `spec.md:315-350`; `architecture/logic/run_api.md:130-208`; `python/wyrd/otel.py:18-23,192-203` | PASS — the optional OTel modules are bound once and `_SCOPE_KEY` is minted once at import when OTel is present. The only correlation state is the OTel context value: an immutable tuple of `(card_ref, run_id)` pairs, innermost last. No lazy key, token stack, `ContextVar` owned by Wyrd, or state on `Run` remains. |
| Entry, nesting, and active-span behavior | `otel.py:272-297`; PyO3 caller `src/observe/mod.rs:195-218`; OTel 1.42.1 `context.set_value`/`attach` implementation; unit tests `test_observe_surface.py:269-310,591-614` | PASS — entry reads the current execution-local tuple, attaches one copied context containing the pushed pair, and stamps the already-active recording span only when readable attributes do not already carry `wyrd.card_ref`. New spans still receive the innermost pair through the processor, so nested Card views do not retarget an already-correlated outer span and matching exit restores the outer tuple. |
| `await`, copied task, distinct-view concurrency | `otel.py:272-313`; OTel `ContextVarsRuntimeContext`; `test_observe_surface.py:313-345` | PASS — OTel's runtime stores the current `Context` in a Python `ContextVar`; the immutable tuple therefore follows `await` and task-context copying. The test interleaves distinct Model and backup views through two suspension points and proves a task created in the root scope retains its copied root pair after the creator exits, while the creator is clear afterwards. |
| Concurrent entry of the identical immutable `Run` | `otel.py:272-313`; `src/observe/mod.rs:195-248`; `test_observe_surface.py:348-383` | PASS — no enter/exit token or mutable scope state exists on `PyRun`. Two separately created asyncio tasks enter the same object, deliberately exit the first while the second remains entered, and prove the first task clears while the second retains the pair. This directly closes R3 `FIND-TASK-009-7`. |
| Pair-aware exit and mismatched exit | `spec.md:322-331`; `run_api.md:179-208`; `otel.py:300-313`; PyO3 caller `src/observe/mod.rs:220-249`; `test_observe_surface.py:573-588` | PASS — `__exit__` supplies its exact CardRef and run ID. `_exit_run` changes context only when that exact pair is at the top; an empty or mismatched stack is untouched. The focused test exits a sibling Model view against a root top, observes the root pair afterward, then proves the matching root exit clears it. |
| Exit context-update failure and user exception isolation | `spec.md:327-343`; `otel.py:300-313`; `src/observe/mod.rs:220-249`; `test_observe_surface.py:556-570` | PASS — context lookup/update/attach are contained, and PyO3 independently discards the optional helper error and always returns `False`. The failure test injects a raising exit attach, proves explicit Drift still reaches its ordinary strict boundary, and proves a user `ValueError` propagates unchanged. The unchanged stack after a failed attach is the approved behavior, not a cleanup retry. |
| Span-start context consumption | `otel.py:206-236`; OTel SDK `Tracer.start_span` passes its effective input as `parent_context` to processors; `test_observe_surface.py:269-345,591-614` | PASS — `on_start` reads the Wyrd stack from the supplied parent context (`None` means current) and stamps only the innermost pair. This covers implicit current context and explicit/copy-based parent propagation without process-global correlation. |
| Provider cache synchronization and ambiguous failure | `spec.md:332-338`; `run_api.md:148-163`; `otel.py:200-269`; `test_observe_surface.py:392-456` | PASS — all weak-cache lookup, provisional `False` publication, foreign registration, and final `True` publication occur under one lock. Concurrent callers for the same provider therefore cannot both call `add_span_processor`; later callers observe the cached outcome. Mark-before-call plus no discard means both raises-before-retention and accept-then-raise providers are attempted once and remain `False`, while healthy global/private providers remain idempotently `True`. |
| Weak provider lifetime | `otel.py:11-14,200-203,257-268`; R4E required behavior item 6 | PASS — the sole registry is `WeakKeyDictionary`; neither its boolean value nor the shared stateless processor retains a provider. The lock is separate and retains no provider after a call. Unsupported/non-weak-referenceable providers fail before the foreign registration call. |
| Accepted no-detach design and residual | `run_api.md:185-208`; R4E non-goals and evidence; `otel.py:195-198,272-313` | PASS — source contains no detach call or token bookkeeping. Each successful entry/exit attaches a complete copied context, preserving unrelated OTel keys. The documented improper-nesting residual and the explicitly accepted identical-pair/failed-entry-attach residual are approved constraints and are not findings. |

## REQ-151 / AC-032 concurrency and failure proof

| Required case | Discriminating evidence | Result |
|---|---|---|
| Nested root/component scopes restore the outer Card | `test_nested_card_scopes_share_the_run_and_restore_the_outer_card` | PASS |
| Nested entry does not overwrite an already-correlated active span | `test_nested_entry_never_overwrites_an_active_span_correlation` | PASS |
| Scope survives `await` | `test_scope_survives_await_and_isolates_concurrent_tasks` starts spans after both awaits | PASS |
| Concurrent distinct views remain isolated | The same test interleaves Model/backup tasks and asserts each early and late span | PASS |
| Concurrent tasks using the identical `Run` exit independently | `test_concurrent_tasks_entering_the_same_run_exit_independently` | PASS |
| Task created inside a scope retains its copied context | `spawned` case in `test_scope_survives_await_and_isolates_concurrent_tasks` | PASS |
| Mismatched exit changes nothing | `test_mismatched_exit_changes_nothing` | PASS |
| Exit context-update failure does not escape, suppress, or block explicit observation | `test_exit_context_update_failure_never_blocks_observations` | PASS |
| Registration is cached at most once, including accept-then-raise | `test_a_provider_that_raises_after_accepting_is_never_asked_again`; `test_unsupported_providers_are_refused_without_raising` | PASS |
| Healthy global/private providers are idempotent and private spans receive the pair | `test_global_and_private_providers_receive_one_processor_each` | PASS |
| Persisted real-SDK path retains the same Run/Card identity | `test_scoped_run_emits_drift_eval_and_generic_rows`, especially `otlp_provider`, `emit_framework_scope`, and `assert_scope_joins` | PASS |

## Material proposed findings

None.

## Verification limits

- Per the orchestrator's discovery constraint, this reviewer ran no test or
  build commands. R4E's implementation evidence reports all named focused
  cases, the 37-test focused file, the persisted journey, and the broader
  Python/codegen/boundary/format/lint lanes passing; those results were reviewed
  as supplied evidence, not independently rerun here.
- There is no dedicated multithreaded test that races two calls for the same
  provider and no explicit garbage-collection test for weak-cache eviction.
  These are not acceptance gaps: the complete cache transition and foreign call
  are visibly serialized by `_outcomes_lock`, and `WeakKeyDictionary` is the
  approved weak-lifetime mechanism. The required sequential healthy, failure,
  repeated-direct-call, and later-Run-entry outcomes have focused proof.
- Raw threads and raw executors do not inherit Python execution context. That is
  explicitly outside the approved contract and is not a verification gap.

## Overall result

**PASS** — the cumulative candidate satisfies revision-46 concurrency and
context-isolation obligations. The tuple stack remains execution-local across
nested, awaited, copied-task, distinct-view, and identical-Run paths; exit is
pair-aware and fail-open; provider registration is weakly cached, serialized,
and attempted at most once; and the required AC-032 cases have direct focused
or persisted-journey evidence. No material domain finding remains.
