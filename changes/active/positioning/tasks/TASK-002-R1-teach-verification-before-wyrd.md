---
id: TASK-002-R1
kind: remediation
status: implemented
spec: SPEC-positioning
spec_revision: 5
requirements: [REQ-007, REQ-008, REQ-010, REQ-011, REQ-013, REQ-014, REQ-015, AC-006, AC-007, AC-009, AC-010, AC-012, AC-013, AC-014, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006]
depends_on: []
parent_task: TASK-002
remediates: []
---

# Teach verification first, then show its application in Wyrd

## Outcome and value

A reader can learn what verification is, how its methods apply to software,
models and agents, and what changes when verification happens during execution,
without installing or choosing Wyrd. A later page demonstrates how Wyrd applies
those concepts. The reader should be able to explain the ideas using an example
that does not appear in the articles.

This is a maintainer-directed editorial revision requested on 2026-11-16, not a
remediation produced by independent task review. No reviewer finding IDs are
assigned. The original [TASK-002](TASK-002-verification-education-and-practices.md)
and its execution evidence remain context; this task does not claim that an
earlier reader test or build proves the revised lesson.

The positioning goal remains Wyrd as a trusted place to learn about and implement
real-time verification. Earn that association by teaching the subject well and
providing supported implementation paths. The foundation must remain useful to
people who never adopt Wyrd. Sources substantiate the explanation and provide
further reading; a bibliography alone does not teach the subject or establish
search visibility.

## Diagnosis and revision boundary

The present Foundations page introduces a report example, then moves through a
taxonomy and research summaries before reaching its practical starting method.
It covers the requested subjects but does not develop them into a continuous
lesson. Its universal four-part description also presents application-oriented
evidence and verdict conventions as if they defined all verification.

There are two concrete teaching errors to correct alongside the structure:

- `foundations-of-verification.md`, "A skipped step," treats a recorded
  authorization lookup as settling authorization. A lookup record can establish
  that a call occurred; its result and context are needed to establish that the
  recipient was authorized. Explain both claims and their different evidence.
- `how-verification-works.md` says every result is stored, while
  `real-time-verification.md` explains asynchronous retention and possible loss
  before publication. Reconcile both with the architecture's existing contract;
  do not change durability behavior or promise guaranteed retained evidence.

The current maintainer instruction refines the original task's editorial
sequence: teach general verification and real-time concepts independently,
then use Wyrd as a practical application. This direction supersedes the original
task's Wyrd-led real-time opening and relocates its finance implementation,
native verify/submit/drift-schedule explanation and Wyrd infrastructure detail
to How verification works in Wyrd. Real-time verification keeps the general
document-sharing example, decision timing, inline/later/recurring distinctions,
latency components and evidence/error boundaries. Required coverage is moved,
not dropped. Preserve the approved subject coverage, finance demonstration,
existing routes, publication boundaries and runtime truth. This is an explicit
maintainer-directed placement change to the earlier REQ-010/TASK-002 teaching
sequence, with no runtime or public-contract decision. No specification or
architecture is edited by this task.

## What changes and what must stay true

Primary owners are the existing three Understand pages, in this order:

| Existing route | Reader question and title | Product boundary |
|---|---|---|
| `understand/foundations-of-verification/` | **Foundations of verification:** What is verification, what methods are available, and what can their results establish? | Teach the engineering discipline. No Wyrd APIs, Cards, installation or sales claims in the explanation. An optional implementation link belongs in Next. |
| `understand/real-time-verification/` | **Real-time verification:** What changes when checks happen while a system runs, and when is a result useful in time? | Teach the general concept first. End with a short, explicit bridge to the Wyrd application page. |
| `understand/how-verification-works/` | **How verification works in Wyrd:** How does Wyrd implement the concepts just learned? | Own the Wyrd explanation, declared contracts, source-backed example and product-specific timing/retention details. Keep its URL. |

All three remain **Concept** pages under `docs/AGENTS.md`: worked explanations
with concrete observations, not installation tutorials or exhaustive references.
"Follow along" means the reader can follow the reasoning and predict the next
result. Installation and complete runnable procedures stay in the Quickstart
and Build and verify; exact contracts stay in Reference. Preserve existing
anchors referenced by other pages, or update all affected links together.

Audit the TASK-002 practice guides, technical FAQ, Introduction/Quickstart Next
links, PRODUCT learning-path wording and generated agent text for consequences
of this split. Make bounded link, terminology, inaccurate-claim and duplicate
explanation repairs. Do not rewrite the entire corpus or remove required
practice/FAQ coverage. TASK-003 still owns new runnable tutorials; TASK-004 owns
shared metadata, canonical/sitemap rendering and final discovery integration.
Preserve its agreed search-title handoff for the real-time page.

## Recommended page structures and required content

Use the following section order and responsibilities. Headings may be adjusted
for clarity without removing a lesson or moving product mechanics back into
the tool-independent teaching. These are content outcomes, not a word quota.

### 1. Foundations of verification

