---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-canonical-evidence-thread-design-system
spec_revision: 3
requirements: [REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-025, REQ-026, REQ-027, REQ-028, REQ-029, REQ-031, REQ-032, REQ-033, REQ-034, REQ-042, REQ-044, INV-001, INV-002, INV-003, INV-004, INV-005, INV-007, INV-008, INV-009, INV-010, INV-011, INV-012, AC-002, AC-003, AC-004, AC-005, AC-006, AC-008, AC-009, AC-010, AC-011, AC-012, AC-014, AC-015]
depends_on: [TASK-001]
parent_task:
remediates: []
---

## Outcome and Value

A new developer lands on the Wyrd docsite, understands Declare → Observe → Verify, runs the supported local server and client path, and sees how to check success. The site provides one clear first action, task-first navigation, search, readable code, and contextual routes into deeper journeys. This task establishes the docs presentation and content pattern that TASK-003–005 fill out.

## Owners, Scope, Consumers, and Prohibited Changes

`docs/` owns its SvelteKit shell, content, frontmatter-derived navigation, search, code rendering, generated docs links, and static build. Consume TASK-001's brand projection. Keep existing public docs URLs and search engine working. Preserve current Wyrd doctrine and headless/API paths. Do not create role gateways, an exhaustive first-level feature inventory, a parallel palette, fictional commands, or new server behavior.

## Approach

1. Audit the existing docs home, local setup, navigation, code blocks, and generated reference links against the approved spec and current source.
2. Make the home and first journey task-led, showing the shortest supported route to a running server and a verifiable result.
3. Adapt the shell, theme bootstrap, sidebar, search, contents, code presentation, and mobile controls to Evidence Thread.
4. Establish one compact journey inventory tying every REQ-033–041 question to its destination, verified capability status, and proof owner; TASK-003–005 complete their entries.
5. Remove superseded docs styling and verify the page, navigation, and build in both modes and at narrow widths.

## Ordered Implementation Scenarios

### Scenario 1 — correct docs theme on first paint

**Behavior.** A first visit follows the system color preference; an explicit choice persists, applies before meaningful paint, and leaves native controls and content equivalent (REQ-010–012, AC-004).

**RED.** Add a focused test for the existing docs theme/bootstrap owner; run `mise exec -- pnpm exec vitest run src/lib/theme.test.ts` from `docs/` and confirm the expected failure.

**GREEN.** Adapt the current bootstrap and control using TASK-001 tokens; rerun the focused test and prior docs tests.

**REFACTOR.** Keep one presentation-state path and remove obsolete theme handling while the test stays green.

### Scenario 2 — a developer finds the first useful task

**Behavior.** Primary navigation exposes a short task path, visible search and reference access, and a current-location cue without requiring role or product selection (REQ-013–014, REQ-019, REQ-033–034, AC-015).

**RED.** Extend the current nav test to assert the task-first first level and active location; run `mise exec -- pnpm exec vitest run src/lib/derived-nav.test.ts` from `docs/` and confirm the expected failure.

**GREEN.** Update the existing content-driven navigation and shell; rerun this test and the theme test.

**REFACTOR.** Simplify frontmatter-to-navigation mapping where the finished task path makes it clearer, preserving existing URL behavior.

### Scenario 3 — code stays readable and copyable

**Behavior.** A labeled, highlighted sample is keyboard-copyable with immediate success feedback and horizontal overflow in both modes (REQ-017, AC-003).

**RED.** Add the smallest code-block interaction test to the docs suite; run `mise exec -- pnpm exec vitest run src/lib/mdsvex/CodeBlock.test.ts` from `docs/` and confirm the missing behavior.

**GREEN.** Adapt the existing Shiki and code-block presentation; rerun all three scenario tests.

**REFACTOR.** Retain the existing renderer and remove only styling or state it supersedes.

## Acceptance Criteria

- The home answers what Wyrd does and points to one obvious local start; the local guide reaches a real health/readiness result and client connection with development-only limits visible.
- The first-level navigation remains small and task-first; all required journeys have a discoverable destination, while planned material does not appear as a completed task.
- A reader can finish the local path without reading unrelated reference pages; optional decisions appear where needed and required prerequisites or warnings stay visible.
- Search, sidebar/current location, contents, theme, code copying, direct reference links, and machine-readable indexes remain usable at 320px and 200% zoom.
- The docs home and local task page render the canonical mark and tokens in both modes; no arcade skin, old fonts, old mark, or remote font loading remains.

## Expected Write Set and Consumer Closure

