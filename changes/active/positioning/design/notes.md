# Positioning and verification documentation rollout

Status: finalized

## Problem

The finalized rollout direction and current approved spec revision 4 remain in force.
The maintainer has also locked the below-hero finance animation as an addition
to Task 1. Use the final left-to-right composition, with conventional Python
formatting, real-time verification primary and Gateway/Bifrost secondary.
See [the locked animation addition](landing-animation.md). Production
implementation remains task work; the local mockup is the design reference.


The maintainer clarified that this change is a positioning and documentation
rollout, not primarily a new finance example. README, docsite, educational
content, tutorials, verification practices, FAQs, and search/agent discovery
must work together. The finance example supplies the runnable demonstration.
The earlier revision-2 packet covered too little of that outcome. Approved
revision 3 and its four proposed replacement tasks now cover the complete
rollout. The finance quickstart is its entry proof, not its entire scope.

## Desired outcome

A developer or agent looking for verification guidance can find a useful
explanation, understand Wyrd’s real-time verification value, follow a tested
tutorial, and apply repeatable verification practices. README and homepage
lead into this content; search and agent-readable indexes expose the same
published documentation. The site should teach verification as a discipline
as well as explain how to use Wyrd.

The maintainer explicitly confirmed the broader goal: revamp README and the
docsite around the new positioning to improve reachability, exposure and
qualified traffic, and establish Wyrd’s team as an authority on verification.
The recommended initial subject area is verification of AI and agent
application behavior against declared requirements. Explain its relationship
to formal verification and other disciplines without claiming ownership of
every meaning of “verification.” Authority should be earned through useful,
original explanations, demonstrated failure cases, and reproducible practice.

## Constraints and non-goals

- Retain the fixed headline, Declare. Observe. Verify., brand, current routes,
  and six navigation sections. Live and during-execution are equivalent terms.
- Reuse and adapt the existing finance example: one Service, one Verifier,
  three requirements, with policy-PDF analysis by a gateway Agent and an
  application-owned release/hold decision. No required step graph or
  concurrency demonstration.
- Explain practices as actionable engineering guidance, not a claim of formal
  certification or universal industry standards.
- Reuse existing concepts, guides, examples, and generators. No duplicate
  marketing site, runtime additions, keyword-stuffing pages, or bespoke
  discovery platform.
- No promised search ranking, indexing, traffic, or AI citation. Outreach,
  videos, account-dependent analytics, and the source’s 30-day distribution
  experiment are separate from this repository documentation proposal.

## Proposed implementation

The maintainer locked this direction, approved spec revision 3 and requested
planning. Four proposed tasks now cover the outcomes below. This design
remains the intent/research record; the approved specification owns behavior
and the proposed tasks own the current implementation proposal.

| Proposed task | Concrete outcome | Reuse and proof |
|---|---|---|
| Positioning and first verification | Align README, homepage, product/editorial owner, introductions and package descriptions. Promote the existing finance example into a short Python/Rust/TypeScript quickstart with setup, requirement results and release/hold behavior. | Existing finance programs/PDF, quickstart journeys, homepage and snippet owners. Prove actual programs and rendered onboarding on supported release pins. |
| Verification concepts, practices and FAQ | Explain what verification is and how it differs from validation, evaluation, monitoring and guardrails; define real-time verification and synchronous, queued and scheduled execution. Teach writing verifiable requirements, selecting checks, collecting adequate evidence, versioning/reusing standards, handling uncertainty/errors, assessing model judges, and controlling latency/cost. Answer adoption questions directly with links to authoritative details. | Extend How verification works and current authoring/recipe guidance; add distinct missing concept/practice pages and the technical FAQ. Map questions to canonical pages and check examples/claims against current contracts. |
| Runnable verification tutorials | Complete workflows for verifying a live request, detecting a skipped required step in an agent workflow despite plausible final output, scheduled continuous verification, and investigating a failed verdict through retained evidence. | Reuse existing request, background/evidence, drift, and response drafts where their purpose fits; inspect existing trace/agent examples before adding one. Each published procedure needs a runnable example and journey proof of success plus its defining failure case. Preserve the quickstart’s short path. |
| Search and agent discoverability | Integrate all revised content into crawlable HTML, descriptive titles/headings, distinct descriptions, canonical URLs, internal links, sitemap and site search; maintain readable source-backed agent indexes with complete examples and prerequisites. | Existing content/frontmatter/navigation, static build, Pagefind, and llms generators. Check built output, draft exclusion, link reachability and human/agent retrieval tasks. Audit existing robots/hosting policy before proposing any change. |

