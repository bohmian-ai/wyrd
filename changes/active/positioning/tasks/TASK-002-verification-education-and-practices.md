---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-positioning
spec_revision: 3
requirements: [REQ-007, REQ-008, REQ-010, REQ-011, REQ-013, REQ-014, REQ-015, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, AC-006, AC-007, AC-009, AC-010, AC-012, AC-013, AC-014, AC-015]
depends_on: [TASK-001]
---

# Establish verification concepts, practices and adoption answers

## Outcome and Value

A reader researching verification finds useful explanations and practical
standards, then reaches the runnable Wyrd example. The site becomes a reference
for understanding requirements, evidence and judgments as well as an API guide.
For example, “the agent should be reliable” becomes a scoped requirement with
observable evidence, a suitable check and cases that distinguish pass, failure
and insufficient evidence.

The existing site explains the parts of a Verifier but lacks the complete
foundation/practices/FAQ path required by the approved rollout. Add that path
within existing sections; do not make another marketing site or a glossary
page for every keyword synonym.

## What Changes and What Must Stay True

Own the verification foundation, real-time category explanation, practical
requirements/evidence/production guidance, adoption FAQ and their contextual
links. Extend current How verification works, Author a Verifier and Recipes
where responsibilities fit; keep one canonical explanation per topic. A new
page needs a distinct reader job, not simply a target phrase.

Concepts and FAQ belong in Understand. Actionable practice guides belong in
Build and verify using its existing Declare/Observe/Verify topics. Preserve
all six top-level sections, current URLs and current reference authority.
The exact public routes required by the spec are real-time-verification and
technical-questions under Understand. Give other coverage a sensible canonical
owner; filenames and whether to consolidate a page remain reversible choices.

Teach evidence-backed application verification without misrepresenting formal
proofs, validation, evaluation, observability or guardrails. Practices explain
the prevented failure, good/bad examples and limits. Show honest model-judge
reliability and sensitive-evidence considerations, not unmeasured calibration
or latency claims. Missing input can make dependent checks inconclusive; an
absent trace alone does not establish a skipped action.

This is editorial/static work using supported behavior. It authorizes no new
check kind, provider behavior, public API, schema or runtime. Task 3 owns the
remaining runnable tutorial implementations, and Task 4 owns shared discovery
logic. Keep copyable examples source-backed and tested. Link only to published
procedures, then close reciprocal tutorial links as Task 3 publishes them.

## Approach

1. Extend Task 1's content disposition with a canonical owner and reader
   question for the foundation, each practice and the FAQ. Record this
   educational/practice/FAQ portion of the approved ownership transfer in
   the older docsite packet, preserving its unrelated obligations.
2. Publish the source-backed foundation, real-time explanation and technical
   FAQ as the educational part of the initial release set.
3. Complete practical guidance for requirements/checks, evidence, evolution
   and production decisions, reusing existing examples and references.
4. Connect the learning entry path, concepts, practices and Task 1's finance
   quickstart with descriptive reciprocal links. Preserve detailed reference.
5. Review claims against current authority and primary external definitions;
   regenerate readable indexes and test navigation/content/link closure.

## Proof Strategy

No manufactured RED is appropriate: this task changes prose, frontmatter and
links, not application behavior. Use source-backed review, existing regression
checks and a reader/agent content walkthrough. If a recommendation needs new
executable example behavior, coordinate it into the actual example owner and
TDD scope rather than quietly adding it to this static task.

The required content coverage is explicit; page count is not a target:

| Reader job | Coverage to establish |
|---|---|
| Understand verification | Requirement → evidence → judgment → application/team decision; relation to validation, evaluation, monitoring, guardrails and formal verification; useful without adopting Wyrd. |
| Understand real-time verification | Finance request flow; synchronous verify, queued submit and scheduled checks; timeout, missing-evidence and native error boundaries. Live and during-execution are equivalent descriptions. |
| Write useful requirements and evolve standards | Scoped observable statements, evidence needed, check selection, examples/counterexamples, deterministic versus model judgment, reliability/regression limits, reuse/version changes and historical attribution. |
| Establish adequate evidence | Provenance, original citations, exact versions, completeness and partial/missing evidence. No universal inference from missing traces. |
| Integrate production decisions | Passed/failed/inconclusive/errors, timeout handling, provider data/tool exposure, cost/latency tradeoffs without invented measurements; accurate reference links. |
| Decide whether to adopt | FAQ answers enforcement ownership, verify/submit/schedules, missing evidence, timeout/errors, provider calls/sensitive data/model limits, versioning/evidence, languages and local/self-hosted deployment. |

Use source-backed concrete examples for each practice, not keyword-only prose.
The query map assigns reader questions to useful canonical content; final
cross-corpus query/retrieval closure belongs to Task 4. The initial release set
requires this task's proven foundation, category explanation and FAQ as well as
Task 1's landing/onboarding work. Publication handoff waits for Task 4's
required discovery repairs and checks. Required later practices cannot be dropped
just because the initial set is ready.

## Acceptance Criteria

