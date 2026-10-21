---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-docsite-incremental-reset
spec_revision: 7
requirements: [REQ-001, REQ-002, REQ-004, REQ-005, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-003, AC-004, AC-005, AC-006]
depends_on: [TASK-002]
parent_task:
remediates: []
---

## Outcome and Value

Readers and agents discover only content with appropriate publication evidence.
Every one of the 32 baseline pages has a disposition, and withdrawal works
consistently across production routes, navigation, landing actions, search,
related links, and both machine indexes. Missing pages and deferred subjects
do not break the retained site.

## Owners, Scope, Consumers, and Prohibited Changes

Use `$wyrd-implement`. Own the corpus audit and publication closure, not a
rewrite of every deferred guide. TASK-002 supplies the three completed
onboarding pages and their evidence. Apply the spec's disposition table to the
remaining 29 pages. Use drafts for useful deferred source; `planned` alone is
not withdrawal. Bounded corrections and trimming are allowed; substantial
rewrites become concise follow-up outcomes and the page is withdrawn.

Keep the six sections, current maximum navigation depth, approved visual
system, SvelteKit/mdsvex/Pagefind/Shiki, accessibility, and both themes. No new
content platform, publication registry, generalized loader, full API catalog,
external application dependency, or restoration of deleted articles.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Production publication and navigation | `docs/src/lib/content.ts`: `getEntry`, `allSlugs`; `derived-nav.ts`: `buildNav`, `firstPage` | `docs/src/routes/[...slug]/+page.ts`: `entries`, `load`; `derived-nav.test.ts`; `shell-links.test.ts` | Draft exclusion exists but source imports and all discovery consumers must agree after withdrawal | Extend the existing content/nav/route path only where fixture or build proof shows a gap | No separate page registry or duplicate navigation owner |
| Machine indexes with missing pages | `docs/scripts/generate_llms_txt.py`: `sidebar_pages`, `render_start`, `render_llms_full` | `docs/src/lib/indexes.test.ts`; `docs/scripts/page_text.mjs` | `render_start` indexes missing titles and always links the errors page | Make existing generation tolerate absent/draft onboarding and reference destinations | No replacement generator or frontmatter engine |
| Valid published destinations | `docs/scripts/check_links.py`: `local_target_exists`, `markdown_pages` | `docs/svelte.config.js`: base rewriting and prerender errors; `shell-links.test.ts` | Source existence does not establish that a linked page is published | Extend existing link validation for publication state and retain built-link proof | No parallel link crawler; test the live property in its owner |
| Source independence including drafts | `content.ts`: eager content glob; `docs/svelte.config.js`: `injectMdsvexComponents`; `+page.server.ts`: direct application YAML import | `highlight.js`: `embedCodeFromFile`; `page-text.test.ts`; application journeys under the Python SDK | Draft modules still compile, and the landing imports the support-desk application | Remove documentation dependency on application-owned files, including compiled draft embeds; reuse local snippets from TASK-001 | No external fetch, mirror, sync, or new example framework |
| Search contains published content | `docs/package.json`: build followed by Pagefind; `docs/src/lib/components/Search.svelte`: `ensure`, `run`, `resultHref` | Catch-all prerender entries and layout search/index markup | Current unit tests alone do not prove search withdrawal | Prove the production output and actual built Pagefind results for withdrawn fixture content | Reuse existing search/build; no second indexer |

## Approach

1. Complete one compact disposition record covering all 32 exact baseline
   paths. Each row records reason, evidence, and final production state; reuse
   TASK-002's three rows. Include deferred outcomes without adding tasks.
2. Review retained concepts against doctrine/Bifrost/security authorities and
   references against their owning contracts, configuration, runtime MCP
   catalog, or generator. Exercise retained procedures at their documented
   versions, prerequisites, and failure/recovery paths; withdraw unproven ones.
3. Close missing-page and publication-link gaps through the scenarios below.
   Update old content assertions when their approved documentation contract
   changes, preserving the runtime properties they previously supported.
4. Apply withdrawals and consumer link cleanup, including the landing,
   related links, compiled draft imports, and machine-readable output. Preserve
   useful application tests independently of the docsite.
5. Verify production routes/search and visual/accessibility regressions, then
   reconcile the disposition record with actual generated and built output.

## Ordered Implementation Scenarios

New selectors below are planned additions to inspected existing Vitest targets;
they do not prescribe private fixture structure. Use an isolated corpus/build
so omission testing never edits the working content destructively.

### Scenario 1 — Missing onboarding and reference pages are valid

**Behavior.** With Setup, Quickstart, and the error reference omitted or draft,
both machine indexes generate without exceptions or missing destinations.
An empty corpus also remains a valid landing-only site (REQ-005, AC-004).

