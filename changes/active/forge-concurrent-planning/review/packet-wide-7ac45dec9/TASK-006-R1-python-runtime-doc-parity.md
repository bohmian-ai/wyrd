---
id: TASK-006-R1
kind: remediation
status: ready
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

