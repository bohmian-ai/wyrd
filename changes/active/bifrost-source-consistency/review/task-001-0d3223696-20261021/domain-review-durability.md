# Independent durability and persistent-data review

Result: **FAIL** — one proposed material finding, `DUR-001`.

## Immutable subject and authority

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-source-consistency`.
- Base: `1c5edf72958af8f47c63eb8ce39eb316672e170a`.
- Candidate: `0d3223696f091fb16bb2e0ec36040a249d39b61f`; HEAD remained this commit when checked.
- Approved authority: `changes/active/bifrost-source-consistency/spec.md`, revision 2; original task `tasks/TASK-001-source-consistency.md` beneath that change.
- Repository authority: `AGENTS.md`, `architecture/agent-rules.md`, reference router, spec-driven-development and maintainer-style references; Bifrost publication/read/recovery/retention authority, Wyrd ownership authority, and domain references for Iceberg, OLAP serving and analytical operations/reliability.

I inspected the cumulative base-to-candidate changed surface, using CodeGraph first for publication, revision and recovery ownership, and then the cumulative source diff and actual surrounding producers/consumers. This domain report concentrates on durability and persistent-state consequences. It does not adopt another discovery reviewer's conclusions.

## Boundary and source coverage

| Boundary | Inspected producers, writers and consumers | Assessment |
|---|---|---|
| Publication revision transaction | New `20261021000000_bifrost_publication_revision.sql`; `insert_artifact_set_fenced`; `artifact_set_outcome`; existing file-list INSERT/UPDATE/DELETE sites | AFTER INSERT advances per tenant/table/node inside the same transaction. Counter-row update serializes same-node table revisions through commit. Exact `ON CONFLICT DO NOTHING` replay does not fire the row trigger; rollback removes the revision update. Publication outcome reads its watermark before commit and moves local authority only after a successful result. |
| SQL cut coherence | `vala.oracle_read_table_cut`; `OracleTableCuts::read`, grouping; `BifrostCatalog::materialize_cut` | Revisions accompany catalog pointer and unresolved hot rows in the statement snapshot. Grouping rejects inconsistent repeated revisions. Materialization preserves existing `HotFileCut::reconcile`, snapshot pointer, exact path overlap and Forge operation identity checks. |
| In-memory authority handoff | `ClaimPublisher::{publish,mark_publication,advance_published}`; `ScribeHotSourceRegistry::{begin_publication,advance_all_atomic,capture_staged,abandon_publication,release}` | The mark precedes the COMMIT-capable call. Unknown outcome keeps selection blocked. Confirmed commit raises table revision in the same registry critical section as removing live authority. Capture reads that revision while deciding memory/staged/published ownership. |
| Local recovery | `ScribeImpl::replay_wal_async`; persistence restore/reconcile/resume sequence; `ScribeStagingRuntime::{restore,restore_authorities}`; `ClaimPublisher::{finish_committed,missed_published_advance,recover_terminal_members}` | Startup restores source authority, marks Publishing members uncertain, replays durable publication manifests, restores node revisions and resumes claims before readiness. Terminal durable member facts are preserved across interrupted cleanup. Existing WAL replay filters the current node and earlier writer epochs, so old-epoch rows retain their producer identity without introducing a new writer-node watermark mapping. |
| Exact replay and uncertain outcomes | `ScribePublicationReconciler::{publish,publication_state,node_publication_revisions}`; `ScribeStageMover::{recover_publications,recover_publication}`; staged publication manifest creation/cleanup | Identical complete artifact identities remain the replay mechanism; contradictory/partial identities fail closed. Reconciliation does not replace objects already known published. Missed local advancement is reconciled before dropping surviving non-Published authorities. |
| Retained files and retirement | `LiveCapture::{take,read}`; `FetchLiveTailService::{capture_active_streams,capture,retain,release_capture,open_capture}`; `CaptureTable`; `LiveTailBatches::into_parts`; follower staged scans; `hot_parquet_scan`; registry lease release/drain; claim normal and recovered retirement | Shared staged leases reach the physical Parquet scan and outlive release of the capture table entry when a read is already open. Cleanup waits before deleting local files. However expiry can run before insertion, leaving an abandoned lease without a future expiry: DUR-001. |
| Oracle validation/cache | `list_live_routes`, `discover_live_routes`, `build_physical_root`, retry loop, `LiveCaptures::drop`, server `RegistryTailStreamDiscovery` | Empty captures still carry node revisions; cuts older than any reported revision are evicted before execution. Entire selections are dropped on retry. Explicit release is best effort, with expiry the essential fallback for lost responses and cancellation. |
| Forge and retained analytical objects | Existing `HotFileCut::reconcile`, catalog pin/materialization; `HotFileCatalog::settle_promoted`; Forge expired-cleanup settlement and file-list deletion | New watermarks remain separate from Forge's maintenance authority and are not reset by hot-row settlement/deletion. Existing snapshot retention and exact hot/Iceberg reconciliation remain in use; no new listing-based deletion or replacement protection mechanism was introduced. |

Existing owners are extended rather than replaced in the reviewed durable paths: `file_list` remains the publication lineage/fence, `ClaimPublisher` remains claim lifecycle owner, `ScribeStageMover` remains manifest replay owner, `ScribeHotSourceRegistry` remains local authority/lease owner, and `HotFileCut` remains published-source reconciliation owner. The new per-node revision is required by the approved spec; cache notifications alone cannot cover the handoff race. No second durable evidence subsystem is present in these changes.

## Proposed finding

### DUR-001 — Expiry can complete before a capture is inserted, retaining staged files indefinitely

- Classification: **INCORRECT**.
- Violated obligation: spec **INV-006** (capture lifetime bounded by release/cancellation/deadline; cleanup cannot delete live sources), **AC-002** cleanup, and REQ-003's ownership/recovery requirement. The implementation must not leave permanently retained sources after abandoned acquisition.
- Changed location: `crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs:814`–829, `FetchLiveTailService::retain`.

**Evidence and reachable producer-to-consumer path.** `capture_active_streams` checks the deadline's remaining duration before `self.capture(...).await`, then synchronously calculates partitions and calls `retain`. `capture` can spend time acquiring all memtable read locks and the registry lock; successful capture does not recheck the deadline. Consequently the deadline can already be due when `retain` runs, even for a request that had a valid future deadline on arrival. Scheduling delay can also consume the remaining time.

`retain` first spawns a task whose entire action is `sleep_until(deadline); captures.remove(capture_id, |_| true)`, and only afterwards inserts the `Arc<RetainedCapture>` into `CaptureTable`. On the production multithread runtime the spawned task can run on another worker immediately. If it observes a due deadline, it removes the still-absent ID and terminates. The original worker then inserts the capture with the completed task's abort handle. There is no second timer, durable sweep or other expiry owner. A caller that timed out, cancelled acquisition or lost the listing response has no capture ID to release. Even an ordinary Oracle release is explicitly best effort.

The retained value owns `LiveCapture`, which owns `Arc<StagedSourceLease>`. The lease decrements the registry only when its last owner drops. `ClaimPublisher::retire_members` and recovered `retire_terminal_member` call `await_lease_drain`, which waits until this count reaches zero before deleting a staged member. Therefore the raced capture retains Arrow references and/or staged leases for the process lifetime and can strand claim cleanup and consume staged capacity indefinitely. This is an availability/resource ownership consequence, not a request to weaken retained-file protection.

**Sibling paths traced.** Both in-process `RegistryTailStreamDiscovery::list_local` and the private gRPC listing reach the same `capture_active_streams`/`retain` producer. Explicit release and expiry both use `CaptureTable::remove`; follower opening uses `get` and clones shared read ownership. Normal and startup/retry member cleanup both reach registry lease drainage. The defect belongs at the single retention producer, not in each consumer, Forge deletion or lease drain.

**Smallest safe correction boundary.** Make insertion and activation of the expiry callback linearize on the existing `CaptureTable` shard so expiry cannot complete its removal before insertion. Holding the selected shard's existing lock through timer scheduling and insertion gives the removal task the required ordering without a second reaper or downstream guards. Preserve dropping removed captures outside the lock, authenticated release, open-reader shared ownership, and the exact query deadline. A deadline check alone does not close a scheduling race between that check and insertion.

**Focused closure proof.** Add one deterministic local test at this owner that makes the deadline due while retention is being installed and permits the expiry callback to run at the earliest scheduling opportunity; after installation/expiry, the capture table and staged lease count must be zero without explicit release, and staged-member lease drainage must complete. Exercise the insertion/expiry ordering, rather than relying on repeated short sleeps or synthetic host load. Retain the existing open-reader lease test to show that an already-open scan still protects its files. No new integration harness, dependency, broad journey suite or altered retention contract is needed.

## Verification evidence and limits

The packet records SQL rollback/replay/tenant tests, atomic authority and Publishing refusal tests, rollover tests, the retained-file test, abandoned-acquisition journey, and scoped integration/static lanes. Source changes since capacity commit `05186c1e4` to the candidate are confined to the task evidence file, consistent with its claim of unchanged runtime code. I did not rerun tests, capacity workloads or Cargo builds.

`an_unreleased_capture_expires_at_its_deadline` takes a capture, calls `retain` with one second still remaining on a paused current-thread runtime, successfully opens it, and only then advances time. It verifies removal after insertion; it cannot cover the reverse scheduling order reported here. `an_open_live_read_keeps_staged_runs_across_publication` does establish normal shared-lease preservation/release. Neither green test establishes the missing insertion/expiry ordering.

No other material durability finding is proposed. Qualification-limit decisions are outside this domain's finding, and no pre-existing debt or optional hardening is included.

Skipped: executing verification or reproducing scheduling with synthetic load; this is a static source-backed audit.
Risk: an abandoned capture can outlive its deadline and indefinitely block safe staged-file retirement until process restart.
