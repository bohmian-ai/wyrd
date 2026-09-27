---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-canonical-evidence-thread-design-system
spec_revision: 3
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-029, REQ-030, REQ-031, INV-001, INV-002, INV-003, INV-004, INV-005, INV-009, INV-010, AC-001, AC-005, AC-009]
depends_on: []
parent_task:
remediates: []
---

## Outcome and Value

One production Evidence Thread identity, asset set, and semantic token authority can be consumed by the documentation and workbench. This task owns the brand-source, generated-projection, and locally shipped asset obligations in the mapped IDs; the two application tasks complete the consumer migration.

## Owners, Scope, Consumers, and Prohibited Changes

The existing `wyrd-ui/brand` directory owns the authored palette, generator, component contracts, design reference, and brand assets. `docs/` and `wyrd-ui/src/` consume its projections. Preserve the exact geometry of `docs/src/assets/wyrd-mark.svg`; recoloring is allowed. Preserve registry and built-component contract parity. Do not create a second palette, keep compatibility token aliases for a retired style, alter Wyrd domain contracts, or change public marketing sites.

## Approach

1. Reconcile the current brand source, generated CSS, component contracts, and rendered reference with the approved Evidence Thread roles and mark.
2. Replace the authored palette and type roles; extend the existing generator only as needed to produce both applications' projections and enforce used contrast pairs.
3. Ship Familjen Grotesk and Fragment Mono locally for both application builds and make the canonical mark available to both consumers.
4. Update the design reference and machine-readable component contracts to describe the new geometry, semantics, and state language.
5. Regenerate projections and confirm every projected value and asset is reproducible from the brand authority.

## Ordered Implementation Scenarios

### Scenario 1 — one source generates both theme projections

**Behavior.** The existing generator produces both applications' complete Evidence Thread modes from one palette, rejects drift and low-contrast used pairs, and emits no retired hard-shadow geometry or undocumented aliases (REQ-001, REQ-005, REQ-008, REQ-030–031, AC-001).

**RED.** Add a focused generator regression test for the changed output and rejection behavior; run `mise exec -- node --test brand/gen-theme.test.mjs` from `crates/wyrd/wyrd-server/wyrd-ui/` and confirm the expected missing-behavior failure.

**GREEN.** Adapt the existing generator only where required, update the palette, regenerate its projections, and rerun the focused test plus `mise run check:tokens`.

**REFACTOR.** Keep one source-to-projection path and remove obsolete generator branches while the focused test and token gate remain green.

## Acceptance Criteria

- The exact spec palette roles and light/dark values generate both app projections; no consumer owns an independent authored color list.
- Intended foreground/background pairs pass the existing contrast mechanism in both modes, including code and state treatments used by the applications.
- Brand references, logo variants, and favicon source preserve the README Wyrd mark geometry and product name.
- Both named fonts are bundled; neither production app requires a remote font request.
- Brand source, component contracts, built components, registry entries, and rendered reference agree after the application tasks; no retired shadow, arcade, or warm-paper contract remains.

## Expected Write Set and Consumer Closure

Likely: `crates/wyrd/wyrd-server/wyrd-ui/brand/` palette, generator, marks, contracts, and renders; generated `brand/theme.css`, `docs/src/styles/wyrd-tokens.css`, and existing skill projection; local font assets or package manifests and lockfiles in the two applications. The docs and workbench tasks own their CSS/component adoption. Paths guide ownership rather than limit adjacent changes.

## Verification and Evidence

- `mise run check:tokens` proves projection drift and configured token contrast.
- The palette values, mark geometry, local font assets, and rendered reference are static obligations: inspect them directly; no manufactured RED is required for those files.
- From `crates/wyrd/wyrd-server/wyrd-ui`: `mise exec -- pnpm exec vitest run src/lib/components/component-contracts.test.ts` and `mise exec -- pnpm exec vitest run src/lib/registry.test.ts` prove named contract and registry tests.
- Inspect built assets and network references for local fonts and mark geometry; record the generated source-to-consumer mapping and both-mode contrast output.
- After consumer tasks, `mise run docs:check` and both application builds verify integration.

## Material Stop Conditions

