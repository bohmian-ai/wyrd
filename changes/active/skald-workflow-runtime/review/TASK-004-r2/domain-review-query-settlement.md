# Domain review: query settlement and durability

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `e86831e5ac784028f8022cc3faeee1c22b12c665`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
- Domain: bounded Bifrost query collection; opening cancellation; Oracle pre-stream settlement; running-query terminal ownership; leader/follower graph release; pod-loss recovery; scheduled and MCP sibling callers; shared memory release
- Review mode: source, cumulative diff, and recorded evidence only. No build, test, Cargo, or mise command was run.

The candidate identity was checked before and after inspection and remained `e86831e5ac784028f8022cc3faeee1c22b12c665`.

## Authority and boundary coverage

| Authority | Domain obligation | Assessment |
|---|---|---|
| `AGENTS.md` §§2, 5, 6, 10–12 | Oracle remains the durable query owner; cancellation and cleanup are bounded and structured; materially changed Rust documentation describes the actual lifecycle; journey evidence covers user-visible recovery | One documentation contradiction remains; runtime ownership and evidence otherwise align. |
| `architecture/agent-rules.md` | Reuse existing owners, avoid duplicate lifecycle machinery, retain journey proof, and keep Rust documentation accurate | `RunningQueryControls`, `BoundedQuery`, Oracle graph ownership, and `PeerCluster` are reused. The stale release-ack description violates the documentation rule. |
| `architecture/references/languages/spec-driven-development.md` | Approved human decisions and architecture outrank task prose or historical mechanics | Applied. The deleted graph-drain polling and supervisor idle refusal were not treated as missing, and no follower release acknowledgement was required. |
| `architecture/bifrost-design.md` §§Durability and visibility; Distributed analytical execution; Admission and memory; Read audit and terminal contract; Resource and failure invariants | One deadline and cancellation tree; terminal-safe results; leader-owned grant streams; follower-local asynchronous cleanup; shared-root memory accounting; no successor query attempt | Runtime paths align. One owner-level rustdoc still describes a superseded acknowledgement/retry lifecycle. |
| Revision 13 specification and TASK-004/R1 | Bounded `bifrost.query`, no partial successful result, tracked settlement after waiter abort, original deadline during open cancellation, honest pod-loss terminal, capacity recovery, and sibling availability | Covered in source and recorded evidence. Revision 13's provider tagging does not alter this domain. |
| Human decisions for this round | Graph-drain polling and idle refusal stay deleted; grant-stream close is follower release; leader does not await a follower release acknowledgement; tests may wait for follower cleanup | Applied without re-litigation. |

## Source coverage

| Boundary | Source inspected | Result |
|---|---|---|
| Cancel during open | `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:118-160,300-431`; callers in `query/collect.rs:281-317` and `query/scheduled.rs:132-180` | Cancellation routes once through the existing controls, and both the owner signal and retained open are bounded by the deadline fixed from the original request. `cancel_while_opening_ends_at_the_original_deadline` directly covers a slower cancellation owner. |
| Bounded complete-result owner | `crates/wyrd/wyrd-server/src/query/collect.rs:253-619` | `BoundedQuery` is the single Workflow/MCP collector. It rejects early EOF, invalid/trailing frames, failed terminals, row mismatch, and result overflow without returning retained rows; any pre-terminal error routes through `cancel_and_settle`. Test-only EOF faults now truncate at the selected frame. |
| MCP sibling | `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:220-250` | MCP calls the same `QueryArguments` validation and `BoundedQuery`; no second collector or query engine exists. |
| Scheduled sibling | `crates/wyrd/wyrd-server/src/query/scheduled.rs:88-255`; `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs:790-912` | Scheduled opening uses the same cancellation seam, and post-return proof distinguishes strict leader settlement from asynchronously completed follower cleanup. The bounded condition wait is test observation, not production release polling. |
| Running-query ownership and telemetry | `crates/vala/vala-bifrost-redux/src/oracle/running.rs:21-175,196-360`; `oracle/query_stream.rs:630-690,783-869` | Registry cancellation marks telemetry before signaling. Terminal removal follows leader-local distributed and Analytical settlement. Followers free their own graphs after grant-stream close. |
| Pre-stream Analytical failure | `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4375-4416`; `oracle/query_stream.rs:824-869` | `release_error` invokes the existing Analytical settlement before releasing residual admission and preserves the original public error. No alternate attempt or cleanup owner is introduced. |
| Leader/follower grant lifecycle | `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:1412-1465,2256-2276,2492-2528,2605-2770`; `oracle/dispatcher.rs:617-642` | The implementation matches the human decision: participant grants are stream handles; dropping them closes the streams; the follower settlement driver cancels and joins follower-local work asynchronously. See QSET-R2-001 for the contradictory owner rustdoc. |
| Shared memory and release | `crates/vala/vala-bifrost-redux/src/resources.rs:3530-3708,3745-3899`; `oracle/analytical_supervisor.rs:973-1011` | Query slot/admission release no longer polls or refuses on nested bytes. Late children retain the shared memory view and return their own charges; the view poisons only if dropped while still charged. The deleted bespoke polling/refusal machinery is correctly absent. |
| Pod loss and recovery | `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:96-269`; `oracle/peer_cluster.rs:392-426`; `oracle/peer_network/join.rs:101-185` | The Workflow journey now asserts `WYRD_VALA_500_QUERY_EXECUTION_FAILED`, no result columns, exact failed telemetry, surviving-owner baselines, killed-member disappearance, snapshot refresh, and a later three-row query. `await_membership` reuses the shared peer fixture. |
| Recorded verification | TASK-004-R1 implementation evidence and verification table | The recorded focused and broad Bifrost/server lanes are consistent with the inspected source. They were not rerun. |

