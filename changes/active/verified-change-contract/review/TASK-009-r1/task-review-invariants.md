# TASK-009 Invariant Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `e4f30547906b8046bdccbfd38dff0eb0ee475d06`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Candidate was still `HEAD` when this report was completed.

## Invariant trace

The shared producer is `WyrdState::run` / `run_for_card`, which resolves a
hydrated `CardRef` before `Run::new` mints one `RunId`. `Run::for_card` clones
that ID and selects another immutable subject; all explicit observations derive
their `Correlation` from the selected view. Python and TypeScript only project
that owner.

Python context entry passes the same native `CardRef` and `RunId` to
`wyrd.otel._enter_run`. An OpenTelemetry `ContextVar` carries the pair through
normal context propagation, while a per-execution token stack pairs nested
entries with exits. The span processor reads the parent context at `on_start`;
the active span is stamped directly on entry. `PyObserveHandle::eval` separately
reads valid active Python span IDs and hands them through the existing explicit
Eval options path. No ambient state is consulted by explicit Drift, Eval, or
record correlation.

The journey installs the processor on a private stock provider, exports via the
authenticated `/v1/traces` route, explicitly flushes that provider, drains the
state-owned Bifrost writer, waits for publication, and queries persisted trace,
custom, and Eval rows. Gate/Scribe continue to resolve the asserted CardRef to
the signed-scope Card UID and stamp publisher identity.

## Obligation matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-123: root runs remain the default; an initial alias selects an exact hydrated Card; sibling views keep one invocation identity | `crates/shared/wyrd-client/src/state.rs:489-511`; `crates/shared/wyrd-client/src/observe/mod.rs:42-91` | Shared focused tests at `observe/tests.rs:625-657`; Python surface tests at `test_observe_surface.py:225-243` | PASS |
| REQ-123: Rust, Python, and TypeScript project the approved initial-Card forms without duplicating lookup | Shared `WyrdState::run_for_card`; Python delegates at `sdks/wyrd-sdk-python/src/state/mod.rs:180-194`; N-API delegates at `sdks/wyrd-sdk-ts/native/src/cards.rs:298-313`; TypeScript wrapper delegates at `wyrd/src/index.ts:1852-1866` | Rust SDK and TypeScript integration additions exercise root, initial alias, same-ID sibling, and unknown alias; generated Python/TypeScript declarations match the public signatures | PASS |
| Unknown aliases fail locally before a run ID or network activity | `WyrdState::run_for_card` resolves `card_ref(alias)?` before `Run::new` at `state.rs:508-510` | Shared, Python, and TypeScript unknown-alias cases | PASS |
| REQ-151: Python `Run` is a synchronous context manager that returns itself and never suppresses user exceptions | `sdks/wyrd-sdk-python/src/observe/mod.rs:195-237` | `test_entering_a_run_returns_it_and_correlates_active_and_child_spans` and the user-exception assertion at `test_observe_surface.py:426-428` | PASS |
| Active recording spans and spans started inside a scope receive exactly `wyrd.card_ref` and `wyrd.run_id` | `_enter_run` at `python/wyrd/otel.py:274-291`; `_RunCorrelationProcessor.on_start` at `otel.py:217-226` | Active, child, grandchild, conflicting-initial-attribute, private-provider, and persisted journey assertions | PASS |
| Nested scopes share one Run ID, select their own Card, restore the outer context, survive `await`, isolate concurrent tasks, and propagate to a task created in scope | Execution-local OTel context plus `_scope_tokens: ContextVar` at `otel.py:189-194,274-307`; no token is stored on `Run` | `test_nested_card_scopes_share_the_run_and_restore_the_outer_card` and `test_scope_survives_await_and_isolates_concurrent_tasks` | PASS |
| Global and explicitly supplied providers receive at most one processor, and unsupported providers fail open | Locked weak-provider set and guarded registration at `otel.py:195-197,241-271` | Idempotency on global/private providers and API-only/failing provider cases at `test_observe_surface.py:352-387` | PASS |
| REQ-151 / AC-032: missing OTel, registration failure, attach failure, span-enrichment failure, and detach failure do not escape **or block explicit observations** | Each optional boundary is caught in `otel.py:250-271,279-290,302-307`; explicit observations remain independent Rust-owned calls | Missing-package case reaches explicit Drift, but registration/enrichment/detach cases do not exercise an explicit observation, and no attach-failure case exists (`test_observe_surface.py:374-424`) | FAIL (`INV-REV-001`) |
| Explicit Eval correlation prefers explicit IDs and otherwise captures valid active Python span IDs | `active_span_ids` at `sdks/wyrd-sdk-python/src/observe/mod.rs:125-153` and Eval option construction in the same module | Unit active/explicit-ID cases and persisted exact Eval-to-span join in the journey | PASS |
| AC-032: real stock OTLP/HTTP export and persisted trace/custom/Eval identity join | Provider/export at `test_observe_journey.py:365-402`; persisted assertions at `test_observe_journey.py:413-474`; lifecycle ordering at `test_observe_journey.py:550-563` | Task evidence records the exact journey command passing; assertions use Bifrost query results, not SDK objects or an in-memory exporter | PASS |
| INV-007: tenant, principal, Card UID, and request identity remain authenticated/server-derived | Client injects only the two constants at `otel.py:186-187,221-224,284-288`; journey uses a registered Service credential and asserts non-null publisher plus resolved component UID | Persisted trace/custom/Eval assertions at `test_observe_journey.py:432-474` | PASS |
| INV-012: reuse existing observation, Eval, Bifrost, and ingest contracts | Shared `Run::correlation` continues through existing `Observe`/writer paths; only the Python foreign-runtime layer owns OTel context | Existing scoped-observation journey is extended in place; no Vala contract or server route changed in the cumulative diff | PASS |
| Context exit is not a flush, shutdown, span lifecycle operation, network call, or durability acknowledgement | `PyRun::__exit__` only delegates `_exit_run`; `_exit_run` only detaches a token | Journey explicitly calls provider flush, `state.shutdown`, and `server.flush_bifrost` after scope exit | PASS |
| Non-goals: no server Run resource, second telemetry pipeline, wrapper span, process-global Card scope, mandatory production OTel dependency, or ambient log/metric promise | Diff adds only a dev OTLP/HTTP exporter; correlation uses caller providers and `ContextVar`; no server/Vala production source changed | Manifest diff and complete base-to-candidate write set | PASS |

