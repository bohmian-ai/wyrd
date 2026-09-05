---
name: wyrd-task-review
description: Review one immutable cumulative Wyrd task implementation against its approved spec, original task, remediation tasks, repository authority, and evidence. Use after implementation or remediation; return approval, bounded findings, or a required spec revision.
---

# Wyrd Task Review

Keep the reviewed source and active packet read-only. Verification may write
ordinary build artifacts or isolated test state. Review exactly one task's
complete immutable base-to-candidate range. After remediation, include the
original task, every remediation task, prior findings, and the cumulative
candidate; never review only the latest fix diff.

## Establish the review subject

Require repository root, unambiguous base and candidate commits, the approved
`changes/active/<slug>/spec.md` revision, the original active task, available
execution evidence, and any remediation tasks or prior findings. If the subject
changes during review, return `BLOCKED` rather than implying the new candidate
was reviewed.

Read `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
[implementation execution](../../../architecture/references/languages/implementation-execution.md),
and [testing workflows](../../../architecture/references/languages/testing-workflows.md).
Start at [the reference router](../../../architecture/references/README.md) and
load applicable [Wyrd design](../../../architecture/wyrd-design.md),
[Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx),
[Bifrost design](../../../architecture/bifrost-design.md), security, operations,
language, and domain references.

Follow CodeGraph instructions. Inspect changed owners plus only the callers,
consumers, negative and cleanup paths, generated surfaces, tests, manifests,
and `mise.toml` commands needed to evaluate the task.

## Run independent code review

Dispatch one independent read-only reviewer over the exact immutable task
subject before deciding the verdict. Give it the approved spec and complete
task authority, but not an intended verdict. The reviewer never edits,
implements, plans remediation, or changes the review subject.

Require explicit coverage of correctness, security, code quality,
maintainability, tests, developer experience, and architecture/contracts.
Audit every diff for persistence and transactions, async and concurrency,
PyO3, Vala or Bifrost, and UI or TypeScript boundaries, plus any other
high-risk boundary identified from repository authority. For each category,
record either the distinct independent specialist who reviewed it or the
source-backed reason it is not applicable. Each reviewer returns:

- a coverage ledger for changed production files, mapped obligations,
  high-risk boundaries, user-facing behavior, and applicable repository rules;
- candidate findings with exact locations, current flow, a realistic failure
  path and consequence, supporting and counterevidence, and focused closure
  verification; and
- for every clean lens, the strongest realistic counterexample attempted and
  why the candidate resisted it.

An independent reviewer runs in a separate agent context from the orchestrator
and does not validate its own candidates. A triggered specialist also runs in
a separate context from the baseline reviewer. One specialist may cover
multiple triggered lenses only when its ledger names each lens and demonstrates
the relevant source inspection; otherwise dispatch another specialist.

The task-review orchestrator independently validates every candidate against
source and authority. Reject unsupported, unreachable, preference-only,
duplicate, and unrelated pre-existing concerns; root-cause deduplicate the
remainder. Audit the coverage ledger rather than treating a clean reviewer
result as sufficient evidence. Missing required coverage blocks `APPROVE`;
return `BLOCKED` when a mandatory independent review or its coverage ledger
cannot be obtained.

## Review adversarially and validate conservatively

Try to falsify each mapped requirement, invariant, acceptance obligation, TDD
scenario, changed state transition, and claimed evidence. Check exact authority
alignment, complete behavior and consumer closure, regressions, applicable hard
rules, test effectiveness and RED-to-GREEN evidence, exact named-test commands,
broader proof, absence of hidden spec changes, and resolution of every prior
finding across the cumulative candidate.

Reuse credible current evidence. Run only the narrowest sequential checks
needed to resolve missing, contradictory, suspicious, or stale proof. A passing
command is not evidence when its assertions cannot detect the claimed defect.
Bind executable evidence to the candidate commit and record its source, exact
command, exit status, date, required setup or environment, and retained output
or result summary. For executable behavior changed by the task, `APPROVE`
cannot use `None` for the evidence snapshot. Static-only obligations instead
name their candidate-bound proof.

## Findings

Classify each reviewed concern as blocking, follow-up, or rejected. A concern
blocks approval only when it is source-validated, reachable, introduced or
left unresolved by the cumulative task candidate, and materially violates the
approved spec, task authority, repository authority, correctness, security,
durability, compatibility, or required verification. A concrete
maintainability defect blocks only when it creates a reachable risk to safe
operation or change. Optional hardening, preference, speculative cleanup, and
unrelated pre-existing debt are follow-up at most; unsupported, unreachable,
or duplicate concerns are rejected.

Every blocking finding includes a stable `FIND-<task>-<n>` ID and severity,
exact source or authority location, mapped spec and task obligations, reachable
scenario and consequence, required testable outcome, supporting and
counterevidence, a decision-complete Ponytail recommendation, and focused
closure verification. Preserve validated follow-up separately without routing
it into required remediation.

Use `CRITICAL` for an exploitable security or tenant boundary, likely data loss
or corruption, or another broad irreversible consequence; `MAJOR` for a
reachable user, agent, or operational failure of an approved obligation; and
`MODERATE` for a bounded but material in-scope defect that still makes task
approval unsafe. Severity never turns follow-up into a blocking finding.

For the Ponytail recommendation, walk the ladder in order: delete; reuse
existing repository behavior; use the standard library or native platform;
use an installed dependency; then write the minimum new code. Name the shared
root cause, production owner, exact control-flow, state, or API outcome,
existing mechanism to reuse, focused test change, and deliberate exclusions.
Do not propose a new abstraction, dependency, configuration surface,
compatibility layer, or speculative flexibility without source-validated
necessity. A recommendation is planning input, not new authority. When
multiple materially different corrections remain valid, state the unresolved
choice and its fixed constraints instead of inventing a preferred private
design. Decision-complete means the required outcome, owner, boundaries, and
closure proof are fixed; it does not require prescribing one private
implementation when several minimal corrections satisfy them.

Use `CRITICAL`, `MAJOR`, or `MODERATE` only for concrete in-scope defects.
Preference, speculative cleanup, and unrelated baseline concerns do not block.
A finding is not product authority and does not prescribe private mechanics
when several solutions satisfy the required outcome.

## Verdict

Return one verdict:

- `APPROVE` — the cumulative candidate satisfies the task and mapped spec;
- `REMEDIATE` — bounded corrections remain under the approved spec;
- `SPEC_REVISION_REQUIRED` — correction requires changed behavior or another
  material decision; or
- `BLOCKED` — the immutable subject, essential authority, mandatory independent
  review, or required coverage ledger cannot be obtained.

Use this exact user-facing structure for every verdict. Keep every heading and
field in this order; write `None` when a field has no entries.

```markdown
# <VERDICT>

