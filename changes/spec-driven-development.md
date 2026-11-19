# Spec-Driven Development

This file defines Wyrd's human-approved, spec-driven development workflow and
its test-driven execution discipline: change specifications, implementation
tasks, task review, remediation, and final change verification. Execution
discipline for one task is in
[implementation execution](implementation-execution.md).

## Design to observable proof

The normal authoring flow is `$wyrd-design` → `$wyrd-spec` → `$wyrd-plan` →
implementation and observable proof. Use the existing Markdown packet and
approval process.

| Skill | Predecessor | Responsibility and handoff |
|---|---|---|
| `wyrd-design` | User intent and repository evidence | Settle the solution, implementation flow, material decisions and observable success in finalized notes and useful supplements. |
| `wyrd-spec` | Finalized design and supplements | Record the agreement as a concise behavioral contract for human approval. |
| `wyrd-plan` | Approved spec and finalized design | Carry the agreed approach into cohesive tasks with real inputs, outputs, dependencies and proof. |

Each stage preserves its predecessor's agreement and adds only the detail
needed for its responsibility. Surface missing material decisions in the
design conversation instead of silently supplying them. If the approved
contract changes, revise the spec and obtain renewed approval before affected
work proceeds. Ordinary private coding choices remain implementation-owned.

The approved spec defines each behavioral requirement once. Tasks reference
those requirements and define scenarios that exercise them; implementation
adds execution evidence to those scenarios. Review independently checks the
requirements, scenario coverage, implementation, and evidence. Neither a task,
a test, an implementation note, nor a review finding creates product authority.
The maintainer initiates task and change reviews; implementation returns its
result to the caller without launching a reviewer or an acceptance checker.

