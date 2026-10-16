---
id: TASK-002-R2
kind: remediation
status: complete
spec: SPEC-verifier-building-blocks
spec_revision: 6
parent_task: TASK-002
remediates: [FIND-TASK-002-5]
---

# Complete three required executor rustdoc items

Execute with `$wyrd-implement`. This closes the remaining portion of an existing mandatory finding; do not change runtime behavior.

## Subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-002`.
Approved spec: `changes/active/verifier-building-blocks/spec.md`, revision 6.
Original task: `changes/active/verifier-building-blocks/tasks/TASK-002-verification-engine.md`.
Original base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
Prior candidate: `d342c54749a871dcacf5b0c7ec29e3b458f2d9e2`.
Current reviewed candidate: `99cf2f18a8cadfe3c500cf7157934047112d156a`.
Read this directory's `verdict.md` and `findings-validation.md`, AGENTS §16 and agent-rules. Prior remediation is `remediation-task.md`; immutable authority/diffs/previous findings are preserved alongside it.

## Diagnosis

All functional findings (FIND-TASK-002-1 through -4) are independently closed. Original FIND-5 omissions were corrected or removed, but the final shared-executor write set introduces three remaining undocumented items in `crates/vala/vala-eval/src/executor.rs`:

1. `IntoIterator::Item` at line 194 and `IntoIterator::IntoIter` at 195 are new explicit associated types without attached rustdoc. The iterator is consumed by both execution paths and yields owned task/settlement pairs in settlement order. Surrounding and trait docs establish the behavior but do not satisfy AGENTS §16's explicit item documentation requirement.
2. `first_skipped_dependency_returns_declaration_order` at line 547 was materially changed to construct the new generic ordered ledger, deliberately inserting b before a while declared dependency order is a before b. It lacks rustdoc and Errors despite returning Result from typed-ID/DAG/registry setup, plus contains a registry-presence expectation and an outcome assertion. This is the changed regression for the new state representation, not an untouched sibling test.

The mandatory documentation gate remains unmet. No runtime failure is alleged. Green library/Clippy checks do not inspect every private/test item. Independent follow-up and final validation rejected requests to repeat cancellation prose on execute_plan, drive, attempt, judge or trace, because their linked/surrounding lifecycle contracts suffice. Those rejected requests are outside this task.

## Selected correction and preserved boundaries

Add concise attached rustdoc to the two associated types, describing the owned `(TaskId, O)` settlement and consuming settlement-order iterator. Document the changed test's purpose (declared dependency order wins over insertion order), Errors for actual propagated setup failures, and Panics for its registry invariant/outcome assertion.

Keep the native IntoIterator implementation, ledger, test setup/assertions and all runtime behavior unchanged. Inspect each complete body to keep documentation truthful. Existing owners already provide the required behavior; no deletion, abstraction, guard, helper, dependency or new test is necessary. Do not document untouched siblings, repeat rejected async prose, invent fixture failures, alter public types, or stage the untracked parent packet. Preserve all source-closed functional findings and scenario behavior.

The write set is attached rustdoc at these three items in `executor.rs` only. No server, SQL, SDK, hosted runtime, statistical, scheduling, persistence, policy, concurrency or public-format change is authorized. This task does not require a new specification decision.

## Acceptance and proof

FIND-TASK-002-5 closes when both associated types have accurate attached rustdoc and the materially changed regression has substantive purpose/Errors/actual Panics documentation, with a documentation-only diff preserving behavior. Inspect those docs against the unchanged full bodies and AGENTS §16. Do not create a prose-only runtime test or manufacture a RED run.

Run only the narrow relevant checks:

```bash
mise run fmt:check
git diff --check
```

The immutable candidate already passed 198 library tests, the three exact functional regressions, scoped Clippy, dependencies and formatting. No fresh library, integration, journey or broad aggregate is required for this documentation-only write set. If behavioral edits become necessary, stop and report that expanded scope rather than hiding them in this correction.

Record actual evidence and request a fresh immutable cumulative `$wyrd-task-review` from original base `41b0f01d9`, including both prior verdicts/ledgers and remediation tasks. The current review orchestrator does not execute this task.

## Implementation evidence

Candidate: `99cf2f18a..ec2a8bb3b` (one commit). The write set is attached rustdoc in `crates/vala/vala-eval/src/executor.rs` only: 13 added lines, every one a `///` line, with no other line changed.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `IntoIterator::Item` and `IntoIter` have accurate attached rustdoc | `executor.rs` `impl IntoIterator for RunLedger<O>`: `Item` documents an owned `(TaskId, O)` settlement, and `IntoIter` documents a consuming iterator in settlement order (`entries.into_iter()`) | Checked against the unchanged `RunLedger::push` / `into_iter` bodies | PASS |
| The changed regression documents its purpose, Errors and actual Panics | `executor::tests::first_skipped_dependency_returns_declaration_order`: declared dependency order wins over settlement order (`b` is pushed before `a`, and `c` reports `a`); Errors for the `tid` / `validate_dag` / `TaskRegistry::from_plan` `?` paths; Panics for the `expect("task c registered")` and the `assert_eq!` | Checked against the unchanged test body | PASS |
| Documentation-only diff | `git diff 99cf2f18a..ec2a8bb3b` adds only `///` lines | `mise run fmt:check` (exit 0); `git diff --check` (exit 0) | PASS |

Non-goals stayed excluded:
- No runtime, test-body, public-type, server, SQL, SDK or scheduling change was made.
- No new test was added and no RED run was manufactured.
- The untracked parent packet was not staged.

Status: IMPLEMENTED. This is ready for a fresh immutable cumulative `$wyrd-task-review` from base `41b0f01d9`, together with both prior verdicts, their ledgers and the remediation tasks.
