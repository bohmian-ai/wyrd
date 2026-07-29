# Bounded Implementation Execution

This reference defines the mandatory execution contract for an agent
implementing one approved Wyrd task. It is a reference document: use it to
check exact inputs, escalation conditions, verification rules, and completion
evidence. Whole-plan ordering and integrated closeout belong to
`wyrd-implement-plan`.

## Contents

- [Authority](#authority)
- [Required Task Contract](#required-task-contract)
- [Repository Preflight](#repository-preflight)
- [Autonomous Execution Loop](#autonomous-execution-loop)
- [Escalation and Deviations](#escalation-and-deviations)
- [Implementation Rules](#implementation-rules)
- [Risk-Specific Rules](#risk-specific-rules)
- [Test Integrity](#test-integrity)
- [Focused Verification](#focused-verification)
- [Final Diff Audit](#final-diff-audit)
- [Completion Standard](#completion-standard)

## Authority

Apply instructions in this order:

1. current user instructions;
2. active task;
3. approved plan;
4. applicable `AGENTS.md` files;
5. repository architecture and conventions;
6. local implementation preferences.

The task's behavior, acceptance criteria, non-goals, prohibited changes,
interfaces, feature requirements, focused verification, and escalation
conditions are authoritative.

An implementation agent executes the approved solution. It does not redesign
the solution, broaden scope, silently reinterpret requirements, or substitute
its preferred architecture. An approved task may explicitly replace a current
design decision; the implementor updates the named authority with the code. It
does not decide independently that a redesign is better.

## Required Task Contract

Do not edit until the active task defines:

- objective and observable outcome;
- requirements and non-goals;
- allowed scope and prohibited changes;
- interfaces, contracts, invariants, and public behavior;
- pseudocode or implementation structure and whether it is normative or
  illustrative;
- dependencies and exact feature requirements;
- acceptance criteria and required tests;
- focused verification commands and affected surface;
- escalation conditions;
- completion evidence.

Read the entire task, referenced plan context, applicable `AGENTS.md` files,
and required architecture references. Do not rely on a summary or partial
excerpt.

## Repository Preflight

Inspect only the task's affected surface:

- current implementation paths and callers;
- existing tests and fixtures;
- naming, ownership, and structural precedents;
- whether named files, crates, interfaces, and types exist;
- whether required interfaces or types already exist;
- default and optional Cargo features;
- relevant `mise` tasks and their actual commands;
- contradictions between the task and current source.

Proceed only when the approved behavior is clear, its owner exists, contracts
fit the approved design, dependencies and features are available, and the
acceptance criteria fit the allowed scope.

## Autonomous Execution Loop

One bounded task is one autonomous execution loop. Before editing, build a live
checklist containing every required change, acceptance criterion, required
test, focused verification command, and completion-evidence item. The checklist
must come only from the approved task; non-goals, later tasks, and adjacent
milestones are not remaining work.

Run:

```text
inspect -> implement -> test -> diagnose -> fix -> verify -> diff audit
   ^                                                          |
   +---------- while approved actionable work remains --------+
```

Update the checklist after each material result and take the next action that
advances an unchecked item. Do not pause for another prompt between coding,
test creation, diagnosis, repairs, verification reruns, and final audit.

Route every failure deliberately:

1. An implementation-caused failure returns to implementation and repair.
2. An incorrect command, filter, feature set, or test lane is corrected to the
   task-prescribed invocation and rerun.
3. A proven unrelated failure that does not prevent required proof is recorded
   and does not stop remaining checks.
4. A proven external or pre-existing failure that prevents required proof and
   has no approved alternative is `BLOCKED`; report the evidence and authority
   or environmental change required to resume.

Elapsed time, difficulty, context length, context compaction, partial progress,
a convenient handoff point, unwritten tests, remaining approved work, or a
recoverable tool failure do not end the loop. After compaction or recoverable
interruption, reconstruct state from the complete task, current diff, and
verification evidence and resume at the first unchecked item. Status updates
are interim checkpoints and do not terminate execution unless the user
explicitly stops or replaces the task.

The only voluntary terminal outcomes are:

- `COMPLETE`: every checklist item and acceptance criterion is satisfied and
  verified.
- `BLOCKED`: an escalation condition or unavailable required authority
  prevents correct completion.

Never voluntarily return partial work as `INCOMPLETE`. If actionable approved
work remains and no escalation condition applies, continue the loop.

## Escalation and Deviations

Stop before editing when:

- a required owner, type, module, command, or feature is missing and adding it
  changes architecture or scope;
- a public contract, migration, security boundary, existing test, or
  acceptance criterion conflicts with the task;
- a new dependency or undocumented feature is required;
- prohibited paths must change;
- verification must expand materially;
- later tasks must change to complete the active task;
- repository behavior materially contradicts an approved decision.

Do not escalate minor local choices that preserve semantics and existing
patterns.

A deviation is any unapproved difference in behavior, architecture, interface,
dependency use, feature selection, scope, verification, compatibility, error
semantics, or data model. When one is required:

1. stop before implementing it;
2. describe the conflict;
3. cite repository evidence;
4. explain why the approved approach cannot work;
5. present the smallest viable alternatives;
6. identify effects on requirements, tasks, tests, and closeout;
7. wait for replanning or explicit approval.

Never implement a preferred alternative first and disclose it afterward.

## Implementation Rules

Treat task behavior as normative. Preserve operation order, validation
boundaries, transaction boundaries, error mapping, concurrency, cancellation,
side-effect order, and data invariants.

Interpret task pseudocode according to its declared status:

- preserve normative semantics;
- adapt exact names only when the task permits repository alignment;
- adapt illustrative structure without changing behavior;
- never change a public interface without explicit permission.

Prefer existing owners, abstractions, helpers, errors, fixtures, dependencies,
and repository patterns. Do not add architectural layers, generic frameworks,
broad abstractions, dependencies, feature flags, helpers, extension points, or
refactors not required by acceptance criteria.

Implement only the active task. Do not:

- implement later tasks or adjacent milestones;
- reformat, rename, or rewrite unrelated code;
- upgrade or replace dependencies;
- modify unrelated tests;
- change requirements or acceptance criteria;
- alter the task or plan to match the implementation;
- perform speculative cleanup.

Every changed file must map to a requirement or necessary verification support.

## Risk-Specific Rules

| Surface | Mandatory behavior |
|---|---|
| Dependencies and features | Use only approved dependencies and exact features. Preserve version conventions, update lockfiles only as required, verify actual use, and record the change. Escalate unexpected transitive requirements or unrelated-crate effects. |
| Database migrations | Follow existing conventions and the approved rollout strategy. Keep migrations deterministic and deployment-order compatible. Do not make destructive changes without approval or modify prior committed migrations unless repository policy permits it. Add migration-focused validation. |
| Errors | Preserve structured errors and exact mappings. Do not collapse states, expose internals, swallow errors, convert recoverable errors into panics, add unspecified retries, log secrets, or change out-of-scope semantics. |
| Concurrency and transactions | Preserve synchronization, atomicity, cancellation, and transaction boundaries. Use existing primitives, avoid time-based synchronization, and test required races or state transitions. |
| Security | Reuse vetted auth, authorization, validation, secret, permission, and cryptographic primitives. Never design custom cryptography or log credentials. Preserve constant-time behavior where applicable. Test negative and replay behavior. Escalate ambiguity. |
| Documentation and comments | Add only task-required, repository-required, or behavior-critical documentation. Explain invariants, safety, transactions, concurrency, compatibility, or unusual choices. Do not narrate obvious code or update unrelated docs. |

## Test Integrity

Write tests alongside behavior. Map every new or changed test to an acceptance
criterion. Cover required success, regression, failure, boundary, error,
state-transition, concurrency, compatibility, and Wyrd user-journey behavior.
Do not add tests solely for line coverage.

Never make tests pass by:

- removing or broadly weakening assertions;
- adding sleeps instead of deterministic synchronization;
- ignoring or disabling cases;
- mocking away the behavior under test;
- swallowing errors;
- changing production behavior to match an incorrect test;
- replacing precise assertions with snapshots or existence checks.

Report a conflict when an existing test contradicts the approved task.

## Focused Verification

The task implementor proves the bounded task. The plan closeout agent proves
the integrated feature.

Run only the task's focused verification over the smallest complete affected
surface. Include consumers, integration tests, shared fixtures, generated
artifacts, schemas, migrations, and feature-gated code only when directly
affected. Escalate instead of silently broadening verification beyond the
approved task.

Follow task-provided commands exactly. Otherwise run:

1. repository-standard formatting for affected code;
2. formatting validation when required;
3. the narrowest relevant test;
4. the broader affected test target;
5. linting for affected crates;
6. task-specific integration, contract, codegen, or boundary checks;
7. `git diff --check`;
8. final diff inspection.

Fix a task-caused failing command before starting later commands. Correct and
rerun a command that used the wrong filter, feature set, or test lane. When a
failure is proven unrelated, continue if it does not prevent the task's
required proof; otherwise report `BLOCKED` with evidence rather than returning
partial work.

Prefer a repository `mise` task after confirming it exists, its implementation
and feature selection match the task, and it does not duplicate a later
required command. Use direct Cargo only when no suitable task exists, the task
would otherwise exceed allowed scope, a focused filter is needed, or the task
explicitly supplies the command. Never invent a `mise` task name.

Run all Rust-related commands sequentially across agents sharing a checkout or
target directory. Do not start background verification or continue
implementation while Cargo-backed verification runs.

Use default Cargo features unless the task explicitly earns exact optional
features. Never use `--all-features` for bounded implementation unless the
active task explicitly requires and justifies it. Full workspace and
all-feature validation belong to integrated plan closeout.

Keep formatting scoped where tooling permits. If a formatter touches unrelated
content, inspect the diff, restore unrelated changes when safe, and report
unavoidable broad formatting.

## Final Diff Audit

Inspect tracked and untracked changes. For every changed file, verify:

- the requirement and acceptance criterion requiring it;
- necessity and absence of unrelated formatting;
- public-contract, dependency, and feature effects;
- generated output provenance;
- test strength;
- absence of debug code, temporary files, secrets, and environment-specific
  values.

Update only task status fields or logs the task explicitly permits. Never edit
the objective, requirements, non-goals, architecture decisions, acceptance
criteria, approved interfaces, or closeout requirements.

## Completion Standard

Report `COMPLETE` only when every acceptance criterion is satisfied and
verified; required tests, lints, and formatting pass; commands ran
sequentially; required `mise` tasks and only approved features were used; no
prohibited or unrelated change remains; and every deviation has explicit
approval.

Report `BLOCKED` only when an escalation condition or unavailable required
authority prevents correct completion. Do not return a final report while
approved actionable work remains, and do not report `COMPLETE` with a failed
or unverified acceptance criterion.

Use this report:

```markdown
## Task Result

Status: COMPLETE | BLOCKED

### Summary
<Implemented outcome>

### Acceptance Criteria
- AC1: PASS | FAIL | UNVERIFIED — <source and test evidence>

### Files Changed
- `path` — <requirement or acceptance criterion>

### Tests Added or Updated
- <test> — <behavior proved>

### Verification
Commands executed sequentially:
1. `command` — PASS | FAIL

Features enabled:
- `crate`: default | `<exact features>`

### Deviations
- None | <deviation, approval, and impact>

### Remaining Work
- None | <blocked item and exact condition required to resume>

### Risks and Notes
- None | <unresolved non-blocking observation>

### Diff or Commit
- <reference when available>
```
