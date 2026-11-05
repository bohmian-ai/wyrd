# Focused follow-up: durable interruption boundary

Result: **RESOLVED**. The source resolves the timing dispute: **FIND-TASK-004-7 remains open as missing required proof**. This is not evidence of production data loss.

Subject: original base `9692bfd06c6bdf2585e88c791884a90669a5dc26`, prior candidate `ed82f63bf35ca124c0a2980c160f0d64535eebd1`, candidate `0c2bfe70d5186dff4dc224c7e402b57c24f354d9`. Read the round scope, prior validated F7 and remediation obligation, candidate helper/tests, WAL append/sync/fault operations, shard processing and lifecycle, ingress ordering, and replay entry point. CodeGraph was consulted before code search. No source edits or execution.

## Governing obligation

`spec.md:20–31` preserves ordered durable series admission and AC-038; `spec.md:105` requires killing between point admission and acknowledgement and reading without an unindexed durable point. Prior `findings-validation.md:98–106` and `TASK-004-R1-metrics-correctness-and-index-coverage.md:103–111` narrow F7 to actual retained-volume interruption after durable series/before points, and durable unanswered points with nonvacuous recovery. No second restart or long-term object-store proof is part of this follow-up.

## Source evidence

All Rust paths below are under `crates/vala/vala-bifrost-redux/src/scribe/`.

1. `tests/scribe_persistence_path.rs:1539–1568`: `crash_once_durable` polls `replay_wal_directory`; the sole exit condition is a readable matching table state with nonempty `commits` and the optional batch match. The helper never observes sync completion, fault consumption, a retained post-sync owner, or owner termination. Its select detects a response only while the condition is false. `replay.rs:165–175,195–204` collects replayable readable WAL state; it does not observe the live writer's sync completion.
2. `execution_lanes.rs:2304–2316`: `WritePrepared` appends and returns `WalWritten`; `SyncWal` is a separate operation calling `sync_segments`. `wal.rs:2474–2547` appends a record and returns touched segments; `wal.rs:1733–1746` uses file writes without record fsync. `wal.rs:2551–2558` performs segment fsync separately. Readability of a complete COMMIT therefore does not imply that the required sync completed.
3. `shards.rs:3792–3829` awaits `WritePrepared`, collects segment handles, and only then awaits `sync_group`. `shards.rs:4484–4504` submits and awaits `SyncWal`. The post-sync fault is consumed later, at `shards.rs:3285–3296`, after `append_and_sync_batch_commits` has returned, before committed hot insertion at `3298–3299`. It moves the group, including its response waiters, into `retained_commit_ambiguity` and returns an error rather than answering. This is an appropriate existing deterministic hold boundary, but the new tests only arm it (`1622`, `1687`); they never establish that execution reached it.
4. `execution_lanes.rs:2198–2228` spawns detached Rayon IO and awaits its oneshot result. The spawned closure can finish after its submit waiter is dropped. `shards.rs:1096–1111` starts owners as Tokio tasks and retains their ordinary join/abort handles. There is no `Drop` implementation on `ScribeImpl` or `ScribeShardRuntime` that invokes cancellation. `shards.rs:1397–1465` can finish its current group before noticing closed channels; `1428–1438` returns to the owner loop after a group error. Dropping ingress cancels its wait, not the shard's admitted work (`ingress.rs:803–811`). Thus `drop((scribe, wal))` at test lines `1630` and `1695` does not establish an immediate stopped old owner before the next epoch starts at `1632` and `1697`.
5. Existing explicit stop owner: `mod.rs:1450–1476` provides `abort_shutdown`, documented to skip graceful flush and leave unfinished work to replay. Its finalizer calls shard `abort_retained` (`1481–1482`), whose implementation cancels retained owner tasks (`shards.rs:961–977`), and closes WAL streams (`mod.rs:1490`). Abort requests alone do not prove cancellation has completed; the retained task/lane lifecycle must be observed before opening the next writer. No production lifecycle redesign is required.

## Reachable counterexample to the claimed proof

The IO thread finishes writing the terminal COMMIT while the shard is still awaiting `WritePrepared`'s result. The test's next synchronous WAL scan sees that COMMIT, exits its loop, drops the ingest future, then drops Scribe. At that point `SyncWal` has not necessarily been submitted, and the post-sync hold need not have been reached. The already-running owner and detached IO are allowed to complete later in the same test runtime. Opening the retained directory in epoch 2 can recover bytes readable from the operating system even though the test never demonstrated an interruption at the claimed durable boundary. The asserted row counts can all pass in this schedule.

This is a missing happens-before relation, not an assertion that every execution takes this schedule. The reported passing recovery cases prove useful retained-WAL reconstruction and point/index assertions, but cannot establish the absent timing/termination observations. Repeating them would not repair that logical gap.

## Acceptance assessment and smallest correction

| Required proof | Existing evidence | Result |
|---|---|---|
| Matching series/points batch reached completed sync and unanswered hold | Fault armed; readable COMMIT detected before independently awaited sync | FAIL |
| Old owner interrupted before next writer begins | Borrowed admission and Scribe dropped; detached owner/IO completion not observed | FAIL |
| Ordinary replay over retained directory with nonvacuous recovered points and index coverage | Next-epoch `replay_wal_async`, one series row and eight recovered points, context membership assertions | PASS for reconstruction/assertions; does not substitute for the first two rows |

Preserve stable finding **FIND-TASK-004-7**, classification **MISSING required proof**. The material consequence is that the explicitly required interruption case remains unverified; no production corruption is alleged.

Reuse the already-existing post-sync failure hold and abort lifecycle. Replace readable-COMMIT polling as the crash trigger with positive, non-consuming observation that the targeted admission has reached the retained post-sync/pre-insertion state, while its response remains unanswered. Ensure that observation is published after the group has been retained, rather than observing only fault arming or the earlier flag swap. Then explicitly stop the old owners through the existing abort path, observe their completion and any already-submitted WAL work before opening epoch 2, and retain the current ordinary replay, retry, nonvacuous point-count and context-coverage assertions. A narrow test-tier observation of the existing hold is sufficient; no new recovery mechanism, production transaction, process shutdown policy, broad crash matrix, second-restart fixture, or persistence redesign is warranted. A sleep, WAL-readability scan, sync-submission counter, or successful replay alone cannot prove the selected boundary.

Skipped runtime reproduction and broader storage scenarios. Risk: acceptance currently overstates the exact interruption boundary despite green recovery tests.
