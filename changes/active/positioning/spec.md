---
id: SPEC-positioning
revision: 3
status: approved
---

# Revamp README and the docsite around real-time verification

## Problem, proposed behavior, and value

Wyrd’s public entry points lead with infrastructure before showing why a
running application would use verification. Existing documentation contains
useful explanations and examples, but it does not yet form a complete learning
path through verification concepts, engineering practices, runnable tutorials,
and adoption questions. Changing the headline and quickstart alone would leave
that gap intact.

Revamp README and the docsite around the agreed public opening:

> The only real-time verification system built for your applications.
>
> You built the system. How do you know it's behaving correctly?

The desired association is Wyrd → real-time verification for AI applications.
The business goal is greater qualified discovery, exposure and traffic, with
Wyrd’s team established as a trusted source for understanding and implementing
verification. Earn that authority through useful original explanations,
concrete failure cases, and reproducible practice. The broad applications
positioning includes traditional services; AI and agent behavior provide the
initial educational and search focus.

A reader arriving from GitHub, search, or an agent should understand the value,
choose a learning or runnable path, complete a real verification, and find
practices for applying it safely. For example, the existing finance programs
become the primary quickstart: one Service has one Verifier with three
requirements. Assertions judge recipient and destination facts; a gateway
Agent reads the company-policy PDF extraction. One synchronous call returns
an overall verdict and the requirement results, and the application decides
whether to release the synthetic report.

The rollout covers four outcomes: positioning and first verification;
verification concepts, practices and FAQ; runnable tutorials; and search/agent
discoverability. These describe required outcomes, not a fixed task count.

### Rationale, costs, alternatives, and limits

Keep the existing brand, site foundation, six navigation sections, and
Declare. Observe. Verify. Extend current pages, examples and shared components
where they fit. A separate marketing site or new navigation system adds no
necessary capability. A copy-only refresh would be smaller, but would not
supply the educational depth and executable proof needed for the agreed goal.

The material cost is authoring, proving and maintaining a coherent content
set. The policy Agent also requires a configured gateway/provider and incurs
model usage; prerequisites and evidence exposure must be explicit. Scripted
provider responses make automated journeys repeatable but do not prove live
model quality or replace the documented-provider walkthrough.

Targeted HTML/component repairs accompany the content where inspection finds
reading or discovery gaps. This is not a mandate to rewrite working components.
Indexability, useful content and agent-readable output are verifiable locally;
first-page placement, traffic, adoption and AI citations are external outcomes,
not guarantees or repository completion gates.

## Authority, existing owners, and change boundaries

The [finalized design](design/notes.md) records the maintainer’s locked scope,
category strategy, content direction, query families, and measurement approach.
This specification carries those outcomes directly; the local source messaging
documents are research inputs, not prerequisites for interpreting this packet.
The earlier [positioning foundation](positioning.md) is read through those
maintainer decisions: retain the exact headline, perform no competitor
analysis, and use live and during-execution as equivalent terms.

Current architecture and repository rules own product truth. The completed
[Card authoring](../../completed/2026/card-authoring.md) and
[SDK ergonomics](../../completed/2026/sdk-authoring-ergonomics.md) changes supply
existing authoring, declaration and language projections. Reuse their complete
workflows, the finance examples, existing tutorial drafts and journey support.
No runtime, SDK contract, evidence-storage or security change is authorized.

Approval of revision 3 transfers the following overlapping documentation work
from `SPEC-docsite-incremental-reset` revision 9 to this change:

- Public positioning; README/homepage and Introduction/Setup/Quickstart
  content; and shared quickstart-example consumers.
- The verification education and tutorials explicitly required below,
  including reuse of relevant drafts, their publication proof, and their
  publication sequence. This rollout’s initial release set takes precedence
  over the older packet’s order for these overlapping pages.
- Content-related discovery integration and bounded shared HTML/component
  repairs needed by this rollout.

Record this bounded handoff in the older packet during implementation. Retain
its unrelated work and its publication-evidence, draft-exclusion, accessibility,
brand, diagram and guide-writing obligations. This transfer adopts none of
its proposed Card-kind, SDK-runtime or migration changes. Shared pages have
one implementation owner for this rollout; they are not rewritten twice.

