# Real-time verification positioning and docsite rollout

| | |
|---|---|
| Change ID | `SPEC-positioning` |
| Completed | 2026-11-18 |
| Reviewed target | `cd1e682e4fe7684d77f29542571d8f510c1027f1` (`docs`) |
| Approved specification | revision 7, approved 2026-11-18 |
| Delivery reference | not supplied |
| Completion authority | **human override**: the maintainer approved completion without a final `$wyrd-change-review` on 2026-11-18 |

## Completion authority

No integrated change review ran. Per-task reviews passed for TASK-001 (with
remediation R1, R2) and TASK-003 (remediation R1–R3, final R3 `PASS` at
`908a16946`). TASK-002, TASK-002-R1 and TASK-004 were implemented with recorded
evidence but no independent task review. The integrated run of every affected
SDK/example journey that the spec assigns to change review was not executed as
one pass; each task ran its own journeys, and TASK-004 ran the full
`mise run docs:check` and `check:examples` at `e0f71fbfc`.

## Intent and value

Wyrd's README and docs led with infrastructure before showing why a running
application would verify anything. This change makes Wyrd's public entry
points lead with one opening, "The only real-time verification system built
for your applications. You built the system. How do you know it's behaving
correctly?", and gives a reader arriving from GitHub, search or an agent a
complete path: understand verification, run a real one, apply it with sound
practices, and find the next step. The business goal is qualified discovery
and Wyrd's team as a trusted source on verification for AI applications.

## Shipped behavior

- **Entry points.** README, homepage and Introduction show the exact opening,
  then the application's decision and working proof before infrastructure.
  The homepage figure renders lines of the journey-tested finance API, its
  three requirements, the release-or-hold decision and asynchronous evidence
  capture; it is complete in server HTML before any motion.
- **Primary Quickstart.** `/wyrd/get-started/quickstart/` runs the finance
  release example in Python, Rust and TypeScript: one Service, one Verifier
  `release-check` with three requirements (recipient authorized and
  destination restricted as assertions; policy compliance judged by a gateway
  Agent reading the extracted sharing-policy PDF), one synchronous `verify`
  per proposed release, release only on `passed`. A policy change publishes a
  new attributable Verifier version without retargeting the old one.
- **Education.** Understand opens with Foundations of verification
  (tool-independent: verification versus evaluation, testing, validation,
  proof and runtime checking), then Real-time verification (the category
  concept: deadlines, evidence and check time, uncertain results, when to
  check later), then How verification works in Wyrd (the product mechanics),
  and a Technical questions FAQ.
- **Practices.** Build and verify guides: Write verifiable requirements,
  Establish sufficient evidence, Act on verdicts in production.
- **Tutorials.** Each runs in all three SDKs against a real server: Verify
  during a request (web API releases only on `passed`), Verify an agent's
  workflow (a trace check catches a skipped lookup behind an identical reply;
  no trace is `inconclusive`), Check on a schedule (the server scheduler judges
  windows `passed`, `failed`, `inconclusive`), Investigate a failed
  verification (failed requirement, judged evidence, exact versions).
- **SDK additions (INV-001, revisions 5–6).** Typed input models and verdict
  predicates in every SDK. Python and TypeScript `state.configure()` installs
  Wyrd-managed telemetry by default (`start_telemetry` is no longer public;
  TypeScript `configure` is asynchronous); Rust keeps
  `wyrd_ai::otel::start_telemetry`. `flush` succeeds without a Bifrost writer;
  the first write starts the writer, so `verify` and `submit` never need
  `start_bifrost`. A `Trace` field marker declares a `trace` slot with one
  name in every SDK. These are breaking changes with no compatibility alias.
