---
id: TASK-006-R1
kind: remediation
status: review
spec: SPEC-forge-concurrent-planning
spec_revision: 11
parent_task: TASK-006
remediates: [FIND-TASK-006-1, FIND-TASK-006-2]
---

# Reconcile Python runtime, stubs, and callback help

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`
- Original task: `changes/active/forge-concurrent-planning/revision/TASK-006-python-api-docstrings.md`
- Reviewed candidate/base: `7ac45dec99535c881b7a936c66623044f15d8823` / `c1508b375`

## Diagnosis

Hand-authored and generated declarations disagree with runtime for `Role`,
`SessionTurn`, Agent mutators/callback registration, and `WyrdError`
construction. Type checking therefore approves calls runtime rejects and hides
supported methods. Native help also says after-hook exceptions abort, but the
reachable chain returns `Abort` to consumers that execute `unreachable!`, so a
documented action can panic the run.

## Intended correction outcome

Runtime help, source stubs, generated stubs, public exports, and top-level tests
describe and exercise one actual Python API. No documented callback behavior
leads to an undocumented panic.

## Decision-complete recommendation

Resolve every task-recorded drift row at its existing hand-authored source,
including `Role.System`, keyword-only `SessionTurn`, actual property names and
model methods, Agent methods, and ordinary exception construction. Regenerate
outputs; do not hand-edit them or redesign the generator. TASK-006 does not
authorize a new callback contract: remove unsupported raise-to-abort promises
and document supported behavior. Only if an existing authoritative callback
contract already requires abort should the shared `loop_runtime` owner be
corrected once; do not add Python-edge guards.

## Preserved behavior and non-goals

- Preserve builder-created structured Wyrd errors and existing callback owner
  boundaries.
- Preserve PyO3 scope and top-level-only Python tests.
- Do not invent constructor overloads or broaden this into callback redesign.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-006-1` | Runtime/source/generated declarations and help agree for the complete recorded public inventory. |
| `FIND-TASK-006-2` | Raising from each after-hook produces exactly the documented result and never an undocumented panic. |

## Focused proof and broader verification

Use top-level Python parity tests for Role, SessionTurn, Agent methods, direct
and builder-created errors, and every after-hook. Run codegen check, Python unit
tests, typecheck, PyO3 scope check, format/lints, and diff check.

## Implementation Evidence

Drift rows 2, 3, 4, and 7 of TASK-006's recorded disagreements are resolved at
their hand-authored sources (`python/wyrd/stubs/*.pyi`, native rustdoc);
generated `python/wyrd/**/__init__.pyi` and `_wyrd.pyi` come only from
`mise run codegen:stubs`. Rows 1 (TASK-004-R1), 5, 6, 8, 9, and 10 are behavior
or other-owner contract items outside this remediation.

Callback decision: the existing public contract already requires abort —
`CallbackOutcome::Abort` ("Terminate the agent run with this error as the
cause") and `FinishReason::CallbackAborted` ("a callback returned
`CallbackOutcome::Abort` ... or raised") — so the shared
`skald-agent/src/loop_runtime.rs` owner was corrected once: an `after_model`
abort discards the response and returns `CallbackAborted`; an `after_agent`
abort returns `CallbackAborted` keeping the held run's iterations and
conversation (`ChainResult::Abort` now carries the held value, no clone); an
`after_tool` abort reports that call to the model as failed and the run
continues, matching the established `before_tool` tool-hook behavior. All
three `unreachable!` arms are gone; no Python-edge guard was added.
`CallbackOutcome::Abort` rustdoc and the native `Agent` argument help now state
exactly these results.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-006-1`: runtime, source, generated declarations, and help agree for the recorded inventory | `stubs/agent.pyi` (`Role.System`, keyword-only `SessionTurn` with `call_id`/`model_*`, Agent `add_tool`/`set_tools`/`with_*`/`add_*`), `stubs/error.pyi` (plain `Exception(*args)` construction), `stubs/testing.pyi` (11 methods), `stubs/state.pyi` defaults, `stubs/model.pyi` (keyword-only `ModelCardMetadata`, no phantom getters, required `hf_task`), `stubs/data.pyi` (`DataCardMetadata.to_dict`), `stubs/prompt.pyi` (static helpers, no `PromptCardMetadata.prompt`, `PromptCard.model_dump`, no phantom `is_card`), `stubs/cards.pyi` (`AgentCard.kind`), `stubs/header.pyi` (`run_wyrd_cli`); regenerated public stubs | `tests/unit/runtime/test_public_api_parity.py` (Role, SessionTurn, direct and builder-created errors, Agent mutators and `add_*` chaining, PromptCard) PASS and type-checked by `py:typecheck`; one-off runtime-vs-generated-stub member scan shows no remaining drift on the inventory classes; `codegen:check` clean | PASS |
| `FIND-TASK-006-2`: raising from each after-hook produces exactly the documented result, never a panic | `crates/skald/skald-agent/src/{loop_runtime.rs,callbacks.rs,python.rs}`; `stubs/agent.pyi` callback prose | Rust `agent_run_after_{model,agent,tool}_abort_*` PASS; Python `test_after_model_raise_*`, `test_after_agent_raise_*`, `test_after_tool_raise_reports_failed_tool_call_and_continues` PASS | PASS |

Retired check: `tests/unit/cards/model/test_model_public_surface.py::test_model_stub_args_docs_use_name_type_description_format`
(and its `_args_doc_lines` helper) required every `Args:` line in
`stubs/model.pyi` to repeat the parameter type as `name (type):`. TASK-006's
approved contract forbids duplicating a type signature as prose and the
TASK-006 sweep removed those 74 annotations, so the test was red at the
reviewed candidate. The property is no longer wanted; parameter types are
enforced by the stub signatures and `py:typecheck`.

Non-goals held: no constructor overloads, no callback redesign beyond removing
the panics the existing contract forbids, builder-created structured errors and
PyO3 scope unchanged, Python tests top-level only.

### Commands

| Command | Result |
|---|---|
| `mise exec -- cargo nextest run --locked -p skald-agent --test callbacks -E 'test(/after_(model\|agent\|tool)_abort/)'` | 3 passed |
| `mise exec -- cargo nextest run --locked -p skald-agent` | 115 passed |
| `mise run test:skald` | 424 passed |
| `mise run py:test:unit` | 539 passed |
| `mise exec -- uv run python -m pytest -q tests/unit/runtime/test_public_api_parity.py tests/unit/runtime/test_callbacks.py` | passed |
| `mise run py:typecheck` | pass (parity test added to the lane in `mise.toml`) |
| `mise run codegen:check` | pass |
| `mise run check:pyo3-scope` | pass |
| `mise run py:format`; `mise run py:lints` | pass |
| `mise run fmt`; `mise run lints` | pass |
| `git diff --check` | clean |