`SPEC-guided-setup` retains CLI bootstrap and guided-agent runtime ownership;
this change updates only affected public instructions and example handoffs.
`SPEC-service-identity-and-telemetry` retains runtime and its separate final
contract-documentation obligation. Describe its current public behavior
accurately and coordinate shared pages without claiming to implement or close
that packet. Architecture contradictions or a required runtime change stop the
affected work and require an authority/specification decision.

Revision 3 was explicitly approved by the maintainer on 2026-11-12. The
existing revision-2 task scope is obsolete and must be replaced or revised
through planning against this revision. Spec approval does not approve an
implementation plan or authorize implementation.

## Requirements

### REQ-001 — Lead public entry points with the agreed positioning

README and homepage prominently show the exact headline and developer question
above. Explain the open-source building blocks for developer-defined
verification through Declare. Observe. Verify. Lead with the problem, the
application’s decision, and working proof before supporting infrastructure.
Do not reopen the headline or conduct competitor research in this change.

### REQ-002 — Promote the existing finance example into onboarding

Adapt the existing `examples/finance-release` Python/Rust programs and PDF
instead of creating a separate duplicate finance story. Supply an equivalent
TypeScript program. All three independently run against the real server using
public SDKs. Retain `/wyrd/get-started/quickstart/` as the primary quickstart.
Use synthetic report, recipient, purpose and destination evidence; simulate
release rather than transmit anything to an outside party.

### REQ-003 — Show a Service with one three-requirement Verifier

The quickstart declares one Service bound to one Verifier with recipient
authorization, policy compliance and destination-control requirements. Use
assertions for supplied recipient/destination facts. The policy requirement
uses an Agent whose model calls go through the Wyrd gateway and whose selected
workspace contains the uploaded PDF’s stored extraction. The judgment must
actually use that extraction. One synchronous verification call per proposed
release returns the overall verdict and individual requirement results.
Separate authored steps, a preparation graph, and concurrency proof are not
required. No latency bound or simultaneous-start claim is made.

### REQ-004 — Make the application decision and failure reason visible

A permitted report passes and the application simulates release. A prohibited
bank-account identifier fails policy compliance while recipient and destination
pass, and the application holds. Unauthorized recipient and inadequate
destination cases independently fail their respective requirements. Only
passed permits release; failed, inconclusive, request errors and execution
errors hold for review. Preserve native error/verdict distinctions. The
application owns the decision; Wyrd never sends or independently blocks the
report.

### REQ-005 — Reuse policy and preserve historical attribution

Upload/extract the policy once and reuse it across requests. The registered
Verifier selects an exact Artifact version and file. Changing policy creates
new attributable policy/Verifier/subject-binding versions through the existing
declaration path without retargeting old declarations. Authored versions may
be omitted where supported; the server still selects exact registered refs.
Expose policy, Verifier and subject identity in results or linked evidence
inspection. Keep policy updates a short follow-on to the main reader path.

### REQ-006 — Close the complete onboarding workflow

Update setup prerequisites, example source/assets, captured output, quickstart,
shared snippets, coding-agent handoff text and affected journey assertions
together. Include the actual gateway deployment, provider credentials/tool
grants, evidence exposure and supported pinned CLI/SDK release. Revalidate
existing install pins rather than assume local HEAD matches a public package.
Use the optional Python Pydantic extra only if the documented program needs it.
The short path is declare, verify permitted/prohibited reports, inspect results.
Other negative cases belong in journey coverage. A retained assertion example
is supplemental; it does not compete as the primary onboarding path.

### REQ-007 — Provide clear learning and runnable entry paths

README presents a concise tested Python example and visible links to Rust and
TypeScript equivalents, then broader uses and supporting infrastructure.
Homepage retains Wyrd’s hero visual, Declare. Observe. Verify. identity,
quickstart demonstration, six-section entry points and GitHub access. Improve
content hierarchy and component presentation within that identity. “Verify in
real time” leads to Quickstart and “Get started” leads to Setup. Provide a
visible learning path beginning with the verification foundation. Explain
requirements and results without making internal architecture a prerequisite.

### REQ-008 — Align the public corpus and editorial owner

Inventory public README files, package descriptions, docsite content and
example introductions; record rewrite, targeted alignment, retain, consolidate
or generated dispositions with reasons and canonical destinations. Update
`docs/PRODUCT.md` to reflect the positioning and educational outcome. Preserve
accurate technical content and give each topic one canonical owner. Existing
Author in code and its embedded finance programs are affected consumers.
Do not rewrite architecture, contributor rules or historical records as
marketing copy. Maintain existing diagrams through their source owners.

