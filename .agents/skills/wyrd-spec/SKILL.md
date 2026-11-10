---
name: wyrd-spec
description: Define or revise a human-approved Wyrd behavioral specification from agreed intent and applicable repository constraints, leaving implementation decisions open.
---

# Wyrd Spec

Preserve the agreement in a specification the maintainer can understand without
reading code or the author conversation. Only explicit human approval makes a
revision authoritative. Do not create tasks or edit implementation code; hand
an approved specification to `$wyrd-plan`.

## Establish the agreement

Read `AGENTS.md`, [spec-driven development](../../../changes/spec-driven-development.md),
and the architecture sections and directory guidance that own the requested
change. Follow CodeGraph instructions before tracing repository behavior.
Inspect existing owners, consumers and tests only far enough to distinguish
current behavior from the agreed outcome and identify relevant constraints.

When `changes/active/<slug>/design/notes.md` exists, read it and relevant
supplements. Carry forward user decisions and distinguish them from agent
assumptions and proposals. Design finalization is not spec approval. A proposed
standards change is not enacted until the user resolves the conflict and the
required authority update is made.

Apply the shared scope discipline: every obligation needs a source in the
agreement, applicable authority, or a necessary consequence of the outcome.
Explain necessary inferences where they arise; do not create a separate ledger.
Keep optional improvements outside requirements until agreed.

Resolve contradictions only when they prevent specifying this change correctly.
Report unrelated documentation drift separately. Before proposing a new public
surface or ownership boundary, identify the missing behavior and why the
existing owner's workflow cannot provide it.

## Specify the outcome

Write `changes/active/<slug>/spec.md` on the caller's change branch. Keep spec
ID, revision, and `draft | approved | superseded` status. Default to:

- the problem and agreed outcome, with an example when useful;
- required behavior, scope and exclusions;
- applicable constraints and material decisions; and
- acceptance observations: the situation exercised and expected result.

Use stable local obligation IDs for task and review traceability. Describe each
promise once; acceptance may reference it without repeating it. Link relevant
authority rather than restating repository policy as new change requirements.
Add alternatives, detailed contracts, definitions, or decision rationale only
when they explain an actual material choice. Record material revisions briefly.

Address failure and edge behavior, security, tenancy, durability, performance,
compatibility, concurrency and persistent formats only where the change affects
them. Existing safeguards remain mandatory; touching an area does not authorize
new guarantees for it.

Resolve decisions that materially change the agreed outcome, public or
persisted contract, required ownership, or safety boundary. Leave ordinary
implementation decisions open, including private functions, modules, algorithms,
available dependency APIs and test fixtures, unless the user or architecture
explicitly makes a mechanism part of the contract.

Stop elaborating when the agreed outcome, affected owner, material constraints
and credible verification are clear. A conceivable failure mode alone does not
authorize another requirement.

## Approve and hand off

Present unresolved material decisions with their consequences. Move a revision
to `approved` only after explicit human approval. A later material edit
increments the revision and returns it to `draft`; stop affected implementation
until that revision is approved. Git staging and commits require caller
authorization.

Complete the shared independent comprehension pass before presenting the draft.
The pass checks explanations and scope fidelity; it grants no approval or
implementation-readiness verdict.

Return the path, revision and status with a short summary of what changes, why,
how it will be verified, and any exact approval or blocking decision. Disclose
remaining comprehension-review limitations.
