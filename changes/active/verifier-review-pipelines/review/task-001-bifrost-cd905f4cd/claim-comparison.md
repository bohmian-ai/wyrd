# Discovery claim comparison

Current authority note (2026-11-06): the [PostgreSQL audit](../task-001-postgres-cd905f4cd/verdict.md)
supersedes the new-handoff approval question and relocation-only correction
below. The rejected SQL owner must be removed under USER-PG-001; the specified
rare delayed-worker gap is accepted. Preserve this comparison as historical
routing, not a mandate to introduce another SQL protocol.

Subject and selected reports: [subject.md](subject.md). This is an orchestrator
routing/grouping report, not source validation. Every proposal remains subject
to a fresh independent validator. No intended verdict is supplied.

| Shared behavior / invariant | Discovery proposals to reconcile | Routing |
| --- | --- | --- |
| Direct final delivery | INV-1, SYS-1 | Withdrawn by user correction on 2026-11-06: direct verification returns without waiting for final Bifrost acknowledgements; former finding 18 creates no remediation. |
| Recorded execution identity and terminal state must support retained reads | BEH-1, BEH-2, INV-2, SEC-I-02 | Compare one canonical custody root with distinct missing identity and non-verdict-terminal producer paths; do not create per-reader fixes or move queue coordination. |
| Selected terminal result must survive queue completion loss/exhaustion | INV-3 | Validate final-attempt completion-loss path and terminal queue writers; distinguish retry/deadline policy from accepted result delivery. |
| Checkpoint acceptance must respect cancellation/expiry/current authority | SYS-2, CONC-I-1, SEC-I-01 | Focused follow-up traces owner-safe correction and approved boundary. Original cross-messaged domain reports are excluded. |
| Previously accepted publication replay still requires current execution authority | CONC-I-2 | Validate the covered fast path separately from new-write acceptance, including prior SQL owner and stale-first replay; reject if only a harmless retained-read difference. |
| Checkpoint preparation must use bounded existing writer facilities | BEH-3, MNT-001 | One preparation owner/cost; validate explicit task constraint, source, supported state size and minimum correction. |
| Mandatory changed-item rustdoc | MNT-002, STD-001 | Consolidate exact changed items into one bounded documentation correction. |
| Tenant SQL authoring rules | STD-002 | Check exact mandatory rules, actual queries/RLS and correction burden; no leak claim inferred merely from rule violation. |
| Direct generic state-head SQL versus request-path authority | STD-003 | Focused follow-up resolves authority and whether correction is local or requires spec revision. |

Data specialist proposes an empty ledger after examining state fencing/replay,
exact accepted reads, migration custody and insertion-cut paging. Its PASS
cannot waive a materially different obligation identified by another reviewer;
validator reconciles complete-state claims from the actual authority and source.
No acceptance is inferred from agreement or from a discovery PASS alone.

## Follow-up outcome routed to validation

`followup-review.md` resolves the direct-request SQL authority scope and
independently traces cancellation after staging. It narrows the lease-expiry
claim to supported timing and marks the precise queue-to-generic-acceptance
handoff correction unresolved under the committed-handoff rule. The fresh
validator must independently resolve or preserve that material decision; this
orchestrator does not choose an unapproved synchronization mechanism. An
unresolved correction is not converted into an implementation task by majority
agreement. No additional discovery pass is requested merely for uniqueness.