## Prior-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `QSET-001` / `FIND-TASK-004-3`: cancel during open could exceed the query deadline | `open_cancellable` derives one monotonic instant from the original request deadline and `cancel_while_opening` applies `timeout_at` to the cancellation request plus the same retained open; the paused-time test proves no fresh cleanup budget | **CLOSED** |
| `QSET-002` / `FIND-TASK-004-4`: pod-loss branch lacked exact settlement and recovery proof | The pod-loss branch now checks the exact stable code, no rows, every surviving Oracle's baseline, membership departure, snapshot refresh, and a later successful query | **CLOSED** |

## Material findings

### QSET-R2-001 — DRIFT: the graph lifecycle owner still documents a nonexistent release-acknowledgement retry protocol

- **Violated obligation:** The human-approved follower lifecycle is grant-stream close: the leader drops each `ParticipantGrant`, does not send or await a release acknowledgement, and the follower independently cancels and settles its graph. Repository Rust documentation must describe the actual owner, operation, and invariant. Historical bespoke machinery that neither the implementation nor approved architecture uses must be deleted rather than preserved as an implied requirement.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:2492-2505`.
- **Evidence:** The type documentation says a release whose acknowledgement did not arrive "has to be retried" and that the lifecycle task avoids a detached timer, second registry, or status RPC; the `transports` field says it issues both reserve and release. The implementation has no release RPC or acknowledgement: `AnalyticalParticipantGrants` owns stream handles (`analytical.rs:2256-2266`), settlement performs `drop(grants)` (`analytical.rs:2740-2742`), and `ParticipantGrant` owns the response stream (`oracle/dispatcher.rs:617-642`). The corrected method documentation at `analytical.rs:2689-2699,2755-2757,7769-7781` and `oracle/query_stream.rs:783-795` explicitly says follower cleanup may finish after leader settlement returns.
- **Observable consequence:** A maintainer reading the owning type receives the opposite lifecycle contract from its methods and governing human decision. The stale description invites reintroducing an explicit release RPC, retry timer, status check, or acknowledgement wait—the exact unsupported bespoke machinery this remediation must not require.
- **Required correction:** Delete the acknowledgement/retry/status-RPC narrative and describe the existing native mechanism only: the lifecycle task retains participant grant streams; dropping them is the release; followers settle asynchronously; the transport directory is used for reservation/admission, not an acknowledged release operation. Do not add a release request, acknowledgement, retry, timer, setting, poll, or test mechanism.
- **Focused closure proof:** Source review of the corrected owner and field rustdoc against `AnalyticalParticipantGrants`, `ParticipantGrant`, and `AnalyticalGraphLifecycle::settle`. Existing scheduled and forwarded journeys remain the behavioral proof; no new runtime test is needed for a documentation-only correction.

## DRIFT assessment

No production mechanism, check, file, setting, or option added in this domain requires a nonstandard Wyrd-only lifecycle protocol. `timeout_at` over one absolute deadline, cancellation tokens, task ownership, stream-handle lifetime, shared reference-counted memory ownership, and condition-based journey waits are established native mechanisms. The removed nested-child poll and supervisor idle refusal remain deleted as directed. QSET-R2-001 is retained because it documents, and could wrongly resurrect, a release-ack protocol that is absent from both the implementation and the approved decision.

## Verification evidence and limits

The remediation records successful focused coverage for cancel-during-open, Workflow forwarded cancel/deadline/pod loss, scheduled query cleanup, MCP collector behavior, Oracle journeys, and the complete Bifrost lane, followed by the server journey and Oracle journey after the follower-release documentation/test adjustment. It also records passing format, lints, codegen, boundary checks, and broader Rust/Python/TypeScript lanes.

This review did not execute any command that builds or tests code. Recorded results were checked only for correspondence with the named source and assertions. The exact runtime timing remains supported by the recorded evidence, not independently reproduced here. The candidate remained immutable during this review.

## Overall result

**FAIL**

The runtime settlement and recovery gaps from the first review are closed, including the fixed human decisions about follower cleanup and deleted polling/refusal machinery. The owning Analytical lifecycle type still states the opposite release protocol, so the candidate does not yet satisfy the repository's mandatory documentation accuracy or the approved no-acknowledgement lifecycle without a bounded documentation correction.