- **Discovery.** One head owner (`PageHead.svelte`) renders page-specific
  browser title, description, canonical URL on `https://docs.bohmian.ai/wyrd/`
  and Open Graph/Twitter tags; optional `browserTitle` frontmatter carries the
  agreed titles ("Wyrd | Real-Time Verification for AI Applications",
  "Real-Time Verification for AI Applications | Wyrd", "Real-Time
  Verification: Technical Questions") without changing headings. A prerendered
  `sitemap.xml` lists exactly the production pages with commit dates; the
  Pages deploy writes a domain-root `robots.txt` naming it. Drafts are absent
  from routes, navigation, search, sitemap and `llms.txt`/`llms-full.txt`.

## Lasting invariants

- Wyrd returns a judgment; the application owns the decision. Only `passed`
  permits the action; failed, inconclusive and errors stay distinct and hold.
- A missing trace never proves a step was skipped; judgments cite original
  evidence and exact versions; a synchronous verdict is not a promise of
  immediate query visibility.
- Docs keep the brand, the six sections (Get started, Build and verify,
  Connect, Operate, Understand, Reference), existing URLs under `/wyrd`, and
  WCAG 2.2 AA. Copyable code comes only from journey-tested example files.
- Discovery stays honest: no hidden keywords, AI-specific schema, new
  crawler, analytics dependency or ranking claim. Canonical and sitemap URLs
  always use the production origin from `docs/site.js`.
- No proof, no publication: an unproven procedure stays a draft.

## Material decisions

- Reuse the existing finance example as the one onboarding story rather than
  a new one; the support-desk example remains README's supplementary path.
- Teach verification independently of Wyrd before explaining Wyrd
  (TASK-002-R1), so the education pages serve readers who have not chosen a
  tool.
- Browser titles live in page frontmatter, not a slug map, so each page owns
  its search title with no parallel inventory.
- Overlapping docsite work transferred from `SPEC-docsite-incremental-reset`
  revision 9 (recorded in that packet's `dispositions.md`); the Operators page
  `respond-to-a-failed-verification` stays a draft owned there.
- Rust keeps explicit telemetry startup because its client tier must not
  depend on OpenTelemetry (revision 7).

## Revisions and deviations

Revisions 3–7 were maintainer-approved (2026-11-12, 11-13, 11-15, 11-18 ×2):
revision 3 widened the change to the full rollout; 4 published docs with the
change and removed live-provider gates; 5 kept the typed input models and
verdict predicates; 6 added REQ-017 for the SDK lifecycle and `Trace` marker;
7 dropped `configure`/`flush` from the Rust agent program. Deviation: the
final integrated change review was waived by the maintainer.

## Acceptance closure

| Acceptance | Evidence |
|---|---|
| AC-001–005 | Finance journeys in Rust (`examples/tests/finance_release.rs`), Python (`test_docsite_quickstart.py`), TypeScript (`finance-release.test.ts`): pass, each independent failure, uncertainty held, policy versions attributable |
| AC-006–007 | `readme.test.ts`, `shell-links.test.ts`, `indexes.test.ts`; public-surface dispositions recorded in TASK-001/002 |
| AC-008 | `content.test.ts` page metadata tests, `indexes.test.ts` sitemap test, fixture-build `withdrawn pages leave every production discovery surface`; `docs:check` pass (106 tests, build, links, a11y 42 pages) |
| AC-009–010, AC-012 | Source-checked claim ledger and fresh-reader passes on the education, practice and FAQ pages |
| AC-011 | Real-server journeys per tutorial in all three SDKs (request, agent workflow, schedule, investigation) |
| AC-013 | Query map and a fresh-agent retrieval walkthrough over built pages, `llms.txt`, sitemap and search; repairs applied |
| AC-014–015 | Commit-history dates only; publication order (initial set, then skipped-step tutorial, then remaining corpus); 14-day-window measurement procedure with GitHub traffic baseline (2026-11-04 to 11-17: 126 views, 38 unique) and Search Console access gap; owner for content, discovery and distribution: Thorrester |
| AC-016 | SDK unit tests for `Trace` slots, default telemetry install and refusals, frozen configure, writerless `flush`; agent-workflow and scheduled-check journeys without `start_bifrost` |

Follow-up owned by the maintainer: verify `docs.bohmian.ai` in Search Console,
submit the sitemap after deploy, and run the day-14 and day-30 reviews.

## Links

- Architecture: [wyrd-design.md](../../../architecture/wyrd-design.md)
  (managed telemetry and the `Trace` marker).
- Docs rules and editorial owner: [docs/AGENTS.md](../../../docs/AGENTS.md),
  [docs/PRODUCT.md](../../../docs/PRODUCT.md),
  [docs/CONTRIBUTING.md](../../../docs/CONTRIBUTING.md).
- Docs site owners: `docs/src/lib/components/PageHead.svelte`,
  `docs/src/routes/sitemap.xml/+server.ts`, `docs/site.js`,
  `docs/scripts/generate_llms_txt.py`, `.github/workflows/publish-docs.yml`.
- Examples: `examples/finance-release/`, `examples/refund-agent/`,
  `examples/scheduled-check/`.
- Related: [SDK authoring ergonomics](sdk-authoring-ergonomics.md),
  [Card authoring](card-authoring.md); `SPEC-service-identity-and-telemetry`
  (revision 17) and `SPEC-docsite-incremental-reset` remain active.