### REQ-009 — Make published content readable to search engines and agents

Published pages have substantive primary HTML, descriptive titles/headings,
unique descriptions, correct canonical URLs under the current site base,
accurate social metadata, stable section anchors and crawlable links. Use
semantic landmarks and a meaningful heading hierarchy. Prose and relevant
code, including language-tab content, are available in built HTML and readable
agent output without requiring interaction to obtain missing content.

Audit existing metadata, page/layout, navigation, code-tab and indexing owners;
make bounded repairs where the required result is absent. Regenerate existing
compact/full agent indexes with embedded source intact. Published pages appear
in navigation, site search and sitemap as appropriate; drafts remain excluded
from every production discovery surface. Validate repository-controlled crawl
policy and document any hosting restriction requiring separate action.

Retain the agreed titles: homepage “Wyrd | Real-Time Verification for AI
Applications”; real-time concept “Real-Time Verification for AI Applications |
Wyrd”; FAQ “Real-Time Verification: Technical Questions”. Other titles describe
the actual reader question. Title wording does not narrow the broader product
audience. A generated index or metadata tag alone is not discovery proof.

### REQ-010 — Establish a useful verification foundation

Publish educational coverage of verification for AI/agent application behavior:
declared requirement → observed evidence → judgment → application/team
decision. Explain its relationship to validation, evaluation, monitoring,
guardrails and formal verification without claiming those disciplines cannot
verify behavior. Distinguish evidence-supported judgments from mathematical
proofs and blanket guarantees of correctness. The explanation must help a
reader who has not selected Wyrd, then link to Wyrd’s demonstrated workflow.

Publish `/wyrd/understand/real-time-verification/` as the category explanation.
Use the finance example to explain synchronous verification, queued submit,
and scheduled continuous verification, including timeout, missing-evidence
and error boundaries. Reuse How verification works and runtime references for
detail rather than publish competing explanations. Define live and
during-execution as equivalent descriptions, not separate modes.

### REQ-011 — Teach repeatable verification practices

Provide practical guidance covering these reader outcomes, extending existing
pages where their purpose fits:

| Reader outcome | Required teaching |
|---|---|
| Write a verifiable requirement | Turn a vague goal into a scoped statement with observable evidence, a suitable check, and examples/counterexamples. |
| Choose checks and assess judgments | Explain assertions, code, trace and model judgments; use deterministic checks when suitable, and explain model-judge reliability, regression cases and limitations without claiming calibration that was not performed. |
| Establish sufficient evidence | Explain provenance, citations, completeness, missing/partial evidence and exact versions. An absent trace does not by itself prove an action was skipped. |
| Reuse and evolve standards | Version requirements and policy evidence, preserve historical attribution, and demonstrate changed/unchanged expectations through regression cases. |
| Integrate verification in production | Handle pass/fail/inconclusive/errors and timeouts explicitly; explain provider data exposure and supported ways to reason about latency/cost without invented measurements. |

For each practice explain the failure it prevents, a concrete good/bad example
and its limits. These are engineering practices, not a certification scheme.
Keep actionable guidance in Build and verify, rationale in Understand, and
exhaustive contract inventories in Reference, following existing page types.
Exact page splits remain implementation/planning choices.

### REQ-012 — Publish complete, proven verification tutorials

Deliver all of the following workflows using current supported contracts:

| Tutorial outcome | Distinct demonstration |
|---|---|
| Verify during an application request | Integrate a synchronous check into the application’s request/decision lifecycle, building on the finance quickstart without copying its whole procedure. |
| Verify a multi-step agent workflow | A plausible final answer conceals a skipped required action. Actual execution evidence distinguishes the complete and skipped-step cases. Missing/insufficient evidence cannot silently establish either completion or a violation. |
| Verify continuously | A declared Trigger/binding runs a supported recurring check; controlled evidence establishes a passing and violating case and an attributable result. Do not substitute a client polling loop for server scheduling. |
| Investigate a failure | Follow a failed result to its requirement reasons, original evidence and exact versions through supported public inspection surfaces; distinguish the returned decision from later evidence visibility. |

