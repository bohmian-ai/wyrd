---
id: TASK-004
kind: implementation
status: implemented
spec: SPEC-positioning
spec_revision: 5
requirements: [REQ-007, REQ-008, REQ-009, REQ-014, REQ-015, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, AC-006, AC-007, AC-008, AC-013, AC-014, AC-015]
depends_on: [TASK-001, TASK-002, TASK-002-R1, TASK-003]
---

# Make the complete verification corpus discoverable and maintainable

## Outcome and Value

A person or agent arriving with a verification question reaches the canonical
explanation, sees complete readable examples and prerequisites, and can follow
the runnable next step. Search engines receive meaningful page metadata,
substantive HTML and a coherent published URL set. Maintainers receive a clear
content/discovery/measurement handoff after the rollout.

This task integrates the completed public corpus and repairs proven gaps in
existing shared owners. It does not create another SEO system or claim a tag
can secure first-page placement. The existing content shell emits a generic
browser-title suffix and optional description, but no canonical/social tags
were found there. Current language tabs already render inactive panels; keep
that behavior and verify it rather than rewrite working components.

## What Changes and What Must Stay True

Own shared metadata and semantic HTML repairs, published discovery closure,
canonical query-to-content mapping, reader/agent retrieval proof and the final
maintenance/measurement handoff. Tasks 1–3 own their content and actual-program
behavior; repair cross-task links and metadata inputs here without taking over
those workflows. Record complete public-surface dispositions in existing task
execution evidence, not a new product catalog.

Keep brand, six sections, existing URLs/site base, accessibility, source-backed
snippets and draft exclusion. Preserve code for every presented language in
built HTML and readable agent output. Reuse native SvelteKit/static generation,
existing metadata/navigation/index owners and installed Pagefind. No new
crawler, dependency, parser framework, hidden keyword content, AI schema,
analytics SDK or per-page copied metadata boilerplate.

This mixed task needs TDD for changed executable rendering/generation and
static/manual proof for content intent and external handoff. Existing correct
behavior uses verification-only evidence. Audit crawl controls and record
hosting restrictions; do not modify production accounts or hosting policy.

Preserve the revised learning hierarchy from TASK-002-R1 in navigation,
page descriptions, initial HTML and readable agent output: Foundations and
Real-time verification teach the discipline independently of Wyrd; How
verification works in Wyrd explains the product; Task 3 supplies runnable
procedures. Keep those reader questions and canonical destinations distinct.
A person or agent asking why Wyrd is built for real-time verification should reach the canonical
request → requirements/evidence → verdict → application decision → response
explanation, including latency components and asynchronous evidence retention.
Do not flatten it into a generic background evaluation feature list. Queued
and scheduled workflows remain discoverable additional uses. Keep fixed titles,
substantive content and natural wording; no new ranking guarantee or synonym
page is needed.

## Approach

1. Assemble the final published-page and query-intent map from Tasks 1–3,
   including TASK-002-R1 and Task 3's updated coverage/disposition handoff.
   Verify each required question has a useful canonical destination and next
   step; remove duplicate explanations rather than add synonym pages.
2. Run the ordered rendering/publication scenarios below, repairing shared
   owners only where an observable requirement is missing.
3. Regenerate compact/full agent text and native static discovery output from
   canonical page inputs. Verify sitemap, search, links and draft exclusion.
4. Test representative reader/agent discovery tasks and responsive/keyboard
   reading against built output; repair obstacles without weakening evidence.
5. Deliver the owner handoff and equal-window measurement procedure, recording
   actual baselines or explicit access gaps. Confirm all required content is
   complete before recommending integrated review/publication.

## Ordered Implementation Scenarios

### Scenario 1 — Render purpose-specific metadata and usable structure

**Behavior.** Homepage, foundation, real-time concept, FAQ and a representative
practice/tutorial expose their intended page-specific browser title,
description, correct base-aware canonical and truthful social metadata. The
spec's three fixed browser titles are honored without replacing the revised
reader-facing H1s. In particular, `real-time-verification.md` keeps the
`Real-time verification` frontmatter title; the shared metadata owner supplies
`Real-Time Verification for AI Applications | Wyrd` as its browser title,
following TASK-002-R1's handoff. Heading order, landmarks and descriptive
links remain usable. No development host becomes the production canonical,
including when previews render the site locally.

