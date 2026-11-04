# System resilience review

Result: **FAIL for the explicit AC-038 recovery-proof gap below.** No source-proven production failure is proposed. The ordered admission, ordinary failure propagation, seen-cache lifecycle and publication selection paths inspected are sound by source. This result must not be described as demonstrated data loss.

## Subject and scope

Repository: /home/thorrester/Documents/GitHub/wyrd. Base: 9692bfd06. Candidate: ed82f63bf35ca124c0a2980c160f0d64535eebd1. HEAD was checked and remains that candidate. First full-range review; no prior acceptance carried forward. Source unchanged.

Reviewed actual Git range and the shared scope map. Read task-review instructions, root repository rules, changes/spec-driven-development.md, agent_docs/index.md, rust.md and maintainer-style.md; architecture/README.md; owning Bifrost system boundary, telemetry series and durability/visibility sections; operations role/readiness/storage and recovery sections. No nested AGENTS.md applies to these Rust directories. Read revision-27/26 specification, TASK-022 and relevant TASK-004/Bloom task obligations. CodeGraph exploration preceded source searches.

This specialist covers Scribe ordered series admission and failures/recovery, process seen-cache, historical tagged WAL regression, Oracle publication/cut-cache interaction and the changed published-test synchronization. Persistent identity encoding, external fork internals and UI behavior belong to other reviewers.

## Deployed paths and authority

The architecture makes wyrd-server the serving/lifecycle owner; one process may contain Scribe, Oracle and Forge, or those roles may run on separate pods. Each durable Scribe has its stable identity and its own persistent WAL/staging volume; Oracle depends on Postgres/object storage and live Scribe routing. See architecture/bifrost-design.md:15-58 and architecture/operations/README.md:45-71.

The changed production ingress path is:
ScribeImpl::prepare_and_dispatch -> prepare_and_dispatch_under -> decode/preprocess/duplicate verification -> admit_series_first -> the same ordinary prepare_and_dispatch for metrics.series -> durable completion -> seen-cache remember -> points shard try_send -> points durable completion.
Evidence: scribe/ingress.rs:577-636,685-824 and scribe/series_index.rs:145-201.

The series helper uses the existing Scribe ingress owner, not a new WAL, fence, or publisher. It derives the series batch identity from the points batch and selected hour/id keys, and forwards the principal/context. Series rows are derived from prepared, stamped points (tables/metrics/series.rs:495-576). Gate refuses client writes of the server-owned table (gate/mod.rs changed admission guard).

The ACK awaited by each ingress is sent after WAL sync, durable control fences and visible insertion; a committed batch is entered in the shared retry index only after insertion succeeds. Evidence: scribe/shards.rs:3300-3367,3766-3835,4054-4111. This is the required existing durability boundary; no cross-table transaction was added.

Oracle production cache and publication behavior are not changed in this range. The test synchronizes with existing state transitions. The cached-cut owner describes notification/liveness/fill fencing in catalog/cut_cache.rs:1-26,85-124. Discovery compares each captured Scribe's publication revision with the pinned cut, evicts a stale selection, and retries before exposing rows (oracle/mod.rs:2887-2903,3093-3099).

## Credible failure and recovery paths

| Interruption | What stops / survives | Recovery and affected capabilities | Assessment |
|---|---|---|---|
| Series validation, decoded-size or admission refusal | The outer prepared points never reach try_send; no series key is remembered. Local held resources drop with the refused future. | Caller retries the points batch through ordinary ingress. Other tables and requests retain their existing owner behavior. | Source satisfies failure ordering. New a_failed_series_admission_fails_the_points_admission explicitly tests decoded-size refusal and same-ID retry. |
| Cancellation while waiting for series completion | The series shard may finish after its request future is dropped; the points are not yet dispatched. The cache may remain cold. | A retry may write/recover the series again, which is explicitly permitted. No acknowledged points are made dependent on uncommitted series state. | Source satisfies ordering; not a fresh cancellation execution. |
| Crash after series ACK and before points dispatch | Series WAL/fence survives on that Scribe's volume; there are no dispatched points from this attempt. Process cache is lost. | Startup restores staged publications and WAL before recovery readiness. Retry uses ordinary fencing; extra series rows after a cold restart are permitted. | Source supports safety; the required actual kill/restart/read proof is missing. |
| Crash after points dispatch and before points ACK | Series ACK already completed. Points may be absent or recoverable from their own WAL/fence. | Stable-volume restart recovery reconstructs each table's ordinary authoritative batches. No new independent replay owner bypasses the existing path. | Source supports safety, including different routing shards; no changed-path kill/restart proof. |
| WAL sync failure / ambiguous fence outcome | Existing WAL/control-fence machinery prevents ACK; uncertain work remains retained/recoverable and admission may close at its existing governed boundary. | Restart replay is the established recovery; the candidate does not panic the shared server or introduce a new process-wide shutdown. | No new fault amplification identified. Existing failure machinery is reused. |
| Seen-cache overflow, restart or poisoned cache lock | Overflow clears bounded entries; restart starts empty; poisoned lock answers unseen. | Only more series writes result. Entries are inserted only after ordinary series admission returns success. Concurrent misses may create duplicate series rows, permitted by authority. | Source satisfies the explicitly approved cache contract (series_index.rs:33-89,180-200). |
| Publication advances after Oracle pins a cut | Capture revisions can prove the cut stale. No rows from that discarded selection are produced. | Existing eviction/repin and retained captures preserve exact rows; LISTEN loss disables cached reads rather than inventing freshness. | No changed production selection behavior. Test retains exact-row, exactly-one-repin and capture-release assertions. |
| Historical WAL retains sensitivity metadata | Replayed data keeps its original Arrow payload/schema, stored fingerprint, request/batch identity and digest. | The newly added tagged_wal_replays_without_identity_changes checks original fixture identity/digest/field IDs and byte-for-byte unchanged WAL. | Appropriate targeted recovery regression; inspected proof, not freshly executed here. |

