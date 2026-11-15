---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-positioning
spec_revision: 4
requirements: [REQ-007, REQ-008, REQ-010, REQ-011, REQ-013, REQ-014, REQ-015, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, AC-006, AC-007, AC-009, AC-010, AC-012, AC-013, AC-014, AC-015]
depends_on: [TASK-001]
approved_design: ../design/verification-foundations.md
---

# Establish verification concepts, practices and adoption answers

## Outcome and Value

A reader researching verification learns how requirements and evidence can
inform a decision while an application is still running, then reaches the
runnable Wyrd example. The site becomes a reference for runtime verification,
requirements, evidence and judgments as well as an API guide. For example, “the agent should be reliable” becomes a scoped requirement with
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

### Locked foundational guide: verification beyond a tool

The maintainer approved this guide and the research direction below.

Publish a dedicated starting page under Understand, with a descriptive title
such as “Foundations of verification”. This is a substantive, cited introduction
to verification as an engineering discipline, useful to readers who never adopt
Wyrd. Keep Wyrd APIs, setup and native execution modes in the linked product
mechanics/category pages. A new top-level navigation section is not required.

The guide must teach:

- Verification, validation, testing, statistical/model evaluation, evaluation of
  agent trajectories, observability and assurance: their questions, evidence,
  overlaps and limits. Definitions vary by field; cite the definition used and
  avoid presenting an arbitrary taxonomy as a universal standard.
- Formal methods and runtime verification as overlapping approaches, not a
  deterministic-versus-probabilistic binary. Explain properties, assumptions,
  abstractions and observed executions, plus the difference between a proof
  under a model and a judgment supported by finite operational evidence.
  Formal methods can reason about probabilistic systems; runtime monitors can
  check formally specified properties. Neither establishes universal correctness.
- Verification when no single reference answer exists: scope requirements to
  context, policy, invariants, prohibited actions and acceptable outcome ranges;
  select evidence and deterministic/statistical/model-assisted checks; account
  for incomplete or delayed evidence and produce an inconclusive judgment when
  warranted. Distinguish uncertainty in the system, evidence and judge.
- Plausible outputs that mask failure. Include traditional ML data leakage
  (apparently strong scores undermined by split/provenance/time contamination),
  drift (a changed distribution or relationship requiring investigation, not
  automatically an incorrect prediction), and an agent giving a plausible answer
  while skipping a required action. Explain what evidence reveals each failure
  and what output-only judging cannot establish.
- The two practical difficulties: reliable low-overhead capture/execution/
  retention infrastructure, and discovering/maintaining adequate requirements.
  Explain how agentic tools can assist implementation, requirements elicitation,
  counterexample generation and contextual judgment, while retaining domain
  accountability and independent checks. Runtime verification predates LLM
  agents; “AI made it possible” or “AI solved reliability” is not an established
  historical or empirical conclusion.
- A repeatable starting method: identify a consequential decision and its
  failures; declare scoped requirements; determine adequate evidence; choose and
  validate checks against known positive/negative/uncertain cases; decide how to
  respond; retain evidence and revise assumptions as the system changes. Model
  judges are fallible; self-consistency and another model's agreement are not
  independent proof. Explain calibration only where actually measured.

Prioritize peer-reviewed foundational work and primary research over recent
preprints for core definitions; use authoritative engineering guidance where
appropriate and label its source type. Cite sources beside the claims they
support, with a concise bibliography. Distinguish established results, reported
experiments, proposals/proofs of concept, philosophical arguments and economic
models. Include methods/limitations where relevant; do not use a paper about
LLM reasoning or an economic thesis as proof of generic runtime reliability.
Provide the publication date/version and source links. Concrete examples and
an actionable learning path should carry the page, rather than a literature
list or marketing claims. Link onward to practices, then optionally to Wyrd.

See the [foundations research brief](../design/verification-foundations.md) for
the maintainer's motivation, supplied references and their intended roles.

### Locked real-time-first teaching and product explanation

Runtime verification is central to the learning path, not an execution-mode
footnote after background evaluation. Teach when evidence becomes useful for
a decision alongside what requirements and evidence mean. The foundations guide
remains useful without Wyrd: introduce checking observed executions and the
choice of judging before an action, during execution, or afterward early in
the explanation. Keep the established relationship to formal methods and finite
observations accurate; runtime verification includes online and offline work,
and not every runtime check satisfies an application's real-time needs.

Make `/wyrd/understand/real-time-verification/` the main bridge from that
foundation to Wyrd. Its opening and principal example follow this order:
request → declared requirements and evidence → Wyrd verification → verdict →
application decision → response. Explain the opportunity to release, hold or
seek review before the action/response is committed. Queued submission and
scheduled checks follow as additional uses of the same engine, with different
decision timing; they do not replace or overshadow the real-time request path.
The product explanation leads with an engine built for real-time verification
that can also run asynchronously, preserving the approved public headline.

Connect the two difficulties explicitly:

- Reliable verification infrastructure must fit the application's latency
  budget while capturing evidence and preserving attributable results.
- Checks need explicit, adequate requirements: what is checked, what evidence
  it needs, how it judges, and how incomplete evidence/errors affect a decision.