Stop for a spec revision if retaining the exact mark geometry, listed palette values, or one authored brand source proves incompatible with required behavior. New domain contracts, a public marketing redesign, or an application-owned palette are outside this task.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Design doctrine](../../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

## Implementation Evidence

Status: `IMPLEMENTED` — awaiting `$wyrd-task-review`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Exact spec roles and light/dark values generate both app projections; no consumer-authored palette | `brand/palette.json` (19 REQ-005 roles + `--declare-ink`, `--evidence-ink`, `--r`); `brand/gen-theme.mjs` `renderTargets` → `brand/theme.css`, `docs/src/styles/wyrd-tokens.css`, `.agents/skills/wyrd-ui/references/wyrd-theme.css` | `brand/gen-theme.test.mjs` "both application projections carry every approved role value in both modes"; `mise run check:tokens` (now runs the generator tests, then drift check) | PASS |
| Used pairs pass contrast in both modes, incl. code and state treatments | `palette.json` `pairs` (30 pairs: text, links, declare/observe soft fields, primary label, evidence label, status on surface/canvas/failure-soft, code) | `check:tokens` contrast assertion; test "a used pair below its contrast floor fails in the mode that breaks it" | PASS |
| Brand references, logo variants, favicon preserve README mark geometry and name | Generator owns the README paths and renders `brand/logo.svg`, `logo-light.svg`, `app-icon.svg`, `docs/src/assets/wyrd-mark.svg`, `docs/public/favicon.svg`; workbench wordmark `bohmian` → `Wyrd` | Test "every Wyrd mark rendering keeps the exact README geometry"; `renders/styleguide.html` headless render (both modes) | PASS |
| Both fonts bundled; no remote font request | `@fontsource/familjen-grotesk@5.3.0`, `@fontsource/fragment-mono@5.3.0` in workbench and docs; Google Fonts import removed from `src/app.css`; old faces removed from docs | Workbench `pnpm build` and docs build emit local `familjen-grotesk-*`/`fragment-mono-*` woff2; `grep googleapis\|gstatic` over build output: none | PASS |
| Brand source, contracts, built components, registry, rendered reference agree; no retired shadow/arcade/warm-paper contract | `components.json` rewritten for Evidence Thread; workbench components migrated off retired tokens and hard-offset shadows; `DESIGN.md`, `brand-skill.md`, wyrd-ui skill rewritten; `renders/` reduced to one token-driven `styleguide.html` | `vitest run src/lib/components/component-contracts.test.ts` (19), `vitest run src/lib/registry.test.ts` (4), `pnpm test` (171), `pnpm check` 0 errors/0 warnings, `pnpm build`, `mise run docs:check` | PASS (see limits) |
| Revision 3: Atom-aligned blue-charcoal dark values for every REQ-005 role, incl. declare/evidence ink and code syntax roles | `brand/palette.json` dark values; `brand/gen-theme.test.mjs` `ROLES` extended to all 25 spec rows; `brand/DESIGN.md` table and dark-mode guidance regenerated from the palette | `mise run check:tokens` (5 tests; all 34 pairs pass in both modes); `mise run check:skills-sync`; `mise run docs:check` (a11y AA light + dark, 61 pages); workbench `vitest` 171 and `pnpm check` 0/0 | PASS |

Commands (all passing): `mise exec -- node --test brand/gen-theme.test.mjs` (RED first: missing exports), `mise run check:tokens`, `mise exec -- pnpm exec vitest run src/lib/components/component-contracts.test.ts`, `mise exec -- pnpm exec vitest run src/lib/registry.test.ts`, `mise exec -- pnpm test`, `mise exec -- pnpm check`, `mise exec -- pnpm build` (workbench), `mise run docs:check`, `mise run check:skills-sync`, `git diff --check`.

Limits and handoffs:

- `--border` (Rule) is 2.0–2.4:1 on surface/canvas, below the 3:1 non-text floor. The values are spec-fixed; the pair is declared at `min: 2.0` and DESIGN.md "Rules are structure" requires every control to carry a label, fill, or text plus a `--declare` focus ring. TASK-006/TASK-002 must honor that for inputs.
- Docs consumer CSS (`docs/src/styles/arcade.css` and docs components) still references retired tokens and faces; TASK-002 owns that adoption. `docs/src/lib/shiki-theme.js` still hand-restates code colours; TASK-002 must derive it from the projection (INV-001).
- Workbench migration here is token-level only (renames, shadow removal, lime-as-accent removal, Panel geometry). Border widths, lift/sink motion, density, and state coverage remain TASK-006.
- `brand/renders/product/*.svg` (wyrd-ui-foundation route mocks) keep their old visuals; their README now marks visual treatment superseded. The old `renders/index.html`, `workbench.html`, `landing.html`, `renders.css` were removed; the `wyrd-ui-foundation` packet still names them.
- Workbench lockfile write re-resolved its `latest` specifiers (vite 8.3.0, vitest 5.0.1, svelte 5.57.1 patch bumps).

Non-goals held: no domain/API/route/auth changes, no public marketing site changes, no compatibility aliases (generator rejects `var()` alias values and undocumented tokens).