**RED.** Add `tolerates omitted onboarding and reference pages` under the
existing `machine-readable indexes follow published pages` group in
`docs/src/lib/indexes.test.ts`. Exercise the real generator with the reduced
corpus; expect current missing-title or stale-link failure. Run:

```bash
mise exec -- pnpm --dir docs exec vitest run src/lib/indexes.test.ts -t '^machine-readable indexes follow published pages tolerates omitted onboarding and reference pages$'
```

**GREEN.** Extend the current generator and its existing publication selection
to emit only valid destinations. Preserve full-corpus ordering, code embedding,
and publication filtering. Run this command and the current index target.

**REFACTOR.** Remove duplicated missing-page assumptions within the current
owner; do not build a new content inventory service or generalized parser.

### Scenario 2 — A source file is not sufficient for a published link

**Behavior.** A published page linking to a draft fails validation even when
the file exists. Valid published pages, served assets, deploy-base links, and
same-page anchors remain accepted; deferred source does not become public to
satisfy validation (REQ-005, INV-004, AC-004).

**RED.** Add `rejects published links to withdrawn content` under a
`publication closure` group in `docs/src/lib/indexes.test.ts`, invoking the
existing link checker against the relevant corpus. Expect its current
existence-only acceptance to fail the assertion. Run:

```bash
mise exec -- pnpm --dir docs exec vitest run src/lib/indexes.test.ts -t '^publication closure rejects published links to withdrawn content$'
```

**GREEN.** Extend the existing checker to enforce valid production
destinations, retaining its meaningful missing-target checks. Rerun Scenario 1
and this exact command; repair published links rather than suppressing errors.

**REFACTOR.** Reuse the existing route/frontmatter conventions and remove
duplicate per-consumer workarounds while preserving validation.

### Scenario 3 — Withdrawal closes every production discovery surface

**Behavior.** Withdrawn fixture content has no production page, nav or landing
action, search result, related link, or machine-index entry. Empty sections
stay hidden. Build/render/test needs no application checkout, even when useful
draft source is preserved (REQ-004/005, AC-003/004/006).

**RED.** Add `withdrawn pages leave every production discovery surface` under
`publication closure` in the existing `docs/src/routes/shell-links.test.ts`
target. Include actual production-output and Pagefind evidence rather than
only asserting source frontmatter or a dev-mode content map. Establish current
stale-link, generation, or application-import failures. Run:

```bash
mise exec -- pnpm --dir docs exec vitest run src/routes/shell-links.test.ts -t '^publication closure withdrawn pages leave every production discovery surface$'
```

**GREEN.** Apply the corpus dispositions and close actual gaps through the
existing publication, landing, link, embed and search owners. Replace
documentation-owned dependence on application examples with the independently
owned source where needed. Rerun all three exact commands and existing
navigation/shell/index regression targets.

**REFACTOR.** Keep one publication decision per existing owner; remove
superseded documentation assertions and imports without removing useful
application behavior tests or weakening any required property.

## Acceptance Criteria

- Exactly the 32 baseline pages have evidence-backed dispositions, including
  the onboarding rows; production state agrees with every row.
- Conditional retention is earned with the evidence class in REQ-002. A
  substantial rewrite or missing proof causes withdrawal and a concise
  deferred outcome, not scope expansion or a runnable warning label.
- Production discovery and all published links contain only valid published
  destinations. Omitted onboarding/reference and empty-section fixtures pass.
- The site and machine-index generation are independent of full application
  examples, including eagerly compiled drafts and the landing import path.
- Useful Python application journeys remain runnable. MCP catalog claims need
  runtime catalog proof; generated schemas alone do not certify them.
- Existing visuals, responsive layouts, keyboard behavior, both themes, and
  accessibility meet INV-001 and the current docs gates.

## Expected Write Set and Consumer Closure

Content frontmatter and bounded content/link edits; one concise disposition
record under this packet; existing content/nav/route/landing/embed/generator/
link-check owners only as needed; associated tests; generated API and machine
indexes through their sources. No server/SDK/CLI semantics or large guide
rewrites. The record location and private production/test organization remain
implementation-owned.

## Verification and Evidence

Run `mise run docs:install` before the three exact selectors above. For
iteration use the existing focused targets:

```bash
mise exec -- pnpm --dir docs exec vitest run src/lib/derived-nav.test.ts src/lib/indexes.test.ts src/routes/shell-links.test.ts
```

After the audit and source edits, prove preservation of the existing app
journey with its confirmed selector (the historical name is retained):

```bash
mise run py:setup
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_support_desk.py::test_published_quickstart_declares_observes_and_verifies
```

