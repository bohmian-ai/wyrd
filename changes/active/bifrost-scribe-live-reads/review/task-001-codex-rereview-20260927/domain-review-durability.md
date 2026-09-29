# Independent domain review: published and live durability

**Subject:** `d1ec13200d332745af2fed8069a21d5b5c39cb47..f1f1d5ebd264e8f9ac861ec79c6340da44a7a1a8` (TASK-001, approved spec revision 3). Source was not edited. **Result: FAIL.**

## Boundary and authority coverage

| Boundary | Authority | Source and verification inspected | Result |
|---|---|---|---|
| WAL ACK, staging, publication order | `AGENTS.md` §§9, 11; `architecture/bifrost-design.md` Scribe lifecycle; `architecture/references/domain/olap-serving.md`, `iceberg.md`, `analytical-operations-reliability.md`; spec INV-001, REQ-006 | Cumulative diff; `scribe/shards.rs`, `hot_source.rs`, `claim_publication.rs`, `tail_rpc.rs`; persistence tests | PASS. This change does not move ACK timing. Claim commit precedes authority advance, lease drain precedes staged retirement. |
| Pinned published cut and live handoff | Spec REQ-001, REQ-002, REQ-006; Bifrost reader authority | `oracle/planner.rs::protect_and_materialize`, `oracle/mod.rs::prepare_query_attempt`, `build_physical_root`, `discover_live_routes`; `oracle/live.rs`; distributed published/live journey | PASS. Reader protection and cut materialization precede Scribe discovery and opening. Publication overlap can omit or duplicate rows as expressly approved. |
| Memtable/staged authority and retained readers | Spec REQ-003, REQ-005, INV-005; analytical reliability retained-reader safety | `FetchLiveTailService::open_live_batches`, `ScribeHotSourceRegistry::staged_sources`/`drain_leases`, `ClaimPublisher::publish`/`retire_members`, `StagedRunWindows`; staged lease tests | PASS for file safety. An in-flight blocking read retains its staged lease after stream cancellation, and publication waits before deletion. |
| Bounded staged reads and cancellation | Spec REQ-005, AC-004; `AGENTS.md` §6; Bifrost live-tail cancellation | `StagedTailReader::run_batches`, `StagedRunWindows::next`, `LiveTailBatches::into_stream`; cancellation unit test | **FAIL: DUR-R1-001.** The filtered iterator can decode an entire run in one blocking step. |
| Failure terminal on missing published data or invalid live listing | Spec REQ-002, REQ-004 | `oracle/mod.rs::discover_live_routes`; `scribe/tail_rpc.rs::tonic_error`; `distributed::live_query_terminal_failure_matrix` | PASS. Prior DUR-001 is closed: ticket refusal fails; unavailable listing degrades; deleted published file fails. |

## Material proposed finding

### DUR-R1-001 — Empty projected windows defeat one-window-per-pull cancellation

- **Classification:** INCORRECT; partial closure of prior FIND-3.
- **Violated obligation:** REQ-005 and AC-004 require bounded, backpressured live production and prompt cancellation/resource release. The remediation also claims one staged decode window per consumer pull.
- **Location and evidence:** [`staged_tail.rs`](../../../../../crates/vala/vala-bifrost-redux/src/scribe/staged_tail.rs) `StagedTailReader::run_batches`, lines 88–113, builds an iterator that filters out every successfully decoded zero-row batch. [`staged_tail.rs`](../../../../../crates/vala/vala-bifrost-redux/src/scribe/staged_tail.rs) `StagedRunWindows::next`, lines 174–179, calls that iterator's `next()` inside one `spawn_blocking` step. Rust iterator `filter::next()` keeps advancing the inner Parquet reader until a nonempty batch or EOF. A valid signed predicate that matches no rows can therefore decode every window in a large staged run in one blocking task. [`tail_rpc.rs`](../../../../../crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs) `LiveTailBatches::into_stream`, lines 671–685, cannot observe cancellation until that task reports. The existing cancellation test uses rows that pass its predicate, so it never exercises the skip path.
- **Observable consequence:** Cancelling a query while an empty filtered window is being decoded does not stop after that window. Scribe may keep decoding the rest of the staged run, retain its staged lease, and delay publication cleanup. Other async tasks can still run, but the one cancelled query continues consuming blocking-pool and file resources beyond its promised single in-flight window.
- **Testable correction:** Keep `run_batches` as an iterator of individual decoded windows, including zero-row results, and skip zero-row batches in `LiveTailBatches::into_stream` after each awaited blocking step. The next step must be scheduled only by the still-live stream. A focused test with a multi-window staged run and a predicate matching no rows should cancel while the first window is held, release it, and prove only that window was decoded and the lease then drops. Preserve the current published cut and staged lease authority rules.

## Verification limits

The task records `verify:bifrost` 9/9 and full gate 48/48. I did not rerun lanes in this immutable review. The staged cancellation unit test proves scheduling and lease behavior when the decoded window contains rows; it does not cover iterator filtering of empty windows. Schema and corrupt-file failures remain unit-only as recorded in the remediation evidence; this review found no independent durability defect in that limit. Admission release before the in-flight blocking step finishes is a separate resource-ownership question for the concurrency review; the finding above stands even if admission remains held.