Explain Wyrd's declared, versioned Verifier contracts in that context: named
inputs/schema constraints, identified requirements/statements/checks,
requirement outcomes, overall decision rule and exact version attribution.
These are Wyrd's consistent public contracts, not a claim of universal industry
standardization, complete requirements discovery or mathematical proof.
Use the existing How verification works/Verifier reference owners for detail.

Give the real-time explanation a concise, source-backed infrastructure section.
Distinguish engine/coordination overhead, evidence preparation/read time and
check/tool/provider execution, including possible admission/cache-miss waits.
Explain process-cache reuse and staging retained results/evidence through the
existing outbox, so returning a verdict does not wait for Bifrost publication.
Retain the documented durability/backpressure/error boundaries; do not promise
that every read is local or that every request is free of I/O/admission costs.
The application chooses suitable checks and handles its response deadline,
timeouts and non-pass outcomes. Model-assisted checks can dominate total time;
synchronous invocation alone is not proof a verifier fits its latency budget.
Real-time here means a judgment available for the current application decision,
not a hard-real-time scheduling or universal bounded-latency guarantee.

Cite peer-reviewed foundations for general lessons and architecture/current
source for Wyrd mechanics. Numeric overhead/end-to-end/provider latency claims
need applicable measured evidence with workload, check mix, concurrency,
hardware, cache conditions and percentiles; otherwise omit the number. An
illustrative animation, scripted provider or historical benchmark target is
not a measurement. No new benchmark experiment or backend optimization is
required or authorized by this prose-only task. Avoid unsupported claims that
most other tools lack runtime checks or requirement contracts.

## Approach

1. Extend Task 1's content disposition with a canonical owner and reader
   question for the foundation, each practice and the FAQ. Record this
   educational/practice/FAQ portion of the approved ownership transfer in
   the older docsite packet, preserving its unrelated obligations.
2. Publish the dedicated cited foundations guide with runtime decision timing
   introduced early, the real-time-first category explanation and the technical
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
| Understand real-time verification | Lead with finance request → declared requirements/evidence → verdict → application decision → response; explain the latency budget, versioned contracts, engine/evidence/provider costs and deferred retention. Then explain queued/scheduled alternatives and timeout/missing-evidence/error boundaries. Live and during-execution are equivalent descriptions. |
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
| REQ-010; AC-009; INV-001–005 | A dedicated cited foundations page stands alone without Wyrd adoption: it explains uncertain systems, formal/runtime overlap, adjacent disciplines, leakage/drift and plausible-but-invalid behavior, evidence sufficiency and a practical starting method. The foundation introduces runtime decision timing early; the real-time/product page leads with verdict-before-decision, explicit versioned contracts, latency components and deferred publication, then queued/scheduled alternatives. Research claims are attributed and qualified; copyable code stays tested. No new product guarantee is invented. |
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
- **Real-time mechanics:** `architecture/wyrd-design.md` owns declared
  Verifier inputs/requirements/decision rules, direct `verify` versus queued
  `submit`, exact result attribution and process outbox capture. The existing
  `Observe::verify` / `Observe::submit` in
  `crates/shared/wyrd-client/src/observe/mod.rs` expose those two flows;
  `how-verification-works.md` currently leads with Verifier parts and only
  later compares them. Gap: a reader-first decision/latency explanation,
  not another invocation mode or contract. Reuse and link these owners;
  validate outbox/publication details against `architecture/bifrost-design.md`.
  `benchmarks/README.md` distinguishes benchmark workloads and percentiles;
  use applicable actual records for numeric claims or omit numbers rather
  than rerun performance work under a content task.
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
new instructions. A fresh reader/agent using the rendered foundation without Wyrd instructions
should distinguish a proof from an evidence-supported judgment, explain why a
plausible output or high ML score is insufficient, identify what leakage/drift
checks need beyond an answer, and outline a scoped verification workflow.
Review each material empirical/definition/historical claim against its cited
source in a compact claim-to-source ledger (section, evidence type and limits),
recording unsupported claims removed or qualified. Then the reader should explain a
verifiable requirement, distinguish evidence gaps from failure, find the
relevant practice and reach the actual runnable next step. Record gaps/repairs.
The same reader should explain why receiving a verdict before an API response
changes what the application can do, identify the declared requirement/evidence
contract, separate engine overhead from evidence/check/provider time, and say
what does not wait for publication. Ask when queued or scheduled verification
is preferable. A page that merely lists three invocation modes, implies a
universal latency bound, or omits the application's decision boundary fails
this proof. Record the reader's answers and the source basis for architecture
and timing claims; this is comprehension evidence, not a backend benchmark.
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

- [Approved specification, revision 4](../spec.md).
- [Repository rules](../../../../AGENTS.md), [workflow](../../../spec-driven-development.md),
  [execution](../../../implementation-execution.md), and [testing](../../../../TESTING.md).
- [Docs rules](../../../../docs/AGENTS.md), [editorial owner](../../../../docs/PRODUCT.md),
  and [contribution rules](../../../../docs/CONTRIBUTING.md).
- [Wyrd doctrine, Verifiers and public surfaces](../../../../architecture/wyrd-design.md),
  [Bifrost evidence](../../../../architecture/bifrost-design.md), and
  [security boundaries](../../../../architecture/wyrd-security-posture.md).