Record final production route and actual built Pagefind results for the fixture
and final corpus. Audit all outgoing discovery links and remove stale content
from generated output; a source-only or dev-mode assertion is insufficient.
Select exact existing procedure/catalog tests only after matching their actual
instructions and versions; record those commands and results per retained
page. If proof is unavailable, withdraw the page. No new procedure capability
is required to retain the rest of the corpus.

```bash
mise run docs:svelte:check
mise run docs:check
git diff --check
```

Regenerate only through `mise run docs:generate`. Respect the docs aggregate's
ordinary committed-output drift check. Manually review keyboard, responsive,
both-theme, and landing/first-path behavior. Run Python format/lint lanes if
Python sources change; check the selected sample with pinned Ruff as needed.
The new publication-aware link property extends an existing check because
types and source existence cannot prove it; do not add a second checker.

## Material Stop Conditions

A desired retained page requires changed product semantics or a substantial
rewrite: withdraw and record follow-up. Stop for authority only if the required
publication behavior cannot be achieved without a platform redesign, changed
brand/navigation commitment, weakened gate, or another prohibited boundary.
Choosing draft instead of conditional retention is explicitly in scope.

## Authority Links

- [Approved spec revision 7 and exact disposition table](../spec.md)
- [Repository standards](../../../../AGENTS.md) and [agent rules](../../../../AGENTS.md)
- [Protocol](../../../../architecture/wyrd-design.md) and [doctrine](../../../../architecture/wyrd-design.md)
- [Bifrost authority](../../../../architecture/bifrost-design.md)
- [Security authority](../../../../architecture/wyrd-security-posture.md)
- [Operations authority](../../../../architecture/operations/README.md)
- [Implementation execution](../../../../changes/implementation-execution.md)
- [Testing workflows](../../../../TESTING.md)

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 32 pages have evidence-backed dispositions matching production | `../dispositions.md` (32 rows); 16 pages `draft: true` | Built routes: 16 pages + landing; a11y checks 16 pages | PASS |
| Conditional retention earned; unproven procedures withdrawn | 8 conditional procedures withdrawn; configure-verification-runtime, 5 concepts, 7 references retained and trimmed to Setup-page density per user direction | Source review per row; runtime/catalog tests below | PASS |
| Only valid published destinations; omitted onboarding/reference and empty sections pass | `generate_llms_txt.py` `START`/`render_start`; `check_links.py` `target_page`/`is_draft` | Scenario 1 and 2 selectors pass; `docs:linkcheck` passes on final corpus | PASS |
| Site and indexes independent of application examples, including drafts and landing | `svelte.config.js` `stripDrafts`; landing Verifier inline in `+page.server.ts`; published pages embed only `examples/quickstart` | Scenario 3 builds a fixture with no `examples/` beside it, a draft embedding a missing app file, and real Pagefind search (`zebracorn` → none; published token → its page) | PASS |
| Python application journey stays runnable; MCP catalog runtime proof | No application files changed | `test_support_desk.py::test_published_quickstart_declares_observes_and_verifies` 1 passed; `discovery::pg_tests::agent_discovers_only_authorized_tables_and_layout` passed | PASS |
| Visuals, a11y, themes intact | No style/layout changes; two landing blurbs reworded | `docs:check` (96 tests, build, a11y AA light+dark) passes; keyboard/responsive manual review not performed | PASS (manual review open) |

Commands: the three exact Scenario selectors; `mise run docs:check`; `mise run docs:svelte:check` (0 errors);
`cargo nextest run -p wyrd-server --lib -E 'test(=config::tests::verification_limits_env_applies_and_refuses) | test(=verification::hosted::tests::memory_ceilings_cap_live_workers)'`;
under `with-test-postgres.sh`: `-p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=discovery::pg_tests::agent_discovers_only_authorized_tables_and_layout)'` and
`-p wyrd-server --test integration --run-ignored=all -E 'test(=sandbox_worker::sandbox_bounds_workers_and_waiters) | test(=pg_hosted_artifacts::capacity_refusal_is_busy_at_every_hosted_consumer) | test(=pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees)'`; `git diff --check`.

Limits: a draft's frontmatter (title, description) still compiles into the client content map; its body does not. The landing Verifier is now page-owned and no longer backed by an application test. Two page-text tests that pinned withdrawn record-a-run content were removed; the range-embed test now uses Quickstart source. Non-goals held: no server/SDK/CLI changes, no new registry, loader, or crawler. Uncommitted `examples/quickstart/quickstart.rs`, `sdks/wyrd-sdk-rust/*`, and `get-started/setup.md` edits predate this task and are not part of it.
