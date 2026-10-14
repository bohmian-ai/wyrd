---
id: SPEC-bifrost-errors
revision: 1
status: approved
---

# Bifrost caller errors and live-table subqueries

## Objective and user value

A caller whose SQL is wrong gets the planner's message, including its suggestion, as a 4xx error on every surface. Subqueries over live tables return the same rows as the equivalent join.

## Approval

Approved by the human owner on 2026-10-14 as part of SPEC-example-journey-fixes
r6. Split into its own change by owner direction on 2026-10-14 so each problem
is reviewed and verified on its own. The requirement text is carried over
unchanged.

## Required behavior

### REQ-002 Bifrost caller errors

- A query that fails planning because of the caller's SQL (unknown column,
  table, or function; type mismatch; ambiguous reference) returns a 4xx
  `WyrdError` whose detail carries the planner's message (including its
  suggestion such as "Did you mean 'vr.verdict'?"), consistently for single
  table queries, joins, and subqueries, over HTTP, SDK, and MCP.
- Internal failures remain 5xx and never expose internals. Messages never
  reveal tables or columns the caller is not authorized to see.

### REQ-006 Live-table subqueries

- Scalar, `IN`, and `EXISTS` subqueries over live (not yet compacted) Bifrost
  tables succeed and return the same rows as the equivalent join, including
  correlated predicates on `run_id`, with tenant isolation preserved.

## Invariants

- INV-001 Tenant isolation holds on every changed path.

## Acceptance obligations

- AC-001 Each REQ has a user-journey test (Rust, Python, and TypeScript where
  the surface ships in all three; MCP/HTTP where agent-facing) that fails on
  the base and passes on the candidate, booting the server the way users do
  where the defect was harness-masked (REQ-003 uses the official TypeScript
  MCP SDK; REQ-007 drives the CLI).
- AC-002 Negative flows: Jev invalid question type and upstream 400/429/5xx;
  Bifrost unauthorized-table errors leak nothing; malformed `traceparent`.
- AC-003 Format, lints, codegen check, and the owning `mise` lanes pass.

## Open material decisions

None.
