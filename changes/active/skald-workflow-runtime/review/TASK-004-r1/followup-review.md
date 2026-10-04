# Focused follow-up review — Oracle query settlement ownership

## Immutable subject

- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `96e993a16706d2fb759e4cdb7371ff490b198a35`
- Conflict: whether removing nested-memory-child polling/refusal releases Oracle capacity or reports quiescence while live query work remains.
- Review mode: source, cumulative diff, and recorded evidence only. No build, test, Cargo, or mise command was run.

## Ownership trace

The candidate still prevents graph release while reachable execution owners remain. Follower settlement waits for no open coordinator connection or live attempt, cancels the graph, finishes retained attempts, closes exchanges, drops its worker/cache, then releases the guard. Leader settlement finishes its attempt, closes exchanges, drops participant grants, and then releases. `AnalyticalSupervisor::release_graph` refuses live registered attempts, and `finish_attempt` joins retained drivers.

The deleted mechanism did not join work. It polled aggregate `MemoryPool::reserved()` until zero and treated any nonzero reservation as a live child. Removed tests manufactured an inert retained `MemoryConsumer`, not continuing CPU, IO, transport, or executable query work.

Late memory stays safely owned: reservations retain the `Arc<dyn MemoryPool>`; `GovernedMemoryView` retains the shared root and per-query ceiling; growth remains checked against both; shrink returns bytes through the governor; final view drop detects an inconsistent ledger. Returning a concurrency slot after executable owners end while reference teardown remains charged to bounded shared memory is conventional separation of concurrency admission from memory accounting.

## Conflict resolution

| Claim | Resolution | Evidence |
|---|---|---|
| Deleted child-idle loop was necessary structured ownership | Rejected | It polled bytes and owned/joined no task, connection, stream, or transport. |
| Late memory permits unbounded oversubscription | Rejected | It remains charged to the same governed root and per-query ceiling. |
| Late memory proves CPU/IO survives graph release | Rejected as unsupported | No reachable path was found after requests, attempts, exchanges, participants, worker, and cache settle. |
| Readiness/shutdown claim clean while structured work remains | Rejected for this path | Real attempts and cleanup failures remain registered; inert memory residue is accounted teardown. |
| Removed tests protected a required invariant | Rejected | They encoded a bespoke `reserved() == 0` polling policy, not approved observable behavior. |
| Restore polling/check machinery | Rejected as DRIFT | Fixed-interval byte polling is neither native structured ownership nor required by the task. |
| Domain distinction between joined work and reference-counted memory | Confirmed | Source separates explicit lifecycle ownership from the shared memory ledger. |

## Proposed findings

None. `SYS-004-001` and the corresponding portion of `STD-004-001` lack a reachable producer-to-consumer execution path. Their remediation would bind admission/readiness to dependency reference teardown or restore bespoke polling.

## Verification limits

No execution commands were run. This follow-up does not resolve other discovery findings, including open-cancellation deadline handling, pod-loss journey evidence, rustdoc, or AgentTool output schemas.

## Follow-up result

**RESOLVED**

Necessary structured lifetime ownership remains; the deleted nested-child polling/refusal/poison machinery was a bespoke check over memory-accounting residue.
