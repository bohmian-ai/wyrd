---
id: TASK-008
status: proposed
approved_spec_revision: 4
requirements: [REQ-010, REQ-011, REQ-016]
acceptance: [AC-006, AC-007, AC-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-007]
---

# Accept approval decisions for exact pending occurrences

Carry **approved-deferred-inputs.md → Approval and deadlines** and
**approved-runtime-and-github.md → Durable waits and resumption** through task
007's canonical state/park/continuation owners. **contracts.md → Human approval**
is approved; the earlier recent-login/five-minute proof restriction is superseded.

## Input → pending occurrence

An exact registered Verifier declares `action: {kind: approval, message: ...,
expires_after_seconds: ...}`. Add that closed action/schema/validation at the
existing spec/engine action owner and attach it to task 001's pre-step queued-only
check. Task 006's accepted source/authority and task 007's restored logical
execution provide its context; no request can expand that accepted authority.

When the action becomes pending, the sole state owner creates a server-issued
approval_id for its logical step/item/iteration occurrence, distinct across
repeat/fanout. Attempts do not silently mint another occurrence. Set expiry once
using database coordination time, capped by the original execution deadline.
Persist pending identity/message/expiry in canonical Bifrost checkpoint state
before returning it in inspection or parking. Independent branches continue;
when all block, use task 007's checkpoint-ACK-before-retirement/resource-release
path, not another wait service or deadline loop.

## Decision acceptance and same-execution continuation

The caller selects the pending ID from execution inspection and uses
`POST /v1/verification/executions/{execution_id}/approvals/{approval_id}` with
approved/rejected and optional reason. Resolve the exact tenant/execution/
Verifier/occurrence; check normal authentication, the distinct Verifier approve
action at its exact scope and current subject authority through normal permission/
audit owners. No login-age cutoff, interactive proof, special approval token,
quorum or credential-type shortcut is authorized.

Extend the existing canonical execution control-fact acceptance owner used by
007 to conditionally accept actor/decision/reason for this pending, unexpired,
current occurrence. Record approved/rejected/expired with server actor/time/reason
(expiry has null actor). Same actor/decision/reason retry replays receipt;
conflicting, expired, stale, terminal or superseded occurrence refuses as approved.
Accept durable decision ACK before positive receipt; a process-local notification
or speculative checkpoint is not acceptance. The execution's single state owner
consumes the accepted fact, including on restart; do not race a second full-state
writer against live independent branches.

Accepted decision queues task 007's stable same-ExecutionId continuation.
Restore completed predecessors, original deadline, remaining attempts/spend and
accepted authority. An ACK→queue gap must recover without asking the user again.
Rejection is returned data, not crash/cancellation; authors may branch. Supply the
standard requirement helper that passes only approved and fails rejection/expiry.
Completion of the action alone cannot satisfy an authored approval requirement.
Discarded speculative/obsolete occurrences and an older head's approval never
authorize current work. Revocation/cancellation/deadline apply while parked.

## Output and shipped projection

Inspection returns pending/status occurrences with ID, step/item/iteration,
message and expiry. Decision receipt is exactly approval_id/status/actor_id/
decided_at/reason from the contracts. Task 009 reads pending reason and accepted
progress for GitHub status without changing verdict. Shared client and thin
Rust/Python/TypeScript, CLI, HTTP/MCP approve/read operations, scopes, typed errors,
permission projection and focused authoring/approval/expiry docs ship here.
Amend **wyrd-design.md → Verifier**, **wyrd-security-posture.md → Permission
model / Permission check** for approve and exact Verifier scope, and the Bifrost
canonical decision/ACK contract at its owner. No UI or alternate login flow.

## Proof

**AC-006 (approval):** Drive task 002 connect→003 declare→006 admit and task 007's
actual park lifecycle, with independent work and a pending approval. Read its ID
through SDK/CLI/MCP and approve/reject as an authorized second principal. Observe
exact receipt, same execution continuation and authored require-approved behavior;
expiry records expired rather than passing/crashing. Unauthorized, foreign tenant,
wrong Verifier scope, conflicting retry, stale occurrence and old-head decision
fail safely. Repeat/fanout have distinct occurrence IDs; discarded speculation
cannot authorize retained work. Observe independent progress and capacity release
through the same resource path already proved by 007.

**AC-007 (decisions):** Crash before/after decision ACK and continuation commit,
then recover actual pending occurrence and accepted fact through 007. A positive
receipt survives without another decision or replay of settled computation;
unknown/unavailable canonical state is not a newly generated approval. Race
expiry, current authority loss and task 006 supersession with acceptance and
observe no stale release. This extends 007's recovery journey with real accepted
decisions, rather than a synthetic checkpoint claiming approval.

**AC-011 (approval):** A reader follows docs using ordinary credentials and
permission, inspects pending identity and decides through each shipped surface.

Tier 1: matched SDK stories and MCP journeys through WyrdTestServer/Postgres,
existing runners in README; tier 2: conditional fact/checkpoint/queue fences in
existing server and SQL integration owners via
`mise run test:bifrost:journey:server` and `mise run test:sql`; tier 3: action,
permission coverage and standard helper behavior at their owners. Record new
focused commands when tests exist. Apply README codegen/typing/OpenAPI/MCP,
format/lint, dependency and tenant checks. No authentication-age tests are added.
