# Spec-Driven Development

This reference defines Wyrd's human-approved, spec-driven development workflow
and its test-driven execution discipline. It governs change specifications,
implementation tasks, task review, remediation, and final change verification.
It does not replace `AGENTS.md` or the architecture authorities.

## Authority

Apply change authority in this order:

1. current user instructions and explicitly approved outcomes;
2. `AGENTS.md`, `architecture/agent-rules.md`, and every applicable architecture
   authority;
3. the approved change specification revision;
4. ready implementation and remediation tasks derived from that revision;
5. tests that express the mapped specification obligations;
6. implementation and review evidence.

Tests and code never silently redefine an approved specification. When a task,
test, repository discovery, or implementation conflicts with a requirement,
invariant, public behavior, material constraint, or required system boundary,
stop and route the conflict through a specification revision and renewed human
approval.

## Decision ownership by workflow stage

The workflow separates product authority from implementation architecture and
local coding freedom. "Implementation detail" is not one undifferentiated
category.

| Stage | Decisions it owns |
|---|---|
| `$wyrd-spec` | Product behavior, externally observable success and failure, invariants, acceptance, material constraints, and required ownership or system boundaries. |
| `$wyrd-plan` | Concrete implementation architecture below the approved spec: owning structs/modules, durable identities and schema, state machines, synchronization and ordering, transaction/atomicity boundaries, failure/shutdown/crash recovery, dependency/API use, consumer wiring, test topology, and real task decomposition. |
| `$wyrd-implement` | Local coding choices that cannot alter the planned architecture: helper names, private signatures, equivalent local containers or expressions, and bounded refactoring inside the task's locked owners, states, and seams. |
| `$wyrd-task-readiness` | Independent proof that the proposed tasks contain every required plan-level decision and can be executed without inventing one. |

Concurrency protocols, persistent schema, durable state, ownership, lifecycle
transitions, cross-process ordering, atomicity, shutdown semantics, and crash or
takeover recovery are always plan-level decisions unless the approved spec or
architecture already fixes them. They are never ordinary implementer judgment.

Use this test at the plan/readiness boundary: if two implementors could follow
the same task and choose materially different durability, synchronization,
ownership, recovery, schema, dependency, or test-topology designs, the task is
not decision-complete.

## Change artifact lifecycle

Use one tracked, repository-local change packet while work is active. Keep the
packet with the implementation so ordinary branches, worktrees, pull requests,
and task branches all see the same approved authority without ignored files,
forced staging, or cross-branch retrieval.

The normal branch shape is:

```text
<base branch>
  `-- change/<slug>                 approved spec, tasks, integrated code
       |-- task/<slug>/TASK-001     one task branch
       |-- task/<slug>/TASK-002     another task branch
       `-- task/<slug>/TASK-001-R1  remediation branch when needed
```

Store the active packet under:

```text
changes/active/<slug>/
|-- spec.md
`-- tasks/
    |-- TASK-001-<name>.md
    `-- TASK-001-R1-<name>.md
```

The directory is tracked normally. Git staging and commits still require the
caller's authorization; no workflow step uses force-staging. After explicit
spec approval, record the approved revision before task branches fork. Each
task branch starts from the change branch and merges back into it. Dependent or
remediation branches start from the updated change branch. Branch names,
worktrees, scheduling, and merge mechanics remain caller-owned execution
choices rather than Wyrd lifecycle metadata.

Keep the complete packet present through final immutable change review. This
lets reviewers read the approved spec, tasks, and task evidence directly from
the reviewed candidate. If review requires remediation, append remediation
tasks to the same active packet and review the new immutable target.

An `APPROVE` verdict from `$wyrd-change-review` automatically invokes
`$wyrd-complete` in the same workflow turn. Completion condenses durable context
to one tracked record and removes the active packet:

```text
changes/completed/<year>/<slug>.md
```

The completion record preserves intent, shipped behavior, lasting invariants
and constraints, material decisions and rationale, approved revisions or
deviations, requirement-to-evidence closure, delivery references, and links to
the current architecture authority. It does not preserve task checklists,
Red-Green mechanics, review conversation, command transcripts, worktree state,
or agent scheduling.