The first task supplies the onboarding example. Concepts and tutorials share
one vocabulary and cross-link rather than repeat procedures. Discovery is
considered while writing each page; the final task owns integration and checks
across the complete published corpus. Task count may be adjusted during
planning if owner/seam evidence supports a simpler split.

For practices, explain the failure each recommendation prevents and give a
concrete example. For FAQs, answer the question immediately and link to the
relevant tutorial/reference. For tutorials, specify input, operation, observable
result, and how to diagnose failure. Use existing page-type conventions.

### Agreed editorial and discovery direction

**Entry experience.** Keep the agreed headline and developer question, then
show the finance release decision, a small tested Python excerpt and its
three requirement results before infrastructure. Explain how assertions and
the PDF-policy Agent contribute to one verdict. Link equivalent Rust and
TypeScript programs rather than expand README into three long tutorials.
Offer a direct quickstart path and a learning path beginning with What is
verification. Preserve the existing visual identity; improve content order,
labels, links, and page hierarchy where needed rather than freeze confusing
presentation. No global navigation replacement is proposed.

**Initial content set.** Treat the following as distinct reader questions,
not an obligation to create one new file per row. Extend existing owners when
their purpose fits and keep one canonical explanation per topic.

| Reader question | Recommended content and concrete value |
|---|---|
| What is verification for an AI application? | A foundational concept with requirement → evidence → judgment → decision; distinguish validation, evaluation, monitoring, guardrails and formal proofs without caricaturing them. |
| What is real-time verification? | The category explanation with the finance request flow, application-owned decisions, and synchronous versus queued/scheduled execution. |
| How do I write useful requirements? | A practice guide turning vague goals into checkable statements; scope, required evidence, check choice, independent outcomes, examples and counterexamples. |
| What evidence is enough to establish a result? | A practice guide covering missing/partial evidence, original citations, exact versions, provenance and why an absent trace does not establish that an action never happened. |
| How should production code handle verification? | A practice guide covering pass/fail/inconclusive/errors, timeout handling, model-judge reliability, sensitive evidence, cost and latency tradeoffs, plus regression cases when standards change. |
| How do I verify a live request? | The runnable finance quickstart plus the existing request-integration guide, each with a distinct purpose and visible release/hold behavior. |
| Can a good-looking answer conceal a skipped required action? | A runnable agent workflow tutorial with passing and skipped-step cases; original observed evidence must establish the distinction. This is a strong demonstration of verification beyond output scoring. |
| How do I verify continuously and investigate failures? | Proven scheduled-verification and retained-evidence guides, reusing current drafts and examples rather than creating a second procedure. |
| What do I need to know before adopting Wyrd? | One technical FAQ: enforcement ownership, verify/submit/schedules, missing evidence, timeout/errors, provider calls and data exposure, model-judge limits, versioning, deployment and language support. Link to complete procedures and references. |

**Authority through evidence.** Each recommendation should state the failure
it prevents, show a concrete good/bad example, and identify its limits. Publish
observed results from tested programs instead of generic advice or invented
performance claims. Attribute substantive external definitions to primary
sources; identify maintainers/reviewers and substantive review dates truthfully
using existing page facilities where available. Keep general concepts useful
even to readers who have not selected Wyrd, then connect them to Wyrd examples.
Do not add an author/profile subsystem merely for SEO.

