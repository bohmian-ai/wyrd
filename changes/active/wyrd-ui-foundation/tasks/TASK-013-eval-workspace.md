---
id: TASK-013
title: Eval Card and Observe workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-004, REQ-015, REQ-016, REQ-017, REQ-018, REQ-080, REQ-081, REQ-084, REQ-085, REQ-086, REQ-087, REQ-089, REQ-090, REQ-114, REQ-127, REQ-128, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-016, INV-021, INV-022, AC-001, AC-003, AC-008, AC-010, AC-011]
depends_on: [TASK-005, TASK-006]
parent_task:
remediates: []
---

# Outcome and value

Let AI practitioners inspect an Eval's authored workflow/task definition and
then investigate one real Eval event through a shareable workflow/task view,
without turning tasks into resources or mixing definition with result.

# Owner and write set

- Own `src/lib/features/cards/workspaces/eval/**`,
  `src/lib/features/observe/eval/**`, `/observe/evaluations` and
  `/observe/evaluations/[recordId]`, projections, tests, and registration.
- Card view exposes dataset/source, ordered/dependent stages, gate, task summary,
  contextual task/workflow definition, and canonical results link.
- Observe inventory uses `record_id`; detail shows resolved Eval Card, subject,
  lifecycle/pass summary, stages, and selected task through URL `task=<taskId>`.

# Locked decisions and non-goals

- `run_id` is correlation, never event identity. Workflow and task panes are
  views of one event, never new route hierarchies.
- Actual values render only when authorized/retained. The browser never computes
  scores, verdicts, pass state, or workflow execution.
- No Eval execution, durable event query contract, task route, or result stored
  on the Card.

# Locked visual implementation authority

- Implement `C-10-light` and `C-10-dark` from
  [`cards.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards.svg)
  for definition inspection, and `O-08`/`O-09` from
  [`observe.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/observe.svg)
  for event inventory and event/workflow/task inspection. The
  [ledger](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md#observesvg)
  locks URL-restored selection, lifecycle and verdict states, authorization
  redaction, selected-task detail, links, and missing/error states.
- [`golden-O-09.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/golden-O-09.svg)
  is the immutable Gate A reference for the large workflow drawer, stage/task
  navigation, dominant selected-task inspection, scrim, spacing, and hierarchy.
- Implement `M-06-light` and `M-06-dark` from
  [`mobile.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/mobile.svg):
  Workflow and Task remain two views of one event, selected task remains in the
  URL, and correlation/technical detail stacks after the result summary.
- Do not substitute a narrow detail rail, standalone task routes, or a generic
  table/detail page. Completion evidence must compare `C-10`, `O-08`, `O-09`,
  and `M-06` in both themes at their declared viewports.

# Ordered test scenarios

1. Card workflow/task declaration inspection retains the surrounding Eval Card
   and links to canonical results.
2. Inventory filters events and navigates by stable `record_id`.
3. Detail restores selected task and shows type/status/stage/operator/expected,
   authorized actual, score, timing, explanation, and related trace.
4. Passed, failed, error, skipped, no-data, unauthorized, and safe error states
   remain distinct and server-authored.
5. Card/subject/trace links preserve scope; narrow and both-theme layouts retain
   workflow/task context.

# Red-Green-Refactor

Prove identity and authorization boundaries first, then Card definition,
inventory, and event detail. Reuse Observe filters and shared data primitives.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/eval/EvalWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/eval/EvalJourney.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/eval/eval-identity.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record definition/result, event/run identity, and authorization evidence against
C-10/O-08/O-09/M-06. Stop before introducing task resources or browser verdicts.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
