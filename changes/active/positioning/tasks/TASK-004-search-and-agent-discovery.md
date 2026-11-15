---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-positioning
spec_revision: 4
requirements: [REQ-007, REQ-008, REQ-009, REQ-014, REQ-015, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, AC-006, AC-007, AC-008, AC-013, AC-014, AC-015]
depends_on: [TASK-001, TASK-002, TASK-003]
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

Preserve the real-time-first learning hierarchy from Tasks 1–3 in navigation,
page descriptions, initial HTML and readable agent output. A person or agent
asking why Wyrd is built for real-time verification should reach the canonical
request → requirements/evidence → verdict → application decision → response
explanation, including latency components and asynchronous evidence retention.
Do not flatten it into a generic background evaluation feature list. Queued
and scheduled workflows remain discoverable additional uses. Keep fixed titles,
substantive content and natural wording; no new ranking guarantee or synonym
page is needed.

## Approach

1. Assemble the final published-page and query-intent map from Tasks 1–3.
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
spec's three fixed titles are honored. Heading order, landmarks and descriptive
links remain usable. No development host becomes the production canonical,
including when previews render the site locally.

**RED.** Extend existing SSR/content or shell regression owners with the
planned [metadata proof](#metadata-proof). Assert the actual page/head output,
not only frontmatter strings. Current missing canonical/social output and
generic title behavior should expose the gap. Confirm the test fails for that
observable difference rather than missing SvelteKit runtime stubs.

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
or external service. Keep Pagefind's current build step. Run Scenario 1 and
existing omission/draft/link checks as well as the new proof.

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
| REQ-007–008; AC-006–007; INV-001, INV-004 | Final corpus dispositions and rendered entry paths agree with actual published content; six-section navigation, current brand and keyboard/mobile use remain intact. No duplicate finance or concept procedure. |
| REQ-014; AC-013; INV-004–006 | Every agreed query family maps to substantive canonical coverage. Representative human/agent questions reach the right explanation, prerequisites and executable next step through public pages/indexes, not source archaeology. Include why a verdict before the response matters, how declared contracts guide the check, and how engine/evidence/provider latency differ from later publication. No keyword quota or live rank is substituted for this proof. |
| REQ-015; AC-014; INV-005–006 | Source attribution/credits/dates are truthful. Handoff identifies responsible maintainers for content, discovery checks, distribution and feedback without external posting or account changes. |
| REQ-016; AC-015; INV-005–006 | Evidence distinguishes the initial release set from completion of every required practice/tutorial. The handoff includes baseline/day-14/day-30 equal-window measurement, available data or explicit access gaps and responsible follow-up ownership. |

## Implementation Reference

- **Metadata/rendering:** `docs/src/routes/[...slug]/+page.svelte` renders
  `<svelte:head>`, article, H1 and actual commit date; `+page.ts::load` uses
  `getEntry` and `resolveArchetype`. Homepage has its own head; `+layout.svelte`
  owns main/header/nav/footer and mobile focus behavior. `content.test.ts` and
  `shell-links.test.ts` already SSR these owners with runtime stubs. Gap:
  required canonical/social/page-purpose output and any discovered semantic
  regressions. Extend these existing owners, not each Markdown page.
- **Published corpus:** `content.ts::allSlugs` / `getEntry` exclude drafts in
  production; `derived-nav.ts::inPrimaryNav` / `buildNav` derive six-section
  navigation. `+page.ts::entries` supplies native prerender entries and
  `svelte.config.js::stripDrafts` removes draft bodies. Gap: a sitemap/URL
  projection and cross-corpus metadata/link closure; reuse these definitions.
  Public docs assets live under `docs/public` and the existing base is /wyrd.
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
proof references from Tasks 1–3. State available Search Console and GitHub
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

- [Approved specification, revision 4](../spec.md).
- [Repository rules](../../../../AGENTS.md), [workflow](../../../spec-driven-development.md),
  [execution](../../../implementation-execution.md), and [testing](../../../../TESTING.md).
- [Docs rules](../../../../docs/AGENTS.md), [editorial owner](../../../../docs/PRODUCT.md),
  and [contribution rules](../../../../docs/CONTRIBUTING.md).
- [Wyrd product/public-surface authority](../../../../architecture/wyrd-design.md).