**Search and agent access.** Map likely reader questions to existing or proposed
pages before writing; these are intent hypotheses, not measured keyword demand.
Use a direct opening answer, descriptive headings, stable section anchors,
text-readable definitions and tables, contextual links and source-backed code.
Keep titles and descriptions specific to each page. Validate canonical URLs,
crawl permissions, sitemap membership, substantive rendered text, accessibility
and mobile reading. Regenerate existing compact/full agent text with embedded
code intact. Test that an agent can find the right guide, identify prerequisites
and execute the documented path from those public surfaces. Do not equate an
llms.txt file with AI search inclusion or add FAQ markup solely for exposure.

**Publication and reach.** Release the revised landing experience with the
foundation, real-time explanation, finance quickstart and FAQ first. Complete
the remaining practice and tutorial set in the same rollout; prioritize the
skipped-step workflow next. Distribution can point to each useful guide and
its concrete failure case rather than repeatedly announce the homepage.
Recommend a post-publication owner handoff for Search Console, a sitemap check,
relevant community sharing and developer feedback, without posting messages or
making account changes during repository work.

**Measurement.** Capture a baseline before publication where access permits.
Compare equal 14-day windows at days 14 and 30: Search Console impressions,
clicks, queries and landing pages; GitHub traffic/referrals; and independent
quickstart attempts, successful completions and recurring questions. Use
existing analytics or a small manual feedback log; add no analytics dependency
by default. Record AI referrals or citations when observable, without treating
them as a complete count. Low exposure suggests discovery/distribution work;
exposure without clicks suggests titles/message fit; visits without successful
trials suggest onboarding problems. These are diagnostic hypotheses to test.
Traffic is a business outcome; reproducible content and discovery checks are
the repository acceptance evidence, not a guarantee of rankings or adoption.

**Delivery and proof.** Revise the spec and task packet around the four outcomes
above, with an explicit page/content disposition and a purpose for every new
page. Reuse the current six-section site and tooling. Verify tutorial behavior
with appropriate real SDK/server journeys, snippets with existing parity tests,
and discovery through built HTML, links, indexes, accessibility and a reader/
agent walkthrough. The meaningful cost is testing and maintaining the useful
content set; metadata work alone would not establish subject authority.

### Section placement and HTML/component scope

Keep the six existing top-level sections. Recommended placement:

| Section | Role in the revamp |
|---|---|
| Get started | Positioning introduction, setup and finance quickstart. |
| Understand | Verification foundations, real-time concepts, comparisons and adoption FAQ. |
| Build and verify | Practical requirements/evidence guidance, best practices, recipes and runnable tutorials. |
| Connect | Existing SDK, HTTP, CLI, MCP and gateway integration guidance, linked from the new journeys. |
| Operate | Production deployment, security and operational guidance, linked where relevant. |
| Reference | Exact contracts, fields, errors and limits supporting educational answers. |

Improve page ordering, descriptive labels, sidebar links and homepage entry
points within that structure. A new top-level SEO/GEO or Best practices section
is unnecessary. Practices remain discoverable through purposeful titles,
contextual links and grouping within Build and verify.

Recommend targeted shared-component and HTML changes where inspection finds
gaps, in addition to the content work. Audit and reuse the current page/layout,
metadata, navigation, code-tab and index owners. In particular:

- Ensure the page title/description, canonical and social metadata express the
  page’s specific purpose and correct production URL.
- Ensure meaningful heading hierarchy, main/article/nav landmarks, real link
  destinations and stable section anchors; preserve accessibility.
- Check that substantive prose and relevant code examples are present in built
  HTML and agent text, including tabbed content, rather than available only
  after interaction. Test before changing existing tab components.