**RED.** Extend existing SSR/content or shell regression owners with the
planned [metadata proof](#metadata-proof). Assert the actual page/head output,
not only frontmatter strings. Current missing canonical/social output and
generic title behavior should expose the gap. The existing content render
helper discards the SSR head; inspect actual head output as well as body text
so a frontmatter-only assertion cannot masquerade as metadata proof.
Confirm the test fails for that observable difference rather than missing SvelteKit runtime stubs.

**GREEN.** Repair the existing shared page/layout/homepage metadata owners
and necessary inputs. Keep article and homepage roles distinct and reuse
existing site-base configuration; no new general SEO abstraction or duplicated
head blocks on each content page. Run the focused metadata and shell checks.

**REFACTOR.** Remove overlapping head generation or hardcoded page copies once
one existing owner can supply the behavior. Preserve actual dates/attribution,
reader semantics, all earlier content and legitimate existing metadata.

### Scenario 2 — Publish one consistent discovery set and exclude drafts

**Behavior.** Published content is reachable through contextual navigation,
site search, a base-aware sitemap and existing compact/full agent indexes.
Drafts are absent from production routes and all discovery surfaces. Generated
links point to real pages/assets; removing or drafting a page cannot leave a
stale advertised destination. Crawl controls permit the intended public pages
or identify the external restriction that needs maintainer action.

**RED.** Extend the existing index/publication regressions with the planned
[publication-set proof](#publication-set-proof). Exercise a published page and
a draft/removal variation against actual generated/built outputs. There is no
sitemap owner in the inspected routes/scripts; the missing advertised URL set
is a real gap. Do not fail merely because no fixture build was prepared.

**GREEN.** Reuse the current published corpus/frontmatter and static build to
supply sitemap output and correct generated index/navigation links. Use the
smallest native static route/build extension; no second page registry, crawler
or external service. Keep Pagefind's current build step and extend the existing
fixture-build withdrawal/search proof rather than recreate it under a new test
name. Run Scenario 1 and existing omission/draft/link checks as well as the new proof.

**REFACTOR.** Eliminate parallel inventories and repeated draft filters where
existing owners already supply them. Preserve the six sections and native
routes; no root aliases or global navigation redesign.

### Scenario 3 — Keep prose and all presented code retrievable

**Behavior.** Readers and agents can obtain substantive prose and all relevant
code variants from built HTML and readable full-text output without clicking
a tab to fetch missing content. Code still matches journey-tested source;
frontmatter, raw component tags and draft snippets do not leak as body text.

**RED.** First run the existing projection/tab/source regressions in
[readable-content proof](#readable-content-proof) and inspect built examples.
The inspected `LangTab` and `Tab` already render hidden panels; record a
verification-only result when they satisfy the requirement. If a concrete
regression remains, add the planned [variant proof](#variant-proof) in the
existing projection owner and show that missing actual source variant.

**GREEN.** Only for a demonstrated gap, extend the existing embed/projection/
render owner while preserving active-tab accessibility and keyboard behavior.
Rerun previous scenarios; no alternate full-text parser or client-only loader.

**REFACTOR.** Keep a single source-embed interpretation shared by site and
agent output. Remove redundant conversion or tab machinery; leave already
correct rendering unchanged and record its proof.

## Acceptance Criteria

| Obligations | Required observation |
|---|---|
| REQ-009; AC-008; INV-004–006 | Scenarios 1–3 establish purpose-specific metadata, canonicals, semantic HTML, complete code/prose, valid links and consistent publication/draft sets. Build, link and accessibility checks pass without new discovery infrastructure. |
| REQ-007–008; AC-006–007; INV-001, INV-004 | Final corpus dispositions and rendered entry paths agree with actual published content; six-section navigation, current brand and keyboard/mobile use remain intact. No duplicate finance or concept procedure. Published support-agent entry examples and the finance quickstart have distinct purposes and point to the appropriate next step. Draft/tutorial dispositions from Task 3 agree across every discovery surface. |
| REQ-014; AC-013; INV-004–006 | Every agreed query family maps to substantive canonical coverage. Representative human/agent questions reach the right explanation, prerequisites and executable next step through public pages/indexes, not source archaeology. Include why a verdict before the response matters, how declared contracts guide the check, and how engine/evidence/provider latency differ from later publication. No keyword quota or live rank is substituted for this proof. |
| REQ-015; AC-014; INV-005–006 | Source attribution/credits/dates are truthful. Handoff identifies responsible maintainers for content, discovery checks, distribution and feedback without external posting or account changes. |
| REQ-016; AC-015; INV-005–006 | Evidence distinguishes the initial release set from completion of every required practice/tutorial. The handoff includes baseline/day-14/day-30 equal-window measurement, available data or explicit access gaps and responsible follow-up ownership. |

## Implementation Reference

- **Metadata/rendering:** `docs/src/routes/[...slug]/+page.svelte` renders
  `<svelte:head>`, article, H1 and actual commit date; `+page.ts::load` uses
  `getEntry` and `resolveArchetype`. Homepage `+page.server.ts::load` supplies
  source-backed finance slices and `+page.svelte` owns their rendering/head.
  `+layout.svelte` owns main/header/nav/footer and mobile focus behavior. `content.test.ts` and
  `shell-links.test.ts` already SSR these owners with runtime stubs. Gap:
  required canonical/social/page-purpose output and any discovered semantic
  regressions. Keep H1/frontmatter distinct from the approved browser-title
  contract. Extend these existing owners, not each Markdown page.
- **Published corpus:** `content.ts::allSlugs` / `getEntry` exclude drafts in
  production; `derived-nav.ts::inPrimaryNav` / `buildNav` derive six-section
  navigation. `+page.ts::entries` supplies native prerender entries and
  `svelte.config.js::stripDrafts` removes draft bodies. Gap: a sitemap/URL
  projection and cross-corpus metadata/link closure; reuse these definitions.
  Public docs assets live under `docs/public` and the existing base is /wyrd.
- **Current content handoff:** TASK-002-R1 owns the revised Foundations,
  Real-time verification and How verification works in Wyrd explanations and
  their query mapping. Read those current pages and its handoff before changing
  descriptions or discovery labels. README's support-agent example and the
  finance quickstart/API have different reader purposes; preserve both source
  owners. Task 3 supplies the final tutorial routes, presented languages,
  actual-program proof links and draft dispositions. Discovery must not expose
  an unfinished tutorial just because an older task named its draft route.
- **Agent projection:** `generate_llms_txt.py::sidebar_pages`, `render_start`,
  `render_llms_txt` and `render_llms_full` already produce canonical navigation
  and text. `page-text.js::pageText` calls `highlight.js::embedCodeFromFile`;
  `scripts/page_text.mjs` reuses it. `indexes.test.ts` checks sidebar parity,
  body embeds and withdrawn pages. Extend source inputs/outputs, not a parallel
  text pipeline or a manually maintained index.
- **Tabs/search:** `components/LangTab.svelte` / `Tab.svelte` render children
  in hidden panels; `LangTabs.test.ts` protects selection/fallback and
  `page-text.test.ts` protects source expansion. `package.json` builds static
  SvelteKit output then runs `pagefind --site build`. `shell-links.test.ts`
  already has actual fixture-build and Pagefind search helpers. Reuse those
  complete workflows; do not invent a browser/search harness or rewrite tabs
  because they are visually hidden.
- **Integrity/accessibility:** `readme.test.ts` owns marked source/diagram
  parity; `check_links.py` owns published-link validity; `check_a11y.py` checks
  built page landmarks/headings/contrast. Its current automatic scope excludes
  homepage/404, so homepage and interactive keyboard/mobile observations need
  explicit walkthrough evidence. No copied validator or keyword gate.

## Verification and Evidence

Use `mise run docs:install` and `mise run docs:generate` before generated-index
regressions. Planned test titles below belong in existing test files; confirm
their actual full titles and nonempty collection after addition, before RED.
The command selects the specific title suffix to allow the existing describe
context without accidentally selecting the whole test file.

### Metadata proof

Planned observable-render regression in the existing content owner:

```bash
mise exec -- pnpm --dir docs exec vitest run src/lib/content.test.ts -t 'positioning pages expose canonical metadata$'
```

### Publication-set proof

Planned generated/built publication regression in the existing index owner:

```bash
mise exec -- pnpm --dir docs exec vitest run src/lib/indexes.test.ts -t 'positioning discovery excludes drafts and lists published pages$'
```

### Variant proof

Planned only if existing proof demonstrates a real missing variant:

```bash
mise exec -- pnpm --dir docs exec vitest run src/lib/mdsvex/page-text.test.ts -t 'positioning examples preserve every presented language$'
```

### Readable-content proof

Existing focused regression files; preserve their generic integrity properties:

```bash
mise exec -- pnpm --dir docs exec vitest run src/lib/readme.test.ts src/lib/indexes.test.ts src/lib/content.test.ts src/lib/derived-nav.test.ts src/lib/mdsvex/page-text.test.ts src/lib/components/LangTabs.test.ts src/routes/shell-links.test.ts
mise run docs:check:commands
mise run docs:linkcheck
mise run docs:svelte:check
mise run docs:build
mise run docs:a11y
mise exec -- git diff --check
```

Inspect built homepage, foundational/category/FAQ/practice/tutorial output for
headings, actual body/code content, fixed titles, metadata, canonical base,
sitemap/agent/search coverage and draft exclusion. Check reading on narrow
screens and keyboard use for CTA/nav/search/tabs. Use existing
`mise run docs:preview` when needed; record viewport/actions/results. Compare
changed asset sizes and check existing page interactions/load behavior with
browser tooling if shared components changed; do not invent a backend latency
claim or add a performance framework. Static correct behavior needs no forced
RED; changed executable rendering/generation follows the scenarios above.

For the non-TDD handoff, consolidate query/page dispositions and publication
proof references from Tasks 1–3, including TASK-002-R1. Exercise separate
retrieval questions for general verification, real-time concepts, Wyrd
mechanics, request integration, skipped-work evidence, recurring verification
and failure investigation. The destination must answer that reader's question
and offer its supported next step; finding a page containing the terms is not
sufficient. Preserve Task 3's distinction between existing source coverage and
completed real-server tutorial proof. No live-provider or already-released
package gate is reinstated here; install pins name the release shipping this
change, as required by spec revision 5. State available Search Console and GitHub
baselines truthfully, or record unavailable access and the responsible
maintainer. Specify equal 14-day windows reviewed at days 14 and 30 for search
queries/impressions/clicks/positions, landing pages/referrals and independent
trial attempts/completions/questions. Record observable AI citations/referrals
as partial observations. Actual future reviews/outreach are not part of this
task, and lack of account access does not block local completion. Resolve
maintenance/distribution owner names with the maintainer before final handoff
if the repository gives none; never invent a responsible person.

The full `mise run docs:check` and affected SDK/example suites belong once at
integrated change review, as required by the approved spec. Preserve the
existing generated-diff guard: do not stage/commit without authorization or
weaken the guard to hide changes. Record component results and any guard that
still needs the caller's commit boundary. No ranking/traffic target gates
repository acceptance. This plan defers the publication handoff until this
task's required shared repairs and initial/full-corpus checks pass. Do not publish the initial set
from Tasks 1–2 while known required metadata/canonical/sitemap repairs remain
outstanding. The publication handoff presents the initial set first, followed
by the skipped-step workflow and remaining proven corpus; private staging of
that sequence must preserve draft exclusion. No task publishes externally,
and the initial set never grants whole-change completion.

## Material Stop Conditions

Return to specification if correct discovery requires new platform/security/
public runtime behavior, a dependency, a new site/section or a promise of live
rank/AI inclusion. Keep external account/hosting changes as explicit handoff
work; do not perform them. Private component repairs or equivalent stronger
focused tests may adapt locally within the established owners.

## Authority Links

- [Approved specification, revision 5](../spec.md).
- [Repository rules](../../../../AGENTS.md), [workflow](../../../spec-driven-development.md),
  [execution](../../../implementation-execution.md), and [testing](../../../../TESTING.md).
- [Docs rules](../../../../docs/AGENTS.md), [editorial owner](../../../../docs/PRODUCT.md),
  and [contribution rules](../../../../docs/CONTRIBUTING.md).
- [Wyrd product/public-surface authority](../../../../architecture/wyrd-design.md).

## Implementation Evidence

Commits: `328f79f92` (metadata owner), `cfd4108e6` (sitemap and crawl
policy), `2769dc6a5` (task-naming browser titles), `9a0d1ee2f` (tutorial
links), `cfb9faa9d` (snippet-length descriptions), `bba3e04a5` (retrieval repairs).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scenario 1: page-specific title, description, base-aware canonical, truthful social metadata; H1 unchanged | `$lib/components/PageHead.svelte` is the one head owner for the landing (`website`) and articles (`article`, `article:modified_time` from the commit date only). Canonical and `og:url` are `__WYRD_SITE__` + path, defined once in `docs/site.js` (`ORIGIN` + `BASE`) through `vite.config.ts`, so dev and preview hosts never appear. Optional `browserTitle` frontmatter (validated in `content.ts`) carries the three agreed titles; headings stay `Real-time verification`, `Technical questions`. Error page is `noindex` | RED: `content.test.ts -t 'page metadata'` failed on `'Wyrd docs'` title and absent `article:modified_time` (observable head, not stubs). GREEN: `positioning pages expose canonical metadata`, `gives every page its own browser title and description`, `dates an article only from its commit history` pass; built `understand/real-time-verification/index.html` canonical `https://docs.bohmian.ai/wyrd/understand/real-time-verification/` | PASS |
| Scenario 2: one discovery set; drafts in none | Prerendered `routes/sitemap.xml/+server.ts` over `allSlugs()` (the existing draft filter), `<lastmod>` from commit dates; `prerender.entries` adds it. `publish-docs.yml` writes a domain-root `robots.txt` (allow all, names the sitemap) and an absolute canonical on the root redirect | RED: `indexes.test.ts -t 'positioning discovery excludes drafts and lists published pages$'` failed (no sitemap owner). GREEN: passes under a production content map (`DEV=false`); existing fixture build `withdrawn pages leave every production discovery surface` extended with sitemap and canonical assertions, passes. Real build: 43 sitemap URLs = landing + 42 Pagefind-indexed pages; the three drafts (`call-wyrd-from-a-stock-client`, `respond-to-a-failed-verification`, `write-and-query-custom-data-with-bifrost`) absent from build, sitemap, `llms.txt`, search | PASS |
| Scenario 3: all presented code retrievable | No change. `LangTab` renders every panel, inactive ones `hidden` | Verification only: built quickstart and the four tutorials carry a `<pre>` for each of Python, Rust, TypeScript in every tab group (1, 3, 3, 5, 2 groups); `llms-full.txt` carries each language for those pages; no raw `CodeFromFile`/`LangTab`/`ANCHOR` text in built HTML | PASS |
| REQ-007–008 / AC-006–007 | Six sections, routes, brand and landing unchanged; Task 3 dispositions (four published tutorials, Operators draft) agree across nav, sitemap, `llms.txt`, search. The request, workflow, schedule and investigation tutorials had no inbound content links (request tutorial: none); bounded `## Next`/in-context links added from Quickstart, Real-time verification, How verification works, Establish sufficient evidence, Act on verdicts, FAQ | `docs:linkcheck`, shell-links and index tests | PASS |
| REQ-014 / AC-013 | Query map below; descriptions of the four positioning pages cut from 202–225 to 151–161 characters, leading with their query wording; 13 terse nav-label pages gain task-naming browser titles | Retrieval walkthrough below | PASS |
| REQ-015 / AC-014 | Dates come only from commit history (page footer, `article:modified_time`, sitemap `lastmod`); no author, reviewer or image invented | Content tests above | PASS |
| REQ-016 / AC-015 | Measurement procedure and baselines below | GitHub traffic read; Search Console access gap recorded | BLOCKED on owner names |

### Commands

| Command | Result |
|---|---|
| `mise run docs:generate` | regenerated `llms.txt`, `llms-full.txt` |
| `mise exec -- pnpm --dir docs exec vitest run src/lib/readme.test.ts src/lib/indexes.test.ts src/lib/content.test.ts src/lib/derived-nav.test.ts src/lib/mdsvex/page-text.test.ts src/lib/components/LangTabs.test.ts src/routes/shell-links.test.ts` | 7 files, 75 passed at `bba3e04a5` |
| `mise run docs:check:commands`, `docs:linkcheck`, `docs:svelte:check` | pass; svelte-check 0 errors |
| `mise run docs:build` | pass; Pagefind indexed 42 pages |
| `mise run docs:a11y` | 42 pages, AA contrast light and dark |
| `git diff --check` | clean |

**Design judgments.** `site.js` holds `ORIGIN` and `BASE` because
`vite.config.ts` importing `svelte.config.js` made svelte-check type-check the
whole config; `generate_llms_txt.py` mirrors it, as before. `browserTitle` is
frontmatter rather than a slug map so the page owns its search title with no
parallel inventory. No JSON-LD, Open Graph image or keyword content: the spec
asks for truthful social metadata and no new schema, and there is no approved
image. Homepage and 404 are outside `check_a11y.py`; the landing markup is
unchanged apart from its head, and its links and figure stay covered by
`shell-links.test.ts`. No shared visual or interactive component changed, so no
new mobile or keyboard behavior was introduced.

**Crawl policy.** Repository-controlled: root `robots.txt` (allow all, sitemap),
absolute canonicals, `noindex` error page. Hosting restriction: GitHub Pages
serves `404.html` for unknown paths with status 404; HTTP headers and redirects
are not configurable there. Search Console property verification and sitemap
submission for `https://docs.bohmian.ai/` need account access and are
maintainer work.

### Query map

| Intent family | Canonical page | Next step it offers |
|---|---|---|
| Real-time category | `understand/real-time-verification` (browser title "Real-Time Verification for AI Applications \| Wyrd") | How verification works in Wyrd; Quickstart; Verify during a request; Check on a schedule |
| Broad education | `understand/foundations-of-verification` ("Foundations of AI Verification \| Wyrd") | Real-time verification; Write verifiable requirements; Establish sufficient evidence |
| Behavioral correctness | `build-and-verify/declare/write-verifiable-requirements` | Quickstart |
| Workflow correctness | `build-and-verify/verify/verify-an-agent-workflow` ("Verify an AI agent completed every step \| Wyrd"); rationale in Establish sufficient evidence §5 | Investigate a failed verification |
| Application integration | `build-and-verify/verify/verify-a-request-in-real-time` ("Verify an AI action before your application executes it \| Wyrd"); mechanics in How verification works | Act on verdicts in production; ASGI apps |
| Policy judgment | `get-started/quickstart` ("Quickstart: verify AI output against a policy document \| Wyrd"); Write verifiable requirements §3 | Verify during a request |
| Production continuity | `build-and-verify/verify/monitor-drift` ("Continuous verification on a schedule \| Wyrd"); concept in Real-time verification | Investigate a failed verification |
| Adoption questions | `understand/technical-questions` ("Real-Time Verification: Technical Questions") | Quickstart |

### Retrieval walkthrough

A fresh agent restricted to the built HTML, `llms.txt`/`llms-full.txt`,
`sitemap.xml` and the built Pagefind index (no repository sources) answered
each question from search or `llms.txt`, following links, and checked every
link and anchor on the 14 pages it visited: none dead.

| Question | Canonical destination reached | Result | Next step |
|---|---|---|---|
| What is AI verification; how does it differ from evaluation? | Foundations of verification | Partial, then repaired: the evaluation contrast was one list entry; a `### Verification and evaluation` subsection now answers it, and search for "verification and evaluation" returns the page | Real-time verification, practice guides |
| Why verify before the response; what adds latency; what happens to evidence after? | Real-time verification | Yes: timing, the three parts of the wait, retained versions and evidence | Quickstart, tutorials (after `9a0d1ee2f`) |
| How do declared requirements decide a verdict? | How verification works in Wyrd (via `llms.txt`) | Yes: one check per requirement, `decide` | Quickstart, four tutorials |
| Verify a proposed action in a web API before it executes | Verify during a request | Yes: releases only on `passed` | Act on verdicts, ASGI apps |
| Verify an agent completed every step | Verify an agent's workflow | Yes: trace check catches the skipped lookup behind an identical reply | Investigate a failed verification |
| Verify production behavior continuously | Check on a schedule | Partial: correct mechanics; the example is a latency budget, so its browser title was corrected in `bba3e04a5` | Investigate a failed verification |
| Find the failed requirement, evidence and exact versions | Investigate a failed verification | Yes: failed requirement, judged evidence, `refund-workflow@0.1.0` / `refund-agent@0.1.0` | Retention and Bifrost pages |
| Verify AI output against a policy document | Quickstart | Partial before `bba3e04a5`: on-site search ranked Recipes first; Quickstart now carries the policy wording in its browser title, and What can I build no longer calls it a single assertion | Verify during a request |

Deliberately unchanged: Pagefind indexes visible page text only, so browser
titles do not affect on-site ranking. Feeding it the titles would require
hidden keyword text, which the spec forbids; external search reads `<title>`
directly. The docs say "requirements", not "contracts". Foundations links to
How verification works, not straight to the Quickstart, following TASK-002-R1's
tool-independent design.

### Publication order

1. Initial set (REQ-016): README and homepage, Foundations of verification,
   Real-time verification, Quickstart, Technical questions. This task's
   metadata, canonical and sitemap repairs are in place, so nothing known
   blocks publishing it.
2. Next: Verify an agent's workflow (skipped step).
3. Then the remaining proven corpus: Verify during a request, Check on a
   schedule, Investigate a failed verification, and the three practice guides.

The whole change is complete only after integrated change review runs the full
`mise run docs:check` and SDK/example suites; the initial set does not grant
completion. `publish-docs.yml` deploys every published page on merge to
`main`, so staging the order above means merging in that order; drafts stay
excluded either way.

### Measurement handoff

Review on equal 14-day windows: baseline is the 14 days before publication, then
days 1–14 (review on day 14) and days 15–28 (review on day 30, after GitHub's
data for day 28 is final).

| Metric | Source | Baseline |
|---|---|---|
| Search impressions, clicks, average position by query, for the intent families above | Search Console, property `https://docs.bohmian.ai/` | Not available: no Search Console access from this repository |
| Landing pages from search, by page | Search Console | Not available |
| GitHub views and unique visitors | `gh api repos/bohmian-ai/wyrd/traffic/views` | 2026-11-04 to 2026-11-17: 126 views, 38 unique |
| GitHub clones | `.../traffic/clones` | Same window: 896 clones, 146 unique |
| Referrers | `.../traffic/popular/referrers` | Google 11 (9 unique), linkedin.com 4 (3), github.com 2 (2), reddit.com 2 (2), gust.com 2 (1), seattle.aitinkerers.org 1, t.co 1 |
| Independent Quickstart attempts, completions, recurring questions | GitHub issues and discussions, manual record | None recorded |
| AI assistant citations and referrals | Referrer list and manual spot checks; partial by nature | None observed |

GitHub keeps only 14 days of traffic, so each review must record its window
the day it ends. Record results in a small dated table in this packet's
successor or the maintainer's notes; no analytics dependency is added. Judge
qualified discovery (search landings on the intent-family pages) and completed
trials, not stars or raw traffic.

### Ownership

The repository names no owner (no CODEOWNERS or maintainer file), so these
are unassigned until the maintainer names them:

| Responsibility | Includes | Owner |
|---|---|---|
| Content maintenance | Query map above, tutorial accuracy, day-14/30 content follow-up | Unassigned |
| Discovery checks | Search Console property and sitemap submission, the reviews above, `robots.txt` | Unassigned |
| Distribution and feedback | Sharing specific pages, recording trial attempts and questions | Unassigned |

Distribution guidance for whoever owns it: share a specific page with the
reader's problem — the skipped-step tutorial for agent builders, the request
tutorial for API teams, the failure investigation for on-call engineers —
rather than a homepage announcement. This task posted nothing externally and
changed no account or hosting setting.