Current architecture remains authoritative. Completion verifies that every
lasting contract or material design change is already reflected in its owning
architecture or product documentation; it never invents those updates during
cleanup. A missing or contradictory authority update routes to remediation.

The completed record and active-packet deletion are ordinary tracked changes
produced and validated by `$wyrd-complete`; they do not depend on squash
merging. Merge-commit and rebase workflows retain the deleted Markdown blobs in
Git history; squash workflows may not. Optimize for a clean current tree and
use the repository's chosen merge policy rather than rewriting history to
remove small text artifacts.

## Specification contract

A specification fixes intent, constraints, invariants, externally observable
behavior, and only those system boundaries that are themselves required. It
leaves ordinary implementation mechanics open.

Use stable local IDs such as `REQ-001`, `INV-001`, and `AC-001`. A proportionate
specification covers:

```yaml
---
id: SPEC-<slug>
revision: 1
status: draft
---
```

1. status and revision;
2. human intent and user value;
3. scope and non-goals;
4. definitions needed to remove ambiguity;
5. required behavior;
6. invariants and prohibited outcomes;
7. externally observable success and failure behavior;
8. security, tenancy, durability, performance, and compatibility constraints;
9. required system boundaries and public interfaces when they are part of the
   contract;
10. acceptance obligations and credible evidence classes;
11. open material decisions; and
12. revision history.

Do not prescribe private function signatures, module boundaries, data
structures, algorithms, internal APIs, persistence mechanics, or concurrency
mechanisms unless the user explicitly makes one a material constraint or an
architecture authority already fixes it.

Use `draft`, `approved`, and `superseded` status. Only explicit human approval
moves a draft to approved. A material edit increments the revision and returns
the spec to draft. An approved spec has no unresolved material decision.

## Task contract

`wyrd-plan` derives implementation tasks from an approved spec revision, from
validated task- or change-review implementation findings under that same spec,
or by reconciling an approved revision with a frozen partially implemented task
candidate and its current-state amendment. Missing or incomplete review
evidence returns to task review rather than becoming an implementation task.
Use as many tasks as cohesive ownership and real dependencies require; task
count is not a target.

Each task identifies:

```yaml
---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-<slug>
spec_revision: 1
requirements: [REQ-001, INV-001, AC-001]
depends_on: []
parent_task:
remediates: []
---
```

- task ID, kind (`implementation` or `remediation`), and status;
- approved spec ID and revision;
- mapped requirement, invariant, and acceptance IDs;
- outcome, owners, scope, non-goals, and direct dependencies;
- material implementation decisions and seams that belong below the spec;
- ordered implementation scenarios that bind each observable behavior and
  decisive RED proof to the concrete GREEN production owner, state, API,
  control-flow, ordering, atomicity, lifecycle, recovery, dependency, migration,
  and consumer decisions required to make it pass wherever those dimensions
  are material;
- the required Red-Green-Refactor execution discipline;
- exact focused commands for every specifically named test;
- broader crate, module, family, integration, codegen, and journey verification;
- completion evidence and material stop conditions; and
- for remediation, validated finding IDs and the parent task when one existing
  task owns the finding; integrated findings spanning tasks instead name the
  affected task set in scope and consumer closure.

Task planning must choose the plan-level implementation architecture listed in
the decision-ownership table. It may not weaken or reinterpret the approved
spec. A task is ready only when an implementer can execute it without making a
new product decision or a new plan-level ownership, durability, schema,
ordering, atomicity, recovery, dependency, migration, security, tenancy,
rollout, test-topology, or acceptance decision.

Test and production design happen together, scenario by scenario. The RED proof
must fail when a material selected production decision is implemented
incorrectly; a task-wide test list and a separate implementation-design section
do not establish that trace. Tasks need no mandatory design ledger, matrix,
pseudocode, or rejected-alternative inventory. Record rationale only where it
explains a material selected decision, then verify cross-scenario owner,
consumer, lifecycle, generated-surface, and evidence-tier closure.
An obligation that cannot meaningfully execute instead binds its equivalent
static or generated proof to the production source change; do not manufacture a
RED test merely to claim TDD.