Reuse the current request, background/evidence, drift and response drafts and
examples where they fit; existing draft prose is not publication proof.
Each workflow has a small independently runnable source-backed example,
prerequisites, actions, observed output, defining negative case and next step.
Use first-class language tabs when a procedure differs by SDK; make language
scope explicit. The primary finance quickstart must run in all three SDKs.
Proof covers every SDK/surface a tutorial actually presents. No missing runtime
capability may be implemented covertly as documentation work.

### REQ-013 — Answer adoption questions directly

Publish `/wyrd/understand/technical-questions/` as one technical FAQ with direct
answers and contextual links to concepts, tutorials and authoritative reference.
Cover application enforcement ownership; verify versus submit and schedules;
missing evidence; timeouts/errors; model-provider calls, sensitive evidence and
model-judge limits; versioning/retained evidence; supported languages and
local/self-hosted deployment. Avoid duplicating procedures or exposing
unproven hosted workflows. Existing detailed execution/reference owners remain
authoritative.

### REQ-014 — Build category visibility around agreed query families

Associate Wyrd consistently with real-time verification for AI applications.
Map the following intent families to canonical useful content, using natural
wording in titles, openings and relevant headings rather than keyword quotas:

| Intent family | Editorial targets |
|---|---|
| Real-time category | real-time AI verification; real-time verification for AI applications |
| Broad education | AI verification; what is AI verification; verification versus evaluation in AI |
| Behavioral correctness | AI behavior verification; verify AI agents against requirements |
| Workflow correctness | AI workflow verification; verify an AI agent completed every step |
| Application integration | verification API for AI applications; verifying a proposed action before the application executes it |
| Policy judgment | verify AI output against a policy document; AI policy compliance verification |
| Production continuity | continuous verification for AI applications; verify production AI behavior |

“Evidence-backed verdicts” and “versioned verification requirements” support
the vocabulary; they do not require separate thin pages. Distinguish observed
proposals/preconditions from proof of an action that has not executed. Search
volume, competition and current positions remain unmeasured; no phrase is
claimed exclusive. Use query data to refine emphasis after publication without
silently changing the agreed product positioning or dropping required content.

### REQ-015 — Make authority attributable and support ongoing reach

Educational content supplies original explanation and tested demonstrations,
not generic keyword copy. Attribute substantive external definitions to primary
sources and identify the limits of each claim. Do not invent maintainers,
reviewers, review dates, customer evidence or research results. Use existing
attribution/date facilities and actual records; no profile subsystem is needed.

Provide a maintainer handoff identifying ownership of content maintenance,
Search Console/discovery checks and distribution/feedback. Describe how to
share specific useful tutorials, reproducible failures and integration guidance
rather than only a generic homepage announcement. This is a documented handoff,
not authorization to publish externally, contact people or alter accounts.

### REQ-016 — Define measurement and publication order

The initial publication set consists of the revised README/homepage,
verification foundation, real-time explanation, finance quickstart and FAQ.
Prove each before publication. Prioritize the skipped-step workflow next;
complete all required practices/tutorials within this change. Incremental
publication does not permit declaring the whole rollout complete early.

Document a baseline and day-14/day-30 review procedure using equal 14-day
windows where access permits: Search Console impressions/clicks/query positions
and landing pages by relevant audience; GitHub traffic/referrals; independent
quickstart attempts, completions and recurring questions. Record observed AI
referrals/citations when available without implying complete attribution.
Use existing analytics or a small manual record; no new analytics dependency.
If account access is unavailable, record the gap and responsible maintainer
rather than fabricate data or block local completion on external metrics.
Track qualified discovery and successful trials, not stars or traffic alone.

## Invariants and non-goals

- **INV-001 — Preserve runtime ownership.** No new runtime, orchestration,
  cache, public API, Card kind, durable data contract, dependency, migration,
  authorization, tenancy or platform behavior. Reuse existing owners; missing
  behavior requires a specification decision, not an implementation workaround.
- **INV-002 — Be precise about evidence.** Use synthetic data and clearly
  label supplied recipient/destination facts. No claim of live third-party
  authorization inspection, autonomous transmission, PII redaction or universal
  model correctness. Disclose provider exposure and preserve tool grants.
- **INV-003 — Preserve native judgments and durability.** Failed,
  inconclusive, request errors and execution errors remain distinct. Missing
  input affects dependent checks under current semantics. Judgments cite
  original evidence and exact versions. No missing trace proves absence.
  A synchronous verdict is not an immediately query-visible durable publication
  guarantee. Do not add request waits/writes or change audit behavior.
