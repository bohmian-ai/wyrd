---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-forge-concurrent-planning
spec_revision: 11
parent_task: TASK-003
remediates: [FIND-TASK-003-1]
---

# Preserve refused prepared-cleanup ownership

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`
- Original task: `changes/active/forge-concurrent-planning/tasks/TASK-003-maintenance-and-removal.md`
- Reviewed candidate/base: `7ac45dec99535c881b7a936c66623044f15d8823` / `c1508b375`

## Diagnosis

A fresh active read or refreshed root can safely refuse a prepared cleanup.
Settlement intentionally leaves the row `prepared`, but the worker then routes
the refusal through generic failure settlement. `retry_failure` accepts only
`claimed` or `running`, so it reports lost attempt ownership and abandons the
immediate same-identity replay path.

## Intended correction outcome

Refusal or uncertain deletion retains the exact prepared identity and returns
to existing reconciliation without a slot-fatal ownership error; once the root
clears, that identity converges.

## Decision-complete recommendation

Represent the already-existing retained-prepared outcome explicitly at the
worker settlement boundary. Bypass generic retry/terminal transitions for that
outcome and route it to the existing prepared-candidate reconciliation owner
under the same task/attempt identity. Add no durable state, retry ledger, or
parallel cleanup owner.

## Preserved behavior and non-goals

- Preserve fail-closed refusal, active-read/root checks, uncertain-effect
  evidence, idempotent deletion, and normal retry behavior for claimed/running
  failures.
- Do not weaken deletion proof or synthesize success.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-003-1` | A prepared candidate refused by a newly visible root produces no settlement conflict and replays the same identity after the root clears. |

## Focused proof and broader verification

Prepare a cleanup candidate, add an active read before delete proof, assert
refusal with retained identity and no ownership error, release the reader, and
prove convergence. Run the cleanup integration/journey lanes, owning Bifrost
verification, format, lints, and diff check.

