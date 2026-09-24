---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-canonical-evidence-thread-design-system
spec_revision: 2
requirements: [REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027, REQ-028, REQ-029, REQ-030, REQ-031, REQ-032, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-009, INV-010, AC-002, AC-003, AC-004, AC-005, AC-007, AC-008, AC-009, AC-014]
depends_on: [TASK-001]
parent_task:
remediates: []
---

## Outcome and Value

Existing workbench routes use the same Evidence Thread identity as the developer docsite at an operating density. A user can scan declarations, observations, verifiers, judgments, and retained evidence in either theme with clear status cues. This task completes the workbench half of the shared visual contract without changing server truth.

## Owners, Scope, Consumers, and Prohibited Changes

`crates/wyrd/wyrd-server/wyrd-ui` owns the SvelteKit shell, mode state, app chrome, component catalog/registry, and existing feature routes. Consume TASK-001's brand projections. Preserve BFF contracts, authentication, tenant checks, route structure, server-owned status/judgment/lineage, and the catalog trust boundary. Evidence connectors use real projected relationships only. Do not add browser calculations of domain truth, a decorative record, or a parallel theme.

## Approach

1. Align current mode initialization and persistence with system preference and pre-paint parity.
2. Migrate shell, controls, primitives, and feature pages to generated Evidence Thread tokens, local fonts, and canonical mark.
3. Establish one dominant region on representative evidence/verification pages with real stage, state, and provenance cues.
4. Bring component contracts, built components, registry tests, and rendered brand reference into agreement as old variants are removed.
5. Verify full state coverage, responsive behavior, accessibility, both modes, and retirement of old styling.

## Ordered Implementation Scenarios

### Scenario 1 — workbench mode follows preference

**Behavior.** Without an explicit selection, the workbench paints in the system-preferred mode. An explicit choice persists and paints correctly on return without changing principal, tenant, or server state (REQ-010–012, INV-005, AC-004).

**RED.** Add a focused test for the existing mode owner and first-paint handoff; run `mise exec -- pnpm exec vitest run src/lib/theme.test.ts` from `crates/wyrd/wyrd-server/wyrd-ui/` and confirm the expected failure.

**GREEN.** Update the current mode/bootstrap behavior and preserve equal mode structure; rerun the focused test and existing component tests.

**REFACTOR.** Keep mode state local to presentation and reuse the current provider rather than introduce a second mode path.

### Scenario 2 — evidence and state remain understandable

**Behavior.** A failed verification view distinguishes Declaration, Observation, Verifier, Judgment, and retained Evidence; stage and status have distinct text/icon cues, and missing or unauthorized states remain explicit (REQ-021–024, INV-004, INV-006, AC-007).

**RED.** Add the smallest presentation assertion to the current feature test owner; run `mise exec -- pnpm exec vitest run src/lib/features/changes/ChangesJourney.test.ts` from `crates/wyrd/wyrd-server/wyrd-ui/` and confirm the expected missing-cue or hierarchy failure.

**GREEN.** Adapt the existing feature presentation using server-projected fields; rerun this test and the mode test.

**REFACTOR.** Remove duplicate old-style treatments while retaining the current feature owner and data flow.

## Acceptance Criteria

- The shell and representative evidence/verification pages render complete light and dark modes with canonical mark, type, semantic colors, ruled geometry, and neutral canvas.
- Shared primitives and touched routes cover normal, hover, focus, active, selected, disabled, loading, empty, unauthorized, warning, failure, and success states without color-only meaning.
- Workbench density approaches the spec's 30px title, 14px body, 11–12px metadata, 40px desktop control, and 52px evidence row targets; functional text remains at least 11px.
- Narrow layouts preserve content, navigation, actions, and appropriate touch targets through stacking or controlled overflow.
- The workbench has no production reference to retired fonts, parent-company mark, warm-paper palette, hard-shadow/five-pixel styling, or remote font imports; component contracts and registry stay consistent.

## Expected Write Set and Consumer Closure

Likely workbench `src/app.css`, app bootstrap/mode owner, `src/lib/components/`, `src/lib/components/app/`, selected feature/route presentation, and adjacent tests. TASK-001 owns brand source and generated tokens. Server handlers and public API contracts are outside this task.

## Verification and Evidence

- Run scenario-focused commands, then from the workbench directory `mise exec -- pnpm test`, `mise exec -- pnpm check`, and `mise exec -- pnpm build`; run `mise run check:tokens` for final projection drift.
- Run existing parity owners from the workbench directory: `mise exec -- pnpm exec vitest run src/lib/components/component-contracts.test.ts` and `mise exec -- pnpm exec vitest run src/lib/registry.test.ts`.
- Inspect desktop and narrow evidence/verification pages in both modes for keyboard access, 200% zoom, reduced motion, focus, contrast, state cues, and local assets. Combine these with TASK-002 docs renderings for AC-014 after integration.

## Material Stop Conditions

Stop if styling requires browser-derived judgments or lineage, new domain/API fields, a route or component trust-boundary change, or a departure from the approved palette, mark, or canonical brand owner.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Design doctrine](../../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
