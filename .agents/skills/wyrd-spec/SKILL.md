---
name: wyrd-spec
description: Define or revise a human-approved Wyrd change specification covering intent, observable behavior, constraints, and expensive-to-reverse decisions while preserving implementation freedom.
---

# Wyrd Spec

Turn human intent into a decision-complete behavioral specification. The agent
may draft and research, but only explicit human approval makes a revision
authoritative. Do not create implementation tasks or edit implementation code;
hand an approved specification to `$wyrd-plan`.

## Establish authority

Read `AGENTS.md`, [agent rules](../../../architecture/agent-rules.md),
[Wyrd design](../../../architecture/wyrd-design.md), and
[Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx). Read
[Bifrost design](../../../architecture/bifrost-design.md) when the change touches
Bifrost. Start at [the reference router](../../../architecture/references/README.md)
and completely read only the applicable references. Follow CodeGraph
instructions before tracing repository behavior.

Repository authority constrains the specification; implementation drift does
not. Inspect existing owners, consumers, tests, manifests, and verification only
far enough to distinguish current behavior from the requested outcome.

## Specify decisions, not code

Fix:

- human intent and user or operator value;
- required observable success, failure, and edge behavior;
- scope, non-goals, and invariants;
- security, tenancy, durability, performance, and compatibility constraints;
- required public interfaces and cross-boundary behavior; and
- decisions expensive to reverse, including public or persisted contracts,
  architectural ownership, cross-service semantics, concurrency guarantees,
  security assumptions, compatibility, and persistent data formats.

Do not prescribe helper functions, private methods, module layout, variable
names, local control flow, dependency APIs already available inside approved
boundaries, test fixture structure, or other reversible choices. Include an
implementation mechanism only when the user or architecture makes that
mechanism part of the contract.

## Write and approve the specification

For an active change, write `changes/active/<slug>/spec.md` on the caller's
change branch. Use proportionate Markdown containing:

1. spec ID, revision, and `draft | approved | superseded` status;
2. objective and user value;
3. requirements and externally observable behavior;
4. constraints, invariants, scope, and non-goals;
5. expensive-to-reverse decisions and required boundaries;
6. acceptance criteria and credible evidence classes;
7. open material decisions; and
8. revision history and materially relevant authority links.

Use stable `REQ-*`, `INV-*`, and `AC-*` IDs where traceability helps. Keep them
atomic enough to map to tasks and evidence without turning the specification
into a test inventory or implementation plan. An approved revision has no open
product, public API, architecture, security, compatibility, concurrency,
cross-service, or persistent-data decision.

Present unresolved material decisions and consequences one at a time. Move a
revision to `approved` only after explicit human approval. A later material edit
increments the revision and returns it to `draft`. Git staging and commits still
require caller authorization.

If implementation or review proves an approved decision wrong, draft the
smallest semantic revision, cite the conflicting evidence, and stop affected
work until the human approves it.

## Handoff

Return the spec path, revision, status, one-sentence outcome, and any exact
approval or blocking decision. Never claim implementation readiness merely
because the specification is approved.
