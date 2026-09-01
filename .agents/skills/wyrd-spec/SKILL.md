---
name: wyrd-spec
description: Design, refine, or revise a human-approved Wyrd change specification that fixes intent, constraints, invariants, and externally observable behavior while preserving implementation freedom. Use before task planning for a material Wyrd change; this is not the wyrd-spec Rust crate workflow.
---

# Wyrd Spec

Turn human intent into a decision-complete behavioral specification. The human
and agent own design reasoning together; the agent may draft and research, but
only explicit human approval makes a revision authoritative.

Do not create implementation tasks or edit implementation code under this
skill. Route approved decomposition to `$wyrd-plan`.

## Establish design authority

Read `AGENTS.md` and [agent rules](../../../architecture/agent-rules.md). Read
[Wyrd design](../../../architecture/wyrd-design.md) and
[Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx) for governed behavior,
contracts, ownership, and public or internal surfaces. Read
[Bifrost design](../../../architecture/bifrost-design.md) whenever the change
touches Scribe, Oracle, Forge, analytical storage, ingest, query, maintenance,
or reliability.

Start at [the reference router](../../../architecture/references/README.md),
read [spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
and completely read the smallest applicable architecture, language, domain,
security, and operations references. Follow CodeGraph instructions before
locating or tracing repository behavior. Inspect the owning implementation,
tests, manifests, consumers, and `mise.toml` only far enough to distinguish
current facts from desired behavior.

Repository authority constrains the spec; implementation drift does not.
Record links that materially informed a requirement or decision. Do not attach
irrelevant references merely to populate a section.

## Preserve behavioral constraint and design freedom

Specify what the system must accomplish, what must always or never be true,
externally observable success and failure behavior, material constraints,
required public interfaces and cross-boundary flow, and system boundaries that
architecture or the user intentionally locks.

Leave ordinary implementation mechanics to tasks: private signatures, module
boundaries, data structures, algorithms, internal APIs, persistence mechanics,
and concurrency mechanisms. Include one only when it is itself an approved
constraint or an architecture authority already fixes it.

Challenge ambiguous architecture language. “Persist separately” is an
implementation choice unless independent persistence is required for a named
durability, security, ownership, or externally observable invariant.

## Draft the specification

For an active change, write `.dev/changes/<slug>/spec.md` on the caller's
change/integration branch. `.dev/` is normally ignored; track only the active
change subtree when the caller authorizes staging or a commit. Do not create a
permanent archive on the main branch.

Use proportionate Markdown with stable local IDs:

1. metadata: spec ID, revision, and `draft | approved | superseded` status;
2. human intent and user value;
3. scope and non-goals;
4. definitions;
5. `REQ-*` required behavior;
6. `INV-*` invariants and prohibited outcomes;
7. externally observable behavior and failure modes;
8. material constraints;
9. required system boundaries, public interfaces, and cross-boundary flow;
10. `AC-*` acceptance obligations and credible evidence classes;
11. open material decisions; and
12. revision history and materially relevant authority links.

Use exact Wyrd vocabulary. Requirements and invariants must be atomic enough to
map to tasks and evidence, but do not turn the spec into a test inventory or
implementation plan. An approved spec has no unresolved material decision.

## Refine and approve

Present unresolved material decisions and their consequences one at a time.
Revise the draft from explicit human direction. Do not infer approval from
silence, implementation activity, or phrases that do not clearly accept the
specification.

On explicit approval, set the current revision to `approved`, record the
approval in revision history, and recommend recording that revision in a
commit before task branches fork. Make no further material edit without
returning the spec to `draft` and incrementing its revision. Git staging and
commits require caller authorization.

## Handle implementation-discovered conflicts

When implementation or review proves the design wrong, create a new draft
revision. State the conflicting requirement or invariant, repository evidence,
affected tasks and tests, and the smallest proposed semantic change. Stop
downstream work until the human explicitly approves the new revision and
`$wyrd-plan` reconciles affected tasks.

## Handoff

Report the spec path, revision, status, locked behavioral obligations, retained
implementation freedoms, selected authority links, and any exact approval or
blocking decision needed. Never claim task or implementation readiness merely
because the spec is approved.

