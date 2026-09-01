---
name: wyrd-implement
description: Implement and verify one ready Wyrd task through scenario-by-scenario Red-Green-Refactor cycles and focused repository verification. Use for one approved-spec implementation or remediation task; stop when implementation would require a spec revision.
---

# Wyrd Implement

Own one task's complete test-driven implementation, evidence, and fix loop.
Implementation reasoning belongs here; approved behavioral authority does not.

## Establish the task contract

Read the approved spec revision at `changes/active/<slug>/spec.md`, one ready
task under `changes/active/<slug>/tasks/`, its dependencies, and any parent task
or validated findings. Confirm mapped `REQ-*`, `INV-*`, and `AC-*` obligations.
A bounded direct instruction may be implemented outside the spec-driven
workflow only when the caller explicitly requests it and no material design
decision is unresolved; never use that exception to bypass an active approved
spec.

Read `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
[implementation execution](../../../architecture/references/languages/implementation-execution.md),
and [testing workflows](../../../architecture/references/languages/testing-workflows.md).
Start at [the reference router](../../../architecture/references/README.md),
then completely read every applicable [Wyrd design](../../../architecture/wyrd-design.md),
[Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx),
[Bifrost design](../../../architecture/bifrost-design.md), security, operations,
language, and domain reference linked by the task or required by its surface.

Follow CodeGraph instructions. Inspect the nearest owner, callers, consumers,
tests, manifests, features, generated projections, and `mise.toml`. Preserve
unrelated user changes. A write forecast is coordination guidance, not an
allowlist; report necessary scope expansion.

## Execute TDD one scenario at a time

For each ordered scenario:

1. **RED** — add or select one test and run the task's exact focused command.
   Confirm the failure is caused by the missing specified behavior, not setup,
   compilation, stale naming, or an unrelated baseline defect.
2. **GREEN** — implement the minimum cohesive repository-native behavior that
   makes the new test pass. Run that test and the relevant previously green
   focused tests.
3. **REFACTOR** — improve ownership, clarity, or structure when the green
   behavior reveals a better design. Keep the tests green.
4. Repeat for the next scenario.

Do not batch all RED tests before implementation. Do not weaken assertions,
delete or ignore tests, add sleeps in place of deterministic synchronization,
mock away required behavior, add unjustified allowances, or edit generated
artifacts instead of their source.

A pre-existing regression test may supply RED. If the behavior is already
implemented, return a verified no-op or add only genuinely missing regression
coverage. Use static or generated evidence for obligations that cannot
meaningfully execute; never manufacture a test solely to claim TDD.

If an exact task command is stale but its test and proof intent are unambiguous,
derive the current exact command, run it, and record the correction. A named
Rust test uses `mise exec -- cargo nextest run` with explicit package, target,
features, and `-E 'test(=...)'`; named Python and TypeScript tests use their
exact repository-native runner, path, and selector. Do not replace a required
integration or journey proof with a unit test.

## Implement complete closure

Satisfy the task's complete outcome, including required owners, consumers,
errors, cleanup, cancellation, audit, tenancy, generated surfaces,
cross-language projections, documentation, and tests. Follow Wyrd's
struct-centered Rust, rustdoc, async, PyO3, contract, and test-tier rules.

Ordinary private mechanics remain the implementer's choice. Stop when
correctness requires changing the approved behavior, invariant, acceptance,
public or persisted contract, material architecture, ownership, security,
tenancy, migration, or dependency/feature decision. Report
`SPEC_REVISION_REQUIRED` with exact evidence; do not rewrite the spec or task.

## Verify

After the scenario cycles, run sequentially:

1. all task-named focused tests by their exact commands;
2. the narrowest applicable module-, crate-, family-, integration-, journey-,
   codegen-, typing-, docs-, and boundary `mise run` tasks;
3. required format and lint lanes from `AGENTS.md`;
4. `git diff --check`; and
5. a final tracked and untracked diff audit.

`mise run gate` is reserved for the broad changes identified by `AGENTS.md` or
an explicit request. Diagnose and fix task-local failures while an in-scope
recovery path remains. Failed or missing proof is never success.

## Record and hand off

Append compact execution evidence to the active task without rewriting its
objective, mapped spec obligations, or accepted behavior. Record each
scenario's expected RED failure, GREEN result, any refactor, broader
verification, command corrections, and material limitations.

Return `COMPLETE`, `SPEC_REVISION_REQUIRED`, or `BLOCKED`, followed by the
implemented outcome, changed owners, spec-obligation evidence trace, exact
commands and results, bounded corrections, risks, and diff or commit reference.

Create a commit only when requested. Implementation completion does not approve
the task, merge it, or authorize deployment; route the immutable cumulative
candidate to `$wyrd-task-review`.
