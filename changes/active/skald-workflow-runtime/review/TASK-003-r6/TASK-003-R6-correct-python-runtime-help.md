---
id: TASK-003-R6
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
parent_task: TASK-003
remediates: [FIND-TASK-003-15]
---

# Correct Python Workflow runtime help

Implementation skill: `$wyrd-implement`.

## Authority and immutable review subject

- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediations: TASK-003 R1 through R5 under
  `changes/active/skald-workflow-runtime/review/`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Reviewed candidate: `b954976648b49429f1c0950c4fa3509573885be8`
- Validated ledger:
  `changes/active/skald-workflow-runtime/review/TASK-003-r6/findings-validation.md`

The human-approved `WyrdGatewayCall.model: ModelRef` amendment, native-`401`
send-once correction, and run-start blocking/single-snapshot decisions remain
authoritative. This remediation changes documentation only.

## Issue diagnosis

### FIND-TASK-003-15 — public runtime help contradicts route-selected execution

The shared and Python runtime behavior is correct. `PyWorkflow` stores the
shared `ClientWorkflow`, and `PyWorkflow::run` delegates to that owner's
route-selected execution. Native steps use the process-local provider
registry; selected Wyrd-gateway steps use the retained or ambient Wyrd client;
selected external-gateway steps resolve their configured secrets at run start.

Both shipped declarations now state that contract correctly:

- `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi:716-725`
- `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:717-726`

The reachable PyO3 owner at
`sdks/wyrd-sdk-python/src/workflow.rs:537-544` still opens with “Run this
workflow against the process-local provider registry.” That unqualified claim
is false for Wyrd and external gateway routes. The same block also says only
that selected bindings' secrets “are read,” omitting the run-start timing
already stated by both declarations and implemented by the shared owner.

Because `PyWorkflow::run` is registered on the public native `agent` module and
re-exported by `wyrd.agent`, runtime introspection exposes this stale text. A
caller can therefore read local-only behavior from `help(Workflow.run)` while
the public declarations correctly disclose conditional configuration reads,
secret reads, and authenticated or external network IO. This is incomplete
closure of the existing public-contract finding, not a runtime defect or a new
product requirement.

## Intended correction outcome

The PyO3 runtime method and both shipped Python declarations describe the same
route-selected `Workflow.run` operation, including the run-start timing of
selected external secret reads. Runtime behavior, types, signatures, and
generated declarations remain unchanged.

## Decision-complete recommendation

Correct only the existing `PyWorkflow::run` documentation in
`sdks/wyrd-sdk-python/src/workflow.rs`:

1. Replace its opening sentence with the wording already present in both
   shipped declarations: “Run this workflow, preparing only what its step
   routes select.”
2. State that only selected bindings' secrets are read **at run start**.

Keep the rest of the method's route descriptions, GIL/runtime behavior,
arguments, return contract, and error contract intact. Reuse the existing
declaration wording rather than introducing another documentation source or
synchronization mechanism.

This is the smallest correction at the public runtime-documentation owner. Do
not modify the method body, shared Workflow implementation, route selection,
configuration or secret loading, stubs, stub assembly, package exports, or
tests merely to create churn. Do not add a semantic-documentation checker,
runtime introspection test, generator, declaration file, setting, option,
dependency, compatibility path, or test harness; those mechanisms are neither
established repository practice nor needed by comparable widely used projects
for this sentence-level repair and would be DRIFT.

## Constraints and preserved behavior

- Preserve the complete shared `ClientWorkflow` through Python loading,
  mutation, and execution.
- Preserve Native, Wyrd-gateway, and external-gateway route semantics.
- Preserve retained-client precedence, the client-less one-snapshot path,
  Native-only no-ambient-read behavior, and selected-secret-only resolution at
  run start.
- Preserve the shared runtime bridge and GIL release behavior.
- Preserve all public signatures, return values, errors, declarations, and
  package exports.
- Preserve native send-once renewal, fallback isolation, cancellation, remote
  lifecycle behavior, and every closed finding from TASK-003 R1 through R5.
- Do not broaden this task into runtime, SDK, stub-generation, configuration,
  documentation-pipeline, or test-harness refactoring.

## Acceptance criteria

| Finding | Required acceptance |
|---|---|
| `FIND-TASK-003-15` | The public PyO3 `Workflow.run` opening sentence describes route-selected preparation rather than local-only execution. |
| `FIND-TASK-003-15` | The PyO3 method states that only selected external bindings' secrets are read at run start. |
| `FIND-TASK-003-15` | Static comparison shows the PyO3 method, hand-authored source declaration, assembled public declaration, and shared route behavior describe the same Native, retained/ambient Wyrd-gateway, external-gateway, and selected-secret contract. |
| `FIND-TASK-003-15` | Runtime behavior, signatures, stubs, assembly, exports, tests, and adjacent documentation are otherwise unchanged. |
| `FIND-TASK-003-15` | No new checker, test harness, runtime introspection test, generator, declaration source, setting, option, dependency, compatibility path, or runtime mechanism is introduced. |

## Focused proof and broader verification

This is a documentation-only correction. Its focused proof is static source
comparison of the PyO3 method with the two existing declarations and the
shared `Workflow::run` route behavior; no new test is justified.

After the two sentence edits, run the existing ordinary checks for the touched
Rust/Python boundary:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run py:typecheck
```

Record the changed source location, static comparison, commands, and results in
this task. Do not add or widen a gate, and do not run the repository-wide
aggregate unless the actual implementation scope independently expands beyond
this documentation-only correction under `AGENTS.md`.

The next `$wyrd-task-review` must reassess the complete original
base-to-new-candidate range, the original task, all six remediation tasks, and
all prior verdicts; it must not review only the R6 delta.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-003-15`: PyO3 `Workflow.run` help matches the stubs | `sdks/wyrd-sdk-python/src/workflow.rs` `Workflow.run` opening now matches `python/wyrd/stubs/agent.pyi`, including run-start secret reads; commit `2ddc56734` | `cargo fmt --all --check`, `mise run codegen:check`, `git diff --check`: clean | PASS |

Closed at human direction ("fix it and move on to the next task") without a further review round.
