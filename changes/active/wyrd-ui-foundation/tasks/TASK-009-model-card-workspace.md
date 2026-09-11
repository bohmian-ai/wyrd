---
id: TASK-009
title: Model Card workspace
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

Make a Model Card useful for AI practitioners by leading with task, interface,
signature, artifacts, provenance, deployment relationships, and direct paths to
its observation, evaluation, and drift evidence.

# Owner and write set

- Own only `src/lib/features/cards/workspaces/model/**`, its fixture projection,
  focused tests, and registration module.
- Present the accepted C-04 information hierarchy: model purpose/task and
  interface first; signature and artifact/provenance next; deployment and
  verification/observation links after.
- Preserve exact Card references and version scope on every related resource.

# Locked decisions and non-goals

- Runtime measurements remain Observe data and link to canonical filtered pages.
- No model training/serving, artifact download pipeline, registry mutation,
  inferred lineage, generic ML experiment product, or placeholder Prompt block.

# Locked visual implementation authority

- Implement `C-04-light` and `C-04-dark` from
  [`cards.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards.svg)
  using the exact route, fixture, hierarchy, material states, and Card/Observe
  links in the [C-04 ledger entry](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md#c-04--model-card).
- Preserve the shared Card header and make purpose/task/interface, signature,
  artifacts, deployment relationships, and linked verification the accepted
  dominant-to-subordinate sequence. Keep the absent Prompt relationship
  explicit; do not invent a model dashboard or execution surface.
- Narrow behavior follows `R-CARD`: primary Model content precedes stacked
  metadata and relationships, with wide signatures contained inside their
  panel. Completion evidence must compare both desktop themes and a 390 × 844
  implementation capture to that rule.

# Ordered test scenarios

1. Model task/interface/signature and exact version identity lead the page.
2. Artifact, training Data/Experiment, and deployed Service relationships retain
   kind/name/version and direct routes.
3. Eval, Drift, Metrics, and Traces links preserve compatible Model/version scope.
4. Absent optional Prompt or runtime data is explicit and never shown as success.
5. Dense signatures remain accessible at narrow width and in both themes.

# Red-Green-Refactor

Drive each scenario with the focused component test; do not generalize model
sections until another real workspace shares the same behavior.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/model/ModelWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record relationship/link cases and paired comparison to C-04. Stop before
runtime or lineage facts are inferred from Card declarations in the browser.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
