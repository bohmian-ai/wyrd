---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-long-horizon-verification
spec_revision: 13
requirements: [REQ-012, AC-007, INV-001, INV-002, INV-003, INV-004, INV-005]
depends_on: []
---

# Scheduled bindings use the Verifier's samples declaration

## Outcome and Value

Register a scheduled drift Verifier once, without redundant binding inputs;
every declared drift requirement is judged, including insufficient-data cases.
Own REQ-012/AC-007, through the existing three-SDK scheduled-drift workflow.

## Owners, Scope, Consumers, and Prohibited Changes

Execute with `$wyrd-implement`. All INV-001–005 apply. No dependencies: this is
independently verifiable on today's scheduled activation. TASK-001 changes the
same contract/registration/queue owners for groups; integrate shared-file edits
rather than revert them, while preserving this task's standalone outcome.

Remove authored schedule-binding inputs and optional samples, including their
runtime/frozen-input consumers. Refuse the removed fields by authored path
using existing contract error families; no compatibility alias or ignored map.
Ordinary JSON/media/trace optionality, non-group on_failure, Drift scoring,
baseline ownership and Window submit recording remain unchanged. No grouping,
outbox admission or benchmark work belongs here.

## Source-backed Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Schedule contract | VerifierSpec::validate_schedule/validate_slot; VerificationBinding and samples InputSlot | verifier_fixtures; components/cards/resolve.rs::check_activation | Selectors repeat samples features; optional samples can be omitted | Remove binding mapping and samples required:false; validate all samples from Verifier | No new selector/default/fallback scheme |
| Claim/runtime | wyrd-sql verifier_executions schedule_inputs / ExecutionInput::Scheduled; DriftEngine::checks | scheduler claims and runner dispatch; pg_verification_runtime | Frozen slots used for contains_key, allowing check omission | Derive all samples/checks from pinned Verifier plus subject/window | No second frozen selector map or changed Drift algorithm |
| Caller closure | Existing Rust/Python/TS scheduled_drift_alerts_operator owners | Fixture registration → submit samples → due schedule → result → hook | Authored examples/fixtures/types repeat inputs | Update shared contracts, fixtures/docs/examples/generated projections and assertions | Reuse existing journeys, no new test server or transport |

## Approach

1. Add field-specific registration negatives and two-requirement scheduled journeys.
2. Remove duplicate mapping and optional-samples declaration through shared validation and every consumer.
3. Make claim/runtime supply all samples/checks from exact Verifier declarations.
4. Close SDK/schema/example/docs consumers and prove unchanged scoring/Operators.

## Ordered Implementation Scenarios

### Scenario 1 — Refuse redundant authoring with a useful field error

**Behavior.** AC-007: schedule binding inputs, even {} or exact selectors,
and samples required:false are refused at those fields. No invalid Card or
binding persists; normal optional JSON/media/trace slots still work.

**RED.** Add scheduled_binding_rejects_redundant_inputs to the existing
scheduled-drift journey owner in each SDK (exact commands below), including
raw HTTP registration to prove server enforcement if local SDK validation
refuses first. Today those authored shapes are accepted.

**GREEN.** Remove the fields from the contract and enforce refusal in existing
registration/validation without changing unrelated optional slot behavior.

**REFACTOR.** Eliminate only obsolete selector/optional-samples branches; regenerate
projections from source and preserve field-specific refusal evidence.

### Scenario 2 — Judge every drift requirement from one declaration

**Behavior.** AC-007: two slots latency/error_rate and two drift requirements
run without binding inputs. Fixture values 120/0.01 yield latency failed,
error_rate passed, overall failed; 80/insufficient error_rate yields passed/
inconclusive, overall inconclusive. Replay/reclaim preserves exact pinned
subject/window/requirements; on_failure behavior remains existing.

**RED.** Add scheduled_binding_scores_every_requirement to each SDK owner,
using existing real-server fitted/sample/clock workflows. Omit binding inputs;
current validation or omission path cannot deliver the required complete result.

**GREEN.** Derive all scheduled samples from the pinned Verifier and remove the
claim/runtime omission gate. Retain actual Drift scoring and existing Operators.

**REFACTOR.** Delete redundant schedule input representations and stale examples
once consumers migrate. Rerun both scenarios plus existing scheduled regressions.

## Acceptance Criteria

- Both scenarios hold through Rust/Python/TypeScript and server HTTP registration.
- Every drift requirement appears with the correct actual outcome; no optional-slot omission produces a pass.
- Existing scheduled scoring, empty windows, baseline fitting and Operator delivery retain their behavior; schemas/docs/fixtures accept no removed fields.
- No changes to group lifecycle or outbox semantics and no aliases remain.

