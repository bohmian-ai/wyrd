# TASK-001 system resilience review

Overall result: **FAIL**.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`; candidate: `47e0c9ab7a5cf91cb8d292bdd96a86bdd966a3f9` (HEAD verified).
- Reviewed cumulative base-to-candidate runtime, contract, persistence and caller changes; used `e4b1e313a...47e0c9ab7` only for remediation navigation. Read the original TASK-001, prior verdict, validated findings, system report and TASK-001-R1 including appended evidence. No other current-round report was consulted.
- Approved authority is revision 13 requirements in `spec.md`, under current user direction. Its draft-14 header is recorded, not approved here; no retained-schema upgrade requirement is inferred. The explicit user simplification of finding 1 permits first-row-within-pinned-finality discovery, newest activity first and one 64-group page. No continuing cursor is required.
- Governing sources: AGENTS.md; architecture/agent-rules.md; architecture/wyrd-design.md (doctrine, runtime identity, Verifier/Trigger/Operator); architecture/wyrd-doctrine.mdx; architecture/bifrost-design.md (durability/visibility, query terminal and failure boundaries); architecture/operations/{deployment-and-release,reliability-and-recovery}.md; architecture/references/{README,doctrine/architecture-constraints,domain/analytical-operations-reliability,languages/spec-driven-development,languages/maintainer-style,languages/testing-workflows}.md.
- TASK-002 scheduled inputs, TASK-003 process staging/pressure/progress and capacity qualification remain excluded.

CodeGraph was consulted before source navigation. Only this report was written; no source/test edits or tests/Cargo commands were performed. Test results are task-record claims checked against source, not independently executed results. The root owns sequential reruns.

## Deployment and failure paths

One logical wyrd-server can comprise multiple role-targeted replicas behind the gateway. Postgres owns binding cursor, execution snapshot, lease/fence and dispatch authority. Bifrost retains input/result rows through Scribe and queries them locally or through Oracle peers. Process outbox staging remains weaker than Scribe acknowledgement. Verification-hosting processes add GroupPoller to the existing scheduler; no second service or queue is introduced.

| Path and owner | Failure, surviving state and recovery | Assessment |
|---|---|---|
| Three SDK Observe surfaces and HTTP/MCP request → VerificationControl → ResultPayloadBuilder | Optional group_id and authenticated owner are copied into existing input/result writes. No producer group write/binding lookup was added. Abrupt process death before Scribe acknowledgement may lose outbox rows under REQ-005. Retained rows survive independently of producer Runs/tracing. | PASS; no new receipt promised. |
| Registration → EffectiveSpecs/source_binding → BindingProjector → verification_bindings | Exact source identities and Trigger settings are pinned before binding persistence. Invalid references/slots refuse the request. Server-side Verifier validation precedes writes. Shared artifact-manifest validation refuses conflicting filesystem targets on server and loader without another runtime. | PASS for failure containment. |
| RuntimeBuilder → scheduler::poll_groups → GroupPoller | Only block-hosting processes poll groups. SKIP LOCKED serializes each binding claim. Read denial/outage/timeout becomes GroupPollError::Read; scheduler logs it, commits cadence advancement and retries later. SQL failure aborts the tenant transaction. A dependency failure stops the poll, not the shared server. Other APIs retain their independent owners. | PASS at source; missing partial-read proof below. |
| counts → identities → refused removal → Ended → snapshot | Oracle statements pin independent cuts; visibility may grow between them. Ceiling classification comes from the earlier count, while actual lists come from later identities. Counts include refused submissions; over-ceiling tuples bypass refusal filtering. | FAIL, SYS-001. |
| snapshot → enqueue_group → committed verifier_executions | Inputs/source IDs/coverage commit with the binding cursor. Crash/cancellation before commit drops the transaction; after commit the partial unique index prevents another same-binding/group execution while work is outstanding. | PASS; existing queue retained. |
| Runner::dispatch Grouped → hosted engine → settlement | Attempts reuse frozen inputs. Existing ClaimLoop/LeaseRenewal owns deadline, drain, release/reclaim and stale-lease fencing. Restart reclaims committed work without reselecting values. Overflow marker completes inconclusive before engine dispatch. Provider/tool failures remain execution/check failures. | PASS lifecycle; invalid upstream selection can already be frozen. |
| complete → previous group verdict → unique dispatch → outbox result | Settlement stores verdict and compares preceding same-group result. Baseline/unchanged verdict produces no dispatch; changed verdict inserts replay-safe intent transactionally. Queue coverage survives publication lag. Existing outbox loss does not reopen settled work. | PASS. |
| Agent check → declared bifrost.query → HostedTools/QueryAuthority | Ordinary tool/query/table authorization applies with accepted authority and SYSTEM attribution. Live spans stay outside the snapshot and may differ on retry by approval. Deadline/refusal/error uses existing check behavior; no workspace/mutation tools or process crash were added. | PASS. |

Existing owners are sufficient: ScheduledQueryCaller owns terminal-safe consumption/cleanup, scheduler owns cadence/claims, and queue owns immutable attempts, leases, serial group admission and settlement. GroupPoller earns selection responsibility; an earlier count is not authority for its later selected list.

## Prior closure in this scope

| Prior finding | Assessment |
|---|---|
| FIND-TASK-001-1 / prior SYS-001–002 | Completion-time suppression and idle+1h cutoff are removed. groups.rs:206–235 uses pinned finality/newest activity; GroupState::covers and outstanding fence decide coverage. One-page ceiling is accepted. grouped_reviews_cover_late_work asserts outstanding/coalesced work and null→actual late outcomes each reach one next snapshot. Closed under revised authority. |
| FIND-TASK-001-2 / prior SYS-003 | Raw-row skip and broad payload reads are removed. Payload SQL carries exact readable IDs and owner/tuple/group/finality predicates. complete() defers some missing selected input/direct-result payloads. Closure remains incomplete: ceiling classification can describe the wrong membership (SYS-001), and incomplete-coverage recovery is not exercised (SYS-002). |
| FIND-TASK-001-3 | Ended uses elapsed comparisons and checked timestamp arithmetic; discovery uses checked subtraction. Maximum-duration proof plus neighboring forever/ordinary binding assertions exist. Previous accepted-duration scheduler panic is closed at source. |
| FIND-TASK-001-5 | GroupPoller now uses Caller::accepted and ScheduledQueryCaller::authenticated; manufactured SYSTEM grants are removed. Denial is confined to polling; regrant journey proves recovery. Drift exception remains unchanged. Closed for this boundary. |
| FIND-TASK-001-8 | Late-work, permission and media assertions added. Failed/truncated discovery/snapshot and incomplete-coverage retry proof remain absent; pre-stream permission denial exercises another state (SYS-002). |

## Proposed material findings

### SYS-001 — INCORRECT: ceiling is decided before selected eligible membership exists

**Prior identity:** preserve FIND-TASK-001-2.

**Obligation:** REQ-009 counts distinct eligible executions after eligibility/deduplication: exactly the ceiling is allowed, over-ceiling completes inconclusive before payload collection. REQ-001/005 excludes refused idempotency conflicts. REQ-004/006 bounds source reads.

**Locations:** `crates/wyrd/wyrd-server/src/verification/groups.rs:138–147,246–273,282–296,319–331`; stale `over` classification at 140–143 and unchecked later lists at 320–330. Sources::identities at 630–637 selects all matching identities without a ceiling. Ended::judge at 749–751 explicitly acknowledges including refused submissions in over-ceiling counts.

**Reachable path A:** count sees 20 executions at ceiling 20. Another execution becomes visible before identities, which pins a new cut under Bifrost's documented visibility contract. The tuple is not `over`; identities returns 21 executions. snapshot includes all 21 IDs in readable, collects their payloads and sets no exceeded_ceiling. A max_age-due group, or late-visible earlier-timed source rows, reaches the end condition without waiting for that activity to become idle. The runner evaluates normally and can pass what must be inconclusive. Larger visibility changes also defeat the claimed per-tuple identity bound.

**Reachable path B:** at ceiling one, one accepted execution plus a retained staged submission rejected by the asynchronous queue writer for idempotency conflict makes counts report two. That tuple becomes over-ceiling, so its identities never reach queue.refused_submissions. A false overflow result replaces a valid one-member judgment. Fixing only the under→over race leaves this wrong-membership branch.

These share one cause: an earlier unfiltered count is authoritative for a later filtered snapshot. All producers share this boundary. Ordinary concurrent writes, delayed visibility or recovery/publication expose it; no crash is required. Finality overcount is not independently claimed: the accepted candidate finality bound normally excludes that case with server-stamped times.

**System consequence:** an over-limit group reads/evaluates too much and reports passed/failed; an allowed group falsely reports overflow. Correct frozen retries cannot repair selection already committed, and reactions can propagate its incorrect verdict.

**Minimum correction:** retain existing GroupPoller/Sources selection, refused_submissions, overflow marker and runner result. Ensure the ceiling governs actual eligible membership whose values will be collected, even across changing cuts; do not rely solely on the prior count. Refused identities cannot create overflow. Preserve bounded selection, genuine nulls, selected-result authority, serial admission and tuple isolation. Correct invalid snapshot production before payload reads; do not add an evaluator guard after them, lifecycle inventory, queue, transport, receipt or producer lookup.

**Focused proof:** make the count see exactly the ceiling and identities see another eligible execution; assert no over-limit payload collection and one named inconclusive result. Retain a refused conflict alongside exactly-ceiling eligible membership and prove it consumes no ceiling. Use controlled visibility/fixture rows, not host load. SQL-string assertions and a stable 21-frame group do not exercise either interleaving.

### SYS-002 — MISSING: partial discovery/snapshot failure and retry remain unproved

**Prior identity:** preserve FIND-TASK-001-8; incomplete-selected-coverage proof also belongs to FIND-TASK-001-2, not a new duplicate finding.

**Obligation:** TASK-001-R1 finding 8 requires deliberate failed/truncated discovery/snapshot reads, no partial enqueue, then successful retry. Finding 2 requires incomplete selected SQL coverage to defer and recover. REQ-004 forbids treating failed/truncated reads as a complete result; AGENTS §11 requires relevant edge/negative proof.

**Locations:** remediation appended evidence row 8e; `groups.rs:324–335,470–523`; current proof `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:2790–2899` and groups.rs tests at 1020–1250.

grouped_reads_need_the_owner_grant removes query capability/table scope then restores it. This proves pre-stream denial/regrant. It never yields rows before terminal failure/truncation or omits a selected payload between independent reads, and never asserts complete() deferred enqueue. Groups unit tests prove timing, SQL predicates and value mapping, not read/defer/retry. Row 8e therefore claims broader closure than its evidence supplies.

**Consequence:** the changed producer of durable snapshots lacks required proof where partial reads could freeze incomplete evidence and advance coverage. Source tracing is encouraging: ScheduledQueryCaller::run_with requires valid terminal plus clean EOF; GroupPoller propagates errors instead of using partial local batches; missing input/direct-result branches defer. That does not assert the changed caller's queue/cursor/retry behavior.

**Minimum correction/proof:** extend the existing grouped real-server journey or narrow query-fault seam. Deliberately fail/truncate discovery or snapshot after rows, assert no partial group snapshot, clear the fault and assert one complete immutable snapshot on a later tick. Independently hide a listed input/direct-result payload from its later cut, assert deferral and recovery when visible. Reuse existing terminal/fault facilities and queue inspection; preserve genuinely missing selected queued outcomes as null. No general harness, parser, full suite or fault-management product is requested.

## Proof limits

Rust process-owned restart proof publishes prior frame results, restarts with an unfinished queued source execution and checks recovery/contribution once. Replica/restart checks preserve serial coverage. Python/TypeScript grouped stories and MCP media cover shared wire/retained projections; Rust owns process-lifecycle proof. Late-work and maximum-duration additions directly address their previous gaps. Ordinary stable 20/21 ceiling proof does not exercise SYS-001.

No finding requests exactly-once providers, extra source durability, frozen spans, availability past pinned finality, continuing pagination, TASK-003 pressure work, retained-schema upgrades or speculative hardening. The remaining items are source-local selection correctness and already-required failure proof.