- Improve homepage hierarchy and learning/quickstart links, page-level related
  guidance, and contextual navigation where the reader’s next step is unclear.
- Verify crawl controls, sitemap, search and generated indexes against the
  published corpus. Preserve draft exclusion.

These are audit-and-repair outcomes, not assertions that every listed mechanism
is currently broken or requires new code. Make shared repairs at the existing
owner, avoiding per-page metadata copies or a replacement component system.
Do not add special AI schema, hidden keyword text, a new crawler service or
structured data without a specific truthful use case. Retain existing visual
identity and assess affected rendering, accessibility and page performance.

### Agreed search-category strategy

The maintainer approved and locked the category and keyword strategy below.
The ambition is to own the space, including first-page discovery for searches
like “verification and AI,” by establishing Wyrd as the reference for real-time
verification of AI applications. The query families are agreed editorial
targets; their search demand, competition and achievable rankings remain
unmeasured. This distinction does not weaken the agreed category ambition.

Recommended category association: Wyrd → real-time verification for AI
applications. Retain the fixed approved public headline; this association
informs descriptive page titles and supporting explanations. Build toward the
broader AI verification topic through specific questions Wyrd can answer and
prove well. Broad terms can refer to identity checks, formal proofs, output
fact-checking and other unrelated jobs; descriptive qualifiers help attract the
intended developer audience.

| Role | Candidate query family | Canonical content destination |
|---|---|---|
| Category association | real-time AI verification; real-time verification for AI applications | Homepage and the distinct real-time category explanation, with different product/learning purposes. |
| Broad educational reach | AI verification; what is AI verification; verification vs evaluation in AI | Foundational verification concept. |
| Behavioral correctness | AI behavior verification; verify AI agents against requirements | Requirements/evidence practice guidance with concrete observable claims. |
| Workflow correctness | AI workflow verification; how to verify an AI agent completed every step | The skipped-required-step tutorial. |
| Inline application use | verify AI actions before execution; verification API for AI applications | Live-request tutorial: observed proposal/evidence, verification result, application-owned decision; never claim proof of an unexecuted action. |
| Policy judgment | verify AI output against a policy document; AI policy compliance verification | Finance/PDF tutorial and requirement practices. |
| Production continuity | continuous verification for AI applications; verify production AI behavior | Scheduled verification and investigation guides. |
| Distinctive vocabulary | evidence-backed verdicts; versioned verification requirements | Supporting explanations throughout canonical pages, not thin pages targeting invented phrases. |

Prioritize intent families, not a new page for every synonym. Place the main
phrase naturally in the title, opening and relevant headings, then answer the
question completely. Link tutorials and practice pages to the corresponding
concept and to each other where they solve the next reader task. Avoid
unsubstantiated claims that a phrase has high volume, low competition or no
other owner. A coined phrase can be memorable yet attract no searches.

Authority-building recommendations: publish reusable requirement examples,
real pass/fail/inconclusive cases, a rigorous skipped-step demonstration and
clear evidence limitations. Give developers reference material they can cite
and maintainers useful examples they can link to. Distribute relevant tutorials
through existing communities, integration documentation and technical writing;
no messages, outreach or external publication is authorized by this research.
Useful external references and independent citations matter alongside technical
crawlability. Do not buy links or manufacture evidence of adoption.

Keyword validation: after publication inspect Search Console’s actual queries,
impressions, clicks and positions by page and relevant geography/device. Record
baseline and equal-window comparisons. Use qualified tutorial visits and trial
completion alongside query visibility; global average position alone does not
establish first-page results for a specific audience. Before selecting targets
on volume/difficulty, obtain actual keyword data; none was available in this
research. The web search sampling is qualitative, not a reproducible Google
rank tracker or competitor analysis.