## Expected Write Set and Consumer Closure

Likely owners: crates/wyrd-spec Verifier/InputSlot/binding validation and schema;
crates/wyrd/wyrd-server registration/drift/runtime; crates/wyrd/wyrd-sql schedule
claim and frozen execution input consumers; SDK Card projections and fixtures;
examples/docs/architecture samples wording; generated schemas/types from source.
Paths are guidance, not a helper/module allowlist. Update all affected existing
schedule-binding fixtures, including the capacity binary's fixtures if needed;
TASK-004 owns new performance cases, not broken authoring left by this removal.

## Verification and Evidence

The existing test roots are Rust package wyrd-ai, integration target/module
scheduled_drift_alerts_operator; Python and TS modules below. The two new test
names are planned, not pre-existing. Add them to these registered owners and
confirm selected names/counts before claiming proof.

Exact Scenario 1 selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::scheduled_binding_rejects_redundant_inputs)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_scheduled_drift_alerts_operator.py::test_scheduled_binding_rejects_redundant_inputs'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/scheduled-drift-alerts-operator.test.ts -t "^scheduled_binding_rejects_redundant_inputs$"'
```

Exact Scenario 2 selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::scheduled_binding_scores_every_requirement)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_scheduled_drift_alerts_operator.py::test_scheduled_binding_scores_every_requirement'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/scheduled-drift-alerts-operator.test.ts -t "^scheduled_binding_scores_every_requirement$"'
```

Build changed SDK dependencies once using mise run py:setup, ts:build and
ts:build:testing. Existing focused scheduled regression (names/source inspected):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=scheduled_drift_alerts_operator::passing_window_alerts_no_one) | test(=scheduled_drift_alerts_operator::empty_window_is_inconclusive_and_alerts_no_one) | test(=scheduled_drift_alerts_operator::ready_spc_baseline_charts_a_scheduled_window)"'
```

The new Python/TS scenarios must retain the equivalent passing/failed/empty/
baseline branches locally, rather than run entire language journey suites.
Additional supporting tests get confirmed exact commands in implementation evidence.

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:deps
mise run check:tenant-isolation
```

When language source/stubs change, run corresponding mise run py:format,
py:lints, py:typecheck or ts:format, ts:lints, ts:typecheck and ts:napi:check.
For docs/ edits run mise run docs:check. Regeneration is source-driven;
format/schema/doc checks are static proof, not manufactured RED. No full
journey suite or broad aggregate is prescribed for this task.

Append the `$wyrd-implement` acceptance evidence table with source locations,
exact RED/GREEN/REFACTOR commands/results, selected counts/features, actual
write-set/non-goal audit and unresolved integrated evidence. Diagnose unexpected
failures with tracing per that skill. Implementation does not approve or merge.

## Material Stop Conditions

Do not change the approved spec or add aliases, another owner/queue/transport,
producer Postgres dependencies or unrelated policy. Resolve private mechanics
through `$wyrd-implement`; route a required material change through `$wyrd-spec`.
Do not add tasks to the owner-approved four-task split.

## Authority Links


- [Approved specification, revision 13](../spec.md).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md).
- [Wyrd design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx), [Bifrost design](../../../../architecture/bifrost-design.md).
- [Spec workflow](../../../../architecture/references/languages/spec-driven-development.md), [implementation execution](../../../../architecture/references/languages/implementation-execution.md), [testing workflows](../../../../architecture/references/languages/testing-workflows.md).
- Execute with [$wyrd-implement](../../../../.agents/skills/wyrd-implement/SKILL.md); implementation returns evidence, not independent task approval.

## Implementation Evidence

Commits: `a2959a435` (contract/runtime/SQL removal), `727bd1d67` (Rust journeys
and fixtures), `c4e1eb6a7` (Python), `4a198a9d2` (TypeScript and regenerated
`card-types.ts`), `48904dd00` (remaining test fixtures, docs, architecture).