- **INV-004 — Preserve the documentation foundation.** Keep current brand,
  responsive reading, WCAG 2.2 AA, existing URLs/site base and the six sections:
  Get started, Build and verify, Connect, Operate, Understand, Reference.
  Concepts/FAQ belong in Understand; practices/tutorials in Build and verify;
  integration/operations/contracts retain their existing owners. Reordering
  pages, links and descriptive labels within those sections is allowed.
  Use current components/generators and pinned reader-native commands.
- **INV-005 — Prove before publication.** No unproven procedure enters
  production routes or discovery surfaces. Do not invent adoption, timings,
  model quality, review credentials or competitive evidence. Preserve generic
  integrity checks when replacing scenario-specific assertions.
- **INV-006 — Keep discovery honest and bounded.** No special AI schema,
  hidden keywords, purchased links, new crawler/index platform, duplicate site,
  analytics dependency or unsupported structured-data promise. Outreach, videos,
  production hosting/account changes and actual future measurement execution
  are outside repository implementation. Ranking/traffic/AI-citation growth is
  the ambition, not an acceptance guarantee.

## Acceptance and evidence

Each row states the observation required for closure. Plans/tasks select exact
commands and source-backed reuse; the table does not prescribe private test
fixtures or implementation structure.

| Acceptance | Observation and expected result | Obligations |
|---|---|---|
| AC-001 | Each actual finance program runs through the real SDK → WyrdTestServer + repository Postgres → SDK path. Permitted release passes; prohibited identifier, unauthorized recipient and inadequate destination independently fail the intended requirement and hold. Reasons/results are visible. | REQ-002, REQ-004; INV-001–003 |
| AC-002 | Missing evidence and model uncertainty exercise native inconclusive behavior where appropriate. Request/execution error cases remain typed errors. Every non-pass/error holds and never invokes simulated release. | REQ-004; INV-002–003 |
| AC-003 | Inspect the declared quickstart graph and real invocation: one Service binds one Verifier with exactly the three requirements, one call returns their results, and the policy Agent’s gateway interaction reads the selected PDF extraction. No step-overlap or timing proof is required. | REQ-003; INV-001–003 |
| AC-004 | Two requests reuse the policy upload/extraction. A changed policy changes selected exact policy/Verifier/binding versions without retargeting the old declaration; public result/evidence inspection identifies the versions used. | REQ-005; INV-003 |
| AC-005 | A fresh reader follows rendered setup/quickstart on the documented release with the documented gateway/provider. Copied programs and captured output reproduce pass/fail, policy reuse and the brief update follow-on. Shared snippets and handoff text agree. | REQ-006; INV-004–005 |
| AC-006 | Rendered README/homepage show the exact opening, application proof before infrastructure, concise example, correct language links and working learning/setup/quickstart destinations. Six-section navigation, brand and responsive keyboard access remain usable. | REQ-001, REQ-007; INV-004 |
| AC-007 | Every inventoried public surface has a reasoned disposition and canonical destination. Revised editorial/package/example surfaces agree, Author in code consumers remain coherent, and generated content is regenerated from its owner. | REQ-008; INV-001, INV-004 |
| AC-008 | Built pages have purpose-specific metadata, valid canonicals/anchors/links, semantic structure and substantive prose/code without requiring interaction. Published coverage agrees across sitemap, search, navigation and compact/full agent text; drafts appear in none. Required docs/build/link/accessibility checks pass. | REQ-009; INV-004–006 |
| AC-009 | A source-backed content review finds clear verification definitions, accurate relationships to adjacent disciplines, and distinct real-time/queued/scheduled explanations with native timeout/evidence/error boundaries. Examples and links agree with current authority. | REQ-010; INV-001–005 |
| AC-010 | Each required practice explains its prevented failure, concrete good/bad examples and limitations. Coverage includes evidence completeness, requirement/version evolution, check/model reliability, and production decisions. Copyable code comes from tested source. | REQ-011; INV-002–005 |
| AC-011 | Every published tutorial has the appropriate real-server journey on each surface it teaches, plus a rendered-release walkthrough. Tests establish the complete/skipped agent-step distinction from adequate evidence, scheduled execution with controlled pass/violation, and attributable failure investigation. Missing evidence never falsely establishes completion or absence. | REQ-012; INV-001–005 |
| AC-012 | The published FAQ directly answers every required adoption topic and links to the correct concept/procedure/reference. No answer promises unsupported enforcement, hosted behavior or model guarantees. | REQ-013; INV-002–005 |
| AC-013 | The editorial query map assigns each intent family to canonical substantive content. A reader/agent given representative questions finds the right published page, its prerequisites and executable next step using public site/index surfaces, without repository archaeology. No arbitrary keyword frequency or live search position is required. | REQ-014; INV-004–006 |
| AC-014 | Substantive external claims have primary-source attribution, demonstrations link to executed evidence, dates/credits are truthful, and the maintainer handoff identifies content/discovery/distribution responsibilities without performing external outreach. | REQ-015; INV-005–006 |
| AC-015 | Publication evidence identifies the initial set and completion of the full required corpus. The handoff defines baseline and equal-window follow-up metrics, records available baseline data or explicit access gaps, and identifies a responsible maintainer for remaining account-dependent work. | REQ-016; INV-005–006 |