Startup evidence: scribe/mod.rs:2575-2670 restores staged state/publication revisions, streams WAL through existing shard owners, leaves recovery_ready false on failure and only sets it true on success. The added series_seen field initializes with an empty default at :1251. No mechanism restores cache hints as durable authority.

## Changed published-test synchronization

At wyrd-testing/tests/bifrost/oracle/published.rs:2106-2122, the test now waits within SOURCE_TRANSITION_BOUND until file_tier_counts reports zero uncompacted hot files for this test's unique table, then retains the existing warm-cache probe. file_tier_counts reads the durable compacted flags (oracle/support.rs:926-946). This replaces an assumption that the earlier Forge settlement has finished with an observation of that state.

The measured case still pauses the query after pinning, publishes the next rows, releases it, requires exact identities, requires exactly one cut read for the repin, and checks capture release (:2123-2170). It has not weakened its measured publication assertion. The change adds bounded polling of a real lifecycle condition; it does not add a production retry or change cut safety.

The caller supplies the commit diagnosis that case-1 promotion settlement invalidated the cache after it had been warmed, adding an unrelated cold read; the user reports a read-only diagnostician confirmed it. No separately inspectable diagnostician report or trace was supplied. The state/notification source corroborates why that race is credible, but this report does not claim to independently reproduce its historical timing or inspect an unavailable artifact.

## Acceptance and proof assessment

| Obligation | Implementation / proof | Assessment |
|---|---|---|
| Revision 27: series durable before points dispatch; both before points ACK | ingress.rs:800-824, series_index.rs:191-200, existing shard ACK boundary | Source satisfied |
| Failed/unacknowledged series admission refuses points; ordinary whole-batch retry | Same propagation; persistence-path test :1278-1320 | Source and inspected focused refusal proof satisfied |
| Bounded per-process suppression; no extra series sync when all keys seen | series_index.rs:33-89,180-200; persistence-path test :1229-1263 | Source and inspected normal-path proof satisfied |
| AC-038: kill between admissions / before acknowledgment, restart and read | No test/evidence exercising this changed path located | **Not satisfied as required proof** |
| Preserve old acknowledged WAL identity without rewriting historical payload | replay.rs:2303-2413 tagged fixture regression | Inspected test directly asserts obligation; execution is caller-reported only |
| Published exact rows across source transitions | Published journey retains all critical assertions; only settlement precondition added | Source satisfied; execution is caller-reported only |

## Proposed material finding

**SYS-01 — MISSING: explicit crash-recovery acceptance proof for ordered series admission.**

Obligation: specification revision 27, lines 30-31, explicitly preserves AC-038; specification :101 requires killing the server before acknowledgment and then reading, with no durable point lacking its hour's series row. The operations recovery boundary requires the stable-volume WAL/staging recovery path, rather than in-memory observation alone.

Location: scribe/tests/scribe_persistence_path.rs:1229-1320 contains the new series persistence tests; the failure test refuses the series before dispatch through a decoded-size limit and retries in the same live embedded Scribe. Neither test kills/restarts a server, reconstructs Scribe/WAL state, or reads after interruption. Searches of the original task evidence found no AC-038 kill/restart result; the orchestrator independently confirmed no such evidence was located.

Evidence consequence: the submitted proof can remain green even if the newly composed two-table path fails during crash/replay handoff. This is a required durability-proof omission, **not a demonstrated implementation data-loss scenario**. Source tracing supports the ordering and supplies no basis for claiming actual lost rows.

Smallest correction: add and execute one focused interruption/restart/read proof using the existing Scribe/WyrdTestServer recovery facilities and the same retained volume. Interrupt the composed path at the specified boundary, restart through ordinary recovery, and check recovered points against their tenant/hour/series keys; retain a points-admitted-before-response case so the assertion is not vacuous. Reuse existing fault/pause facilities where sufficient. No new durability mechanism, process-wide fail-closed behavior, cross-table transaction, production recovery layer or public contract is requested. This recommendation is a proof addition within AC-038, not a new specification approval gate.

## Limits

No runtime tests were started by this reviewer; costly execution was coordinated with the orchestrator. Reported library/journey/fmt/clippy passes are supplied claims, not fresh executions here. Static inspection does not prove actual kill/restart behavior. No implementation defect or optional resilience machinery is proposed beyond the explicit missing acceptance proof.

Skipped fresh runtime/fault execution and external-fork inspection; AC-038 remains unverified until the required composed recovery proof runs.

