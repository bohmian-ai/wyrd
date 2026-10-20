---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-052]
invariants: [INV-005, INV-006, INV-008]
acceptance: [AC-001, AC-008]
depends_on: [TASK-001]
parent_task: null
remediates: []
---

# TASK-002 — Compact analytics-workbench shell

## Outcome and Value

Authenticated tenant pages use the approved compact labeled rail and responsive overlay, leaving maximum width for analytical content and establishing the visual baseline for later slices.

## Source-Backed Reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Shell/layout | `src/lib/components/app/Shell.svelte::{entries,current,trail,closeMenu}` | tenant `+layout.svelte`; `Shell.test.ts` tests `exactly six tenant-relative destinations...` and `multiple tenants...` | 212/60px preference rail, text initials, Settings in primary list, no Verify | Modify the existing shell/navigation model to the approved fixed 80–88px labeled rail | None |
| SSR preference | tenant layout and `wyrd-nav` cookie consumers | shell test and layout server load | Obsolete expand/collapse state | Remove only the rail preference path and its callers | Deletion |
| Visual direction | `references/canvas/canvas.json`; current brand theme/components | all tenant pages | Canvas horizontal artboard navigation is not production shell authority | Apply canvas density/content intent inside the approved left-rail decision | None |

## Owners, Scope, Consumers, and Prohibited Changes

- Owners: existing shell, tenant layout inputs, shared shell styles/tests.
- Consumers: every authenticated route, tenant/principal controls, Changes and Settings.
- Prohibited: horizontal global navigation, a second workspace selector in the header, DreamCanvas runtime markup, hidden icon-only navigation, or page-local tabs in the rail.

## Approach

1. Replace the collapsible preference with the fixed compact desktop rail and visible icon+label entries.
2. Add Verify; anchor Settings below the primary group.
3. Keep tenant, principal, theme/help, and route context in the utility header.
4. Preserve skip navigation, current-location semantics, Escape/focus restoration, WCAG AA, and reduced motion.
5. Verify unclipped desktop content and mobile overlay, then pause for human visual acceptance.

## Ordered Implementation Scenarios

### Scenario 1 — Desktop rail and utility header

**Behavior.** Home, Cards, Observe, Verify, Changes, and Query are visible labeled primary destinations in an 80–88px rail; Settings is bottom-anchored; the header contains utilities rather than duplicate navigation.

**RED.** Update existing test `exactly six tenant-relative destinations and static single-tenant identity` to the approved destinations/structure. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/app/Shell.test.ts -t 'exactly six tenant-relative destinations and static single-tenant identity'`.

**GREEN.** Modify the existing shell and remove the obsolete preference inputs; rerun the exact test.

**REFACTOR.** Remove dead collapse-cookie markup/styles while keeping route derivation in its current owner.

### Scenario 2 — Responsive keyboard navigation

**Behavior.** Narrow widths use a keyboard-operable overlay that closes with Escape, restores focus, announces current location, and does not cover active content after dismissal.

**RED.** Extend existing test `multiple tenants have searchable switching and accessible mobile navigation`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/app/Shell.test.ts -t 'multiple tenants have searchable switching and accessible mobile navigation'`.

**GREEN.** Adapt the existing `navOpen`, `menuButton`, and `closeMenu` behavior; rerun both shell tests.

**REFACTOR.** Keep one navigation entry source for desktop and mobile.

## Acceptance Criteria

- AC-001 passes at agreed desktop/mobile widths and by keyboard.
- Primary tables/editors/charts retain usable width with no shell-caused clipping.
- Human desktop/mobile evidence is accepted before TASK-003; this visual proof is non-TDD and is recorded with task evidence.

## Expected Write Set and Consumer Closure

- Likely: `Shell.svelte`, `Shell.test.ts`, tenant layout/server input and shared CSS where the removed preference is consumed.
- Closure: every nav route, tenant switching, logout, theme, breadcrumbs, skip link, responsive and reduced-motion styles.

## Verification and Evidence

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/app/Shell.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run build
```

## Material Stop Conditions

- A supported route cannot fit the approved navigation hierarchy without changing the specification.
- Accessibility requires a shell interaction conflicting with the approved fixed rail.
- Human visual acceptance is withheld.

## Authority Links

- [Approved specification](../spec.md)
- [Canvas reference](../references/canvas/canvas.json)
- [Repository rules](../../../../AGENTS.md)

