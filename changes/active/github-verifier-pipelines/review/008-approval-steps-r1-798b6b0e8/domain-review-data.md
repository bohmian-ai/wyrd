# TASK-008 persistent-data and concurrency review

Result: **FAIL**.

Subject: `cd0b8e18bd5737267f13c8e72110a06d379d7a0b...798b6b0e83595356c0d2328fa541119267e84fb2`; HEAD verified unchanged. First review, no carried-forward acceptance evidence. No production edits or executable tests performed by this reviewer.

## Boundary and authority

Reviewed decision custody and its concurrency/queue boundary: exact accepted occurrence → authorization → queue eligibility → first-wins Scribe decision → inspection/runner read → same-execution queue wake and park recheck. Read the task, approved spec revision 8 REQ-010/011 and INV-002–006, contracts Human approval, approved-deferred-inputs Approval and deadlines, approved-runtime-and-github Durable waits and resumption, applicable root/SQL AGENTS, and the changed Bifrost canonical acceptance contract. CodeGraph was used before source exploration.

Source coverage includes full `verification/approvals.rs`, `components/verification/service/approvals.rs`, the relevant `retained/status/approvals` inspection flow, `verification/inputs.rs::decisions`, runner decision restoration and deadline construction, state wait/occurrence IDs, SQL decision-open/wake and existing claim SQL, the Scribe Fenced/frame/ack extensions, ApprovalsTable/generic-write restrictions, and all new pg_approvals test bodies. Compared the complete new acceptance flow with existing `DeferredEvidence::{accepted,accept}` and existing queue/RunState custody. The sibling decision table supplies occurrence-specific actor/decision/reason custody; it does not introduce another full execution-state writer. Scribe's existing first-wins batch fence and queue park/recheck are reused. A distinct schema/row projection is needed; no new generic abstraction is justified merely to merge the two projections.

## Scoped acceptance matrix

| Obligation/invariant | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| Decisions name tenant/execution/exact recorded occurrence | `retained` reads accepted RunState and tenant-bound queue; service lines 74–109 requires the recorded ID and uses accepted exact Verifier/subject | pg_approvals first journey asserts foreign tenant 404, unknown occurrence 404, wrong scope 403 | PASS |
| Bifrost owns accepted decisions; no SQL payload mirror or competing full-state writer | ApprovalDecisions stages a dedicated Fenced row; queue SQL stores no decision; runner consumes decisions keyed by recorded occurrence and retains sole RunState ownership | Direct source comparison with DeferredEvidence and state/runner | PASS |
| First accepted decision final; identical actor/decision/reason retry returns same receipt | `accept` reserves/stages a one-row batch keyed by approval ID; Scribe frames keep its stable batch identity; service `replay` compares actor/status/reason; acceptance error rereads fence before replay | pg journey asserts full receipt equality and three conflicts (different decision/reason/actor) | PASS |
| Positive receipt after durable ACK, unavailable accepted state never absence | ApprovalDecisions awaits oneshot ACK; decided checks committed batch fences then refuses unavailable missing rows; runner maps unavailability to defer | stalled-write journey asserts 503 before ACK and eventual replay; fenced-but-invisible case is source-proven, not directly exercised by that test | PASS, proof limit noted |
| Same execution resumes; ACK→wake loss recoverable | wake_decided changes only due time; runner rechecks accepted decisions every claim; bounded park recheck survives lost notification | pg journey asserts original ExecutionId passes/reject fails; stopped-runtime test resumes under the same ID | PASS for source flow; process-restart test limit below |
| Original execution deadline also bounds decision acceptance | Runner caps runtime ceiling with authored `limits.timeout_seconds`; service open check binds only runtime ceiling | Existing expiry test uses only runtime ceiling and waits for queue terminal state before deciding; it does not cover authored shorter timeout with worker stopped | **FAIL — DATA-001** |
| Old-head decision never authorizes new work | occurrence ID hashes ExecutionId; queue terminal/supersession stops subsequent claims; accepted decision remains attached to old execution | old/new head occurrence IDs differ; supersession test confirms cancelled old execution never passes | PASS |