Open with a plain definition of verification as establishing whether something
meets specified requirements, and why an apparently good result may be
insufficient. Establish the scope as engineering, with software, ML and agents
as the examples. Do not define all verification as monitoring an execution or
returning Wyrd's three outcome values.

| Section, in reading order | What to teach and show |
|---|---|
| **A result can look right and still be wrong** | Start with a sorting example: input `[3, 1, 3]` produces `[1, 3]`. The result is ordered but lost an item. Show the expected `[1, 3, 3]`. Separate the ordering and item-preservation requirements; explain why looking plausible does not establish either requirement. Use illustrative input/output data, not a new executable example project. |
| **What is being checked, and against what?** | Develop that example into a stated requirement, relevant evidence and a check. Show which requirement the result meets and which it violates. Explain assumptions and evidence quality. Decisions are a possible use of verification, not a compulsory fourth stage of every method. Distinguish a violated requirement from not having enough evidence to determine it. |
| **When verification happens** | Use the same property to distinguish testing chosen executions before deployment, checking an actual execution while it runs, and examining recorded executions later. Introduce runtime verification here, explain why timing matters, and link to the next concept page. Online and offline monitoring both exist; real-time use adds a decision deadline. Avoid classifying "before deployment" and "runtime" as mutually exclusive. |
| **Different methods establish different things** | Explain testing versus verification, verification versus validation, formal proof and runtime checking using the sorting example and a concrete intended-use question. Explain assumptions and abstraction; a few successful tests do not prove every execution. Include concise treatment of evaluation, observability, guardrails and assurance and their overlap. Do not present an arbitrary taxonomy as a universal standard or imply only one discipline verifies behavior. Formal methods can reason about probabilistic systems, and runtime monitors can check formal properties. Explain those two facts plainly before naming advanced techniques. |
| **When there is no single expected answer** | Use two differently worded summaries that preserve the same source facts. Show how scoped properties, prohibited content, acceptable ranges or justified relations between runs can be checked without one reference string. Introduce the oracle problem after showing the difficulty. Explain deterministic, statistical, human and model-assisted judgments and uncertainty in the system, evidence and judge. Model agreement and repeatability alone do not establish correctness. |
| **What the output cannot establish** | Work through three short cases with concrete evidence: an ML evaluation made optimistic by leakage; changed data or relationships requiring investigation; and an agent claiming it ran tests or performed a lookup. For each, show what must be inspected and what remains unknown. A shift does not prove an individual prediction wrong. A logged call does not prove its outcome. A missing log does not prove a skipped action without a justified completeness assumption. Keep these distinct from ordinary output-property checks, where the output itself is valid evidence. |
| **Build a verification practice** | Connect the examples into a repeatable approach: define requirements, choose adequate evidence and methods, test checks on positive/negative/uncertain cases, act where appropriate, retain provenance and revise as systems change. Explain the two continuing difficulties: adequate requirements and reliable capture/execution/retention infrastructure. Domain accountability remains necessary; agentic assistance does not solve either difficulty universally. End with a small transfer question and explained answer using an example outside the opening story. |
| **Sources and further reading** | Provide the sources used, their scope and useful reading directions. Keep the practical conclusion before the bibliography. Preserve citations beside claims and make the specific source reachable. |
| **Next** | Primary continuation: Real-time verification. Also offer the requirement/evidence practices and an explicitly optional Wyrd implementation path. |

Use the sorting case to teach the early reasoning, then show its limits through
ML and agent examples. Finance must not organize this page. Do not replace the
current literature survey with another list of disconnected examples: explain
what each new case adds to the reader's understanding.

### 2. Real-time verification

Open with a general explanation of checking relevant properties during
execution soon enough to inform the current action or response. State the
page's meaning of real-time and distinguish it from hard real-time scheduling.
The definition must make sense without knowing Wyrd, its invocation names or
its result format.

| Section, in reading order | What to teach and show |
|---|---|
| **The same check can arrive too late** | Introduce an application preparing to share a document. Compare checking the proposed document and authorization before sharing with discovering a violation afterward. Show how the same evidence/check can inform different responses depending on timing. This is a general document-sharing example; identify its later finance Quickstart as one implementation. |
| **What can be checked during execution?** | Follow a generic request or agent step through available evidence, checks, result and the application's next action. Distinguish checking proposed content and preconditions from verifying a completed action's effects. An authorization check before a send does not prove delivery or subsequent recipient behavior. |
| **A useful result must arrive in time** | Explain the application's deadline and the components of elapsed time: collecting/preparing evidence, engine/coordination work, and check/tool/provider execution. Contrast a local comparison with a model-assisted policy judgment without invented timings. Explain refusal, timeout and the application's deliberate fallback. Synchronous invocation alone establishes no suitable latency bound. |
| **Missing evidence and uncertain judgments** | Use the example to distinguish a violation, insufficient evidence and a check that failed to execute. Show how an application can hold, seek review or choose another documented fallback. Do not turn generic terminology into a universal enum or error-code standard. Explain the trust and exposure implications of sending evidence to an external judge. |
| **When a later check is appropriate** | Compare inline, queued and recurring verification by the decision each can inform. General recurring verification is broader than Wyrd's current supported scheduler. Clearly distinguish that general concept from Wyrd's drift-only scheduled bindings on the application page. Live and during-execution are equivalent descriptions here. |
| **What must be measured and retained** | Explain representative end-to-end measurements, expensive/variable judges, evidence provenance, changed requirements and the difference between a returned result and a durable record. Exact publication behavior depends on the implementation. No PostgreSQL/cache variables, Bifrost tables or Wyrd error catalogs here. |
| **Apply these ideas with Wyrd** | One short bridge linking to How verification works in Wyrd, the tested Quickstart and relevant production guidance. Describe Wyrd as an implementation of the preceding ideas; do not imply verification requires it or that every best practice is implemented automatically. |
| **Sources, further reading and Next** | Cite general runtime claims to primary sources; distinguish the article's practical synthesis from established research terminology. Make the Wyrd application the natural next option while preserving links back to general foundations. |

