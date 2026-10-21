---
id: TASK-001
kind: implementation
status: superseded
spec: SPEC-developer-docsite-rebuild
spec_revision: 2
requirements: [REQ-001, REQ-006, REQ-007, REQ-008, REQ-014, REQ-015, INV-001, INV-002, INV-003, INV-005, AC-001, AC-004, AC-005, AC-006]
depends_on: []
parent_task:
remediates: []
---

## Supersession — 2026-10-20

Remaining documentation work is owned by
[SPEC-docsite-incremental-reset revision 2](../../docsite-incremental-reset/spec.md)
and its four tasks. Preserve this task's historical implementation evidence;
this status does not assert completion or revoke the retained brand, platform,
navigation, or accessibility commitments.

## Outcome and Value

The landing page and overall documentation shell form a complete, navigable
site even while every old content page stays deleted. The approved visual
system remains intact; the landing's words, examples, actions, and the shell's
links accurately reflect what readers can reach now.

## Owners, Scope, Consumers, and Prohibited Changes

`docs/` owns the landing, shared layout, navigation, search, content metadata,
generated indexes, and documentation checks. Preserve the current landing
artwork, including its CoverScene, alongside the approved Wyrd mark, tokens,
fonts, light/dark behavior, and visual language. Do not
restore the old page corpus, publish placeholders, change Wyrd product
behavior, or create a new theme, search service, or documentation platform.
Use `$human-tech-docs` for all landing and shell prose, including its
Diátaxis, humanizer, and voice passes.

## Approach

1. Keep the current landing artwork and make its value statement, example,
   and actions truthful and useful for the currently published site.
2. Bring the header, sidebar, responsive shell, page templates, search, code
   controls, anchors, dates, and tables into the approved reading experience.
3. Make navigation, generated indexes, and documentation checks follow the
   pages actually published rather than recreating deleted articles.
4. Review the built landing and shell in both themes and at narrow widths.

## Ordered Implementation Scenarios

### Scenario 1 — published pages define navigation

**Behavior.** A landing-only build exposes no empty sidebar sections or dead
content routes; adding a published page places it under the approved section
and no deeper than section → topic → page. Planned and draft pages do not
appear as runnable tasks.

**RED.** Update the existing navigation regression for this behavior and run
`cd docs && mise exec -- pnpm exec vitest run src/lib/derived-nav.test.ts`;
expect the old six-section assumptions or missing-page behavior to fail.

**GREEN.** Make the smallest navigation/content change that passes this and
previously valid navigation behavior.

**REFACTOR.** Keep one published-content source for the sidebar and pagination;
remove obsolete hardcoded section assumptions while tests stay green.

### Scenario 2 — landing and shell actions reach real destinations

**Behavior.** Every visible internal action in the landing, header, footer,
sidebar, and related navigation resolves in the built site. The landing has a
clear first-success action when that page exists, and no unmeasured time or
simulated-verdict claim before it exists.

**RED.** Add a focused regression for rendered internal destinations and run
`cd docs && mise exec -- pnpm test`; it must fail on the current links to
deleted routes.

**GREEN.** Update the copy and destinations within the current visual system;
rerun the new regression and navigation test.

**REFACTOR.** Keep link decisions tied to published content so later tasks do
not reintroduce dead shell links.

### Scenario 3 — page metadata gives a real update date

**Behavior.** A published content page can show a date tied to a real content
change, never the build time, while a landing-only build remains valid.

**RED.** Add a focused metadata/rendering regression and run
`cd docs && mise exec -- pnpm exec vitest run src/lib/content.test.ts`;
expect the unsupported update-date behavior to fail.

**GREEN.** Supply the minimum content contract and display behavior; rerun
the focused check and the earlier shell checks.

**REFACTOR.** Keep date provenance understandable and avoid a second authored
source when existing repository metadata can supply it.

### Scenario 4 — the documentation gate respects the empty corpus

**Behavior.** A normal docs check succeeds without recreating the deleted
legacy pages; published pages alone determine searchable and machine-readable
indexes.

**RED.** Add a focused regression for generation from the current published
corpus and run `cd docs && mise exec -- pnpm test`; expect the existing
generators to recreate removed reference routes or advertise stale content.

**GREEN.** Reconcile generation and checks with incremental publication; run
the focused suite, `mise run docs:check`, and earlier shell checks.

**REFACTOR.** Retain source-derived reference generation when its pages are
published, without a parallel catalog of obsolete routes.

## Acceptance Criteria

- Landing, shell, and footer contain no internal dead ends or unverified claims.
- The sidebar follows only published content and the approved shallow map;
  search and machine-readable indexes agree with it.
- Search, code copy, heading anchors, responsive navigation, tables, and real
  update dates work for published content; empty-state behavior is clear.
- Both themes and keyboard use retain the approved brand and WCAG 2.2 AA
  contract; the current landing artwork remains in place and the deleted page
  set stays deleted.

## Expected Write Set and Consumer Closure

Likely `docs/src/routes/`, `docs/src/lib/`, `docs/src/styles/`, `docs/scripts/`,
`docs/public/`, and the docs tasks in `mise.toml` if the existing gate assumes
the old corpus. The canonical brand source is a constraint, not a write target.

## Verification and Evidence

Run both focused commands above, `cd docs && mise exec -- pnpm test`,
`mise run docs:svelte:check`, and `mise run docs:check`. Inspect the built site
in light/dark and desktop/mobile modes, keyboard-operate navigation/search/
copy, follow all landing/shell actions, and confirm the deleted content files
were not restored. Static visual and content claims need direct inspection;
TDD applies only to the changed executable shell behavior.