## Proposed finding

### DATA-001 — INCORRECT: decision acceptance ignores the authored execution ceiling

Violated obligation: spec REQ-010/011, task Decision acceptance and AC-006, and contracts Human approval require pending approvals to share the execution's original expiration and refuse a decision after it. This is an execution deadline, not a request-specific or per-approval timer.

Location: `crates/wyrd/wyrd-server/src/components/verification/service/approvals.rs:124–128`, consumed by `crates/wyrd/wyrd-sql/src/queries/verifier_executions/approvals.rs:20–24`.

Evidence and reachable flow:

1. The exact registered Verifier can set `limits.timeout_seconds` below the operator execution ceiling. The existing queued runner explicitly uses it: `runner.rs:1449–1460` derives the remaining authored timeout from admission age and takes the minimum with runtime remaining.
2. The new decide endpoint instead passes only `self.state.verification.execution_timeout()` into `open_for_decision`. Its SQL considers an unfinished row open until `created_at + that runtime duration`; it never sees the Verifier's shorter duration.
3. Register an approval Verifier with an eight-second authored ceiling under a five-minute runtime ceiling. Reach the accepted pending occurrence, stop the verification worker (ordinary restart/deployment downtime), wait past eight seconds, then decide while the API/Scribe remain available. The queue remains unfinished while stopped. `retained` finds the accepted occurrence, OPEN_SQL returns true, and Scribe accepts an approved decision with a successful durable receipt after this execution's actual deadline.
4. When workers return they correctly time the execution out using the authored limit. The user has nevertheless received a new approved receipt for an expired occurrence, which contradicts the required refusal and records an invalid decision. This scenario needs no overlapping request/deadline race or unusual input.

Materiality: author-defined execution ceilings are supported public configuration, and deciding during worker interruption is specifically supported by the new stopped-runtime journey. The control endpoint must enforce the same deadline even before a worker has materialized `timed_out`; otherwise the exact expiration contract depends on worker availability.

Smallest correction: use the same effective admission-relative timeout as the queued runner when determining whether a new decision is open: minimum of the operator execution ceiling and the accepted exact Verifier's authored ceiling. Reuse the existing exact target/spec resolution owner (`VerifierCardCache::resolve`, which already serves queued work) and existing `open_for_decision`/PostgreSQL statement clock, preserving tenant/exact-version resolution. Do not add an approval deadline, clock anchor, SQL payload store, timer service, or consumer guard in the engine. This is a bounded eligibility correction; the existing SQL parameter already represents the needed duration. Preserve identical already-accepted retry receipts and current actor/scope/subject checks, Scribe ACK semantics, and wake recovery.

Focused proof: extend the existing server approval journey with an authored timeout lower than the runtime ceiling; park the execution, stop its runtime, decide after the authored deadline but before the runtime ceiling, and assert 409 APPROVAL_NOT_PENDING with no accepted approved/rejected decision. Include the before-deadline success case and ensure ordinary no-authored-limit approvals retain current behavior. Use the existing fixture/runtime controls and narrow pg_approvals selector; no new harness or dependency is needed.

## Limits and rejected hypotheses

The test named `a_decision_made_while_stopped_finishes_after_a_restart` stops and reconstructs `VerificationRuntime` on the same WyrdTestServer, retaining the Scribe process/memtables and settings. It exercises queue/RunState reload but is not evidence that a full server/Scribe restart preserves a receipt or that changing operator configuration preserves an original absolute expiration. Broader restart-proof assessment is left to behavior/system review; this report does not duplicate it as another data finding.

The precheck transaction commits before Scribe acceptance, so a request already admitted can overlap deadline/supersession while awaiting ACK. I do not retain that overlap alone as a separate blocker: source-head continuation remains fenced, the approved design does not select cross-store atomic acceptance, and a stronger linearization protocol would need burden/authority analysis. DATA-001 is independent: its request starts after the applicable authored deadline and the existing open predicate deterministically accepts it.

No Cargo commands or live PostgreSQL tests were run by this reviewer; source/test assertions above are static evidence. No production changes, commits, or pushes.
