---
id: TASK-009-R1
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 35
requirements: [REQ-123, REQ-151, INV-007, INV-012, AC-032]
depends_on: [TASK-009]
parent_task: TASK-009
remediates:
  - FIND-TASK-009-1
  - FIND-TASK-009-2
  - FIND-TASK-009-3
  - FIND-TASK-009-4
---

# Restore Python Run correlation under failure and concurrent first use

## Authority and immutable inputs

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Review base: `7d96c30066425e0cde2290842d5801307843283d`
- Reviewed candidate: `e4f30547906b8046bdccbfd38dff0eb0ee475d06`
- Validated ledger: `changes/active/verified-change-contract/review/TASK-009-r1/findings-validation.md`

## Issue diagnosis

### `FIND-TASK-009-1` — incomplete fail-open proof

AC-032 and TASK-009 Scenario 3 require focused evidence that optional
registration, enrichment, and detach failures do not escape or block explicit
observations; REQ-151 also makes attach failure fail-open. The missing-package
test reaches the ordinary Drift boundary, but the API-only/registration,
processor-enrichment, and detach tests never emit an explicit observation, and
no test injects attach failure. Source separation is plausible but does not
satisfy the approved proof class. A future coupling could therefore break
explicit observations while the focused suite remains green.

### `FIND-TASK-009-2` — false Run subject invariant

`WyrdState::run_for_card` now resolves a non-root Card before constructing the
Run, but `Run::subject` still says the subject is the root Service until
`for_card`. The owner type therefore contradicts the new initial-selection
path and can mislead maintenance of every language projection.

### `FIND-TASK-009-3` — detach failure leaks stale correlation

`_exit_run` removes its local token bookkeeping before attempting OTel detach
and does not restore or neutralize the Wyrd context value when reset fails or
is swallowed by OTel's public wrapper. The existing failure test confines the
leak to a copied context instead of proving recovery. Later spans in a real
same-context caller can consequently carry a stale Card/Run pair, producing
misattributed evidence or a signed-scope rejection. Nested failure must restore
the outer pair rather than merely clear all correlation.

### `FIND-TASK-009-4` — concurrent first-use key race

`_scope_key` uses an unlocked lazy check/create while OTel `create_key` returns
a distinct UUID-backed key per call. Two threads directly entering Run scopes
for the first time can attach under different keys; the last global assignment
is the only key the processor later reads. One valid in-scope span can then be
persisted without Card or Run correlation and cannot join the custom/Eval
evidence required by the task.

## Intended correction outcome

Every direct Run entry uses one process-private OTel key; every exit restores
the correlation that preceded that entry even when detach/reset fails; optional
telemetry failures remain unable to block explicit observations; and the shared
Run owner documents its actual subject invariant.

## Decision-complete recommendation

Keep all runtime correction inside the existing `wyrd.otel` scope owner.

1. Serialize lazy `_scope_key` creation with the module's existing lock and a
   second `None` check inside the lock. Keep creation lazy so importing or using
   Runs without OpenTelemetry remains supported. Do not add another lock,
   dependency, eager OTel import, or key abstraction.
2. Extend each successful scope entry's existing execution-local bookkeeping
   to retain the prior Wyrd correlation along with the exact attach token.
   During exit, attempt the exact-token detach, then ensure the Wyrd key reflects
   the recorded prior value even when OTel's public detach raises or internally
   swallows reset failure. The outer pair must be restored for nesting and no
   pair must remain after the outermost scope. Contain fallback failures and
   preserve user exceptions. Do not add retries, warnings, a parallel context
   system, exporter/provider lifecycle ownership, or implicit flush/shutdown.
3. Reuse the existing server-free Scenario 3 test home. Pair API-only or
   registration failure, attach failure, the existing processor-enrichment
   failure, and detach failure with one representative explicit Drift call and
   assert its ordinary `WYRD_SDK_400_BIFROST_NOT_STARTED` result. The detach
   recovery cases also prove same-context post-exit correlation and nested
   outer restoration. Do not multiply cases across all observation methods or
   every enrichment call site.
4. Change only the `Run::subject` field rustdoc so it accurately describes the
   exact Card for a root run, initially selected run, or immutable sibling view.

These corrections use the existing owners and native mechanisms: the current
module lock, OTel context key/token behavior, execution-local token stack,
focused Python test file, and Run terminology. The two runtime defects remain
separate invariants even though their tests share a file.

## Constraints and preserved behavior

- Preserve the approved public Rust, Python, and TypeScript APIs.
- Preserve one UUIDv7 `run_id` across immutable Card views and local alias
  failure before opening a Run.
- Preserve optional, fail-open Python OTel integration and strict Card lookup,
  validation, authorization, and explicit observation errors.
- Preserve normal nesting, asyncio propagation/isolation, task-context copying,
  active-span stamping, provider idempotency, and private-provider installation.
- Preserve authenticated OTLP ingest and server-derived tenant, publisher, and
  Card UID.
- Preserve explicit provider flush and `WyrdState.shutdown()` as separate
  lifecycle barriers; Run exit remains correlation cleanup only.

## Non-goals

- No server Run resource, Vala/Gate/Scribe contract change, new correlation
  field, second exporter or queue, wrapper span, log/metric enrichment, or
  client-authored managed identity.
- No raw-thread context propagation guarantee; the concurrent test covers
  separate threads that each directly enter a Run scope.
- No new test harness, dependency, provider wrapper, warning/retry mechanism,
  or refactor of unrelated `OtelObserver` behavior.
- No per-observation-method failure matrix or per-enrichment-call-site test.

## Acceptance criteria

1. `FIND-TASK-009-4`: two deterministically overlapped first entries in
   separate threads use one private key, and spans from both scopes carry their
   own exact CardRef and Run ID.
2. `FIND-TASK-009-3`: after injected detach/reset failure and same-context
   exit, a newly created span has no Wyrd correlation; after an inner failure,
   the next span carries the outer scope's exact CardRef and shared Run ID.
3. `FIND-TASK-009-1`: API-only/registration, attach, processor-enrichment, and
   detach failures each permit a representative explicit Drift call to reach
   `WYRD_SDK_400_BIFROST_NOT_STARTED`; the existing user exception propagates
   unchanged and unknown aliases remain strict.
4. `FIND-TASK-009-2`: `Run::subject` rustdoc accurately covers root, initially
   selected, and sibling-selected views without changing runtime code.
5. Existing healthy-path active/child, nested, asyncio, private/global provider,
   missing-package, and authenticated persisted-join behavior remains green.

## Focused proof and broader verification

Run the existing focused Python surface file after each correlation correction:

```bash
mise run py:setup
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py)
```

Then run the original persisted journey to ensure recovery changes do not alter
the healthy authenticated path:

```bash
mise run py:setup:testing
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/state/test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows'
```

Run the touched-surface gates:

```bash
mise run test:shared
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run codegen:check
mise run check:pyo3-scope
mise run fmt
mise run py:format
mise run lints
mise run py:lints
git diff --check
```

Route this remediation directly to `$wyrd-implement` and review the complete
original base-to-remediated-candidate range afterward.
