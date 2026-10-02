---
id: TASK-009-R4E
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 46
requirements: [REQ-151, AC-032]
depends_on: [TASK-009, TASK-009-R1, TASK-009-R2, TASK-009-R3]
parent_task: TASK-009
remediates: [FIND-TASK-009-8]
supersedes: [TASK-009-R4]
---

# Token-free Python Run correlation

Route directly to `$wyrd-implement`. This task replaces
`TASK-009-R4-stabilize-provider-registration-idempotency.md` after a
human-approved design revision (spec revision 46).

## Authority and immutable inputs

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 46
  (REQ-151, AC-032). Revision 46 is the authority for every behavior below.
- Architecture: `changes/active/verified-change-contract/architecture/logic/run_api.md`,
  section "Python OpenTelemetry run scope".
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`.
- Prior remediations: TASK-009-R1, TASK-009-R2, TASK-009-R3 in their review
  directories. The R3 case-to-test table is carried forward below.
- Round-four review: `changes/active/verified-change-contract/review/TASK-009-r4/`
  (`verdict.md`, `findings-validation.md`). Review base
  `7d96c30066425e0cde2290842d5801307843283d`; reviewed candidate
  `017a54d5390eb488e820f890bbb953a5f1ca3a53`.

## Issue diagnosis

### `FIND-TASK-009-8` — ambiguous registration failure installs duplicate processors

`install_run_correlation` in `sdks/wyrd-sdk-python/python/wyrd/otel.py` marks
a provider, calls its duck-typed `add_span_processor`, and discards the mark
when the call raises. `_enter_run` retries on every later Run entry. A provider
that retains the processor and then raises therefore receives a second
processor on the next attempt (reproduced: two calls both returned `False`
while the retained count grew from one to two).

### Root cause

The correlation value is stored in the right place (the OpenTelemetry
context). The defects across four review rounds came from the bookkeeping
around it: a lazily created context key guarded by double-checked locking
(r1 `FIND-TASK-009-4`), a separate `ContextVar` token stack whose `detach` can
fail plus a compare-and-reattach fallback (r1 `FIND-TASK-009-3`), and a
registry that forgets a provider after a failed attempt (r4). Revision 46
removes that bookkeeping instead of patching each layer.

Empirical facts behind the design (OpenTelemetry 1.42.1): `context.detach`
and `ContextVar.reset` fail in the same cases (token from another context, or
reused); `attach` / `ContextVar.set` do not. The OpenTelemetry context value
propagates across `await`, `create_task`, `to_thread`, `copy_context().run`,
OTel-only propagation (`attach(captured)` in a worker), and
`start_span(context=captured)`; the SDK passes that explicit context as
`parent_context` to `on_start`. Raw threads and raw executors carry nothing
under any design and remain uncovered, as already documented.

## Required behavior (spec revision 46)

1. One private OpenTelemetry context key is created at `wyrd.otel` import when
   OpenTelemetry is importable. Its value is a tuple of `(card_ref, run_id)`
   pairs, innermost last. When OpenTelemetry is absent at import, run
   correlation is a no-op and `install_run_correlation` returns `False`; the
   module must still import without OpenTelemetry.
2. Entry (`_enter_run(card_ref, run_id)`) ensures registration on the global
   provider, then pushes its pair by one `attach(set_value(...))`. Nothing is
   recorded outside the context value.
3. Exit (`_exit_run(card_ref, run_id)`) reads the stack from the current
   context and, only when the top equals its own pair, pops it by one
   `attach(set_value(...))`. A mismatched top, empty stack, or failing context
   call changes nothing. Exit never calls `detach`, never raises, and never
   masks the block's exception.
4. The span processor's `on_start` reads the stack from `parent_context` (None
   means current) and stamps the innermost pair; conflicting initial values on
   a newly started span are replaced, as today.
5. On entry, the already-active recording span is stamped with
   `wyrd.card_ref` / `wyrd.run_id` only when it does not already carry
   `wyrd.card_ref`. Use the span's readable attributes when the span exposes
   them; a span whose attributes cannot be read is stamped as today.
6. Registration is idempotent and attempted at most once per provider. Mark
   the provider before the foreign call and never discard the mark; cache the
   outcome (`True` only after `add_span_processor` returns normally). Later
   calls return the cached outcome without calling the provider again. A
   provider that cannot be weakly referenced, lacks `add_span_processor`, or
   raises (before or after retaining the processor) yields `False` and no
   further attempt. Weak provider lifetime and thread safety are preserved.
   One shared stateless processor instance is acceptable. State the
   never-retried behavior in the `install_run_correlation` docstring.
7. The PyO3 `Run.__exit__` in `sdks/wyrd-sdk-python/src/observe/mod.rs` passes
   the view's `card_ref` and `run_id` to `_exit_run`, mirroring `__enter__`.
   Update the rustdoc of both `__enter__` and `__exit__` so it no longer
   mentions tokens or detach (AGENTS.md §16). `__exit__` keeps its signature,
   defaults, and `False` return.

Delete what the design makes dead: `_key()` and its lock, `_scope_key`, the
`_scope_tokens` `ContextVar`, every `detach` call, the compare-and-reattach
fallback, the discard-on-failure path, the separate `_registered` set and
lock, and any imports left unused. Attach without detach is a documented,
deliberate decision in `run_api.md`; do not add detach back.

## Constraints and preserved behavior

- Shared Rust keeps ownership of Run identity and hydrated Card selection.
- Exact attribute names `wyrd.card_ref` and `wyrd.run_id`; the client injects
  nothing else. Authenticated ingest and server-derived identity unchanged.
- OpenTelemetry stays optional; the `otel` extra stays optional; the processor
  stays duck-typed (no SDK subclass).
- `install_run_correlation(provider=None) -> bool` signature, never-raises
  contract, and public stubs unchanged. `_enter_run` / `_exit_run` remain
  private.
- Context exit is not a flush, shutdown, or durability barrier.
- Strict Card lookup, authorization, validation, explicit observation errors,
  and user exceptions keep their behavior.

## Non-goals

- No baggage, resource attributes, plain `ContextVar` scope store, provider
  wrapper, pipeline introspection, retry, warning, timeout, new dependency,
  second registry, exporter, or queue.
- No server Run, Gate/Scribe/Bifrost change, wrapper span, process-global Card
  scope, or log/metric enrichment.
- No change to shared Rust Run behavior, Rust/TypeScript projections, or
  persisted schemas. No new journey.
- Do not handle the residual where entry's attach fails while an outer scope
  holds the identical pair (exit would then pop the outer entry); attach is
  not expected to fail and this is accepted fail-open behavior.

## Test changes

All in `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py`.

- Delete `test_concurrent_first_entries_share_one_scope_key` and its
  now-unused support (`_ContentionLock`, thread helpers): the key is minted at
  import.
- Replace `test_detach_failure_restores_the_prior_correlation` with two tests
  and remove `ORIGINAL_DETACH`:
  - exit context-update failure: with `otel_context.attach` raising only
    during exit, exit does not raise, a user exception still propagates
    unchanged, and a representative explicit Drift call after the failed exit
    still reaches `WYRD_SDK_400_BIFROST_NOT_STARTED`;
  - mismatched exit does nothing: exiting a view whose pair is not the top of
    the stack (for example `run.__enter__()` then `model.__exit__()`) leaves
    spans started afterwards carrying the outer pair; the matching exit then
    clears it.
- Extend `test_global_and_private_providers_receive_one_processor_each` (or
  the provider-registration test beside it) with one weak-referenceable
  provider that retains the supplied processor and then raises: repeated
  direct `install_run_correlation` calls and a later Run entry whose global
  lookup returns that exact object all return/yield `False`, call
  `add_span_processor` exactly once, and leave exactly one retained processor.
  Keep a raises-before-retention provider attempted once per provider object.
  `test_registration_and_attach_failures_never_block_observations` keeps its
  per-instance `Raising` provider (a new instance per entry, so two attempts
  remain correct).
- `test_missing_opentelemetry_is_a_no_op` must simulate absence at import
  time, because blocking `sys.modules` after import no longer affects the
  module-level imports. Patch the module's import-time OpenTelemetry binding
  (or reload in isolation without leaking a second module object into other
  tests) and keep the assertions: `install_run_correlation()` is `False`, the
  Run enters and exits, and explicit Drift reaches its ordinary error.
- Add a nested-scope-does-not-overwrite test: with an outer span active,
  entering the root Run stamps it with the root pair; entering a nested Card
  view while that span (or a processor-stamped child) is current does not
  overwrite its `wyrd.card_ref`; spans started inside the nested scope carry
  the nested pair; after exit the outer pair is restored for new spans.
- Keep `test_concurrent_tasks_entering_the_same_run_exit_independently` as the
  regression guard for r3 `FIND-TASK-009-7`.
- `test_entering_a_run_returns_it_and_correlates_active_and_child_spans` keeps
  its `grandchild` case: the processor still replaces a conflicting initial
  `wyrd.card_ref` on a span started inside the scope.

### AC-032 case-to-test map (carry forward, update in evidence)

Record the final table in implementation evidence. Every row must map to a
test that would fail on a regression of its case.

| Case | Test |
|---|---|
| Sync entry returns Run; active recording span and child spans stamped; conflicting initial attrs on in-scope spans replaced; exit clears | `test_entering_a_run_returns_it_and_correlates_active_and_child_spans` |
| Nested root/component scopes share run ID, own CardRefs, restore outer | `test_nested_card_scopes_share_the_run_and_restore_the_outer_card` |
| Nested entry does not overwrite an active span's existing `wyrd.card_ref` | new nested-scope-does-not-overwrite test |
| `await` | `test_scope_survives_await_and_isolates_concurrent_tasks` |
| Concurrent tasks, distinct Run views | `test_scope_survives_await_and_isolates_concurrent_tasks` |
| Concurrent tasks, identical Run object | `test_concurrent_tasks_entering_the_same_run_exit_independently` |
| Task created inside the scope | `test_scope_survives_await_and_isolates_concurrent_tasks` |
| Missing `opentelemetry-api` + explicit observation | `test_missing_opentelemetry_is_a_no_op` (import-time absence) |
| API-only/no-SDK provider + explicit observation | `test_registration_and_attach_failures_never_block_observations`; `test_unsupported_providers_are_refused_without_raising` |
| Processor registration failure + explicit observation | `test_registration_and_attach_failures_never_block_observations` |
| Registration attempted at most once per provider, incl. accept-then-raise | extended provider-registration test |
| Entry attach failure + explicit observation | `test_registration_and_attach_failures_never_block_observations` |
| Span enrichment failure + explicit observation | `test_enrichment_failure_never_blocks_observations` |
| Exit context-update failure + explicit observation | new exit context-update failure test |
| Mismatched exit changes nothing | new mismatched-exit test |
| User exception propagates unchanged | `test_registration_and_attach_failures_never_block_observations`, `test_enrichment_failure_never_blocks_observations`, exit context-update failure test |
| Unknown alias fails before entry | `test_unknown_alias_is_refused`, `test_run_card_refuses_an_unknown_alias` |
| Provider idempotency (global + private) and private-provider attributes | `test_global_and_private_providers_receive_one_processor_each` |
| `__exit__` never suppresses; stub parity | `test_run_exit_accepts_conventional_keywords_and_omitted_arguments` |
| Real SDK→server OTLP export, custom-row and Eval joins, active-span ids, exit not a barrier | `tests/integration/state/test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows` (unchanged) |

## Acceptance criteria

1. `FIND-TASK-009-8`: a provider that retains the processor and then raises
   receives exactly one processor across repeated direct installs and a later
   Run entry; every call reports `False` without calling the provider again.
2. Healthy global and private providers return `True` idempotently, hold one
   processor, and enrich active and child spans with the exact pair.
3. `otel.py` contains no `detach` call, no token storage, no lazily created
   key, and no discard-on-failure path; `_exit_run` receives the view's pair
   from the PyO3 boundary.
4. Nested entry leaves an already-correlated active span untouched; in-scope
   spans carry the innermost pair.
5. Every AC-032 row above maps to a passing test; the persisted journey passes
   unchanged.

## Verification

Name every new or renamed test in evidence and run each exactly, then the
focused file (`mise run py:setup:testing` does not exist; use `mise run
py:setup`):

```bash
mise run py:setup
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py::<exact_test_name>)
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py)
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/state/test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows'
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run codegen:check
mise run check:pyo3-scope
mise run fmt
mise run lints
mise run py:format
mise run py:lints
git diff --check
```

On any failure, re-run with `WYRD_LOG=info` (AGENTS.md §11) before diagnosing.

## Stop conditions

Stop and report to the caller, without improvising, if:

- correct behavior requires calling `detach`, storing a token or per-scope
  state outside the OpenTelemetry context value, or storing state on `Run`;
- the OpenTelemetry context value does not propagate on a path the existing
  tests or journey rely on;
- a provider-registration outcome cannot be cached per provider without
  retaining providers strongly or changing the public
  `install_run_correlation` contract;
- satisfying item 5 requires reading private SDK internals beyond the span's
  readable attributes;
- the persisted journey or any AC-032 case needs a Gate, Scribe, Bifrost,
  shared Rust, or stub/contract change; or
- any requirement here conflicts with spec revision 46.

A later task review must reassess the complete base-to-remediated-candidate
range, not only this correction.
