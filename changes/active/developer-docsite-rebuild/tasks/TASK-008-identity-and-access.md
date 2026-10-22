---
id: TASK-008
kind: implementation
status: superseded
spec: SPEC-developer-docsite-rebuild
spec_revision: 2
requirements: [REQ-004, REQ-005, REQ-006, REQ-007, REQ-010, REQ-013, REQ-014, REQ-016, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-003, AC-004, AC-005, AC-006]
depends_on: [TASK-001]
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

An operator can configure supported human OIDC sign-in, grant and inspect
permissions, delegate access, and verify both allowed and denied outcomes.
Reference lets the team diagnose access without guessing a principal's scope.
The Identity and authorization explanation is owned by TASK-002; these guides
link to it and keep it current.

## Owners, Scope, Consumers, and Prohibited Changes

Own Operate identity/access guides and relevant permission, error, and configuration reference. Preserve the
Auth-plane, tenant, principal, delegation, and audit boundaries from Wyrd
design and security posture. No credential values are committed to examples.
Do not invent an OIDC provider flow, broaden scopes, bypass audit, add product
auth behavior, or confuse human login with workload credentials.
Use `$human-tech-docs` for every reader-facing guide, explanation, and
reference page, including its Diátaxis, humanizer, and voice passes.

## Approach

1. Confirm supported human OIDC, role/permission, credential, and delegation
   workflows and their provider prerequisites.
2. Author stepwise setup and access-change guides with safe example values,
   observable permission checks, and recovery.
3. Explain the authority and tenant boundaries; link exact configuration,
   stable errors, and documented limits.
4. Verify allowed/denied outcomes and integrate published pages into Operate,
   Understand, Reference, and search.

## Proof Strategy

The task documents current security behavior, not a new security feature.
Exercise supported configuration and access paths in a safe test environment
using repository-managed identity fixtures or a provider test setup where the
documented flow requires it. Verify a successful grant, a denied
under-privileged action, and audit/accountability expectations without using
real secrets in content. Compare every claim with Wyrd security posture and
current server contracts. Run `mise run docs:check` and built-site link/search
checks. No manufactured RED applies.

## Acceptance Criteria

- Each published OIDC, permission, and delegation task has complete
  prerequisites, exact supported steps, a success check, and a denied or
  expired/revoked failure path as applicable.
- Human, workload, platform, and tenant authority remain distinguishable.
- No example reveals a real credential or implies access across tenants.
- Source-backed configuration and error links resolve; unsupported provider
  behavior is identified rather than invented.

## Expected Write Set and Consumer Closure

New Operate, Understand, and Reference content under `docs/src/content/docs/`
and generated indexes for published pages. No server or SDK authentication
implementation write is expected.

## Verification and Evidence

Allowed/denied walkthrough records, redacted example review, source/security
comparison, `mise run docs:check`, and built-site navigation/search/link
inspection.

## Material Stop Conditions

Stop for a spec revision if the approved OIDC, permission, or delegation
outcome needs a new public auth contract or cannot be safely demonstrated with
current behavior.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Testing workflow](../../../../TESTING.md)
