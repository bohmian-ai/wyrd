---
name: wyrd-task-readiness
description: Review one or more proposed Wyrd implementation or remediation tasks for approved-spec traceability, executability, TDD quality, real dependencies, and credible verification. Use before implementation; this is not post-implementation task review.
---

# Wyrd Task Readiness

Remain read-only. Decide whether one or more proposed tasks can be implemented
without inventing or changing a material decision.

## Establish authority

Read the approved spec revision and every proposed task. Read `AGENTS.md`,
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

## Review readiness

Assess:

1. **Authority** — the spec revision is explicitly approved and each task
   preserves it.
2. **Coverage** — every mapped requirement, invariant, acceptance obligation,
   consumer, generated surface, and required journey has an owner.
3. **Cohesion** — each task owns one implementable outcome; shared mutable
   seams are not split unsafely.
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
8. **Decisions** — no task hides a new product, public/durable contract,
   ownership, security, tenancy, migration, rollout, or acceptance choice.
9. **Remediation integrity** — remediation tasks map validated findings to the
   approved spec without broadening behavior or reviewing only a fix diff.

Do not reject a task for harmless Markdown shape, optional metadata, private
helper names, or a different reasonable implementation choice.

## Findings and verdict

A blocking finding includes the exact task location, governing authority or
spec obligation, repository evidence, implementation consequence, and smallest
required task correction.

Return one verdict: `READY`, `REVISE_TASKS`, `SPEC_REVISION_REQUIRED`, or
`BLOCKED`. Lead with the verdict, summarize the nine axes, and list only
blocking findings. For `READY`, state `No blocking readiness findings.` This
review does not edit tasks, implement code, approve an implementation
candidate, or authorize task branches.
