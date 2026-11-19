# Task 010 design and readiness review

Original verdict: REVISE_TASKS

Current disposition: task clarification applied; implementation verification pending.

Reviewed the current proposed [task 010](../tasks/010-nonblocking-verification-execution.md)
against [approved spec revision 7](../spec.md), the finalized
[execution design](approved-nonblocking-execution.md) and the existing affected
owners. This is review of a mutable proposed task, not an immutable implementation
review or runtime qualification. The spec approval was explicitly given by the
maintainer in this conversation; recording it alone did not close the original observations below. The subsequent
clarification requires reuse of existing runner error/cancellation/lease-loss
cleanup and tests it through the changed lifecycle. These observations do not
establish a need for another subsystem or a new public failure policy.

## What is settled

The direct/queued flow is coherent: preserve preparation, use the same memory
engine, release compute independently of persistence, manage leases and batched
SQL in background, retain finished queued state within bounded tracking, and stop
new claims when tracking fills. Preserve RS gateway/capacity behavior and Bifrost.
The task correctly calls out that Completions cannot simply be detached while
retaining its old execution-slot-based memory bound. No further decision is
needed about a second engine, cold-cache refusal or Bifrost optimization.

## Findings

### F010-1 — Permanent recording failure has no completion policy

Location: task 010 §Background lease and bounded queued completion (retention
until canonical evidence acceptance), §Loss/recovery (outbox full/unavailable)
and O4. Related obligations: REQ-017, AC-012; existing byte/format validation.

The specified retry flow covers capacity pressure and unavailable Scribe but
requires acceptance as its only successful exit. Existing persistence can fail
permanently: `Checkpoint::prepare` can reject a state exceeding a Variant bound
([state.rs](../../../../crates/wyrd/wyrd-server/src/verification/state.rs):480–505),
and `ScribeSink::tally` explicitly drops unencodable or terminally rejected
writes rather than retrying them
([scribe_outbox.rs](../../../../crates/wyrd/wyrd-server/src/scribe_outbox.rs):1895–1935).
An identical unencodable/rejected terminal payload cannot become accepted merely
by retrying after an outage ends. Enough such finished claims could occupy the
entire tracking allowance indefinitely and stop future claims.

Required task correction: distinguish transient staging/transport failure from
permanent encoding/format/rejection failure. Identify the supported existing
visible failure path and bounded custody cleanup, preserving the computed result's
meaning and current validation. Add focused proof that a permanently rejected
finished record cannot cause endless retry or permanently exhaust tracking.
Do not bypass limits, fabricate acceptance, introduce a SQL payload mirror or
silently choose a new durability exception. If the existing failure contract
cannot express this outcome, return that specific choice to the maintainer.

This is a source-backed missing task branch; no runtime stall was reproduced.

### F010-2 — Finished custody lacks an ownership-loss exit

Location: task 010 §Background lease and bounded queued completion (retain/renew
through acceptance), cancellation/takeover row and O5. Related obligations:
REQ-017, AC-012, INV-001/005/006.

A finished result may wait for persistence while its queue claim expires, is
cancelled/superseded, or is taken by a replacement. Existing renewal returns only
unexpired running tokens
([batch functions](../../../../crates/wyrd/wyrd-sql/migrations/20261022000000_verifier_execution_batches.sql):69–111).
Cancellation makes the queue row cancelled and clears lease expiry
([queue queries](../../../../crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs):621–637).
`LeaseRenewal::renew` then cancels the local token; it does not define the lifetime
of the new finished-state tracking proposed here
([leases.rs](../../../../crates/wyrd/wyrd-server/src/verification/leases.rs):183–219).

The task protects compute cancellation and fenced settlement but never explicitly
states when an obsolete background custodian stops renewal/retry and returns
its tracking allowance. Awaiting ACK as its only exit can pin capacity for work
it no longer owns; continuing success settlement would instead violate fencing.

Required task correction: connect ownership-loss observation to ending the old
custodian's retry/settlement authority and releasing its bounded tracking claim.
Preserve accepted history and the documented already-frozen-write race; already
submitted IO may finish. Test takeover/cancellation after terminal selection with
ACK held: old custody returns tracking capacity without ACK, cannot settle the
replacement claim or initiate fresh work, and replacement recovery uses only
accepted evidence. Private cleanup/guard mechanics remain implementation-owned.

This is a lifecycle/proof omission, not a request for new leases or durability.

## Authority and evidence limits

Revision 7 approval is recorded. Contradictory architecture, historical runtime
text and dependent 005/007–009 task wording still need the already-listed owner
updates before affected production changes. Those declared updates are not a
new third design finding. Spec/contracts comprehension was independently checked;
small stale-status/proof-map wording was repaired.

Source inspection supports the missing failure paths above. No production code,
SQL migrations, tests or benchmarks were executed/modified by this review.
Performance feasibility and final implementation correctness remain unverified.

## Subsequent clarification

Task 010 now keeps the existing runner as claim owner and gives compute permits
only the computation lifetime. It expressly preserves existing error/cancellation/
lease-loss transitions, rejects endless retry of permanent recording refusal, and
requires local tracking cleanup on ownership loss. O4/O5 exercise those existing
paths after computation while ACK is held. No separate completion manager, retry
framework, SQL mirror or repair workflow is prescribed.

F010-1/F010-2 are addressed as task ownership/verification clarifications. This is
not proof that the current implementation already covers the changed lifetime.
If implementation reveals a concrete incompatibility with approved semantics,
report that evidence rather than inventing a new policy. No implementation PASS
or new readiness review is claimed by this document update.

The subsequent Ponytail comparison against task 010 identified two concrete task
revisions, now applied: name the existing ClaimLoop permit as the lifecycle bound
(and distinguish it from compute resources in O2/O4/O5), and retain the existing
cancellation-notification migration/trigger/listener. The other audit observations
were current-code defects already covered by the task, not additional task gaps.
