---
id: TASK-003
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

A reader can declare a Service with exact component Card versions, bind a
Verifier, register the declaration, and inspect its resulting identity and
relationships. Focused explanation and source-backed Card lookup support the
task without interrupting it.

## Owners, Scope, Consumers, and Prohibited Changes

Own the Declare task pages and the Cards/relationships explanation and Card
schema reference needed to finish them. Continue the verified example from
TASK-002. Use server-derived relationships/status and exact `CardRef` shapes;
do not author those fields, invent Card kinds, hand-edit generated schema
artifacts, or restore prior articles.
Use `$human-tech-docs` for every reader-facing guide, explanation, and
reference page, including its Diátaxis, humanizer, and voice passes.

## Approach

1. Confirm the current Service, Card, `verified_by`, and registration contract.
2. Author task-first declaration, registration, and inspection procedures.
3. Add concise conceptual and source-backed lookup pages for terms and fields
   the procedure uses.
4. Verify success and common invalid-reference or authorization failures using
   the supported interface, then integrate navigation and search.

## Proof Strategy

The work documents existing behavior and source-generated contracts; it adds
no Wyrd runtime logic. Run the complete published declaration from TASK-002's
environment, inspect the registered exact version and derived relationships,
and reproduce a material rejected input. Compare reference fields with
`wyrd-spec` schemas and `architecture/wyrd-design.md`; generate rather than
hand-edit reference output. Run `mise run docs:check` and built-site link/search
checks. Use existing codegen checks if source contracts unexpectedly need
changes; such changes are otherwise outside this task.

## Acceptance Criteria

- A reader can complete declaration, binding, registration, and inspection
  from complete examples with observable results.
- The explanation distinguishes Cards, Specs, `CardRef`s, versions, derived
  relationships, and server-managed status.
- Card reference mirrors current source and links back to the task; failures
  identify the rejected input and recovery.
- Only supported pages appear under Declare, Understand, and Reference.

## Expected Write Set and Consumer Closure

New Declare, Understand, and Reference content under `docs/src/content/docs/`;
source-derived reference output and indexes if required by the existing docs
generation path. No Card contract or server implementation write is expected.

## Verification and Evidence

Exact example execution and rejected-case record, source/reference comparison,
`mise run docs:check`, and built-site sidebar/search/link inspection.

## Material Stop Conditions

Stop for a spec revision if the approved Card/Service model cannot support the
documented declaration outcome, or if a public contract change is required.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Positioning and vocabulary](../../../../architecture/references/doctrine/positioning-and-vocabulary.md)
