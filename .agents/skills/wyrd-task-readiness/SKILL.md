---
name: wyrd-task-readiness
description: Review one or more proposed Wyrd implementation or remediation tasks for approved-spec traceability, executability, TDD quality, real dependencies, and credible verification. Use before implementation; this is not post-implementation task review.
---

# Wyrd Task Readiness

Remain read-only. Decide whether one or more proposed tasks can be implemented
without inventing or changing a material decision.

This is an independent review. A task author cannot issue the authoritative
`READY` verdict for a task it authored or materially edited in the same
planning context. Use a fresh-context reviewer when one is available. If
independent review is unavailable, return
`BLOCKED — INDEPENDENT_READINESS_REVIEW_REQUIRED`; do not self-certify.

## Establish authority

Read the approved spec revision at `changes/active/<slug>/spec.md` and every
proposed task under `changes/active/<slug>/tasks/`. Read `AGENTS.md`,
[agent rules](../../../architecture/agent-rules.md),
[spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
[implementation execution](../../../architecture/references/languages/implementation-execution.md),
and [testing workflows](../../../architecture/references/languages/testing-workflows.md).
Start at [the reference router](../../../architecture/references/README.md) and
load applicable [Wyrd design](../../../architecture/wyrd-design.md),
[Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx),
[Bifrost design](../../../architecture/bifrost-design.md), security, operations,
and focused references.

Follow CodeGraph instructions. Inspect only the owners, consumers, tests,
manifests, features, and `mise.toml` tasks needed to adjudicate a credible gap.

## Reconstruct the implementor's decision ledger

Before checking document structure, independently answer:

> What choices would an implementor still have to make before writing the
> first production change for each scenario?

Classify every choice:

- **Local coding choice** — helper names, private signatures, equivalent local
  containers or expressions, or bounded refactoring that cannot alter the
  planned architecture. This may remain open.
- **Plan-level design choice** — ownership, durable identity/schema, state
  transitions, synchronization, ordering, atomicity, failure/shutdown/crash
  recovery, dependency/API use, migration, consumer wiring, or test topology.
  Return `REVISE_TASKS`.
- **Product-level choice** — required behavior, invariant, acceptance, public
  contract, material constraint, security/tenancy rule, or required system
  boundary. Return `SPEC_REVISION_REQUIRED`.

Do not trust a task's design-closure ledger by declaration. Reconstruct it from
the approved spec, repository owners, and reachable call paths, then compare it
with the task. Descriptions such as “use an ordering boundary,” “authoritative
registry,” “or equivalent,” “appropriate owner,” “smallest exact protocol,” or
“if the API cannot support this, stop” are unresolved when they stand in for a
concrete plan-level choice.

## Review readiness

Assess:

1. **Authority** — the spec revision is explicitly approved and each task
   preserves it.
2. **Coverage** — every mapped requirement, invariant, acceptance obligation,
   consumer, generated surface, and required journey has an owner.
3. **Cohesion** — each task owns one implementable outcome; shared mutable
   seams are not split unsafely, and independently reviewable owners or
   outcomes are not bundled merely because they share a parent phase.
4. **Dependencies** — edges are real integrated prerequisites rather than
   scheduling preferences.
5. **TDD scenarios** — observable success, failure, edge, and regression
   behavior is sufficient for scenario-by-scenario Red-Green-Refactor.
6. **Command precision** — every named test has an exact repository-native
   `mise exec --` command and selector; Rust uses `cargo nextest run` with an
   explicit package, target, and exact test expression. Broader lanes use
   applicable `mise run` tasks.
7. **Evidence** — verification can detect the claimed defect at the correct
   unit, integration, journey, contract, and cross-language tiers.
8. **Decisions** — every plan-level owner, durable identity/schema, state
   transition, ordering/atomicity boundary, lifecycle failure, recovery,
   dependency/API, migration, consumer, and test-topology choice is concrete;
   no product-level decision is hidden.
9. **Remediation integrity** — remediation tasks map validated findings to the
   approved spec without broadening behavior or reviewing only a fix diff.

Do not reject a task for harmless Markdown shape, optional metadata, private
helper names, or a different reasonable implementation choice.

Return `REVISE_TASKS` when any of these are true:

- a coordination mechanism or its concrete owner is unnamed;
- durable identities, schema, or state transitions are incomplete;
- lock, fence, transaction, or atomicity lifetime is unspecified;
- publication failure, cancellation, shutdown, crash, takeover, or recovery is
  delegated to implementation;
- a dependency capability needed by the selected protocol remains to be
  discovered during implementation;
- a named test, owner, target, or topology is optional or left to choose;
- materially separate owners or evidence outcomes are bundled without a shared
  mutable seam; or
- a successor delegates acceptance through “complete the remaining original
  task” instead of owning the inherited obligations explicitly.

## Findings and verdict

A blocking finding includes the exact task location, governing authority or
spec obligation, repository evidence, implementation consequence, and the
simplest concrete task correction supported by that evidence. The correction
must preserve the approved spec, follow the current architecture's ownership
and dependency direction, and prefer the uniquely applicable existing
repository shape. It must resolve the choice rather than delegate discovery to
implementation.

Return one verdict: `READY`, `REVISE_TASKS`, `SPEC_REVISION_REQUIRED`, or
`BLOCKED`. Lead with the verdict, summarize the nine axes, and list only
blocking findings. For `READY`, state `No blocking readiness findings` and
confirm that the independently reconstructed decision ledger contains no open
plan-level choice. This review does not edit the active packet, implement code,
approve an implementation candidate, or authorize task branches.
