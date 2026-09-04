---
id: TASK-010
title: Agent Card workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-015, REQ-016, REQ-017, REQ-018, REQ-080, REQ-081, REQ-084, REQ-085, REQ-111, REQ-127, REQ-128, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-016, INV-021, INV-022, AC-003, AC-008, AC-010, AC-011]
depends_on: [TASK-006]
parent_task:
remediates: []
---

# Outcome and value

Give AI engineers a focused Agent definition workspace where the registered
Prompt is the primary dependency and tools, run limits, publication targets,
Services, and observation paths remain understandable in context.

# Owner and write set

- Own only `src/lib/features/cards/workspaces/agent/**`, its fixture projection,
  focused tests, and registration module.
- Render Prompt identity/version/provider/model/message-variable summaries with
  read-only contextual inspection and a direct Prompt Card link.
- Show Prompt → Agent → runtime-tool composition, run limits, publication
  targets, containing Services, and filtered Traces/Evaluations links.

# Locked decisions and non-goals

- Prompt inspection reuses one Card reference; it never copies Prompt identity.
- Tools remain Skald runtime registrations, not Card kinds.
- No agent playground, execution, tool registry editor, secret values, inferred
  runtime activity, or separate Agent route tree.

# Ordered test scenarios

1. Prompt is the primary dependency and its inspection sheet retains Agent
   context plus a direct Card route.
2. Tools, run limits, publication targets, and Service relationships preserve
   their distinct meanings and exact references.
3. Trace/Eval links preserve Agent, Service, version, and time scope when known.
4. Missing optional artifacts/runtime state is explicit.
5. Contextual inspection becomes a usable full-width sheet at narrow width and
   remains accessible in both themes.

# Red-Green-Refactor

Implement Prompt inspection before secondary sections. Reuse disclosure/sheet
primitives; keep Agent composition semantics local.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/agent/AgentWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record Prompt inspection, relationship, and mobile behavior against C-07/M-09.
Stop before an execution contract or Tool Card is invented.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
