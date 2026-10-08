# Concurrency and execution-context domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `d01307b8c37b47488115743c94b624a29d66e4be`
- Scope: the cumulative Python Run OpenTelemetry execution-context boundary,
  including both TASK-009 remediations.
- Candidate immutability: `HEAD` was the candidate at review start and at the
  final check. Review output was written only under `TASK-009-r3/`; reviewed
  source was not modified.
- CodeGraph: no `.codegraph/` index exists, so repository-native source search
  and direct caller inspection were used as instructed.

## Authority coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| One execution-local Run pair, exact attributes, nested restoration, `await`, and task creation | Approved spec revision 45 `REQ-151`; `AC-032`; `architecture/logic/run_api.md:130-206`; `architecture/wyrd-design.md:600-630`; telemetry-observations context-propagation rule | `python/wyrd/otel.py:186-326`; `src/observe/mod.rs:195-241`; focused tests at `test_observe_surface.py:269-351,482-501` | PASS |
| Concurrent first direct entry uses one process-private OTel key | `REQ-151`; `AC-032`; R1 `FIND-TASK-009-4` and its raw-thread non-goal | `_key` double-checks `_scope_key` under the existing `_registered_lock` at `otel.py:201-212`; deterministic two-thread proof at `test_observe_surface.py:504-562` | PASS |
| Provider registration is thread-safe and idempotent per provider | `REQ-151`; `AC-032`; Run API provider contract | weak provider membership and registration are serialized together at `otel.py:246-276`; global/private idempotency proof at `test_observe_surface.py:354-389` | PASS |
| Entry and exit bookkeeping remains isolated across nested scopes and copied task contexts | `REQ-151`; Run API execution-local token requirement; R1 `FIND-TASK-009-3` | immutable tuple stack in a `ContextVar` at `otel.py:189-195`; entry records exact `(token, prior)` at `:279-297`; exit consumes only the innermost entry and restores its prior value at `:300-326`; nested/async/task-copy proof at `test_observe_surface.py:291-351,482-501` | PASS |
| Optional registration, attach, enrichment, and detach failure cannot replace explicit-observation behavior | `REQ-151`; `AC-032`; R1/R2 `FIND-TASK-009-1` | complete try boundaries at `otel.py:222-230,255-276,284-297,307-326`; actual raising global registration, attach, enrichment, and detach paths reach the ordinary Drift boundary at `test_observe_surface.py:408-501` | PASS |
| Raw-thread behavior stays within the approved boundary | Run API `:198-200`; R1 non-goal | the only thread guarantee tested is two threads that each directly enter their own Run scope; no implicit propagation claim or mechanism was added | PASS |

## Boundary trace

`PyRun.__enter__` passes the native view's exact CardRef and shared Run ID to
`wyrd.otel._enter_run`; `PyRun.__exit__` delegates cleanup and always returns
false. Entry first makes a best-effort registration attempt, obtains the one
lazy private OTel key, reads the prior value, attaches the new pair, and records
the exact token plus prior value in the current execution context. The span
processor reads that same key from the span's parent context.

The key creation race identified in R1 is closed at its source: both first-use
threads can observe `None`, but only one can create under `_registered_lock`,
and the second check makes every caller use the winning key. Each directly
entered thread still receives its own OpenTelemetry `ContextVar` context and
its own `_scope_tokens` value, so serialization does not create process-global
Card/Run scope.

Nested and asyncio behavior uses native `contextvars` semantics. The immutable
tuple stack prevents in-place sharing after a task context is copied. A task
created inside a scope inherits the approved pair and may retain it after the
creator exits; later stack updates remain local to each task. On exit, the
owner attempts the exact token detach, then compares the effective value with
the recorded prior value and reattaches the prior value if the OTel wrapper
raised or swallowed reset failure. This restores the outer pair for nesting
and an unenriched value after the outermost scope.

The pinned OpenTelemetry 1.42.1 implementation was also checked: `create_key`
mints a UUID-suffixed distinct key, its default runtime stores current context
in a `ContextVar`, and public `detach` logs and swallows runtime reset errors.
Those mechanics match the race diagnosis and the value-check fallback used by
the candidate.

## Verification

Fresh focused execution passed:

```text
mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py::test_scope_survives_await_and_isolates_concurrent_tasks \
  tests/unit/state/test_observe_surface.py::test_global_and_private_providers_receive_one_processor_each \
  tests/unit/state/test_observe_surface.py::test_detach_failure_restores_the_prior_correlation \
  tests/unit/state/test_observe_surface.py::test_concurrent_first_entries_share_one_scope_key \
  tests/unit/state/test_observe_surface.py::test_registration_and_attach_failures_never_block_observations

5 passed
```

The cumulative task and R1/R2 evidence additionally records the full focused
surface, Python unit/integration lanes, and authenticated persisted journey as
passing. I did not rerun Postgres-backed or broad repository lanes in this
domain pass. No free-threaded-Python qualification, implicit raw-thread context
propagation, or guarantee against an arbitrary provider method that never
returns is required by the approved task.

## Material findings

None.

## Overall result

**PASS** — the candidate closes the concurrent-first-use and failed-detach
defects at the existing Python OTel owner, preserves native execution-context
isolation and approved task-copy behavior, and includes direct proof for the
reachable concurrency and recovery paths required by `REQ-151` and `AC-032`.
