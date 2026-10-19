# DATA / CONCURRENCY domain review

Result: **FAIL**.

Subject: cumulative `ef3a57b07...47e0c9ab7a5cf91cb8d292bdd96a86bdd966a3f9`; HEAD was checked at entry and after source inspection and remained this candidate. `e4b1e313a...47e0c9ab7` was used only to navigate remediation. No source edits, test execution, Cargo operations, or other current-round reports were used.

Authority: approved revision 13 requirements in `changes/active/long-horizon-verification/spec.md`, original TASK-001, previous verdict/finding ledger and TASK-001-R1; AGENTS.md, architecture/agent-rules.md, Wyrd design's Verifier/Trigger/Operator boundaries, doctrine, Bifrost query-cut/visibility authority, evaluation and analytical-operations-reliability references, spec-driven-development, maintainer-style and testing-workflows. Revision 14's draft reset exception is not approval; no upgrade journey is demanded. The explicit user simplification of finding 1 governs: first-source-within-pinned-finality discovery, newest-active one-page ceiling, no continuing discovery cursor. TASK-002 schedule inputs and TASK-003 pressure/progress are excluded.

## Boundary and coverage

CodeGraph was used before code navigation; the cumulative changed-file map and actual data/concurrency diffs were read independently. The reviewed paths are:

| Boundary | Source coverage and conclusion |
|---|---|
| Producer to retained evidence | shared client Observe/Target/StagedRun; principal owner stamp; verification execute/enqueue/stage_direct; ResultPayloadBuilder; execution_inputs/results managed schemas. Group metadata remains separate from authored input slots; owner comes from accepted Service identity without a group membership write. Source staging retains the existing outbox loss boundary. |
| Registration to frozen lookup | cards service validate_request/RegistrationWriter/BindingProjector, EffectiveSpecs group-source resolution, source contract, shared artifact-manifest validation and loader caller. Binding projection uses caller-owned TenantConn; no callee commit was introduced. Manifest validation now executes before registration writes and reuses one pure rule; no additional transaction problem found. |
| Discovery to snapshot | entire groups.rs: Sources, candidate/count/identity/payload statements, Ended, Payloads, complete; queue refused_submissions/selected_results/group_states/enqueue_group; ScheduledQueryCaller run_with and Bifrost's separate per-query cuts. Findings below remain at this boundary. |
| Snapshot to result and dispatch | scheduler poll_groups and binding claim/cursor, execution migration/index, runner grouped input and source-limit result, lease settlement/COMPLETE/PREVIOUS_GROUP/dispatch SQL, results mapper, binding state. One binding claim is held through enqueue/commit; unique partial index independently fences pending/running/retrying groups. Inputs/coverage are serialized on the execution and retries reuse them. Queued outcomes use selected result identity rather than an abandoned attempt. |
| Existing siblings | ordinary direct/manual/scheduled queue paths, Drift reader exception, ordinary on_failure, owner activity arming, result staging and query stream settlement. These remain with their existing owners; no replacement data store or writer was introduced. |
| Proof consumers | Rust grouped support/refund/coding/frames/recovery plus late-work/owner-grant journeys, SQL group serialization/dispatch test, groups unit tests; Python/TS grouped story bodies and MCP media proof as claimed in task evidence. Public generated declarations and OpenAPI changes are delegated to the contract reviewers; this domain does not independently certify their schema parity. |

## Acceptance assessment

| Data/concurrency obligation | Implementation and verification evidence | Result |
|---|---|---|
| Exact tenant/owner/subject/Verifier/group source tuple; input order, identity dedup and aligned nulls | Sources::eligible/any; Ended::executions; Payloads::value; support/recovery journeys; selected_results authority. RLS remains TenantConn-owned. | PASS for the inspected normal paths |
| Serial snapshots, immutable retries, lease recovery and verdict-change settlement | DUE_GROUP_SQL SKIP LOCKED; outstanding unique index; GroupSnapshot in inputs/coverage; complete transaction and replay dispatch fence; SQL group_executions_serialize_and_dispatch_on_verdict_change and Rust restart/recovery proof. | PASS by source and available reported proof |
| Accepted simplified discovery and duration safety | candidates finality HAVING / newest ordering / fixed 64 page; no completion watermark; GroupState covers count+time; checked duration arithmetic. Late-work and largest-duration tests address the prior source faults. The one-page ceiling is expressly accepted and is not a finding. | PASS |
| Per-reference ceiling on eligible distinct source membership, before payload collection | Count is taken on one Bifrost cut, identities on another; over-limit classification also precedes queue refusal filtering. Existing stable 20/21 frame proof does not cover these paths. | FAIL: DATA-01 and DATA-02 |
| Bounded mixed-source reads; terminal errors do not silently accept SQL prefixes | Payload SQL now names selected execution IDs and all query streams require terminal settlement. Failed selected-input coverage defers. Raw-prefix suppression and reads of already-known over-limit tuple payloads were removed. | PASS for those specific prior defects; ceiling correctness remains failed |
| Retained evidence semantics and non-goals | No new producer receipt, readiness barrier, group lifecycle/revision table, checkpoint, retention policy or upgrade subsystem. | PASS |

## Proposed material findings

### DATA-01 — INCORRECT: a newer identity cut can bypass the source-execution ceiling

Related prior finding: **FIND-TASK-001-2**, not a new discovery-progress requirement.

Obligation: REQ-009 bounds each reference's distinct source executions and requires overflow to complete inconclusive before payload collection; REQ-004/006 require bounded subsequent reads. Bifrost explicitly pins each statement independently and allows newly visible or overlapping rows.

