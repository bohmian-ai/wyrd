---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 3
parent_task: TASK-001
remediates: [FIND-TASK-001-3, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8]
---

# Use the async Parquet stream for staged live reads

## Authority and subject

Implement under approved [spec revision 3](../../spec.md) and [original TASK-001](../../tasks/TASK-001-unified-scribe-live-query.md). The [validated re-review ledger](findings-validation.md) is the diagnosis source. Reviewed base `d1ec13200d332745af2fed8069a21d5b5c39cb47`; candidate `f1f1d5ebd264e8f9ac861ec79c6340da44a7a1a8`. Reassess the full cumulative range after implementation. Use `$wyrd-implement` for execution.

## Diagnosis and selected implementation

**FIND-TASK-001-3 and FIND-TASK-001-7 have one architectural source: the staged reader hands work to a detached blocking task while the Scribe stream owns admission.** `StagedTailReader::run_batches` filters empty batches inside a synchronous iterator (`scribe/staged_tail.rs:87-114`), so one `next()` may decode an entire zero-match run. `StagedRunWindows` then sends that `next()` to `spawn_blocking` through a oneshot (`:126-205`); dropping the stream cannot stop the task, but it does release the Scribe follower grant (`wyrd-server/src/oracle/peer_service.rs:311-353`). Its staged-file lease survives, so file deletion is protected, while decode work is no longer admitted. The current cancellation test covers only a nonempty first batch. Fix the ownership split by deleting this handoff, not by adding a cancellation guard or copying admission into it.

Use the already-installed Parquet async API (`parquet = 59.2`, `async` enabled) for each leased staged run. In `scribe/staged_tail.rs`, replace `StagedRunWindows`, its boxed iterator, `on_blocking_pool`, oneshot, and production synchronous `run_batches` path with `ParquetRecordBatchStreamBuilder` over `tokio::fs::File`. Open the file asynchronously, derive the existing name-based projection mask from the builder's schema, retain the required-column reorder and signed predicate evaluation, use the existing 8,192-row output batch size, and read with `StreamExt::next().await`. Keep one live Scribe stream in `LiveTailBatches::into_stream` (`scribe/tail_rpc.rs`): it iterates leased runs, pulls one Parquet batch at a time, applies the signed predicate, skips an empty result only **after** that awaited batch, and yields a nonempty batch before pulling again. Preserve errors as terminal faults. Do not collect a run, row group, or whole fragment into `Vec<RecordBatch>`.

The Scribe output stream continues to own its existing `ScribeFollowerLease`; the live producer owns the staged-file lease and the Parquet stream. Dropping the query child drops that reader and starts no later batch or run. Tokio may finish a file operation it has already started internally, but Wyrd must not leave its own detached decoder running or release file protection while an outstanding read still needs the file. Confirm the async file/read drop behavior in the focused cancellation proof; if an internal pending read outlives the owner, keep its existing file protection and admission charged only until that read completes using the narrowest existing ownership mechanism. Do not reintroduce per-window worker tasks, a new deadline, or a second admission system.

**Bounded memory is an outcome, not a one-window requirement.** Parquet's async reader may buffer a selected row group. Staged runs come from the existing writer, which admits at most 32 MiB of canonical Arrow input and 131,072 rows per row group (`parquet/memory.rs`), and Scribe already grants 256 MiB per follower (`resources.rs::ORACLE_PARTITION_MEMORY_BYTES`). Reuse the staged writer/footer validation and the current grant; verify the async reader's actual peak memory for a maximum-width row group, including page buffers, predicate/reorder output, and one transport batch. If the reader can exceed that grant, enforce the bound at the existing staged-file admission/reader boundary and fail as a resource fault; do not silently permit unaccounted buffering or restore the window handoff. The check must show one large run and a zero-match predicate cannot make memory grow with the number of batches or runs. The async API also performs CPU decoding while polled: prove a worst-case admitted group does not starve unrelated Tokio tasks; if it does, stop and report that the selected async path does not meet the runtime constraint rather than hiding it behind new custom workers.

**FIND-TASK-001-5, -6, -8 are direct source fixes.** `oracle/follower.rs:1171` and `oracle/exec.rs:6730` add function-local imports despite the module-top rule. The new `oracle/tail_discovery.rs:11-42` uses qualified types in fields and signatures despite the bare imported-type rule, and its `discover` `# Errors` paragraph names `BifrostError::QueryVisibilityUnavailable` although the method returns `TailReadError`. Move the cited imports to their module blocks, use bare imported types in discovery, and document the actual `TailReadError` classes. Keep runtime error mapping unchanged.

## Constraints and non-goals

Preserve the one leader-owned query deadline and cancellation, authenticated signed assignments, pull-driven bounded streaming, staged-file retirement protection, follower resource bounds, publication-overlap tradeoff, public terminal semantics, and all prior R1 error classifications. Delete the test-only gate and tests whose sole subject is the removed `StagedRunWindows` mechanism; retain or replace their behavioral coverage. Do not add a public mode, independent Scribe lifetime, new persisted state, planner, retry, scheduler, general abstraction, or alternate Drift path. Do not change ACK/publication order.

## Acceptance and focused proof

| Finding | Required observation |
|---|---|
| FIND-TASK-001-3 | The custom `StagedRunWindows`/oneshot/`spawn_blocking` handoff is gone. A large staged run with a zero-match predicate emits no empty batch, does not predecode the full run, and stops future reads on query cancellation. Decode and predicate errors still fail the query. |
| FIND-TASK-001-7 | The Parquet reader and staged lease follow the Scribe stream; cancel/drop leaves no Wyrd-owned decoder running after admission release, protects any pending file operation until it exits, and releases admission and source references. A replacement follower cannot enter while charged work remains. |
| Bounded async reading | A maximum-width admitted staged row group and a many-run fragment remain within the existing Scribe follower grant and do not scale resident data with total fragment size; another Tokio task progresses during the read. The >30-second and early-`LIMIT` journeys still pass. |
| FIND-TASK-001-5 | Both cited imports reside in their owning module import blocks; `mise run fmt` and `mise run lints` pass. |
| FIND-TASK-001-6 | The new discovery fields and signatures use imported bare types; source inspection and format/lints pass. |
| FIND-TASK-001-8 | The trait's `# Errors` text matches its `TailReadError` return and retained failure classes; source inspection and format/lints pass. |

Run exact `mise exec -- cargo nextest run --locked -p <package> --lib -E 'test(=<exact-name>)'` commands for every specifically named new or changed Rust test, verifying names with nextest first; use the repository-managed Postgres wrapper if a journey needs it. Run the narrow owning Bifrost/Scribe/Oracle journeys and `mise run verify:bifrost`; run `mise run fmt`, `mise run lints`, and relevant boundary checks. Because this correction crosses the Scribe producer and server admission owner, run `mise run gate` sequentially as in R1 and record results. Do not weaken a failing check. The real-server Drift Failed-to-no-verdict and staged corruption/schema journeys remain recorded verification limits rather than new work in this remediation.