Readiness does not require pseudocode or eliminate normal implementation
judgment. An implementer is expected to resolve helper names, private
signatures, equivalent local containers or expressions, routine error
propagation, stale symbol locations, and bounded refactoring from the nearest
repository precedent. Task revision is required only when implementation would
otherwise choose among materially different plan-level designs.

Task status moves through `proposed`, `ready`, `in_progress`, `review`, and
`approved`; use `superseded` when an approved spec revision or replacement task
invalidates it. Review phases remain read-only. The caller or active execution
harness records a task-review verdict in the active packet or delivery system
when that durability is needed.

## Test command precision

Use `mise run <task>` for the repository's module-, crate-, family-,
environment-, or aggregate-level lanes. Inspect `mise.toml` before selecting a
lane.

Every named test in a task must also carry its exact focused command. Run the
repository-pinned toolchain through `mise exec --`. For Rust this normally uses
`cargo nextest run`, an explicit package and target, and an exact test
expression. Examples:

```bash
mise exec -- cargo nextest run --locked -p <crate> --lib \
  -E 'test(=module::tests::test_name)'

mise exec -- cargo nextest run --locked -p <crate> --test <target> \
  -E 'test(=test_name)'
```

When the test requires repository-managed Postgres or another environment, the
exact command must include the owning setup wrapper or use the narrowest
environment-owning `mise` task. Do not invent a selector. Confirm target and
test names from source and, when needed,
`mise exec -- cargo nextest list`. Avoid positional filters that can
accidentally select no test.

Named Python and TypeScript tests likewise carry the exact repository-native
pytest or package-runner command, path, and selector plus any required setup.
Do not replace a named-test command with only a package-wide `mise run` lane.

## TDD execution

Execute one scenario at a time:

1. **RED** — add or select one test, run its exact focused command, and confirm
   it fails for the expected missing behavior rather than setup, compilation,
   or an unrelated defect.
2. **GREEN** — implement the minimum cohesive behavior that satisfies the new
   test, then run the new test and the relevant previously green tests.
3. **REFACTOR** — improve the implementation when the passing behavior exposes
   a clearer repository-native design; keep the tests green.
4. Repeat for the next scenario.
5. **VERIFY** — run the broader module-, crate-, family-, integration-, journey-,
   codegen-, format-, lint-, and boundary checks required by `AGENTS.md` for the
   touched surface.

Record the expected RED failure, GREEN result, and final verification. A
pre-existing regression test may supply RED. When executable behavior already
exists, report a verification-only or no-op result instead of manufacturing
production churn. Documentation, generated-artifact, and static obligations
may use a credible non-runtime proof; do not add meaningless tests solely to
imitate TDD.

## Review and remediation

Pre-implementation task readiness and post-implementation task correctness are
different reviews.

Task readiness reviews one or more proposed tasks and returns `READY`,
`REVISE_TASKS`, `SPEC_REVISION_REQUIRED`, or `BLOCKED`. The author of a task
cannot issue its authoritative `READY` verdict. Use a fresh-context reviewer;
when none is available, return `BLOCKED` with
`INDEPENDENT_READINESS_REVIEW_REQUIRED`.

During implementation, a task gap that leaves materially different plan-level
designs open returns `TASK_REVISION_REQUIRED` to `$wyrd-plan`. A required change
to approved behavior or material product authority returns
`SPEC_REVISION_REQUIRED` to `$wyrd-spec`. Neither verdict applies to ordinary
repository-local coding choices that preserve the task's concrete design.

Task review inspects one immutable cumulative task candidate against the
approved spec, original task, all remediation tasks, and available evidence. It
returns `APPROVE`, `REMEDIATE`, `SPEC_REVISION_REQUIRED`, or `BLOCKED`.
Post-remediation review always reassesses the original task's complete
base-to-candidate range, not only the latest fix diff.

