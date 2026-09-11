---
id: TASK-015
title: Operator Card workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-015, REQ-016, REQ-017, REQ-080, REQ-081, REQ-084, REQ-085, REQ-117, REQ-127, REQ-128, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-016, INV-021, INV-022, AC-003, AC-008, AC-010, AC-011]
depends_on: [TASK-006]
parent_task:
remediates: []
---

# Outcome and value

Make an Operator's one declared reaction understandable from upstream signal
through Trigger to action target, budget, credentials reference, and separately
projected recent outcome.

# Owner and write set

- Own only `src/lib/features/cards/workspaces/operator/**`, its fixture
  projection, focused tests, and registration module.
- Render exactly one action and its variant-specific configuration, inbound
  Trigger/Eval/Drift context, execution budget, redacted credential reference,
  target link when one exists, and contextual outcome.
- Support Workflow, notification, and HTTP variants through a closed typed view
  union; the representative fixture need render only its declared variant.

# Locked decisions and non-goals

- Workflow actions link to Workflow Cards; notification/HTTP actions do not
  invent Card targets. Outcome never changes the declaration.
- No action execution/test button, secret value, editor, multi-action workflow,
  credential manager, or inferred success.

# Locked visual implementation authority

- Implement `C-14-light` and `C-14-dark` from
  [`cards.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards.svg)
  using the exact one-action hierarchy, readable input template, raw JSON
  disclosure, execution budget, inbound chain, redacted credential reference,
  target links, and separately projected recent outcome in the
  [C-14 ledger entry](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md#c-14--operator-card).
- Notify and HTTP are typed alternate fixture variants, not extra panels on the
  Workflow fixture. Narrow behavior follows `R-CARD`, with long templates
  contained inside their region. Do not substitute a generic action builder.
- Completion evidence must compare both desktop themes and a 390 × 844 capture,
  including proof that no secret value or execution control appears.

# Ordered test scenarios

1. A Workflow action shows readable input/template, raw disclosure, budget,
   redacted credential reference, and exact Workflow link.
2. Notification and HTTP variant models reject irrelevant configuration and do
   not display a fake Card target.
3. Trigger/upstream chain and recent outcome remain visibly distinct.
4. Secret-like fixture values cannot reach rendered page data.
5. Long templates remain contained and keyboard accessible in both themes and
   at narrow width.

# Red-Green-Refactor

Prove the closed action union and secret boundary first, then presentation.
Reuse disclosure and relationship primitives without a generic action engine.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/operator/OperatorWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record action variants, redaction, and relationship cases against C-14. Stop
before any execution or credential behavior is added.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
