---
id: TASK-009-R4
kind: remediation
status: superseded
spec: SPEC-verified-change-contract
spec_revision: 45
requirements: [REQ-151, AC-032]
depends_on: [TASK-009, TASK-009-R1, TASK-009-R2, TASK-009-R3]
parent_task: TASK-009
remediates: [FIND-TASK-009-8]
---

Superseded by TASK-009-R4E-token-free-run-correlation.md (human-approved design revision, spec rev 46)

# Stabilize provider registration idempotency

## Authority and immutable inputs

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 45.
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`.
- Prior remediations: TASK-009-R1, TASK-009-R2, and TASK-009-R3 in their respective review directories.
- Review base: `7d96c30066425e0cde2290842d5801307843283d`.
- Reviewed candidate: `017a54d5390eb488e820f890bbb953a5f1ca3a53`.
- Validated ledger: `changes/active/verified-change-contract/review/TASK-009-r4/findings-validation.md`.

## Issue diagnosis

### `FIND-TASK-009-8` — ambiguous registration failure can install duplicate processors

REQ-151 and AC-032 require thread-safe, idempotent registration. TASK-009
Scenario 3 makes the user-visible bound explicit: each global or explicitly
supplied private provider receives at most one Wyrd processor, while optional
registration failure remains fail-open.

At `sdks/wyrd-sdk-python/python/wyrd/otel.py:246-276`,
`install_run_correlation` records a provider, calls its duck-typed
`add_span_processor`, and removes the mark when that foreign call raises.
`_enter_run` at `:279-297` retries the same owner on later Run entries. An
exception cannot prove that the foreign side effect did not happen. A supported
framework wrapper can delegate to a stock provider, retain the supplied Wyrd
processor, then fail in its own post-registration hook. The next explicit call
or Run entry hands it another processor.

The defect was reproduced without source changes: two installation calls on
one retaining-then-raising provider both returned `False`, while its retained
Wyrd processor count grew from one to two. Both processors then receive every
span-start callback. Existing tests cover healthy success, unsupported
providers, raises-before-retention, or distinct failing provider instances;
they do not exercise this ambiguous same-provider transition.

This is separate from closed `FIND-TASK-009-1`. That finding proves a
registration exception does not escape or block explicit observations.
`FIND-TASK-009-8` concerns the state retained after that contained exception.

## Intended correction outcome

One provider receives no more than one Wyrd processor even when its registration
method retains the processor and then raises. Successful registration remains
idempotently successful. A failed or ambiguous attempt remains unavailable and
fail-open without being retried. Run entry, application exceptions, and explicit
Wyrd observations keep their existing behavior.

## Decision-complete recommendation

Keep the correction in the existing locked, weak per-provider registration
owner in `wyrd.otel`. Represent one terminal outcome for each attempted
provider: pending/failed before the foreign call and successful only after the
call returns normally. Later calls for a successful provider return `True`.
Later calls for a failed or ambiguous provider return `False` without handing
over another processor.

Reuse the current weak provider lifetime and module lock. Do not inspect private
provider pipelines to guess whether insertion occurred: the supported boundary
is duck-typed, and the foreign exception makes the side effect unknowable.
Do not retry that ambiguous side effect; TASK-009 already rejects an unnecessary
telemetry retry mechanism. This closes the source invariant once instead of
adding guards to Run entry or individual provider consumers.

The public Boolean must remain truthful: only a normally completed registration
is `True`; a failed or ambiguous terminal outcome is `False`. Preserve the
current unsupported-provider result and optional-import containment.

## Constraints and preserved behavior

- Preserve shared Rust ownership of Run identity and hydrated Card selection.
- Preserve active-span and child-span correlation, nesting, asyncio/task
  isolation, identical-Run concurrency, one private context key, and detach
  restoration.
- Preserve optional, fail-open OpenTelemetry behavior and strict Card lookup,
  authorization, validation, application exceptions, and explicit observation
  errors.
- Preserve weak provider lifetime, thread-safe registration, the explicit
  private-provider hook, and caller-owned provider/exporter lifecycle.
- Preserve exact `wyrd.card_ref` and `wyrd.run_id` attributes, authenticated
  ingest, server-derived identity, and the existing persisted joins.
- Preserve runtime/stub parity and all Rust, Python, and TypeScript Run APIs.

## Non-goals

- No provider wrapper, pipeline introspection, retry, warning, timeout, new
  dependency, second registry, exporter, queue, or lifecycle owner.
- No server Run, Bifrost/Gate/Scribe contract change, new correlation field,
  wrapper span, process-global Card scope, or mandatory OTel dependency.
- No changes to shared Rust Run behavior, TypeScript/Rust projections,
  persisted schemas, or unrelated tests.
- Do not reopen prior findings 1 through 7.

## Acceptance criteria

1. `FIND-TASK-009-8`: a single weak-referenceable provider that retains the
   supplied Wyrd processor and then raises receives exactly one processor across
   repeated direct installation and a later Run entry.
2. Every call after that failed or ambiguous attempt remains fail-open and
   reports `False` without invoking `add_span_processor` again; ordinary
   application exceptions and one representative explicit observation retain
   their existing behavior.
3. A provider that raises before retention is likewise attempted once per
   provider object and returns `False` on later calls without retry.
4. Healthy global and explicit private providers still return `True`
   idempotently, receive exactly one processor, and enrich active/child spans
   with the exact Run/Card pair.
5. Existing missing/API-only/attach/enrichment/detach failure containment,
   nested and async restoration, same-Run concurrency, public boundary parity,
   and authenticated persisted joins remain green.

## Focused proof and broader verification

Use the existing provider-registration test home in
`sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py`. The focused
case must reuse one provider object that retains the supplied processor before
raising, then exercise repeated direct installation and a Run entry whose
global lookup returns that exact object. Assert the processor count, Boolean
outcomes, unchanged application exception propagation, and representative
explicit-observation behavior. Keep the healthy provider coverage.

Name the exact focused test in implementation evidence and run it through the
repository toolchain, followed by the complete focused file:

```bash
mise run py:setup
cd sdks/wyrd-sdk-python
mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py::<exact_ambiguous_registration_test>
mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py
```

Then run the unchanged authenticated journey and the touched-surface gates from
the repository root:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/state/test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows'
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run codegen:check
mise run check:pyo3-scope
mise run py:format
mise run py:lints
git diff --check
```

Route this remediation directly to `$wyrd-implement`. A later task review must
reassess the complete original base-to-remediated-candidate range, not only
this correction.
