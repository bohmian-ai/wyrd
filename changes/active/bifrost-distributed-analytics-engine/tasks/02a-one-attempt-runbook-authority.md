---
id: BIFROST-R4-T02A-RUNBOOK-AUTHORITY
title: Remove the stale Oracle peer-retry runbook contract
kind: remediation
mode: REMEDIATE
status: implemented
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 4
depends_on: [BIFROST-R3-T00-AUTHORITY]
requirements: [REQ-007]
invariants: [INV-004, INV-008]
acceptance: [AC-003, AC-008]
parent_task: BIFROST-R3-T00-AUTHORITY
remediates: [REVISE_TASKS-1]
---

# One-attempt Oracle runbook authority

## Outcome and value

The Oracle peer-failure runbook agrees with the approved one-attempt query
contract before physical qualification begins: every failure after Analytical
selection is terminal, and a caller may submit a new logical query after the
failure terminal. Operators are no longer instructed to retry the failed
attempt.

Required execution skill: `$wyrd-implement`.

## Owner, scope, consumers, and non-goals

The only owner is `architecture/operations/runbooks.md`. Task 3 and later
Oracle implementation, review, and incident response consume the corrected
authority.

Do not change production code, the approved spec, peer fencing, mTLS, tickets,
audit-WAL recovery, cleanup, readiness, or retry behavior belonging to Scribe,
Forge, catalogs, or unrelated protocols.

## Ordered implementation scenario

### Scenario 1 — Peer failure remains one terminal attempt

**Behavior.** The Oracle peer path cancels and joins the failed stage tree,
reports the selected Analytical attempt terminally failed, and permits only a
caller-initiated new logical query. Maps REQ-007, INV-004, INV-008, AC-003,
AC-008.

**RED.** Static inspection fails while the runbook requires Oracle to retry a
selected distributed attempt or requires retry journey evidence:

```bash
rg -n "Retry once|cancellation/retry/terminal" architecture/operations/runbooks.md
```

**GREEN.** In the `Oracle audit-WAL or peer failure` peer path, replace the
retry instruction with terminal failure after joined cleanup and state that a
caller may submit a new query after receiving that terminal. Change the go/no-go
evidence from cancellation/retry/terminal journeys to cancellation/terminal/
cleanup journeys. Leave the audit-WAL path and unrelated retry protocols
unchanged.

**REFACTOR.** Keep the runbook operational: summarize the canonical
one-attempt contract in `architecture/bifrost-design.md` without creating a
second lifecycle policy.

## Authority and verification

Authority: `AGENTS.md`, the approved spec REQ-007,
`architecture/bifrost-design.md`, and
`architecture/references/languages/spec-driven-development.md`.

```bash
rg -n "Retry once|cancellation/retry/terminal" architecture/operations/runbooks.md
mise run docs:check
git diff --check
```

The search must return no Oracle-query retry mandate; remaining retry language
must belong to another named protocol.

## Completion evidence

- The focused authority diff removes only the stale peer-retry requirement.
- Search output finds no Oracle-query retry or retry-journey mandate.
- Documentation and diff checks pass.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if reconciliation would permit an automatic
successor attempt or weaken joined cleanup, peer trust, audit, or terminal
semantics.

## Execution evidence

- `architecture/operations/runbooks.md` now makes every post-selection
  Analytical failure terminal, forbids a successor attempt, and permits a
  caller-initiated new logical query after the failure terminal.
- The Oracle go/no-go text now requires cancellation/terminal stream evidence
  and no longer requires retry evidence.
- `rg -n "Retry once|cancellation/retry/terminal" architecture/operations/runbooks.md`
  returned no matches.
- `git diff --check` passed.
- `mise run docs:check` reached generated-document drift unrelated to this
  runbook-only change: the current OpenAPI source adds running-query routes and
  schemas not yet reflected in `docs/src/content/docs/api/`. No generated file
  was changed by this remediation.
