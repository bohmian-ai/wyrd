---
id: TASK-009-R5
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 46
requirements: [REQ-151, AC-032]
depends_on: [TASK-009, TASK-009-R1, TASK-009-R2, TASK-009-R3, TASK-009-R4E]
parent_task: TASK-009
remediates:
  - FIND-TASK-009-9
  - FIND-TASK-009-10
  - FIND-TASK-009-11
  - FIND-TASK-009-12
  - FIND-TASK-009-13
---

# Close provider identity, failed-registration enrichment, and proof state

Route directly to `$wyrd-implement`.

## Authority and immutable inputs

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 46
  (REQ-151 and AC-032).
- Architecture: `changes/active/verified-change-contract/architecture/logic/run_api.md`,
  section "Python OpenTelemetry run scope".
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`.
- Prior remediations: TASK-009-R1, TASK-009-R2, TASK-009-R3, and TASK-009-R4E.
  R4E supersedes TASK-009-R4 under revision 46.
- Round-five review: `changes/active/verified-change-contract/review/TASK-009-r5/`.
- Review base: `7d96c30066425e0cde2290842d5801307843283d`.
- Reviewed candidate: `11eb8ed2b64b70903f171335723dcc549b463352`.

## Issue diagnosis

### `FIND-TASK-009-9` — distinct equal providers share one outcome

REQ-151 requires registration to be attempted at most once per provider.
`install_run_correlation` stores outcomes in a `weakref.WeakKeyDictionary` at
`sdks/wyrd-sdk-python/python/wyrd/otel.py:200-203,239-269`.
`WeakKeyDictionary` follows referent hash and equality, so two distinct live
provider objects that compare equal address one entry. A failed first provider
can therefore suppress registration on a healthy equal peer. Spans from the
second provider receive no CardRef/Run ID pair and cannot participate in the
required persisted joins. Existing tests cover ordinary identity semantics,
not distinct equal providers.

### `FIND-TASK-009-10` — failed retained processor still enriches spans

Revision-46 REQ-151 defines a provider whose registration raises, including
after accepting the processor, as terminal `False` with no enrichment. The
candidate pre-marks the outcome `False`, passes one shared stateless processor
to the provider, and changes the outcome to `True` only after normal return.
An accept-then-raise provider keeps that shared processor active despite the
reported failure. Calling its retained processor inside a Run scope therefore
stamps both Wyrd attributes, contradicting the approved Boolean and
no-enrichment outcome. The current test proves one handoff but never exercises
the retained processor.

### `FIND-TASK-009-11` — changed Run authority claims revision 34

`changes/active/verified-change-contract/architecture/logic/run_api.md:3`
still says it is approved through specification revision 34 even though its
changed contract records revision-46 token-free behavior. That contradiction
makes the status of the owning authority ambiguous.

### `FIND-TASK-009-12` — superseded R4 remains selectable as ready

`TASK-009-R4-stabilize-provider-registration-idempotency.md` remains
machine-readable as `status: ready`, while its body and R4E's `supersedes`
metadata identify R4E as its approved replacement. Tooling can still select the
invalid revision-45 task.

### `FIND-TASK-009-13` — named Rust proof is not exact or replayable

The original task names two shared Rust tests and the Rust SDK journey, but its
implementation evidence records a regex-selected bare Cargo command for the
shared tests and only describes the journey as running under a wrapper.
`AGENTS.md` section 11 and the spec-driven test-command rule require a separate
exact `mise exec -- cargo nextest run` command for every named Rust test, plus
the complete repository-owned environment wrapper where required. Broader
green lanes do not replace that durable proof.

## Intended correction outcome

Each provider object's terminal registration outcome is weakly tracked by
identity. A registration attempt owns a processor that cannot enrich until the
foreign call returns normally; a processor retained by a failed call remains
inert forever. Healthy providers still enrich with the exact CardRef/Run ID
pair, failed providers remain fail-open and never retried, and providers are
not retained strongly. The Run authority, superseded task state, and exact
Rust verification evidence then agree with revision 46 and repository rules.

## Required correction

Keep the correction in the existing Python OTel registration owner. Reuse its
lock and terminal-outcome responsibility, but replace equality-keyed weak
outcomes with weak identity bookkeeping. Compare live referents with `is`,
discard dead entries during lookup, pre-mark the exact provider identity
`False`, and update only that identity to `True` after normal return. A linear
weak-reference collection is sufficient for the naturally small provider set;
do not introduce a registry abstraction or dependency.

Give each registration attempt its own initially inactive correlation
processor and activate it only after `add_span_processor` returns normally.
Thus, a processor retained by an accept-then-raise provider remains inert.
Remove the shared processor instance. This is the source correction: it keeps
invalid activation from being produced instead of adding guards to every span
consumer. Preserve the same terminal weak outcome, lock, one attempt per
provider identity, optional dependency, fail-open exception containment, and
caller-owned provider lifecycle.

Update the Run authority only where needed to identify revision 46 and remove
any statement that a shared stateless processor is compatible with the failed
registration outcome. Mark TASK-009-R4's frontmatter `superseded` while
retaining its historical body and R4E link.

Finally, run and record each named Rust test with its exact repository-native
command. The two shared tests require separate `mise exec -- cargo nextest`
commands with exact `test(=...)` selectors. The Rust SDK journey requires its
complete repository-managed Postgres setup and migration wrapper around an
exact `mise exec -- cargo nextest` command. Preserve the broader lane evidence;
do not alter implementation or tests merely to satisfy this evidence item.

## Constraints and preserved behavior

- Shared Rust remains the sole owner of Run identity and hydrated Card
  selection; Rust, Python, and TypeScript projections remain thin.
- Preserve exact attributes `wyrd.card_ref` and `wyrd.run_id`; inject no
  tenant, publisher, Card UID, or other server-owned identity.
- OpenTelemetry remains optional. `install_run_correlation(provider=None) ->
  bool` stays duck-typed and never raises.
- Registration remains at most once per provider identity with weak provider
  lifetime. Failed outcomes are terminal and are not retried.
- Healthy global and private providers continue to enrich active and child
  spans. Explicit observations remain independent of ambient OTel success.
- Preserve the import-time context key, tuple stack, nested/async isolation,
  top-matching exit, and fail-open boundaries approved by revision 46.
- Context exit remains unrelated to flush, shutdown, export, or durability.
- Preserve strict Card lookup, authorization, validation, explicit observation
  errors, and user exception propagation.

## Non-goals

- No provider wrapper, pipeline introspection, retry, warning, timeout, second
  registry, new dependency, exporter, queue, or provider lifecycle ownership.
- No server Run, Gate/Scribe/Bifrost change, wrapper span, process-global Card
  scope, baggage, resource attribute, or log/metric enrichment.
- No shared Rust Run behavior, language API, generated declaration, persisted
  schema, or journey behavior change.
- No redesign of the task lifecycle or historical remediation bodies.
- No broad documentation cleanup or additional test harness.

## Acceptance criteria

1. `FIND-TASK-009-9`: two distinct, weak-referenceable providers that compare
   equal each receive their own single registration attempt and terminal
   outcome. A failed first provider does not suppress a healthy equal peer,
   whose in-scope spans carry the exact CardRef/Run ID pair.
2. `FIND-TASK-009-10`: an accept-then-raise provider receives one processor;
   repeated installation returns `False` without another call, and invoking
   the retained processor inside a Run scope writes neither Wyrd attribute.
   Healthy global and private providers still return `True` idempotently and
   enrich with the exact pair.
3. `FIND-TASK-009-11`: the Run authority identifies specification revision 46
   and does not retain the invalid shared-stateless-processor allowance.
4. `FIND-TASK-009-12`: TASK-009-R4 has `status: superseded`; R4E remains its
   revision-46 replacement and the historical cross-reference remains intact.
5. `FIND-TASK-009-13`: the original task's evidence records all three complete
   Rust commands and zero-exit results. Every command uses `mise exec --`, each
   selector is exact, and the journey includes the complete Postgres setup and
   migration wrapper.

## Focused proof and broader verification

Add the two smallest focused Python regressions beside the existing provider
registration tests: distinct equal provider identities, and invocation of the
processor retained by an accept-then-raise provider. Run each exact test and
the complete focused file.

Record separate exact commands for the named shared tests:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=observe::tests::run_for_card_selects_the_initial_view_and_shares_its_invocation)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=observe::tests::run_for_card_refuses_an_unknown_alias_without_network_io)'
```

