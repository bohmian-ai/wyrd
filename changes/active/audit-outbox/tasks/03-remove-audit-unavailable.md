---
id: AUDIT-OUTBOX-T03
title: Remove audit-unavailable contracts and the doctrine allowance
kind: implementation
status: ready
spec: SPEC-audit-outbox
spec_revision: 1
depends_on: [AUDIT-OUTBOX-T02]
requirements: [REQ-004]
acceptance: [AC-006]
---

# Remove audit-unavailable contracts and the doctrine allowance

## Outcome and Value

No error exists solely to report an audit write failure:
`WYRD_VALA_500_AUDIT_UNAVAILABLE`, the query/Bifrost audit-unavailable codes,
Gate `IngestError::AuditUnavailable`, catalog/platform/auth variants, SDK error
mappings, proto, generated schemas, and docs. `AGENTS.md` §2 and
`architecture/wyrd-design.md` drop the "not yet converted" allowance.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-spec` error catalog, `vala-bifrost-redux`, `wyrd-client`, `wyrd-tonic`,
SDK packages, generated artifacts (regenerate, never hand-edit), docs.
Prohibited: compatibility aliases for removed codes. A durable stored value
that retained history must still decode is not an error code and stays.

## Approach

1. Delete the variants and every match arm and mapping.
2. Regenerate generated artifacts with the codegen tasks.
3. Update doctrine and docs.

## Proof Strategy

Removal of dead contract surface; TDD is not applicable. Proof is compilation,
`mise run codegen:check`, the served OpenAPI contract test, and a repository
search that finds no live reference.

## Acceptance Criteria

A search for `audit.?unavailable` finds no live error, code, mapping, or doc
reference (historical completed records excepted).

## Verification and Evidence

`mise run codegen:check`, `mise run test:principals:integration`,
`mise run fmt`, `mise run lints`, `mise run py:test:unit` and
`mise run py:typecheck` when Python mappings change.

## Material Stop Conditions

A removed code that durable stored data still requires for decoding history.

## Authority Links

`changes/active/audit-outbox/spec.md`, `AGENTS.md` §2,
`architecture/wyrd-design.md`.
