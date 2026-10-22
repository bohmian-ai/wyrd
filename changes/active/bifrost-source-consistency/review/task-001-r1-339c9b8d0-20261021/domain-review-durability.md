# Durability and persistent-data review — TASK-001 R1

**Overall: FAIL.** FIND-TASK-001-1 remains partially unclosed: the new shared Arrow charge does not reach the existing persistence payload retaining the same buffers. Publication ordering/recovery and retained-file expiry changes otherwise preserve the reviewed durability boundary. This is an existing finding's shared cause, not a request for a new stable ID.

## Subject, authority and coverage

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-source-consistency`. Base: `1c5edf72958af8f47c63eb8ce39eb316672e170a`. Cumulative candidate: `339c9b8d0484421b5fd67b71194eed8fcc60cadb`. R1 range starts at `0d3223696f091fb16bb2e0ec36040a249d39b61f`. HEAD remained the candidate; only the current review directory was observed untracked. No current discovery conclusions were consulted.

Authority reviewed: AGENTS and agent rules; Wyrd design/doctrine; Bifrost design source-selection, admission/memory and recovery boundaries; reference router and applicable OLAP-serving and analytical-operations/reliability guidance; spec-driven-development and maintainer-style references; approved source-consistency specification revision 2, original TASK-001, prior verdict/findings and R1 remediation/evidence. CodeGraph was used before ordinary source navigation. Full changed bodies and surrounding owners were inspected, not just implementation summaries.

| Boundary | Source coverage | Assessment |
| --- | --- | --- |
| Publication transaction | Migration `20261021000000_bifrost_publication_revision.sql`; `file_list_writer::{insert_artifact_set_fenced,artifact_set_outcome,node_publication_revisions}`; SQL cut projection and publication test | AFTER INSERT counter updates share the file-list transaction. Per-node/table counter-row locks serialize fresh publication commits. Rollback undoes increments; conflict-no-op replay fires no row trigger. Outcome reads revision before commit; replay may safely report a newer committed revision. |
| Cut versus disappearing live rows | `ScribeHotSourceRegistry::{advance_all_atomic,begin_publication,abandon_publication,restore_publication_revisions,capture_staged,release}`; `LiveCapture::{take,read}`; Oracle acquisition, `list_live_routes`, cache eviction | Registry authority and revision use the same state-lock order. Unsettled publications refuse capture. Published transitions raise revision before removal, and revision survives final key removal. Empty captures report it. Older cuts repin before execution. |
| Commit uncertainty and recovery | Full ClaimPublisher publication, marking, advancement, missed-advance, finish-committed and terminal-retirement bodies; staging restore/resume; `ScribeImpl::replay_wal_async`; persistence restoration delegation | Known refusal clears marks; unresolved commit outcomes retain them. Identical fenced replay remains settlement. Startup restores staged state, reconciles manifests, loads node revisions and resumes claims before readiness. Missed in-process advancement loads revisions before terminal removal. |
| File safety and progress | Normal/recovered claim cleanup; registry lease acquisition/drain/drop; capture/read ownership; follower `live_leaf` and staged scan owner | Opened readers keep leases after capture removal. Both cleanup paths drain leases before deleting files. Expiry removes abandoned registry ownership without weakening opened-reader protection. WAL/cohort progress does not wait for capture completion. |
| Arrow ownership | Full `RetainedArrowCharge::hold`, `ScribeOwnership::detach_immutable`, bucket/frozen/read owners, `commit_retirement`, ordinary/replay retirement and rollback; full MemtableSource; persistence payload construction/snapshot/worker delivery | Captures and projected/pruned/fetch-rewritten/opened memory sources share one lease. Existing persistence payload loses that owner while cloning the same Arrow buffers: DUR-R1-001. |
| Deadline cleanup | Capture table insert/get/remove, `retain`, `open_capture`, release; server discovery admission/release; retained-route Drop | Insertion precedes timer activation; elapsed installation removes/refuses its entry. Open checks original monotonic deadline before projection. Peer release joins use the original absolute deadline. |
| Proof | New charge/expiry/stalled-release tests; freeze-clone correction; original SQL revision replay/rollback test; transition journeys, staged-file tests and recorded owner/static lanes | Proof supports capture ownership and expiry closure. Charge proof omits the persistence payload, while the corrected replay test leaves that payload alive when expecting budget return. |

Existing owners remain appropriate: file-list lineage/fencing, ClaimPublisher and ScribeStageMover recovery, hot-source authority/leases, and ScribeOwnership/ScribeMemoryLease accounting. The shared charge slot is justified by overlapping lifetimes. No second governor, per-query charge, replacement cleanup workflow or delayed durable publication is needed.

## Prior finding closure

| Prior finding | Source-backed disposition |
| --- | --- |
| FIND-TASK-001-1 | **Partially closed, still material.** Bucket/frozen → LiveCapture → CapturedRead → LiveTailBatches → MemtableSource and rewrites/streams retain one detached lease. Persistence payload drops the ownership edge; see DUR-R1-001. |
| FIND-TASK-001-2 / prior DUR-001 | **Closed.** `retain` inserts first, removes/refuses an elapsed entry, then spawns removal. Expiry cannot finish before a later installation. Stored deadline and `open_capture` independently reject elapsed authority before projection. New elapsed-installation proof checks both table and staged lease become empty. |
| FIND-TASK-001-3 | **Closed within this domain.** Failed-acquisition cleanup and last-route Drop pass original deadline to the shared server release owner; its peer join uses `timeout_at`. Real mTLS pending-release proof exercises that owner. Expiry remains fallback. |
| FIND-TASK-001-4 | Cited methods now describe errors and relevant lifecycle. No durability defect remains at those sites; general documentation gating belongs to repository review. |
| FIND-TASK-001-5 | Current guidance distinguishes listing degradation from fatal accepted-capture loss. No durability waiver was introduced. |

## Material proposed finding

### DUR-R1-001 — Persistence payload bypasses the shared generation charge

**Classification:** INCORRECT; incomplete closure of **FIND-TASK-001-1**. **Obligation:** spec INV-006 / AC-002, Bifrost shared-memory ownership invariant, and R1's selected correction that Arrow stays charged until its final physical holder drops while preserving persistence/replay ownership.

**Locations:** `scribe/persistence.rs:529` (`ImmutableGeneration::from_frozen`, rows clone within that body), `:568` (`frozen_snapshot`, new unrelated empty charge at 581); `scribe/shards.rs:2381` (retirement), `:2702` and `:2793` (replay retirement); test correction `scribe/shards.rs:5658`–5665. Paths are under `crates/vala/vala-bifrost-redux/src/`.

**Producer-to-consumer evidence.** Both `prepare_and_queue_generation` and `prepare_replay_state` construct ImmutableGeneration via `from_frozen`. It clones original RecordBatch buffers into `rows`, but the struct has no shared charge field. `frozen_snapshot` clones those rows again with `charge: Arc::default()`. Worker `process_job` retains an Arc to the payload; `deliver_completion` retains `job.generation` while awaiting the shard visibility acknowledgment. These are production physical holders.

On replay completion, `handle_persistence_completion` calls `complete_owned_replay_generation` / `complete_replay_generation` (or the single-generation sibling) while `front.generation` is alive and the worker still holds its payload awaiting acknowledgment. Retirement detaches the ledger into the memtable charge and removes the frozen entry. With no remaining capture/frozen holder, that charge drops while the pending front and worker still retain the buffers. Replay may advance to the next generation before these holders drop. Ordinary age retirement can also run after completion acknowledgment is sent but before the worker observes it and drops its payload. The root can reuse memory still resident in persistence.

**Why the corrected test misses it.** `replay_identity_failures_settle_response_and_release_ownership` now drops `current_frozen` and `next_frozen`, but first constructs `current` and `next` through `owner_generation` / `ImmutableGeneration::from_frozen`. `current` remains alive across helper calls and baseline assertions with its uncharged original buffers. Dropping only frozen clones clears the assertion by omitting another genuine holder. The new charge test constructs retirement state directly without constructing the persistence payload, so it cannot expose this missing edge.

**Consequence:** the shared root reports reclaimed bytes while persistence/replay retains their allocations, allowing admitted work to reuse that capacity. This is the resident-memory guarantee FIND-1 sought to restore; no row-loss claim or new durable-publication requirement is asserted.

**Smallest correction:** carry the existing generation charge through ImmutableGeneration construction and snapshot reconstruction so payload and CPU-stage clones share it. Reuse the existing detached lease; add no reservations, pool or publication/WAL wait. Preserve pre-adoption rollback and failed-stage behavior. Extend the existing charge proof to retain an actual ImmutableGeneration through retirement, remove capture/read/frozen holders, assert root bytes remain charged, then drop the last payload and assert exact single return. Make the replay-identity test drop genuinely finished row holders at its intended settlement boundary, or account for legitimately retained bytes, preserving all response/error/identity assertions. This completes the same owner graph rather than changing architecture.

## Verification limits

Static source/evidence review only. No Cargo command, test run, benchmark, source edit, commit or subagent was used. Recorded RED/GREEN and scoped lane results were compared with source scenarios, not independently rerun. They do not prove persistence charge ownership because that edge is absent.

Original REQ-006 / AC-006 qualification still records failed strict latency targets; no R1 capacity qualification is supplied. That remains original-task acceptance evidence independently of this durability finding and cannot be waived by baseline comparability.

Skipped: executing tests and capacity workloads; broader performance/standards acceptance belongs to its owning reviews.
Risk: persistence-held Arrow can remain resident after the shared root reports its bytes reclaimed.