## Proposed findings

### INV-REV-001 — MISSING — required fail-open proof does not exercise explicit observations

- **Violated obligation:** REQ-151 and AC-032 require focused Python evidence
  that registration, attach, span-enrichment, and detach failures do not escape
  or block explicit Wyrd observations.
- **Exact location:**
  `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:374-424`.
- **Evidence:** `test_unsupported_providers_are_refused_without_raising` calls
  only `install_run_correlation`; `test_enrichment_and_detach_failures_never_escape`
  creates an uncorrelated span and injects detach failure but never calls
  `run.observe.*`; only the missing-package test reaches explicit Drift. No test
  injects `opentelemetry.context.attach` failure, and the active-span stamping
  failure path is not exercised.
- **Observable consequence:** a future coupling that makes explicit Drift,
  Eval, or record emission fail after registration, attach, enrichment, or
  detach failure would still leave every new focused test green. The candidate
  therefore does not supply the proof class explicitly required by AC-032,
  even though source inspection shows the current explicit observation path is
  independent.
- **Required testable correction:** extend the existing focused failure tests,
  without a new harness, so each required failure is driven through `with run`
  and an explicit `run.observe.*` call reaches its normal boundary result
  (for this server-free fixture, `WYRD_SDK_400_BIFROST_NOT_STARTED`) rather than
  a telemetry exception. Add the missing `context.attach` failure and a direct
  active-span enrichment failure. Preserve the user exception and no-escape
  assertions.

## Verification assessment

The task records successful focused Rust and Python commands, the exact Python
OTLP journey, first-class SDK lanes, typechecks, codegen, boundary checks,
formatting, and lints. This reviewer inspected the immutable cumulative diff,
owning source, sibling consumers, generated declarations, and tests. No
overlapping Cargo-backed command was run. One read-only attempt to invoke the
repository Python toolchain failed before execution because `mise exec --
python` had no configured binary; it provides no verification evidence and
does not alter the finding above.

## Overall result

**FAIL**

The producer-to-sink identity and lifecycle invariants are implemented
coherently, but the approved acceptance criterion makes the missing focused
fail-open/explicit-observation proof a material completion gap.