Exact locations: `crates/wyrd/wyrd-server/src/verification/groups.rs:139–144`, `:282–298`, `:320–330`, and `Sources::identities` at `:631–637`.

Reachable producer-to-consumer trace:

1. The count query sees 20 executions for a tuple with ceiling 20, so the tuple is absent from `over`.
2. An already accepted input/result becomes visible before identities opens its separate query. This requires ordinary publication/live-tail timing, not an exceptional transport or synthetic load.
3. `Sources::identities` selects every distinct identity/time for the tuple, with no cap or count tied to that selection, and returns 21 or more IDs.
4. `snapshot` trusts the earlier `over` map, makes the entire newer list readable, reads all selected payloads and pins `exceeded_ceiling = None`. The runner's only ceiling check is the snapshot marker, so a successful normal judgment is possible over 21 source executions. With a large visibility burst the identity output and subsequent payload membership are likewise no longer bounded by the deployment ceiling.

No Postgres binding lock freezes Bifrost's cuts. `complete` checks missing payloads, not the number of selected executions, and therefore does not correct this state. The existing SQL/transport terminal owner can correctly finish both queries while this defect occurs.

Observable consequence: the configured ceiling is bypassed and an over-limit group can report passed/failed instead of completed/inconclusive; excess source payloads are collected before any overflow decision.

Minimum correction boundary: retain GroupPoller/Sources and the existing source-limit outcome. Bound distinct identity selection per tuple at the actual membership read (an overflow sentinel is sufficient), and derive the overflow marker from that selected eligible membership before putting any of its IDs into `readable`. Do not rely on an earlier independently pinned count as a hard bound and do not fix this with a post-payload runner guard, new group store, or producer IO. Preserve complete selected lists within the ceiling; never evaluate a prefix as the whole list.

Focused proof: force a new eligible execution to become visible between the count and identity reads at the existing read seam. With ceiling N, show N remains allowed, N+1 is completed/inconclusive with the named reason, and no payload request includes the overflowing reference. Include a mixed under/over tuple so the other reference cannot accidentally trigger its payload collection. Use the existing query owner/fixtures; no broad suite, benchmark or host-load mechanism is needed.

### DATA-02 — INCORRECT: refused submissions can manufacture an overflow that excludes valid evidence

Related prior finding: **FIND-TASK-001-2**. This is the other side of the same eligibility-before-ceiling obligation and may be consolidated with DATA-01 under that existing ID.

Obligation: REQ-001 preserves replay/conflict semantics without adding source executions; REQ-009 counts eligible distinct executions, after eligibility and deduplication. The original support story explicitly excludes idempotency-conflicted enqueue attempts.

Exact locations: `groups.rs:139–147`, `:246–253`, `:287–291`, and the deliberate approximation comment at `:750–752`. Producer evidence: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:1230–1246` and `accept`'s conflict branch `:1307–1326`; refusal query `:259–265`.

Reachable trace: enqueue inputs are retained before the background queue writer decides a key conflict. The queue stores such an attempt errored with zero attempts under its own execution identity. Take 20 legitimate source executions at ceiling 20, plus one conflicting request with the same group and exact source tuple. `counts` counts all 21 Bifrost identities. The tuple is marked over, so `identities` does not list it at all. `refused_submissions` receives only identities of the remaining under-limit tuples and can never remove the conflicting identity from this over-limit tuple. The resulting snapshot is permanently marked exceeded although the eligible membership is exactly 20. The code comment acknowledges inclusion of refused submissions but a local ponytail comment does not approve a public change to the eligible-execution ceiling.

Observable consequence: a group that should evaluate the allowed 20 executions instead completes inconclusive with a false `source_execution_limit_exceeded`; its pinned inputs omit all valid evidence for that reference. If only conflicted attempts exist, they can also manufacture activity without any accepted source execution.

Minimum correction boundary: apply the queue writer's existing refusal authority before deciding that a tuple's eligible membership exceeds the ceiling. Reuse/extend `VerifierExecutionQueue::refused_submissions` and GroupPoller's source selection, rather than changing acceptance/idempotency semantics or allowing conflicted rows into a review. The final list and overflow count must share this eligibility decision, while preserving payload avoidance and bounded identity collection. This is not a mandate for per-call Postgres work, a new membership inventory, or a new public ceiling.

Focused proof: through the existing HTTP enqueue/support or media journey, retain exactly N accepted executions and additional distinct errored idempotency-conflict attempts for the same tuple/group. Assert that the review still contains exactly N accepted IDs, has no exceeded marker and returns the normal judgment; then one additional accepted source execution must produce the explicit overflow result. The existing support test uses few rows well below its ceiling and cannot expose the problem.

## Prior closure and verification limits

Prior finding 1's requested simplified discovery is closed in source; no cursor/fairness expansion is proposed. Prior finding 2's physical prefix and broad payload membership faults are corrected, but its eligible-membership-before-payload invariant is not closed for the two paths above. Prior timing finding 3 is closed by checked arithmetic and elapsed comparisons. Prior selected-result, tenant isolation, serial-fence and immutable retry behavior were source-checked rather than accepted from the implementation summary.

The task/remediation append reported green checks and focused journeys. Root reruns are forthcoming and are not claimed as this review's execution evidence. I ran no tests, as instructed. Stable preloaded 20/21 cases, query-denial retry and SQL-string assertions do not exercise the two membership interleavings above. Source evidence suffices to propose these reachable defects; independent validation should retain, revise or reject them before the final ledger. No unapproved revision-14 exception, upgrade requirement, TASK-002 or TASK-003 work, or optional hardening is included.