Run the named Rust SDK journey with the complete repository Postgres wrapper,
database migration setup, and this exact inner command:

```bash
mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test observe_run \
  -P journey --run-ignored=all \
  -E 'test(=scoped_run_emits_drift_eval_and_generic_rows)'
```

Then run the smallest broader lanes covering the touched implementation,
public Python surface, task artifacts, and repository boundaries:

```bash
mise run py:setup
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py::<equal-provider-test>)
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py::<accept-then-raise-test>)
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py)
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

On a failure, rerun with `WYRD_LOG=info` before diagnosing. Record every new or
renamed test and its exact command in implementation evidence. A later task
review must reassess the complete base-to-remediated-candidate range, not only
this correction.

## Stop conditions

Stop and report rather than improvising if identity-based weak bookkeeping
cannot avoid retaining providers strongly; failed retained processors cannot
be kept inert without changing the public installation contract; a required
join needs a Gate, Scribe, Bifrost, shared Rust, generated-contract, or schema
change; or any correction conflicts with specification revision 46.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1. `FIND-TASK-009-9` distinct equal providers | `python/wyrd/otel.py` `_outcomes` is a list of `[weakref, outcome]` pairs; `_outcome_entry` prunes dead refs and matches with `is` | `test_equal_providers_register_independently_by_identity` (red on prior code, green now) | PASS |
| 2. `FIND-TASK-009-10` retained processor inert | per-attempt `_RunCorrelationProcessor` with `active=False`, set `True` only after `add_span_processor` returns; shared `_PROCESSOR` removed | `test_a_processor_retained_by_a_failed_registration_never_enriches` (red on prior code, green now); existing healthy global/private and accept-then-raise tests still pass | PASS |
| 3. `FIND-TASK-009-11` Run authority revision | `architecture/logic/run_api.md` status now says revision 46; "stateless" allowance replaced with identity-keyed, per-attempt inert processor text | review | PASS |
| 4. `FIND-TASK-009-12` R4 superseded | `review/TASK-009-r4/TASK-009-R4-stabilize-provider-registration-idempotency.md` frontmatter `status: superseded`; body and R4E link unchanged | review | PASS |
| 5. `FIND-TASK-009-13` exact Rust proof | original task evidence records the three exact commands | all three exit 0 (journey: 1 passed under `with-test-postgres.sh` + `db:migrate:all:inner`) | PASS |

Verification (all exit 0):

```bash
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py::test_equal_providers_register_independently_by_identity)
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py::test_a_processor_retained_by_a_failed_registration_never_enriches)
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py)  # 39 passed
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=observe::tests::run_for_card_selects_the_initial_view_and_shares_its_invocation)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=observe::tests::run_for_card_refuses_an_unknown_alias_without_network_io)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && \
  mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test observe_run \
  -P journey --run-ignored=all -E 'test(=scoped_run_emits_drift_eval_and_generic_rows)'"
mise run py:setup
mise run py:test:unit
mise run py:test:integration   # 72 passed
mise run py:typecheck
mise run codegen:check
mise run check:pyo3-scope
mise run fmt
mise run lints
mise run py:format
mise run py:lints
git diff --check
```

Non-goals stayed out: no provider wrapper, retry, warning, new dependency, shared Rust, generated declaration, schema, or journey change. Changed files: `otel.py`, `test_observe_surface.py`, `run_api.md`, the R4 frontmatter, the original task's evidence, and this file.
