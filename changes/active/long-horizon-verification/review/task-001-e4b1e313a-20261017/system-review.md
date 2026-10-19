# TASK-001 system-resilience review

Overall result: **FAIL**.

## Immutable subject and authority

- Worktree: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Base: `ef3a57b07`; candidate: `e4b1e313aacab50d7eb0f0a2060b8547bf19a632`.
- Reviewed cumulative `git diff ef3a57b07...e4b1e313a`, expanding the runtime changes into their existing callers and recovery owners. HEAD still names the candidate.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-001-grouped-verification.md`.
- Authority: approved revision 13 behavior in `spec.md` (current file declares draft revision 14; no new exception to revision 13 recovery/coverage is inferred); AGENTS.md; architecture/agent-rules.md; architecture/wyrd-design.md; architecture/wyrd-doctrine.mdx; architecture/bifrost-design.md; references/languages/{spec-driven-development,maintainer-style,testing-workflows}.md; references/doctrine/architecture-constraints.md; references/domain/analytical-operations-reliability.md.
- TASK-002 schedule input simplification and TASK-003 new outbox admission/pressure/progress are excluded. The findings below concern TASK-001 discovery, coverage and its specified overflow outcome, not the other tasks' process-pressure obligations.

CodeGraph was consulted first for runtime navigation (the broad groups query was unhelpful; the ScheduledQueryCaller query located its existing owner and consumers). No source, tests or production configuration were changed. No tests were run by this reviewer; the orchestrator owns sequential Cargo verification.

## Deployed paths and recovery boundaries

| Path | Owner, deployment and surviving authority | Failure and recovery assessment |
|---|---|---|
| Grouped direct verify / submit | Observe and shared client project group_id; VerificationControl stages attributed existing execution-input/results writes on process ScribeOutbox; submitted work is admitted by the existing submission writer into Postgres. | Producer grouping adds metadata, not a new receipt/durability boundary. Abrupt loss before Scribe acknowledgement remains allowed. The production outbox diff only adds group_id to its test builder, preserving this existing lifecycle. |
| Discovery | VerificationRuntimeBuilder::build composes GroupPoller only with hosted blocks, while using the existing VerificationScheduler. Scheduler claims one binding under TenantConn, resolves BackgroundAuthority, queries Bifrost locally or through Oracle peers, inserts snapshots, commits cursor and queue work together. | Query failure becomes GroupPollError::Read; scheduler logs it and commits cadence advancement so a later tick retries. SQL failure aborts the tenant transaction. Neither path intentionally crashes the shared server. However candidate discovery can permanently omit retained eligible rows after lag/outage (SYS-001, SYS-002). |
| Snapshot and serial admission | GroupPoller identities→refused_submissions→Ended→group_states→snapshot→enqueue_group. Existing queue persists inputs, through, members and source IDs. The partial unique index fences outstanding work per tenant/binding/group; binding claim uses SKIP LOCKED. | Crash before commit rolls back cursor and snapshot; after commit another runner can claim the same frozen snapshot. Outstanding checks and unique index prevent parallel group reviews. Physical-row cutoff can prevent a required terminal overflow outcome before this owner is reached (SYS-003). |
| Execution | VerifierRunner::dispatch executes ExecutionInput::Grouped using its stored inputs, or completes inconclusive when source coverage marks overflow. Existing ClaimLoop and LeaseRenewal own claims, drain, lease expiry, retry and release. | Restart recovers a committed snapshot without rereading its sources. Engine timeout/refusal is contained to its execution; settlement retries while the lease holds; stale lease cannot settle. Under shutdown, abandoned work releases or remains reclaimable after expiry. No new group-specific runtime or queue was introduced. |
| Settlement, results and reactions | Queue complete stores group verdict and compares previous settled same-group result; baseline dispatches nothing, changed verdict inserts dispatches atomically. ResultPayloadBuilder stages the immutable result through existing outbox. | Group queue coverage survives even while result visibility lags or its staged write is lost. Existing Operator retry/dedup ownership is retained. But discovery's completed-result timestamp shortcut prevents some legitimate next snapshots from ever reaching this correct downstream guard (SYS-001). |
| Live Agent span query | Agent check tool session calls HostedTools with accepted caller; BifrostQuery uses existing authorization, query bounds and deadline. | It remains a live read outside pinned inputs. Query errors use existing block failure/retry handling rather than manufacturing a pass. Replay can see different live spans, explicitly permitted by REQ-002. |

Existing alternatives were traced: ScheduledQueryCaller already provides deadline/terminal-safe query consumption, and groups correctly reuse it; VerificationScheduler already owns binding cadence/claim; VerifierExecutionQueue already owns execution retry, lease/fence and settlement, and groups extend it instead of introducing another queue. GroupPoller earns its new source-selection responsibility. The existing `group_states` / `GroupState::covers` is the correct retained-coverage authority; the added candidates prefilter is not an equivalent substitute.

## Proposed material findings

### SYS-001 — INCORRECT: completed-result time suppresses uncovered late-visible source rows

**Obligation:** REQ-003/004 and AC-006 require uncovered rows within finality to produce a subsequent whole-group snapshot, including activity arriving during an outstanding review and Bifrost visibility lag. Revision comparison uses source coverage, not the time the reviewing engine finished.

**Location:** `crates/wyrd/wyrd-server/src/verification/groups.rs:222–229`, especially `HAVING ... MAX(s.at) > j.judged` at line 228. Existing correct downstream owner: `wyrd-sql/src/queries/verifier_executions.rs:985–994` (`GroupState::covers`) and `groups.rs:170–178`.

**Reachable producer → consumer:** direct and queued source writes retain server recorded_at/ended_at via ResultPayloadBuilder and ScribeOutbox. Review A snapshots source execution 1. While A is running, execution 2 finishes at t=20 (or an earlier-timed source result becomes visible late). A finishes at t=30 and its group result becomes visible. On the next discovery, all source row times are at most t=20, and latest group result ended_at is t=30. The HAVING predicate rejects the group. Thus group_states never observes the increased members/newest source coverage, even though its `covers` method explicitly handles late-visible earlier-timed rows. The same defect affects whole-slot sources and requirement-outcome sources, every source producer, and all SDK/HTTP/MCP callers; it is one discovery defect, not separate caller bugs.

**Observable system consequence:** review A remains the last review indefinitely; the coalesced/late source is retained and eligible but never judged. A changed source outcome cannot update the group verdict or dispatch its specified reaction. This occurs under ordinary slow Agent reviews and asynchronous visibility, without any crash or overload.

**Smallest correction:** remove completion-time suppression as proof of coverage in BindingPoll::candidates. Let bounded discovery reach the existing queue-backed source-coverage check (through plus distinct membership), or use genuinely equivalent source-coverage filtering. Retain bounded reads, owner/source tuple filters, finality, immutable snapshots, existing serial queue fencing and cadence. Do not add a group lifecycle store or a new producer write.

**Focused closure proof:** hold a real first group review outstanding, produce/flush another eligible source whose recorded activity precedes that review's eventual ended_at, finish/flush the first review, then assert exactly one next serial snapshot includes both IDs and prior history remains intact. Include a late-visible earlier-timed result variant to exercise the same producer source. This proof belongs in the existing grouped journey owner; it needs no parallel harness.

### SYS-002 — INCORRECT: a one-hour discovery horizon permanently loses retained groups after an outage

**Obligation:** INV-001, REQ-003/004 and AC-006 preserve background rediscovery/retry/recovery from existing retained input/result evidence. The only specified history limits are finality, evidence retention and accepted outbox loss. Fixed-cadence delay is not a one-hour eligibility expiry.

**Location:** `groups.rs:53–57`, `groups.rs:218–221`; the code's own comment states that groups ended longer ago are not rediscovered after a scheduler outage.

**Reachable producer → consumer:** a source call is successfully retained in Bifrost at t=0, with idle=30m and finality=30d. The process/pool hosting grouped verification is unavailable until t=2h (or the query dependency remains unavailable for that period). Claim/cursor state survives in Postgres; source evidence survives in Bifrost. Recovery computes since=t=30m, excludes the t=0 rows from both source surfaces, and returns no candidate. Every later tick moves since forward, so the eligible unjudged group can never recover unless the application sends additional source activity. Neither snapshot nor queue admission is reached.

**Observable system consequence:** a finite scheduler/Oracle outage becomes permanent loss of group verification even though evidence was retained and no finality deadline was exceeded. Inline APIs and unrelated server capabilities can remain healthy, obscuring the missing work. Widening the constant only moves the same failure threshold.

**Smallest correction:** remove the arbitrary age eligibility cutoff and discover unjudged/uncovered retained groups with bounded discovery reads under the existing binding scheduler and queue coverage owners. Preserve actual finality/retention and finite query resources; do not add per-source inventories, a new lifecycle table, producer counters or durability receipts. The implementation must not use bounded work per tick to silently expire required work.

**Focused closure proof:** retain a source row older than idle plus one hour but still within finality, with no group execution; run recovery polling using existing fixture time/cursor controls and assert it gets one snapshot/result. Repeat polling to prove no duplicate. A dedicated long wall-clock sleep is unnecessary.

### SYS-003 — INCORRECT: physical-row cutoff bypasses the required source-ceiling result

**Obligation:** REQ-009 mandates exactly one deployment ceiling measured in distinct eligible execution IDs, after deduplication/exclusion. Exceeding it completes the group inconclusive/source_execution_limit_exceeded; physical rows/retries are not the unit. REQ-004 forbids treating truncated reads as complete selection.

**Location:** `groups.rs:59–61`, `groups.rs:159–165`, and `groups.rs:263–272`. The separate source-execution ceiling is applied only later at `groups.rs:293–327` and converted to the required outcome by `runner.rs:420–429`.

**Reachable producer → consumer:** at default source ceiling 1,000, a legitimate group with 50,001 direct source executions can have 100,002 retained input/result identity rows. `identities` returns 100,001 physical rows; poll immediately logs/skips before refused-submission removal, identity deduplication, per-source ceiling detection, snapshot pinning or enqueue. Every later poll encounters the same retained prefix. The same issue can arise with more physical rows than distinct eligible executions, because deduplication and refused-row filtering occur after the guard. Neither normal ceiling configuration nor legitimate producer admission forbids this retained group size.

**Observable system consequence:** a required inconclusive result is replaced by permanent absence of a group execution/result. With 64 older such groups, the fixed earliest-64 candidate page can also repeatedly exclude a later ordinary eligible group; the oversized groups do not drain over successive polls as the comment claims. This is discovery correctness, not a request to implement TASK-003's pressure system.

**Smallest correction:** make bounded identity discovery count/select distinct eligible executions and detect the existing configured ceiling before payload reads. Use that existing overflow representation and runner outcome for oversized source references, preserving finality, owner/source exclusion, refused submissions, ordering and immutable coverage. Remove the independent physical-row skip as an alternative terminal policy. Ensure groups that cannot proceed do not permanently occupy the same complete candidate page and block other binding/group work. No new public limit, authored budget or second queue is required.

**Focused closure proof:** with retained physical input/result rows exceeding the internal row cap and distinct eligible membership above the configured ceiling, assert one completed inconclusive result with the named source/ceiling and no payload evaluation. Also prove physical duplicates/excluded submissions do not consume the distinct-execution ceiling; seed rows through existing fixture owners rather than generating host load. A later normal group must remain discoverable when older oversized groups are present.

## Proof and limits

The appended implementation record claims all five grouped Rust/Python/TypeScript journeys plus MCP/OpenAPI and static checks pass. Source inspection confirms Rust `grouped_sampled_frames_are_bounded` flushes the first 19 results before restarting a server with one unfinished source execution, then asserts recovery and final ordered outcomes. `grouped_conversations_recover` checks no duplicates after a replica and restart, but those restart checks follow already-settled visible reviews. They do not simulate a discovery outage exceeding the new horizon or a source timed before a concurrently completing group review. The ordinary 20/21 frame ceiling proof does not reach the independent 100,000 physical-row guard.

`ScheduledQueryCaller::run_with` consumes to a validated terminal and cancels/settles failed streams; GroupPoller returns the error without using its locally collected partial batches. Scheduler catches read errors and retries cadence. This source path supports containment and refusal of a failed/truncated individual query. There is no dedicated TASK-001 journey assertion for this required failure/recovery edge; the implementer explicitly records only incidental teardown logs. No assertion here treats those incidental logs as completed recovery proof.

The snapshot is persisted before queue execution, runner retry reuses it, and queue settlement fencing protects it. Process-memory outbox loss remains accepted, not a finding. This review does not require node-loss receipts, exactly-once provider calls, frozen live spans, retained-table upgrades, synthetic load, or any TASK-002/003 behavior.

No prior review findings were supplied for closure. The proposed ledger is SYS-001/SYS-002/SYS-003; independent validation must confirm or revise each against approved authority before it enters the final verdict.