Task review combines conformance review with independent code review. A
separate read-only reviewer covers correctness, security, code quality,
maintainability, tests, developer experience, and architecture/contracts;
sensitive changed boundaries receive applicable specialist review from an
agent distinct from the baseline reviewer. The task reviewer independently
validates and root-cause deduplicates candidate findings, audits changed-file
and obligation coverage, and requires adversarial clean evidence before
approval. The independent reviewers do not implement or plan fixes.

Every blocking finding has a stable ID, exact location, mapped spec and task
obligations, reachable scenario, concrete consequence, required outcome, and
supporting evidence. It blocks only when the cumulative candidate introduces
or leaves unresolved a reachable material violation of approved behavior,
repository authority, correctness, security, durability, compatibility, or
required verification. Optional hardening, preferences, speculative cleanup,
and unrelated pre-existing debt are non-blocking follow-up. A finding is not
new product authority.

Approval evidence includes an auditable ledger mapping every changed
production file, specification obligation, high-risk boundary, and applicable
review lens to an independent reviewer. Each clean lens records the strongest
realistic counterexample attempted. The task reviewer keeps a validation ledger
showing whether each candidate concern was blocking, follow-up, or rejected and
why. Missing mandatory independent review or coverage evidence blocks approval.

Every blocking finding carries a decision-complete Ponytail recommendation:
fix the shared root cause with the first viable option among deletion,
repository reuse, standard-library or native behavior, an installed
dependency, and minimum new code. It names the production owner, required
control-flow or state outcome, focused closure test, and deliberate exclusions;
new abstractions, dependencies, configuration, compatibility layers, and
speculative flexibility require source-validated necessity.

`wyrd-plan` converts validated task- or change-review implementation findings
into the minimum cohesive remediation tasks. Missing review evidence routes
directly to task review. If the required outcome changes the approved behavior
or a material constraint, route it to `SPEC_REVISION_REQUIRED` instead.

## Final change verification

After all tasks are approved and integrated, final review maps every required
specification obligation to the strongest applicable evidence, checks
cross-task seams and required user journeys, and inspects the complete immutable
base-to-target range. It returns `APPROVE`, `REMEDIATE`,
`SPEC_REVISION_REQUIRED`, or `BLOCKED`.

Final review verifies that every task has credible independent code-review and
validation evidence. It reuses that evidence only when the reviewed task
candidate is unchanged in the integrated target by ancestry or exact patch
equivalence; otherwise the affected cumulative task receives a new task review.
A separate independent reviewer covers integrated interactions, resolution
drift, and regressions, and the final reviewer validates its findings. Missing
review coverage routes directly to task review.

Final review applies the same blocking, follow-up, and rejected materiality
rules as task review. Confirmed cross-task implementation defects under the
approved spec route to `$wyrd-plan`; missing task-review evidence routes to
`$wyrd-task-review`; changed behavior or another material decision routes to
`$wyrd-spec`.

The review phase never edits its subject. `APPROVE` immediately hands its
validated completion payload to `$wyrd-complete`, which writes the compact
record and removes the active packet. Completion does not merge, push, deploy,
or grant product authorization.

## Primary grounding

These sources inform the workflow but do not override Wyrd authority:

- [Kent Beck, Canon TDD](https://newsletter.kentbeck.com/p/canon-tdd) — one
  scenario and one Red-Green-Refactor cycle at a time;
- [Martin Fowler, Test-Driven Development](https://martinfowler.com/bliki/TestDrivenDevelopment.html)
  — test sequencing and refactoring discipline;
- [W3C, A Method for Writing Testable Conformance Requirements](https://www.w3.org/TR/test-methodology/)
  — stable requirements and requirement-to-test traceability; and
- [GitHub Spec Kit](https://github.github.com/spec-kit/) — separation of
  specification, technical planning, tasks, and implementation;
- [GitHub Spec Kit, Spec Persistence](https://github.github.com/spec-kit/concepts/spec-persistence.html)
  — explicit separation of durable specifications from disposable derived
  plans and tasks; and
- [Michael Nygard, Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions)
  — concise durable records for material decisions and rationale.
