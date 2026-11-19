# Bounded Implementation Execution

This file defines the execution discipline for an actionable Wyrd change.
It covers authority classification, controlled adaptation, verification
recovery, test integrity, diff audit, and completion evidence without requiring
a particular request or packet shape.

## Contents

- [Authority](#authority)
- [Change contract](#change-contract)
- [Three execution classes](#three-execution-classes)
- [Execution record](#execution-record)
- [Failure routing](#failure-routing)
- [Repository-managed environments](#repository-managed-environments)
- [Equivalent verification](#equivalent-verification)
- [Implementation and test integrity](#implementation-and-test-integrity)
- [Focused verification](#focused-verification)
- [Final diff audit](#final-diff-audit)
- [Completion standard](#completion-standard)

## Authority

Apply instructions in the order defined in
[spec-driven development](spec-driven-development.md#authority).

The request fixes required behavior, acceptance outcomes, explicitly approved
public or persisted contracts, prohibited changes, and safety boundaries.
Repository architecture remains authoritative for ownership, protocol, tenant,
security, and operational invariants unless the user explicitly changes that
authority. Repository reality fixes private mechanics when several approaches
satisfy the same contract.

## Change contract

Before editing, establish:

- objective, requirements, non-goals, and acceptance criteria;
- allowed and prohibited material scope;
- expensive-to-reverse ownership, interface, and invariant decisions already
  fixed by the task or architecture;
- likely owners and consumers, without treating paths as a private
  implementation allowlist;
- focused verification and completion evidence; and
- material stop conditions.

An approved task is the normal input for a spec-driven change. Bounded direct
instructions remain valid outside that workflow when they are already
decision-complete and the caller explicitly requests implementation. Do not use
that exception to bypass an approved spec or unresolved material design.

Use the task's existing acceptance entries as the live checklist and evidence
record until every required outcome and proof is complete. Do not create a
second obligations ledger or evidence table.

Requirements remain defined in the approved spec. Task scenarios exercise
them; implementation only adds evidence and source-backed coverage corrections.
Preserve the expected behavior when recording results. Follow
[ambiguous wording](spec-driven-development.md#ambiguous-wording) rather than
guessing, omitting behavior, or introducing another promise.

Do not require the task to predict helpers, private methods, local module
structure, exact control flow, already-approved dependency APIs, test fixture
structure, or other reversible choices. The implementation agent owns them.

Before adding architectural machinery, revalidate relevant reuse evidence as
defined in
[reuse evidence](spec-driven-development.md#reuse-evidence-and-task-conflicts).

## Three execution classes

### Local and reversible

Implement autonomously:

- private helpers and local structure;
- rustdoc, formatting, and diff-caused lint repairs;
- mechanical caller and test updates;
- current equivalents of private names and paths;
- existing fixtures and test-support seams;
- in-scope compile and test repairs.

These changes preserve behavior and remain inside the established owner.

### Bounded correction

Proceed, record, and continue:

- replacing a defective command or filter with equivalent non-weaker proof;
- adding a narrow adjacent private fixture or support file within the owner;
- repairing a small pre-existing defect in touched code;
- provisioning repository-managed local services, migrations, and test
  environment;
- recording repository discoveries with the execution evidence.

The expected file list is not a strict whitelist for adjacent private
implementation and verification support. Explicit prohibited material paths
remain prohibited.

### Material

Stop before implementation when correctness requires:

- changing a public, wire, generated, or persisted contract;
- adding or changing a migration or destructive data behavior;
- adding a dependency or Cargo feature;
- changing authentication, authorization, tenancy, policy, audit, secret, or
  data-loss semantics;
- redesigning ownership or dependency direction;
- changing requirements or acceptance outcomes.

Report repository evidence, why no in-scope solution remains, and the exact
authority required to resume. Return `SPEC_REVISION_REQUIRED` when approved
behavior needs clarification or correctness requires a new material decision.

## Execution record

The execution record may append or correct:

- current repository facts;
- internal paths and private symbol names;
- existing or added private implementation/test-support seams;
- equivalent verification commands;
- progress, failures, and evidence.

Update the existing acceptance entries with proof references and results.
Include actual assertion locations, focused commands, observed RED failures,
GREEN results, and material limitations. Record shared commands once.
Add a short note for a material failure, bounded
correction or deviation: affected criterion, repository evidence, action and
verification result. Do not narrate routine implementation or repeat the
contract, file list or test inventory already visible in the task and diff.

Do not rewrite the objective, requirement or decision IDs, required behavior,
public or persisted contracts, security semantics, material architecture or
ownership boundaries, prohibited material boundaries, or acceptance criteria.
Stop for a material decision when one must change.

## Failure routing

Use this order:

1. If the diff caused the failure, fix it and rerun.
2. If the defect is small and in touched code, fix and report it.
3. If the command or filter is defective, establish equivalent proof and
   rerun.
4. If private plumbing or test support is missing, add the smallest in-owner
   support and continue.
5. If local repository setup is missing, provision it and rerun.
6. If the failure is outside the diff, diagnose and fix it at its owner. A
   failure you did not cause is still yours; never spend effort proving it
   predates the change.
7. If approved behavior needs clarification or a material change is required,
   finish unaffected work and return `SPEC_REVISION_REQUIRED`.
8. If necessary access or external authority is unavailable, or contradictory
   task mechanics prevent an in-scope solution under settled approved behavior,
   finish unaffected work and return `BLOCKED` with the precise impediment.
9. Otherwise continue localization and diagnosis.

Compiler, test, formatter, Clippy, rustdoc, process, or tool failures are
diagnostic results. They do not terminate execution by category.

Do not return partial work, create a remediation plan, or wait for another
prompt while an in-scope recovery path remains.

## Repository-managed environments

Missing local services and local test variables are reversible mechanics when
the repository defines them.

Inspect:

- the relevant `mise.toml` task body;
- its `depends` setup and migration tasks;
- its checked-in `env` values;
- invoked scripts and documented local emulators.

Then start the service, run migrations, invoke the owning task, or supply the
same checked-in local-only values to an equivalent focused command.

For example, a focused Postgres test must not stop merely because
`WYRD_DATABASE_URL` is absent from the current shell when its canonical `mise`
task defines the value and depends on Postgres setup.

Never invent or expose production secrets. An external credential, protected
service, or user-only permission is unavailable authority only when the
repository provides no local substitute and required proof cannot be
established otherwise.

## Equivalent verification

A corrected command is equivalent only when it:

- proves the same acceptance criterion;
- exercises the same relevant code, feature, and target;
- preserves required negative, integration, and environment behavior;
- is no weaker in assertions or dependency coverage;
- records why the original recipe was defective.

Examples:

- add `--lib` when an unrelated explicit integration target is invalid under
  the intended default-feature proof;
- correct a stale test filter to the current test name;
- invoke the canonical `mise` task that supplies Postgres setup instead of a
  raw Cargo command missing its environment.

Do not call a unit test equivalent to a required integration or user-journey
test. Do not silently drop features, assertions, negative flows, or consumers.

## Implementation and test integrity

Follow the root and nested `AGENTS.md` files for the touched code. In
particular:

- preserve operation order, validation, transactions, concurrency,
  cancellation, side effects, errors, and invariants;
- avoid speculative abstractions, dependencies, features, and cleanup;
- execute behavioral scenarios through the Red-Green-Refactor loop defined in
  [spec-driven development](spec-driven-development.md#tdd-execution), one
  scenario at a time;
- map every test to an acceptance criterion.

Never pass a check by:

- removing or weakening assertions;
- ignoring, disabling, or deleting a required test;
- adding sleeps instead of deterministic synchronization;
- mocking away required behavior;
- swallowing errors or changing semantics to fit an incorrect test;
- adding an unjustified lint allowance;
- editing generated artifacts instead of their source.

An actual conflict between a required test and an approved material contract
is a material blocker.

## Focused verification

Run the smallest complete affected surface sequentially:

1. each named narrow test or reproduction through its exact command
   ([named tests](../TESTING.md#named-tests));
2. broader affected owner or package task;
3. applicable integration, contract, codegen, typing, migration, docs, or
   boundary checks;
4. repository-required format and lint checks;
5. `git diff --check`;
6. final diff inspection.

Lane selection, the `gate` aggregate rule, feature selection, and sequential
Cargo use are in [`TESTING.md`](../TESTING.md#which-lane-do-i-run).

A red lane blocks completion whoever turned it red. If its fix needs a
material change, return `SPEC_REVISION_REQUIRED` with the diagnosis after every
unaffected item is complete. Use `BLOCKED` only for unavailable access or
authority, or contradictory task mechanics under settled approved behavior.

## Final diff audit

Inspect tracked and untracked changes. For every changed file, confirm:

- the requirement or verification need that requires it;
- it is inside the established owner or explicitly approved scope;
- no unrelated formatting or cleanup entered the diff;
- public-contract, dependency, feature, migration, and security effects are
  unchanged unless explicitly approved;
- generated outputs came from their source;
- tests prove behavior without weakened integrity;
- no debug artifacts, real secrets, or machine-specific values remain.

Durable execution notes must follow [Execution record](#execution-record).

## Completion standard

`IMPLEMENTED` requires every acceptance criterion and required proof to pass, every
non-goal to remain excluded, no unrelated or prohibited change, sequential
verification, a clean diff audit, and recorded bounded corrections.

Implementation is not task completion. The task completes only after an
independent `$wyrd-task-review` returns `PASS`.
Return implementation evidence to the caller. Do not spawn reviewers or
acceptance checkers, invoke task/change review, or automatically hand the
candidate to a review workflow. The maintainer initiates reviews. External
read-only diagnosticians may investigate concrete implementation failures;
they return causes and fix sites, not acceptance verdicts.

`SPEC_REVISION_REQUIRED` means approved behavior needs clarification or
correctness requires a new material decision. Name the contract or boundary
affected and the exact decision needed.

`BLOCKED` requires evidence that no in-scope solution remains because necessary
access or external authority is unavailable, or contradictory task mechanics
need correction under settled approved behavior. Difficulty, elapsed
time, partial progress, context compaction, a recoverable command failure,
missing repository-managed setup, or a convenient handoff point do not qualify.

Return the status, task path, implemented outcome, behavioral verification,
TDD compliance (`PASS`, `VIOLATED`, or `NOT_APPLICABLE`), material risk or blocker,
and diff or commit reference. Link to the task's acceptance evidence rather
than reproducing it. Omit empty sections.

A TDD violation remains visible even when all behavioral criteria pass. Follow
[TDD execution](spec-driven-development.md#tdd-execution) for disclosure and
remaining work; a later mutation check does not establish test-first execution.