Reuse: refusal-by-path is one `serde_path_to_error` decode in
`wyrd-spec/src/envelope.rs::spec_from_kind_value` (already in the lockfile via
axum) feeding the existing `deny_unknown_fields` contract; no removed-field
check, alias or ignored map. Server refusal is the existing
`WYRD_REGISTRY_400_INVALID_CARD_SPEC`; SDK loader refusal is the existing
`WYRD_LOADER_400_INVALID_ENVELOPE`, both carrying the authored path.
`verification_bindings.inputs` is dropped by additive migration
`20261017000000_drop_verification_binding_inputs.sql`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Binding `inputs` (`{}` or exact selectors) and samples `required: false` refused at their path; nothing persists | `VerificationBinding` has no `inputs`; `InputSlot::Samples` has no `required` (`wyrd-spec/src/card/verifier/mod.rs`); path-prefixed decode (`envelope.rs`); `fixtures/invalid/{scheduled-binding-empty-inputs,scheduled-binding-exact-inputs,optional-samples-slot}.yaml` | `scheduled_binding_rejects_redundant_inputs` in Rust (SDK + raw HTTP 400 + 404 resolve), Python (3 params), TS (3 cases); `card::verifier_card_tests::removed_scheduled_samples_fields_fail_at_their_path` | PASS |
| Every drift requirement scored from the pinned Verifier; no omission path | `DriftEngine::checks` scores all `spec.drift_checks()` (`wyrd-server/src/verification/drift.rs`); `ExecutionInput::Scheduled { window }` without slots and `schedule_inputs` claim subquery removed (`wyrd-sql/src/queries/verifier_executions.rs`); `check_activation` only requires a samples slot (`components/cards/resolve.rs`) | `scheduled_binding_scores_every_requirement`: 120/0.01 → latency failed, error_rate passed, overall failed; 80/no error_rate → passed/inconclusive, overall inconclusive (Rust, Python 2 params, TS 2 cases) | PASS |
| Existing scheduled scoring, empty windows, baseline fitting, Operator delivery unchanged | Shared harness helpers keep the fitted path; Custom-only path skips baseline wait | Rust `scheduled_drift_alerts_operator::` 7/7; Python file 10/10; TS file 10/10; `pg_verification_runtime`, `pg_operator_delivery`, `pg_verification_routes`, `pg_card_registration_route` 93/93; `wyrd-testing --test server verification_runtime::` 10/10 | PASS |
| Schemas/docs/fixtures accept no removed fields | `codegen:regen` output; fixtures stripped; docs/architecture rewritten; `fixtures/README.md` rows added | `codegen:check`, `docs:check`, `wyrd-spec` 760/760, CLI loader/card_lifecycle 21/21, MCP `verification::` 5/5 | PASS |
| No group lifecycle/outbox changes; no aliases | Diff audit below | `git diff main...HEAD --stat` inspected | PASS |

Commands (all exit 0):

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec                      # 760 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && \
  cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E "test(/^pg_verification_bindings::|^pg_verifier_executions::/)" && \
  cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(/^(pg_card_registration_route|pg_verification_routes|pg_verification_runtime|pg_operator_delivery)::/)" && \
  cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(/verification_runtime::/)" && \
  cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(/^verification::/)" && \
  cargo nextest run --locked -p wyrd-cli --test cli -E "test(/loader::end_to_end|card_lifecycle/)" && \
  cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(/^scheduled_drift_alerts_operator::/)"'
  # 32, 93, 10, 5, 21, 7 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && \
  uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_scheduled_drift_alerts_operator.py && \
  cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/scheduled-drift-alerts-operator.test.ts'
  # Python 10 passed; TS 10 passed
mise run fmt && mise run lints && mise run codegen:check && mise run check:deps && \
  mise run check:tenant-isolation && mise run ts:napi:check && mise run docs:check && git diff --check
mise run ts:format && mise run ts:lints && mise run ts:typecheck   # TS sources touched
```

Exact Scenario selectors run (the Rust and Python selectors in this task are
correct as written; the TS `-t` selectors are corrected to the repository's
spaced test names):

```bash
... cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::scheduled_binding_rejects_redundant_inputs)"
... cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::scheduled_binding_scores_every_requirement)"
... pnpm exec vitest run tests/integration/scheduled-drift-alerts-operator.test.ts -t "^scheduled binding (rejects redundant inputs|scores every requirement)"   # 5 passed
```

Material notes:

- RED was not executed as a separate run: both scenarios were implemented with
  the contract removal. The prior contract accepted binding `inputs` and
  samples `required: false` (removed `ScheduledInput`, `validate_schedule`,
  `schedule_inputs` claim column) and filtered drift checks by frozen slots,
  established by source inspection of the parent commit.
- Deleted `wyrd-testing/tests/bifrost/server/verification_runtime.rs::omitted_optional_slot_is_not_scored_from_retained_rows`:
  it asserted the removed omission behavior, which AC-007 forbids.
- Python stubs are unchanged (no `.pyi` exposed the removed fields), so
  `py:typecheck` was not required.
- No unexpected failures occurred; no diagnosis entries.

Non-goal audit: no grouping, group lifecycle, outbox admission, benchmark or
capacity-case changes; JSON/media/trace `required` optionality, `on_failure`,
Drift algorithms, baseline ownership and Window submit recording unchanged.
No compatibility alias or ignored field remains.

Return: `IMPLEMENTED`.