Keep the main explanation independent of Wyrd. Relocate its current long YAML
declaration, native call/status/error inventory and cache/outbox discussion to
the existing application or reference owner as appropriate. Preserve accurate
coverage and repair all inbound section links; do not simply delete it.

### 3. How verification works in Wyrd

Open by identifying Wyrd as one implementation of the preceding practices.
Describe its concrete role without claiming certification, universal correctness
or ownership of verification. Prior knowledge is the two concept pages, not
architecture documents or installation. Link to those concepts for newcomers.

| Section, in reading order | What to teach and show |
|---|---|
| **The concepts in Wyrd** | A compact mapping from requirements, evidence, checks, results and provenance to the corresponding Wyrd concepts. Introduce Card, Verifier and binding only as needed. This mapping describes Wyrd, not universal terminology. |
| **Follow one verification** | Reuse the tested finance declaration/API to follow a request through supplied evidence and pinned policy, the checks, verdict and application-owned decision. Label finance as the example. Explain its two assertions and model judge; distinguish supplied authorization facts from live external authorization inspection. Source code must come from existing tested files. Link to full setup rather than repeat it. |
| **Declare requirements and choose checks** | Explain named inputs, optional schema, requirements/statements/checks, optional preparation and the overall decision rule through that declaration. Keep the existing concise check-kind/workspace explanation where it answers a reader question; move exhaustive limits to linked reference. Explain exact fixed policy selection and why a new policy requires a new declaration. |
| **Call verification and handle the result** | Explain native `verify`, `submit` and supported drift schedules after the inline example. Distinguish requirement outcomes, overall verdict and execution statuses/errors. Preserve missing-input, trace-visibility, citation, timeout and Operator boundaries through concise explanation and canonical detail links. |
| **Understand the time and retention costs** | Own the concise source-backed Wyrd infrastructure explanation moved from the real-time page: process target cache and misses/listener loss, permission/admission costs, evidence resolution, check/provider work, and result staging followed by publication. Preserve admission/durability/backpressure semantics. Do not claim all reads are local or every returned verdict is already stored. Use current architecture/runtime references; no unsupported numbers. |
| **Inspect evidence and change requirements** | Explain the returned compact result versus later inspection, citations, exact versions and policy changes that do not retarget old executions. Link to the existing inspection/version owners. Reconcile all categorical retention claims across these pages and the FAQ. |
| **Next** | Offer the Quickstart for first execution, practice guides for requirements/evidence/production decisions, and current published reference. No links to unpublished TASK-003 tutorials. |

## Sources and writing models

### Research sources

Retain the approved [foundations research brief](../design/verification-foundations.md).
The source set is valuable; improve its use rather than deleting the scholarly
foundation or substituting unsupported intuition. Each published claim needs
the relevant primary passage and the right scope:

| Teaching need | Existing sources and appropriate use |
|---|---|
| Requirements and validation | NASA handbook for its stated engineering definitions; Zave/Jackson for requirements, specification and environment assumptions. Label the discipline/context. |
| Formal and runtime verification | Leucker/Schallhart, Bauer et al., Agha/Palmskog and Dennis et al. for their respective methods and assumptions. A finite observation, a statistical bound and a deductive proof establish different things. |
| No single reference output | Barr et al. for the oracle problem; Segura et al. for justified relations between runs. Passing one relation is not proof of either output's correctness. |
| ML evaluation and changing conditions | Kapoor/Narayanan, Gama et al., Ovadia et al. and Breck et al. for leakage, drift, measured uncertainty and production practices, with study scope intact. |
| Agent and model judgments | Zheng et al., AgentBench and VerifiAgent for the studied benchmarks/methods. Preference agreement, factual correctness and domain-policy compliance are different claims. |
| Recent proposals and wider arguments | AgentGuard remains a workshop proof of concept; Catalini et al. remains an economic argument. Use only where they help teach a required idea; otherwise retain them as annotated further reading. |

Read the relevant methods and limitations for empirical claims, not just the
abstract. Update the existing TASK-002 claim ledger with claim, exact source
section, source type/version, limits and the example it supports. Record access
limits and narrow or remove unverified detailed claims. Do not cite
every shortlisted paper merely to satisfy a quota. Do not copy research prose
or make the learner follow citation chains to understand the main lesson.