Likely `docs/src/routes/`, `docs/src/lib/derived-nav.ts`, existing docs components, `docs/src/styles/`, `docs/src/app.html`, overview/get-started/local-development content, and adjacent Vitest tests. Generated docs remain generator-owned. Other journey pages belong to TASK-003–005 and should plug into this site's navigation pattern without rewriting its shell.

## Verification and Evidence

- Run the three focused scenario commands in order, then `mise run docs:svelte:check` and `mise run docs:check` for generated content, commands, links, build/search, and accessibility.
- Execute the local setup and health/readiness path with repository-managed commands; compare each visible command and result to current source, tests, and build outputs.
- Review home and local guide in light/dark at desktop and 320px; keyboard-check search, navigation, theme, and copy; inspect 200% zoom and reduced motion.
- Record the initial journey inventory and first-time developer walkthrough; TASK-003–005 close its remaining entries. Combine docs renderings with TASK-006 for AC-014 after integration.

## Material Stop Conditions

Return to spec authority if the required docs path needs changed server/API contracts, a different search platform, or a new public URL policy. If a local step is not shipped, label the exact gap rather than inventing it.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Design doctrine](../../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

## Implementation Evidence

Status: `BLOCKED` on REQ-034. The local guide cannot yet reach a real server health/readiness result and client connection: a fresh server does not boot without `bifrost.peer.api_key`, which only a running server can issue. The owner is refactoring server startup. Everything else below is in place for review once that lands.

Revision 3 reconciliation: the packet now cites revision 3. The only revision 3 change in TASK-002's scope is the REQ-005 dark palette, which docs consume only through the generated `docs/src/styles/wyrd-tokens.css`. `git grep` over `docs/src` finds no literal hex or `rgb()` colour outside that projection. The added full-screen dark review (AC-002) is recorded below.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Home says what Wyrd does and points to one obvious local start | `docs/src/routes/+page.svelte`: one primary action, the Declare → Observe → Verify spine, and three next tasks. The hint now states what the linked page actually delivers (a DataCard example) instead of a running server | Screenshots `d-home-{light,dark}-{1440,320}` | PASS |
| Local guide reaches real health/readiness and a client connection, with dev-only limits visible | Not done; the current `get-started` page runs the local DataCard example only | Blocked (server boot) | FAIL |
| First level small and task-first; required journeys discoverable; planned work not shown as done | `derived-nav.ts` task-first sections. `Sidebar.svelte`: a topic's index page is linked from its subhead rather than repeated under it (removes "Overview / Overview", "Tutorials / Tutorials", "How-to / How-to", "Bifrost / Bifrost"). `overview/index.svx`: the kind count is corrected and next steps are task-first | `vitest run src/lib/derived-nav.test.ts`; screenshot `d-overview-dark-1440` | PASS (journey inventory for REQ-033–041 not yet written) |
| Search, sidebar/current location, contents, theme, copy, reference links, and llms indexes usable at 320px and 200% zoom | `docs.css`: the search label stays on one line and truncates at narrow widths (it wrapped to two lines at 320px); heading anchors no longer render H1s as indigo underlined links | `vitest` (44 tests incl. `theme.test.ts`, `derived-nav.test.ts`, `mdsvex/CodeBlock.test.ts`); `mise run docs:check` (build, pagefind 61 pages, a11y AA light + dark); screenshots at 320px | PASS (200% zoom and keyboard walkthrough not re-run this pass) |
| Canonical mark and tokens in both modes; no arcade skin, old fonts, old mark, or remote fonts | Generated tokens and mark only. The home hint moved from mono to sans (DESIGN.md: prose is never mono) | `mise run check:tokens`; `mise run docs:check` | PASS |
| AC-002 (rev 3): full-screen dark review with prose, metadata, borders, and code visible together | Home, local guide (code blocks visible), and overview at 1440×900 and 320px, in both modes, from the built site | Headless Chrome captures; found and fixed four defects (H1 link styling, duplicated nav labels, wrapped search label, mono prose). No glare or stark code blocks in dark: code surface `#262B33` sits on canvas `#282C34`, delimited by a 1px rule | PASS (docs). The workbench full-screen review belongs to TASK-006 |

Commands: `mise exec -- pnpm exec vitest run src/lib/theme.test.ts src/lib/derived-nav.test.ts src/lib/mdsvex/CodeBlock.test.ts` (31 pass), `mise exec -- pnpm exec vitest run` in `docs/` (44 pass), `mise run docs:svelte:check` (0/0), `mise run docs:check`, `mise run check:tokens`, `mise run check:skills-sync`, `git diff --check`.

Non-goals held: no server/API change, no new search platform, no URL changes, no docs-only palette.