## Material Stop Conditions

Stop for a spec revision if preserving the approved brand or landing visual
language conflicts with an explicit reader-facing requirement, or if the
landing needs to promise an unshipped Wyrd capability to be useful.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Design doctrine](../../../../architecture/wyrd-design.md)
- [Testing workflow](../../../../TESTING.md)

## Implementation Evidence

Status: `IMPLEMENTED` (uncommitted working tree on `docsite-vcc`).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Landing, shell, and footer contain no internal dead ends or unverified claims | `docs/src/routes/+page.svelte` (doors and quickstart render only for published sections; install box, simulated "Verification passed" verdict, and time claims removed; copy limited to shipped behavior), `+layout.svelte` and `+error.svelte` (Search, Reference, and Get started links render only when a published page exists; error page always offers Docs home) | `docs/src/routes/shell-links.test.ts` server-renders landing, home shell, doc shell, and error page and requires every `/wyrd/...` link to be a published route or served file; browser: empty-corpus landing links only to `/wyrd/`, `/wyrd/llms.txt`, and GitHub; 404 offers Docs home only | PASS |
| Sidebar follows only published content and the approved shallow map; search and indexes agree | `docs/src/lib/content.ts` (`SECTIONS`, `TOPICS`, section/topic validation), `derived-nav.ts` (`buildNav`, `firstPage`, `locate`, `siblings`), `Sidebar.svelte`, `scripts/generate_llms_txt.py` (sidebar order, published pages only), `generate_card_docs.py`/`generate_api_docs.py` (refresh existing pages only), `check_a11y.py` (exact published set) | `derived-nav.test.ts` (21), `content.test.ts` (24), `src/lib/indexes.test.ts` (llms links resolve; llms.txt page order equals sidebar order); Pagefind indexes 1 page on the empty corpus and 3 with probe pages | PASS |
| Search, code copy, anchors, responsive nav, tables, and real update dates work; empty state is clear | `docs/src/lib/updated.js` (`remarkUpdated` sets `updated` from `git log -1 --format=%cs`, never build time), `[...slug]/+page.svelte` (Last updated), `svelte.config.js`, publish workflow `fetch-depth: 0`, landing empty-state paragraph, `+layout.svelte` Escape closes the drawer and refocuses its toggle, landing grid tracks `minmax(0, 1fr)` | `content.test.ts` page-update-date and rendering tests; browser with three temporary probe pages: Cmd+K search and Esc, heading anchor, Copy focusable, code and table scroll inside their boxes at 390px, drawer opens by keyboard and Escape closes it with focus returned, uncommitted probe shows no date; probes then deleted | PASS |
| Both themes and keyboard keep the brand and WCAG 2.2 AA; artwork in place; deleted pages stay deleted | CoverScene, tokens, fonts, and theme script unchanged; landing styles kept except removed install/verdict rules | `mise run docs:check` a11y lane: contrast AA verified (light + dark); browser light/dark at desktop and 390px; `git status`: 64 staged deletions under `docs/src/content/docs`, no restored or untracked content | PASS |

Commands run (all passing on the empty corpus):

```sh
cd docs && mise exec -- pnpm exec vitest run src/lib/derived-nav.test.ts   # 21 passed
cd docs && mise exec -- pnpm exec vitest run src/lib/content.test.ts       # 24 passed
cd docs && mise exec -- pnpm test                                          # 9 files, 73 passed
mise run docs:svelte:check                                                 # 0 errors, 0 warnings
mise run docs:check                                                        # drift, unit tests, commands, links, build, a11y OK
git diff --check                                                           # clean
```

`docs:check` ran with a temporary copy of the git index in which the
regenerated `docs/public/llms*.txt` were staged, because its drift check
compares the working tree to the index and those files are uncommitted. The
user's index was not modified.

`mise.toml` gained a hidden `docs:test` task (run by `docs:check`), and the
drift check now also fails on untracked generated files.

### Diagnoses

- **Unseen catch-all route.** Symptom: `pnpm build` failed with "routes marked
  as prerenderable ... not prerendered: /[...slug]". Cause: with no content,
  `entries()` returns no slugs, so SvelteKit reports the catch-all as unseen.
  Fix site: `svelte.config.js` `prerender.handleUnseenRoutes` tolerates only
  `/[...slug]`; any other unseen route still fails. `check_a11y.py` fails if a
  published page is missing from the build, so a dropped page cannot pass
  silently.
- **Generators recreated deleted pages.** Symptom: `mise run docs:generate`
  wrote the deleted card and API pages back as untracked files, which
  `git diff --exit-code` did not catch. Cause: the generators wrote every page
  unconditionally. Fix site: `generate_card_docs.py` and
  `generate_api_docs.py` refresh only pages that already exist; the drift
  check now also rejects untracked generated output.
- **Landing overflow at 390px.** Symptom: page scroll width 732px at a 390px
  viewport. Cause: `.qs` grid tracks were `1fr` (minimum = content width), so
  the unwrapped YAML widened the column. Fix site: landing grid tracks use
  `minmax(0, 1fr)`; the code block now scrolls inside its box.

### Non-goals and risks

- No content pages were written or restored; non-goals stayed excluded.
- The landing CTA, quickstart, section doors, search, and Reference link stay
  hidden until TASK-002 and later tasks publish pages; until then the landing
  points to the repository.
- Update dates need full git history; the deploy checkout uses
  `fetch-depth: 0`, and a shallow clone shows no date rather than a wrong one.