| Obligations | Required observation |
|---|---|
| REQ-010; AC-009; INV-001–005 | Foundation and real-time coverage accurately explain adjacent disciplines, native execution/evidence semantics and limits, with tested examples and current authority links. No new product guarantee is invented. |
| REQ-011; AC-010; INV-002–005 | Every required practice has its prevented failure, concrete good/bad examples and limits; evidence completeness, check/model reliability, version evolution and production decisions are covered. |
| REQ-013; AC-012; INV-002–005 | The technical FAQ directly answers every required topic, links to its canonical owner and avoids unsupported enforcement, hosted or model-quality promises. |
| REQ-007–008, REQ-014; AC-006–007, AC-013; INV-004–006 | The learning route and content disposition lead to distinct published pages with natural query wording, reciprocal links and no duplicate procedures. This task supplies its query/content mapping to Task 4. |
| REQ-015–016; AC-014–015; INV-005–006 | External definitions cite primary sources and attribution/dates are truthful. Evidence identifies initial-set pages and all required practice coverage; Task 4 consolidates maintenance/measurement responsibility. |

## Implementation Reference

Inspected owners, their consumers, and the missing behavior:

- **Concept owner:** `understand/how-verification-works.md` already defines
  parts, check kinds, workspaces, bindings and verify/submit; its old
  `CodeFromFile` quickstart is changed by Task 1. Extend/link this owner for
  mechanics while adding a distinct foundational definition and the required
  real-time category page. Do not duplicate its reference inventories.
- **Practice owners:** `author-a-verifier.md` teaches assertion → code →
  Agent checks, and `verifier-recipes.md` groups assertions, traces, model
  judges, multi-step, files, sessions and drift. Their consumer path already
  links to runtime and Card references. Missing: a complete explanation of
  writing/evolving standards, evidence sufficiency and production choices.
  Reuse their patterns and correct touched unsupported claims (for example a
  blanket millisecond assertion) rather than repeat them.
- **Composition owner:** `declare/author-in-code.md` embeds the actual
  three-language program and modular finance files. Its authoring/versioning
  purpose stays distinct from the finance quickstart; retain tested embeds
  and update learning/practice links rather than creating a second grammar.
- **Editorial/navigation:** `docs/AGENTS.md` fixes page types and source-backed
  examples; `docs/PRODUCT.md` owns audience/workflow positioning. In
  `content.ts`, `validateFrontmatter` and `toSlug` decode sections/topics;
  `derived-nav.ts::buildNav` derives the sidebar. Supply valid frontmatter
  and links; no new section, metadata parser or navigation framework.
- **Readable consumers:** `generate_llms_txt.py::sidebar_pages` /
  `render_llms_txt` / `render_llms_full`, and `mdsvex/page-text.js::pageText`
  already derive readable page bodies and resolve source embeds through
  `highlight.js::embedCodeFromFile`. Content tests, navigation tests and
  index tests guard these consumers. Task 4 repairs shared behavior if needed;
  this task changes canonical page inputs and regenerates output.

## Verification and Evidence

Use `mise run docs:install` and `mise run docs:generate`, then focused existing
regression files and current owner checks:

```bash
mise exec -- pnpm --dir docs exec vitest run src/lib/content.test.ts src/lib/derived-nav.test.ts src/lib/indexes.test.ts src/lib/mdsvex/page-text.test.ts
mise run docs:check:commands
mise run docs:linkcheck
mise run docs:svelte:check
mise run docs:build
mise run docs:a11y
mise exec -- git diff --check
```

Record content dispositions, primary-source/authority validation, example proof
references inherited from Task 1 and existing journeys, published routes and
cross-links. Do not claim existing tests or previous completion records prove
new instructions. A fresh reader/agent using rendered pages should explain a
verifiable requirement, distinguish evidence gaps from failure, find the
relevant practice and reach the actual runnable next step. Record gaps/repairs.
Task 4 performs final built HTML/query/index integration, and integrated review
runs the complete docs gate once. No backend tests or benchmarks are required
for this prose-only write set.

## Material Stop Conditions

Return to specification if a required lesson depends on a new runtime/SDK or
security contract, or if accurate terminology conflicts with authority. Leave
unproven procedures unpublished and report the missing behavior. Do not treat
a new top-level section, fabricated research/credentials, or keyword stuffing
as an implementation choice. Ordinary page splits and source-backed wording
repairs remain implementation-owned.

## Authority Links

- [Approved specification, revision 3](../spec.md).
- [Repository rules](../../../../AGENTS.md), [workflow](../../../spec-driven-development.md),
  [execution](../../../implementation-execution.md), and [testing](../../../../TESTING.md).
- [Docs rules](../../../../docs/AGENTS.md), [editorial owner](../../../../docs/PRODUCT.md),
  and [contribution rules](../../../../docs/CONTRIBUTING.md).
- [Wyrd doctrine, Verifiers and public surfaces](../../../../architecture/wyrd-design.md),
  [Bifrost evidence](../../../../architecture/bifrost-design.md), and
  [security boundaries](../../../../architecture/wyrd-security-posture.md).