Automated provider fixtures prove execution mechanics, not live model accuracy.
Record actual supported release pins, provider/model setup, commands and observed
outcomes for the rendered walkthroughs. Run the complete documentation checks
and affected SDK/example journeys at integrated review; a successful build alone
is insufficient. Verify targeted component changes for their affected rendering,
accessibility and performance properties using existing tools. No backend hot
path change is authorized; if one becomes necessary, return to specification
rather than treating docs verification as its approval.

## Open material decisions and approval

No material design decision remains open. Public scope, headline, six-section
structure, category/query strategy, finance composition, educational/tutorial
coverage and evidence boundaries are fixed above. Page splits, existing-example
selection, provider/model choice within the supported gateway, reversible
component repairs and exact commands belong to planning/implementation.
Supported release availability and provider access must be established before
publication. Unavailable account metrics are explicit handoff gaps, not product
choices or permission to omit required content.

The maintainer explicitly approved revision 3 on 2026-11-12 and requested
planning. Approval covers the complete positioning and documentation rollout,
bounded ownership transfer, invariants and acceptance evidence. Implementation
tasks must reference this approved revision; the old revision-2 scope must
not be executed as its plan. Spec approval does not approve implementation
tasks or grant implementation readiness.

## Revision history

- Approval — The maintainer explicitly approved revision 3 on 2026-11-12 and
  requested task planning; no behavioral edits were made with approval.

- Revision 3 — Expand from quickstart-centered work to the locked README/docsite
  rollout: verification education, practices, FAQ, proven tutorials, category
  strategy, targeted HTML/component repairs, discovery and measurement handoff.
  Retain finance-example reuse and six sections; broaden the explicit transfer
  of overlapping docsite work. Existing tasks require replanning.
- Revision 2 — Simplify the finance example to one Service, one Verifier and
  three requirements; remove mandatory steps/concurrency proof, retain gateway
  PDF judgment, and identify existing finance-program reuse.
- Revision 1 — Initial draft positioning and sensitive-release quickstart,
  accompanied by two proposed tasks. Revisions 1 and 2 were not approved.

## Authority and evidence links

- [Finalized design and research](design/notes.md)
- [Repository rules](../../../AGENTS.md)
- [Wyrd doctrine, Verifiers, authoring and public surfaces](../../../architecture/wyrd-design.md)
- [Evidence durability and visibility](../../../architecture/bifrost-design.md)
- [Security boundaries](../../../architecture/wyrd-security-posture.md)
- [Docs rules](../../../docs/AGENTS.md), [editorial owner](../../../docs/PRODUCT.md), and [contributing](../../../docs/CONTRIBUTING.md)
- [Workflow](../../spec-driven-development.md), [execution](../../implementation-execution.md), and [testing](../../../TESTING.md)
- [Docsite reset](../docsite-incremental-reset/spec.md), [guided setup](../guided-setup/spec.md), and [identity/telemetry](../service-identity-and-telemetry/spec.md)
- [Google Search Essentials](https://developers.google.com/search/docs/essentials), [helpful-content guidance](https://developers.google.com/search/docs/fundamentals/creating-helpful-content), and [AI search guidance](https://developers.google.com/search/docs/appearance/ai-features) support the discovery rationale; they do not override Wyrd authority or guarantee visibility.
