---
id: TASK-004
kind: implementation
status: superseded
spec: SPEC-developer-docsite-rebuild
spec_revision: 2
requirements: [REQ-003, REQ-005, REQ-006, REQ-007, REQ-010, REQ-011, REQ-013, REQ-014, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-003, AC-004, AC-005, AC-006]
depends_on: [TASK-002]
parent_task:
remediates: []
---

## Supersession — 2026-10-20

Remaining documentation work is owned by
[SPEC-docsite-incremental-reset revision 2](../../docsite-incremental-reset/spec.md)
and its four tasks. Preserve this task's historical implementation evidence;
this status does not assert completion or revoke the retained brand, platform,
navigation, or accessibility commitments.

## Outcome and Value

A reader can record a run and observations for the declared service, find the
retained evidence, and use supported telemetry or custom Bifrost data paths.
The Observe guides explain what the recorded facts mean and where Bifrost fits.

## Owners, Scope, Consumers, and Prohibited Changes

Own Observe task content, the runs/observations/Bifrost explanations it needs,
and relevant Bifrost lookup links. Continue the service example from TASK-002.
Keep Bifrost architecture in Understand and write/query procedures in guides.
Do not expose Gate, Scribe, Oracle, or Forge as separate public clients, imply
cross-tenant data access, or claim an unshipped ingest/query mode works.
Use `$human-tech-docs` for every reader-facing guide, explanation, and
reference page, including its Diátaxis, humanizer, and voice passes.

## Approach

1. Confirm supported native observation, telemetry, and Bifrost write/query
   surfaces and their authentication and lifecycle constraints.
2. Author the smallest complete record → flush/shutdown → read procedure.
3. Add focused telemetry and custom-data procedures only for supported paths,
   with relevant failure/recovery guidance.
4. Publish concise observation/Bifrost explanation and source-backed lookup
   links; check the resulting sidebar and search.

## Proof Strategy

These are documentation and reference changes. Run every published path from
the stated environment through write, shutdown/flush, and read using a real
server. Confirm attribution to the intended Service and Run, plus a material
permission or schema failure. Compare behavior with Bifrost design and current
public client/API contracts. Run `mise run docs:check`; inspect built-site
search, links, and mobile tables/code overflow. No manufactured RED applies.

## Acceptance Criteria

- At least one supported native observation path has complete code and a
  visible readback tied to the declared service.
- Traces/logs/metrics and custom Bifrost data are documented as separate
  procedures only where each is executable; gaps are explicit at point of use.
- Bifrost architecture and the identity/retention model explain the task
  without turning the procedure into an internal-engine tour.
- Permission and data-shape failures include observable refusal and recovery.

## Expected Write Set and Consumer Closure

New Observe and Understand content plus relevant reference links under
`docs/src/content/docs/`; indexes generated from the published pages. No
server, Bifrost engine, or SDK behavior change is in scope.

## Verification and Evidence

Real write/flush/read transcript, refusal case, source comparison,
`mise run docs:check`, and rendered link/search/responsive inspection.

## Material Stop Conditions

Stop for a spec revision if native observations cannot be tied to the declared
service as the approved spec requires, or if supporting a promised path needs
new public Bifrost behavior.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)
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
