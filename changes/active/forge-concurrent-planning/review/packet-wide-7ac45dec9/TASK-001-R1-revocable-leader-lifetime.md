---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-forge-concurrent-planning
spec_revision: 11
parent_task: TASK-001
remediates: [FIND-TASK-001-1]
---

# Revocable Forge leader lifetime

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`
- Original task: `changes/active/forge-concurrent-planning/tasks/TASK-001-leader-and-promotion.md`
- Review verdict: `review/packet-wide-7ac45dec9/verdict.md`
- Reviewed candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Base: `c1508b375`

## Diagnosis

REQ-001 and INV-001 require loss of the 30-second PostgreSQL term to stop
dispatch and leader-timer work. The only heartbeat loop awaits hinted
promotion and a sequential promotion-debt sweep that can block on catalog,
object-store, and SQL IO. PostgreSQL can elect a successor while the old
process retains `held: Some`; peer handlers accept that cached term and an
already-running maintenance pass retains a cloned term without revocation.
The observable result is concurrent leader behavior and stale maintenance even
though downstream task/table fences reduce corruption risk.

## Intended correction outcome

PostgreSQL renewal progresses independently of promotion work. Expiry,
renewal failure, replacement, or shutdown revokes the exact in-memory term;
all dispatch, promotion, peer, and maintenance consumers stop before their next
durable effect.

## Decision-complete recommendation

Keep `ForgeLeadership` as the owner. Separate its renewal lifecycle from the
promotion sweep and make the existing `ForgeHeldTerm` revocable. Clearing or
replacing `held` must revoke that same term, and existing consumers must check
the revocation at their effect boundaries. Reuse the current term token,
schedule owner, and cancellation machinery. Do not add a second lease,
scheduler, durable schedule, or host-clock authority.

## Preserved behavior and non-goals

- Preserve single-row PostgreSQL election, immediate graceful resignation,
  volatile scheduling state, durable promotion recovery, and local/peer route
  equivalence.
- Preserve per-table/task fencing and per-table failure isolation.
- Do not redesign promotion, worker pull, or maintenance membership.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-1` | Promotion IO cannot prevent renewal; a replaced term is revoked; notify/pull/report and maintenance perform no later effect under it; the successor resumes recovery without duplicate settlement. |

## Focused proof and broader verification

Park a real hinted/debt promotion beyond the lease while a standby contends;
prove the successor obtains a larger token and the old replica refuses all
leader handlers. Pause maintenance, revoke the term, and prove no later rewrite,
expiration, or cleanup effect occurs. Run the exact focused tests, the Forge
journey, the owning Bifrost verification lane, format, lints, and diff check.

