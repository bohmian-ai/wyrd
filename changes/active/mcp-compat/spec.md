---
id: SPEC-mcp-compat
revision: 1
status: approved
---

# Wyrd MCP works with common MCP clients

## Objective and user value

The official TypeScript MCP SDK and the OpenAI Agents SDK connect to `/mcp`, list and convert every tool, and call a read tool.

## Approval

Approved by the human owner on 2026-10-14 as part of SPEC-example-journey-fixes
r6. Split into its own change by owner direction on 2026-10-14 so each problem
is reviewed and verified on its own. The requirement text is carried over
unchanged.

## Required behavior

### REQ-003 MCP compatibility

- `tools/list` results satisfy MCP protocol revision 2026-07-28, including
  `ttlMs` and `cacheScope` (already implemented on the change branch).
- Every advertised tool, read and write, has an input schema accepted by
  common tool-calling consumers: a top-level `type: object` with no top-level
  `oneOf`, `anyOf`, `allOf`, `enum`, `const`, or `not` (OpenAI function tool
  rules), and output schemas accepted by the official MCP SDK.
- The official TypeScript MCP SDK and the OpenAI Agents SDK can connect to
  `/mcp`, list the full catalog for an administrator, convert every tool, and
  call a read tool.

## Invariants

- INV-001 Tenant isolation holds on every changed path.

## Acceptance obligations

- AC-001 Each REQ has a user-journey test (Rust, Python, and TypeScript where
  the surface ships in all three; MCP/HTTP where agent-facing) that fails on
  the base and passes on the candidate, booting the server the way users do
  where the defect was harness-masked (REQ-003 uses the official TypeScript
  MCP SDK; REQ-007 drives the CLI).
- AC-003 Format, lints, codegen check, and the owning `mise` lanes pass.

## Open material decisions

None.
