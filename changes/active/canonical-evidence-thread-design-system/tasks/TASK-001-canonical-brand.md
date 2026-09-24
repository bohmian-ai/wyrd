---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-canonical-evidence-thread-design-system
spec_revision: 2
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
