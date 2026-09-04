---
id: TASK-006
title: Card inventory shared shell and workspace host
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-003, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-080, REQ-081, REQ-092, REQ-095, REQ-100, REQ-101, REQ-127, REQ-128, REQ-129, REQ-130, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-016, INV-021, INV-022, AC-001, AC-003, AC-010, AC-011]
depends_on: [TASK-003]
parent_task:
remediates: []
---

# Outcome and value

Create Card discovery and one stable `/cards/[uid]` host so all focused Card
workspaces can be implemented independently and in parallel without duplicating
routes, shell behavior, or a central kind switch.

# Owner and write set

- Own `/t/[tenantKey]/cards`, `/cards/[uid]`, `src/lib/features/cards/core/**`,
  Card view-model envelopes, and the workspace discovery seam.
- Inventory supports URL-backed lookup plus kind, label, status, and authorized
  Space filters. Detail owns identity, exact version, metadata, relationships,
  common navigation, and typed Spec fallback.
- Define one small typed workspace module contract discovered at build time from
  `src/lib/features/cards/workspaces/*/index.ts`; later workspace tasks add only
  their own folder and tests. Duplicate kind registration fails deterministically.
- Implement the smaller Workflow and Verifier specialized presentations here;
  all other unregistered specializations use the typed Spec fallback.

# Locked decisions and non-goals

- Workspace modules provide presentation, local URL-state parsing, and typed
  fixture projection only. They do not own Card identity, routing, auth, or IO.
- Use SvelteKit/Vite capabilities already installed. No plugin runtime, dynamic
  remote import, schema renderer, per-kind route, editor, or central switch that
  every parallel task must modify.
- Relationships and status arrive from the server projection; the browser does
  not infer lifecycle, lineage, or execution.

# Ordered test scenarios

1. Inventory restores filters, covers all registrable fixture kinds, and handles
   empty/unauthorized/error states.
2. Generic detail renders exact identity/version/metadata/relationships/Spec.
3. Workspace discovery selects one local module and rejects duplicate kinds.
4. An absent workspace uses the generic fallback; Workflow and Verifier render
   their earned presentation within the same shell.
5. Long tables/specs and relationship content retain information at narrow width
   in both themes.

# Red-Green-Refactor

Prove inventory, shared detail, then discovery. Keep the registration contract
minimal; do not add extension features that no planned workspace needs.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardsInventory.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/workspace-registry.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record the filter matrix, fallback behavior, workspace registration proof, and
responsive shell captures. Stop if parallel workspace additions require route
or core-host edits; repair the seam without turning it into a runtime plugin API.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