Existing approved packets remain authoritative; do not manufacture retroactive
design artifacts. Bounded direct instructions remain valid under
[implementation execution](implementation-execution.md#change-contract).

## Authority

Apply instructions in this order, for spec-driven changes and bounded direct
instructions alike:

1. current user instructions and explicitly approved outcomes;
2. applicable `AGENTS.md` files and every applicable architecture authority;
3. the approved change specification revision, when the work belongs to one;
4. ready implementation and remediation tasks derived from that revision;
5. tests that express the mapped specification obligations;
6. repository conventions consistent with the authorities above;
7. local implementation preferences.

Tests and code never silently redefine an approved specification. When a task,
test, repository discovery, or implementation conflicts with a requirement,
invariant, public behavior, material constraint, or required system boundary,
stop and route the conflict through a specification revision and renewed human
approval.

## Scope discipline

Every requirement must come from an explicit user decision, an applicable
repository authority, or behavior strictly necessary to make the agreed
outcome correct. Explain necessary inferences where they arise; no separate
provenance ledger is required. A useful improvement or conceivable failure
mode alone does not authorize an obligation. Keep optional proposals outside
requirements until agreed.

Resolve inferred behavioral promises during specification, before approval.
Planning and implementation may derive scenarios needed to prove an existing
promise, but may not infer additional promises from an outcome label. Cite
the existing requirement or governing authority for a discovered coverage gap.

Planning may decompose approved obligations and identify their necessary work;
it may not add guarantees, supported cases, interfaces, operational features or
acceptance obligations. Newly discovered material needs return to design, then
specification and human approval when the approved contract must change.
Existing repository safeguards remain mandatory. Reference
their relevant consequences without restating policy as new change requirements.

Resolve decisions that materially change the agreed outcome, public or
persisted contract, required ownership, or safety boundary. Ordinary reversible
implementation decisions remain implementation-owned. Resolve repository
contradictions only when they prevent specifying or delivering this change
correctly; report unrelated documentation drift separately.

Stop elaborating when the agreed outcome, affected owner, material constraints
and credible verification are clear. Do not fill categories, templates or
scenario lists with additional scope.

### Ambiguous wording

Consult the approved spec and owning authority first. Resolve reversible
private coding choices autonomously. If reasonable interpretations change
observable behavior, required coverage, or a material boundary, report the
exact wording, competing interpretations, and smallest decision needed to the
caller; continue unaffected work. Correct unclear task wording through
`wyrd-plan` when the approved behavior is settled. An unresolved ambiguity in
approved behavior requires `SPEC_REVISION_REQUIRED` and explicit clarification
and approval through `wyrd-spec`. Repair wording at its source; do not preserve
a competing interpretation in an implementation ledger or review finding.

## Write for maintainer approval

Specs and tasks serve two readers: the maintainer deciding what to approve and
the agent executing the approved work. A maintainer must understand the change,
its limits and its proof without reading the conversation or implementation.
Execution completeness and maintainer comprehension are both required.

Lead with the problem and proposed behavior. Use one concrete before/after
example when it makes the change easier to understand. Explain the approach,
why it was chosen, and its material costs before introducing detailed contracts,
source symbols or commands. For an internal change, explain the effect on the
system or its contributors; do not invent a user story for mechanical work.

- Name the actor and action: who reads, writes, refuses, retries or releases
  something. Define specialist terms when first needed, keeping the precise
  domain name once explained.
- Give each requirement or invariant one principal promise. State the failure
  it prevents. Separate independent promises rather than packing migration,
  concurrency, tenancy and cleanup into one long obligation.
- Explain verification as an observation: what situation is exercised, what
  must happen, and what result would show a violation. A command or test name
  alone does not explain the proof.
- Describe the situations needed to establish the agreed behavior. Group
  equivalent cases when useful; do not expand the supported cases while
  elaborating acceptance or repeat the same promise as several scenarios.
- Keep IDs in metadata and one requirement-to-proof mapping. Use descriptive
  names in prose; an acceptance criterion must explain success without making
  the reader chase IDs through several sections.
- Put useful source references and verification commands after the behavioral
  explanation. Record them once. Implementation discovers private mechanics
  and records actual execution evidence; planning need not predict them.

A compact proof table can connect the decision to its evidence:

| Rule | Failure prevented | Verification and expected result |
|---|---|---|
| Replaying an acknowledged batch does not insert its rows again. | A transport retry duplicates evidence. | Submit the same batch twice through the real client; the second acknowledgement succeeds and the query returns the original row count. |

This is an illustrative row, not a new product requirement. Technical detail
is welcome when it explains a decision, boundary or failure; readability does
not justify removing constraints, weakening proof or prescribing private code.

## Independent comprehension pass

Before handing a new or materially revised spec/task packet to the maintainer,
obtain one bounded, read-only comprehension pass. Review the current spec and
all proposed tasks together when tasks exist; a spec-only handoff reviews only
the spec. Reuse a reader in an already-running document/readiness review when
it can perform this pass independently. Otherwise use one independent reader;
do not create a reviewer per task or another approval workflow. Recheck changed
explanations and their dependencies without rereviewing unchanged material.

Give the reader the packet without the author conversation, code or intended
interpretation. Ask it to explain, citing the relevant sections:

1. What problem is fixed, and what behavior changes?
2. Why this approach, and what costs or limits remain?
3. What must never happen, and why does each important invariant matter?
4. What does each important proof exercise and assert? What would fail?
5. What exactly is the maintainer being asked to approve or decide?

The reader reports missing explanations, ambiguous interpretations,
contradictions and obligations without a source in the agreement or applicable
authority. Repairs clarify or remove unsupported scope; new material needs
follow spec approval. The reader does not redesign the system, run verification
or reject wording for subjective style preferences. The author repairs the documents
before handoff and checks that the repair preserves their technical meaning.
Meaning-changing discoveries follow the existing spec approval rules.

This pass adds no readiness status, durable review artifact or human approval
round. Report any remaining comprehension gap in the existing handoff. If an
independent reader is unavailable, disclose that limitation and perform the
same author check; never claim independence for a self-review.

## Change artifact lifecycle

A `$wyrd-design` conversation normally starts the packet at
`changes/active/<slug>/design/notes.md`, with supplemental files only as needed.
It records the problem, agreed solution, end-to-end implementation flow,
evidence, material decisions and observable success before a spec exists.
Design status is `exploring` or `finalized`. Finalize when the user agrees the
proposal and no unresolved decision would materially change the solution.
Stopping a conversation with such a decision open leaves it `exploring`;
private coding choices need not be settled. Finalization makes the design ready
for specification, not approved for implementation. Design may propose changes
to repository standards, recording the exact conflict and the user's decision
without silently changing authority. Required authority updates must be
resolved before spec approval.

`wyrd-spec` consumes the finalized notes and relevant supplements to record the
agreement without redesigning it. `wyrd-plan` reads both the approved spec and
the design to decompose the selected approach into tasks. Distinguish explicit
decisions from illustrative sketches and private suggestions. Revalidate claims
only where they matter to the current handoff; missing material decisions
return to design. The approved spec remains the behavioral contract.

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
|-- design/                    agreed solution, flow and supporting evidence
|   `-- notes.md
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

A `PASS` verdict from `$wyrd-change-review` automatically invokes
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

When a packet contains design notes or supplements, completion also preserves
lasting rationale consistent with the approved spec and shipped behavior.
Exploratory proposals are not shipped facts. Condense useful context into the
completed record and avoid links to design files that will be deleted with the
active packet.

Current architecture remains authoritative. Completion verifies that every
lasting contract or material design change is already reflected in its owning
architecture or product documentation; it never invents those updates during
cleanup. A missing or contradictory authority update routes to remediation.

The completed record and active-packet deletion are ordinary reviewed changes
and do not depend on squash merging. Merge-commit and rebase workflows retain
the deleted Markdown blobs in Git history; squash workflows may not. Optimize
for a clean current tree and use the repository's chosen merge policy rather
than rewriting history to remove small text artifacts.

## Specification contract

A specification records the finalized design's intent, constraints, material
decisions, externally observable behavior and required system boundaries.
Preserve settled implementation constraints without promoting illustrative
private suggestions into requirements. Link the design and relevant supplements
and summarize the agreed flow; keep supporting research in the design. Missing
material decisions return to design rather than being invented during conversion.
Ordinary implementation mechanics remain open.

Use stable local obligation IDs such as `REQ-001`, `INV-001`, and `AC-001`
for traceability; do not require separate families for the same promise. Keep
spec ID, revision and status:

```yaml
---
id: SPEC-<slug>
revision: 1
status: draft
---
```

Default to the problem and agreed outcome, required behavior and exclusions,
applicable constraints and material decisions, and acceptance observations.
Describe each promise once. Add definitions, alternatives, detailed contracts
or rationale only when they explain an actual material choice. Record material
revisions briefly and identify unresolved material decisions in drafts.

For each behavioral requirement, state the relevant starting conditions, actor
or trigger, and observable result or state transition. Include failure behavior
and prohibited side effects where they are part of the agreement. Define
material terms such as completion or visibility; do not substitute an outcome
label such as "supports retries" for a promise a scenario can falsify.
Acceptance observations reference requirements rather than redefining them.

Address failure and edge behavior, security, tenancy, durability, performance,
compatibility, concurrency and persistent formats only where the change affects
them. Touching an area does not authorize new guarantees for it. Use the
maintainer-approval guidance above without mandatory boilerplate headings.

Do not prescribe private function signatures, module boundaries, data
structures, algorithms, internal APIs, persistence mechanics, or concurrency
mechanisms unless the user explicitly makes one a material constraint or an
architecture authority already fixes it.

Use `draft`, `approved`, and `superseded` status. Only explicit human approval
moves a draft to approved. A material edit increments the revision and returns
the spec to draft. An approved spec has no unresolved material decision as
defined in Scope discipline; it need not settle ordinary implementation choices.

## Task contract

`wyrd-plan` derives implementation tasks from an approved spec revision and
the finalized design, carrying forward the selected implementation approach.
`wyrd-task-review` packages validated review findings into remediation tasks
under that same spec. Default to one task; split only for real dependencies,
distinct behavioral owners or independently deliverable outcomes. Include each
outcome's necessary consumer and test updates rather than creating mechanical
subtasks.

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
- outcome, scope, existing owner and missing behavior;
- the agreed approach and relevant design references;
- inputs and their actual sources, outputs and their consumers;
- an ordered acceptance scenario list mapped once to approved obligations;
- credible verification at the required test tier, with the test owner and
  existing runner; exact commands for existing tests named in the task;
- applicable owner lanes, boundary and format/lint checks;
- relevant exclusions and any real dependencies or material blockers; and
- for remediation, the parent task and validated finding IDs.

Each task implements an agreed part of the flow: input and source → agreed
change → output and consumer → observable proof. Inputs come from existing
repository behavior or an identified predecessor task's output. Explain which
output a dependency supplies; do not add placeholder tasks for existing
capabilities or fabricate inputs to hide a missing connection.

Use the smallest readable structure and enough of the agreed approach to
execute without rediscovering the solution. Order behavioral acceptance
scenarios for test-first implementation, beginning with the smallest observable
behavior and then adding required variants and failure cases. Each scenario
references its approved requirement or governing authority and states:

- starting state and real input, with its source;
- actor and action or trigger;
- expected result or state transition, and required absence of side effects;
- the test owner, tier, and actual consumer path exercised; and
- the missing behavior that should make the test fail before production changes.

Include these details only as needed to make the scenario unambiguous. A
scenario is one test-first cycle, not a separate task or new requirement.
Leave exact new test names, private mechanics and useful refactoring to
implementation. Static or already-correct behavior needs appropriate proof,
not manufactured tests.

Maintain one acceptance list through planning and implementation. Implementers
add proof references and results to its existing entries, preserving required
cases without repeating the contract in an obligations ledger or a second
evidence table. Older packets may use their existing proof rows or bullets
without a rewrite. Record shared proof and commands once and reference them
from the entries they cover.

The scenario's expected behavior remains fixed while its evidence is added.
Record the actual assertion location, focused command, observed RED failure,
GREEN result, and material limitations. If an existing requirement lacks a
necessary scenario, add the scenario with that requirement reference; do not
invent a requirement ID or rewrite the promise to match the implementation.

Proof exercises actual inputs and demonstrates the promised outputs. At
connected boundaries it establishes that the downstream consumer uses the
predecessor's output through the agreed path. Preserve the end-to-end
acceptance journey; disconnected fixtures alone do not prove the delivered flow.

A task is ready when it covers its approved obligations and an implementer can
proceed without a new material decision as defined in Scope discipline. It may
not weaken, reinterpret or enlarge the approved spec. Preserve required
journeys and shipped language surfaces; full journey lanes and required broad
aggregates run on the integrated candidate at change review.

Task status moves through `proposed`, `ready`, `in_progress`, `review`, and
`complete`; use `superseded` when an approved spec revision or replacement task
invalidates it. Reviews keep the candidate source, requirements, and scenario
expectations immutable; reviewers may write reports and remediation tasks in
the review directory. The caller or active execution harness records the
task-review verdict in the active packet or delivery system when needed.

## Reuse evidence and task conflicts

Identify the existing owner and missing behavior in each task; keep routine
references brief. When proposing new machinery or a changed ownership boundary,
compare the existing owner's complete workflow and explain why extending it
cannot appropriately satisfy the outcome. Leaf-call reuse inside duplicate
orchestration is insufficient. Private helper choices are not a symbol or file
allowlist, and routine changes need no capability-by-capability research table.

Implementers revalidate relevant reuse evidence before adding machinery; a
routine task need not contain a separate reuse map. Reviews independently
compare actual new machinery with existing owners even when a task prescribed
it. Correct private task conflicts through `wyrd-plan` under the same approved
spec; route material behavior or boundary changes through `wyrd-spec` and human
approval. Task compliance cannot waive stronger repository authority. Existing
review roles perform this comparison; no additional controller or check is
required.

## Test command precision

Every existing test named in a task carries its exact focused command, as
defined in [`TESTING.md`](../TESTING.md#named-tests). For tests that do not exist,
planning identifies the test owner, tier and existing runner without inventing
names or selectors. Implementation records actual focused commands after
adding tests, before RED. A package-wide lane never replaces executing a named
test through its focused command.

## TDD execution

Execute one scenario at a time:

1. **RED** — add or select one test, run its exact focused command, and confirm
   it fails for the expected missing behavior rather than setup, compilation,
   or an unrelated defect. Record the exact command and expected failure before
   writing production code for that scenario.
2. **GREEN** — implement the minimum cohesive behavior that satisfies the new
   test, then run the new test and the relevant previously green tests.
3. **REFACTOR** — improve the implementation when the passing behavior exposes
   a clearer repository-native design; keep the tests green.
4. Repeat for the next scenario.
5. **VERIFY** — run the broader module-, crate-, family-, integration-, journey-,
   codegen-, format-, lint-, and boundary checks required by `AGENTS.md` for the
   touched surface.

Record the expected RED failure, GREEN result, and final verification against
the existing acceptance entries. A pre-existing regression test may supply RED.
When executable behavior already
exists, report a verification-only or no-op result instead of manufacturing
production churn. Documentation, generated-artifact, and static obligations
may use a credible non-runtime proof; do not add meaningless tests solely to
imitate TDD.

Report behavioral verification separately from TDD compliance (`PASS`,
`VIOLATED`, or `NOT_APPLICABLE`). Production written before its failing test is
a process violation; disclose the affected scenarios and resume test-first work
for the remaining scenarios. Later mutation checks may prove assertion
strength, but cannot establish test-first execution. Do not undo correct code
or manufacture historical RED evidence. Verified behavior does not erase the
violation; preserve it in the implementation handoff and task-review verdict.

## Review and remediation

The maintainer initiates review, directly or through an explicit instruction to
an orchestrator. Implementers and reviewers return results to the caller;
neither automatically launches the next implementation or review phase.

Pre-implementation task readiness and post-implementation task correctness are
different reviews.

Task readiness reviews one or more proposed tasks and returns `READY`,
`REVISE_TASKS`, `SPEC_REVISION_REQUIRED`, or `BLOCKED`.

Task review inspects one immutable cumulative task candidate against the
approved spec, original task, all remediation tasks, and available evidence. It
returns `PASS`, `FIX_REQUIRED`, `SPEC_REVISION_REQUIRED`, or `BLOCKED`.
Check assigned spec requirements independently of the task's scenario list,
then check scenario coverage, implementation correctness, and evidence strength
separately. Passing planned tests cannot excuse an omitted requirement.
Keep one primary reviewer and an independent coverage/finding validator for
the first review, with specialists for affected boundaries. Focus remediation
review on closure and affected paths; preserve independent validation of any
new finding. See `wyrd-task-review` for delegation and report details.
The first review covers the complete base-to-candidate range. Post-remediation
review checks finding closure, the latest fix diff, and affected paths against
the original task, retaining the cumulative candidate as context. Carry forward
source-backed evidence for unchanged behavior unless changed dependencies,
authority, or new evidence invalidate it; record why any area is reopened.

A blocking finding requires an explicit requirement, governing authority, or
supported-use regression, a concrete scenario, and a material consequence, or
an explicit mandatory repository-gate violation. Distinguish a source-proven
implementation defect from an evidence gap: missing proof does not establish
that the code is wrong, but required behavior without credible proof cannot
pass. A contract ambiguity requires clarification, not the reviewer's chosen
semantics. Reachability alone is insufficient.
Corrections must justify their implementation and maintenance burden against
deletion, reuse, and smaller alternatives without weakening required safety
boundaries. After two unsuccessful remediation reviews, assess convergence
and set the next bounded scope; do not restart the full audit automatically
or grant an automatic PASS.

Every actionable finding has a stable ID, exact location, mapped requirement
or authority, starting conditions and trigger, expected behavior, actual
behavior or missing proof, concrete consequence, and supporting evidence.
A review matrix records the verdict against existing requirement and scenario
references; it is not another requirements ledger. Behavioral claims require
appropriate executable evidence; source inspection may establish structural
constraints. A finding is not new product authority.

`wyrd-task-review` packages validated `FIX_REQUIRED` findings into the minimum
cohesive remediation task and returns it to the caller for `wyrd-implement`.
An implementation defect needs a correction at its owner; an evidence gap may
need only a test or valid execution evidence. If the required outcome changes
approved behavior or a material constraint, or that behavior is ambiguous,
return `SPEC_REVISION_REQUIRED` instead. Never prescribe production churn or
historical RED evidence to repair a TDD execution-order violation.

## Final change verification

After all tasks are approved and integrated, final review maps every required
specification obligation to the strongest applicable evidence, checks
cross-task seams and required user journeys, and inspects the complete immutable
base-to-target range. It returns `PASS`, `FIX_REQUIRED`,
`SPEC_REVISION_REQUIRED`, or `BLOCKED`.

The review phase never edits its subject. `PASS` immediately hands its
validated completion payload to `$wyrd-complete`, which writes the compact
record and removes the active packet. Completion does not merge, push, deploy,
or grant product authorization.

## Primary grounding

These sources inform the workflow but do not override Wyrd authority:

- [Rust RFC template](https://github.com/rust-lang/rfcs/blob/main/0000-template.md)
  — summary, motivation and guide-level examples before reference-level detail;
- [Kubernetes KEP template](https://github.com/kubernetes/enhancements/blob/master/keps/NNNN-kep-template/README.md)
  — reader-facing proposal and goals, separate design details, test plan and
  production-readiness questions;
- [GitHub Spec Kit specification template](https://github.com/github/spec-kit/blob/main/templates/spec-template.md)
  — plain-language scenarios, independently testable outcomes and measurable
  success criteria; Wyrd retains its own mandatory TDD and journey rules;
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
