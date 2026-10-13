---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-developer-docsite-rebuild
spec_revision: 2
requirements: [REQ-004, REQ-005, REQ-006, REQ-007, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-003, AC-004, AC-005, AC-006]
depends_on: [TASK-002]
parent_task:
remediates: []
---

## Outcome and Value

A developer can connect through supported Rust, Python, and TypeScript SDKs,
automate with CLI/HTTP/MCP, and use a supported gateway/framework path. The
Connect guides lead to source-backed API, SDK, tool, command, error, and limit
reference rather than duplicate contract prose.

## Owners, Scope, Consumers, and Prohibited Changes

Own Connect task content and the relevant Reference lookup pages. Wyrd's
server-owned wire contract remains the authority across languages. Gateway
and framework pages appear only when their actual path works. Do not invent
SDK parity, a hosted API explorer, separate gateway credentials/tenancy, or a
public route absent from the server. Do not alter Wyrd clients or gateway code
to make an example pass.
Use `$human-tech-docs` for every reader-facing guide and reference page,
including its Diátaxis, humanizer, and voice passes.

## Approach

1. Confirm currently supported SDK, CLI, HTTP, MCP, gateway, and framework
   entry points and the scopes they require.
2. Author goal-led connection and automation guides with complete examples
   and a visible success and rejection result.
3. Publish source-backed SDK, CLI, MCP, OpenAPI, error, and applicable limit
   lookup pages; link `GET /openapi.json` to its live contract.
4. Include gateway/framework procedures only where tested end to end, and
   integrate published pages with the sidebar, search, and landing paths.

## Proof Strategy

This task documents existing public surfaces and uses existing source
generation; it does not add runtime behavior. Run each published example in
its owning language or client surface against a real server, including a
material authorization or malformed-request result. Compare SDK/API/MCP/CLI
reference with owning declarations and the live OpenAPI document. Verify
gateway claims against actual gateway behavior, not a design page. Run
`mise run docs:check` and inspect the built site and generated output. No
manufactured RED applies.

## Acceptance Criteria

- The supported connection and automation guides have complete commands,
  expected outputs, required permissions, and a recovery path.
- Rust, Python, and TypeScript tabs are equivalent only where current SDK
  behavior is equivalent; missing support is stated at the point of use.
- Gateway and framework pages occupy Connect only when their path is
  executable; unsupported paths create no misleading primary-nav entry.
- OpenAPI, SDK APIs, CLI commands, MCP tools, stable errors, and documented
  limits are findable and sourced from the owning contracts.

## Expected Write Set and Consumer Closure

New Connect and Reference content under `docs/src/content/docs/`, existing
documentation generation and indexes as needed for publication. No SDK,
server, gateway, or protocol implementation write is expected.

## Verification and Evidence

Per-surface example transcript with supported failure, source/reference
comparison including live `/openapi.json`, `mise run docs:check`, and
rendered navigation/search/link inspection.

## Material Stop Conditions

Stop for a spec revision if the approved language-agnostic or gateway story
requires new Wyrd public behavior, or if a claimed supported route cannot be
honestly documented without changing its contract.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Testing workflow](../../../../architecture/references/languages/testing-workflows.md)

## MCP investigation slice evidence

The local-server-bootstrap MCP supplement contributed four draft pages:
Connect agent-through-MCP, Reference MCP tools, Observe custom Bifrost data,
and Verify retained evidence. This is a bounded contribution; it does not
complete the remaining SDK, observation, verification, or Connect task scope.

Source and real-server example proof live in the supplement at
`local-server-bootstrap-task-003/changes/active/local-server-bootstrap/mcp-investigation-supplement.md`.
All four pages follow human-tech-docs, passed five fresh reader questions,
and passed the humanizer lint. `mise run docs:check:commands`,
`mise run docs:linkcheck`, `mise run docs:build`, and `mise run docs:a11y`
passed. Because these pages retain draft:true, the published accessibility
lane checks zero draft pages; existing check_page checks also passed on all
four rendered dev previews. No page was published or marked task-complete.
