---
name: wyrd-design
description: Settle a Wyrd change's solution, implementation flow, material decisions, and observable success with the user, maintaining design notes for wyrd-spec and wyrd-plan. Use for exploring or resuming a design; ordinary advice and implementation belong to their existing workflows.
---

# Wyrd Design

Help the user turn an idea into the simplest evidence-backed implementation
proposal that solves the agreed problem. Maintain the design record throughout
the conversation. The user owns the decisions and decides when design is
finished. Produce a decision-complete implementation proposal for `$wyrd-spec`
to record and `$wyrd-plan` to divide into deliverable tasks. Preserve the shared
handoff agreement; do not expand scope to make the proposal seem complete.

## Start or resume

Create `changes/active/<slug>/design/notes.md` at the start of a new design,
using a short descriptive lowercase slug with internal hyphens. Reuse the
user's packet when supplied; do not attach a new idea to an unrelated active
change. Clarify an ambiguous packet before writing. Resume existing notes
without overwriting decisions or supplemental work. Do not create an empty
spec or task set, switch branches, stage, or commit.

Read `AGENTS.md` and the relevant parts of
[spec-driven development](../../../changes/spec-driven-development.md).
Load only architecture sections and directory guidance relevant to the current
question. Follow CodeGraph instructions when inspecting code. Use one
conversational agent by default; research does not require delegation or
invoking `$wyrd-advise`.

Maintain a concise current design, not a transcript. Start with this shape,
marking unknowns plainly and expanding sections only when useful:

```markdown
# <Change name>

Status: exploring

## Problem
## Desired outcome
## Constraints and non-goals
## Proposed implementation
## Evidence and rationale
## Decisions and alternatives
## Standards conflicts
## Open questions
## Handoff
```

Create supplemental research notes, diagrams, examples, or experiment results
only when they explain or substantiate this design. Link them from the notes;
do not precreate empty folders or duplicate evidence.

## Agree on the problem and outcome

Establish what happens today, who or what it affects, why it matters, and what
would be observably different after the change. Identify actual constraints and
non-goals, including the user's implementation idea when supplied.

Ask the material question that most affects the next decision. Avoid a fixed
questionnaire and questions already answered by available context. Record the
agreed problem and outcome before moving into implementation discussion; use
ordinary conversational agreement rather than a separate approval ceremony.
Keep agent assumptions and proposals distinct from user decisions. Silence is
not agreement. Revisit the problem if later evidence shows it was misframed.

## Develop the implementation proposal

Start from the user's suggested approach when one exists. Investigate it fairly
before replacing it. Otherwise inspect the existing workflow and propose the
smallest sufficient extension.

Research only questions that could change the proposal. Inspect existing
owners, callers, tests, and relevant configuration before inventing machinery.
Compare complete workflows, not just reusable leaf calls. For uncertain or
current external facts, use primary documentation or original research. Link
repository paths/symbols and primary sources near the claims they support.
Distinguish verified facts, inferences, untested hypotheses, and user choices;
never imply inspection or experimental verification that did not occur.

Explain enough for the user to assess:

- the end-to-end flow, its actual inputs and sources, what changes, what each
  part produces and consumes, and what can be reused;
- affected owners and consumers, including public surfaces or stored data;
- important success, failure, edge, and transition behavior where relevant;
- why the approach achieves the agreed outcome, its costs and limitations;
- how the connected flow will be exercised and what observation proves success;
- supporting evidence and uncertainties that could change the choice.

Algorithms, code sketches, and possible module changes are welcome when they
resolve a real question. Label suggested private mechanics separately from
explicit user constraints and expensive-to-reverse decisions. Design research
does not authorize production changes. Do not change production code or run
consequential experiments without user authorization; describe a useful
experiment when evidence is missing.

Offer alternatives only for meaningful trade-offs; do not manufacture an option
count. Develop the strongest simple proposal and explain when another approach
would win. Update the notes after material decisions, evidence, or proposal
changes rather than leaving the record until finalization.

## Keep complexity earned

For each proposed component, abstraction, dependency, configuration option, or
operational mechanism, explain which agreed requirement needs it and why reuse
or a simpler approach is insufficient. Keep this explanation in the proposal
rather than creating another gate or checklist artifact.

Prefer deletion, existing owners, standard facilities, and installed dependencies
before new machinery. Separate complexity needed by the current problem or an
explicit constraint from flexibility for a hypothetical future. Leave the latter
out unless the user chooses it. Do not invent scale, extensibility, rollout,
or configurability requirements. Simplicity must still cover agreed failure
cases, security, tenancy, durability, and other required behavior.

## Challenge standards without changing them

Repository standards describe current constraints and may themselves be a
subject of design. Do not reject a potentially better proposal solely because
it conflicts with `AGENTS.md` or architecture guidance. Identify the exact rule
and source, explain the benefit and consequences of departing from it, and
leave the decision to the user. Continue exploring rather than treating the
conflict as a reason to end the design conversation.

Record the user's disposition: keep the standard, revise it, or leave the
decision open. Do not silently edit standards, claim an exception is approved,
or implement against a proposed exception. A choice to revise a standard is
input to the later authority update, not evidence that the update happened.
Permission to question standards does not override higher-priority instructions
or execution permissions.

## Finalize and hand off

When the user asks to finalize, reconcile the notes and relevant supplements.
The agreed solution, end-to-end implementation flow, material decisions,
constraints, limits and observable success must make sense without the
conversation. Remove stale or contradictory proposals; retain rejected
approaches only when their rationale is useful.

Set `Status: finalized` when the user agrees the proposal and no unresolved
decision would materially change the solution. If such a decision remains,
name it and keep `exploring`; ending a conversation does not make an incomplete
design ready for handoff. Do not reopen settled decisions or require another
round to resolve ordinary private coding choices. Record those choices as
implementation-owned. A resumed design returns to `exploring` while revised.

Finalization makes the design ready for specification, not approved for
implementation. Return the notes path, agreed approach, connected flow and
proof, and any remaining private choices. Do not automatically invoke
`$wyrd-spec` or `$wyrd-plan`. The spec records the agreement for human approval;
planning uses both that approved contract and this design to write tasks.