Research on 2026-11-12 sampled “verification and ai,” “real-time verification”
with AI, AI agent/workflow verification, AI behavior verification and continuous
verification. Google’s primary documentation supports useful content, relevant
wording in prominent places, crawlable links and appropriate promotion; it
provides no mechanism for guaranteeing a first-page result. Existing SEO/agent
HTML checks remain necessary but do not by themselves earn category authority.

## Evidence and rationale

Source documents inspected at the maintainer’s request:

- `.dev/wyrd-positioning/Wyrd — Positioning & Messaging.md`, especially
  “What I recommend publishing” and “Where I’d start,” explicitly calls for
  concepts, best practices, how-to guides and a Technical FAQ. It identifies
  What Is Verification, Real-Time Verification and the multi-step agent guide
  as priorities, while preserving the existing site structure.
- `.dev/wyrd-positioning/Wyrd — Real-Time-First Positioning (Approved Direction) (1).md`
  fixes the real-time-first narrative and calls for precise execution,
  missing-evidence, timeout and failure explanations. Earlier maintainer
  decisions retain the headline without competitor research.

These local source files are research inputs; the eventual tracked spec must
contain the required outcomes rather than depend on private files being present.

Repository evidence:

- [Approved spec revision 3](../spec.md) requires the complete positioning,
  educational, tutorial and discovery corpus, with native runtime boundaries
  and source-backed publication proof.
- [Task 1](../tasks/TASK-001-positioning-and-finance-onboarding.md) already reuses the
  existing finance programs. [Task 2](../tasks/TASK-002-verification-education-and-practices.md)
  owns concepts, practices and FAQ. Tasks 3 and 4 separately own runnable
  tutorials and final shared discovery integration.
- [Finance program](../../../../examples/finance-release/finance_release.py)
  supplies the Service/Verifier/PDF-Agent starting point; the agreed adaptation
  adds destination controls, explicit decisions, and TypeScript parity.
- [How verification works](../../../../docs/src/content/docs/understand/how-verification-works.md),
  [Write a Verifier](../../../../docs/src/content/docs/build-and-verify/verify/author-a-verifier.md),
  and [Recipes](../../../../docs/src/content/docs/build-and-verify/verify/verifier-recipes.md)
  contain reusable explanation and examples.
- Existing request-verification, background/evidence, drift and failure-response
  guides are marked draft. Existing text is not evidence that their complete
  workflows have passed; publication requires revalidation and journey proof.
- [Editorial owner](../../../../docs/PRODUCT.md) requires complete runnable
  developer workflows and retains the current site foundation.
- [Agent index generator](../../../../docs/scripts/generate_llms_txt.py)
  already produces compact/full text and resolves embedded source. Extend it
  instead of adding another index or parallel retrieval system.

External discovery guidance checked on 2026-11-12:

- [Google’s AI features guidance](https://developers.google.com/search/docs/appearance/ai-features)
  says established SEO practices apply, with no special AI files or schema
  required. Eligibility is not guaranteed inclusion. Existing llms files are
  useful agent-facing navigation, not proof of Google or AI discovery.
- [Google’s SEO starter guide](https://developers.google.com/search/docs/fundamentals/seo-starter-guide)
  supports useful content, understandable organization and crawlable links.
  Recommendation: treat GEO as clear, attributable, retrievable answers and
  runnable examples, with no unsupported promise of AI recommendations.

- [Google’s helpful-content guidance](https://developers.google.com/search/docs/fundamentals/creating-helpful-content)
  supports original, substantial and evidence-backed subject expertise.
- [Search Console guidance](https://developers.google.com/search/docs/monitor-debug/search-console-start)
  supports checking indexing and search performance after publication; account
  access is not required to finish the local content implementation.

## Decisions and alternatives

Maintainer decisions: keep the finance story, simplify it to three requirements,
retain the gateway PDF Agent, reuse existing examples, and restore the broader
positioning, educational and discovery outcome described above.

The maintainer accepted the four-outcome direction and the editorial,
publication and measurement recommendations, with reach, traffic and subject
authority as the goals. The maintainer also approved and locked the category
association, candidate query families, authority-building approach and
query-driven measurement strategy. The design is finalized; individual page
boundaries and private implementation choices remain for specification and
planning. Search data must validate demand and prioritization during rollout;
no ranking, traffic or citation result is represented as established.
A broader corpus costs more authoring and journey verification than a homepage
rewrite, but it addresses the actual goal. Publishing only the current two
concept pages would leave tutorials and verification practices underspecified.

## Standards conflicts

No architecture exception is proposed. Keep technical authorities authoritative;
correct tutorial drift against them. Do not implement missing runtime behavior
inside documentation work. The expanded scope is recorded in approved spec revision 3 and four proposed
replacement tasks. They introduce no new runtime/security authority. Task
approval and implementation/review remain separate from design finalization
and specification approval.

## Open questions

The animation direction is locked and assigned to Task 1; no visual decision
blocks implementation. Scale/compression/cost modules and numerical claims are
excluded from this addition. See [the approved addition](landing-animation.md).

The exact page split and reuse of existing tutorial examples need source-backed
planning. Supported release pins and live provider access still need checking
before publication. No decision about a new site section or new runtime is
needed; the proposal uses existing owners and navigation.

## Current task alignment and handoff

Reviewed the four proposed tasks against approved revision 3 and the locked
positioning. Their roles form one reader journey:

| Task | Reader outcome | Contribution to positioning |
|---|---|---|
| 1 — Positioning and finance onboarding | Understand why Wyrd matters and run the existing finance example with three requirements and an explicit release/hold decision. | Makes real-time verification concrete and converts interest into a first trial. |
| 2 — Verification education and practices | Understand verification, write useful requirements, reason about evidence and production decisions, and answer adoption questions. | Earns subject authority with useful explanations and practice rather than product claims alone. |
| 3 — Runnable verification tutorials | Integrate request checks, distinguish skipped agent work, run scheduled verification and investigate retained evidence. | Supplies reproducible proof and deeper adoption paths for the concepts. |
| 4 — Search and agent discovery | Find canonical answers and runnable next steps through coherent public HTML, search, sitemap and readable agent indexes. | Connects the content to the agreed query families and supplies maintenance/distribution/measurement ownership. |

No material positioning mismatch was found in this design-level read. Keep
the existing six sections and visual identity; technical/shared repairs serve
the content, not a new site. Task 3 carries the largest executable-example and
cross-language proof burden and must remain an adaptation of current contracts,
not a runtime expansion. Task 4 improves technical/content discoverability but
cannot by itself deliver exposure: actual distribution, independent references
and post-publication measurement remain operational handoff work. These limits
are explicit in the approved scope and do not require another design decision.

All four tasks remain proposed. Do not claim implementation, runtime feasibility
or search performance from this alignment review. The spec and tasks were not
changed in this review; only this current design record was reconciled with the
approved/planned packet. Implementation and independent task/change review
remain outstanding.

## Locked lower-page workflow strip

The maintainer proposed a second wide, shallow landing-page section near the
bottom: four animations in one desktop row, separated by dashed vertical rules.
The accepted finance/API animation above remains locked. The maintainer
approved the four-panel mockup; Task 1 owns both animation sections.

Purpose: after understanding real-time verification, a visitor recognizes
other systems they can build with the same verification engine. The maintainer
requested a matching animated review mockup with these explicit shapes:

| Panel | Required illustrated shape |
|---|---|
| Real-time verification | Request → verification → application decision. |
| Agent evaluation | Multi-step agent workflow (plan, tool/retrieve, compose), queued/submit evaluation of its execution, then a result. |
| Code review pipeline | Fanout DAG to multiple agent reviewers (e.g. correctness, security, tests), then fan-in to the review result. |
| Session evaluation | Accumulating session turns/events/evidence, then collected session information through a Verifier to a session verdict. |

All four share one desktop row with dashed vertical separators and the accepted
animation's quiet geometry, progress/pulses, typography and colors. Keep each
miniature shallow and readable with minimal labels; no four code editors.
Coordinate motion and provide pause, static/reduced-motion and offscreen/hidden
suspension. Reflow to readable rows on narrow screens. The lower strip complements
the locked detailed API integration animation; it does not replace it.

The maintainer explicitly does not want this concept discussion restricted by
current support checks: these workflows are or will be supported. Show their
intended product value without roadmap disclaimers or support warnings in the
visual. Production contract alignment is later implementation work, not a
reason to stop the requested concept preview. No new runtime is authorized here.

The approved four-panel composition is now part of Task 1.
The approved finance animation remains locked in Task 1. No performance claims,
external services or new production dependencies are part of the mockup.

The [four-panel mockup](mockups/workflows/index.html#workflows) is now available,
authored with Claude Code Fable 5.2 medium. It includes multi-step agent/run
evaluation, a three-reviewer fanout/fan-in DAG, and session evidence accumulation
before verification. [Browser evidence](mockups/workflows/README.md) records
four columns/dashed rules, 203px desktop diagrams, tablet/mobile reflow, controls,
reduced motion and offscreen/hidden suspension. The maintainer approved this composition and it is recorded in Task 1.

## Dedicated, cited verification foundations

The maintainer clarified that Task 2 must establish subject expertise through
a dedicated, tool-independent starting guide, not only documentation about
Wyrd. Task 2 now explicitly requires foundational comparisons, verification
under uncertainty, evidence and judgment limits, leakage/drift and skipped-action
examples, the infrastructure/requirements challenges, and a practical starting
method with primary citations. See the [research brief](verification-foundations.md).
The guide must remain useful without adopting Wyrd; product mechanics and
integration follow through links. This makes REQ-010/011 explicit without
changing runtime scope or adding a top-level navigation section.

The maintainer requested broader research prioritizing core peer-reviewed
sources. The [foundations research brief](verification-foundations.md) now
contains an annotated scholarly set spanning runtime verification, the test
oracle problem, metamorphic checks, statistical model checking, leakage, drift,
uncertainty, model judges, interactive agent evaluation and production ML
requirements, plus labeled NASA engineering guidance. It records access/reading
limits and maps sources to the teaching sequence. Task 2 prioritizes these
foundations and requires claim-to-source validation. No public guide is written
or product claim established by the bibliography alone.

The maintainer approved and locked the dedicated foundations guide, core
scholarly reading direction, teaching sequence and claim-to-source requirements.
Task 2 records this approval and links the locked research brief. Preserve the
tool-independent teaching purpose and separate product integration. Further
claim-specific reading is publication work, not an open direction decision.
Implementation remains pending.

## Locked real-time-first emphasis across the tasks

The maintainer approved a planning correction: runtime/real-time verification
is central, not an afterthought after background evaluation. Tasks 1–4 now
carry the same reader journey: declared requirements and evidence → verdict
while the application can still decide → response, with separate retained
evidence publication. The tool-independent foundation introduces runtime
decision timing early; the real-time category page explains Wyrd's engine and
contracts, then queued/scheduled alternatives.

Explain low-latency infrastructure and adequate requirements as two connected
challenges. Distinguish engine, evidence and check/provider costs, respect
admission/error/visibility boundaries, and avoid hard-real-time guarantees or
unsupported numeric/competitor claims. Existing proof gains comprehension and
observed ordering checks rather than benchmark work. Approved spec revision 4,
existing routes, fixed headline and runtime contracts are unchanged.

A bounded independent comprehension pass over approved revision 4 and all four
tasks found the real-time-first amendment consistent, with coherent dependencies
and no material decision left open. All 37 mapped spec obligations remain
owned; task revision/scenario structure and whitespace checks passed. This is
planning evidence only, not implementation or performance qualification.