## Subject
- Base: <commit>
- Candidate: <commit>
- Planning snapshot: <commit or None>
- Evidence snapshot: <commit or None>

## Verdict Basis
- <why this verdict follows from validated findings, review coverage, or a blocking condition>

## Verification
- Reused: <current recorded evidence or None>
- Rerun: <commands or None>
- Not run: <commands and limits or None>

## Findings

### <FINDING-ID> — <SEVERITY>: <title>
- Obligations: <spec and task IDs>
- Locations: <source and authority locations>
- Scenario: <reachable path>
- Consequence: <observable failure>
- Supporting evidence: <evidence>
- Counterevidence: <evidence or None>
- Ponytail recommendation: <smallest root-cause correction after walking delete, reuse, native/stdlib, installed dependency, minimum code; production owner; focused test; deliberate exclusions>
- Required outcome: <testable result>
- Closure verification: <focused tests and commands>

## Follow-up
- <FOLLOW-task-n: validated concern, exact location, evidence, and reason it does not block, or None>

## Obligation Coverage
- <compact coverage statement>

## Code Review Coverage
- Baseline lenses: <coverage and independent reviewer identity>
- Specialist trigger audit: <each sensitive category, reviewing specialist identity, or source-backed not-applicable reason>
- Changed production files: <each file mapped to inspecting reviewer and lenses>
- Obligations: <each mapped obligation and inspecting reviewer>
- High-risk boundaries and user-facing behavior: <each boundary and inspecting reviewer, or None>
- Adversarial clean evidence: <lens, attempted counterexample, and result, or None>

## Validation Ledger
- <candidate concern, blocking/follow-up/rejected classification, and source-validated reason>

## Prior Finding Closure
- <finding ID and status, or None>

## Routing
- Next skill: <$wyrd-plan, $wyrd-spec, $wyrd-change-review, or None>
- Finding IDs: <IDs or None>
```

When there are no blocking findings, replace the complete example finding
block under `Findings` with `- None`. Under `APPROVE`, keep obligation coverage
compact instead of repeating the task or spec. List only validated blocking
findings and validated follow-up.
`REMEDIATE` routes them to `$wyrd-plan`, not directly to ad hoc implementation.
`APPROVE` approves only this task candidate; it does not merge, push, deploy, or
replace final `$wyrd-change-review`. The review phase does not edit the active
packet; the caller or execution harness may record the returned verdict there.
