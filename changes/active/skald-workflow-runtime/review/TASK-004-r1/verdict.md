# TASK-004 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `96e993a16706d2fb759e4cdb7371ff490b198a35`
- Scope: complete base-to-candidate range

The candidate identity remained unchanged throughout discovery, follow-up, and validation.

## Independent review results

| Review | Result | Material outcome |
|---|---|---|
| Behavior | FAIL | Six groups of required journey evidence remain incomplete. |
| Invariants | FAIL | Accepted-job authority is absent from governing design/security authority. |
| Repository standards | FAIL | Rustdoc and typed AgentTool output contracts fail repository rules; the Oracle cleanup claim was later rejected. |
| Maintainer | FAIL | Eight concrete AgentTool declaration methods lack required rustdoc. |
| System resilience | FAIL | Proposed Oracle capacity regression was rejected after focused follow-up. |
| Security/tenancy | FAIL | No authenticated second-tenant journey crosses the actual boundary. |
| Concurrency/lifecycle | FAIL | Reservation publication and blocking preparation can escape shutdown ownership. |
| Query settlement | FAIL | Open cancellation can exceed the original deadline; pod-loss recovery proof is incomplete. |
| Focused follow-up | RESOLVED | Deleted Oracle byte polling/refusal was bespoke DRIFT, not necessary structured ownership. |
| Ponytail validation | COMPLETE | Fourteen deduplicated findings retained; Oracle polling findings rejected. |

## Reconciled acceptance matrix

| Obligation group | Result | Validated findings |
|---|---|---|
| Workflow preparation is owned before observable reservation and drains under one shutdown budget | FAIL | `FIND-TASK-004-1`, `FIND-TASK-004-2` |
| Query cancellation/settlement stays under the original deadline and owner-loss recovery is proved | FAIL | `FIND-TASK-004-3`, `FIND-TASK-004-4` |
| Real tenant isolation across Workflow, run, Card, and query paths | FAIL | `FIND-TASK-004-5` |
| AgentTool declarations are documented and publish exact typed output contracts | FAIL | `FIND-TASK-004-6`, `FIND-TASK-004-7` |
| Pinned accepted authority and live gateway governance are proved | FAIL | `FIND-TASK-004-8` |
| Scoped preparation/idempotency covers waiter, shutdown, tenant, and lost-response cases | FAIL | `FIND-TASK-004-9` |
| Agent tool negative paths cover bounds, object denial, and terminal integrity | FAIL | `FIND-TASK-004-10` |
| In-process gateway covers all required protocols, Vertex, fallback, deadline, and cancellation | FAIL | `FIND-TASK-004-11` |
| Lifecycle journey covers explicit races, global eviction, queued shutdown, and complete terminals | FAIL | `FIND-TASK-004-12` |
| Graph/snapshot journey covers deep admission, Bifrost sibling service, aggregate overflow, and terminal reserve | FAIL | `FIND-TASK-004-13` |
| Governing design and security authority describe accepted-job lifetime | FAIL | `FIND-TASK-004-14` |
| Existing owner reuse, process-local/non-durable boundary, audit ownership, no Workflow principal, and no new remote tool surface | PASS | None |
| Oracle structured ownership after deleting byte-residue polling | PASS | `STD-004-001` and `SYS-004-001` rejected |
| Recorded broad verification commands | PASS as recorded evidence only | Missing focused assertions above still block acceptance |

## Validated finding ledger

The authoritative detailed ledger is `findings-validation.md`. Retained findings are:

- `FIND-TASK-004-1` — reservation publication can outrun preparation tracking.
- `FIND-TASK-004-2` — blocking preparation is detached from Workflow shutdown ownership.
- `FIND-TASK-004-3` — cancellation during query open can exceed the original deadline.
- `FIND-TASK-004-4` — forwarded pod-loss evidence skips recovery and exact settlement proof.
- `FIND-TASK-004-5` — no real second-tenant journey crosses the security boundary.
- `FIND-TASK-004-6` — built-in declaration methods lack mandatory rustdoc.
- `FIND-TASK-004-7` — built-in output schemas erase known result contracts.
- `FIND-TASK-004-8` — admission and accepted-authority journeys omit pinned/live-governance cases.
- `FIND-TASK-004-9` — idempotency/preparation journey does not close AC-021.
- `FIND-TASK-004-10` — Agent tool journey omits required bound, denial, and terminal-negative paths.
- `FIND-TASK-004-11` — in-process gateway adapter lacks required direct protocol and fallback proof.
- `FIND-TASK-004-12` — lifecycle journey omits explicit races and global/queued shutdown cases.
- `FIND-TASK-004-13` — graph/snapshot proof omits deep admission, Bifrost sibling service, aggregate overflow, and terminal reserve.
- `FIND-TASK-004-14` — accepted-job authority is absent from governing design/security authority.

No retained finding requires a new material product, public API, architecture, security, compatibility, concurrency-semantics, resource-ownership, or persistent-data decision. Remediation remains within approved revision 12.

## Follow-up decision

The focused follow-up was required because the query-domain/maintainer reports and system/standards reports materially disagreed about deleted Oracle graph-child checks. Independent validation confirmed that attempts, drivers, exchanges, participants, workers, and caches retain native lifecycle ownership, while late memory remains charged to the governed shared root. The deleted mechanism only polled reserved bytes and joined no executable owner. Restoring it or its mechanism-specific checks would be unsupported bespoke DRIFT and is excluded from remediation.

## Verification limits

- Per instruction, no build, test, Cargo, or mise command was run during review.
- Candidate-recorded command results were treated as claims and compared with the named tests' actual assertions.
- Missing proof is retained as a finding where the approved task explicitly requires the user journey.
- No required reviewer, report, authority, diff, or source was unavailable.
- No reviewer disagreement remains unresolved.

## Prior-finding closure

This is the first TASK-004 review attempt. There are no prior stable `FIND-TASK-004-*` findings to close.

## Verdict

**FIX_REQUIRED**

Remediation task: `TASK-004-R1-close-accepted-job-gaps.md`.