### Documentation and tutorial examples for the author

Read these before drafting. They were opened on 2026-11-16 for this task.
They are writing models, not verification authorities or permission to copy
wording, APIs, install commands or page structure wholesale.

| Reference page | Technique to borrow | Where it helps |
|---|---|---|
| [Google ML Crash Course: Accuracy, recall and precision](https://developers.google.com/machine-learning/crash-course/classification/accuracy-precision-recall) | Connect a definition to a concrete classification example and show a counterexample where an attractive metric misleads. Explain why the distinction matters. | Foundations: evidence, evaluation validity, leakage/drift and limits. |
| [The Rust Book: What is ownership?](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html) | Develop a difficult concept through small related examples and explain the consequence of each change. Borrow the progression, not its jargon, length or language-specific internals. | Foundations: requirements, observations, checks and increasing uncertainty. |
| [MDN: Overview of HTTP](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Overview) | Establish the overall interaction before examining individual parts; walk through the sequence of events. Link onward for exhaustive details. | Real-time concepts and the Wyrd request-to-result explanation. |
| [pytest: Get started](https://docs.pytest.org/en/stable/getting-started.html) | Show a small example, its observed failure and an explanation of what that failure means. | Worked observations and the Wyrd example. This is a tutorial model; do not copy its installation procedure into a concept page. |
| [Diataxis: Explanation](https://diataxis.fr/explanation/) | Connect concepts, explain why distinctions matter and bound the subject. | Keep all three articles understanding-oriented; link to guides/reference for other reader jobs. |

### Required repository style

`docs/AGENTS.md`, `docs/CONTRIBUTING.md` and `docs/PRODUCT.md` govern the output;
external examples do not override them. In particular:

- Open each concept with what it is and why it matters, before the first
  heading. Use sentence-case descriptive headings and concrete language.
- Follow Concept voice: third person; use "you" only where a reader action
  matters. A worked explanation does not require numbered imperatives.
- Introduce each technical term when its example needs it. Define it in plain
  language before adding an acronym or research name. Avoid slogans and
  abstract headings that do not tell readers what the section explains.
- Explain the observation, why it follows, and its limits in connected prose.
  Use a table for parallel comparisons, not as a substitute for reasoning.
  Do not mechanically force every section into identical sentence/bullet counts.
- Keep useful caveats next to the claim they qualify. Preserve evidence gaps,
  study limits and failure semantics while removing repetitive disclaimers.
- Use existing source-backed CodeFromFile embeds for copyable Wyrd code.
  Illustrative data examples may use small tables with explicitly stated
  expected/observed results; they do not require a new example application.
- Use existing link/callout/diagram components. Every diagram answers one
  question and has a text alternative. No new visuals are required for this
  prose revision; maintain any affected existing source-owned diagram.
- Keep citations beside substantive external claims and exact references easy
  to follow. No invented author, reviewer, date, performance or adoption claims.
- Apply human-tech-docs, its Forrester voice reference, and the humanizer pass.
  Run the existing humanizer lint on revised pages and explain deliberate hits.
  A clean word lint is not proof that a page teaches well.

## Approach and existing owners

1. Reconcile the three outlines with current source and authority, then revise
   Foundations as a complete conceptual lesson before editing the product page.
2. Write the independent real-time explanation; move necessary Wyrd-specific
   material into the existing application/reference owners and repair links.
3. Reconcile TASK-002 practices, FAQ and learning entry links, then regenerate
   readable agent content through the existing generator.
4. Validate claims and run independent reader transfer/retrieval checks, repair
   gaps, and execute only the focused documentation checks below.

| Existing owner and inspected consumers | Gap and selected reuse |
|---|---|
| Three existing Understand Markdown files; practices, FAQ, Introduction and Quickstart | Reorganize existing prose and examples at their canonical owners. Preserve URLs, concepts and public mechanics; no duplicate educational site or synonym pages. |
| `docs/src/lib/content.ts`: `validateFrontmatter`, `getEntry`, `allSlugs`; `derived-nav.ts`: `buildNav`, `firstPage`, `siblings` | Existing metadata/navigation already provide the sequence. Adjust page inputs and contextual links; no new registry/parser/navigation mechanism. Existing content/derived-nav regression files cover publication/order. |
| `docs/src/lib/mdsvex/highlight.js`: `embedCodeFromFile`; `page-text.js`: `pageText`; existing page-text/index regressions | Existing source embedding/projection supplies site and agent text. Preserve shared interpretation and tested example files. No handwritten code copies or second text converter. |
| `docs/scripts/generate_llms_txt.py`: `sidebar_pages`, `render_llms_txt`, `render_llms_full` | Regenerate existing outputs after canonical content changes. Do not hand-edit generated prose or introduce a discovery system. |
| Existing route page/head owner and TASK-004 metadata plan | Supply natural titles/descriptions and query mapping; leave shared canonical/social/sitemap repairs to TASK-004. Preserve the agreed real-time browser-title handoff. |
| `architecture/wyrd-design.md` Verifier/Run/workspace sections, `architecture/bifrost-design.md` verification projection/outbox sections and current runtime reference | Verify Wyrd claims at their existing authority. Correct prose drift only; no new retention, runtime, scheduling or deadline guarantee. |

## Proof strategy and acceptance criteria

This is prose/frontmatter/link/generated-content work. No executable behavior
is added, so do not manufacture RED tests or rerun every language journey.
Retain actual-program proof for reused Wyrd embeds; changed example/runtime
behavior is outside this revision and must return to its owning task.

| Obligation | Required observation and proof |
|---|---|
| REQ-010 / AC-009 | Foundations teaches the broader discipline through worked reasoning, retains all locked subject coverage and avoids treating Wyrd's contracts as universal. A fresh reader explains a new software/ML/agent example without Wyrd terminology. |
| REQ-010 / AC-009 | Real-time stands alone: the reader explains checking during execution, decision deadlines, evidence/check time, uncertainty and later checks before seeing Wyrd implementation details. The application page accurately maps those ideas to the existing product. |
| REQ-011/013 / AC-010/012 | Practices and FAQ remain coherent, all previous required topics remain covered, and authorization/process evidence plus retention distinctions agree across the pages. Links point to the right canonical owner. |
| REQ-007/008/014 / AC-006/007/013 | Published entry/next links and query map distinguish broad education, real-time concepts and Wyrd implementation. A reader arriving on any of the three pages can find the next useful explanation or runnable procedure. Generated text contains the same lesson and embedded examples. |
| REQ-015 / AC-014 | Updated claim ledger identifies relevant passages, methods/limits, source type/version and teaching use. Direct citations/further reading are usable; source access gaps and unsupported claims are recorded. |
| INV-001–006 | No runtime/API/security/retention change, unsupported claim, new dependency, route break or false publication evidence. Repository writing style and existing docs checks pass. |

### Reader and transfer checks

Use a fresh reader with only the revised rendered articles and their normal
links, not the task, source ledger or intended answers. Give these questions
and record answers with the supporting passages and any repairs:

1. Given a new small software example, what requirement is being verified and
   what would establish a violation? What would remain unknown?
2. How do testing, validation, formal proof and runtime verification relate?
   Can the reader explain overlap and assumptions without a false binary?
3. Why might a strong ML score or plausible agent answer be insufficient?
   What additional evidence would the reader seek?
4. What can be checked when there is no single expected answer, and how would
   the reader assess a model judge rather than trust its confidence?
5. What does an authorization-call log establish? What does its absence establish?
6. What makes a result useful in real time? How do deadlines, delayed evidence
   and slow judges change the choice between inline and later checks?
7. Which ideas can be implemented without Wyrd, and which names/contracts
   belong specifically to Wyrd? What practical path is offered to implement them?
8. Does receiving a Wyrd verdict mean its evidence is already durable/queryable?
   Can the reader find the correct current boundary without reconciling contradictions?

Also ask the reader where they had to reread, infer missing context or leave
the lesson to understand a term. Correct factual answers alone do not establish
an easy learning experience. Record those obstacles and their bounded repairs.

### Focused commands

Run through mise using the current owners. Preserve source and generation
integrity; do not weaken checks or commit merely to satisfy a generated guard.

```bash
mise run docs:generate
mise exec -- pnpm --dir docs exec vitest run src/lib/content.test.ts src/lib/derived-nav.test.ts src/lib/indexes.test.ts src/lib/mdsvex/page-text.test.ts
mise run docs:check:commands
mise run docs:linkcheck
mise run docs:svelte:check
mise run docs:build
mise run docs:a11y
mise exec -- git diff --check
mise exec -- bash /Users/stevenforrester/.codex/skills/humanizer/scripts/lint.sh docs/src/content/docs/understand/foundations-of-verification.md
mise exec -- bash /Users/stevenforrester/.codex/skills/humanizer/scripts/lint.sh docs/src/content/docs/understand/real-time-verification.md
mise exec -- bash /Users/stevenforrester/.codex/skills/humanizer/scripts/lint.sh docs/src/content/docs/understand/how-verification-works.md
```

The last three commands use the current local skill installation. Resolve its
installed location if it changes, and lint any other materially revised pages
with the same tool. Inspect built text, source embeds, anchors and Next links.
Record source-checking and reader evidence, actual commands/results and the
TASK-003/004 handoff in this task's execution evidence. Full integrated docs/
journey gates run once at change review; search ranking is not a local gate.

## Non-goals and material stop conditions

Do not implement these pages while merely planning this task. During later
implementation, add no runtime feature, executable example framework, glossary
site, new page registry, analytics, SEO schema, competitor audit or claim that
Wyrd defines all verification. No numeric performance target is authorized.
Preserve the caller's unrelated changes, including landing-page edits.

If a required Wyrd explanation needs a new public/runtime/security/retention
contract, stop with SPEC_REVISION_REQUIRED. Do not edit architecture to match
prose. A required scholarly claim lacking accessible support must be narrowed,
qualified or reported as a publication gap; do not invent evidence. Ordinary
wording, examples, section refinements and source-backed link repairs remain
implementation-owned within the explicit teaching sequence above.

## Authority links

- [Approved positioning specification, revision 5](../spec.md).
- [Original TASK-002 and its recorded research/reader evidence](TASK-002-verification-education-and-practices.md).
- [Foundations research brief](../design/verification-foundations.md) and [positioning design](../design/notes.md).
- [Repository rules](../../../../AGENTS.md), [spec-driven workflow](../../../spec-driven-development.md), [implementation execution](../../../implementation-execution.md), [testing](../../../../TESTING.md).
- [Docs writing rules](../../../../docs/AGENTS.md), [contribution rules](../../../../docs/CONTRIBUTING.md), [editorial owner](../../../../docs/PRODUCT.md).
- [Wyrd architecture](../../../../architecture/wyrd-design.md), [Bifrost architecture](../../../../architecture/bifrost-design.md).

## Planning evidence

An independent comprehension reader reviewed the approved specification,
original TASK-002, this proposal and docs/AGENTS.md together. It understood the
three-page teaching sequence, boundaries and proof; its requested clarification
of the relocated finance/native-mode/infrastructure coverage was applied in
Diagnosis and revision boundary. Humanizer lint and whitespace checks passed
for the proposed task. No documentation implementation or runtime check was
performed, and the maintainer's existing landing-page edit was preserved.

## Execution evidence

Implemented 2026-11-16. No runtime, API, retention, example or architecture
file changed; the maintainer's `docs/src/routes/+page.svelte` edit is untouched.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-010 / AC-009 Foundations | `understand/foundations-of-verification.md`: sort `[3, 1, 3]` → `[1, 3]` → requirement/evidence/check table, assumptions, undecidable vs violated, decisions optional → timing (chosen/actual/recorded; online/offline) → testing, V&V, proof, runtime checking, SMC and LTL monitors, neighboring-practice table → two incident summaries, oracle problem, scoped properties, judge uncertainty → leakage, drift, agent claim (call vs result vs absence) → six-step practice, two difficulties, transfer question (currency conversion) → Sources with reading directions → Next. No Wyrd API in the lesson | Reader answers 1–5 below; humanizer lint clean | PASS |
| REQ-010 / AC-009 Real-time | `understand/real-time-verification.md`: definition and hard-real-time distinction; document-sharing example; proposed content vs completed-action effects; deadline and three time components; violation / undecidable / not run; provider exposure; inline/queued/recurring; measurement, provenance, returned result vs durable record; one-paragraph Wyrd bridge; synthesis labeled in Sources | Reader answer 6; no Wyrd mechanics before the bridge | PASS |
| REQ-010 Wyrd application | `understand/how-verification-works.md` (title "How verification works in Wyrd", URL kept): concept mapping; finance handler and `release-check.yaml` via existing `CodeFromFile`; supplied authorization fact vs live inspection; declaration parts, fixed policy selection; verify/submit/drift schedule table; verdicts, statuses, missing-evidence/timeout/error table; trace evidence; Operators; where the time goes (moved); staging, loss on abrupt stop, compact result vs stored detail, versions | Reader answers 7–8; claims checked against `architecture/wyrd-design.md` (verify staging, `202` confirms staging only, loss on abrupt death) and `architecture/bifrost-design.md` (three verification tables) | PASS |
| REQ-011/013 / AC-010/012 | FAQ links repointed; "Where is the evidence kept?" now states the loss boundary; foundations "skipped step" replaced by call/result/absence distinction; how-verification-works no longer says every result is stored | Anchor scan of built HTML: every link into the three pages resolves | PASS |
| REQ-007/008/014 / AC-006/007/013 | Introduction, Quickstart, What can I build, Act on verdicts, Verifier recipes, architecture, cards and draft request guide links/link text; `PRODUCT.md` learning path; TASK-002 query map; `docs/public/llms*.txt` regenerated by `docs:generate` | Vitest content/nav/index/page-text 57 passed; linkcheck, build, a11y pass | PASS |
| REQ-015 / AC-014 | TASK-002 claim ledger: R1 teaching-use table (claim, source section, limit, example); Koohestani moved to annotated further reading; no new external claim | Ledger review | PASS |
| INV-001–006 | No runtime/API/security/retention change, no new dependency, routes kept, titles left to TASK-004 | `git diff --stat` audit | PASS |

### Commands

| Command | Result |
|---|---|
| `mise run docs:generate` | regenerated `llms.txt`, `llms-full.txt` |
| `mise exec -- pnpm --dir docs exec vitest run src/lib/content.test.ts src/lib/derived-nav.test.ts src/lib/indexes.test.ts src/lib/mdsvex/page-text.test.ts` | 4 files, 57 tests passed |
| `mise run docs:check:commands` | pass |
| `mise run docs:linkcheck` | pass (does not check anchors; built-HTML anchor scan run separately) |
| `mise run docs:svelte:check` | 0 errors, 1 warning: unused `.lede + .lede` in `src/routes/+page.svelte`, caused by the maintainer's uncommitted landing edit; left for the maintainer |
| `mise run docs:test` (after diagram text repair) | 13 files, 102 tests passed |
| `mise run docs:build` | pass |
| `mise run docs:a11y` | 38 pages, AA light and dark |
| `mise exec -- git diff --check` | clean |
| humanizer `lint.sh` on the three pages and `technical-questions.md` | one hit ("genuinely") removed; clean |

The anchor scan also found seven broken anchors, all in unpublished draft pages
(`call-wyrd-from-a-stock-client`, `run-a-verification-and-inspect-its-evidence`,
`respond-to-a-failed-verification`, `verify-a-request-in-real-time`,
`monitor-drift`). They are not published and belong to TASK-003.

### Reader and transfer proof

A fresh agent read only the three built pages (`docs/build/understand/*/index.html`),
with no task, ledger or intended answers.

| Q | Reader answer (abridged) | Supporting passage | Result |
|---|---|---|---|
| 1 | New example: rate limiter, 100 requests per client per 60 s; violation = 101 admitted in one window; unknown if logging is sampled, whether 100 is right (validation), single-clock assumption | Foundations, "What is being checked, and against what" | PASS |
| 2 | Tests cover chosen inputs, proof covers all under assumptions, runtime checks actual runs, validation asks fitness; overlap via SMC and LTL monitors; not exclusive | Foundations, "When verification happens", "Different methods establish different things" | PASS |
| 3 | Leakage, drift; agent report only shows it wrote the report; seeks split/feature provenance, outcome labels, CI exit status, tool-call results | Foundations, "What the output cannot establish" | PASS |
| 4 | Required facts, prohibited content, ranges, metamorphic relations; judge measured on labeled in-domain cases, remeasured on change; agreement is not correctness | Foundations, "When there is no single expected answer" | PASS |
| 5 | Log shows the call, not its result or use; absence decides only under a complete-capture assumption | Foundations, "An agent's claim about its own work" | PASS |
| 6 | Useful before the decision; three time parts; judge dominates; inline only what must settle first; fallback chosen in advance, never a pass | Real-time, "A useful result must arrive in time", "When a later check is appropriate" | PASS |
| 7 | Build-a-practice is tool-independent; lists Wyrd-specific names; Wyrd path plus practice guides | How verification works in Wyrd, "The concepts in Wyrd" | PASS |
| 8 | No: "A returned verdict is therefore not proof that its record is already stored or queryable"; flagged the `verification-paths` diagram text "The inputs and result are stored" as misleading | How verification works in Wyrd, "What Wyrd retains, and how requirements change" (renamed from "Inspect evidence and change requirements") | PASS after repair |

Obstacles and bounded repairs:

| Obstacle | Repair |
|---|---|
| Diagram text implied synchronous storage | `src/lib/diagrams/verification.ts`: "The server answers first and stores the inputs and result afterward." |
| Check-level error (`inconclusive`) vs execution error (no verdict) not stated | Concepts table row and "Verdicts" now state the split |
| "subject" used before definition | Defined beside binding in "The concepts in Wyrd" |
| `not_applicable` unexplained | Defined from the step `when` rule (Verifier Card reference) |
| Finance assertion reads a supplied boolean, the pattern Foundations warns about | Explicit sentence linking back to Foundations and naming stronger evidence (lookup response or trace) |
| "A Verifier never names what it checks" repeated | Duplicate removed from the page |

Not repaired, with reason: Scribe and LLM gateway names appear only in the
existing diagram and in the required infrastructure section, each linked to its
owner; the target-cache and workspace detail is required coverage moved by this
task; the separate 60 s call bound, 5 min attempt bound and trace retry waits
are each stated at their owner and in Runtime limits; the diagram's repeated
Operator text alternative belongs to the existing diagram owner.

### Handoffs

- TASK-004: the frontmatter title stays `Real-time verification`; the agreed
  browser title still needs its metadata owner. Query map updated in TASK-002.
- TASK-003: tutorials link to `how-verification-works` for Wyrd mechanics and
  to `real-time-verification` for the concept.

### docs/AGENTS.md conformance pass

A post-implementation audit against the Concept rules found imperative
headings, an imperative step list, imperative prose, four-sentence openings and
the banned words "easy" and "just" in new prose. Repairs: headings renamed
("A repeatable verification practice", "Applying the ideas to a new case",
"These ideas in Wyrd", "One verification, end to end", "How a Verifier
declares requirements", "Calls, verdicts, and failures", "What Wyrd retains,
and how requirements change"); practice list rewritten as third-person noun
phrases; Foundations and Real-time openings reduced to two sentences; inbound
anchors in the FAQ and Act on verdicts updated. Rerun: `docs:generate`,
`docs:build`, `docs:test` (102 passed), `docs:linkcheck`, `docs:check:commands`,
`docs:a11y`, humanizer lint on all four pages, `git diff --check`: all pass,
and every published anchor resolves in the built HTML.

Deliberate exceptions: the pages carry several worked examples rather than the
Concept shape's single example, because this task requires the sorting, ML and
agent progression. The two short `verify`/`submit` Python snippets and the
`drift` row's "You care whether…" were carried over unchanged from the existing
page; they are not source-embedded and are left for their owner.

### Maintainer review: definition before the example

The maintainer found that Foundations reached requirements before explaining
what verification is or why it matters. Added "What verification is" ahead of
the sort example: the question every verification answers, the three parts
(requirement, evidence, check with met/violated/cannot decide), why the
requirement is stated first, why ML models and agents raise the stakes
(plausible output, varying runs, irreversible actions), and the page's scope.
The opener is two plain sentences again. Rerun: humanizer lint clean,
`docs:test` 102 passed, linkcheck, build, a11y, `git diff --check` pass.

Second maintainer review: the section still described verification's parts
without defining it, and folded importance into one sentence. Split into
"What verification is" (direct definition, NASA definition cited, unverified
means unknown, the three parts, requirement first) and "Why verification is
important" (knowing vs assuming; four reasons: earlier catch, inspectable
claims, systems change, a record for responsibility; then why ML and agents
strengthen each). Rerun: humanizer lint clean, `docs:test` 102 passed,
linkcheck, build, a11y, `git diff --check` pass.

Third maintainer review: opener removed from Foundations at the maintainer's
direction; the page goes from the title straight to "What verification is".
This departs from the docs/AGENTS.md Concept rule ("opens with two sentences
before any heading"), pending the maintainer's decision on that rule.
Rerun: `docs:test` 102 passed, build, a11y, linkcheck, `git diff --check` pass.

### Plain-language pass on Foundations

Maintainer direction: write for developers and non-specialists. Research terms
were replaced by plain explanations, the "two common simplifications"
paragraph became "methods work together" (payment-after-approval rule), the
neighboring-practices table became one plain paragraph, the drift case was
split into two labeled cases, and research detail moved to a plain further-
reading list in Sources. Every remaining claim keeps its citation; wording was
checked against the claim ledger's corrected limits.

Two fresh readers read only the built page: a developer without CS theory and
a non-engineer. Both answered the definition, methods, misleading-score/agent-
claim and missing-log questions correctly with their own examples. Repaired
from their reports: the "opposite of verified" riddle, the ML "validation set"
aside (removed), the compiler assumption, the deadline cliffhanger, the
customer-records assumption order, "metamorphic relation" (term removed),
"two models agree", why shared customers leak, drift wording, model confidence,
the practices table, the AI-tools paragraph, the preprint wording and the
Sources wall of names. Rerun: humanizer lint clean, `docs:test` 102 passed,
linkcheck, commands, build, a11y, `git diff --check` pass.

### Plain-language pass on Real-time verification

Same treatment as Foundations: no opener; "What real-time verification is" and
"Why timing matters" first; plain wording throughout; coverage unchanged.
Foundations' judge material was also split into "Can an AI judge be trusted?"
(FAQ link updated). Developer and non-engineer readers answered every
question correctly with their own examples. Repaired from their reports:
"whatever runs the checks"/"the caller", measured conditions, "turn work
away", empty vs failed authorization lookup, the misplaced provider-exposure
paragraph (now "What an AI model sees"), the three later-check conditions
(now a list), "inputs slowly changing", the overloaded measured-and-kept
section (split in two), the returned-vs-saved sentence and "a run" in
Sources. Rerun: humanizer lint clean, `docs:test` 102 passed, linkcheck,
build, a11y, `git diff --check` pass.

### Reading-effort audit

Maintainer review found sentences that were correct but had to be decoded.
Both pages were rewritten so each sentence states one complete idea with no
gap to fill, no undefined term, and no back-reference. Independent
sentence-by-sentence audits flagged 56 (Foundations) and 47 (Real-time)
sentences; all were fixed in third person. A second audit flagged 7 more,
also fixed. The third result is now "can't tell" on both pages. Rerun:
humanizer lint clean, `docs:test` 102 passed, linkcheck, build, a11y,
`git diff --check` pass.

### Code example de-duplication

The full report-release program now appears only on Run your first
verification. The landing page keeps the tested API handler figure and drops
its Declare/Observe/Verify code tabs; What Wyrd does shows the verify loop and
output, not the 97-line declaration; How verification works links to the
handler in Act on verdicts in production instead of repeating it; Cards and
versions shows one setup Card. The dead `.lede + .lede` selector was removed.
Rerun: `docs:test` 102 passed, svelte-check 0 warnings, linkcheck, build,
a11y, anchors, `git diff --check` pass.

### Code and YAML declaration consistency

One rule now governs declarations (docs/AGENTS.md Examples): code tabs plus
the same Cards as YAML where the page teaches the code route, YAML with a
link to Author in code elsewhere. How verification works gains "Code or
YAML" (same Cards; code to start; YAML recommended in production). The
landing page restores Declare/Observe/Verify as short tested regions
(`verifier`, `verify`) of the finance-release files in each language plus
YAML; the Quickstart offers the YAML files in a note; the YAML guides link to
Author in code and back. Rerun: `docs:test` 102 passed, svelte-check 0
warnings, linkcheck, commands, build, a11y, `git diff --check`, and
`cargo check` on the examples pass.
