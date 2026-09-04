---
id: TASK-014
title: Data Card workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-015, REQ-016, REQ-017, REQ-018, REQ-080, REQ-081, REQ-084, REQ-085, REQ-127, REQ-128, REQ-131, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-016, INV-021, INV-022, AC-003, AC-010, AC-011]
depends_on: [TASK-006]
parent_task:
remediates: []
---

# Outcome and value

Give data scientists a focused Data definition view for schema, profiles,
distributions, splits, targets, lineage, and direct analytical investigation.

# Owner and write set

- Own only `src/lib/features/cards/workspaces/data/**`, its fixture projection,
  focused tests, and registration module.
- Present identity/source and schema first, then authorized profile/distribution,
  splits/targets, lineage, related Drift, and Query links.
- Use accessible tables and shared chart frames for real profiled values only.

# Locked decisions and non-goals

- Profiles are server projections with freshness/sample context; absent or
  unauthorized data is not zero and is not inferred from schema.
- No data preview unless explicitly present and authorized, upload/editor,
  profiling computation, SQL generation, or browser-derived lineage.

# Ordered test scenarios

1. Schema preserves names, types, nullability, roles, and exact Data version.
2. Profiles/distributions expose sample and freshness and truthful unavailable
   states; splits/targets remain distinct from schema fields.
3. Lineage and Drift links retain exact Card references; Query handoff carries
   only supported catalog context.
4. Wide schema tables and charts remain accessible at narrow width/both themes.

# Red-Green-Refactor

Drive schema before optional profiles. Reuse table/chart primitives; keep Data
meaning and ordering local.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/data/DataWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record schema/profile/link cases against C-03. Stop before data access,
profiling, or query semantics are invented in the browser.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
