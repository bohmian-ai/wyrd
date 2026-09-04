---
id: TASK-008
title: Experiment Card analytical workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-015, REQ-016, REQ-017, REQ-080, REQ-081, REQ-084, REQ-085, REQ-118, REQ-119, REQ-120, REQ-121, REQ-122, REQ-127, REQ-128, REQ-131, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-017, INV-018, INV-021, INV-022, AC-003, AC-009, AC-010, AC-011]
depends_on: [TASK-006]
parent_task:
remediates: []
---

# Outcome and value

Give the primary Data Scientist / AI Engineer persona a complete Card-local
workspace for understanding an Experiment definition, its runs, comparisons,
outputs, and immutable Card versions.

# Owner and write set

- Own only `src/lib/features/cards/workspaces/experiment/**`, its typed mock
  projection, tests, and registration module.
- Implement Overview, Runs, Compare, Outputs, and Versions with all selection,
  filtering, sorting, section, output, comparison, and version state in the URL.
- Cover model-training and agentic run inspection, lifecycle/partial output,
  explicit compatible comparison and baseline, and Metrics/Tables/Visuals/Files/
  registered Artifact outputs with producer and lineage preserved.

# Locked decisions and non-goals

- Experiment Card version, run identity/lifecycle, and output identity remain
  distinct. A run is not a Card and never mutates the declaration version.
- Metric deltas remain neutral unless explicit authoritative directionality is
  selected. Files are not Artifact Cards.
- No run launcher, run persistence/query contract, notebook, experiment tracker,
  browser aggregation, or claim that current servers already implement fixtures.

# Ordered test scenarios

1. Overview and zero-run state preserve the declaration without inventing zero
   results or an execution action.
2. Runs restore filters/selection and render queued, running/partial, completed,
   failed, cancelled, model, and agentic inspection states.
3. Compare enforces compatibility, explicit baseline, neutral deltas, aligned
   recorded steps, provenance, and output differences.
4. Outputs preserve producing run and distinguish metrics, tables, visuals,
   files, and registered Artifact lineage.
5. Versions preserve immutable declaration identity and run association.
6. Narrow and both-theme states retain all URL state and technical evidence.

# Red-Green-Refactor

Implement one local destination at a time in the order above. Share generic
primitives only through TASK-002; keep Experiment run/output semantics local.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/experiment/ExperimentWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/experiment/experiment-url-state.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record the state/output ledger and paired comparison to
`brand/renders/product/cards/experiment/`, explicitly retaining its deferred
contract gaps. Stop before inventing durable run, output, or execution behavior.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
